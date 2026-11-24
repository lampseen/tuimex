//! `Remote` over IMAP, through the pinned `imap` crate (ADR-023). The TLS
//! connection is made here and handed to the crate (ADR-029), which is
//! what lets a pinned certificate be checked (ADR-012) and a blocked call
//! be ended by closing its socket (ADR-006).

use std::cell::Cell;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use imap::extensions::idle::{SetReadTimeout, WaitOutcome};
use imap::types::Flag;
use imap::{Client, Session};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, SignatureScheme, StreamOwned};
use tuimex_core::{
    Capabilities, Deadline, Fetched, FlagChange, Flags, Interrupt, MoveOutcome, Remote, RemoteError, RemoteFolder,
    RemoteWrite, Selected, Timestamp, UidRange, Woken,
};
use zeroize::Zeroizing;

use crate::oauth::Tokens;
use crate::{Connect, Error, Keyring, Letter, Secret, pin, resolve, smtp};

/// How long a server may take to accept a connection or answer a command
/// before the connection counts as lost.
pub(crate) const PATIENCE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct Server {
    pub host: String,
    pub port: u16,
    /// Upgrade a plain connection with STARTTLS; otherwise TLS from the
    /// first byte. There is no third way (ADR-012).
    pub starttls: bool,
    pub user: String,
}

struct Tls(StreamOwned<ClientConnection, TcpStream>);

impl Read for Tls {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Tls {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl SetReadTimeout for Tls {
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> imap::Result<()> {
        self.0.sock.set_read_timeout(timeout).map_err(imap::Error::Io)
    }
}

/// Accepts the one certificate the user pinned and no other, whoever
/// signed it (ADR-012). The handshake's signature is still checked, so the
/// server must hold that certificate's key.
#[derive(Debug)]
struct Pinned {
    account: String,
    certificate: Vec<u8>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        presented: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match pin::check(&self.account, &self.certificate, presented.as_ref()) {
            Ok(()) => Ok(ServerCertVerified::assertion()),
            Err(changed) => Err(rustls::Error::General(changed.to_string())),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// What a server presented, and whether this system's roots vouch for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub certificate: Vec<u8>,
    pub trusted: bool,
}

/// Lets a handshake finish whatever the certificate, and keeps the
/// certificate and the verdict on it. Only `certificate` uses it, on a
/// connection that is closed without anything being sent over it: it is
/// how `tuimex setup` learns what to show before the user accepts or
/// refuses a pin (ADR-012).
#[derive(Debug)]
struct Capture {
    roots: Option<Arc<WebPkiServerVerifier>>,
    seen: Mutex<Option<Seen>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Capture {
    fn verify_server_cert(
        &self,
        presented: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let vouched = |roots: &Arc<WebPkiServerVerifier>| {
            roots.verify_server_cert(presented, intermediates, name, ocsp, now).is_ok()
        };
        let seen = Seen { certificate: presented.to_vec(), trusted: self.roots.as_ref().is_some_and(vouched) };
        // A poisoned lock still holds a place to put the answer.
        *self.seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(seen);
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

fn lost(error: impl std::fmt::Display) -> RemoteError {
    RemoteError::Connection(error.to_string())
}

fn system_roots() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    roots
}

/// Which protocol's words ask a plain connection to start TLS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Imap,
    Smtp,
}

/// The certificate `server` presents, without signing in or sending it
/// anything past the handshake (and, before it, the request to start TLS).
pub fn certificate(server: &Server, protocol: Protocol) -> Result<Seen, RemoteError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = system_roots();
    // With no roots, or none that parse, nothing is vouched for.
    let roots = (!roots.is_empty())
        .then(|| WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone()).build().ok())
        .flatten();
    let capture = Arc::new(Capture { roots, seen: Mutex::new(None), provider: provider.clone() });
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(lost)?
        .dangerous()
        .with_custom_certificate_verifier(capture.clone())
        .with_no_client_auth();
    drop(secure(server, config, protocol)?);
    let seen = capture.seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    seen.ok_or_else(|| lost("the server completed a handshake without presenting a certificate"))
}

/// What the certificate is checked against: the account's pinned one, or
/// the system's roots when it has none.
fn tls_config(account: &str, pinned: Option<Vec<u8>>) -> Result<ClientConfig, RemoteError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder =
        ClientConfig::builder_with_provider(provider.clone()).with_safe_default_protocol_versions().map_err(lost)?;
    let Some(certificate) = pinned else {
        let roots = system_roots();
        if roots.is_empty() {
            return Err(lost("this system has no trusted certificate roots to check a server against"));
        }
        return Ok(builder.with_root_certificates(roots).with_no_client_auth());
    };
    let verifier = Pinned { account: account.to_string(), certificate, provider };
    Ok(builder.dangerous().with_custom_certificate_verifier(Arc::new(verifier)).with_no_client_auth())
}

/// Reads SMTP's reply lines up to the last of a reply, which is the one
/// with a space after its code, and says whether the code was `code`.
fn smtp_reply(plain: &mut impl BufRead, code: &str) -> Result<bool, RemoteError> {
    let mut line = String::new();
    loop {
        line.clear();
        if plain.read_line(&mut line).map_err(lost)? == 0 {
            return Err(lost("the server hung up when asked to start TLS"));
        }
        if line.as_bytes().get(3) != Some(&b'-') {
            return Ok(line.starts_with(code));
        }
    }
}

/// A TLS stream to the server, checked as `config` says, and a second
/// handle on its socket. Nothing but the request to start TLS is sent
/// before the certificate has been checked, and a server that will not
/// upgrade ends the attempt.
fn secure(
    server: &Server,
    config: ClientConfig,
    protocol: Protocol,
) -> Result<(StreamOwned<ClientConnection, TcpStream>, TcpStream), RemoteError> {
    // A name may have several addresses, not all of them listening: the
    // first that answers is used, and the last refusal is the error.
    let addresses = (server.host.as_str(), server.port).to_socket_addrs().map_err(lost)?;
    let mut reached = Err(lost(format!("{} has no address", server.host)));
    for address in addresses {
        reached = TcpStream::connect_timeout(&address, PATIENCE).map_err(lost);
        if reached.is_ok() {
            break;
        }
    }
    let mut tcp = reached?;
    tcp.set_read_timeout(Some(PATIENCE)).map_err(lost)?;
    if server.starttls {
        // Read through a handle that is dropped with whatever it had
        // buffered, so nothing a third party slipped in after the
        // server's answer is ever taken as having come over TLS.
        let mut plain = BufReader::new(tcp.try_clone().map_err(lost)?);
        let mut line = String::new();
        let agreed = match protocol {
            Protocol::Imap => {
                plain.read_line(&mut line).map_err(lost)?;
                tcp.write_all(b"tuimex STARTTLS\r\n").map_err(lost)?;
                while !line.starts_with("tuimex ") {
                    line.clear();
                    if plain.read_line(&mut line).map_err(lost)? == 0 {
                        return Err(lost("the server hung up when asked to start TLS"));
                    }
                }
                line.starts_with("tuimex OK")
            }
            Protocol::Smtp => {
                smtp_reply(&mut plain, "220")?;
                tcp.write_all(b"EHLO localhost\r\n").map_err(lost)?;
                smtp_reply(&mut plain, "250")?;
                tcp.write_all(b"STARTTLS\r\n").map_err(lost)?;
                smtp_reply(&mut plain, "220")?
            }
        };
        if !agreed {
            return Err(lost(format!("the server would not start TLS, so nothing was sent to it: {}", line.trim())));
        }
    }
    let name = ServerName::try_from(server.host.clone()).map_err(lost)?;
    let mut tls = ClientConnection::new(Arc::new(config), name).map_err(lost)?;
    while tls.is_handshaking() {
        // rustls prefixes a verifier's own words, which here are the
        // whole message a person needs (ADR-012).
        tls.complete_io(&mut tcp).map_err(|e| lost(e.to_string().trim_start_matches("unexpected error: ")))?;
    }
    let socket = tcp.try_clone().map_err(lost)?;
    Ok((StreamOwned::new(tls, tcp), socket))
}

fn open(server: &Server, config: ClientConfig) -> Result<(Client<Tls>, TcpStream), RemoteError> {
    let (stream, socket) = secure(server, config, Protocol::Imap)?;
    let mut client = Client::new(Tls(stream));
    if !server.starttls {
        client.read_greeting().map_err(lost)?;
    }
    Ok((client, socket))
}

fn failed(error: imap::Error) -> RemoteError {
    match error {
        imap::Error::No(_) | imap::Error::Bad(_) => RemoteError::Refused(error.to_string()),
        // The crate reads the server's refusal of an `APPEND` and keeps
        // none of its words.
        imap::Error::Append => RemoteError::Refused("the server would not take the message".to_string()),
        imap::Error::Io(_) | imap::Error::ConnectionLost | imap::Error::Bye(_) => lost(error),
        other => RemoteError::Protocol(other.to_string()),
    }
}

/// What an account signs in with: its password, or for Gmail an access
/// token (ADR-019). The same one opens its SMTP server.
#[derive(Debug, Clone)]
pub enum Credential {
    Password(Secret),
    Bearer(Secret),
}

/// The XOAUTH2 exchange. The token is sent once, with the command or in
/// answer to the first challenge; a server that refuses it says why in a
/// further challenge, which is answered with nothing so that it goes on
/// to its `NO`.
struct Xoauth2 {
    said: Zeroizing<String>,
    asked: Cell<bool>,
}

impl imap::Authenticator for Xoauth2 {
    type Response = Zeroizing<String>;

    fn process(&self, _challenge: &[u8]) -> Zeroizing<String> {
        if self.asked.replace(true) { Zeroizing::new(String::new()) } else { self.said.clone() }
    }
}

pub struct Imap {
    session: Session<Tls>,
    socket: Arc<TcpStream>,
    capabilities: Capabilities,
}

impl Imap {
    pub fn connect(
        server: &Server,
        account: &str,
        credential: &Credential,
        pinned: Option<Vec<u8>>,
    ) -> Result<Imap, RemoteError> {
        let (mut client, socket) = open(server, tls_config(account, pinned)?)?;
        let signed_in = match credential {
            Credential::Password(password) => client.login(&server.user, password.expose()),
            Credential::Bearer(token) => {
                let said = Zeroizing::new(format!("user={}\x01auth=Bearer {}\x01\x01", server.user, token.expose()));
                // With SASL-IR the token rides on the command itself, which
                // is the only form some servers take; the crate has no
                // name for that, so it goes in where the mechanism's does.
                if client.capabilities().is_ok_and(|offered| offered.has_str("SASL-IR")) {
                    let command = Zeroizing::new(format!("XOAUTH2 {}", STANDARD.encode(said.as_bytes())));
                    client.authenticate(command.as_str(), &Xoauth2 { said, asked: Cell::new(true) })
                } else {
                    client.authenticate("XOAUTH2", &Xoauth2 { said, asked: Cell::new(false) })
                }
            }
        };
        // The error is reduced to its kind: its text answers a command
        // that held the password (ADR-010).
        let mut session = signed_in.map_err(|(error, _)| match error {
            imap::Error::Io(e) => lost(e),
            imap::Error::No(_) | imap::Error::Bad(_) => RemoteError::Auth,
            _ => RemoteError::Protocol("the sign-in exchange could not be completed".to_string()),
        })?;
        let offered = session.capabilities().map_err(failed)?;
        let mut capabilities = Capabilities {
            uidplus: offered.has_str("UIDPLUS"),
            r#move: offered.has_str("MOVE"),
            // Only as it is switched on below, which takes ENABLE.
            condstore: offered.has_str("CONDSTORE") && offered.has_str("ENABLE"),
            special_use: offered.has_str("SPECIAL-USE"),
            idle: offered.has_str("IDLE"),
            gmail: offered.has_str("X-GM-EXT-1"),
        };
        drop(offered);
        if capabilities.condstore {
            capabilities.condstore = session.run_command_and_check_ok("ENABLE CONDSTORE").is_ok();
        }
        Ok(Imap { session, socket: Arc::new(socket), capabilities })
    }

    fn fetch(&mut self, uids: &str, items: &str) -> Result<imap::types::Fetches, RemoteError> {
        self.session.uid_fetch(uids, items).map_err(failed)
    }
}

fn flags(fetch: &imap::types::Fetch<'_>) -> Flags {
    let has = |flag: Flag<'_>| fetch.flags().contains(&flag);
    Flags { seen: has(Flag::Seen), flagged: has(Flag::Flagged), answered: has(Flag::Answered), draft: has(Flag::Draft) }
}

impl Remote for Imap {
    fn folders(&mut self) -> Result<Vec<RemoteFolder>, RemoteError> {
        // The crate does not export the attribute's type, so it is read
        // by the name its `Debug` gives it: `NoSelect`, `Trash`, and for
        // one the crate has no name for, `Extension("\\NonExistent")`.
        let attribute = |a: &dyn std::fmt::Debug| {
            let word = format!("{a:?}");
            match word.as_str() {
                "NoSelect" => Some("\\Noselect".to_string()),
                "All" | "Archive" | "Drafts" | "Junk" | "Sent" | "Trash" => Some(format!("\\{word}")),
                _ => word.contains("NonExistent").then(|| "\\NonExistent".to_string()),
            }
        };
        let names = self.session.list(Some(""), Some("*")).map_err(failed)?;
        let folder = |n: &imap::types::Name<'_>| RemoteFolder {
            name: n.name().to_string(),
            delimiter: n.delimiter().and_then(|d| d.chars().next()),
            attributes: n.attributes().iter().filter_map(|a| attribute(a)).collect(),
        };
        Ok(names.iter().map(folder).collect())
    }

    fn select(&mut self, folder: &str) -> Result<Selected, RemoteError> {
        let mailbox = self.session.select(folder).map_err(failed)?;
        let missing = || RemoteError::Protocol(format!("{folder} was selected without a UIDVALIDITY"));
        Ok(Selected {
            uid_validity: mailbox.uid_validity.ok_or_else(missing)?,
            uid_next: mailbox.uid_next.unwrap_or(0),
            highest_modseq: mailbox.highest_mod_seq.filter(|_| self.capabilities.condstore),
        })
    }

    fn envelopes(&mut self, uids: UidRange) -> Result<Vec<Fetched>, RemoteError> {
        let last = uids.last.map_or("*".to_string(), |last| last.to_string());
        let range = format!("{}:{last}", uids.first);
        let fetches = self.fetch(&range, "(UID FLAGS RFC822.SIZE INTERNALDATE BODY.PEEK[HEADER])")?;
        let fetched = |f: &imap::types::Fetch<'_>| {
            Some(Fetched {
                uid: f.uid?,
                headers: f.header()?.to_vec(),
                flags: flags(f),
                size: f.size.unwrap_or(0),
                // A server that gives no arrival time gives a message
                // that is old: its body waits until it is opened.
                arrived: Timestamp(f.internal_date().map_or(0, |date| date.timestamp())),
            })
        };
        Ok(fetches.iter().filter_map(fetched).collect())
    }

    fn flags(&mut self, since: Option<u64>) -> Result<Vec<FlagChange>, RemoteError> {
        let items = since.map_or("(UID FLAGS)".to_string(), |modseq| format!("(UID FLAGS) (CHANGEDSINCE {modseq})"));
        let fetches = self.fetch("1:*", &items)?;
        Ok(fetches.iter().filter_map(|f| Some(FlagChange { uid: f.uid?, flags: flags(f) })).collect())
    }

    fn uids(&mut self) -> Result<Vec<u32>, RemoteError> {
        let mut uids: Vec<u32> = self.session.uid_search("UNDELETED").map_err(failed)?.into_iter().collect();
        uids.sort_unstable();
        Ok(uids)
    }

    fn body(&mut self, uid: u32) -> Result<Vec<u8>, RemoteError> {
        let fetches = self.fetch(&uid.to_string(), "(UID BODY.PEEK[])")?;
        let body = fetches.iter().find(|f| f.uid == Some(uid)).and_then(|f| f.body());
        body.map(<[u8]>::to_vec).ok_or_else(|| RemoteError::Refused(format!("the server has no message {uid}")))
    }

    fn idle(&mut self, until: Deadline) -> Result<Woken, RemoteError> {
        let wait = until.0.saturating_duration_since(Instant::now());
        let woken = self.session.idle().timeout(wait).keepalive(false).wait_while(|_| false).map_err(failed);
        // The crate leaves the socket with no read timeout after an idle.
        self.socket.set_read_timeout(Some(PATIENCE)).map_err(lost)?;
        Ok(if woken? == WaitOutcome::MailboxChanged { Woken::Changed } else { Woken::TimedOut })
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn interrupt(&self) -> Interrupt {
        let socket = self.socket.clone();
        Interrupt::new(move || {
            // A socket that is already closed is as closed as was wanted.
            let _ = socket.shutdown(Shutdown::Both);
        })
    }
}

/// The inside of an IMAP quoted string. The crate writes a folder's name
/// into some commands as it is given, and a `Message-ID` is whatever its
/// sender wrote: neither may end the string it is put in, or the line.
fn quotable(text: &str) -> Option<String> {
    let plain = text.chars().all(|c| c.is_ascii() && !c.is_ascii_control());
    plain.then(|| text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn folder_name(folder: &str) -> Result<String, RemoteError> {
    quotable(folder).ok_or_else(|| RemoteError::Refused(format!("{folder:?} cannot be named in a command")))
}

fn set(uids: &[u32]) -> String {
    uids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

fn flag_names(f: Flags) -> String {
    let named = [(f.seen, "\\Seen"), (f.flagged, "\\Flagged"), (f.answered, "\\Answered"), (f.draft, "\\Draft")];
    named.iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect::<Vec<_>>().join(" ")
}

impl RemoteWrite for Imap {
    fn store_flags(&mut self, uids: &[u32], set_: Flags, clear: Flags) -> Result<(), RemoteError> {
        for (sign, flags) in [("+", set_), ("-", clear)] {
            if flags != Flags::default() {
                let change = format!("{sign}FLAGS.SILENT ({})", flag_names(flags));
                self.session.uid_store(set(uids), change).map_err(failed)?;
            }
        }
        Ok(())
    }

    /// `UID MOVE` where the server has it. Otherwise a copy, `\Deleted` on
    /// the original, and a `UID EXPUNGE` naming it where the server has
    /// UIDPLUS; without that the original is left for the server to purge,
    /// because a bare `EXPUNGE` would take every message anyone had marked
    /// (ADR-014).
    fn move_to(&mut self, uids: &[u32], target: &str) -> Result<MoveOutcome, RemoteError> {
        let (uids, target) = (set(uids), folder_name(target)?);
        if self.capabilities.r#move {
            self.session.run_command_and_check_ok(format!("UID MOVE {uids} \"{target}\"")).map_err(failed)?;
            return Ok(MoveOutcome::Moved);
        }
        self.session.run_command_and_check_ok(format!("UID COPY {uids} \"{target}\"")).map_err(failed)?;
        self.session.uid_store(&uids, "+FLAGS.SILENT (\\Deleted)").map_err(failed)?;
        if !self.capabilities.uidplus {
            return Ok(MoveOutcome::CopiedAndFlagged);
        }
        self.session.uid_expunge(&uids).map_err(failed)?;
        Ok(MoveOutcome::Moved)
    }

    fn delete(&mut self, uids: &[u32]) -> Result<bool, RemoteError> {
        let uids = set(uids);
        self.session.uid_store(&uids, "+FLAGS.SILENT (\\Deleted)").map_err(failed)?;
        if self.capabilities.uidplus {
            self.session.uid_expunge(&uids).map_err(failed)?;
        }
        Ok(self.capabilities.uidplus)
    }

    fn append(&mut self, folder: &str, message: &[u8], flags: Flags) -> Result<(), RemoteError> {
        let folder = folder_name(folder)?;
        let mut command = self.session.append(&folder, message);
        for (on, flag) in [(flags.seen, Flag::Seen), (flags.flagged, Flag::Flagged), (flags.draft, Flag::Draft)] {
            if on {
                command.flag(flag);
            }
        }
        command.finish().map(drop).map_err(failed)
    }

    fn create(&mut self, folder: &str) -> Result<(), RemoteError> {
        match self.session.create(folder_name(folder)?).map_err(failed) {
            // Servers word "it is there already" each in their own way;
            // the list of folders says it in one.
            Err(RemoteError::Refused(why)) if !self.folders()?.iter().any(|f| f.name == folder) => {
                Err(RemoteError::Refused(why))
            }
            Err(RemoteError::Refused(_)) => Ok(()),
            other => other,
        }
    }

    fn find(&mut self, message_id: &str) -> Result<Vec<u32>, RemoteError> {
        // An id that cannot be written in a command is one the server
        // cannot be asked for.
        let Some(id) = quotable(message_id) else { return Ok(Vec::new()) };
        let query = format!("UNDELETED HEADER Message-ID \"<{id}>\"");
        let mut uids: Vec<u32> = self.session.uid_search(query).map_err(failed)?.into_iter().collect();
        uids.sort_unstable();
        Ok(uids)
    }
}

/// Everything needed to sign in to one account's servers, none of it a
/// secret: the password is found each time a connection is made and kept
/// by nothing.
pub struct ImapAccount {
    pub name: String,
    pub server: Server,
    /// Where the account's mail is handed over for sending.
    pub smtp: Option<Server>,
    pub password_cmd: Option<Vec<String>>,
    /// For an account that signs in with Google and has no password.
    pub gmail: Option<Tokens>,
    pub keyring: Box<dyn Keyring>,
    /// Where the account's pinned certificate is, if it has one.
    pub config_dir: PathBuf,
}

/// What an account signs in with now: its password from the command or
/// the keyring, or for Gmail an access token, refreshed when it must be.
pub fn credential(
    account: &str,
    password_cmd: Option<&[String]>,
    gmail: Option<&Tokens>,
    keyring: &dyn Keyring,
) -> Result<Credential, Error> {
    match gmail {
        Some(tokens) => Ok(Credential::Bearer(tokens.access(account, keyring)?)),
        None => Ok(Credential::Password(resolve(account, password_cmd, keyring)?)),
    }
}

impl ImapAccount {
    fn credential(&self) -> Result<Credential, Error> {
        credential(&self.name, self.password_cmd.as_deref(), self.gmail.as_ref(), self.keyring.as_ref())
    }

    /// A server that refuses a token may know something the token's own
    /// lifetime did not say, so the next attempt asks for a new one.
    fn refused<T>(&self, result: Result<T, RemoteError>) -> Result<T, Error> {
        if let (Err(RemoteError::Auth), Some(tokens)) = (&result, &self.gmail) {
            tokens.forget();
        }
        Ok(result?)
    }
}

impl Connect for ImapAccount {
    fn connect(&self) -> Result<Box<dyn RemoteWrite>, Error> {
        let credential = self.credential()?;
        let pinned = pin::load(&self.config_dir, &self.name)?;
        Ok(Box::new(self.refused(Imap::connect(&self.server, &self.name, &credential, pinned))?))
    }

    fn send(&self, letter: &Letter) -> Result<(), Error> {
        let none = || Error::Unsendable(format!("account {} has no `smtp` server in the config", self.name));
        let server = self.smtp.as_ref().ok_or_else(none)?;
        let credential = self.credential()?;
        let pinned = pin::load(&self.config_dir, &self.name)?;
        let sent = smtp::send(server, &self.name, &credential, pinned, letter);
        if let (Err(Error::Remote(RemoteError::Auth)), Some(tokens)) = (&sent, &self.gmail) {
            tokens.forget();
        }
        sent
    }
}

/// Signs in to `server` with what `tuimex setup` has in hand, before any
/// of it is saved.
pub fn sign_in(
    server: &Server,
    account: &str,
    credential: &Credential,
    pinned: Option<Vec<u8>>,
) -> Result<Box<dyn Remote>, RemoteError> {
    Ok(Box::new(Imap::connect(server, account, credential, pinned)?))
}

#[cfg(test)]
mod tests;

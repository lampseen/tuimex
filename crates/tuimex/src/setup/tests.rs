use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use anyhow::Result;
use tuimex_core::{RemoteError, RemoteFolder};
use tuimex_engine::calendar::{CalendarError, Got, Sent, Wire};
use tuimex_engine::oauth::{self, Endpoints};
use tuimex_engine::{Credential, Keyring, Protocol, Secret, Seen, Server, pin};
use tuimex_fixtures::FakeGoogle;
use tuimex_store::files;

use super::{Prompt, Setup, write};
use crate::config::Config;
use crate::net::Net;
use crate::outside::Opener;

pub(crate) const PASSWORD: &str = "synthetic-app-password-71c0";
pub(crate) const CLIENT_ID: &str = "synthetic-client.apps.example.com";
pub(crate) const CLIENT_SECRET: &str = "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";

/// A directory of this test's own under the build directory.
pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("setup-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// The person at the terminal: answers in order, and everything they were
/// shown. A question with no answer left is the end of input.
#[derive(Default)]
pub(crate) struct Script {
    answers: VecDeque<String>,
    pub said: Vec<String>,
}

impl Script {
    pub fn new(answers: &[&str]) -> Script {
        Script { answers: answers.iter().map(ToString::to_string).collect(), said: Vec::new() }
    }

    fn next(&mut self, question: &str) -> Result<String> {
        self.said.push(question.to_string());
        self.answers.pop_front().ok_or_else(|| anyhow::anyhow!("no answer was scripted for: {question}"))
    }

    pub fn shown(&self) -> String {
        self.said.join("\n")
    }
}

impl Prompt for Script {
    fn say(&mut self, text: &str) {
        self.said.push(text.to_string());
    }

    fn ask(&mut self, question: &str) -> Result<String> {
        self.next(question)
    }

    fn secret(&mut self, question: &str) -> Result<Secret> {
        assert!(question.contains("not shown"), "{question}");
        self.next(question).map(Secret::new)
    }
}

/// A keyring that exists only in the test: the user's is never opened.
#[derive(Default)]
pub(crate) struct MemoryKeyring {
    pub kept: Mutex<BTreeMap<String, String>>,
    /// Why the service cannot be reached, where the test wants none.
    pub down: Option<&'static str>,
}

impl MemoryKeyring {
    pub fn names(&self) -> Vec<String> {
        self.kept.lock().unwrap().keys().cloned().collect()
    }

    pub fn get(&self, entry: &str) -> Option<String> {
        self.kept.lock().unwrap().get(entry).cloned()
    }
}

impl Keyring for MemoryKeyring {
    fn password(&self, entry: &str) -> Result<Option<Secret>, String> {
        self.down.map_or(Ok(()), |why| Err(why.to_string()))?;
        Ok(self.get(entry).map(Secret::new))
    }

    fn set(&self, entry: &str, secret: &Secret) -> Result<(), String> {
        self.down.map_or(Ok(()), |why| Err(why.to_string()))?;
        self.kept.lock().unwrap().insert(entry.to_string(), secret.expose().to_string());
        Ok(())
    }

    fn remove(&self, entry: &str) -> Result<(), String> {
        self.down.map_or(Ok(()), |why| Err(why.to_string()))?;
        self.kept.lock().unwrap().remove(entry);
        Ok(())
    }
}

/// An account's two servers, as far as setup and doctor ask after them.
pub(crate) struct FakeNet {
    pub password: &'static str,
    /// Asked whether a bearer token is one it issued, as Gmail asks Google.
    pub google: Option<FakeGoogle>,
    pub imap: Seen,
    pub smtp: Seen,
    pub folders: Vec<&'static str>,
    pub gmail: bool,
    /// Every sign-in attempted: the server's port and what it was pinned to.
    pub signed_in: RefCell<Vec<(u16, Option<Vec<u8>>)>>,
}

pub(crate) fn certificate(byte: u8, trusted: bool) -> Seen {
    Seen { certificate: vec![byte; 32], trusted }
}

impl FakeNet {
    pub fn trusted() -> FakeNet {
        let seen = certificate(1, true);
        FakeNet {
            password: PASSWORD,
            google: None,
            imap: seen.clone(),
            smtp: seen,
            folders: vec!["INBOX", "Archive", "Trash", "Sent", "Drafts", "Junk"],
            gmail: false,
            signed_in: RefCell::default(),
        }
    }

    pub fn self_signed(byte: u8) -> FakeNet {
        FakeNet { imap: certificate(byte, false), smtp: certificate(byte, false), ..FakeNet::trusted() }
    }

    /// What a real connection does with a certificate and a credential.
    fn admit(
        &self,
        server: &Server,
        seen: &Seen,
        credential: &Credential,
        pinned: Option<Vec<u8>>,
    ) -> Result<(), RemoteError> {
        self.signed_in.borrow_mut().push((server.port, pinned.clone()));
        match pinned {
            Some(pinned) => pin::check("the account", &pinned, &seen.certificate)
                .map_err(|changed| RemoteError::Connection(changed.to_string()))?,
            None if !seen.trusted => {
                return Err(RemoteError::Connection("invalid peer certificate: UnknownIssuer".to_string()));
            }
            None => {}
        }
        let taken = match credential {
            Credential::Password(password) => password.expose() == self.password,
            Credential::Bearer(token) => self.google.as_ref().is_some_and(|google| google.accepts(token.expose())),
        };
        if taken { Ok(()) } else { Err(RemoteError::Auth) }
    }
}

impl Net for FakeNet {
    fn certificate(&self, server: &Server, protocol: Protocol) -> Result<Seen, RemoteError> {
        // Each port speaks TLS one way, as a real one does.
        if server.starttls == [993, 465].contains(&server.port) {
            return Err(RemoteError::Connection("the handshake was answered with something else".to_string()));
        }
        Ok(if protocol == Protocol::Imap { self.imap.clone() } else { self.smtp.clone() })
    }

    fn folders(
        &self,
        server: &Server,
        _account: &str,
        credential: &Credential,
        pinned: Option<Vec<u8>>,
    ) -> Result<(Vec<RemoteFolder>, bool), RemoteError> {
        self.admit(server, &self.imap, credential, pinned)?;
        let folder =
            |name: &&str| RemoteFolder { name: name.to_string(), delimiter: Some('/'), attributes: Vec::new() };
        Ok((self.folders.iter().map(folder).collect(), self.gmail))
    }

    fn smtp(
        &self,
        server: &Server,
        _account: &str,
        credential: &Credential,
        pinned: Option<Vec<u8>>,
    ) -> Result<(), tuimex_engine::Error> {
        Ok(self.admit(server, &self.smtp, credential, pinned)?)
    }
}

/// A feed's private address, which no file and no line may hold.
pub(crate) const FEED: &str = "https://calendar.example.net/api/calendar/v1/url/SYNTHETIC-FEED-TOKEN-4f1a/calendar.ics";
/// Gmail's calendar for `ada@gmail.com`, as setup offers it.
pub(crate) const GOOGLE_DAV: &str = "https://www.google.com/calendar/dav/ada@gmail.com/events/";

pub(crate) const DENTIST: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:d@example.net\r\n\
                                  DTSTART:20261127T090000Z\r\nSUMMARY:Dentist\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

/// A CalDAV calendar's three answers: what it is, what it lists, and the
/// events asked for, told apart by what each request's body asks.
pub(crate) fn caldav(url: &'static str) -> Vec<(&'static str, &'static str, &'static str, u16, String)> {
    let path = url.trim_start_matches("https://www.google.com");
    vec![
        (
            "PROPFIND",
            url,
            "propfind",
            207,
            format!(
                r#"<D:multistatus xmlns:D="DAV:" xmlns:CS="http://calendarserver.org/ns/"><D:response><D:href>{path}</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:displayname>ada@gmail.com</D:displayname><CS:getctag>t1</CS:getctag></D:prop></D:propstat></D:response></D:multistatus>"#
            ),
        ),
        (
            "REPORT",
            url,
            "calendar-query",
            207,
            format!(
                r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>{path}standup.ics</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:getetag>"1"</D:getetag></D:prop></D:propstat></D:response></D:multistatus>"#
            ),
        ),
        (
            "REPORT",
            url,
            "calendar-multiget",
            207,
            format!(
                r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:response><D:href>{path}standup.ics</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:getetag>"1"</D:getetag><C:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:standup@example.org
DTSTART:20261126T100000Z
SUMMARY:Standup
END:VEVENT
END:VCALENDAR
</C:calendar-data></D:prop></D:propstat></D:response></D:multistatus>"#
            ),
        ),
    ]
}

/// Calendar servers: each request is answered by the first entry whose
/// method and address it has and whose body holds the entry's words. An
/// entry's address that ends in `*` is every address that begins so: a
/// new event's name is made when it is written.
/// What it was asked, it keeps: method, address, and the sign-in's user.
#[derive(Default)]
pub(crate) struct FakeWire {
    pub served: Vec<(&'static str, &'static str, &'static str, u16, String)>,
    pub asked: RefCell<Vec<(String, String, Option<String>)>>,
    /// The tag every answer carries, for a server that gives one.
    pub etag: Option<&'static str>,
}

impl FakeWire {
    pub fn new(served: Vec<(&'static str, &'static str, &'static str, u16, String)>) -> FakeWire {
        FakeWire { served, asked: RefCell::default(), etag: None }
    }
}

impl Wire for FakeWire {
    fn send(&self, sent: &Sent<'_>) -> Result<Got, CalendarError> {
        let user = sent.sign_in.map(|(user, _)| user.to_string());
        self.asked.borrow_mut().push((sent.method.to_string(), sent.url.to_string(), user));
        let body = sent.body.map_or("", |body| body.text);
        let at = |url: &str| url.strip_suffix('*').map_or(url == sent.url, |begins| sent.url.starts_with(begins));
        let found = self
            .served
            .iter()
            .find(|(method, url, words, ..)| *method == sent.method && at(url) && body.contains(words));
        let (.., status, body) = found.ok_or_else(|| CalendarError::Unreachable("its host was not found".into()))?;
        Ok(Got { status: *status, body: body.clone(), etag: self.etag.map(str::to_string) })
    }
}

/// One run of setup in a scratch root, as the person in `script`.
pub(crate) struct Run<'a> {
    pub root: &'a Path,
    pub keyring: &'a MemoryKeyring,
    pub net: &'a FakeNet,
    pub google: Option<&'a FakeGoogle>,
    /// Whether this machine has a browser to open.
    pub browser: bool,
    pub claude: bool,
}

impl Run<'_> {
    pub fn config(&self) -> PathBuf {
        self.root.join("config/config.toml")
    }

    /// With calendar servers that answer nothing.
    pub fn go(&self, answers: &[&str]) -> (Result<()>, Script) {
        self.go_with(&FakeWire::default(), answers)
    }

    pub fn go_with(&self, wire: &FakeWire, answers: &[&str]) -> (Result<()>, Script) {
        let mut script = Script::new(answers);
        let endpoints = self.google.map_or_else(
            // A port nothing listens on: a test that reaches it has gone wrong.
            || Endpoints { authorize: "http://127.0.0.1:9/auth".into(), token: "http://127.0.0.1:9/token".into() },
            |google| Endpoints { authorize: google.authorize_url(), token: google.token_url() },
        );
        let mut browser = Browser(self.google.cloned());
        let (config_dir, config_path) = (self.root.join("config"), self.config());
        let result = Setup {
            prompt: &mut script,
            keyring: self.keyring,
            net: self.net,
            config_dir: &config_dir,
            data: &self.root.join("data"),
            cache: &self.root.join("cache"),
            config_path: &config_path,
            endpoints,
            browser: if self.browser { Some(&mut browser) } else { None },
            claude: self.claude.then(|| PathBuf::from("/usr/bin/claude")),
            wire,
        }
        .run();
        (result, script)
    }
}

/// The browser: Google's consent, then the redirect it answers with,
/// followed on a thread as a browser would while setup waits. No test
/// opens a real one.
struct Browser(Option<FakeGoogle>);

impl Opener for Browser {
    fn open(&mut self, address: &str) -> std::io::Result<()> {
        let google = self.0.as_ref().expect("a browser was opened with no Google to visit");
        let back = google.authorize(address).map_err(std::io::Error::other)?;
        thread::spawn(move || visit(&back));
        Ok(())
    }
}

pub(crate) fn visit(address: &str) {
    let rest = address.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut stream = TcpStream::connect(host).unwrap();
    stream.write_all(format!("GET /{path} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes()).unwrap();
    let mut page = String::new();
    stream.read_to_string(&mut page).unwrap();
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// Address, provider, the four server answers, username, name, sign-in by
/// password, the password, no agent access, and no calendar.
const BY_HAND: [&str; 13] = [
    "ada@example.org",
    "5",
    "imap.example.org",
    "",
    "smtp.example.org",
    "587",
    "",
    "work",
    "1",
    PASSWORD,
    "n",
    "n",
    "",
];

const WORK: &str = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n\
                    imap = { host = \"imap.example.org\", port = 993 }\nsmtp = { host = \"smtp.example.org\", port = 587 }\n\
                    auth = \"password\"\n";

#[test]
fn an_account_is_added_from_nothing_once_both_servers_have_taken_the_password() {
    let root = scratch("from-nothing");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet::trusted());
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let (result, script) = run.go(&BY_HAND);
    result.unwrap();
    assert_eq!(fs::read_to_string(run.config()).unwrap(), WORK);
    assert_eq!((mode(&run.config()), mode(&root.join("config"))), (0o600, 0o700));
    assert_eq!(keyring.get("work").as_deref(), Some(PASSWORD));
    assert_eq!(keyring.names(), ["work"]);
    // Both were signed in to, IMAP first, against the system's roots.
    assert_eq!(*net.signed_in.borrow(), [(993, None), (587, None)]);
    assert!(!root.join("config/work.pem").exists());
    let shown = script.shown();
    assert!(shown.contains("Signed in to imap.example.org:993 and found 6 folders."), "{shown}");
    assert!(shown.contains("Signed in to smtp.example.org:587; nothing was sent."), "{shown}");
    assert!(!shown.contains(PASSWORD), "{shown}");
}

#[test]
fn a_refused_sign_in_a_refused_certificate_or_a_dead_keyring_saves_nothing() {
    let root = scratch("nothing-saved");
    let saved = |keyring: &MemoryKeyring| (root.join("config").exists(), keyring.names().len());
    let wrong = BY_HAND.map(|answer| if answer == PASSWORD { "not-the-password" } else { answer });

    let (keyring, net) = (MemoryKeyring::default(), FakeNet::trusted());
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let error = run.go(&wrong).0.unwrap_err().to_string();
    assert_eq!(error, "imap.example.org:993 refused the sign-in; nothing was saved");
    assert_eq!(saved(&keyring), (false, 0));

    // A certificate nobody vouches for is shown, and the answer is no.
    let net = FakeNet::self_signed(7);
    let run = Run { net: &net, ..run };
    let mut declined = BY_HAND[..10].to_vec();
    declined.push("n");
    let (result, script) = run.go(&declined);
    assert_eq!(result.unwrap_err().to_string(), "the certificate was not accepted; nothing was saved");
    assert!(script.shown().contains(&format!("SHA-256 fingerprint: {}", pin::fingerprint(&[7; 32]))));
    assert!(net.signed_in.borrow().is_empty(), "no password was offered to a server that was not trusted");
    assert_eq!(saved(&keyring), (false, 0));

    // A machine with no keyring service: the advice, and still nothing.
    let down = MemoryKeyring { down: Some("no such service"), ..MemoryKeyring::default() };
    let net = FakeNet::trusted();
    let run = Run { keyring: &down, net: &net, ..run };
    let error = run.go(&BY_HAND).0.unwrap_err().to_string();
    assert!(error.starts_with("the keyring would not keep it: no such service\n"), "{error}");
    assert!(error.contains("a command that prints its password") && error.ends_with("Nothing was saved."), "{error}");
    assert_eq!(saved(&down), (false, 0));
}

#[test]
fn an_untrusted_certificate_is_shown_and_once_accepted_pinned_for_both_servers() {
    let root = scratch("pinned");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet::self_signed(7));
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    // Proton Bridge: its ports and username are asked, with its defaults.
    let answers = ["ada@proton.me", "", "", "", "bridge-user", "", "1", PASSWORD, "y", "n", "n", ""];
    let (result, script) = run.go(&answers);
    result.unwrap();
    let shown = script.shown();
    assert!(shown.contains("127.0.0.1:1143 presents a certificate this system has no reason to trust"), "{shown}");
    assert!(shown.contains("Proton Bridge must be running on this computer"), "{shown}");
    assert!(shown.contains("  1) The password Proton Bridge shows, typed now."), "{shown}");
    assert_eq!(pin::load(&root.join("config"), "proton").unwrap().unwrap(), [7; 32]);
    assert_eq!(mode(&root.join("config/proton.pem")), 0o600);
    assert_eq!(*net.signed_in.borrow(), [(1143, Some(vec![7; 32])), (1025, Some(vec![7; 32]))]);
    let config = fs::read_to_string(run.config()).unwrap();
    assert!(config.contains("imap = { host = \"127.0.0.1\", port = 1143, user = \"bridge-user\" }"), "{config}");
    assert!(config.contains("smtp = { host = \"127.0.0.1\", port = 1025, user = \"bridge-user\" }"), "{config}");

    // Two servers with two certificates cannot share the account's one pin.
    let split = FakeNet { smtp: certificate(8, false), ..FakeNet::self_signed(7) };
    let other = scratch("pinned-split");
    let run = Run { root: &other, net: &split, ..run };
    let error = run.go(&answers).0.unwrap_err().to_string();
    assert!(error.starts_with("the two servers present different certificates"), "{error}");
    assert!(!other.join("config").exists());
}

#[test]
fn a_second_account_is_added_after_what_was_there_which_is_kept_to_the_byte() {
    let root = scratch("second");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet::trusted());
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: true };
    fs::create_dir_all(root.join("config")).unwrap();
    let mine = "# mine, and it stays\n[ui]\nglyphs = \"ascii\"   # for the old terminal\n\n[[account]]\nname = \"home\"\n\
                address = \"ada@example.com\"  # the first\n";
    fs::write(run.config(), mine).unwrap();
    // Enter adds another; iCloud is guessed from the address; the name
    // offered is taken; an agent may read; `:agent` may use Claude.
    let (result, script) = run.go(&["", "ada@icloud.com", "", "", "1", PASSWORD, "y", "n", "y", ""]);
    result.unwrap();
    let text = fs::read_to_string(run.config()).unwrap();
    let added = "\n[[account]]\nname = \"icloud\"\naddress = \"ada@icloud.com\"\n\
                 imap = { host = \"imap.mail.me.com\", port = 993, user = \"ada\" }\n\
                 smtp = { host = \"smtp.mail.me.com\", port = 587 }\nauth = \"password\"\n\n\
                 [account.agent]\nread = true\nfolders = [\"INBOX\"]\ndraft = false\n\n[agent]\nuse = \"claude\"\n";
    assert_eq!(text, format!("{mine}{added}"));
    assert_eq!(mode(&run.config()), 0o600, "a file that was the user's is owner-only once setup has written it");
    let shown = script.shown();
    assert!(shown.contains("Accounts here: home.") && shown.contains("that text goes to Anthropic"), "{shown}");
    assert!(!shown.contains("--tools"), "no argv is shown to the user");

    // A third: the name offered steps past one that is taken, and with an
    // [agent] table there the question is not asked again.
    let (result, script) = run.go(&["", "bob@icloud.com", "", "", "2", "printf", "%s", PASSWORD, "", "n", "n", ""]);
    result.unwrap();
    let third = fs::read_to_string(run.config()).unwrap();
    assert!(third.starts_with(&text) && !script.shown().contains("Claude"), "{third}");
    assert!(
        third.ends_with("auth = \"command\"\npassword_cmd = [\"printf\", \"%s\", \"synthetic-app-password-71c0\"]\n")
    );
    assert!(third.contains("name = \"icloud-2\""));
    assert_eq!(keyring.names(), ["icloud"], "a command's password is not the keyring's");
    Config::parse(&third).unwrap();
}

#[test]
fn a_config_setup_cannot_add_to_is_left_alone_and_the_table_is_given_to_add_by_hand() {
    let root = scratch("by-hand");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet::trusted());
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    fs::create_dir_all(root.join("config")).unwrap();
    // Valid, and laid out so that no `[[account]]` can follow it.
    let inline = "account = [{ name = \"home\", address = \"ada@example.com\" }]\n";
    fs::write(run.config(), inline).unwrap();
    let mut answers = vec![""];
    answers.extend(BY_HAND);
    let error = run.go(&answers).0.unwrap_err().to_string();
    assert!(error.starts_with("nothing was written; add this to the config by hand:\n\n[[account]]\nname = \"work\""));
    assert_eq!(fs::read_to_string(run.config()).unwrap(), inline);
    assert!(keyring.names().is_empty(), "nothing is kept for an account the config did not take");

    fs::write(run.config(), "[[account]]\nname = \"home\"\npassword = \"hunter2\"\n").unwrap();
    let error = format!("{:#}", run.go(&answers).0.unwrap_err());
    assert!(
        error.contains("does not read, and setup adds only to one that does: line 3: unknown key `password`"),
        "{error}"
    );
    assert!(!error.contains("hunter2"));

    assert!(write::appended("[sync]\nfolders = [", &["[agent]\nuse = \"claude\"\n".to_string()]).is_err());
    let odd = write::appended(
        "",
        &[write::account(&write::Entry {
            name: "odd".into(),
            address: "a\"b@example.com".into(),
            imap: Server { host: "h\\".into(), port: 143, starttls: false, user: "a\"b@example.com".into() },
            smtp: Server { host: "s".into(), port: 465, starttls: true, user: "u\n".into() },
            auth: crate::net::Auth::Password,
            password_cmd: Vec::new(),
            folders: vec![("archive", "All \"Mail\"".into())],
            agent: (false, true),
        })],
    )
    .unwrap();
    let read = Config::parse(&odd).unwrap();
    let (imap, smtp) = crate::net::servers(&read.accounts[0]).unwrap();
    assert_eq!((imap.host.as_str(), imap.starttls, smtp.unwrap().user.as_str()), ("h\\", false, "u\n"));
    assert!(read.accounts[0].grants().draft && !read.accounts[0].grants().read);
}

#[test]
fn the_roles_the_mapping_could_not_fill_are_asked_for_from_the_folders_left() {
    let root = scratch("roles");
    let keyring = MemoryKeyring::default();
    let net = FakeNet { folders: vec!["INBOX", "Sent", "Kept", "Bin"], ..FakeNet::trusted() };
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    // Archive is the first of the two left, Trash then the only one left,
    // and nothing is left to ask about for Drafts or Junk.
    let mut answers = BY_HAND[..10].to_vec();
    answers.extend(["1", "1", "n", "n", ""]);
    let (result, script) = run.go(&answers);
    result.unwrap();
    let text = fs::read_to_string(run.config()).unwrap();
    assert!(text.contains("folders = { archive = \"Kept\", trash = \"Bin\" }\n"), "{text}");
    let shown = script.shown();
    assert!(shown.contains("There is no folder for: drafts, junk. tuimex asks before it makes an Archive"), "{shown}");
    let asked = "No folder here is plainly the archive folder, which is where tuimex puts what you archive with `e`.";
    assert!(shown.contains(asked), "{shown}");
}

fn google() -> FakeGoogle {
    FakeGoogle::start(CLIENT_ID, CLIENT_SECRET).unwrap()
}

#[test]
fn two_gmail_accounts_sign_in_through_a_browser_and_share_one_client() {
    let root = scratch("gmail");
    let google = google();
    let keyring = MemoryKeyring::default();
    let net = FakeNet { google: Some(google.clone()), gmail: true, folders: vec!["INBOX"], ..FakeNet::trusted() };
    let run = Run { root: &root, keyring: &keyring, net: &net, google: Some(&google), browser: true, claude: false };
    // Gmail is guessed and its name is the default; Google's sign-in is
    // the third way offered.
    let (result, script) = run.go(&["ada@gmail.com", "", "", "3", CLIENT_ID, CLIENT_SECRET, "n", "n", ""]);
    result.unwrap();
    let shown = script.shown();
    assert!(
        shown.contains("create a client of type \"Desktop app\"") && shown.contains("A browser is opening"),
        "{shown}"
    );
    let entries = [oauth::refresh_entry("gmail"), oauth::CLIENT_ID.to_string(), oauth::CLIENT_SECRET.to_string()];
    assert_eq!(keyring.names(), entries);
    assert_eq!(keyring.get(oauth::CLIENT_SECRET).as_deref(), Some(CLIENT_SECRET));
    assert!(keyring.get(&entries[0]).unwrap().starts_with("1//synthetic-refresh-"));
    assert_eq!(net.signed_in.borrow().len(), 2, "IMAP and SMTP both took the token");

    // The second is asked for no client: the first one's is in the keyring.
    let (result, script) = run.go(&["", "bob@gmail.com", "", "", "3", "n", "n", ""]);
    result.unwrap();
    assert!(script.shown().contains("Using the Google client already in the keyring."));
    assert!(!script.shown().contains("Client ID:"));
    assert_ne!(keyring.get(&oauth::refresh_entry("gmail")), keyring.get(&oauth::refresh_entry("gmail-2")));
    let text = fs::read_to_string(run.config()).unwrap();
    let account = |name: &str, address: &str| {
        format!(
            "[[account]]\nname = \"{name}\"\naddress = \"{address}\"\nimap = {{ host = \"imap.gmail.com\", port = 993 }}\n\
             smtp = {{ host = \"smtp.gmail.com\", port = 465 }}\nauth = \"gmail-oauth\"\n"
        )
    };
    assert_eq!(text, format!("{}\n{}", account("gmail", "ada@gmail.com"), account("gmail-2", "bob@gmail.com")));
    for secret in google.tokens().iter().map(String::as_str).chain([CLIENT_SECRET]) {
        assert!(!text.contains(secret) && !shown.contains(secret), "{secret}");
    }
}

#[test]
fn with_no_browser_the_address_is_printed_and_the_pasted_redirect_or_code_is_taken() {
    let root = scratch("gmail-ssh");
    let google = google();
    let keyring = MemoryKeyring::default();
    keyring.set(oauth::CLIENT_ID, &Secret::new(CLIENT_ID.into())).unwrap();
    keyring.set(oauth::CLIENT_SECRET, &Secret::new(CLIENT_SECRET.into())).unwrap();
    let net = FakeNet { google: Some(google.clone()), gmail: true, ..FakeNet::trusted() };
    let run = Run { root: &root, keyring: &keyring, net: &net, google: Some(&google), browser: false, claude: false };
    // The script cannot know the address before setup prints it, so the
    // person here is a prompt that visits Google when asked to paste.
    struct Pasting<'a>(Script, &'a FakeGoogle, bool);
    impl Prompt for Pasting<'_> {
        fn say(&mut self, text: &str) {
            self.0.say(text);
        }
        fn ask(&mut self, question: &str) -> Result<String> {
            if !question.starts_with("That address") {
                return self.0.ask(question);
            }
            let shown = self.0.shown();
            let url = shown.lines().rev().find(|line| line.trim().starts_with("http://127.0.0.1")).unwrap().trim();
            let back = self.1.authorize(url).unwrap();
            if self.2 {
                return Ok(back);
            }
            Ok(back.split("code=").nth(1).unwrap().split('&').next().unwrap().to_string())
        }
        fn secret(&mut self, question: &str) -> Result<Secret> {
            self.0.secret(question)
        }
    }
    for (whole, name) in [(true, "gmail"), (false, "gmail-2")] {
        let mut person = Pasting(Script::new(&["", "ada@gmail.com", "", "", "3", "n", "n", ""]), &google, whole);
        if name == "gmail" {
            person.0.answers.pop_front();
        }
        let (config_dir, config_path) = (root.join("config"), run.config());
        Setup {
            prompt: &mut person,
            keyring: &keyring,
            net: &net,
            config_dir: &config_dir,
            data: &root.join("data"),
            cache: &root.join("cache"),
            config_path: &config_path,
            endpoints: Endpoints { authorize: google.authorize_url(), token: google.token_url() },
            browser: None,
            claude: None,
            wire: &FakeWire::default(),
        }
        .run()
        .unwrap();
        assert!(person.0.shown().contains("copy the whole address from the address bar"));
        assert!(keyring.get(&oauth::refresh_entry(name)).is_some());
    }
}

#[test]
fn an_account_chosen_from_the_menu_accepts_its_server_s_new_certificate_without_rewriting_the_config() {
    let root = scratch("re-pin");
    let keyring = MemoryKeyring::default();
    let first = FakeNet::self_signed(7);
    let run = Run { root: &root, keyring: &keyring, net: &first, google: None, browser: false, claude: false };
    let mut answers = BY_HAND[..10].to_vec();
    answers.extend(["y", "n", "n", ""]);
    run.go(&answers).0.unwrap();
    let written = fs::read_to_string(run.config()).unwrap() + "# a note of mine\n";
    fs::write(run.config(), &written).unwrap();

    // The same certificate: nothing to ask, and the kept password still works.
    let (result, script) = run.go(&["3"]);
    result.unwrap();
    let shown = script.shown();
    let menu = "\nAccounts here: work.\n  1) Add another account\n  2) Sign in again to work\n  \
                3) Accept a new certificate for work\n  4) Rename an account\n  \
                5) Add a calendar to an account that has none\n  6) Quit\n";
    assert!(shown.contains(menu), "{shown}");
    assert!(shown.contains("The server still presents the certificate pinned for work."));
    assert!(shown.contains("The certificate of work's server has not changed; there was nothing to accept."));

    // A new one: both fingerprints, and the answer decides.
    let renewed = FakeNet::self_signed(9);
    let run = Run { net: &renewed, ..run };
    let (result, script) = run.go(&["3", "n"]);
    assert_eq!(result.unwrap_err().to_string(), "the certificate was not accepted; nothing was saved");
    let shown = script.shown();
    assert!(shown.contains(&format!("  pinned:    {}", pin::fingerprint(&[7; 32]))), "{shown}");
    assert!(shown.contains(&format!("  presented: {}", pin::fingerprint(&[9; 32]))), "{shown}");
    assert_eq!(pin::load(&root.join("config"), "work").unwrap().unwrap(), [7; 32]);
    run.go(&["3", "y"]).0.unwrap();
    assert_eq!(pin::load(&root.join("config"), "work").unwrap().unwrap(), [9; 32]);
    assert_eq!(fs::read_to_string(run.config()).unwrap(), written, "the config, comments and all, was not touched");

    // Signing in afresh replaces the password in the keyring.
    let moved = FakeNet { password: "synthetic-second-password", ..FakeNet::self_signed(9) };
    let run = Run { net: &moved, ..run };
    assert!(run.go(&["3"]).0.unwrap_err().to_string().contains("refused the sign-in"));
    run.go(&["2", "synthetic-second-password"]).0.unwrap();
    assert_eq!(keyring.get("work").as_deref(), Some("synthetic-second-password"));
    // What is not on the menu is asked for again, and quitting changes nothing.
    let (result, script) = run.go(&["work", "7", "6"]);
    result.unwrap();
    let shown = script.shown();
    assert_eq!(shown.matches("Answer with a number from 1 to 6.").count(), 2, "{shown}");
    assert!(shown.ends_with("Nothing was changed."), "{shown}");
    assert_eq!(keyring.get("work").as_deref(), Some("synthetic-second-password"));
    assert_eq!(fs::read_to_string(run.config()).unwrap(), written);
}

/// What a first-time user is told at the question that stopped one: what
/// each way needs from them, with nothing they have to know already
/// leading a line.
#[test]
fn the_sign_in_question_says_what_each_way_needs_and_an_app_password_is_gmail_s_default() {
    let root = scratch("ways");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet { gmail: true, ..FakeNet::trusted() });
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    // Gmail is guessed; Enter takes its name, and Enter the first way.
    let (result, script) = run.go(&["ada@gmail.com", "", "", "", PASSWORD, "n", "n", ""]);
    result.unwrap();
    let shown = script.shown();
    let question = &shown[shown.find("How should tuimex sign in to ada@gmail.com?").expect(&shown)..];
    let ways: Vec<&str> =
        question.lines().filter(|line| line.starts_with("  ") && line[3..].starts_with(") ")).collect();
    assert_eq!(ways.len(), 3, "{question}");
    assert!(ways[0].starts_with("  1) An app password, typed now (the quickest): a separate 16-character password"));
    assert!(ways[1].starts_with("  2) A command that prints the password: for people who keep passwords in a"));
    assert!(ways[2].starts_with("  3) Sign in with Google in a browser: tuimex is given no password."), "{question}");
    for told in [
        "https://myaccount.google.com/apppasswords",
        "2-Step",
        "It is not your Google password.",
        "`pass show\n     mail/gmail`",
        "a machine with no keyring",
        "about ten minutes",
        "asks for\n     the client's ID and secret",
    ] {
        assert!(question.contains(told), "{told:?} in:\n{question}");
    }
    assert!(!ways.iter().any(|way| way.contains("OAuth")), "no line leads with a word to look up: {ways:?}");
    assert!(fs::read_to_string(run.config()).unwrap().contains("auth = \"password\""));
    assert_eq!(keyring.get("gmail").as_deref(), Some(PASSWORD));

    // A provider with one kind of password is offered two ways, and what
    // is not offered is not taken.
    let (result, script) = run.go(&["1", "ada@fastmail.com", "", "", "3", "1", PASSWORD, "n", "n", ""]);
    result.unwrap();
    let shown = script.shown();
    assert!(shown.contains("  1) An app password, typed now: a separate password Fastmail makes"), "{shown}");
    assert!(!shown.contains("Google") && shown.contains("Answer with a number from 1 to 2."), "{shown}");
}

/// Bridge's window has the answers to the questions about ports, so it
/// is named before they are asked.
#[test]
fn proton_bridge_is_named_before_the_questions_only_its_window_answers() {
    let root = scratch("bridge-first");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet::self_signed(7));
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let (_, script) = run.go(&["ada@proton.me", ""]);
    let shown = script.shown();
    let (bridge, port) = (shown.find("Proton Bridge must be running on this computer"), shown.find("IMAP port"));
    assert!(bridge.is_some() && bridge < port, "{shown}");
}

/// What setup leaves when a calendar has been offered: a config that
/// parses, with no secret in it.
fn calendar_saved(run: &Run) -> Config {
    let text = fs::read_to_string(run.config()).unwrap();
    for secret in [PASSWORD, FEED, "SYNTHETIC-FEED-TOKEN"] {
        assert!(!text.contains(secret), "{secret} in:\n{text}");
    }
    Config::parse(&text).unwrap()
}

/// Gmail with an app password: its calendar is offered over CalDAV, which
/// is tried with that password once before anything is written.
#[test]
fn a_gmail_account_s_calendar_is_read_over_caldav_with_its_app_password() {
    let root = scratch("calendar-gmail");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet { gmail: true, ..FakeNet::trusted() });
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let wire = FakeWire::new(caldav(GOOGLE_DAV));
    // Yes to reading it, and no to trying a write.
    let (result, script) = run.go_with(&wire, &["ada@gmail.com", "", "", "", PASSWORD, "n", "n", "y", "n"]);
    result.unwrap();
    let text = fs::read_to_string(run.config()).unwrap();
    assert!(
        text.ends_with(&format!("\n[[calendar]]\naccount = \"gmail\"\nkind = \"caldav\"\nurl = \"{GOOGLE_DAV}\"\n")),
        "{text}"
    );
    let config = calendar_saved(&run);
    assert_eq!(config.calendar("gmail").unwrap().url.as_deref(), Some(GOOGLE_DAV));
    assert_eq!(
        *wire.asked.borrow(),
        [("PROPFIND".to_string(), GOOGLE_DAV.to_string(), Some("ada@gmail.com".to_string()))],
        "one request, signed in as the mail is"
    );
    assert_eq!(keyring.names(), ["gmail"], "nothing more is kept: the app password is the calendar's too");
    let shown = script.shown();
    assert!(shown.contains("Google's calendar took the sign-in.") && shown.contains("Saved gmail's calendar in"));
    assert!(shown.contains("to show beside the mail: `gc` opens it."), "{shown}");
    assert!(shown.contains("Nothing was written. The calendar is read"), "{shown}");
    assert!(!config.writes("gmail"), "a write nobody tried is not a calendar tuimex writes");
}

/// The address a new event is put at and deleted from: a name tuimex
/// makes under Google's calendar.
const GOOGLE_EVENTS: &str = "https://www.google.com/calendar/dav/ada@gmail.com/events/*";

fn methods(wire: &FakeWire) -> Vec<String> {
    wire.asked.borrow().iter().map(|(method, ..)| method.clone()).collect()
}

/// ADR-062: writing is found out, not assumed. Setup says what it will
/// do, asks, puts one event a year ahead and deletes it, and only then
/// writes the key that has tuimex write the calendar.
#[test]
fn a_calendar_is_written_once_setup_has_put_an_event_there_and_removed_it_again() {
    let root = scratch("calendar-write");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet { gmail: true, ..FakeNet::trusted() });
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let mut served = caldav(GOOGLE_DAV);
    served.push(("PUT", GOOGLE_EVENTS, "SUMMARY:tuimex write test", 201, String::new()));
    served.push(("DELETE", GOOGLE_EVENTS, "", 204, String::new()));
    let wire = FakeWire { etag: Some("\"t1\""), ..FakeWire::new(served) };
    let (result, script) = run.go_with(&wire, &["ada@gmail.com", "", "", "", PASSWORD, "n", "n", "y", "y"]);
    result.unwrap();
    let shown = script.shown();
    let (told, asked) = (
        shown.find("adds one event named \"tuimex write test\", a year from today, and removes"),
        shown.find("Let tuimex write to gmail's calendar? No keeps it read-only. [y/N]"),
    );
    assert!(told.is_some() && told < asked, "what it will do is said before it is asked: {shown}");
    assert!(shown.contains("The event was added and removed again: tuimex will write this calendar."), "{shown}");
    assert_eq!(methods(&wire), ["PROPFIND", "PUT", "DELETE"]);
    let asked = wire.asked.borrow();
    assert_eq!(asked[1].1, asked[2].1, "what was put is what was removed");
    let name = asked[1].1.strip_prefix(GOOGLE_DAV).and_then(|name| name.strip_suffix(".ics")).expect("an .ics name");
    assert!(name.len() == 32 && name.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
    let text = fs::read_to_string(run.config()).unwrap();
    let table =
        format!("\n[[calendar]]\naccount = \"gmail\"\nkind = \"caldav\"\nurl = \"{GOOGLE_DAV}\"\nwrite = true\n");
    assert!(text.ends_with(&table), "{text}");
    assert!(calendar_saved(&run).writes("gmail"));
    let store = tuimex_store::calendar::Calendars::open(&root.join("data"), &root.join("cache")).unwrap();
    assert!(store.held().unwrap().is_empty(), "nothing of the test is kept");
}

/// Whatever goes wrong is said as it happened, and the calendar is left
/// one tuimex reads and does not write.
#[test]
fn a_write_that_fails_is_said_and_leaves_the_calendar_read_only() {
    let ending = |root: &Path, put: u16, delete: u16| {
        let (keyring, net) = (MemoryKeyring::default(), FakeNet { gmail: true, ..FakeNet::trusted() });
        let run = Run { root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
        let mut served = caldav(GOOGLE_DAV);
        served.push(("PUT", GOOGLE_EVENTS, "", put, String::new()));
        served.push(("DELETE", GOOGLE_EVENTS, "", delete, String::new()));
        let wire = FakeWire { etag: Some("\"t1\""), ..FakeWire::new(served) };
        let (result, script) = run.go_with(&wire, &["ada@gmail.com", "", "", "", PASSWORD, "n", "n", "y", "y"]);
        result.unwrap();
        let config = calendar_saved(&run);
        assert_eq!(config.calendar("gmail").unwrap().url.as_deref(), Some(GOOGLE_DAV), "it is still read");
        assert!(!config.writes("gmail"));
        (script.shown(), methods(&wire))
    };
    let (shown, asked) = ending(&scratch("calendar-write-refused"), 403, 204);
    let said = "The event could not be added: the calendar's server refused the sign-in. The calendar stays read-only \
                here, and what you do for it is kept locally.";
    assert!(shown.contains(said), "{shown}");
    assert_eq!(asked, ["PROPFIND", "PUT"], "nothing was added, so nothing is removed");

    let (shown, _) = ending(&scratch("calendar-write-left"), 201, 500);
    let said = "The event was added and could not be removed (the calendar's server answered 500): delete \"tuimex \
                write test\", a year from today, in the calendar itself. The calendar stays read-only here";
    assert!(shown.contains(said), "{shown}");
}

/// A calendar that was in the file before the write was tried gains the
/// key in a table of its own after everything that was there, which is
/// kept to the byte (ADR-043).
#[test]
fn a_calendar_already_in_the_config_is_tried_from_the_menu_and_the_key_is_added_after_what_was_there() {
    let root = scratch("calendar-write-later");
    files::private_dir(&root.join("config")).unwrap();
    let before = format!(
        "# my mail\n[[account]]\nname = \"gmail\"\naddress = \"ada@gmail.com\"\n\
         imap = {{ host = \"imap.gmail.com\", port = 993 }}\nsmtp = {{ host = \"smtp.gmail.com\", port = 465 }}\n\
         auth = \"password\"\n\n[[calendar]]\naccount = \"gmail\"\nkind = \"caldav\"\nurl = \"{GOOGLE_DAV}\"\n\n\
         [send]\ndelay = \"30s\"\n"
    );
    fs::write(root.join("config/config.toml"), &before).unwrap();
    let (keyring, net) = (MemoryKeyring::default(), FakeNet { gmail: true, ..FakeNet::trusted() });
    keyring.set("gmail", &Secret::new(PASSWORD.to_string())).unwrap();
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let served = vec![
        ("PUT", GOOGLE_EVENTS, "SUMMARY:tuimex write test", 201, String::new()),
        ("DELETE", GOOGLE_EVENTS, "", 204, String::new()),
    ];
    let wire = FakeWire { etag: Some("\"t1\""), ..FakeWire::new(served) };

    // Asked and declined, nothing is sent and nothing written.
    let (result, script) = run.go_with(&wire, &["5", "n"]);
    result.unwrap();
    let shown = script.shown();
    assert!(shown.contains("  5) Find out whether a calendar that is only read can be written\n  6) Quit"), "{shown}");
    assert!(wire.asked.borrow().is_empty());
    assert_eq!(fs::read_to_string(run.config()).unwrap(), before);

    let (result, script) = run.go_with(&wire, &["5", "y"]);
    result.unwrap();
    assert_eq!(methods(&wire), ["PUT", "DELETE"]);
    let text = fs::read_to_string(run.config()).unwrap();
    assert_eq!(text, format!("{before}\n[[calendar]]\naccount = \"gmail\"\nwrite = true\n"));
    assert!(calendar_saved(&run).writes("gmail"));
    assert!(script.shown().contains("gmail's calendar is written from now on."), "{}", script.shown());
    // Written, it is no longer offered.
    let (result, script) = run.go(&["5"]);
    result.unwrap();
    assert!(!script.shown().contains("Find out whether"), "{}", script.shown());
}

/// A refusal is said in plain words, and the feed is offered instead.
#[test]
fn a_gmail_calendar_that_refuses_the_app_password_is_offered_by_its_secret_address() {
    let root = scratch("calendar-gmail-refused");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet { gmail: true, ..FakeNet::trusted() });
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let wire = FakeWire::new(vec![
        ("PROPFIND", GOOGLE_DAV, "", 401, String::new()),
        ("GET", FEED, "", 200, DENTIST.to_string()),
    ]);
    let (result, script) = run.go_with(&wire, &["ada@gmail.com", "", "", "", PASSWORD, "n", "n", "y", "y", FEED]);
    result.unwrap();
    let shown = script.shown();
    assert!(
        shown.contains("The calendar could not be read that way: Google refused the app password for the calendar."),
        "{shown}"
    );
    assert!(shown.contains("Settings, then the calendar, then \"Secret address in iCal format\""), "{shown}");
    assert!(shown.contains("calendar.example.net served a calendar."), "{shown}");
    assert!(!shown.contains(FEED) && !shown.contains("SYNTHETIC-FEED-TOKEN"), "{shown}");
    let config = calendar_saved(&run);
    let calendar = config.calendar("gmail").unwrap();
    assert_eq!((calendar.kind.as_deref(), calendar.url.as_deref()), (Some("feed"), None));
    assert_eq!(keyring.get("gmail/calendar-feed").as_deref(), Some(FEED));
}

/// Any other account: where the address is found, when that is known,
/// asked for with nothing shown, read once, and kept in the keyring.
#[test]
fn a_feed_is_asked_for_unseen_checked_once_and_kept_in_the_keyring_alone() {
    let root = scratch("calendar-feed");
    let (keyring, net) = (MemoryKeyring::default(), FakeNet::self_signed(7));
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let wire = FakeWire::new(vec![("GET", FEED, "", 200, DENTIST.to_string())]);
    // Proton Bridge, as in the test of pinning; then yes, and an address
    // given as `webcal:`, which is the same one over HTTPS.
    let webcal = FEED.replace("https://", "webcal://");
    let answers = ["ada@proton.me", "", "", "", "bridge-user", "", "1", PASSWORD, "y", "n", "n", "y", &webcal];
    let (result, script) = run.go_with(&wire, &answers);
    result.unwrap();
    let shown = script.shown();
    assert!(shown.contains("Settings, then the calendar, then \"Share via link\", with full view."), "{shown}");
    assert!(shown.contains("Anyone with the address can read the calendar"), "{shown}");
    assert!(script.said.iter().any(|line| line == "The calendar's address (it is not shown as you type):"));
    assert_eq!(*wire.asked.borrow(), [("GET".to_string(), FEED.to_string(), None)]);
    let text = fs::read_to_string(run.config()).unwrap();
    assert!(text.ends_with("\n[[calendar]]\naccount = \"proton\"\nkind = \"feed\"\n"), "{text}");
    calendar_saved(&run);
    assert_eq!(keyring.get("proton/calendar-feed").as_deref(), Some(FEED));
    assert!(shown.contains("Its address is in the keyring, and in no file."), "{shown}");
}

/// The menu offers a calendar to each account that has none, once each;
/// a web page in place of a calendar, and a keyring that cannot keep the
/// address, each save nothing; no is not remembered.
#[test]
fn an_account_already_here_is_offered_a_calendar_and_what_cannot_be_kept_is_not_written() {
    let root = scratch("calendar-menu");
    files::private_dir(&root.join("config")).unwrap();
    let two = format!(
        "{WORK}\n[[account]]\nname = \"home\"\naddress = \"ada@example.com\"\nimap = {{ host = \"imap.example.com\", port = 993 }}\n"
    );
    fs::write(root.join("config/config.toml"), &two).unwrap();
    let (keyring, net) = (MemoryKeyring::default(), FakeNet::trusted());
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let page = FakeWire::new(vec![("GET", FEED, "", 200, "<!doctype html><title>Sign in</title>".to_string())]);

    // No to both: each asked once, nothing written; the next run asks again.
    let (result, script) = run.go_with(&page, &["7", "", ""]);
    result.unwrap();
    let shown = script.shown();
    assert!(shown.contains("  7) Add a calendar to an account that has none\n  8) Quit"), "{shown}");
    for name in ["work", "home"] {
        let asked = format!("Add {name}'s calendar by its private address? [y/N]");
        assert_eq!(shown.matches(&asked).count(), 1, "{shown}");
    }
    assert_eq!(fs::read_to_string(run.config()).unwrap(), two);

    // A web page where the calendar should be.
    let (result, script) = run.go_with(&page, &["7", "y", FEED, ""]);
    result.unwrap();
    assert!(script.shown().contains("What that address served is not a calendar"), "{}", script.shown());
    assert_eq!(fs::read_to_string(run.config()).unwrap(), two);
    assert!(keyring.names().is_empty());

    // No keyring service: the address needs a command, and nothing is
    // written anywhere.
    let down = MemoryKeyring { down: Some("no such service"), ..MemoryKeyring::default() };
    let served = FakeWire::new(vec![("GET", FEED, "", 200, DENTIST.to_string())]);
    let (result, script) = Run { keyring: &down, ..run }.go_with(&served, &["7", "y", FEED, ""]);
    result.unwrap();
    let shown = script.shown();
    assert!(shown.contains("The keyring would not keep the address: no such service"), "{shown}");
    assert!(shown.contains("address_cmd = [\"pass\", \"show\", \"calendar/work\"]"), "{shown}");
    assert!(shown.contains("Nothing was saved.") && !shown.contains(FEED), "{shown}");
    assert_eq!(fs::read_to_string(run.config()).unwrap(), two);

    // Then one is added, and the menu offers the other alone.
    run.go_with(&served, &["7", "y", FEED, ""]).0.unwrap();
    calendar_saved(&run);
    assert_eq!(keyring.get("work/calendar-feed").as_deref(), Some(FEED));
    let (result, script) = run.go_with(&served, &["7", ""]);
    result.unwrap();
    let shown = script.shown();
    assert!(!shown.contains("Add work's calendar") && shown.contains("Add home's calendar"), "{shown}");
}

/// A rename carries the account's calendar with it: its table and the
/// keyring entry that holds its feed's address.
#[test]
fn a_rename_takes_the_account_s_calendar_and_its_feed_s_address_along() {
    let root = scratch("calendar-rename");
    files::private_dir(&root.join("config")).unwrap();
    let text = format!("{WORK}\n[[calendar]]\naccount = \"work\"\nkind = \"feed\"\n");
    fs::write(root.join("config/config.toml"), &text).unwrap();
    let keyring = MemoryKeyring::default();
    keyring.set("work", &Secret::new(PASSWORD.into())).unwrap();
    keyring.set("work/calendar-feed", &Secret::new(FEED.into())).unwrap();
    let net = FakeNet::trusted();
    let run = Run { root: &root, keyring: &keyring, net: &net, google: None, browser: false, claude: false };
    let store = tuimex_store::calendar::Calendars::open(&root.join("data"), &root.join("cache")).unwrap();
    store.keep("work", DENTIST).unwrap();
    store.put("work", "feed", "", None, DENTIST).unwrap();
    drop(store);
    let (result, script) = run.go(&["4", "office", "y"]);
    result.unwrap();
    assert!(
        script.shown().contains("  4) Rename an account\n  5) Quit"),
        "no calendar is offered to an account with one"
    );
    let renamed = fs::read_to_string(run.config()).unwrap();
    assert_eq!(renamed, text.replace("\"work\"", "\"office\""));
    assert_eq!(keyring.names(), ["office", "office/calendar-feed"]);
    assert_eq!(keyring.get("office/calendar-feed").as_deref(), Some(FEED));
    // Its own events go with it; what was fetched is dropped, to be
    // fetched again under the new name.
    assert!(!root.join("data/.calendars/work").exists());
    assert_eq!(fs::read_dir(root.join("data/.calendars/office")).unwrap().count(), 1);
    assert!(!root.join("cache/calendar.sqlite").exists());
}

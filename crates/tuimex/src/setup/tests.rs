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
use tuimex_engine::oauth::{self, Endpoints};
use tuimex_engine::{Credential, Keyring, Protocol, Secret, Seen, Server, pin};
use tuimex_fixtures::FakeGoogle;

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

    pub fn go(&self, answers: &[&str]) -> (Result<()>, Script) {
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
/// password, the password, no agent access.
const BY_HAND: [&str; 12] =
    ["ada@example.org", "5", "imap.example.org", "", "smtp.example.org", "587", "", "work", "1", PASSWORD, "n", "n"];

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
    let answers = ["ada@proton.me", "", "", "", "bridge-user", "", "1", PASSWORD, "y", "n", "n"];
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
    let (result, script) = run.go(&["", "ada@icloud.com", "", "", "1", PASSWORD, "y", "n", "y"]);
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
    let (result, script) = run.go(&["", "bob@icloud.com", "", "", "2", "printf", "%s", PASSWORD, "", "n", "n"]);
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
    answers.extend(["1", "1", "n", "n"]);
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
    let (result, script) = run.go(&["ada@gmail.com", "", "", "3", CLIENT_ID, CLIENT_SECRET, "n", "n"]);
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
    let (result, script) = run.go(&["", "bob@gmail.com", "", "", "3", "n", "n"]);
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
        let mut person = Pasting(Script::new(&["", "ada@gmail.com", "", "", "3", "n", "n"]), &google, whole);
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
    answers.extend(["y", "n", "n"]);
    run.go(&answers).0.unwrap();
    let written = fs::read_to_string(run.config()).unwrap() + "# a note of mine\n";
    fs::write(run.config(), &written).unwrap();

    // The same certificate: nothing to ask, and the kept password still works.
    let (result, script) = run.go(&["3"]);
    result.unwrap();
    let shown = script.shown();
    let menu = "\nAccounts here: work.\n  1) Add another account\n  2) Sign in again to work\n  \
                3) Accept a new certificate for work\n  4) Rename an account\n  5) Quit\n";
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
    let (result, script) = run.go(&["work", "6", "5"]);
    result.unwrap();
    let shown = script.shown();
    assert_eq!(shown.matches("Answer with a number from 1 to 5.").count(), 2, "{shown}");
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
    let (result, script) = run.go(&["ada@gmail.com", "", "", "", PASSWORD, "n", "n"]);
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
    let (result, script) = run.go(&["1", "ada@fastmail.com", "", "", "3", "1", PASSWORD, "n", "n"]);
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

//! ADR-010: a secret reaches no file tuimex writes, and nothing it says.
//! The engine is given a known password both ways it can be (the keyring,
//! a `password_cmd`), syncs, fetches a body, sends, and fails at each in
//! every way it can; then every file under the scratch root and every
//! event and error it produced is searched for the password.

// A search that cannot read its input must fail rather than pass on less.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use tuimex_core::{Action, Capabilities, Event, Flags, Origin, RemoteError, RemoteWrite, Timestamp};
use tuimex_engine::sync::Policy;
use tuimex_engine::{Account, Connect, Engine, Error, Keyring, Letter, Secret, StoreHandle, pin, resolve};
use tuimex_fixtures::{FakeServer, NOW};
use tuimex_store::Store;

const SECRET: &str = "tuimex-synthetic-secret-8f3a91c2";
const PATIENCE: Duration = Duration::from_secs(20);

/// A keyring that exists only in this test: the user's is never opened.
struct FakeKeyring(Result<Option<&'static str>, &'static str>);

impl Keyring for FakeKeyring {
    fn password(&self, _account: &str) -> Result<Option<Secret>, String> {
        self.0.map(|found| found.map(|p| Secret::new(p.to_string()))).map_err(ToString::to_string)
    }

    fn set(&self, _entry: &str, _secret: &Secret) -> Result<(), String> {
        Err("this keyring is read only".to_string())
    }
}

/// Signs in the way the real connection will: resolve the secret, hand it
/// to the server, keep nothing.
struct Door {
    server: FakeServer,
    password_cmd: Option<Vec<String>>,
    keyring: FakeKeyring,
    /// What the server that sends says of a message, once signed in to.
    mail: Result<(), RemoteError>,
}

impl Connect for Door {
    fn connect(&self) -> Result<Box<dyn RemoteWrite>, Error> {
        let secret = resolve("work", self.password_cmd.as_deref(), &self.keyring)?;
        Ok(Box::new(self.server.connect(secret.expose())?))
    }

    /// Signs in as a real send does, with the same password and the same
    /// refusals, before the message is taken or turned away.
    fn send(&self, _letter: &Letter) -> Result<(), Error> {
        let secret = resolve("work", self.password_cmd.as_deref(), &self.keyring)?;
        self.server.connect(secret.expose())?;
        Ok(self.mail.clone()?)
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() { files_under(&path, out) } else { out.push(path) }
    }
}

/// Every file under `dir` whose name or content holds the secret.
fn holding_the_secret(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    files_under(dir, &mut files);
    assert!(!files.is_empty(), "nothing was written under {}", dir.display());
    let holds = |path: &PathBuf| {
        // The store's thread closes the index a moment after its engine
        // stops, and SQLite then deletes its write-ahead log: a file gone
        // since it was listed is that one, and holds nothing any more.
        let content = match fs::read(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
            read => read.unwrap(),
        };
        path.to_string_lossy().contains(SECRET) || content.windows(SECRET.len()).any(|w| w == SECRET.as_bytes())
    };
    files.into_iter().filter(holds).collect()
}

/// Runs one engine until `done` picks an event, then stops it. Everything
/// it said on the way is added to `said`, as a person and as a log would
/// see it.
fn run(root: &Path, door: Door, said: &mut Vec<String>, act: impl FnOnce(&Engine, &Receiver<Event>, &mut Vec<String>)) {
    said.push(format!("{:?} {:?}", door.password_cmd, resolve("work", door.password_cmd.as_deref(), &door.keyring)));
    let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
    let (sender, events) = mpsc::channel();
    let account = Account {
        name: "work".to_string(),
        roles: Vec::new(),
        policy: Policy::default(),
        identities: vec!["ada@example.org".to_string()],
        send_delay: Duration::ZERO,
    };
    let engine = Engine::start(account, Box::new(door), StoreHandle::start(store), sender);
    act(&engine, &events, said);
    engine.stop();
    said.extend(events.try_iter().map(|e| format!("{e:?}")));
}

/// Waits for the first event `wanted` accepts, recording all it passes.
fn until(events: &Receiver<Event>, said: &mut Vec<String>, wanted: impl Fn(&Event) -> bool) -> Event {
    loop {
        let event = events.recv_timeout(PATIENCE).expect("the engine went quiet before the event came");
        said.push(format!("{event:?}"));
        if let Event::SyncError { msg, .. } = &event {
            said.push(msg.clone());
        }
        if wanted(&event) {
            return event;
        }
    }
}

fn synced(event: &Event) -> bool {
    matches!(event, Event::Synced { folder, .. } if folder.0 == "INBOX")
}

fn failed(event: &Event) -> bool {
    matches!(event, Event::SyncError { .. })
}

/// Writes a draft, sends it, and waits for what became of it.
fn send(engine: &Engine, events: &Receiver<Event>, said: &mut Vec<String>) -> Event {
    let draft = b"From: ada@example.org\r\nTo: grace@example.net\r\nSubject: sent\r\n\r\nhello\r\n".to_vec();
    let on = vec![engine.save_draft(draft, None, Origin::Local)];
    engine.act(tuimex_core::Ask { serial: 1, action: Action::Send, on });
    until(events, said, |e| matches!(e, Event::Sent { .. } | Event::SendFailed { .. }))
}

#[test]
fn a_secret_never_reaches_a_file_or_anything_the_engine_says() {
    let root = scratch("secret-root");
    // The stand-in for a password manager, outside what tuimex writes.
    let vault = scratch("secret-vault").join("password");
    fs::write(&vault, format!("{SECRET}\n")).unwrap();
    let vault = vault.to_string_lossy().to_string();
    let argv = |words: &[&str]| Some(words.iter().map(ToString::to_string).collect::<Vec<_>>());

    let capabilities =
        Capabilities { uidplus: true, r#move: true, condstore: true, special_use: true, idle: true, gmail: false };
    let server = FakeServer::new(SECRET, capabilities);
    server.add_folder("INBOX", &[], 7);
    let old = Timestamp(NOW.0 - 3 * 366 * 86_400);
    server.deliver("INBOX", b"Subject: one\r\n\r\nthe first body\r\n", Flags::default(), old);
    let door = |password_cmd, keyring| Door {
        server: server.clone(),
        password_cmd,
        keyring: FakeKeyring(keyring),
        mail: Ok(()),
    };
    let mut said = Vec::new();

    // The password from the keyring: a sync, then a body fetched on demand.
    run(&root, door(None, Ok(Some(SECRET))), &mut said, |engine, events, said| {
        until(events, said, synced);
        let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
        let key = store.synced("work", "INBOX").unwrap().remove(0).key;
        engine.body(tuimex_core::MessageId { account: tuimex_core::AccountId("work".to_string()), key }, 80);
        let body = until(events, said, |e| matches!(e, Event::BodyReady { .. } | Event::BodyFailed { .. }));
        assert!(matches!(body, Event::BodyReady { .. }), "{body:?}");
        let sent = send(engine, events, said);
        assert!(matches!(sent, Event::Sent { .. }), "{sent:?}");
    });
    // The same send, turned away by the server that sends and then cut
    // off from it.
    for mail in [
        RemoteError::Refused("refused by smtp.example.org: 552 too large".to_string()),
        RemoteError::Connection("reset".to_string()),
    ] {
        let turned_away = Door { mail: Err(mail), ..door(None, Ok(Some(SECRET))) };
        run(&root, turned_away, &mut said, |engine, events, said| {
            let unsent = send(engine, events, said);
            assert!(matches!(unsent, Event::SendFailed { .. }), "{unsent:?}");
        });
    }
    // The password from a command, with new mail to bring.
    server.deliver("INBOX", b"Subject: two\r\n\r\nthe second body\r\n", Flags::default(), NOW);
    run(&root, door(argv(&["cat", &vault]), Ok(None)), &mut said, |_, events, said| _ = until(events, said, synced));

    // Each way of failing. A command that prints the password and then
    // fails; a keyring with nothing, and one that cannot be read.
    let leaky = format!("cat {vault}; cat {vault} >&2; exit 3");
    for (password_cmd, keyring) in
        [(argv(&["sh", "-c", &leaky]), Ok(None)), (None, Ok(None)), (None, Err("the keyring is locked"))]
    {
        run(&root, door(password_cmd, keyring), &mut said, |engine, events, said| {
            until(events, said, failed);
            let unsent = send(engine, events, said);
            assert!(matches!(unsent, Event::SendFailed { .. }), "{unsent:?}");
        });
    }
    // A server that no longer takes the password, for mail in or out.
    let moved_on = FakeServer::new("another-password", capabilities);
    let refused = Door { server: moved_on, ..door(None, Ok(Some(SECRET))) };
    run(&root, refused, &mut said, |engine, events, said| {
        until(events, said, failed);
        let unsent = send(engine, events, said);
        assert!(matches!(unsent, Event::SendFailed { .. }), "{unsent:?}");
    });
    // A server nobody answers at, then one that hangs up mid-sync, then
    // one whose folder was renumbered.
    server.refuse_connections(usize::MAX);
    run(&root, door(None, Ok(Some(SECRET))), &mut said, |_, events, said| _ = until(events, said, failed));
    server.refuse_connections(0);
    // On a server that cannot idle: a watcher that idles asks a second
    // thing of its own, and when that is what the hang-up answers, the
    // sync that connects afterwards never meets it.
    let hangs_up = FakeServer::new(SECRET, Capabilities { idle: false, ..capabilities });
    hangs_up.add_folder("INBOX", &[], 7);
    hangs_up.drop_after(1);
    let lost = Door { server: hangs_up, ..door(None, Ok(Some(SECRET))) };
    run(&root, lost, &mut said, |_, events, said| _ = until(events, said, failed));
    server.renumber("INBOX", 70);
    run(&root, door(None, Ok(Some(SECRET))), &mut said, |_, events, said| _ = until(events, said, synced));
    // A pinned certificate and its refusal are written and said too.
    pin::save(&root.join("config"), "work", b"a synthetic certificate").unwrap();
    said.push(pin::check("work", b"a synthetic certificate", b"another").unwrap_err().to_string());
    said.push(format!("{:?}", Secret::new(SECRET.to_string())));

    // The search finds the secret where it really is, so finding nothing
    // elsewhere means something.
    assert_eq!(holding_the_secret(Path::new(&vault).parent().unwrap()).len(), 1);
    assert_eq!(holding_the_secret(&root), Vec::<PathBuf>::new(), "files that hold the secret");
    assert!(root.join("cache/index.sqlite").exists() && root.join("config/work.pem").exists());
    assert!(root.join("data/work/journal.jsonl").exists() && root.join("data/work/.Outbox/cur").exists());
    assert!(said.len() > 40, "the engine said too little to have been exercised: {said:#?}");
    let leaks: Vec<&String> = said.iter().filter(|line| line.contains(SECRET)).collect();
    assert_eq!(leaks, Vec::<&String>::new(), "things said that hold the secret");
}

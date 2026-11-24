//! What the unit tests of every module share, and the tests of `parse`.

use std::cell::Cell;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tuimex_core::{
    AccountId, Action, Ask, Capabilities, Event, Flags, FolderId, MessageId, Query, Remote, RemoteError, RemoteWrite,
    Security, Sort, Timestamp,
};
use tuimex_fixtures::{FakeServer, NOW, mailbox};
use tuimex_store::Store;
use tuimex_store::journal::Entry;

use super::sync::Policy;
use super::{Account, Connect, Engine, Error, Letter, StoreHandle, parse};

pub(crate) const ALL: Capabilities =
    Capabilities { uidplus: true, r#move: true, condstore: true, special_use: true, idle: true, gmail: false };
/// A server that cannot idle is watched by polling, minutes apart, so
/// once the engine has settled a test is the only thing that moves.
pub(crate) const QUIET: Capabilities = Capabilities { idle: false, ..ALL };
pub(crate) const PASSWORD: &str = "synthetic-password";
/// Long enough that only a hang reaches it.
pub(crate) const PATIENCE: Duration = Duration::from_secs(20);

pub(crate) fn the_account() -> Account {
    Account {
        name: "work".to_string(),
        roles: Vec::new(),
        policy: Policy::default(),
        identities: vec!["ada@example.org".to_string()],
        send_delay: Duration::ZERO,
    }
}

/// The server that sends the account's mail: it keeps what it is handed,
/// and answers the next attempts as a test scripts them.
#[derive(Clone, Default)]
pub(crate) struct PostOffice(Arc<Mutex<(Vec<Letter>, VecDeque<RemoteError>)>>);

impl PostOffice {
    pub fn fail_next(&self, error: RemoteError) {
        self.0.lock().unwrap().1.push_back(error);
    }

    pub fn handed(&self) -> Vec<Letter> {
        self.0.lock().unwrap().0.clone()
    }
}

pub(crate) struct Door {
    pub server: FakeServer,
    pub password: &'static str,
    pub post: PostOffice,
}

impl Connect for Door {
    fn connect(&self) -> Result<Box<dyn RemoteWrite>, Error> {
        Ok(Box::new(self.server.connect(self.password)?))
    }

    fn send(&self, letter: &Letter) -> Result<(), Error> {
        let mut post = self.post.0.lock().unwrap();
        match post.1.pop_front() {
            Some(error) => Err(error.into()),
            None => {
                post.0.push(letter.clone());
                Ok(())
            }
        }
    }
}

pub(crate) struct Running {
    pub world: World,
    pub engine: Engine,
    pub events: Receiver<Event>,
    pub post: PostOffice,
    /// The serial of the last ask `act` made.
    pub serial: Cell<u64>,
}

pub(crate) fn start(
    name: &str,
    capabilities: Capabilities,
    password: &'static str,
    prepare: impl FnOnce(&World),
) -> Running {
    let world = world(name, capabilities);
    prepare(&world);
    Running::over(world, the_account(), password)
}

impl Running {
    /// An engine over a world that may already have a history.
    pub fn over(world: World, account: Account, password: &'static str) -> Running {
        let ((sender, events), post) = (mpsc::channel(), PostOffice::default());
        let door = Door { server: world.server.clone(), password, post: post.clone() };
        let engine = Engine::start(account, Box::new(door), world.store.clone(), sender);
        Running { world, engine, events, post, serial: Cell::new(0) }
    }

    /// The next event `wanted` picks, skipping the others.
    pub fn wait_for<T>(&self, wanted: impl Fn(&Event) -> Option<T>) -> T {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let event = self.events.recv_timeout(left).expect("the engine went quiet before the event came");
            if let Some(found) = wanted(&event) {
                return found;
            }
        }
    }

    /// Waits out the passes every start makes: the syncer's own, and the
    /// one the watcher asks for once its connection is open. The Trash is
    /// the last folder of a pass.
    pub fn settle(&self) {
        for _ in 0..2 {
            self.wait_for(|e| matches!(e, Event::Synced { folder: FolderId(f), .. } if f == "Trash").then_some(()));
        }
        self.world.server.take_log();
    }

    pub fn retry_in(&self) -> (String, Option<Duration>) {
        self.wait_for(|e| match e {
            Event::SyncError { account: AccountId(a), msg, retry_in } if a == "work" => Some((msg.clone(), *retry_in)),
            _ => None,
        })
    }

    /// Asks for `action` and waits for what the engine says of it.
    pub fn act(&self, action: Action, on: &[MessageId]) -> Result<String, (String, String)> {
        self.serial.set(self.serial.get() + 1);
        self.engine.act(Ask { serial: self.serial.get(), action, on: on.to_vec() });
        self.said()
    }

    /// The next thing said of an action, done or not, or said unasked.
    pub fn said(&self) -> Result<String, (String, String)> {
        self.told().1
    }

    /// As `said`, with the serial of the ask a refusal names.
    pub fn told(&self) -> (Option<u64>, Result<String, (String, String)>) {
        self.wait_for(|e| match e {
            Event::ActionDone { summary } => Some((None, Ok(summary.clone()))),
            Event::ActionFailed { serial, reason, hint } => Some((Some(*serial), Err((reason.clone(), hint.clone())))),
            Event::Notice { error: true, text, hint, .. } => Some((None, Err((text.clone(), hint.clone())))),
            Event::Notice { error: false, text, .. } => Some((None, Ok(text.clone()))),
            _ => None,
        })
    }

    /// Waits until the journal is empty and the passes that emptied it
    /// have ended: a move is confirmed before its message is numbered, and
    /// two whole passes asked for afterwards end after both.
    pub fn replayed(&self) {
        let deadline = Instant::now() + PATIENCE;
        while !self.world.journal().is_empty() {
            assert!(Instant::now() < deadline, "the journal never emptied: {:?}", self.world.journal());
            std::thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..2 {
            self.engine.sync();
            self.wait_for(|e| matches!(e, Event::Synced { folder: FolderId(f), .. } if f == "Trash").then_some(()));
        }
    }
}

/// A server with the usual folders and a store with nothing in it, under
/// a scratch root.
pub(crate) struct World {
    pub server: FakeServer,
    pub store: StoreHandle,
    pub root: PathBuf,
}

pub(crate) fn world(name: &str, capabilities: Capabilities) -> World {
    let root = scratch(name);
    let server = FakeServer::new(PASSWORD, capabilities);
    for (folder, attributes, uid_validity) in [
        ("INBOX", &[][..], 7),
        ("Archive", &["\\Archive"][..], 8),
        ("Trash", &["\\Trash"][..], 9),
        ("Projects", &[][..], 10),
    ] {
        server.add_folder(folder, attributes, uid_validity);
    }
    let store = Store::open(&root.join("data"), &root.join("cache"), parse).unwrap();
    World { server, store: StoreHandle::start(store), root }
}

pub(crate) fn message(n: u32) -> Vec<u8> {
    let head =
        format!("From: Grace Hopper <grace@example.net>\r\nSubject: message {n}\r\nMessage-ID: <{n}@example.net>");
    format!("{head}\r\nContent-Type: text/plain\r\n\r\nthe body of message {n}\r\n").into_bytes()
}

impl World {
    /// Delivers `message(n)` to the server's Inbox just now, unread.
    pub fn deliver(&self, n: u32) -> u32 {
        self.server.deliver("INBOX", &message(n), Flags::default(), NOW)
    }

    /// What is here in a folder: each message's UID, subject, flags and
    /// whether only its headers are, lowest UID first.
    pub fn here(&self, folder: &'static str) -> Vec<(u32, String, Flags, bool)> {
        let found = self.store.run(move |s| {
            let mut found = Vec::new();
            for envelope in s.query(&Query::parse(&format!("in:{folder}")), NOW, Sort::NewestFirst, None)? {
                let stored = s.stored(&envelope.id)?.1;
                found.push((stored.uid.unwrap_or(0), envelope.subject, envelope.flags, stored.headers_only));
            }
            Ok(found)
        });
        let mut found = found.unwrap();
        found.sort_by_key(|(uid, ..)| *uid);
        found
    }

    pub fn uids(&self, folder: &'static str) -> Vec<u32> {
        self.here(folder).into_iter().map(|(uid, ..)| uid).collect()
    }

    /// The message here that the server numbered `uid` in the Inbox.
    pub fn id(&self, uid: u32) -> MessageId {
        let synced = self.store.run(|s| s.synced("work", "INBOX")).unwrap();
        let key = synced.into_iter().find(|m| m.uid == uid).unwrap().key;
        MessageId { account: AccountId("work".to_string()), key }
    }

    pub fn journal(&self) -> Vec<Entry> {
        self.store.run(|s| s.pending("work")).unwrap()
    }

    /// The UIDs the server lists in a folder, as another client sees them.
    pub fn there(&self, folder: &str) -> Vec<u32> {
        let mut other = self.server.connect(PASSWORD).unwrap();
        other.select(folder).unwrap();
        other.uids().unwrap()
    }
}

/// A path of this test's own under the build directory, with nothing at
/// it. Never the system temporary directory, and never anything the user
/// owns.
pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

#[test]
fn parse_gives_the_index_what_the_fixture_mailbox_says_of_each_message() {
    for mail in mailbox() {
        let parsed = parse(&mail.raw());
        let at = format!("{} in {}/{}", mail.subject, mail.account, mail.folder);
        assert_eq!(parsed.date, mail.date, "{at}");
        assert_eq!(parsed.from, std::slice::from_ref(&mail.from), "{at}");
        assert_eq!((parsed.to, parsed.cc), (mail.to.clone(), mail.cc.clone()), "{at}");
        assert_eq!(parsed.subject, mail.subject, "{at}");
        assert_eq!(parsed.references.len(), mail.references.len(), "{at}");
        assert_eq!(parsed.message_id.as_deref(), Some(mail.message_id.trim_matches(['<', '>'])), "{at}");
        assert_eq!(parsed.has_attachments, mail.has_attachments, "{at}");
        assert_eq!(parsed.attachments.is_empty(), !mail.has_attachments, "{at}: a calendar part is not a file");
        assert_eq!(parsed.has_invite, mail.has_invite, "{at}");
        assert_eq!(parsed.has_unsubscribe, mail.has_unsubscribe, "{at}");
        assert_eq!(parsed.security, mail.security, "{at}");
        assert!(parsed.text.contains(mail.text), "{at}: {:?}", parsed.text);
    }
}

#[test]
fn bytes_that_are_no_message_are_indexed_as_one_with_nothing_in_it() {
    let parsed = parse(b"");
    assert_eq!((parsed.date, parsed.subject.as_str(), parsed.security), (Timestamp(0), "", Security::None));
    assert!(parsed.from.is_empty() && parsed.text.is_empty());
}

#[test]
fn a_header_block_that_promises_attachments_counts_none_until_its_body_is_here() {
    let head = "From: a@example.com\r\nSubject: two files\r\nMIME-Version: 1.0\r\n\
                Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n";
    let parsed = parse(head.as_bytes());
    assert!(parsed.has_attachments, "`has:attachment` goes by what the header says");
    assert_eq!(parsed.attachments, [], "a row says `1 file` of nothing it has counted");
    let part = |name: &str| {
        format!("--b\r\nContent-Type: application/pdf\r\nContent-Disposition: attachment; filename={name}\r\n\r\nx\r\n")
    };
    let whole =
        format!("{head}--b\r\nContent-Type: text/plain\r\n\r\nhello\r\n{}{}--b--\r\n", part("a.pdf"), part("b.pdf"));
    let names: Vec<String> = parse(whole.as_bytes()).attachments.into_iter().map(|file| file.name).collect();
    assert_eq!(names, ["a.pdf", "b.pdf"], "and the body brings the count");
}

/// Work is handed to the store's thread and not waited for, so whoever
/// stops has to: what was handed over is done when `finish` returns, and
/// the store is closed.
#[test]
fn finishing_the_store_carries_out_what_it_was_handed_before_it_closes() {
    let world = world("finish-store", QUIET);
    let done = Arc::new(Mutex::new(false));
    let slow = done.clone();
    assert!(world.store.hand(move |_| {
        std::thread::sleep(Duration::from_millis(200));
        *slow.lock().unwrap() = true;
    }));
    world.store.finish();
    assert!(*done.lock().unwrap(), "it stopped with work still waiting");
    assert!(matches!(world.store.run(|_| Ok(())), Err(Error::StoreStopped)));
    world.store.finish();
}

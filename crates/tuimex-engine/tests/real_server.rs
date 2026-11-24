//! M3's and M4's exits, against a real server named by the environment
//! (the variables are listed in `tests/spike.rs`): a sync fills a Maildir,
//! a second transfers nothing, and a message added on the server arrives
//! through IDLE; archive, trash and the undo of each reach the server;
//! a message sent to oneself arrives and is in Sent once. Each test says
//! it was skipped when there is no server, and `ran` when it ran, which
//! `make real-server` looks for.

// A test that cannot reach its server should stop with the reason.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tuimex_core::{Action, Ask, Event, MessageId, Origin, Query, Remote, RemoteError, Role, Sort, Timestamp};
use tuimex_engine::sync::{self, Policy};
use tuimex_engine::{Account, Connect, Engine, Error, Letter, StoreHandle, pin};
use tuimex_store::Store;

const PATIENCE: Duration = Duration::from_secs(60);

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-real-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(dir.join("config")).unwrap();
    dir
}

/// A subject no earlier run of these tests left on the server.
fn unique(what: &str) -> String {
    format!("{what}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos())
}

struct Here {
    root: PathBuf,
    store: StoreHandle,
    account: Account,
}

/// A scratch root whose config directory pins the server's certificate,
/// as `tuimex setup` will once the user has accepted it.
fn here(name: &str, server: &common::Server) -> Here {
    let root = scratch(name);
    fs::copy(&server.ca, root.join("config/work.pem")).unwrap();
    let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
    let account = Account {
        name: "work".to_string(),
        roles: Vec::new(),
        policy: Policy::default(),
        identities: vec![format!("{}@example.com", server.user)],
        send_delay: Duration::from_secs(1),
    };
    Here { root, store: StoreHandle::start(store), account }
}

impl Here {
    fn subjects(&self) -> Vec<String> {
        let now = Timestamp(i64::MAX / 2);
        let all = self.store.run(move |s| s.query(&Query::parse("in:inbox"), now, Sort::NewestFirst, None)).unwrap();
        all.into_iter().map(|e| e.subject).collect()
    }
}

#[test]
fn a_sync_fills_the_maildir_and_a_second_transfers_nothing() {
    let Some(server) = common::server("real-server sync") else { return };
    let subject = unique("exit-sync");
    server.session().append("INBOX", &common::message(&subject)).finish().unwrap();
    let h = here("sync", &server);
    let reach = server.account(&h.root.join("config"));
    let pass = || {
        let changed = RefCell::new(Vec::new());
        let note = |event| {
            if let Event::Synced { folder, changed: ids, .. } = event {
                changed.borrow_mut().push((folder.0, ids.len()));
            }
        };
        sync::once(&reach, &h.store, &h.account, &note).unwrap();
        changed.into_inner()
    };
    let first = pass();
    let inbox = first.iter().find(|(folder, _)| folder == "INBOX").expect("the Inbox was synced");
    assert!(inbox.1 >= 1, "{first:?}");
    assert!(h.subjects().contains(&subject), "{:?}", h.subjects());
    let files = fs::read_dir(h.root.join("data/work/cur")).unwrap().count();
    assert_eq!(files, inbox.1, "each message is a file in the Maildir");
    let roles = h.store.run(|s| s.folders()).unwrap();
    assert!(roles.iter().any(|f| f.role == Role::Inbox));

    let second = pass();
    assert!(second.iter().all(|(_, changed)| *changed == 0), "the second pass changed something: {second:?}");
    assert_eq!(fs::read_dir(h.root.join("data/work/cur")).unwrap().count(), files);
    println!("real-server: ran sync ({files} messages; first {first:?}, second {second:?})");
}

#[test]
fn a_message_added_on_the_server_arrives_through_idle() {
    let Some(server) = common::server("real-server idle") else { return };
    let h = here("idle", &server);
    let (sender, events) = mpsc::channel();
    let reach = server.account(&h.root.join("config"));
    let engine = Engine::start(h.account.clone(), Box::new(reach), h.store.clone(), sender);
    let wait_for = |wanted: &dyn Fn(&Event) -> bool| loop {
        let event = events.recv_timeout(PATIENCE).expect("the engine went quiet before the event came");
        assert!(!matches!(event, Event::SyncError { .. }), "{event:?}");
        if wanted(&event) {
            break event;
        }
    };
    // The pass every start makes, and the one the watcher asks for once
    // its own connection is open: after the second, it is idling.
    for _ in 0..2 {
        wait_for(&|e| matches!(e, Event::Synced { folder, .. } if folder.0 == "INBOX"));
    }
    let subject = unique("exit-idle");
    assert!(!h.subjects().contains(&subject));
    server.session().append("INBOX", &common::message(&subject)).finish().unwrap();
    let new = wait_for(&|e| matches!(e, Event::NewMail { .. }));
    assert!(matches!(new, Event::NewMail { count: 1, .. }), "{new:?}");
    assert!(h.subjects().contains(&subject), "{:?}", h.subjects());
    engine.stop();
    println!("real-server: ran idle ({new:?})");
}

#[test]
fn a_certificate_that_is_not_the_pinned_one_or_not_pinned_at_all_is_refused_before_signing_in() {
    let Some(server) = common::server("real-server certificate") else { return };
    let h = here("certificate", &server);
    let config = h.root.join("config");
    let pinned = pin::load(&config, "work").unwrap().unwrap();
    pin::save(&config, "work", b"the certificate this server had last year").unwrap();
    let changed = match server.account(&config).connect() {
        Err(Error::Remote(RemoteError::Connection(why))) => why,
        other => panic!("a changed certificate was not refused: {:?}", other.map(|_| "connected")),
    };
    let was = pin::fingerprint(b"the certificate this server had last year");
    assert!(changed.contains(&was) && changed.contains(&pin::fingerprint(&pinned)), "{changed}");

    // With no pin the server's own certificate is checked against the
    // system's roots, which did not sign it.
    fs::remove_file(config.join("work.pem")).unwrap();
    let unknown = server.account(&config).connect().map(|_| "connected");
    assert!(matches!(unknown, Err(Error::Remote(RemoteError::Connection(_)))), "{unknown:?}");

    // And the pin itself still lets the account in.
    pin::save(&config, "work", &pinned).unwrap();
    let mut remote: Box<dyn Remote> = server.account(&config).connect().unwrap();
    assert!(remote.select("INBOX").is_ok());
    println!("real-server: ran certificate ({changed})");
}

/// How many messages with `subject` the server lists in each of
/// `folders`, as another client sees them. One marked deleted is not
/// counted: it is gone for every client that hides what awaits a purge.
fn on_server(server: &common::Server, subject: &str, folders: &[&str]) -> Vec<usize> {
    let mut other = server.session();
    let count = |folder: &&str| {
        other.select(folder).unwrap();
        other.uid_search(format!("UNDELETED SUBJECT {subject}")).unwrap().len()
    };
    let counts = folders.iter().map(count).collect();
    other.logout().unwrap();
    counts
}

/// Waits until the server holds `subject` in `folders` as often as `want`.
fn until_on_server(server: &common::Server, subject: &str, folders: &[&str], want: &[usize]) {
    let deadline = std::time::Instant::now() + PATIENCE;
    while on_server(server, subject, folders) != want {
        assert!(
            std::time::Instant::now() < deadline,
            "{subject} in {folders:?}: {:?}, never {want:?}",
            on_server(server, subject, folders)
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The folders §4.1 finds by name on a server without SPECIAL-USE. They
/// are there already on a server these tests have run against.
fn with_folders(server: &common::Server, folders: &[&str]) {
    let mut other = server.session();
    for folder in folders {
        if let Err(e) = other.create(folder) {
            println!("real-server: creating {folder}: {e}");
        }
    }
    other.logout().unwrap();
}

struct Live {
    engine: Engine,
    events: mpsc::Receiver<Event>,
}

impl Live {
    fn start(h: &Here, server: &common::Server) -> Live {
        let (sender, events) = mpsc::channel();
        let reach = server.account(&h.root.join("config"));
        let live = Live { engine: Engine::start(h.account.clone(), Box::new(reach), h.store.clone(), sender), events };
        // The pass every start makes, and the one the watcher asks for.
        for _ in 0..2 {
            live.wait_for(&|e| matches!(e, Event::Synced { folder, .. } if folder.0 == "INBOX"));
        }
        live
    }

    fn wait_for(&self, wanted: &dyn Fn(&Event) -> bool) -> Event {
        loop {
            let event = self.events.recv_timeout(PATIENCE).expect("the engine went quiet before the event came");
            let bad = matches!(event, Event::SyncError { .. } | Event::ActionFailed { .. } | Event::SendFailed { .. });
            assert!(!bad, "{event:?}");
            if wanted(&event) {
                break event;
            }
        }
    }

    fn act(&self, action: Action, on: &[MessageId]) -> String {
        self.engine.act(Ask { serial: 1, action, on: on.to_vec() });
        match self.wait_for(&|e| matches!(e, Event::ActionDone { .. })) {
            Event::ActionDone { summary } => summary,
            other => unreachable!("{other:?}"),
        }
    }
}

#[test]
fn archive_and_trash_and_the_undo_of_each_reach_the_server() {
    let Some(server) = common::server("real-server archive-trash-undo") else { return };
    with_folders(&server, &["Archive", "Trash"]);
    let subject = unique("exit-move");
    server.session().append("INBOX", &common::message(&subject)).finish().unwrap();
    let h = here("move", &server);
    let live = Live::start(&h, &server);
    let wanted = subject.clone();
    let found =
        h.store.run(move |s| s.query(&Query::parse("in:inbox"), Timestamp(i64::MAX / 2), Sort::NewestFirst, None));
    let id = found.unwrap().into_iter().find(|e| e.subject == wanted).expect("the message was synced").id;
    let mut said = Vec::new();
    for (action, folder) in [(Action::Archive, "Archive"), (Action::Trash, "Trash")] {
        said.push(live.act(action, std::slice::from_ref(&id)));
        until_on_server(&server, &subject, &["INBOX", folder], &[0, 1]);
        said.push(live.act(Action::Undo, std::slice::from_ref(&id)));
        until_on_server(&server, &subject, &["INBOX", folder], &[1, 0]);
    }
    let back = "Undone: 1 message back in INBOX";
    assert_eq!(said, ["Archived 1 message", back, "Trashed 1 message", back]);
    // A whole pass later it is here once, where it began, under the
    // same name.
    live.engine.sync();
    live.wait_for(&|e| matches!(e, Event::Synced { folder, .. } if folder.0 == "Trash"));
    assert_eq!(h.subjects().iter().filter(|s| **s == subject).count(), 1);
    let kept = id.clone();
    assert_eq!(h.store.run(move |s| s.stored(&kept)).unwrap().0, "INBOX");
    assert_eq!(h.store.run(|s| s.pending("work")).unwrap(), []);
    live.engine.stop();
    println!("real-server: ran archive-trash-undo ({said:?})");
}

fn to_self(server: &common::Server, subject: &str) -> Vec<u8> {
    let me = format!("{}@example.com", server.user);
    let head = format!("From: Spike <{me}>\r\nTo: Spike <{me}>\r\nBcc: {me}\r\nMessage-ID: <{subject}@example.com>");
    format!("{head}\r\nSubject: {subject}\r\n\r\nsent by tuimex to itself\r\n").into_bytes()
}

#[test]
fn a_message_sent_to_oneself_arrives_and_is_in_sent_exactly_once() {
    let Some(server) = common::server("real-server send") else { return };
    if server.smtp.is_none() {
        println!("real-server send: NOT RUN, this server takes no mail to send (TUIMEX_SERVER_SMTPS_PORT)");
        return;
    }
    with_folders(&server, &["Sent"]);
    let h = here("send", &server);
    let live = Live::start(&h, &server);
    let subject = unique("exit-send");
    let draft = live.engine.save_draft(to_self(&server, &subject), None, Origin::Local);
    let held = live.act(Action::Send, std::slice::from_ref(&draft));
    assert_eq!(held, "Sending to 2 recipients in 1s");
    live.wait_for(&|e| matches!(e, Event::Sent { id, .. } if *id == draft));
    // One delivery to the mailbox `To` and `Bcc` both name, and one copy
    // filed.
    until_on_server(&server, &subject, &["INBOX", "Sent"], &[1, 1]);
    for _ in 0..2 {
        live.engine.sync();
        live.wait_for(&|e| matches!(e, Event::Synced { folder, .. } if folder.0 == "Sent"));
    }
    assert_eq!(on_server(&server, &subject, &["INBOX", "Sent"]), [1, 1], "passes later, still once");
    let wanted = subject.clone();
    let here_ =
        h.store.run(move |s| s.query(&Query::parse("in:sent"), Timestamp(i64::MAX / 2), Sort::NewestFirst, None));
    assert_eq!(here_.unwrap().iter().filter(|e| e.subject == wanted).count(), 1, "and once here");
    let outbox = h.store.run(|s| s.query(&Query::parse("in:outbox"), Timestamp(i64::MAX / 2), Sort::NewestFirst, None));
    assert_eq!((outbox.unwrap().len(), h.store.run(|s| s.pending("work")).unwrap().len()), (0, 0));
    // What arrived carries no Bcc header.
    let mut other = server.session();
    other.select("INBOX").unwrap();
    let uids = other.uid_search(format!("SUBJECT {subject}")).unwrap();
    let set = uids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    for arrived in other.uid_fetch(set, "BODY.PEEK[HEADER]").unwrap().iter() {
        let head = String::from_utf8_lossy(arrived.header().unwrap()).to_lowercase();
        assert!(!head.contains("\nbcc:"), "{head}");
    }
    // Answered from here, the message that arrived is marked so on the
    // server. It is here by now: the passes above cover every folder.
    let reply = String::from_utf8(to_self(&server, &format!("re-{subject}"))).unwrap();
    let reply = reply.replace("Message-ID:", &format!("In-Reply-To: <{subject}@example.com>\r\nMessage-ID:"));
    let reply = live.engine.save_draft(reply.into_bytes(), None, Origin::Local);
    live.act(Action::Send, std::slice::from_ref(&reply));
    let answered = live.wait_for(&|e| matches!(e, Event::Sent { id, .. } if *id == reply));
    assert!(matches!(&answered, Event::Sent { answered, .. } if !answered.is_empty()), "{answered:?}");
    let deadline = std::time::Instant::now() + PATIENCE;
    while other.uid_search(format!("ANSWERED SUBJECT {subject}")).unwrap().len() != 1 {
        assert!(std::time::Instant::now() < deadline, "{subject} was never marked \\Answered in the server's INBOX");
        std::thread::sleep(Duration::from_millis(200));
        other.noop().unwrap();
    }
    live.engine.stop();
    println!("real-server: ran send ({subject}: delivered, without its Bcc header, in Sent once, and answered)");
}

#[test]
fn mail_is_not_handed_to_a_server_with_another_certificate_or_one_that_will_not_start_tls() {
    let Some(server) = common::server("real-server send-tls") else { return };
    let Some((_, plain)) = server.smtp else {
        println!("real-server send-tls: NOT RUN, this server takes no mail to send (TUIMEX_SERVER_SMTPS_PORT)");
        return;
    };
    let h = here("send-tls", &server);
    let config = h.root.join("config");
    let subject = unique("exit-send-tls");
    let me = format!("{}@example.com", server.user);
    let letter = Letter { from: me.clone(), to: vec![me], message: to_self(&server, &subject) };
    let refused = |reach: &dyn Connect| match reach.send(&letter) {
        Err(Error::Remote(RemoteError::Connection(why))) => why,
        other => panic!("the message was not held back: {other:?}"),
    };

    let pinned = pin::load(&config, "work").unwrap().unwrap();
    pin::save(&config, "work", b"the certificate this server had last year").unwrap();
    let changed = refused(&server.account(&config));
    let was = pin::fingerprint(b"the certificate this server had last year");
    assert!(changed.contains(&was) && changed.contains(&pin::fingerprint(&pinned)), "{changed}");

    // With no pin the server's own certificate is checked against the
    // system's roots, which did not sign it.
    fs::remove_file(config.join("work.pem")).unwrap();
    let unknown = refused(&server.account(&config));

    // On its plain port this server offers no STARTTLS, and is told
    // nothing more.
    pin::save(&config, "work", &pinned).unwrap();
    let mut reach = server.account(&config);
    reach.smtp = Some(tuimex_engine::Server { port: plain, starttls: true, ..reach.smtp.clone().unwrap() });
    let no_tls = refused(&reach);
    assert!(no_tls.contains("would not start TLS, so nothing was sent to it"), "{no_tls}");

    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(on_server(&server, &subject, &["INBOX"]), [0], "none of the three attempts delivered anything");
    println!("real-server: ran send-tls ({unknown}; {no_tls})");
}

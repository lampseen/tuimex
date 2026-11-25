//! The interface over engines and a store, with a server in memory at the
//! far end: what a person sees and presses, and what the server then holds.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tuimex_core::{Capabilities, Event, Flags, Remote, RemoteWrite, Timestamp};
use tuimex_engine::sync::Policy;
use tuimex_engine::{Account, Connect, Error, Letter};
use tuimex_fixtures::FakeServer;
use tuimex_store::Store;
use tuimex_ui::app::App;
use tuimex_ui::keymap::Key;

use super::{Live, sections};
use crate::calendar::rig;
use crate::config::Config;
use crate::diary::Diary;
use crate::outside::{Opener, Outside};
use crate::term::Backend;
use crate::term::tests::{Glass, Typist, no_calendar};

const PASSWORD: &str = "synthetic-password";
/// Who the two accounts are, as a config would say it.
const CONFIG: &str = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n\n\
                      [[account]]\nname = \"home\"\naddress = \"ada@example.org\"\n";
const CAPABILITIES: Capabilities =
    Capabilities { uidplus: true, r#move: true, condstore: true, special_use: true, idle: true, gmail: false };
/// Long enough that only a hang reaches it.
const PATIENCE: Duration = Duration::from_secs(20);

struct Door(FakeServer);

impl Connect for Door {
    fn connect(&self) -> Result<Box<dyn RemoteWrite>, Error> {
        Ok(Box::new(self.0.connect(PASSWORD)?))
    }

    fn send(&self, _: &Letter) -> Result<(), Error> {
        Ok(())
    }
}

/// Opens nothing, and a test that asks it to fails.
struct Never;

impl Opener for Never {
    fn open(&mut self, target: &str) -> std::io::Result<()> {
        panic!("a test was about to open {target}");
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("live-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

fn message(n: u32, subject: &str) -> Vec<u8> {
    format!(
        "From: Grace Hopper <grace@example.net>\r\nTo: ada@example.org\r\nSubject: {subject}\r\n\
         Message-ID: <m{n}@example.net>\r\nDate: Mon, 23 Nov 2026 10:{n:02}:00 +0000\r\n\r\nThe body of message {n}.\r\n"
    )
    .into_bytes()
}

fn server(subjects: &[(u32, &str)]) -> FakeServer {
    let server = FakeServer::new(PASSWORD, CAPABILITIES);
    server.add_folder("INBOX", &[], 7);
    server.add_folder("Archive", &["\\Archive"], 8);
    server.add_folder("Trash", &["\\Trash"], 9);
    for (n, subject) in subjects {
        server.deliver("INBOX", &message(*n, subject), Flags::default(), Timestamp(1_795_435_200));
    }
    server
}

/// The interface, the backend under it, and the two servers.
struct Session {
    app: App,
    live: Option<Live>,
    work: FakeServer,
    home: FakeServer,
    /// The passes over a folder that changed something, as account and
    /// folder, since this was last emptied.
    synced: Vec<(String, String)>,
    root: PathBuf,
    /// What hands the interface its calendars, where the session has any.
    diary: Option<Diary>,
}

impl Session {
    fn start(name: &str) -> Session {
        Session::over(name, |_, _| {})
    }

    /// As `start`, on servers `prepare` has changed first.
    fn over(name: &str, prepare: impl FnOnce(&FakeServer, &FakeServer)) -> Session {
        Session::ending(name, prepare, "Trash")
    }

    /// As `over`, where `last` is the last folder of the home account's
    /// pass.
    fn ending(name: &str, prepare: impl FnOnce(&FakeServer, &FakeServer), last: &str) -> Session {
        Session::built(name, prepare, last, None)
    }

    /// As `over`, with the calendars of the account `work` kept and
    /// carried out as `kind` says its config would have them.
    fn calendared(name: &str, prepare: impl FnOnce(&FakeServer, &FakeServer), kind: rig::Kind) -> Session {
        Session::built(name, prepare, "Trash", Some(kind))
    }

    fn built(
        name: &str,
        prepare: impl FnOnce(&FakeServer, &FakeServer),
        last: &str,
        calendar: Option<rig::Kind>,
    ) -> Session {
        let root = scratch(name);
        let (work, home) = (server(&[(1, "Quarterly numbers"), (2, "Standup notes")]), server(&[(3, "Cabin weekend")]));
        prepare(&work, &home);
        let account = |name: &str| Account {
            name: name.to_string(),
            roles: Vec::new(),
            policy: Policy::default(),
            identities: vec!["ada@example.org".to_string()],
            send_delay: Duration::from_secs(10),
        };
        let accounts: Vec<(Account, Box<dyn Connect>)> =
            vec![(account("work"), Box::new(Door(work.clone()))), (account("home"), Box::new(Door(home.clone())))];
        let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
        let cache = root.join("cache");
        let outside = move || Outside::new(Box::new(Never), cache.join("downloads"), &cache);
        let sections = sections(&Config::default());
        let (shown, arriving) = std::sync::mpsc::channel();
        let reader = calendar.map(|kind| rig::reader(&root, kind, shown).0);
        let diary = reader.is_some().then(|| Diary::new(arriving, None));
        let me = vec!["ada@example.org".into()];
        let (mailbox, live) = Live::with(store, accounts, sections, me, outside, reader).unwrap();
        assert!(mailbox.mail.is_empty(), "nothing is here before a server has answered");
        let mut app = App::new(crate::theme::choose(None, None, |_| None), mailbox);
        // An answer is given as somebody: the calendar needs who the
        // accounts are, which the mail's own tests leave unsaid.
        if diary.is_some() {
            app.set_up(crate::live::identities(&Config::parse(CONFIG).unwrap()), false);
        }
        let mut session = Session { app, live: Some(live), work, home, synced: Vec::new(), root, diary };
        // The Trash is the last folder of an account's pass.
        session.until_synced(&[("work", "Trash"), ("home", last)], "the first sync of both accounts");
        session.pass();
        session
    }

    /// Passes until a pass over each of `folders` has changed something,
    /// and the account's folders after it are known.
    fn until_synced(&mut self, folders: &[(&str, &str)], what: &str) {
        let deadline = Instant::now() + PATIENCE;
        let seen =
            |s: &Session, (account, folder): &(&str, &str)| s.synced.iter().any(|(a, f)| a == account && f == folder);
        while !folders.iter().all(|folder| seen(self, folder)) {
            assert!(Instant::now() < deadline, "{what} never happened: {:?}", self.synced);
            self.pass();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// What the loop does on a pass with no key.
    fn pass(&mut self) {
        let live = self.live.as_mut().unwrap();
        for ask in self.app.asks() {
            live.ask(ask);
        }
        // As the loop does: what is told, the rows after it, and again.
        loop {
            let (told, rows) = (live.events(), live.refreshes());
            if told.is_empty() && rows.is_empty() {
                break;
            }
            for event in told {
                match &event {
                    Event::Synced { account, folder, changed }
                        if !changed.is_empty() || ["Archive", "Trash"].contains(&folder.0.as_str()) =>
                    {
                        self.synced.push((account.0.clone(), folder.0.clone()));
                    }
                    _ => {}
                }
                self.app.event(event);
            }
            for refresh in rows {
                self.app.refresh(refresh.now, refresh.after, refresh.asked.as_deref(), refresh.rows, refresh.folders);
            }
        }
        if let Some(diary) = self.diary.as_mut() {
            diary.hand(&mut self.app);
        }
    }

    fn screen(&mut self) -> Vec<String> {
        let lines = self.app.lines(120, 40);
        lines
            .iter()
            .map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>().trim_end().to_string())
            .collect()
    }

    /// Passes until the screen is as `wanted` says, with no key pressed.
    fn until(&mut self, what: &str, wanted: impl Fn(&[String]) -> bool) -> Vec<String> {
        let deadline = Instant::now() + PATIENCE;
        loop {
            self.pass();
            let screen = self.screen();
            if wanted(&screen) {
                return screen;
            }
            assert!(Instant::now() < deadline, "{what} never happened:\n{}", screen.join("\n"));
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn press(&mut self, keys: &str) {
        for c in keys.chars() {
            self.app.key(match c {
                '\r' => Key::Enter,
                '\t' => Key::Tab,
                '\x1b' => Key::Esc,
                other => Key::Char(other),
            });
        }
        self.pass();
    }

    /// The row the cursor is on.
    fn cursor(&mut self) -> String {
        self.screen().into_iter().find(|line| line.starts_with('▌')).unwrap_or_default()
    }

    fn stop(mut self) {
        self.live.take().unwrap().stop(|accounts| panic!("nothing was being sent, and it waited for {accounts:?}"));
    }
}

/// The subjects a server holds in a folder, once it holds `count` of them.
fn there(server: &FakeServer, folder: &str, count: usize) -> Vec<u32> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let mut remote = server.connect(PASSWORD).unwrap();
        // A folder that is not there yet holds nothing yet.
        let uids = remote.select(folder).and_then(|_| remote.uids()).unwrap_or_default();
        if uids.len() == count {
            return uids;
        }
        assert!(Instant::now() < deadline, "{folder} never held {count} messages: {uids:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn shows(text: &'static str) -> impl Fn(&[String]) -> bool {
    move |screen| screen.iter().any(|line| line.contains(text))
}

#[test]
fn two_accounts_are_one_list_that_follows_their_servers_and_changes_them() {
    let mut s = Session::start("two-accounts");
    let all =
        |screen: &[String]| ["Quarterly numbers", "Standup notes", "Cabin weekend"].iter().all(|t| shows(t)(screen));
    let first = s.until("the first sync of both accounts", all);
    assert!(first[0].starts_with("  Inbox 3 "), "{}", first[0]);
    s.press("gg");
    assert!(s.cursor().contains("Cabin weekend"), "newest first, whichever account it is in: {}", s.cursor());

    // A message arriving on a server appears with no key pressed, and the
    // cursor stays on the message it was on.
    s.work.deliver("INBOX", &message(30, "Just arrived"), Flags::default(), Timestamp(1_795_435_300));
    s.until("new mail", shows("Just arrived"));
    assert!(s.cursor().contains("Cabin weekend"), "{}", s.cursor());

    // `e` archives on the server, and `u` brings it back there.
    s.press("e");
    assert!(!shows("Cabin weekend")(&s.screen()), "it leaves the list at once");
    assert_eq!((there(&s.home, "Archive", 1).len(), there(&s.home, "INBOX", 0).len()), (1, 0));
    s.press("u");
    assert!(shows("Cabin weekend")(&s.screen()), "and is back at once");
    assert_eq!((there(&s.home, "INBOX", 1).len(), there(&s.home, "Archive", 0).len()), (1, 0));
    s.until("the engines to settle", |_| true);

    // A refusal, which arrives when the server is reached, puts the row
    // back and is said.
    s.work.refuse_moves();
    s.press(":account work\rd");
    assert!(!shows("Just arrived")(&s.screen()));
    let refused = s.until("the refusal", shows("E :trash: work refused it"));
    let said = refused.last().unwrap();
    assert!(said.ends_with("the move was refused · 1 message back in INBOX"), "{said}");
    assert!(shows("Just arrived")(&refused), "the row is back");
    assert_eq!(there(&s.work, "INBOX", 3).len(), 3);
    s.press("u");
    assert!(s.screen().last().unwrap().contains("E undo: nothing to undo"), "a refused action is not there for u");
    s.stop();
}

/// A refusal puts its row back on screen, and the rows the store read
/// before the refusal still say the row is gone. A loop that found both
/// waiting took the refusal first and those rows after it, and the row
/// was gone again until the next rows came: "the row is back" failed about
/// one loaded run in some hundreds. What is told after rows now waits for
/// the loop to have taken them.
#[test]
fn a_refusal_found_waiting_behind_rows_read_before_it_is_taken_after_them() {
    let ((jobs, _queue), (worker, news)) = (std::sync::mpsc::channel(), std::sync::mpsc::channel());
    let mut live = Live {
        jobs,
        news,
        worker: None,
        read: Vec::new(),
        rows: Vec::new(),
        next: None,
        failed: Vec::new(),
        asked: Vec::new(),
    };
    let rows = |after| {
        let (now, folders) = (Timestamp(0), Vec::new());
        super::News::Rows(crate::term::Refresh { now, after, asked: None, rows: Vec::new(), folders })
    };
    let refusal = || Event::ActionFailed { serial: 3, reason: "the move was refused".into(), hint: String::new() };
    let synced = || Event::Synced {
        account: tuimex_core::AccountId("work".into()),
        folder: tuimex_core::FolderId("INBOX".into()),
        changed: Vec::new(),
    };
    for news in [super::News::Told(synced()), rows(3), rows(3), super::News::Told(refusal()), rows(4)] {
        worker.send(news).unwrap();
    }
    let mut turn = || (live.events(), live.refreshes().iter().map(|rows| rows.after).collect::<Vec<_>>());
    assert_eq!(turn(), (vec![synced()], vec![3, 3]), "what was told first, and the rows read before the refusal");
    assert_eq!(turn(), (vec![refusal()], vec![4]), "then the refusal, and the rows read after it");
    assert_eq!(turn(), (Vec::new(), Vec::new()));
}

#[test]
fn a_message_is_read_through_the_engine_and_the_trash_is_emptied_on_the_server() {
    let mut s = Session::start("read-and-empty");
    s.until("the first sync", shows("Cabin weekend"));
    s.press("gg\r");
    let deadline = Instant::now() + PATIENCE;
    while s.live.as_ref().unwrap().read.is_empty() {
        assert!(Instant::now() < deadline, "the body never came");
        s.pass();
    }
    assert!(shows("The body of message 3.")(&s.screen()));
    let live = s.live.as_mut().unwrap();
    let read = live.read[0].id.clone();
    let (names, body) = live.shown(&read).expect("what was read is there for the agent's question");
    assert_eq!((names, body.text.trim()), (vec!["Grace Hopper".to_string()], "The body of message 3."));
    assert_eq!(live.picture(&read, 0), None, "it has no attachment");

    s.synced.clear();
    s.press("\x1bd");
    there(&s.home, "Trash", 1);
    // The pass that moved it ends once it has its number in the Trash.
    s.until_synced(&[("home", "INBOX")], "the move to be numbered");
    s.pass();
    s.press(":empty-trash home\r");
    assert!(shows("Delete 1 message in home's Trash for good?")(&s.screen()), "{:#?}", s.screen());
    s.press("y");
    s.until("the count the engine made", shows("Deleted 1 message from home's Trash"));
    there(&s.home, "Trash", 0);
    let log = s.home.take_log();
    assert!(log.iter().any(|line| line.starts_with("uid-expunge")), "{log:?}");
    assert!(!log.iter().any(|line| line.starts_with("expunge")), "never a bare one (ADR-014): {log:?}");
    s.stop();
}

/// PLAN §4.1, end to end: the question, and after a yes a folder on the
/// server that the message is in, and out of again with `u`.
#[test]
fn an_archive_is_made_on_the_server_after_a_yes_and_never_before() {
    let mut s = Session::over("make-archive", |_, home| home.remove_folder("Archive"));
    s.until("the first sync", shows("Cabin weekend"));
    s.press("gge");
    assert!(shows("home has no Archive folder. Create one on its server?")(&s.screen()), "{:#?}", s.screen());
    s.press("n");
    s.until("the engines to settle", |_| true);
    assert!(shows("Cabin weekend")(&s.screen()));
    assert!(!s.home.take_log().iter().any(|line| line.starts_with("create")), "a no makes nothing");
    s.press("ey");
    assert!(!shows("Cabin weekend")(&s.screen()), "archived at once");
    assert_eq!((there(&s.home, "Archive", 1).len(), there(&s.home, "INBOX", 0).len()), (1, 0));
    s.press("u");
    assert_eq!((there(&s.home, "INBOX", 1).len(), there(&s.home, "Archive", 0).len()), (1, 0));
    s.until("the row to be back", shows("Cabin weekend"));
    s.stop();
}

/// ADR-047, end to end: the same for a Trash.
#[test]
fn a_trash_is_made_on_the_server_after_a_yes_and_never_before() {
    let mut s = Session::ending("make-trash", |_, home| home.remove_folder("Trash"), "Archive");
    s.until("the first sync", shows("Cabin weekend"));
    s.press("ggd");
    assert!(shows("home has no Trash folder. Create one on its server?")(&s.screen()), "{:#?}", s.screen());
    s.press("n");
    s.until("the engines to settle", |_| true);
    assert!(shows("Cabin weekend")(&s.screen()));
    assert!(!s.home.take_log().iter().any(|line| line.starts_with("create")), "a no makes nothing");
    s.press("dy");
    assert!(!shows("Cabin weekend")(&s.screen()), "trashed at once");
    assert_eq!((there(&s.home, "Trash", 1).len(), there(&s.home, "INBOX", 0).len()), (1, 0));
    s.press("u");
    assert_eq!((there(&s.home, "INBOX", 1).len(), there(&s.home, "Trash", 0).len()), (1, 0));
    s.until("the row to be back", shows("Cabin weekend"));
    s.stop();
}

/// The draft is what the store holds of it: bytes built once, with their
/// `Bcc`, read back by `tuimex-content` into the row compose opens from.
#[test]
fn a_draft_saved_with_a_blind_copy_is_stored_with_it_and_opens_again_with_it() {
    let mut s = Session::start("draft-bcc");
    let work = tuimex_core::AccountId("work".into());
    let address = tuimex_core::Address { name: None, email: "ada@example.org".into() };
    s.app.set_up(vec![tuimex_core::Identity { account: work, address }], false);
    s.until("the first sync", shows("Standup notes"));
    s.press(":account work\rcgrace@example.net\t\taudit@example.org\tQuiet copy");
    s.app.key(Key::Ctrl('d'));
    s.pass();
    assert!(shows("Draft saved")(&s.screen()), "{:#?}", s.screen());
    s.app.key(Key::Esc);
    s.press(":section Drafts\r");
    s.until("the stored draft", shows("Quiet copy"));
    // What is shown now is the store's row, and no longer what compose
    // held: the reading that followed the save has replaced it.
    s.until("the engines to settle", |_| true);
    s.press("r");
    let reopened = s.screen();
    assert!(reopened.iter().any(|row| row.trim() == "Bcc      audit@example.org"), "{reopened:#?}");
    let root = s.root.clone();
    s.stop();
    let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
    let drafts: Vec<_> =
        store.listed(None).unwrap().into_iter().filter(|m| m.envelope.subject == "Quiet copy").collect();
    let [draft] = drafts.as_slice() else { panic!("{drafts:#?}") };
    assert_eq!(draft.envelope.bcc.iter().map(|a| a.email.as_str()).collect::<Vec<_>>(), ["audit@example.org"]);
}

/// `^d` and then leaving at once: the store's thread is handed the draft
/// and not waited for, so stopping waits for it. Over and over, because
/// what is being caught is a thread that was sometimes fast enough.
#[test]
fn a_draft_saved_a_moment_before_leaving_is_in_the_store_every_time() {
    for round in 0..12 {
        let mut s = Session::start(&format!("draft-then-stop-{round}"));
        let work = tuimex_core::AccountId("work".into());
        let address = tuimex_core::Address { name: None, email: "ada@example.org".into() };
        s.app.set_up(vec![tuimex_core::Identity { account: work, address }], false);
        s.press(":account work\rcgrace@example.net\t\t\tSaved on the way out");
        s.app.key(Key::Ctrl('d'));
        s.pass();
        let root = s.root.clone();
        s.stop();
        let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
        let saved = store.listed(None).unwrap().into_iter().filter(|m| m.envelope.subject == "Saved on the way out");
        assert_eq!(saved.count(), 1, "round {round}");
    }
}

/// Enter on a draft over a real store: compose opens on what was saved,
/// read back from the stored message and not from the row's opening of it.
#[test]
fn enter_on_a_stored_draft_opens_compose_with_its_fields_files_and_whole_body() {
    let mut s = Session::start("draft-open");
    let work = tuimex_core::AccountId("work".into());
    let address = tuimex_core::Address { name: None, email: "ada@example.org".into() };
    s.app.set_up(vec![tuimex_core::Identity { account: work, address }], false);
    s.until("the first sync", shows("Standup notes"));
    s.press(":account work\rcgrace@example.net\tcarol@example.net\taudit@example.org\tLong plan");
    // Longer than the opening a row is given, and ending on a line of its own.
    let body: String = (1..=40).map(|n| format!("Step {n} of the plan is written out in full here.\n\n")).collect();
    let body = format!("{body}The last line of the draft.");
    s.app.event(Event::Edited { text: Ok(body.clone()) });
    let notes = s.root.join("notes.txt");
    std::fs::write(&notes, "attached notes\n").unwrap();
    let file = tuimex_core::Attached { name: "notes.txt".into(), size: 15, path: Some(notes) };
    s.app.event(Event::Attached { file: Ok(file) });
    s.app.key(Key::Ctrl('d'));
    s.pass();
    s.app.key(Key::Esc);
    s.press(":section Drafts\r");
    // The store's row says how many files it holds, which compose's did not.
    s.until("the stored draft", |screen| screen.iter().any(|row| row.contains("Long plan") && row.contains("1 file")));
    assert!(shows("Enter edit")(&s.screen()), "{:#?}", s.screen());
    s.press("\r");
    let opened = s.until("compose on the draft", shows("Bcc      audit@example.org"));
    let has = |text: &str| opened.iter().any(|row| row.trim() == text);
    assert!(has("To       grace@example.net") && has("Cc       carol@example.net"), "{opened:#?}");
    assert!(has("Subject  Long plan") && opened.iter().any(|row| row.contains("Files    notes.txt")), "{opened:#?}");
    let tall = s.app.lines(120, 200);
    let drawn: Vec<String> = tall.iter().map(|line| line.iter().map(|s| s.text.as_str()).collect()).collect();
    let line = |text: &str| drawn.iter().any(|row| row.trim() == text);
    assert!(line("Step 1 of the plan is written out in full here."), "{drawn:#?}");
    assert!(
        line("Step 40 of the plan is written out in full here.") && line("The last line of the draft."),
        "{drawn:#?}"
    );
    s.stop();
}

/// `A` then `r` on a row, over the real backend and the desk that runs the
/// agent command: compose opens on the reply, the configured command is
/// handed the thread, and what it prints is the body. The command here is
/// a shell line; no agent's own program is run by any test.
#[test]
fn capital_a_then_r_on_a_row_writes_the_reply_with_the_configured_agent_command() {
    let mut s = Session::start("agent-reply");
    let work = tuimex_core::AccountId("work".into());
    let address = tuimex_core::Address { name: None, email: "ada@example.org".into() };
    s.app.set_up(vec![tuimex_core::Identity { account: work, address }], true);
    s.until("the first sync", shows("Standup notes"));
    let script =
        "grep -q 'The body of message 2' && echo 'Thanks, I have the notes.' || echo 'the thread was not handed over'";
    let agent = tuimex_engine::agent::Agent {
        command: ["sh", "-c", script].map(String::from).to_vec(),
        timeout: Duration::from_secs(20),
        dir: s.root.join("cache/agent"),
    };
    s.press(":account work\r");
    assert!(s.screen()[3].contains("Standup notes"), "{:#?}", s.screen());
    let mut desk = crate::desk::Desk::new(s.live.take().unwrap(), Some(agent), &s.root.join("cache"));
    s.app.key(Key::Char('A'));
    assert!(shows("r   write a reply")(&s.screen()) && s.app.asks().is_empty(), "the menu sends nothing");
    s.app.key(Key::Char('r'));
    let deadline = Instant::now() + PATIENCE;
    let written = loop {
        s.app.asks().into_iter().for_each(|ask| desk.ask(ask));
        desk.events().into_iter().for_each(|event| s.app.event(event));
        for refresh in desk.refreshes() {
            s.app.refresh(refresh.now, refresh.after, refresh.asked.as_deref(), refresh.rows, refresh.folders);
        }
        let screen = s.screen();
        if shows("Reply written")(&screen) {
            break screen;
        }
        assert!(Instant::now() < deadline, "the agent never answered:\n{}", screen.join("\n"));
        std::thread::sleep(Duration::from_millis(5));
    };
    let has = |text: &str| written.iter().any(|row| row.trim() == text);
    assert!(has("To       Grace Hopper <grace@example.net>") && has("Subject  Re: Standup notes"), "{written:#?}");
    assert!(has("Thanks, I have the notes.") && shows("Written by an agent")(&written), "{written:#?}");
    desk.into_inner().stop(|accounts| panic!("nothing was being sent, and it waited for {accounts:?}"));
}

/// `/` and Enter are answered by the store, from every folder of both
/// accounts and through the bodies, which no row holds until a message is
/// read.
#[test]
fn a_search_finds_a_word_of_a_body_in_every_account_and_folder() {
    let mut s = Session::start("search");
    s.until("the first sync", shows("Cabin weekend"));
    // One of them is filed away first: a search is not of the inboxes.
    s.press("gge");
    s.until("the engines to settle", |_| true);
    s.press("/body\r");
    let found = s.until("the search", |screen| {
        screen.iter().filter(|row| row.contains("Hopper") && row.contains("The body of message")).count() == 3
    });
    assert!(found[0].ends_with("Search body"), "{}", found[0]);
    let row = |subject: &str| found.iter().find(|row| row.contains(subject)).unwrap_or_else(|| panic!("{found:#?}"));
    assert!(row("Cabin weekend").contains("home   Grace Hopper"), "{}", row("Cabin weekend"));
    assert!(row("Cabin weekend").contains("Cabin weekend  The body of message 3."), "the words around the match");
    assert!(row("Standup notes").contains("work   Grace Hopper"));
    // The folder's name is in the row's words, before a date that is a time
    // of day only on the day the mail is dated.
    let archived = found.iter().filter(|row| row.contains("Hopper") && row.contains(" Archive  ")).count();
    assert_eq!(archived, 1, "the one that was archived says where it is: {found:#?}");

    s.press("\x1b/zebra\r");
    let none = s.until("the empty search", shows("Nothing matches \"zebra\"."));
    assert!(shows("Searched 3 messages in 2 accounts.")(&none), "{none:#?}");
    s.press("\x1b");
    assert!(shows("Cabin weekend")(&s.screen()) || shows("Standup notes")(&s.screen()), "Esc is back in the Inbox");
    s.stop();
}

/// A key held on one row asks twice of one message before the store has
/// answered once. The rows owed for both asks are read together, and the
/// message is one row of that reading and one message of the list.
#[test]
fn asks_that_were_waiting_together_are_answered_with_each_message_once() {
    let mut s = Session::start("asked-together");
    s.until("the first sync of both accounts", |screen| screen[0].starts_with("  Inbox 3 "));
    s.press("gg");
    for _ in 0..20 {
        "ssss".chars().for_each(|key| s.app.key(Key::Char(key)));
        let live = s.live.as_mut().unwrap();
        let asks = s.app.asks();
        let last = asks.last().unwrap().serial;
        asks.into_iter().for_each(|ask| live.ask(ask));
        let deadline = Instant::now() + PATIENCE;
        let mut answered = 0;
        while answered < last {
            assert!(Instant::now() < deadline, "the store never answered");
            live.events().into_iter().for_each(|event| s.app.event(event));
            for refresh in live.refreshes() {
                let mut ids: Vec<_> = refresh.rows.iter().map(|row| row.envelope.id.clone()).collect();
                ids.dedup();
                assert_eq!(ids.len(), refresh.rows.len(), "a message is read once: {ids:?}");
                answered = answered.max(refresh.after);
                s.app.refresh(refresh.now, refresh.after, refresh.asked.as_deref(), refresh.rows, refresh.folders);
            }
        }
        let screen = s.screen();
        assert!(screen[38].trim().starts_with("1 of 3 "), "three messages, as there were: {}", screen[38]);
    }
    s.stop();
}

/// A backend that counts what it hands the loop, and notes the end of the
/// last folder's first pass.
struct Counted<'a> {
    live: &'a mut Live,
    refreshes: usize,
    rows: usize,
    synced: &'a mut bool,
}

impl Backend for Counted<'_> {
    fn ask(&mut self, ask: tuimex_core::Ask) {
        self.live.ask(ask);
    }

    fn events(&mut self) -> Vec<Event> {
        let events = self.live.events();
        *self.synced |= events.iter().any(|e| matches!(e, Event::Synced { folder, .. } if folder.0 == LAST));
        events
    }

    fn working(&self) -> bool {
        self.live.working()
    }

    fn refreshes(&mut self) -> Vec<crate::term::Refresh> {
        let refreshes = self.live.refreshes();
        self.refreshes += refreshes.len();
        self.rows += refreshes.iter().map(|refresh| refresh.rows.len()).sum::<usize>();
        refreshes
    }

    fn now(&self) -> Option<Timestamp> {
        self.live.now()
    }
}

/// The last folder of the account that is synced while keys are held.
const LAST: &str = "Folder 9";

/// Holds `key` down for three seconds at `rate` a second, through the
/// loop a terminal runs, and prints how far behind the screen was: at its
/// worst, and for the last key, which is how long it went on moving after
/// the key was let go.
fn hold(app: &mut App, live: &mut Live, synced: &mut bool, what: &str, key: char, rate: u32) -> Duration {
    use ratatui::backend::CrosstermBackend;
    use ratatui::{Terminal, TerminalOptions, Viewport};
    let (send, keys) = std::sync::mpsc::channel();
    let typing = std::thread::spawn(move || {
        let started = Instant::now();
        for n in 1..=rate * 3 {
            send.send((key, Instant::now())).unwrap();
            std::thread::sleep((started + Duration::from_secs(1) * n / rate).saturating_duration_since(Instant::now()));
        }
        std::thread::sleep(Duration::from_millis(400));
        // `:q` and Enter leave: no key does.
        ":q\r".chars().for_each(|key| send.send((key, Instant::now())).unwrap());
    });
    let frames = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let viewport = Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 120, 40));
    let glass = CrosstermBackend::new(Glass(frames.clone()));
    let mut terminal = Terminal::with_options(glass, TerminalOptions { viewport }).unwrap();
    let mut typist = Typist { keys, next: None, read: Vec::new() };
    let mut counted = Counted { live, refreshes: 0, rows: 0, synced };
    crate::term::event_loop(&mut terminal, &mut typist, app, &mut counted, &mut no_calendar()).unwrap();
    typing.join().unwrap();
    app.quit = false;
    let frames = frames.borrow();
    typist.read.truncate(typist.read.len().saturating_sub(3));
    let behind = |(sent, read): &(Instant, Instant)| {
        frames.iter().find(|frame| *frame >= read).expect("a frame follows every key").duration_since(*sent)
    };
    let (worst, last) = (typist.read.iter().map(behind).max().unwrap(), behind(typist.read.last().unwrap()));
    println!(
        "{what}, {key} at {rate}/s for 3 s: worst {worst:?} behind, still moving {last:?} after the last key; \
         {} frames, {} refreshes of {} rows",
        frames.len(),
        counted.refreshes,
        counted.rows
    );
    last
}

/// The megabytes of memory this process holds.
fn resident() -> usize {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap();
    statm.split(' ').nth(1).unwrap().parse::<usize>().unwrap() * 4096 / (1 << 20)
}

/// A mailbox of `count` messages on one server, as `large` in the
/// interface's own tests has it: a fifth in the Inbox and the rest in the
/// Archive, every fourth a reply to the one before, one in 97 flagged. A
/// second account is then synced for the first time, in ten folders of a
/// thousand, while a key is held.
fn held_keys(count: u32) {
    let root = scratch(&format!("held-{count}"));
    let now = chrono::Utc::now().timestamp();
    let mail = |n: u32, from: &str| {
        let date = chrono::DateTime::from_timestamp(now - i64::from(n) * 600, 0).unwrap().to_rfc2822();
        let answers = if n % 4 == 1 { format!("References: <{from}{}@example.net>\r\n", n - 1) } else { String::new() };
        let raw = format!(
            "From: Grace Hopper <grace@example.net>\r\nTo: ada@example.org\r\nSubject: Subject number {n} of the long \
             mailbox\r\nMessage-ID: <{from}{n}@example.net>\r\n{answers}Date: {date}\r\n\r\nThe opening of message \
             {n}, as the index keeps it.\r\n"
        );
        let flags = Flags { seen: !n.is_multiple_of(7), flagged: n.is_multiple_of(97), ..Flags::default() };
        (raw.into_bytes(), flags, Timestamp(now - i64::from(n) * 600))
    };
    let work = server(&[]);
    for n in 0..count {
        let (raw, flags, arrived) = mail(n, "w");
        work.deliver(if n % 5 == 0 { "INBOX" } else { "Archive" }, &raw, flags, arrived);
    }
    let home = server(&[]);
    let folders: Vec<String> = (0..10).map(|n| format!("Folder {n}")).collect();
    for (at, folder) in folders.iter().enumerate() {
        home.add_folder(folder, &[], 20 + at as u32);
        for n in 0..1000 {
            let (raw, flags, arrived) = mail(at as u32 * 1000 + n, "h");
            home.deliver(folder, &raw, flags, arrived);
        }
    }
    let account = |name: &str, folders: &[String]| Account {
        name: name.to_string(),
        roles: Vec::new(),
        policy: Policy { folders: folders.to_vec(), ..Policy::default() },
        identities: vec!["ada@example.org".to_string()],
        send_delay: Duration::from_secs(10),
    };
    let open = |accounts: Vec<(Account, Box<dyn Connect>)>| {
        let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
        let cache = root.join("cache");
        let outside = move || Outside::new(Box::new(Never), cache.join("downloads"), &cache);
        Live::over(store, accounts, sections(&Config::default()), vec!["ada@example.org".into()], outside).unwrap()
    };

    let started = Instant::now();
    let (_, mut live) = open(vec![(account("work", &[]), Box::new(Door(work.clone())))]);
    let synced = |live: &mut Live| {
        live.refreshes();
        live.events().iter().any(|e| matches!(e, Event::Synced { folder, .. } if folder.0 == "Trash"))
    };
    while !synced(&mut live) {
        std::thread::sleep(Duration::from_millis(20));
    }
    println!("{count} messages: first sync from a server in memory {:?}", started.elapsed());
    live.stop(|_| {});

    let (before, started) = (resident(), Instant::now());
    let (mailbox, mut live) = open(vec![
        (account("work", &[]), Box::new(Door(work.clone()))),
        (account("home", &folders), Box::new(Door(home.clone()))),
    ]);
    assert_eq!(mailbox.mail.len(), count as usize);
    let mut app = App::new(crate::theme::choose(None, None, |_| None), mailbox);
    println!(
        "start: the store opened and every row read in {:?}, {} MB more held",
        started.elapsed(),
        resident() - before
    );
    app.lines(120, 40);
    println!("start: the first frame {:?} after it began", started.elapsed());

    let (mut overran, mut synced) = (Vec::new(), false);
    for state in ["during a first sync", "idle"] {
        while state == "idle" && !synced {
            let mut counted = Counted { live: &mut live, refreshes: 0, rows: 0, synced: &mut synced };
            counted.events().into_iter().for_each(|event| app.event(event));
            for r in counted.refreshes() {
                app.refresh(r.now, r.after, r.asked.as_deref(), r.rows, r.folders);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        for (section, key) in [("Archive", 'j'), ("Flagged", 's')] {
            for rate in [30, 60] {
                assert!(app.section(section));
                assert!(state == "idle" || !synced, "the first sync was over before the keys were");
                let what = format!("{count} messages, {state}, in {section}");
                overran.push(hold(&mut app, &mut live, &mut synced, &what, key, rate));
            }
        }
    }
    live.stop(|_| {});
    std::fs::remove_dir_all(&root).unwrap();
    assert!(overran.iter().all(|last| last.as_millis() < 100), "{overran:?}");
}

/// Where start-up goes over a long mailbox on disk: with a current index,
/// and with none, which is the rebuild an index of an older version gets.
/// Run by hand, optimised:
/// `cargo test --release -p tuimex --bin tuimex start_up -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, which a loaded machine would fail"]
fn start_up_over_a_hundred_and_fifty_thousand_messages_with_an_index_and_with_one_to_rebuild() {
    const COUNT: u32 = 150_000;
    let root = scratch("start-up");
    let (data, cache) = (root.join("data"), root.join("cache"));
    let store = Store::open(&data, &cache, tuimex_engine::parse).unwrap();
    for (folder, role) in [("INBOX", tuimex_core::Role::Inbox), ("Archive", tuimex_core::Role::Archive)] {
        store.ensure_folder("work", folder, role).unwrap();
    }
    let body = "A line of the message, of the length most lines of mail are.\r\n".repeat(30);
    for n in 0..COUNT {
        let raw = format!(
            "From: Grace Hopper <grace@example.net>\r\nTo: ada@example.org\r\nSubject: Subject number {n} of the long \
             mailbox\r\nMessage-ID: <w{n}@example.net>\r\nDate: Mon, 23 Nov 2026 10:00:00 +0000\r\n\r\n{body}"
        );
        let flags = Flags { seen: true, ..Flags::default() };
        let delivery = tuimex_store::maildir::Delivery {
            key: format!("k{n}"),
            uid: Some(n + 1),
            origin: tuimex_core::Origin::Server,
            flags,
        };
        store.deliver("work", if n % 5 == 0 { "INBOX" } else { "Archive" }, &delivery, raw.as_bytes()).unwrap();
    }
    drop(store);
    let timed = |what: &str| {
        let started = Instant::now();
        let mut steps = 0;
        let store = Store::open_saying(&data, &cache, tuimex_engine::parse, &mut |_, _| steps += 1).unwrap();
        let opened = started.elapsed();
        let rows = store.listed(None).unwrap().len();
        println!(
            "{COUNT} messages, {what}: the store opened in {opened:?} ({steps} steps of progress said), {rows} rows read by {:?}",
            started.elapsed()
        );
    };
    timed("a current index");
    for suffix in ["", "-wal", "-shm"] {
        // One that is not there was never made.
        let _ = std::fs::remove_file(cache.join(format!("index.sqlite{suffix}")));
    }
    timed("no index, which is rebuilt as one of an older version is");
    timed("the index that rebuild left");
}

/// Run by hand, optimised, for the numbers PLAN §9 quotes:
/// `cargo test --release -p tuimex --bin tuimex held_down -- --ignored --nocapture --test-threads 1`.
#[test]
#[ignore = "a measurement, which a loaded machine would fail"]
fn a_key_held_down_over_fifty_thousand_messages_stops_when_it_is_let_go() {
    held_keys(50_000);
}

#[test]
#[ignore = "a measurement, which a loaded machine would fail"]
fn a_key_held_down_over_a_hundred_and_fifty_thousand_messages_stops_when_it_is_let_go() {
    held_keys(150_000);
}

/// The calendar's files of the account `work`, as text.
fn kept(root: &Path) -> Vec<String> {
    let store = tuimex_store::calendar::Calendars::open(&root.join("data"), &root.join("cache")).unwrap();
    store.held().unwrap().into_iter().filter(|held| held.calendar.is_none()).map(|held| held.text).collect()
}

/// The whole path of an answer: an invitation that is in mail alone is on
/// the calendar, `i a` answers it there, the reply waits in the Outbox,
/// the event is in the account's local calendar, and `u` takes both back.
#[test]
fn an_invitation_in_mail_is_answered_in_the_calendar_and_u_takes_the_reply_and_the_entry_back() {
    // Two days from now, so that it is in reach of `I` whenever this runs.
    let day = (chrono::Utc::now() + chrono::Duration::days(2)).format("%Y%m%d").to_string();
    let part = rig::invitation("planning@example.com", 0, Some("REQUEST"), "").replace("20261126", &day);
    let invitation = rig::mail(1, "Lars Holm <lars@example.com>", "REQUEST", &part);
    let deliver = |work: &FakeServer, _: &FakeServer| {
        work.deliver("INBOX", &invitation, Flags::default(), Timestamp(1_795_435_200));
    };
    let mut s = Session::calendared("calendar-answer", deliver, rig::Kind::None);
    s.until("the invitation's mail", shows("Invitation 1"));
    // `I` goes to the day of the next invitation waiting, and Enter into
    // that day, where its first event is selected.
    s.press("gcI\r");
    let waiting = s.until("the invitation on the calendar", shows("needs answer"));
    assert!(shows("Planning")(&waiting), "{waiting:#?}");
    assert!(kept(&s.root).is_empty(), "it is shown from the mail, and kept nowhere yet");

    s.press("ia");
    let said = s.screen().pop().unwrap();
    assert!(said.contains("Accepted Planning") && said.contains("in work, local"), "{said}");
    let accepted = s.until("the calendar read after the answer", |screen| !shows("needs answer")(screen));
    assert!(shows("you yes")(&accepted) && shows("local, only in tuimex")(&accepted), "{accepted:#?}");
    // The screen is changed when the key is pressed and the file is
    // written by the worker after it, so the file is waited for.
    let root = s.root.clone();
    s.until("the answer in the calendar's file", |_| kept(&root).len() == 1);
    let files = kept(&s.root);
    assert!(files.len() == 1 && files[0].contains("PARTSTAT=ACCEPTED"), "{files:?}");
    // The reply is mail like any other while it waits: a row of the
    // Outbox, in the list `gm` returns to.
    s.press("gm");
    s.until("the Outbox among the sections", |screen| screen[0].ends_with("Archive   Outbox"));
    s.press("llllll");
    let held = s.until("the reply in the Outbox", shows("Accepted: Planning"));
    assert!(s.cursor().contains("Accepted: Planning"), "it is the Outbox's one row: {held:#?}");
    s.press("hhhhhh");

    s.press("gcu");
    assert!(s.screen().pop().unwrap().contains("Undone"), "{:?}", s.screen().pop());
    s.until("the invitation waiting again", shows("needs answer"));
    s.until("the calendar's file gone", |_| kept(&root).is_empty());
    assert!(kept(&s.root).is_empty(), "the event is in no calendar");
    s.press("gm");
    let back = s.until("the Outbox without the reply", |screen| !shows("Outbox")(screen));
    assert!(!shows("Accepted: Planning")(&back), "it is in no folder, Drafts included: {back:#?}");
    s.stop();
}

/// The reader's card answers once the message has been read: the worker
/// hands the interface the event the message carries, and `i a` on the
/// card is the same ask as in the calendar, threaded under that message.
#[test]
fn an_invitation_s_card_in_the_reader_answers_once_its_message_has_been_read() {
    let part = rig::invitation("planning@example.com", 0, Some("REQUEST"), "");
    let invitation = rig::mail(1, "Lars Holm <lars@example.com>", "REQUEST", &part);
    let deliver = |work: &FakeServer, _: &FakeServer| {
        work.deliver("INBOX", &invitation, Flags::default(), Timestamp(1_795_435_200));
    };
    let mut s = Session::calendared("calendar-card", deliver, rig::Kind::None);
    s.until("the invitation's mail", shows("Invitation 1"));
    // The filter puts the cursor on its one match, and Esc leaves it there.
    s.press("/Invitation 1");
    s.press("\x1b\r");
    s.until("the card", shows("Planning"));
    let deadline = Instant::now() + PATIENCE;
    loop {
        s.press("ia");
        let said = s.screen().pop().unwrap();
        if said.contains("Accepted Planning") {
            assert!(said.contains("in work, local"), "{said}");
            break;
        }
        // Until the event is handed over the card says it cannot answer.
        assert!(said.contains("could not be read"), "{said}");
        assert!(Instant::now() < deadline, "the card never learned its event: {said}");
        std::thread::sleep(Duration::from_millis(5));
    }
    let deadline = Instant::now() + PATIENCE;
    while kept(&s.root).is_empty() {
        assert!(Instant::now() < deadline, "the answer was never kept");
        s.pass();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(kept(&s.root)[0].contains("PARTSTAT=ACCEPTED"));
    // `u` in mail takes back what was answered from mail.
    s.press("u");
    assert!(s.screen().pop().unwrap().contains("Undone"), "{:?}", s.screen().pop());
    let deadline = Instant::now() + PATIENCE;
    while !kept(&s.root).is_empty() {
        assert!(Instant::now() < deadline, "the answer was never taken back");
        s.pass();
        std::thread::sleep(Duration::from_millis(5));
    }
    s.stop();
}

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};

use tuimex_core::calendar::{Guest, Moment, Said};
use tuimex_core::{Address, Identity};
use tuimex_ui::calendar::Shelf;
use tuimex_ui::keymap::Key;

use super::*;
use crate::notify::{self, Notifier, SWITCH_FILE, Settings, Via};

/// A directory of this test's own under the build directory, never the
/// system temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("diary-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

/// Each title it was given, and whether to fail. No test raises a real
/// notification.
#[derive(Clone, Default)]
struct Recorder {
    titles: Rc<RefCell<Vec<String>>>,
    fails: bool,
}

impl Notifier for Recorder {
    fn notify(&mut self, title: &str) -> Result<(), String> {
        self.titles.borrow_mut().push(title.to_string());
        if self.fails { Err("notify-send could not be started".into()) } else { Ok(()) }
    }
}

/// The clock every test here is set from, and the lead reminders are
/// given.
const NOON: i64 = 1_795_435_200;
const MINUTE: i64 = 60;
const ON: Settings = Settings { events: true, lead: 10 * MINUTE, via: Via::None };

const WORK: &str = "robin.vale@example.org";
const HOME: &str = "robin@example.com";

fn event(summary: &str, start: i64, guests: &[(&str, Said)]) -> Event {
    let guest =
        |(email, said): &(&str, Said)| Guest { who: Address { name: None, email: (*email).into() }, said: *said };
    Event {
        uid: summary.into(),
        sequence: 0,
        instead_of: None,
        summary: summary.into(),
        start: Moment::Utc(start),
        end: None,
        location: None,
        description: None,
        organizer: None,
        guests: guests.iter().map(guest).collect(),
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

/// A calendar of one shelf for each account named, in order.
fn calendar(shelves: Vec<(&str, Vec<Event>)>) -> Calendar {
    let shelf = |(account, events): (&str, Vec<Event>)| Shelf {
        account: AccountId(account.into()),
        local: false,
        name: None,
        read: None,
        failed: None,
        events,
    };
    let identity = |(account, email): (&str, &str)| Identity {
        account: AccountId(account.into()),
        address: Address { name: None, email: email.into() },
    };
    Calendar {
        shelves: shelves.into_iter().map(shelf).collect(),
        identities: [("work", WORK), ("home", HOME)].into_iter().map(identity).collect(),
        ..Calendar::empty(Timestamp(NOON))
    }
}

fn app() -> App {
    App::new(crate::theme::choose(None, None, |_| None), crate::fixture_mailbox())
}

/// The command line.
fn said(app: &mut App) -> String {
    let line = app.lines(400, 40).pop().unwrap();
    line.iter().map(|span| span.text.as_str()).collect::<String>().trim().to_string()
}

fn keys(app: &mut App, diary: &mut Diary, keys: &str) {
    for key in keys.chars() {
        app.key(Key::Char(key));
        diary.switched(app);
    }
}

/// A diary that reminds through `recorder` and keeps its switch under
/// `state`, and where its calendars are sent.
fn diary(state: PathBuf, recorder: &Recorder) -> (Diary, Sender<Calendar>) {
    let (shown, arriving) = mpsc::channel();
    let reminders = Reminders::new(ON, Some(Box::new(recorder.clone())), state);
    (Diary::new(arriving, Some(reminders)), shown)
}

#[test]
fn a_calendar_that_arrives_is_handed_to_the_interface_once() {
    let (shown, arriving) = mpsc::channel();
    let (mut app, mut diary) = (app(), Diary::new(arriving, None));
    shown.send(crate::fixture_calendar()).unwrap();
    diary.begin(&mut app);
    assert!(app.reminders(), "with nothing that reminds, the switch is left as the interface has it");
    assert!(!diary.hand(&mut app), "it was taken before the first frame");
    keys(&mut app, &mut diary, "gc");
    let screen: String = app.lines(120, 40).iter().flatten().map(|span| span.text.as_str()).collect();
    assert!(screen.contains("Halden contract call"), "{screen}");
    // Of several that arrived together only the newest is the calendar.
    shown.send(calendar(vec![("work", vec![event("Older", NOON, &[])])])).unwrap();
    shown.send(calendar(vec![("work", vec![event("Newer", NOON, &[])])])).unwrap();
    assert!(diary.hand(&mut app));
    assert_eq!(diary.held.events.iter().map(|event| event.summary.as_str()).collect::<Vec<_>>(), ["Newer"]);
    assert!(!diary.tick(&mut app, Timestamp(NOON)), "and nothing reminds without reminders");
}

/// PLAN M15: on a clock the test sets, a reminder is raised once, to a
/// notifier that records.
#[test]
fn a_reminder_is_raised_once_when_it_falls_due_and_not_on_any_later_look() {
    let recorder = Recorder::default();
    let (mut diary, shown) = diary(scratch("once"), &recorder);
    let mut app = app();
    shown.send(calendar(vec![("work", vec![event("Stand-up", NOON + 15 * MINUTE, &[])])])).unwrap();
    diary.begin(&mut app);
    for (after, raised) in
        [(0, 0), (5 * MINUTE - 1, 0), (5 * MINUTE, 1), (5 * MINUTE, 1), (5 * MINUTE + 1, 1), (20 * MINUTE, 1)]
    {
        assert!(!diary.tick(&mut app, Timestamp(NOON + after)), "nothing is said for a reminder that was raised");
        assert_eq!(recorder.titles.borrow().len(), raised, "{after} seconds after noon");
    }
    assert_eq!(*recorder.titles.borrow(), ["Stand-up"]);
    let due: Vec<_> = diary.due.iter().map(|reminder| (reminder.summary.as_str(), reminder.until)).collect();
    assert_eq!(due, [("Stand-up", 10 * MINUTE)], "what fell due is kept for the interface");
}

#[test]
fn the_clock_is_not_looked_at_before_there_is_a_calendar_to_look_in() {
    let recorder = Recorder::default();
    let (mut diary, shown) = diary(scratch("late"), &recorder);
    let mut app = app();
    diary.begin(&mut app);
    assert!(!diary.tick(&mut app, Timestamp(NOON + 7 * MINUTE)));
    // It fell due two minutes before the calendar came, and has not begun.
    shown.send(calendar(vec![("work", vec![event("Stand-up", NOON + 15 * MINUTE, &[])])])).unwrap();
    assert!(diary.hand(&mut app));
    diary.tick(&mut app, Timestamp(NOON + 7 * MINUTE + 1));
    assert_eq!(*recorder.titles.borrow(), ["Stand-up"]);
}

/// ADR-058: off stays off across a restart, and the key that turns
/// reminders on or off is what writes the file.
#[test]
fn nothing_is_raised_while_off_and_the_switch_is_written_when_the_interface_s_changes() {
    let state = scratch("switch");
    notify::switch(&state, false).unwrap();
    let recorder = Recorder::default();
    let (mut diary, shown) = diary(state.clone(), &recorder);
    let mut app = app();
    let events = vec![event("While off", NOON + 15 * MINUTE, &[]), event("Once on", NOON + 30 * MINUTE, &[])];
    shown.send(calendar(vec![("work", events)])).unwrap();
    diary.begin(&mut app);
    assert!(!app.reminders(), "the interface starts as the switch was left");
    diary.tick(&mut app, Timestamp(NOON + 5 * MINUTE));
    assert_eq!(recorder.titles.borrow().len(), 0);

    let kept = || std::fs::read_to_string(state.join(SWITCH_FILE)).unwrap();
    keys(&mut app, &mut diary, "gc");
    assert_eq!(kept(), "off\n", "a key that is not the switch writes nothing");
    keys(&mut app, &mut diary, "gn");
    assert!(app.reminders());
    assert_eq!(kept(), "on\n");
    diary.tick(&mut app, Timestamp(NOON + 6 * MINUTE));
    assert_eq!(recorder.titles.borrow().len(), 0, "what fell due while off is not raised late");
    diary.tick(&mut app, Timestamp(NOON + 20 * MINUTE));
    assert_eq!(*recorder.titles.borrow(), ["Once on"]);
    keys(&mut app, &mut diary, "gn");
    assert_eq!((app.reminders(), kept().as_str()), (false, "off\n"));
}

#[test]
fn a_switch_that_cannot_be_kept_holds_for_this_run_and_says_so() {
    let root = scratch("unkept");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a-file"), "").unwrap();
    let (mut diary, _shown) = diary(root.join("a-file/state"), &Recorder::default());
    let mut app = app();
    diary.begin(&mut app);
    keys(&mut app, &mut diary, "gcgn");
    assert!(!app.reminders());
    let said = said(&mut app);
    assert!(said.contains("reminders are off until tuimex closes"), "{said}");
    keys(&mut app, &mut diary, "j");
    assert!(!app.reminders(), "and it is not tried again on every key");
}

#[test]
fn a_notifier_that_fails_is_said_once_and_the_reminders_are_still_kept() {
    let recorder = Recorder { fails: true, ..Recorder::default() };
    let (mut diary, shown) = diary(scratch("fails"), &recorder);
    let mut app = app();
    let events = vec![event("First", NOON + 15 * MINUTE, &[]), event("Second", NOON + 30 * MINUTE, &[])];
    shown.send(calendar(vec![("work", events)])).unwrap();
    diary.begin(&mut app);
    assert!(diary.tick(&mut app, Timestamp(NOON + 5 * MINUTE)), "something was said, so a frame is owed");
    let said = said(&mut app);
    assert!(said.ends_with("notify-send could not be started"), "{said}");
    assert!(!diary.tick(&mut app, Timestamp(NOON + 20 * MINUTE)), "the second failure is not said");
    assert_eq!(*recorder.titles.borrow(), ["First", "Second"]);
    assert_eq!(diary.due.iter().map(|reminder| reminder.summary.as_str()).collect::<Vec<_>>(), ["Second"]);
}

/// ADR-057: "you" is the account whose calendar holds the event. Two
/// accounts asked to one meeting hold the same event, and the one that
/// declined is not reminded of it.
#[test]
fn an_event_is_declined_by_the_account_that_holds_it_and_by_no_other() {
    let asked = [(WORK, Said::Accepted), (HOME, Said::Declined)];
    let meeting = || event("Planning", NOON + 15 * MINUTE, &asked);
    for (shelves, raised) in [
        (vec![("work", vec![meeting()])], 1),
        (vec![("home", vec![meeting()])], 0),
        (vec![("home", vec![meeting()]), ("work", vec![meeting()])], 1),
        (vec![("work", vec![meeting()]), ("home", vec![meeting()])], 1),
    ] {
        let recorder = Recorder::default();
        let (mut diary, shown) = diary(scratch("declined"), &recorder);
        let mut app = app();
        let accounts: Vec<&str> = shelves.iter().map(|(account, _)| *account).collect();
        shown.send(calendar(shelves)).unwrap();
        diary.begin(&mut app);
        diary.tick(&mut app, Timestamp(NOON + 5 * MINUTE));
        assert_eq!(recorder.titles.borrow().len(), raised, "held by {accounts:?}");
    }
}

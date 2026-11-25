use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use tuimex_core::calendar::{Event, Moment, Zone, days_from_civil};

use super::*;

/// A directory of this test's own under the build directory, never the
/// system temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("notify-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

/// Each title it was given, and whether to fail.
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

fn at(day: i64, hour: i64, minute: i64) -> i64 {
    days_from_civil(2026, 11, day) * 86_400 + hour * 3_600 + minute * 60
}

fn event(uid: &str, summary: &str, start: i64) -> Event {
    Event {
        uid: uid.into(),
        sequence: 0,
        instead_of: None,
        summary: summary.into(),
        start: Moment::Utc(start),
        end: None,
        location: Some("Room 4".into()),
        description: Some("the agenda".into()),
        organizer: None,
        guests: Vec::new(),
        cancelled: false,
        links: vec!["https://meet.example.com/abc".into()],
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

const ON: Settings = Settings { events: true, lead: 600, via: Via::Desktop };

#[test]
fn a_hostile_title_is_one_line_with_nothing_to_steer_text_or_a_terminal() {
    for (title, cleaned) in [
        ("Stand-up", "Stand-up"),
        ("two\nlines\tand\r\n  spaces ", "two lines and spaces"),
        ("\x1b]9;owned\x07after", "]9;owned after"),
        ("ends \x1b\\ early", "ends \\ early"),
        ("c1 \u{9c} terminator", "c1 terminator"),
        ("\u{202E}gnp.exe\u{202C} \u{2066}x\u{2069}\u{200F}", "gnp.exe x"),
        ("", "An event"),
        ("\x07\x1b", "An event"),
        ("<b>bold</b> &amp;", "<b>bold</b> &amp;"),
    ] {
        assert_eq!(clean(title), cleaned, "{title:?}");
    }
    let long = "x".repeat(LONGEST + 10);
    let cut = clean(&long);
    assert_eq!(cut.chars().count(), LONGEST);
    assert!(cut.ends_with('…'));
    assert_eq!(clean(&"é".repeat(LONGEST)), "é".repeat(LONGEST), "the limit is characters, not bytes");
}

#[test]
fn the_title_is_one_argument_after_the_options_end_and_never_markup_or_script() {
    let title = "-v <i>x</i> \" & do shell script \"rm\"";
    let linux = System::Linux.argv(title);
    assert_eq!(linux, ["notify-send", "--app-name=tuimex", "--", title, "Reminder from tuimex"]);
    let mac = System::Mac.argv(title);
    assert_eq!(mac.last().map(String::as_str), Some(title));
    assert_eq!(mac[mac.len() - 2], "--");
    assert_eq!(mac[0], "osascript");
    assert!(mac[..mac.len() - 1].iter().all(|a| !a.contains("rm")), "the script is fixed: {mac:?}");
    assert!(mac.iter().any(|a| a.contains("item 1 of argv")));
}

#[test]
fn osc9_writes_the_cleaned_title_and_an_escape_in_it_cannot_end_the_sequence_early() {
    let mut osc = Osc9::new(Vec::new());
    osc.notify("Call\x07\x1b]52;c;cGF5bG9hZA==\x1b\\ now").unwrap();
    let written = String::from_utf8(osc.out).unwrap();
    assert_eq!(written, "\x1b]9;Call ]52;c;cGF5bG9hZA== \\ now\x07");
    assert_eq!(written.matches('\x1b').count(), 1);
    assert_eq!(written.matches('\x07').count(), 1);
}

#[test]
fn a_program_that_is_not_there_is_a_sentence_naming_it() {
    let program = scratch("missing").join("notify-send").display().to_string();
    let mut desktop = Desktop { system: System::Linux, program: program.clone() };
    let said = desktop.notify("Stand-up").unwrap_err();
    assert!(said.starts_with(&program), "{said}");
    assert!(said.contains("status line"), "{said}");
}

#[test]
fn the_switch_is_remembered_and_a_missing_or_strange_file_is_the_config_s_default() {
    let state = scratch("switch");
    assert!(switched(&state, true), "no file: the default");
    assert!(!switched(&state, false));
    switch(&state, false).unwrap();
    assert!(!switched(&state, true));
    assert_eq!(std::fs::read_to_string(state.join(SWITCH_FILE)).unwrap(), "off\n");
    switch(&state, true).unwrap();
    assert!(switched(&state, false));
    std::fs::write(state.join(SWITCH_FILE), "maybe\n").unwrap();
    assert!(switched(&state, true) && !switched(&state, false));
}

#[test]
fn a_switch_file_that_is_a_link_is_not_read_and_not_written_through() {
    let state = scratch("switch-link");
    files::private_dir(&state).unwrap();
    let elsewhere = state.join("elsewhere");
    std::fs::write(&elsewhere, "off\n").unwrap();
    std::os::unix::fs::symlink(&elsewhere, state.join(SWITCH_FILE)).unwrap();
    assert!(switched(&state, true), "the link is no choice");
    switch(&state, true).unwrap();
    assert_eq!(std::fs::read_to_string(&elsewhere).unwrap(), "off\n");
    assert!(switched(&state, false));
}

#[test]
fn a_reminder_is_raised_once_with_its_title_alone_and_handed_to_the_interface() {
    let state = scratch("tick");
    let recorder = Recorder::default();
    let mut reminders = Reminders::new(ON, Some(Box::new(recorder.clone())), state);
    let events = vec![event("a", "-rf <b>Stand-up</b>\x1b[2J", at(24, 10, 0)), event("b", "Lunch", at(24, 12, 0))];
    let (zones, local) = (BTreeMap::new(), Zone::fixed(0));
    let mut handed = Vec::new();
    for minute in 0..=180 {
        let tick = reminders.tick(at(24, 9, 0) + minute * 60, &events, &zones, &local, &|_| false);
        assert_eq!(tick.failure, None);
        handed.extend(tick.due);
    }
    assert_eq!(handed.iter().map(|r| r.uid.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(handed[0].until, 600);
    // The recorder is handed the calendar's text; each notifier cleans it.
    assert_eq!(*recorder.titles.borrow(), ["-rf <b>Stand-up</b>\x1b[2J", "Lunch"]);
    let joined = recorder.titles.borrow().join(" ");
    for never in ["Room 4", "agenda", "meet.example.com"] {
        assert!(!joined.contains(never), "{never} was sent");
    }
}

#[test]
fn a_notifier_that_fails_is_said_once_and_reminders_still_reach_the_interface() {
    let recorder = Recorder { fails: true, ..Recorder::default() };
    let mut reminders = Reminders::new(ON, Some(Box::new(recorder.clone())), scratch("fails"));
    let events = vec![event("a", "One", at(24, 10, 0)), event("b", "Two", at(24, 10, 0))];
    let (zones, local) = (BTreeMap::new(), Zone::fixed(0));
    reminders.tick(at(24, 9, 0), &events, &zones, &local, &|_| false);
    let tick = reminders.tick(at(24, 9, 50), &events, &zones, &local, &|_| false);
    assert_eq!(tick.due.len(), 2);
    assert_eq!(tick.failure.as_deref(), Some("notify-send could not be started"));
    let later = vec![event("c", "Three", at(24, 11, 0))];
    let tick = reminders.tick(at(24, 10, 50), &later, &zones, &local, &|_| false);
    assert_eq!((tick.due.len(), tick.failure), (1, None), "said once a run");
    assert_eq!(recorder.titles.borrow().len(), 3, "each is still tried");
}

#[test]
fn off_stays_off_across_a_restart_and_raises_nothing_and_no_backlog_when_turned_on() {
    let state = scratch("off");
    let recorder = Recorder::default();
    let events = vec![event("a", "Stand-up", at(24, 10, 0)), event("b", "Lunch", at(24, 12, 0))];
    let (zones, local) = (BTreeMap::new(), Zone::fixed(0));
    let mut first = Reminders::new(ON, Some(Box::new(recorder.clone())), state.clone());
    first.set(false).unwrap();
    drop(first);

    let mut again = Reminders::new(ON, Some(Box::new(recorder.clone())), state.clone());
    assert!(!again.is_on(), "the config says on, and the switch said off");
    for minute in 0..=60 {
        assert_eq!(again.tick(at(24, 9, 0) + minute * 60, &events, &zones, &local, &|_| false), Tick::default());
    }
    assert_eq!(*recorder.titles.borrow(), Vec::<String>::new());
    again.set(true).unwrap();
    // The stand-up fell due at 09:50, while reminders were off.
    assert_eq!(again.tick(at(24, 10, 1), &events, &zones, &local, &|_| false), Tick::default(), "no backlog");
    let tick = again.tick(at(24, 11, 50), &events, &zones, &local, &|_| false);
    assert_eq!(tick.due.iter().map(|r| r.uid.as_str()).collect::<Vec<_>>(), ["b"]);
    assert_eq!(*recorder.titles.borrow(), ["Lunch"]);
    assert!(Reminders::new(Settings { events: false, ..ON }, None, state).is_on(), "the switch beats the config");
}

#[test]
fn a_declined_event_is_not_raised_and_a_clock_set_back_raises_nothing_twice() {
    let recorder = Recorder::default();
    let mut reminders = Reminders::new(ON, Some(Box::new(recorder.clone())), scratch("declined"));
    let events = vec![event("a", "Stand-up", at(24, 10, 0)), event("no", "Declined", at(24, 10, 0))];
    let (zones, local) = (BTreeMap::new(), Zone::fixed(0));
    let declined = |e: &Event| e.uid == "no";
    reminders.tick(at(24, 9, 49), &events, &zones, &local, &declined);
    assert_eq!(reminders.tick(at(24, 9, 51), &events, &zones, &local, &declined).due.len(), 1);
    reminders.tick(at(24, 9, 45), &events, &zones, &local, &declined);
    assert_eq!(reminders.tick(at(24, 9, 52), &events, &zones, &local, &declined).due.len(), 0);
    assert_eq!(*recorder.titles.borrow(), ["Stand-up"]);
}

#[test]
fn notify_s_keys_have_their_defaults_and_a_wrong_word_is_named() {
    let parse = |text: &str| crate::config::Config::parse(text).map(|c| settings(c.notify.as_ref()));
    assert_eq!(settings(None).unwrap(), Settings { events: true, lead: 600, via: Via::Desktop });
    let set = parse("[notify]\nevents = false\nlead = \"1h\"\nvia = \"osc9\"\n").unwrap().unwrap();
    assert_eq!(set, Settings { events: false, lead: 3_600, via: Via::Osc9 });
    assert_eq!(parse("[notify]\nlead = \"90s\"\nvia = \"none\"\n").unwrap().unwrap().lead, 90);
    let wrong = |text: &str| format!("{:#}", crate::config::Config::parse(text).unwrap_err());
    assert_eq!(wrong("[notify]\nvia = \"dbus\"\n"), "notify.via = \"dbus\": it is one of desktop, osc9, none");
    assert_eq!(wrong("[notify]\nlead = \"2d\"\n"), "notify.lead = \"2d\": the unit is one of s, m, h");
}

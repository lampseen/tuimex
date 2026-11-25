//! The flows that change the calendar or open something from it, each a strip of the day view: `cal-answer`,
//! `cal-propose`, `cal-ignore`, `cal-join`, `cal-delete`, `cal-repeating`, `cal-reminder`, and `cal-new-2`.
//! Each step is the keys of its flow typed into the real `App`, from Enter on the day the design puts it on.

use tuimex_core::calendar::DAY;
use tuimex_core::{AccountId, Timestamp};
use tuimex_fixtures::design as fx;
use tuimex_ui::app::App;
use tuimex_ui::calendar::{Calendar, Due};
use tuimex_ui::keymap::Key;
use tuimex_ui::span::Line;

use super::{calendar, over, typed};
use crate::{both, press};

pub const DRAWN: &[&str] = &[
    "cal-answer-0",
    "cal-answer-1",
    "cal-answer-2",
    "cal-propose-0",
    "cal-propose-1",
    "cal-propose-2",
    "cal-ignore-0",
    "cal-ignore-1",
    "cal-join-0",
    "cal-join-1",
    "cal-join-2",
    "cal-join-3",
    "cal-join-4",
    "cal-new-2",
    "cal-local-0",
    "cal-local-1",
    "cal-delete-0",
    "cal-delete-1",
    "cal-repeating-0",
    "cal-repeating-1",
    "cal-repeating-2",
    "cal-reminder-0",
    "cal-reminder-1",
    "cal-reminder-2",
    "cal-reminder-3",
];

/// Thursday's day, its first event selected; `j` from there is the
/// roadmap review and `jj` the vendor call.
const THU: &str = "gclll\r";
/// Friday's, where `jj` is the bank's account review.
const FRI: &str = "gcllll\r";

/// The design's `calendarStrip`: the label of the cursor's day and that
/// day's rows, the `panel` rows over the status line when a panel is
/// open, then the blank row, the status line and the command line.
fn strip(app: &mut App, panel: usize) -> Vec<Line> {
    let lines = app.lines(120, 40);
    let blank = |line: &Line| line.iter().all(|span| span.text.trim().is_empty());
    let day = lines[2..].iter().take_while(|line| !blank(line)).count();
    let foot = lines.len() - 3;
    lines[2..2 + day].iter().chain(&lines[foot - panel..]).cloned().collect()
}

/// Work's own calendar is one tuimex writes at its provider, which is
/// where the design's accepted answer goes.
fn writing(keys: &str) -> App {
    let mut app = typed("");
    app.set_writable_calendars(vec![AccountId("work".to_string())]);
    press(&mut app, keys);
    app
}

/// Work's own calendar is read over CalDAV and no write has been tried on
/// it, which is what the design's `cal-local` is drawn over.
fn untried(keys: &str) -> App {
    let mut app = typed("");
    app.set_unwritten_calendars(vec![AccountId("work".to_string())]);
    press(&mut app, keys);
    app
}

/// The design's `offsite`, typed into the form opened on Thursday's day:
/// Right on Calendar goes from the first account's to work's.
fn offsite(mut app: App) -> App {
    press(&mut app, &format!("{THU}cOffsite planning\t"));
    app.key(Key::Right);
    let guests = "Imani Castell <imani@example.org>, Lars Lindqvist <lars@example.org>";
    press(&mut app, &format!("\t14:00\t\t\t\tRoom Fjord\t{guests}^sy"));
    app
}

/// Today, ten minutes before the Halden call, which the reminder is of:
/// the calendar as it is read at ten to three.
fn reminded(keys: &str) -> App {
    let start = Timestamp(fx::NOW.0.div_euclid(DAY) * DAY + 15 * 3600);
    let mut app = over(Calendar { now: Timestamp(start.0 - 600), ..calendar() }, "");
    app.set_reminder(Some(Due {
        account: AccountId("work".to_string()),
        uid: "halden-call@design.example.net".to_string(),
        summary: "Halden contract call".to_string(),
        start,
    }));
    press(&mut app, &format!("gc\rj{keys}"));
    app
}

pub fn draw(name: &str, _width: usize, _height: usize) -> Option<Vec<Line>> {
    let day = |keys: &str| typed(keys);
    let (mut app, panel) = match name {
        "cal-answer-0" => (day(&format!("{THU}ji")), 0),
        "cal-answer-1" => (writing(&format!("{THU}jia")), 0),
        "cal-answer-2" => (writing(&format!("{THU}jiau")), 0),
        "cal-propose-0" => (day(&format!("{THU}jip")), 0),
        "cal-propose-1" => (day(&format!("{THU}jipthu 14:00")), 0),
        "cal-propose-2" => (day(&format!("{THU}jipthu 14:00\r")), 0),
        "cal-ignore-0" => (day(&format!("{FRI}jji")), 0),
        "cal-ignore-1" => (day(&format!("{FRI}jjix")), 0),
        "cal-join-0" => (day(&format!("{THU}jo")), 3),
        "cal-join-1" => (day(&format!("{THU}jo\r")), 0),
        "cal-join-2" => (day(&format!("{FRI}jjo")), 3),
        "cal-join-3" => (day(&format!("{FRI}jjo\r")), 0),
        "cal-join-4" => (day(&format!("{FRI}jjo\rn")), 0),
        "cal-new-2" => (offsite(typed("")), 0),
        "cal-local-0" => (offsite(untried("")), 0),
        "cal-local-1" => (untried(&format!("{THU}jia")), 0),
        "cal-delete-0" => (day(&format!("{THU}jjd")), 0),
        "cal-delete-1" => (day(&format!("{THU}jjdu")), 0),
        "cal-repeating-0" => (day(&format!("{THU}d")), 0),
        "cal-repeating-1" => (day(&format!("{THU}dt")), 0),
        "cal-repeating-2" => (day(&format!("{THU}dtu")), 0),
        "cal-reminder-0" => (reminded(""), 0),
        "cal-reminder-1" => (reminded("o\r"), 0),
        "cal-reminder-2" => (reminded("o\rgn"), 0),
        "cal-reminder-3" => (reminded("o\rgngn"), 0),
        _ => return None,
    };
    Some(strip(&mut app, panel))
}

fn steps(flow: &str, count: usize) {
    (0..count).for_each(|step| both(&format!("{flow}-{step}")));
}

#[test]
fn cal_answer() {
    steps("cal-answer", 3);
    steps("cal-propose", 3);
    steps("cal-ignore", 2);
}

#[test]
fn cal_join() {
    steps("cal-join", 5);
}

#[test]
fn cal_new() {
    both("cal-new-2");
    steps("cal-local", 2);
}

#[test]
fn cal_delete() {
    steps("cal-delete", 2);
    steps("cal-repeating", 3);
}

#[test]
fn cal_reminder() {
    steps("cal-reminder", 4);
}

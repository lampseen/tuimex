//! The calendar's outputs. Each is drawn by typing its caption's keys into
//! the real `App` over the design's calendar, as `tuimex --fixtures` holds
//! it. Today is Monday 23 November, so three days on from where `gc` opens
//! is the Thursday the design puts its cursor on.
//!
//! The month's outputs are here, with those of the list beside its grid
//! when that has the focus. Each other view and the flows have a file
//! of their own, with the outputs it draws and the tests that compare them.

mod day;
mod event;
mod flows;
mod form;
mod week;

use tuimex_core::{AccountId, Timestamp};
use tuimex_fixtures::design as fx;
use tuimex_ui::app::App;
use tuimex_ui::calendar::{Calendar, Shelf};
use tuimex_ui::span::Line;

use super::{DEFAULT, both, check, press, usual_mail};

/// The outputs drawn here; `drawn` adds those of the other files.
const DRAWN: [&str; 20] = [
    "cal-month",
    "cal-month-80-ascii",
    "cal-month-list-focus",
    "cal-month-list-focus-80-ascii",
    "cal-month-wide-list-focus",
    "cal-list-0",
    "cal-list-1",
    "cal-list-2",
    "cal-list-3",
    "cal-list-4",
    "cal-list-5",
    "cal-list-6",
    "cal-list-7",
    "cal-month-account",
    "cal-empty",
    "cal-unread",
    "cal-calendars",
    "help-calendar",
    "cal-zoom-0",
    "cal-goto-2",
];

/// Every calendar output some file draws.
pub fn drawn() -> Vec<&'static str> {
    [&DRAWN[..], week::DRAWN, day::DRAWN, event::DRAWN, form::DRAWN, flows::DRAWN].concat()
}

/// To Thursday the 26th from where `gc` opens.
const TO_THURSDAY: &str = "gclll";
/// And from its grid into the list beside it, by Alt and `l`.
const LISTED: &str = "gclll⌥l";
/// On through Thursday's four events to the first of Friday's.
const ACROSS: &str = "gclll⌥ljjjj";
/// Into the list from Sunday the 22nd, which has nothing on it.
const FROM_EMPTY: &str = "gch⌥l";

/// The design's `CALENDARS` and `EVENTS`, read when its `read` says.
fn calendar() -> Calendar {
    let ago = |seconds: u32| Timestamp(fx::NOW.0 - i64::from(seconds));
    let shelf = |kept: fx::Kept| Shelf {
        account: AccountId(kept.account.to_string()),
        local: kept.local,
        name: None,
        read: kept.read.map(ago),
        failed: None,
        events: kept.events,
    };
    Calendar {
        shelves: fx::calendars().into_iter().map(shelf).collect(),
        identities: fx::identities(),
        ..Calendar::empty(fx::NOW)
    }
}

/// The app over the design's mail and `calendar`, with `keys` typed.
fn over(calendar: Calendar, keys: &str) -> App {
    let mut app = super::app(usual_mail());
    app.set_calendar(calendar);
    press(&mut app, keys);
    app
}

fn typed(keys: &str) -> App {
    over(calendar(), keys)
}

/// The design's `cut`: the first `n` rows of a screen and its last three.
fn cut(lines: Vec<Line>, n: usize) -> Vec<Line> {
    lines[..n].iter().chain(&lines[lines.len() - 3..]).cloned().collect()
}

pub fn draw(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    Some(match name.strip_suffix("-80-ascii").unwrap_or(name) {
        "cal-month" => typed(TO_THURSDAY).lines(width, height),
        "cal-month-list-focus" => typed(FROM_EMPTY).lines(width, height),
        "cal-month-wide-list-focus" => typed(ACROSS).lines(width, height),
        "cal-list-0" => cut(typed(LISTED).lines(120, 40), 12),
        "cal-list-1" => cut(typed(&format!("{LISTED}j")).lines(120, 40), 12),
        "cal-list-2" => cut(typed(ACROSS).lines(120, 40), 12),
        "cal-list-3" => cut(typed(&format!("{ACROSS}l")).lines(120, 40), 12),
        "cal-list-4" => cut(typed(FROM_EMPTY).lines(120, 40), 12),
        "cal-list-5" => {
            // The design's `busy` Thursday, on a screen with eight rows for
            // it, and down to the last of its eight events.
            let mut calendar = calendar();
            let own = calendar.shelves.iter_mut().find(|shelf| shelf.account.0 == "personal" && !shelf.local).unwrap();
            own.events.extend(fx::busy());
            over(calendar, &format!("{LISTED}jjjjjjj")).lines(120, 14)
        }
        "cal-list-6" => cut(typed(&format!("{ACROSS}⌥h")).lines(120, 40), 12),
        // No day after the last standup of the year has anything on it.
        "cal-list-7" => cut(typed("gcgd2027-01-02\r⌥l").lines(120, 40), 12),
        "cal-month-account" => typed("gclllgt").lines(width, height),
        "cal-empty" => over(Calendar::empty(fx::NOW), "gc").lines(width, height),
        "cal-unread" => {
            // The account's own calendar could not be fetched, and what is
            // shown of it was read two hours ago.
            let mut calendar = calendar();
            let own = calendar.shelves.iter_mut().find(|shelf| shelf.account.0 == "personal" && !shelf.local).unwrap();
            (own.failed, own.read) = (Some("offline".to_string()), Some(Timestamp(fx::NOW.0 - 2 * 3600)));
            over(calendar, TO_THURSDAY).lines(width, height)
        }
        "cal-calendars" => typed("gclllCj ").lines(width, height),
        "help-calendar" => typed("gc?").lines(width, height),
        "cal-zoom-0" => cut(typed(TO_THURSDAY).lines(120, 40), 12),
        "cal-goto-2" => cut(typed("gclllgd5 dec\r").lines(120, 40), 12),
        _ => {
            let others = [week::draw, day::draw, event::draw, form::draw, flows::draw];
            return others.iter().find_map(|draw| draw(name, width, height));
        }
    })
}

#[test]
fn cal_month() {
    both("cal-month");
    check(DEFAULT, "cal-month-80-ascii");
    both("cal-month-account");
    both("cal-unread");
}

#[test]
fn cal_empty() {
    both("cal-empty");
}

#[test]
fn cal_calendars() {
    both("cal-calendars");
}

#[test]
fn help_calendar() {
    both("help-calendar");
}

/// The steps of the flows that are the month's screen.
#[test]
fn cal_month_steps() {
    both("cal-zoom-0");
    both("cal-goto-2");
}

/// The focus in the list beside the grid, at each width the design draws
/// it at, and the flow that moves it.
#[test]
fn cal_month_list() {
    both("cal-month-list-focus");
    check(DEFAULT, "cal-month-list-focus-80-ascii");
    both("cal-month-wide-list-focus");
    (0..8).for_each(|step| both(&format!("cal-list-{step}")));
}

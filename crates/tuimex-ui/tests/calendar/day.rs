//! The day view's outputs: `cal-day`, `cal-day-80-ascii`, `cal-day-wide`, `cal-find`, `cal-search`, and the
//! steps of the flows that are the day's screen or a strip of it: `cal-zoom-1`, `cal-zoom-3`, `cal-goto-0` and
//! `cal-goto-1`.

use tuimex_ui::keymap::Key;
use tuimex_ui::span::Line;

use super::{TO_THURSDAY, cut, typed};
use crate::{DEFAULT, both, check};

pub const DRAWN: &[&str] = &[
    "cal-day",
    "cal-day-80-ascii",
    "cal-day-wide",
    "cal-find",
    "cal-search",
    "cal-zoom-1",
    "cal-zoom-3",
    "cal-goto-0",
    "cal-goto-1",
];

/// Into Thursday from the month, and down from its first event to the
/// invitation the design selects.
const ROADMAP: &str = "gclll\rj";

/// The design's `calendarStrip` as the day view holds it: the label of the
/// cursor's day and that day's rows, then the status line and the command
/// line under the blank row before them.
fn strip(lines: Vec<Line>) -> Vec<Line> {
    let blank = |line: &Line| line.iter().all(|span| span.text.trim().is_empty());
    let day = lines[3..].iter().take_while(|line| !blank(line)).count();
    lines[3..3 + day].iter().chain(&lines[lines.len() - 3..]).cloned().collect()
}

pub fn draw(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    Some(match name.strip_suffix("-80-ascii").unwrap_or(name) {
        "cal-day" | "cal-day-wide" => typed(ROADMAP).lines(width, height),
        "cal-find" => typed("gc3/call").lines(width, height),
        "cal-search" => typed("gc/call\r").lines(width, height),
        "cal-zoom-1" => cut(typed(ROADMAP).lines(120, 40), 12),
        "cal-zoom-3" => {
            let mut app = typed(&format!("{ROADMAP}\r"));
            app.key(Key::Esc);
            cut(app.lines(120, 40), 12)
        }
        "cal-goto-0" => strip(typed(&format!("{TO_THURSDAY}3gd")).lines(width, 40)),
        "cal-goto-1" => strip(typed(&format!("{TO_THURSDAY}3gd5 dec")).lines(width, 40)),
        _ => return None,
    })
}

#[test]
fn cal_day() {
    both("cal-day");
    check(DEFAULT, "cal-day-80-ascii");
    both("cal-day-wide");
}

#[test]
fn cal_find() {
    both("cal-find");
}

#[test]
fn cal_search() {
    both("cal-search");
}

/// The steps of the flows that are the day's screen.
#[test]
fn cal_day_steps() {
    both("cal-zoom-1");
    both("cal-zoom-3");
    both("cal-goto-0");
    both("cal-goto-1");
}

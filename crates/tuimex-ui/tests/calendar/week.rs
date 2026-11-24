//! The week view's outputs: `cal-week` and `cal-week-80-ascii`.

use tuimex_ui::span::Line;

use super::{DEFAULT, TO_THURSDAY, both, check, typed};

pub const DRAWN: &[&str] = &["cal-week", "cal-week-80-ascii"];

pub fn draw(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    Some(match name.strip_suffix("-80-ascii").unwrap_or(name) {
        // The week view, and the second of Thursday's events.
        "cal-week" => typed(&format!("{TO_THURSDAY}2jj")).lines(width, height),
        _ => return None,
    })
}

#[test]
fn cal_week() {
    both("cal-week");
    check(DEFAULT, "cal-week-80-ascii");
}

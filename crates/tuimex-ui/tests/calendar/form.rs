//! The event form's outputs: `cal-edit`, and the two steps of `cal-new` that are the form, `cal-new-0` and
//! `cal-new-1`. Each is typed into the real `App`: on Thursday, narrowed to work so the form opens in work's
//! calendar, every field filled in turn and Tab round to the title again.

use tuimex_ui::span::Line;

use super::{both, cut, typed};

pub const DRAWN: &[&str] = &["cal-edit", "cal-new-0", "cal-new-1"];

/// The design's `offsite`, typed field by field from Title.
const OFFSITE: &str = "gclllgtgtcOffsite planning\t\t14:00\t\t\t\tRoom Fjord\t\
                       Imani Castell <imani@example.org>, Lars Lindqvist <lars@example.org>\t\t";

pub fn draw(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    Some(match name.strip_suffix("-80-ascii").unwrap_or(name) {
        "cal-edit" => typed(OFFSITE).lines(width, height),
        "cal-new-0" => cut(typed(OFFSITE).lines(120, 40), 13),
        "cal-new-1" => cut(typed(&format!("{OFFSITE}^s")).lines(120, 40), 13),
        _ => return None,
    })
}

#[test]
fn cal_edit() {
    both("cal-edit");
}

#[test]
fn cal_new_steps() {
    both("cal-new-0");
    both("cal-new-1");
}

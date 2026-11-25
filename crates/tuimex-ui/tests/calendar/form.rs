//! The event form's outputs: `cal-edit`, `cal-edit-choice` with one more Tab, and the two steps of `cal-new`
//! that are the form, `cal-new-0` and `cal-new-1`, and the flow `cal-when`, which is typed into Starts and Ends. Each is typed into the real `App`: on Thursday, narrowed to work so the form opens in work's
//! calendar, every field filled in turn and Tab round to the title again.

use tuimex_ui::span::Line;

use super::{both, cut, typed};

pub const DRAWN: &[&str] =
    &["cal-edit", "cal-edit-choice", "cal-new-0", "cal-new-1", "cal-when-0", "cal-when-1", "cal-when-2", "cal-when-3"];

/// The design's `offsite`, typed field by field from Title.
const OFFSITE: &str = "gclllgtgtcOffsite planning\t\t14:00\t\t\t\tRoom Fjord\t\
                       Imani Castell <imani@example.org>, Lars Lindqvist <lars@example.org>\t\t";

pub fn draw(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    Some(match name.strip_suffix("-80-ascii").unwrap_or(name) {
        "cal-edit" => typed(OFFSITE).lines(width, height),
        "cal-edit-choice" => typed(&format!("{OFFSITE}\t")).lines(width, height),
        "cal-new-0" => cut(typed(OFFSITE).lines(120, 40), 14),
        "cal-new-1" => cut(typed(&format!("{OFFSITE}^s")).lines(120, 40), 14),
        // Two fields on from the title is Starts: a time alone is typed
        // there, the field is left, and words that name nothing follow.
        "cal-when-0" => cut(typed(&format!("{OFFSITE}\t\t")).lines(120, 40), 14),
        "cal-when-1" => cut(typed(&format!("{OFFSITE}\t\t15:30")).lines(120, 40), 14),
        "cal-when-2" => cut(typed(&format!("{OFFSITE}\t\t15:30\t")).lines(120, 40), 14),
        "cal-when-3" => cut(typed(&format!("{OFFSITE}\t\t15:30\tsoon")).lines(120, 40), 14),
        _ => return None,
    })
}

#[test]
fn cal_edit() {
    both("cal-edit");
    both("cal-edit-choice");
}

#[test]
fn cal_new_steps() {
    both("cal-new-0");
    both("cal-new-1");
}

#[test]
fn cal_when() {
    (0..4).for_each(|step| both(&format!("cal-when-{step}")));
}

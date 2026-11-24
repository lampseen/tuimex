//! The event form's outputs: `cal-edit`, and the two steps of `cal-new` that are the form, `cal-new-0` and
//! `cal-new-1`.
//! None is drawn until what it shows is built: `DRAWN` lists those that are, and `draw` draws them.

use tuimex_ui::span::Line;

pub const DRAWN: &[&str] = &[];

pub fn draw(_name: &str, _width: usize, _height: usize) -> Option<Vec<Line>> {
    None
}

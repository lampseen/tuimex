//! The flows that change the calendar or open something from it, each a strip of the day view: `cal-answer`,
//! `cal-propose`, `cal-ignore`, `cal-join`, `cal-delete`, `cal-repeating`, `cal-reminder`, and `cal-new-2`.
//! None is drawn until what it shows is built: `DRAWN` lists those that are, and `draw` draws them.

use tuimex_ui::span::Line;

pub const DRAWN: &[&str] = &[];

pub fn draw(_name: &str, _width: usize, _height: usize) -> Option<Vec<Line>> {
    None
}

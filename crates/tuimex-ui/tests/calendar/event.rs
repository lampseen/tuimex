//! The event screen's outputs: `cal-event`, `cal-event-80-ascii`, `cal-event-more`, `cal-event-answer`,
//! `cal-event-local`, and `cal-zoom-2`.
//! None is drawn until what it shows is built: `DRAWN` lists those that are, and `draw` draws them.

use tuimex_ui::span::Line;

pub const DRAWN: &[&str] = &[];

pub fn draw(_name: &str, _width: usize, _height: usize) -> Option<Vec<Line>> {
    None
}

//! The week view's outputs: `cal-week` and `cal-week-80-ascii`.
//! None is drawn until what it shows is built: `DRAWN` lists those that are, and `draw` draws them.

use tuimex_ui::span::Line;

pub const DRAWN: &[&str] = &[];

pub fn draw(_name: &str, _width: usize, _height: usize) -> Option<Vec<Line>> {
    None
}

//! The day view's outputs: `cal-day`, `cal-day-80-ascii`, `cal-day-wide`, `cal-find`, `cal-search`, and the
//! steps of the flows that are the day's screen or a strip of it: `cal-zoom-1`, `cal-zoom-3`, `cal-goto-0` and
//! `cal-goto-1`.
//! None is drawn until what it shows is built: `DRAWN` lists those that are, and `draw` draws them.

use tuimex_ui::span::Line;

pub const DRAWN: &[&str] = &[];

pub fn draw(_name: &str, _width: usize, _height: usize) -> Option<Vec<Line>> {
    None
}

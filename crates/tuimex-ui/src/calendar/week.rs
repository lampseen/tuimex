//! The week view: the design's `weekScreen`, days as columns against the
//! clock. It is not drawn: the screen says so, in the frame every view has.

use super::parts::{self, Scene};
use crate::say;
use crate::span::Line;

pub fn screen(o: &Scene, width: usize, height: usize) -> Vec<Line> {
    let mut content = vec![say::not_built("week", "the calendar's week view").line(width)];
    content.resize(o.room(height), Line::new());
    parts::frame(o, width, "", content, parts::day_status(o, o.cursor), parts::hint(o.entry().as_ref()))
}

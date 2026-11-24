//! The day view: the design's `dayScreen`, the list of mail again with
//! events from the cursor's day on and the selected one below or beside
//! them, and with `Scene::search` what a search found. It is not drawn:
//! the screen says so, in the frame every view has.

use super::parts::{self, Scene};
use crate::say;
use crate::span::Line;

pub fn screen(o: &Scene, width: usize, height: usize) -> Vec<Line> {
    let what = if o.search.is_some() { "search" } else { "day" };
    let mut content = vec![say::not_built(what, "the calendar's day view").line(width)];
    content.resize(o.room(height), Line::new());
    parts::frame(o, width, "", content, parts::day_status(o, o.cursor), parts::hint(o.entry().as_ref()))
}

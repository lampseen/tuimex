//! One event, whole: the design's `eventScreen`, with its fields, its
//! notices, its notes and what is folded under More. It is not drawn: the
//! screen is the event's title and a line that says so.

use super::Entry;
use super::parts::{self, Scene};
use crate::list::{heading, status_row};
use crate::palette::Tok;
use crate::say;
use crate::span::{Line, run};

pub fn screen(o: &Scene, entry: &Entry, width: usize, height: usize) -> Vec<Line> {
    let title = heading(vec![run(entry.event.summary.as_str(), Tok::Fg).bold()]);
    let mut lines = vec![title, Line::new(), say::not_built("event", "the calendar's event view").line(width)];
    lines.resize(height.saturating_sub(3 + o.panel.len()), Line::new());
    lines.extend(o.panel.iter().cloned());
    let left = o.left.clone().unwrap_or_default();
    let command = o.command.clone().unwrap_or_else(|| parts::hint(Some(entry)));
    lines.extend([Line::new(), status_row(width, &left, &o.right), command]);
    lines
}

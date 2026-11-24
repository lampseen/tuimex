//! The form for a new event or a change to one: the design's
//! `eventFormScreen`, a field to a row as in compose. It is not drawn: the
//! screen is its heading and a line that says so.

use super::parts::Scene;
use crate::keymap;
use crate::list::{heading, hint, status_row};
use crate::palette::Tok;
use crate::say;
use crate::span::{Line, run};

/// `title` heads the form: a new event, or the one being changed.
pub fn screen(o: &Scene, title: &str, width: usize, height: usize) -> Vec<Line> {
    let mut lines = vec![
        heading(vec![run(title, Tok::Fg).bold()]),
        Line::new(),
        say::not_built("form", "the calendar's event form").line(width),
    ];
    lines.resize(height.saturating_sub(3 + o.panel.len()), Line::new());
    lines.extend(o.panel.iter().cloned());
    let left = o.left.clone().unwrap_or_default();
    let command = o.command.clone().unwrap_or_else(|| hint(&[keymap::hint("event-form.close")]));
    lines.extend([Line::new(), status_row(width, &left, &o.right), command]);
    lines
}

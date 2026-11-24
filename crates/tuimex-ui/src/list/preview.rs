//! The preview beside or under the list: the design's `previewPane`, which
//! is the message as the reader would open it, under its subject.

use tuimex_core::Listed;

use crate::glyphs::CURSOR;
use crate::layout::{INDENT, MARGIN};
use crate::palette::Tok;
use crate::reader;
use crate::span::{Line, fit, gap, run};

/// What `pane` drew: exactly as many rows as it was given, and which lines
/// of the message they are.
pub(crate) struct Pane {
    pub rows: Vec<Line>,
    /// How many lines are scrolled off the top, and the most that can be.
    pub top: usize,
    pub last_top: usize,
    /// The last line in sight, counted from 1, and how many there are.
    pub last: usize,
    pub total: usize,
    /// How many lines of the message fit under the subject.
    pub room: usize,
}

/// The newest message of the selected row: the subject as a heading, then
/// who, to whom, anything to read first, and the whole message from
/// `o.pane_top`. Focused, the subject carries the cursor. Until the
/// message has been read it is its opening, or with `o.arriving` a line
/// saying that the body is being fetched.
pub(crate) fn pane(msg: &Listed, o: &super::List, width: usize, height: usize) -> Pane {
    let inner = width.saturating_sub(MARGIN);
    let (head, mut text, _) = reader::block(msg, o.body, o.check, &[], width, true);
    if o.arriving {
        text = vec![vec![gap(INDENT), run("Fetching this message.", Tok::Dim)]];
    }
    let mut body = head;
    body.push(Line::new());
    body.extend(text);
    let room = height.saturating_sub(2);
    let last_top = body.len().saturating_sub(room);
    let top = o.pane_top.min(last_top);
    let last = body.len().min(top + room);
    let cursor = if o.focus { run(CURSOR, Tok::Acc) } else { gap(1) };
    let title = vec![cursor, gap(MARGIN - 1), run(msg.envelope.subject.as_str(), Tok::Fg).bold()];
    let mut rows = vec![fit(&title, inner), Line::new()];
    rows.extend(body[top..last].iter().map(|row| if row.is_empty() { Line::new() } else { fit(row, inner) }));
    rows.resize(height, Line::new());
    Pane { rows, top, last_top, last, total: body.len(), room }
}

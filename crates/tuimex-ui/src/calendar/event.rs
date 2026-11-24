//! One event, whole: the design's `eventScreen`, `eventNotices`,
//! `moreRows` and `eventHint`. The title with where the event sits among
//! its day's, the fields every event has and then its own, the notices,
//! the notes, and what only one provider sends folded under More.

use tuimex_core::Address;
use tuimex_core::calendar::{Standing, alarm_words, links_of, repeat_words};

use super::Entry;
use super::parts::{self, Scene, day_short};
use crate::glyphs::{DOT, FOLDED, NOTICE, UNFOLDED};
use crate::keymap;
use crate::layout::{INDENT, MARGIN};
use crate::list::{self, heading, status_row};
use crate::palette::Tok;
use crate::reader;
use crate::span::{Line, cells, fit, gap, left_right, run};

/// How the event is being looked at: how many rows of its body are
/// scrolled off the top, and whether More is unfolded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct At {
    pub top: usize,
    pub more: bool,
}

/// What `screen` drew beside the lines: the top it kept to, the furthest
/// the body can scroll, and how many of its rows show.
#[derive(Clone, Debug)]
pub struct Drawn {
    pub lines: Vec<Line>,
    pub top: usize,
    pub last_top: usize,
    pub room: usize,
}

fn name(who: &Address) -> String {
    who.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| who.email.clone())
}

fn indent(rows: Vec<Line>) -> Vec<Line> {
    rows.into_iter().map(|row| [vec![gap(INDENT)], row].concat()).collect()
}

/// `2 of 4 on Thu 26 Nov`: where the event is among its day's, as the day
/// view lists them.
fn place(o: &Scene, entry: &Entry) -> String {
    let days = o.days(entry.day, entry.day + 1);
    let all = days.on(entry.day);
    match all.iter().position(|other| other.is(&entry.pick())) {
        Some(at) => format!("{} of {} on {}", at + 1, all.len(), day_short(entry.day)),
        None => String::new(),
    }
}

/// What the reader has said, as the Answer field says it.
fn answer(entry: &Entry) -> Option<Line> {
    let (said, tone) = match (entry.proposed, entry.standing?) {
        (Some(_), _) => ("proposed another time", Tok::Fg),
        (None, Standing::Needs) => ("not answered", Tok::Warn),
        (None, Standing::Accepted) => ("accepted", Tok::Fg),
        (None, Standing::Tentative) => ("tentative", Tok::Fg),
        (None, Standing::Declined) => ("declined", Tok::Fg),
    };
    Some(vec![run(said, tone)])
}

/// The header block in the reader's form, `width` cells wide: the fields
/// every event has, then those the preview leaves out.
pub fn fields(o: &Scene, entry: &Entry, width: usize) -> Vec<Line> {
    let mut rows = parts::event_fields(o, entry, false, width);
    let event = entry.event;
    let mut own: Vec<Line> = Vec::new();
    own.extend(answer(entry).into_iter().flat_map(|said| reader::field("Answer", vec![said])));
    if let Some(repeat) = &event.repeat {
        own.extend(reader::field("Repeats", vec![vec![run(repeat_words(repeat), Tok::Fg)]]));
    }
    if !event.alarms.is_empty() {
        let said: Vec<String> = event.alarms.iter().map(|before| alarm_words(*before)).collect();
        own.extend(reader::field("Reminder", vec![vec![run(said.join(", "), Tok::Fg)]]));
    }
    rows.extend(own.iter().map(|row| fit(row, width)));
    rows
}

/// What is so about the event before anything else is read: that it waits
/// for an answer, and that only tuimex holds it (ADR-055).
fn notices(entry: &Entry) -> Vec<Line> {
    let mut rows = Vec::new();
    if entry.waiting() {
        let by = entry.event.organizer.as_ref().map(name).unwrap_or_default();
        rows.push(reader::notice(NOTICE, Tok::Warn, "Needs your answer", format!("asked by {by}")));
    }
    if entry.local {
        let detail = format!("{}'s own calendar does not have it {DOT} :export writes it out", entry.account.0);
        rows.push(reader::notice(NOTICE, Tok::Warn, "Only in tuimex", detail));
    }
    rows
}

/// What only one provider sends: folded, a line that names it; unfolded,
/// each under its label. An event with none has no More.
fn more(entry: &Entry, open: bool, width: usize) -> Vec<Line> {
    let more = &entry.event.more;
    if more.is_empty() {
        return Vec::new();
    }
    if !open {
        let labels: Vec<String> = more.iter().map(|(label, _)| label.to_lowercase()).collect();
        let line = [run(format!("{FOLDED} More"), Tok::Dim), run(format!(" {DOT} {}", labels.join(", ")), Tok::Dim)];
        return vec![Line::new(), fit(&line, width)];
    }
    let mut rows = vec![Line::new(), vec![run(format!("{UNFOLDED} More"), Tok::Dim)]];
    for (label, value) in more {
        let field = reader::field(label, vec![vec![run(value.as_str(), Tok::Fg)]]);
        rows.extend(field.into_iter().map(|row| fit(&[vec![gap(2)], row].concat(), width)));
    }
    rows
}

/// Everything under the header block on a screen `width` cells wide,
/// before it is indented: the notices, the notes, and More.
pub fn body(entry: &Entry, open: bool, width: usize) -> Vec<Line> {
    let inner = width.saturating_sub(INDENT + MARGIN);
    let notices: Vec<Line> = notices(entry).iter().flat_map(|row| reader::notice_lines(row, inner)).collect();
    let notes = entry.event.description.as_deref().filter(|notes| !notes.trim().is_empty());
    let notes = notes.map_or_else(Vec::new, |notes| reader::text::rows(notes, reader::measure(width), None).0);
    let mut rows = notices;
    if !rows.is_empty() && !notes.is_empty() {
        rows.push(Line::new());
    }
    rows.extend(notes);
    rows.extend(more(entry, open, inner));
    rows
}

/// The idle command line: the keys that do something to this event.
fn hint(o: &Scene, entry: &Entry) -> Line {
    let mut ids = Vec::new();
    ids.extend(entry.waiting().then_some("calendar.answer"));
    ids.extend((!links_of(entry.event).is_empty()).then_some("calendar.links"));
    let came_in = o.calendar.came_in.iter().any(|(uid, id)| *uid == entry.event.uid && id.account == *entry.account);
    ids.extend(came_in.then_some("event.mail"));
    ids.extend(["calendar.edit", "calendar.delete", "global.back"]);
    list::hint(&ids.iter().map(|id| keymap::hint(id)).collect::<Vec<_>>())
}

/// The whole screen: the title, the header block, as much of the body as
/// fits from `at.top` on, the panel, a blank row, the status line and the
/// command line.
pub fn screen(o: &Scene, entry: &Entry, at: At, width: usize, height: usize) -> Drawn {
    let inner = width.saturating_sub(INDENT + MARGIN);
    let title = heading(vec![run(entry.event.summary.as_str(), Tok::Fg).bold()]);
    let mut lines =
        vec![left_right(&title, &[gap(2), run(place(o, entry), Tok::Dim), gap(MARGIN)], width), Line::new()];
    lines.extend(indent(fields(o, entry, inner)));
    lines.push(Line::new());

    let body = indent(body(entry, at.more, width));
    let room = height.saturating_sub(lines.len() + o.panel.len() + 3);
    let last_top = body.len().saturating_sub(room);
    let top = at.top.min(last_top);
    let mut middle = body[top..body.len().min(top + room)].to_vec();
    while middle.last().is_some_and(|line| cells(line) <= INDENT) {
        middle.pop();
    }
    middle.resize(room, Line::new());
    lines.extend(middle);
    lines.extend(o.panel.iter().cloned());
    let command = o.command.clone().unwrap_or_else(|| hint(o, entry));
    lines.extend([Line::new(), status_row(width, o.left.as_deref().unwrap_or_default(), &o.right), command]);
    Drawn { lines, top, last_top, room }
}

#[cfg(test)]
mod tests;

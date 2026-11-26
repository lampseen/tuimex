//! The reader as `design/tui.js` draws it: `readerScreen`, `headerBlock`
//! and `noticeRows`, ported and kept behaviourally identical (ADR-002).

mod panel;
pub(crate) mod text;

use tuimex_core::{Address, Body, Check, Listed, Origin, Security};

pub(crate) use self::panel::{Look, files, link_rows, links, look};
pub use self::text::Placed;
use crate::glyphs::{AGENT, BAD, DOT, NOTICE, OK};
use crate::keymap;
use crate::list::{heading, hint, status_row};
use crate::palette::Tok;
use crate::say::count;
use crate::span::{Line, Span, cells, fit, gap, left_right, run};
use crate::spot::{Spot, Target};
use crate::when::{clock, date, long_date};

const MARGIN: usize = 2;
const INDENT: usize = 4;
/// Body text wraps here however wide the terminal is.
const MEASURE: usize = 76;

pub(crate) fn measure(width: usize) -> usize {
    MEASURE.min(width.saturating_sub(INDENT + MARGIN))
}

/// A size as the design writes one: `96 KB`, `2.4 MB`, `31 MB`.
pub(crate) fn size_text(bytes: u32) -> String {
    let kb = f64::from(bytes) / 1024.0;
    let mb = kb / 1024.0;
    if mb < 1.0 {
        format!("{} KB", kb.round().max(1.0))
    } else if mb < 10.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{mb:.0} MB")
    }
}

/// The design's `LAYOUT.header`: the label column, and how many rows a
/// list of recipients or files may take in the preview and in the reader
/// before it says how many more there are.
pub(crate) const LABEL: usize = 9;
pub(crate) const ROWS: (usize, usize) = (2, 6);

/// A value under its label: the label on the first row, and every row
/// after it under the value.
pub(crate) fn field(label: &str, value: Vec<Line>) -> Vec<Line> {
    let row = |(i, line): (usize, Line)| {
        [vec![run(format!("{:<LABEL$}", if i == 0 { label } else { "" }), Tok::Dim)], line].concat()
    };
    value.into_iter().enumerate().map(row).collect()
}

/// A line as its cells, each in its own style.
fn explode(line: &[Span]) -> Line {
    line.iter().flat_map(|s| s.text.chars().map(move |c| Span { text: c.to_string(), ..s.clone() })).collect()
}

/// Styled text wrapped at spaces to `width`, each word keeping its style:
/// the design's `wrapRuns`. A word is broken only when it alone is wider
/// than the row.
pub(crate) fn wrap_runs(line: &[Span], width: usize) -> Vec<Line> {
    let (width, cells) = (width.max(1), explode(line));
    let mut lines: Vec<Line> = vec![Line::new()];
    // The space before a word, which goes with it onto a row it shares.
    let mut space = None;
    for word in cells.split_inclusive(|cell| cell.text == " ") {
        let (letters, after) = match word.split_last() {
            Some((last, letters)) if last.text == " " => (letters, Some(last)),
            _ => (word, None),
        };
        let last = lines.last().map_or(0, Vec::len);
        if !letters.is_empty() && last > 0 && last + 1 + letters.len() > width {
            lines.push(Line::new());
        } else if !letters.is_empty() && last > 0 {
            lines.last_mut().into_iter().for_each(|line| line.extend(space.take()));
        }
        for cell in letters {
            if lines.last().is_some_and(|line| line.len() >= width) {
                lines.push(Line::new());
            }
            lines.last_mut().into_iter().for_each(|line| line.push(cell.clone()));
        }
        space = after.cloned().or(space);
    }
    lines
}

/// Whole items, each a recipient or a file, packed into rows of `width`
/// with a comma between them: the design's `packed`. An item moves to the
/// next row sooner than be split. Past `most` rows the last ends `and N
/// more`.
pub(crate) fn packed(items: Vec<Line>, width: usize, most: usize) -> Vec<Line> {
    let used = |row: &[Line]| row.iter().map(|item| cells(item) + 2).sum::<usize>().saturating_sub(2);
    let mut rows: Vec<Vec<Line>> = Vec::new();
    for item in &items {
        match rows.last_mut().filter(|row| used(row) + 2 + cells(item) < width) {
            Some(row) => row.push(item.clone()),
            None => rows.push(vec![item.clone()]),
        }
    }
    let cut = rows.len() > most;
    rows.truncate(most);
    let shown = |rows: &[Vec<Line>]| rows.iter().map(Vec::len).sum::<usize>();
    let more = |rows: &[Vec<Line>]| run(format!(" and {} more", items.len() - shown(rows)), Tok::Dim);
    while cut && rows.last().is_some_and(|last| last.len() > 1 && used(last) + 1 + cells(&[more(&rows)]) > width) {
        rows.last_mut().and_then(Vec::pop);
    }
    let tail = cut.then(|| more(&rows));
    let count = rows.len();
    let line = |(y, row): (usize, Vec<Line>)| {
        let mut joined: Line = row.join(&run(", ", Tok::Dim));
        if y + 1 < count || tail.is_some() {
            joined.push(run(",", Tok::Dim));
        }
        joined.extend(tail.clone().filter(|_| y + 1 == count));
        wrap_runs(&joined, width)
    };
    rows.into_iter().enumerate().flat_map(line).collect()
}

pub(crate) fn person(a: &Address) -> Line {
    match a.name.as_deref().filter(|n| !n.is_empty()) {
        Some(name) => vec![run(format!("{name} "), Tok::Fg), run(format!("<{}>", a.email), Tok::Dim)],
        None => vec![run(a.email.as_str(), Tok::Fg)],
    }
}

/// A mark and what is so, in the colour of how serious it is, and the
/// detail after a dot.
pub(crate) fn notice(mark: char, tok: Tok, what: &str, detail: String) -> Line {
    vec![run(format!("{mark} {what}"), tok), run(format!(" {DOT} {detail}"), Tok::Dim)]
}

/// A notice wrapped under its own text, clear of its mark: the design's
/// `noticeLines`. Each word keeps its colour, so a headline that wraps is
/// still the colour of its mark.
pub(crate) fn notice_lines(notice: &[Span], width: usize) -> Vec<Line> {
    let all = explode(notice);
    let (mark, text) = all.split_at(2.min(all.len()));
    let wrapped = wrap_runs(text, width.saturating_sub(2)).into_iter().enumerate();
    wrapped.map(|(i, line)| [if i == 0 { mark.to_vec() } else { vec![gap(2)] }, line].concat()).collect()
}

/// PLAN §6.5: one line each, only when true. A signed or encrypted message
/// nothing has checked says that and no more: its structure is not a
/// verdict (ADR-020).
fn notices(msg: &Listed, body: Option<&Body>, check: Option<&Check>) -> Vec<Line> {
    let e = &msg.envelope;
    let mut rows = Vec::new();
    if e.origin == Origin::Agent {
        rows.push(notice(AGENT, Tok::Agent, "Written by an agent", format!("{} {}", date(e.date), clock(e.date))));
    }
    if let Some(b) = body.filter(|b| !b.blocked_remote.is_empty()) {
        let mut detail = count(b.blocked_remote.len(), "host");
        if b.remote_images > 0 {
            detail = format!("{}, {detail}", count(b.remote_images, "image"));
        }
        if !b.trackers.is_empty() {
            detail += &format!(" {DOT} tracker: {}", b.trackers.join(", "));
        }
        rows.push(notice(NOTICE, Tok::Warn, "Remote content not loaded", detail));
    }
    let unchecked = |what, why: &str| notice(NOTICE, Tok::Warn, what, why.to_string());
    match (check, e.security) {
        (Some(Check::Good { signer, key }), _) => {
            rows.push(notice(OK, Tok::Ok, &format!("Signed by {signer}"), format!("key {key}")));
        }
        (Some(Check::Bad { signer }), _) => {
            let detail = format!("changed after signing, or not signed by {signer}");
            rows.push(notice(BAD, Tok::Err, "Bad signature", detail));
        }
        (Some(Check::UnknownKey { key }), _) => {
            rows.push(unchecked("Signature not checked", &format!("gpg has no key {key}")));
        }
        (Some(Check::Decrypted { key }), _) => rows.push(notice(OK, Tok::Ok, "Encrypted to you", format!("key {key}"))),
        (None, Security::Signed) => rows.push(unchecked("Signature not checked", "tuimex does not verify one yet")),
        (None, Security::Encrypted | Security::Both) => rows.push(unchecked("Encrypted", "tuimex cannot open it yet")),
        (None, Security::None) => {}
    }
    if let Some(was) = &msg.sender_was {
        let detail = format!("earlier messages came from {was}");
        rows.push(notice(NOTICE, Tok::Warn, "Sender differs from the thread", detail));
    }
    rows
}

/// From, To, Cc, Date and Files, then the notices after a blank row, all
/// within `width` and none of it cut: a value wraps under itself, and a
/// notice under its own text, clear of its mark. `brief` leaves out what
/// the list row already says and keeps the lists short.
pub(crate) fn header(msg: &Listed, body: Option<&Body>, check: Option<&Check>, brief: bool, width: usize) -> Vec<Line> {
    let e = &msg.envelope;
    let (room, most) = (width.saturating_sub(LABEL), if brief { ROWS.0 } else { ROWS.1 });
    let list = |label: &str, who: &[Address]| {
        let rows = field(label, packed(who.iter().map(person).collect(), room, most));
        if rows.is_empty() { field(label, vec![Line::new()]) } else { rows }
    };
    let mut rows: Vec<Line> = Vec::new();
    if let Some(from) = e.from.first() {
        // An address with no name is the name, and is said once.
        let value = match from.name.as_deref().filter(|n| !n.is_empty()) {
            Some(name) => vec![run(name, Tok::Fg).bold(), gap(1), run(format!("<{}>", from.email), Tok::Dim)],
            None => vec![run(from.email.as_str(), Tok::Fg).bold()],
        };
        rows.extend(field("From", wrap_runs(&value, room)));
    }
    rows.extend(list("To", &e.to));
    if !e.cc.is_empty() {
        rows.extend(list("Cc", &e.cc));
    }
    if !brief {
        rows.extend(field("Date", vec![vec![run(format!("{} {}", long_date(e.date), clock(e.date)), Tok::Fg)]]));
    }
    let attached = body.map_or(&msg.attachments, |b| &b.attachments);
    if !attached.is_empty() {
        let file =
            |a: &tuimex_core::Attachment| vec![run(format!("{} ", a.name), Tok::Fg), run(size_text(a.size), Tok::Dim)];
        rows.extend(field("Files", packed(attached.iter().map(file).collect(), room, most)));
    }
    let notices = notices(msg, body, check);
    if !notices.is_empty() {
        rows.push(Line::new());
        rows.extend(notices.iter().flat_map(|notice| notice_lines(notice, width)));
    }
    rows
}

/// The header block and the opening of the body, indented: what the
/// preview shows whole and the reader shows under its title. The body is
/// what was read of the message, or failing that the list's snippet.
pub(crate) fn block(
    msg: &Listed,
    body: Option<&Body>,
    check: Option<&Check>,
    me: &[String],
    width: usize,
    brief: bool,
    account: Tok,
) -> (Vec<Line>, Vec<Line>, Vec<Placed>) {
    let indent = |row: Line| if row.is_empty() { row } else { [vec![gap(INDENT)], row].concat() };
    let inner = width.saturating_sub(INDENT + MARGIN);
    let head =
        header(msg, body, check, brief, inner).into_iter().map(|row| fit(&indent(row), width.saturating_sub(MARGIN)));
    let shown = body.map(|b| b.text.as_str()).or(msg.text.as_deref()).unwrap_or(&msg.snippet);
    // The card is the reader's: the preview is the message's own text.
    let card = body.filter(|_| !brief).and_then(|b| b.invite.as_ref());
    let came_to = (msg.envelope.id.account.0.as_str(), account);
    let mut rows = card.map_or_else(Vec::new, |card| text::invite(card, me, came_to));
    let (lines, mut placed) = text::rows(shown, measure(width), body);
    placed.iter_mut().for_each(|p| p.row += rows.len());
    rows.extend(lines);
    (head.collect(), rows.into_iter().map(indent).collect(), placed)
}

/// Everything the reader shows; the design's `readerScreen` options.
pub struct Reader<'a> {
    pub msg: &'a Listed,
    pub body: Option<&'a Body>,
    pub check: Option<&'a Check>,
    /// The addresses mail arrives at, which an invitation calls "you".
    pub me: &'a [String],
    /// The colour of the message's account, which an invitation's card
    /// names.
    pub account: Tok,
    /// Which message of how many in its thread this is, counted from 1.
    pub place: (usize, usize),
    /// How many lines of the body are scrolled off the top.
    pub top: usize,
    /// What `list::panel` made, and any lines under it.
    pub panel: Vec<Line>,
    /// What can be pressed in `panel`, counted from its first line.
    pub panel_spots: Vec<Spot>,
    /// The left of the status line, in place of the scroll position.
    pub left: Option<Line>,
    pub right: Line,
    /// What the command line holds; the hint for the message when idle.
    pub command: Option<Line>,
}

/// What `screen` drew beside the lines: where the body starts, how far it
/// can scroll, how many of its lines show, and the pictures in sight.
pub struct Drawn {
    pub lines: Vec<Line>,
    pub top: usize,
    pub last_top: usize,
    pub room: usize,
    /// Each picture's place on the screen: `row` and `col` count from the
    /// screen's first cell, and one partly scrolled off is left out.
    pub pictures: Vec<Placed>,
    /// Each `[n]` of the body in sight, and what the panel has.
    pub spots: Vec<Spot>,
}

/// Where `line`, on `row`, has a link's number: the `[n]` of `references`.
fn numbers(row: usize, line: &[Span], width: usize) -> Vec<Spot> {
    let number = |span: &Span| span.text.strip_prefix('[')?.strip_suffix(']')?.parse().ok();
    let on = line.iter().enumerate().filter(|(_, span)| span.fg == Tok::Link);
    let spots = on.filter_map(|(at, span)| Some(Spot::over(row, &line[..at], span, Target::Reference(number(span)?))));
    spots.filter(|spot| spot.cols.end <= width).collect()
}

fn reader_hint(o: &Reader) -> Line {
    let mut extra = Vec::new();
    let has = |f: fn(&Body) -> bool| o.body.is_some_and(f);
    let files = o.body.map_or(&o.msg.attachments, |b| &b.attachments);
    for (id, on) in [
        ("reader.invite", has(|b| b.invite.is_some())),
        ("reader.thread", o.place.1 > 1),
        ("reader.attachments", !files.is_empty()),
        ("reader.links", has(|b| !b.links.is_empty())),
        ("reader.browser", has(|b| !b.blocked_remote.is_empty())),
    ] {
        if on && extra.len() < 3 {
            extra.push(keymap::hint(id));
        }
    }
    hint(
        &[vec![keymap::hint("list.reply"), keymap::hint("list.archive")], extra, vec![keymap::hint("global.back")]]
            .concat(),
    )
}

/// The whole screen: the subject, the header block, as much of the body as
/// fits, the panel, a blank row, the status line and the command line.
pub fn screen(o: &Reader, width: usize, height: usize) -> Drawn {
    let place = if o.place.1 > 1 { format!("{} of {} in the thread", o.place.0, o.place.1) } else { String::new() };
    let title = left_right(
        &heading(vec![run(o.msg.envelope.subject.as_str(), Tok::Fg).bold()]),
        &[gap(2), run(place, Tok::Dim), gap(MARGIN)],
        width,
    );
    let (head, body, placed) = block(o.msg, o.body, o.check, o.me, width, false, o.account);
    let mut lines = vec![title, Line::new()];
    lines.extend(head);
    lines.push(Line::new());

    let room = height.saturating_sub(lines.len() + o.panel.len() + 3);
    let last_top = body.len().saturating_sub(room);
    let top = o.top.min(last_top);
    let last = body.len().min(top + room);
    let scroll = if body.len() <= room {
        Line::new()
    } else {
        vec![run(format!("lines {}-{last} of {}", top + 1, body.len()), Tok::Dim)]
    };
    let head_rows = lines.len();
    let pictures = placed.into_iter().filter(|p| p.row >= top && p.row + p.height <= last);
    let pictures = pictures.map(|p| Placed { row: p.row - top + head_rows, col: INDENT, ..p }).collect();

    let mut middle = body[top..last].to_vec();
    while middle.last().is_some_and(|line| cells(line) == 0) {
        middle.pop();
    }
    middle.resize(room, Line::new());
    let mut spots: Vec<Spot> =
        middle.iter().enumerate().flat_map(|(at, line)| numbers(head_rows + at, line, width)).collect();
    lines.extend(middle);
    spots.extend(o.panel_spots.iter().map(|spot| spot.clone().moved(lines.len(), 0)));
    lines.extend(o.panel.iter().cloned());
    let command = o.command.clone().unwrap_or_else(|| reader_hint(o));
    lines.extend([Line::new(), status_row(width, o.left.as_ref().unwrap_or(&scroll), &o.right), command]);
    Drawn { lines, top, last_top, room, pictures, spots }
}

#[cfg(test)]
mod tests;

//! The reader's text as rows: the design's `bodyRows`, `imageRows` and
//! `inviteRows`. A message arrives as plain text (ADR-017), so what the
//! design styles is told from how a line begins: `# ` a heading, `> ` a
//! quote, `* ` a list item, `[image: …]` a picture, tabs a table's cells,
//! and a rule the start of the small print.

use tuimex_core::{Body, EventTime, Invite, Picture};

use crate::glyphs::{BAR, DOT, SHADE};
use crate::palette::Tok;
use crate::span::{Line, gap, run, wrap};
use crate::when::{clock, long_date};

/// The most cells a picture takes: the design's `cells` for one.
const PICTURE: (usize, usize) = (48, 12);

/// Where a picture's cells are, for whoever can draw more than the
/// placeholder there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    /// The picture's place in `Body::attachments`.
    pub index: usize,
    pub row: usize,
    pub col: usize,
    pub width: usize,
    pub height: usize,
}

fn is_rule(line: &str) -> bool {
    let line = line.trim();
    line.chars().count() >= 3 && line.chars().all(|c| matches!(c, '-' | '_' | '─' | '━'))
}

/// A table as rows: each column as wide as its widest cell and three
/// more, the first row dim.
fn table(rows: &[&str], measure: usize) -> Vec<Line> {
    let cells: Vec<Vec<&str>> = rows.iter().map(|row| row.split('\t').map(str::trim).collect()).collect();
    let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
    let width = |col: usize| cells.iter().filter_map(|row| row.get(col)).map(|c| c.chars().count()).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns).map(width).collect();
    let tone = |i: usize| if i == 0 { Tok::Dim } else { Tok::Fg };
    let line = |(i, row): (usize, &Vec<&str>)| {
        let text: String = row.iter().zip(&widths).map(|(cell, w)| format!("{cell:<width$}", width = w + 3)).collect();
        vec![run(text, tone(i))]
    };
    // Columns that do not fit are given up for all the rows: each is its
    // cells in a line, parted by dots, wrapped under itself.
    let dotted = |(i, row): (usize, &Vec<&str>)| {
        let wrapped = wrap(&row.join(&format!(" {DOT} ")), measure).into_iter().enumerate();
        wrapped.map(move |(n, l)| [vec![gap(if n == 0 { 0 } else { 2 })], references(&l, tone(i))].concat())
    };
    match (widths.iter().map(|w| w + 3).sum::<usize>()).saturating_sub(3) <= measure {
        true => cells.iter().enumerate().map(line).collect(),
        false => cells.iter().enumerate().flat_map(dotted).collect(),
    }
}

/// The cells a picture takes: the placeholder a terminal that draws no
/// pictures shows, with the file's name and size in its middle row.
fn picture(name: &str, p: &Picture, width: usize, height: usize) -> Vec<Line> {
    let shade = |n: usize| run(SHADE.to_string().repeat(n), Tok::Faint);
    let mut rows = vec![vec![shade(width)]; height];
    // A placeholder narrower than its label keeps as much of it as fits.
    let label: String = format!(" {name}  {}x{} ", p.width, p.height).chars().take(width).collect();
    let len = label.chars().count();
    let before = (width - len) / 2;
    rows[height / 2] = vec![shade(before), run(label, Tok::Dim), shade(width - len - before)];
    rows
}

/// A line with each `[n]` in the colour of a link's number, which is what
/// the list of links calls it by.
pub(crate) fn references(line: &str, tone: Tok) -> Line {
    let mut out = Line::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let digits = rest[open + 1..].bytes().take_while(u8::is_ascii_digit).count();
        let end = open + 1 + digits;
        if digits > 0 && rest[end..].starts_with(']') {
            out.extend([run(&rest[..open], tone), run(&rest[open..=end], Tok::Link)]);
            rest = &rest[end + 1..];
        } else {
            out.push(run(&rest[..=open], tone));
            rest = &rest[open + 1..];
        }
    }
    out.push(run(rest, tone));
    out.retain(|span| !span.text.is_empty());
    out
}

/// `text` as the reader's rows, wrapped at `measure`, and where each
/// picture of `body` was put. A picture the text does not name by its
/// file's name follows the text.
pub(crate) fn rows(text: &str, measure: usize, body: Option<&Body>) -> (Vec<Line>, Vec<Placed>) {
    let lines: Vec<&str> = text.lines().collect();
    let small_from = lines.iter().rposition(|line| is_rule(line));
    let pictures: Vec<(&str, &Picture)> = body.map_or_else(Vec::new, |b| {
        let name = |p: &Picture| b.attachments.iter().find(|a| a.index == p.index).map(|a| a.name.as_str());
        b.pictures.iter().filter_map(|p| Some((name(p)?, p))).collect()
    });
    let (mut out, mut placed): (Vec<Line>, Vec<Placed>) = (Vec::new(), Vec::new());
    let place = |out: &mut Vec<Line>, placed: &mut Vec<Placed>, name: &str, p: &Picture| {
        let width = PICTURE.0.min(measure).max(1);
        // A cell is about twice as tall as it is wide.
        let tall = width as u64 * u64::from(p.height) / (2 * u64::from(p.width.max(1)));
        let height = (tall as usize).clamp(1, PICTURE.1);
        placed.push(Placed { index: p.index, row: out.len(), col: 0, width, height });
        out.extend(picture(name, p, width, height));
    };
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        let tone = if small_from.is_some_and(|rule| i > rule) { Tok::Dim } else { Tok::Fg };
        let cells = lines[i..].iter().take_while(|l| l.contains('\t')).count();
        if is_rule(line) {
            // The rule goes, and the blank line after it with it.
            i += 1 + usize::from(lines.get(i + 1).is_some_and(|next| next.is_empty()));
            continue;
        } else if cells > 1 {
            out.extend(table(&lines[i..i + cells], measure));
            i += cells;
            continue;
        }
        // A control character has no cell, so a tab left in would join the
        // words on either side of it.
        let line = line.replace('\t', "    ");
        let marks = line.len() - line.trim_start_matches('#').len();
        let alt = line.strip_prefix("[image: ").and_then(|rest| rest.strip_suffix(']'));
        if let Some((name, p)) = alt.and_then(|alt| pictures.iter().find(|(name, _)| *name == alt)) {
            place(&mut out, &mut placed, name, p);
        } else if alt.is_some() {
            out.extend(wrap(&line, measure).into_iter().map(|l| vec![run(l, Tok::Dim)]));
        } else if let Some(heading) = line[marks..].strip_prefix(' ').filter(|_| (1..=6).contains(&marks)) {
            out.extend(wrap(heading, measure).into_iter().map(|l| vec![run(l, Tok::Fg).bold()]));
        } else if let Some(quote) = line.strip_prefix("> ") {
            let quoted = wrap(quote, measure.saturating_sub(2)).into_iter();
            out.extend(quoted.map(|l| vec![run(format!("{BAR} "), Tok::Dim), run(l, Tok::Dim)]));
        } else if let Some(item) = line.strip_prefix("* ") {
            let wrapped = wrap(item, measure.saturating_sub(2)).into_iter().enumerate();
            out.extend(wrapped.map(|(n, l)| {
                vec![run(if n == 0 { format!("{DOT} ") } else { "  ".to_string() }, Tok::Dim), run(l, tone)]
            }));
        } else if line.is_empty() {
            out.push(Line::new());
        } else {
            out.extend(wrap(&line, measure).iter().map(|l| references(l, tone)));
        }
        i += 1;
    }
    for (name, p) in &pictures {
        if !placed.iter().any(|at| at.index == p.index) {
            out.push(Line::new());
            place(&mut out, &mut placed, name, p);
        }
    }
    (out, placed)
}

/// When an invitation says it happens. Only `At` is an instant; the others
/// are shown as written, with the zone they named.
fn moment(time: &EventTime) -> (String, String) {
    match time {
        EventTime::At(t) => (long_date(*t), clock(*t)),
        EventTime::Local { wall, zone } => {
            let zone = zone.as_deref().map_or_else(String::new, |z| format!(" {z}"));
            (long_date(*wall), format!("{}{zone}", clock(*wall)))
        }
        EventTime::Day(t) => (long_date(*t), String::new()),
    }
}

/// The card of ADR-021: what, when, where and who, then a blank row.
pub(crate) fn invite(card: &Invite, me: &[String]) -> Vec<Line> {
    let field = |label: &str, value: crate::span::Span| vec![run(format!("{label:<9}"), Tok::Dim), value];
    let place = |place: &str| [vec![run(format!("{:<9}", "Where"), Tok::Dim)], references(place, Tok::Fg)].concat();
    let (day, time) = moment(&card.start);
    let mut when = if time.is_empty() { day.clone() } else { format!("{day}, {time}") };
    if let Some((end_day, end_time)) = card.end.as_ref().map(moment) {
        when += &match (end_day == day, end_time.is_empty()) {
            (true, false) => format!(" to {end_time}"),
            (false, false) => format!(" to {end_day}, {end_time}"),
            (false, true) => format!(" to {end_day}"),
            (true, true) => String::new(),
        };
    }
    let mine = |email: &str| me.iter().any(|m| m.eq_ignore_ascii_case(email));
    let invited = card.attendees.iter().filter(|a| card.organizer.as_ref().is_none_or(|o| o.email != a.email));
    let (you, others): (Vec<_>, Vec<_>) = invited.partition(|a| mine(&a.email));
    let mut who: Vec<String> =
        card.organizer.iter().map(|o| o.name.clone().unwrap_or_else(|| o.email.clone())).collect();
    who.extend((!you.is_empty()).then(|| "you".to_string()));
    let mut who = who.join(", ");
    if !others.is_empty() {
        let n = others.len();
        who += &format!("{}{n} other{}", if who.is_empty() { "" } else { " and " }, if n == 1 { "" } else { "s" });
    }
    let mut rows = vec![field("What", run(card.summary.as_str(), Tok::Fg).bold()), field("When", run(when, Tok::Fg))];
    rows.extend(card.location.as_deref().map(place));
    rows.push(field("Who", run(who, Tok::Fg)));
    rows.extend(card.recurrence.as_deref().map(|rule| field("Repeats", run(rule, Tok::Fg))));
    rows.push(Line::new());
    rows
}

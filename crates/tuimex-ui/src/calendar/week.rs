//! The week view: the design's `weekScreen`, days as columns against the
//! clock. An event is a bar in its account's colour down the rows it
//! lasts, its title beside the first row, its hours beside the second and
//! what is most not normal about it beside the third. One that begins
//! while another runs is drawn `CAL_STEP` cells in, over the other's rows,
//! so both titles stay whole and the first one's bar still shows how long
//! it lasts; two that begin together halve the column.

use tuimex_core::calendar::{DAY, civil_from_days, weekday_of};
use unicode_width::UnicodeWidthChar;

use super::Entry;
use super::parts::{self, MONTH_NAMES, Scene, clock, event_words};
use crate::glyphs::{BAR, CURSOR};
use crate::layout::{CAL_AXIS, CAL_DAY_FROM, CAL_SLOT, CAL_STEP, CAL_WEEK_FROM, MARGIN};
use crate::palette::Tok;
use crate::span::{Line, Span, fill, fit, gap, run};
use crate::when::{DAYS, MONTHS};

#[cfg(test)]
mod tests;

/// Minutes in a day.
const CLOCK: i64 = 24 * 60;

/// How many days the week shows at `width`, and how wide each is.
pub(crate) fn columns(width: usize) -> (usize, usize) {
    let count = if width >= CAL_WEEK_FROM { 7 } else { 3 };
    (count, width.saturating_sub(MARGIN * 2 + CAL_AXIS) / count)
}

/// The first day shown: the cursor's Monday for a whole week, else the
/// day before the cursor, so the cursor is in the middle.
pub(crate) fn first_day(cursor: i64, count: usize) -> i64 {
    match count {
        7 => cursor - (weekday_of(cursor) + 6) % 7,
        _ => cursor - 1,
    }
}

fn minutes(clock: &str) -> i64 {
    let (hours, mins) = clock.split_once(':').unwrap_or((clock, "0"));
    hours.parse::<i64>().unwrap_or(0) * 60 + mins.parse::<i64>().unwrap_or(0)
}

/// The minutes of `day` an entry covers, from the start of its first row
/// to its end, and never less than one row. What began the day before
/// starts at midnight and what runs into the next day ends there, so an
/// event over midnight is drawn on each day for its part of it.
pub(crate) fn rows_of(entry: &Entry, day: i64) -> (i64, i64) {
    let midnight = day * DAY;
    let start = (entry.start - midnight).div_euclid(60).clamp(0, CLOCK);
    let end = (entry.end - midnight).div_euclid(60).clamp(0, CLOCK);
    let from = start / CAL_SLOT * CAL_SLOT;
    (from, end.max(from + CAL_SLOT))
}

/// Where an event sits in its day's column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lane {
    pub lane: usize,
    /// Another event of the day overlaps it.
    pub shares: bool,
    /// One of those begins at the same time, so the two are side by side.
    pub abreast: bool,
}

/// The lane of each of a day's timed events, given in order of their
/// start as `(from, to)` minutes, and how many lanes the day needs. An
/// event takes the first lane free by the time it begins.
pub(crate) fn lanes(spans: &[(i64, i64)]) -> (Vec<Lane>, usize) {
    let mut free: Vec<i64> = Vec::new();
    let placed = spans
        .iter()
        .enumerate()
        .map(|(i, &(from, to))| {
            let lane = free.iter().position(|&end| end <= from).unwrap_or(free.len());
            match free.get_mut(lane) {
                Some(end) => *end = to,
                None => free.push(to),
            }
            let meets: Vec<&(i64, i64)> =
                spans.iter().enumerate().filter(|&(j, o)| j != i && o.0 < to && from < o.1).map(|(_, o)| o).collect();
            Lane { lane, shares: !meets.is_empty(), abreast: meets.iter().any(|o| o.0 == from) }
        })
        .collect();
    (placed, free.len().max(1))
}

/// The minute the window of `slots` rows begins at: `CAL_DAY_FROM`, moved
/// on to the hour after the selected event ends when it ends after the
/// window, and back to the hour it begins in when it begins before. A
/// window never runs past the day.
pub(crate) fn window(slots: usize, picked: Option<(i64, i64)>) -> i64 {
    let span = i64::try_from(slots).unwrap_or(i64::MAX / 2).saturating_mul(CAL_SLOT);
    let mut from = minutes(CAL_DAY_FROM);
    if let Some((start, end)) = picked {
        if end > from + span {
            from = (end + 59).div_euclid(60) * 60 - span;
        }
        if start < from {
            from = start.div_euclid(60) * 60;
        }
    }
    from.min(CLOCK - span).max(0)
}

/// A line as its cells. The second cell of a wide character is a span
/// with no text, so an index is always a cell.
fn explode(line: &[Span]) -> Line {
    let mut out: Line = Vec::new();
    for span in line {
        for c in span.text.chars() {
            match (c.width().unwrap_or(0), out.last_mut()) {
                (0, Some(last)) => last.text.push(c),
                (0, None) => {}
                (w, _) => {
                    out.push(Span { text: c.to_string(), ..span.clone() });
                    if w > 1 {
                        out.push(Span { text: String::new(), ..span.clone() });
                    }
                }
            }
        }
    }
    out
}

/// `top` written over `base` from cell `x` on. A wide character with one
/// of its halves written over becomes a space, so no cell is lost.
fn overlay(base: &mut Line, x: usize, top: &[Span]) {
    for (i, cell) in explode(top).into_iter().enumerate() {
        if let Some(under) = base.get_mut(x + i) {
            *under = cell;
        }
    }
    for i in 0..base.len() {
        let wide = base[i].text.chars().next().and_then(|c| c.width()).unwrap_or(0) > 1;
        let half = base[i].text.is_empty();
        let next_half = base.get(i + 1).is_some_and(|next| next.text.is_empty());
        let after_wide = i > 0 && base[i - 1].text.chars().next().and_then(|c| c.width()).unwrap_or(0) > 1;
        if (wide && !next_half) || (half && !after_wide) {
            base[i] = Span { text: " ".to_string(), ..base[i].clone() };
        }
    }
}

/// Row `y` of an event's block, `w` cells wide.
fn block(o: &Scene, entry: &Entry, y: i64, w: usize) -> Line {
    let selected = o.picked.is_some_and(|picked| entry.is(picked));
    let lead = if selected { run(CURSOR, Tok::Acc) } else { run(BAR, o.colour_of(entry.account)) };
    let text = match y {
        0 => {
            let tone = if o.ended(entry) { Tok::Dim } else { Tok::Fg };
            vec![Span { bold: entry.waiting(), ..run(entry.event.summary.as_str(), tone) }]
        }
        1 if !entry.all_day => vec![run(format!("{}-{}", clock(entry.start), clock(entry.end)), Tok::Dim)],
        2 => event_words(entry).into_iter().take(1).map(|(word, own)| run(word, own.unwrap_or(Tok::Dim))).collect(),
        _ => Vec::new(),
    };
    let mut row = vec![lead];
    row.extend(fit(&text, w.saturating_sub(2)));
    row.push(gap(1));
    if selected { fill(row, Tok::Sel) } else { row }
}

/// A timed event of one day, with the rows and the lane it takes.
struct Placed<'a> {
    entry: Entry<'a>,
    from: i64,
    to: i64,
    lane: Lane,
}

/// The whole screen.
pub fn screen(o: &Scene, width: usize, height: usize) -> Vec<Line> {
    let room = o.room(height);
    let (count, col) = columns(width);
    let first = first_day(o.cursor, count);
    let shown: Vec<i64> = (first..first + count as i64).collect();
    let days = o.days(first, first + count as i64);
    let today = o.calendar.today();

    let mut head = vec![gap(MARGIN + CAL_AXIS)];
    for &day in &shown {
        let (_, _, dom) = civil_from_days(day);
        let label = format!("{} {dom}", DAYS[weekday_of(day) as usize]);
        let tone = if day == today {
            Tok::Acc
        } else if day == o.cursor {
            Tok::Fg
        } else {
            Tok::Dim
        };
        let label = Span { bold: day == today || day == o.cursor, ..run(label, tone) };
        head.extend(fit(&[gap(1), label], col));
    }

    let all_day: Vec<Vec<&Entry>> =
        shown.iter().map(|&day| days.on(day).iter().filter(|entry| entry.all_day).collect()).collect();
    let most = all_day.iter().map(Vec::len).max().unwrap_or(0);
    let all_rows = (0..most).map(|y| {
        let mut row = vec![gap(MARGIN)];
        row.extend(fit(&if y == 0 { vec![run("all day", Tok::Dim)] } else { Vec::new() }, CAL_AXIS));
        for list in &all_day {
            match list.get(y) {
                Some(entry) => row.extend(block(o, entry, 0, col)),
                None => row.push(gap(col)),
            }
        }
        row
    });
    let all_rows: Vec<Line> = all_rows.collect();

    let timed: Vec<(Vec<Placed>, usize)> = shown
        .iter()
        .map(|&day| {
            let list: Vec<(Entry, (i64, i64))> = days
                .on(day)
                .iter()
                .filter(|entry| !entry.all_day)
                .map(|entry| (entry.clone(), rows_of(entry, day)))
                .collect();
            let (placed, lanes) = lanes(&list.iter().map(|(_, span)| *span).collect::<Vec<_>>());
            let list = list.into_iter().zip(placed).map(|((entry, (from, to)), lane)| Placed { entry, from, to, lane });
            (list.collect(), lanes)
        })
        .collect();

    let slots = room.saturating_sub(1 + most + usize::from(most > 0));
    let picked = o.picked.and_then(|picked| timed.iter().flat_map(|(list, _)| list).find(|p| p.entry.is(picked)));
    let from = window(slots, picked.map(|p| (p.from, p.to)));
    let now = shown.contains(&today).then(|| o.calendar.wall().rem_euclid(DAY) / 3600 * 60);

    let grid = (0..slots as i64).map(|s| {
        let at = from + s * CAL_SLOT;
        let label = match at % 60 {
            0 => {
                let here = now == Some(at);
                vec![Span { bold: here, ..run(format!("{:02}:00", at / 60), if here { Tok::Acc } else { Tok::Dim }) }]
            }
            _ => Vec::new(),
        };
        let mut row = vec![gap(MARGIN)];
        row.extend(fit(&label, CAL_AXIS));
        for (list, lanes) in &timed {
            let mut cell = explode(&[gap(col)]);
            for p in list.iter().filter(|p| p.from <= at && at < p.to) {
                let half = col / lanes;
                let x = match (p.lane.shares, p.lane.abreast) {
                    (false, _) => 0,
                    (true, true) => p.lane.lane * half,
                    (true, false) => p.lane.lane * CAL_STEP,
                };
                let w = if p.lane.abreast { half } else { col.saturating_sub(x) };
                let rows = block(o, &p.entry, (at - p.from) / CAL_SLOT, w);
                // A blank cell before a block drawn over another parts the two.
                match x > 0 && !p.lane.abreast {
                    true => overlay(&mut cell, x - 1, &[vec![gap(1)], rows].concat()),
                    false => overlay(&mut cell, x, &rows),
                }
            }
            row.extend(cell);
        }
        row
    });

    let (a, b) = (civil_from_days(first), civil_from_days(first + count as i64 - 1));
    let title = match a.1 == b.1 {
        true => format!("{} to {} {} {}", a.2, b.2, MONTH_NAMES[(b.1 - 1) as usize], b.0),
        false => format!("{} {} to {} {} {}", a.2, MONTHS[(a.1 - 1) as usize], b.2, MONTHS[(b.1 - 1) as usize], b.0),
    };
    let mut content = vec![head];
    content.extend(all_rows.iter().cloned());
    if most > 0 {
        content.push(Line::new());
    }
    content.extend(grid);
    let content = content.iter().map(|row| fit(row, width)).collect();
    parts::frame(o, width, &title, content, parts::day_status(o, o.cursor), parts::hint(o.entry().as_ref()))
}

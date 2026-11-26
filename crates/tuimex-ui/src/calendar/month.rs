//! The month view: the design's `monthRows`, `monthsColumn` and
//! `monthScreen`. The cursor's month and as many of the months after it as
//! fit whole, and beside them the cursor's day and the days that follow.
//! The focus is in the grid or, with `Scene::focus`, in that list, which
//! then begins on `Scene::listed_from`.

use tuimex_core::calendar::{civil_from_days, days_from_civil, days_in_month, weekday_of};

use super::Days;
use super::parts::{self, Drawn, MONTH_NAMES, Pressed, Scene};
use crate::glyphs::{BAR, CURSOR, FEW, MANY};
use crate::keymap;
use crate::layout::{CAL_CELL, INDENT, MARGIN};
use crate::list;
use crate::palette::Tok;
use crate::span::{Line, Span, cells, fill, fit, gap, run};
use crate::spot::{Spot, Target};

const WEEK: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
/// The most months the column ever holds.
const MOST: i64 = 12;

/// The year and the month `months` after those of `day`.
fn month_after(day: i64, months: i64) -> (i64, i64) {
    let (year, month, _) = civil_from_days(day);
    let count = year * 12 + month - 1 + months;
    (count.div_euclid(12), count.rem_euclid(12) + 1)
}

/// How many rows a month takes: its name, the weekdays, and its weeks.
fn rows_of(year: i64, month: i64) -> usize {
    let lead = (weekday_of(days_from_civil(year, month, 1)) + 6) % 7;
    2 + usize::try_from((lead + days_in_month(year, month) + 6) / 7).unwrap_or(0)
}

/// One month, seven days wide. A day is bright when something is on it and
/// the accent when it is today; its mark says how much (one, two, more)
/// and is the warning colour while an invitation on it waits for an answer.
/// Beside the rows are the cells of each day.
fn month_rows(o: &Scene, days: &Days, year: i64, month: i64, current: bool) -> (Vec<Line>, Vec<Spot>) {
    let first = days_from_civil(year, month, 1);
    let (length, lead) = (days_in_month(year, month), (weekday_of(first) + 6) % 7);
    let title = format!("{} {year}", MONTH_NAMES[(month - 1) as usize]);
    let name = Span { bold: current, ..run(title, if current { Tok::Fg } else { Tok::Dim }) };
    let mut rows =
        vec![vec![gap(1), name], WEEK.iter().flat_map(|day| [gap(1), run(*day, Tok::Dim), gap(1)]).collect()];
    let today = o.calendar.today();
    let cell = |dom: i64| -> Line {
        if dom < 1 || dom > length {
            return vec![gap(CAL_CELL)];
        }
        let day = first + dom - 1;
        let entries = days.on(day);
        let mark = match entries.len() {
            0 => gap(1),
            n => {
                let much = [FEW, ':'].get(n - 1).copied().unwrap_or(MANY);
                run(much, if entries.iter().any(|entry| entry.waiting()) { Tok::Warn } else { Tok::Dim })
            }
        };
        let number = match (day == today, entries.is_empty()) {
            (true, _) => run(format!("{dom:>2}"), Tok::Acc).bold(),
            (false, false) => run(format!("{dom:>2}"), Tok::Fg),
            (false, true) => run(format!("{dom:>2}"), Tok::Dim),
        };
        // The cursor is the accent where the focus is and dim where it is
        // not, as the list's is in mail while the preview has the focus.
        let cursor = run(CURSOR, if o.focus { Tok::Dim } else { Tok::Acc });
        match day == o.cursor {
            true => fill(vec![cursor, number, mark], Tok::Sel),
            false => vec![gap(1), number, mark],
        }
    };
    let (mut at, mut spots) = (-lead, Vec::new());
    while at < length {
        let mut row = Line::new();
        for dom in at + 1..=at + 7 {
            let from = cells(&row);
            row.extend(cell(dom));
            if (1..=length).contains(&dom) {
                spots.push(Spot { row: rows.len(), cols: from..cells(&row), target: Target::Day(first + dom - 1) });
            }
        }
        rows.push(row);
        at += 7;
    }
    (rows.into_iter().map(|row| fit(&row, 7 * CAL_CELL)).collect(), spots)
}

/// The cursor's month and as many of the months after it as fit whole in
/// `room` rows, a blank row between them, and the cells of their days.
fn months_column(o: &Scene, room: usize) -> (Vec<Line>, Vec<Spot>) {
    let mut fits = 0;
    let mut used = 0;
    while fits < MOST {
        let (year, month) = month_after(o.cursor, fits);
        let more = rows_of(year, month) + usize::from(fits > 0);
        if used + more > room {
            break;
        }
        (fits, used) = (fits + 1, used + more);
    }
    let (first, end) = (month_after(o.cursor, 0), month_after(o.cursor, fits));
    let days = o.days(days_from_civil(first.0, first.1, 1), days_from_civil(end.0, end.1, 1));
    let (mut rows, mut spots) = (Vec::new(), Vec::new());
    for i in 0..fits {
        let (year, month) = month_after(o.cursor, i);
        if i > 0 {
            rows.push(Line::new());
        }
        let (more, its) = month_rows(o, &days, year, month, i == 0);
        spots.extend(its.into_iter().map(|spot| spot.moved(rows.len(), 0)));
        rows.extend(more);
    }
    (rows, spots)
}

/// The whole screen. With no calendar at all the grid still moves, and
/// the list says how one is added.
pub fn screen(o: &Scene, width: usize, height: usize) -> Drawn {
    let room = o.room(height);
    let (months, days) = months_column(o, room);
    let left_width = INDENT + 7 * CAL_CELL + MARGIN;
    let pane_width = width.saturating_sub(left_width + 1);
    let none = o.calendar.shelves.is_empty();
    let (pane, listed) = match none {
        true => (
            vec![
                Line::new(),
                vec![gap(MARGIN), run("No calendar yet.", Tok::Fg)],
                vec![gap(MARGIN), run("`tuimex setup` asks each account for the address of its calendar.", Tok::Dim)],
            ],
            Pressed::default(),
        ),
        false => parts::agenda(o, o.listed_from, room, pane_width, o.picked.filter(|_| o.focus)),
    };
    // The grid and the list are each under what is drawn on them.
    let whole =
        |cols: std::ops::Range<usize>, target| (0..room).map(move |row| Spot { row, cols: cols.clone(), target });
    let mut spots: Vec<Spot> = whole(0..left_width, Target::CalGrid).collect();
    spots.extend(days.into_iter().map(|spot| spot.moved(0, INDENT)));
    spots.extend(whole(left_width + 1..left_width + 1 + pane_width, Target::CalList));
    let listed = listed.moved(0, left_width + 1);
    spots.extend(listed.spots);
    let pressed = Pressed { spots, events: listed.events };
    let blank = Line::new();
    let content = (0..room).map(|y| {
        let month = [vec![gap(INDENT)], months.get(y).cloned().unwrap_or_default()].concat();
        let mut row = fit(&month, left_width);
        row.push(run(BAR, Tok::Faint));
        row.extend(fit(pane.get(y).unwrap_or(&blank), pane_width));
        row
    });
    let (year, month, _) = civil_from_days(o.cursor);
    let title = format!("{} {year}", MONTH_NAMES[(month - 1) as usize]);
    let (left, idle) = match none {
        true => (Line::new(), list::hint(&[keymap::hint("global.help")])),
        false => (parts::day_status(o, o.cursor), parts::hint(o.entry().filter(|_| o.focus).as_ref(), o.focus)),
    };
    parts::frame(o, width, &title, (content.collect(), pressed), left, idle)
}

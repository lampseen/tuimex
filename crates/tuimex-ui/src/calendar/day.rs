//! The day view: the design's `dayScreen` and `eventPane`. The list of mail
//! again, with events from the cursor's day on and the selected one below
//! the divider or beside the list, and with `Scene::search` what a search
//! found, under a Search tab in place of the active one.

use tuimex_core::calendar::civil_from_days;

use super::Entry;
use super::parts::{self, Scene};
use crate::glyphs::{BAR, RULE};
use crate::keymap;
use crate::layout::{self, INDENT, MARGIN, MEASURE};
use crate::list::{self, Place};
use crate::palette::Tok;
use crate::reader;
use crate::span::{Line, fit, gap, run};

/// How far either side of today a search looks, so that the count of what
/// matched is of what `n` and `N` can step to.
const SEARCHED: i64 = 366;

#[cfg(test)]
mod tests;

/// The selected event as the day view shows it beside or under the list:
/// its title, then its fields and its notes, in exactly `height` rows of
/// `width` cells.
fn pane(o: &Scene, entry: &Entry, width: usize, height: usize) -> Vec<Line> {
    let inner = width.saturating_sub(INDENT + MARGIN);
    let mut body = parts::event_fields(o, entry, true, inner);
    if let Some(notes) = entry.event.description.as_deref().filter(|notes| !notes.is_empty()) {
        body.push(Line::new());
        body.extend(reader::text::rows(notes, MEASURE.min(inner), None).0);
    }
    let mut rows = vec![vec![gap(MARGIN), run(entry.event.summary.as_str(), Tok::Fg).bold()], Line::new()];
    rows.extend(body.into_iter().map(|row| [vec![gap(INDENT)], row].concat()));
    rows.truncate(height);
    let mut rows: Vec<Line> = rows.iter().map(|row| fit(row, width)).collect();
    rows.resize(height, fit(&[], width));
    rows
}

/// How many events a search found, each counted once however often it
/// repeats.
fn found(o: &Scene) -> usize {
    let today = o.calendar.today();
    let days = o.days(today - SEARCHED, today + SEARCHED + 1);
    let mut events: Vec<(&str, bool, &str)> =
        days.all().map(|entry| (entry.account.0.as_str(), entry.local, entry.event.uid.as_str())).collect();
    events.sort_unstable();
    events.dedup();
    events.len()
}

pub fn screen(o: &Scene, width: usize, height: usize) -> parts::Drawn {
    let entry = o.entry();
    let room = o.room(height);
    let place = if entry.is_some() { list::place(o.preview, width, height) } else { Place::Off };
    // Under the list the pane gives the panel its last rows, and with fewer
    // than its title and one line left it is not drawn, as mail's preview.
    let under = layout::preview_height(height).saturating_sub(o.panel.len());
    let place = match place {
        Place::Below if under < 4 || room <= under => Place::Off,
        place => place,
    };
    let selected = entry.as_ref().map(Entry::pick);
    let content = match (place, &entry) {
        (Place::Below, Some(entry)) => {
            let (mut content, pressed) = parts::agenda(o, o.cursor, room - under, width, selected.as_ref());
            content.push(vec![run(RULE.to_string().repeat(width), Tok::Faint)]);
            content.extend(pane(o, entry, width, under - 1));
            (content, pressed)
        }
        (Place::Right, Some(entry)) => {
            let pane_width = layout::preview_width(width);
            let list_width = width.saturating_sub(pane_width + 1);
            let (list, pressed) = parts::agenda(o, o.cursor, room, list_width, selected.as_ref());
            let pane = pane(o, entry, pane_width, room);
            let beside = |(row, beside): (Line, Line)| [row, vec![run(BAR, Tok::Faint)], beside].concat();
            (list.into_iter().zip(pane).map(beside).collect(), pressed)
        }
        _ => parts::agenda(o, o.cursor, room, width, selected.as_ref()),
    };

    let (year, _, _) = civil_from_days(o.cursor);
    let (title, left, idle) = match o.search {
        Some(query) => {
            let n = found(o);
            let said = if n == 1 { "event matches" } else { "events match" };
            let ids = ["calendar.open", "calendar.match", "global.back", "global.help"];
            let keys = list::hint(&ids.iter().map(|id| keymap::hint(id)).collect::<Vec<_>>());
            (String::new(), vec![run(format!("{n} {said} \"{query}\""), Tok::Dim)], keys)
        }
        None => {
            let day = o.picked.map_or(o.cursor, |picked| picked.day);
            (
                format!("{} {year}", parts::day_long(o.cursor)),
                parts::day_status(o, day),
                parts::hint(entry.as_ref(), false),
            )
        }
    };
    parts::frame(o, width, &title, content, left, idle)
}

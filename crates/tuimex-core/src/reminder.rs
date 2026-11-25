//! Which reminders fall due between two instants (ADR-058). The caller
//! asks again and again, each time from the instant it last asked up to
//! now, and is given each reminder exactly once.
//!
//! Instants here are seconds since the epoch. Each occurrence's start is
//! worked out from the zone its event was written in, not from the
//! reader's clock, so an occurrence in the hour a clock change repeats is
//! still one instant.

use std::collections::BTreeMap;

use crate::calendar::{DAY, Event, Moment, Repeat, Zone, occurrences};

#[cfg(test)]
mod tests;

/// How late a reminder may still be raised. A longer gap since the last
/// look means tuimex was not running, or not looking, and what fell due
/// then is news nobody can act on: only what fell due in the last five
/// minutes, for an event that has not begun, is raised.
pub const LATE: i64 = 5 * 60;

/// An alarm further from its start than this is not one a person set,
/// and is not worked out: it would only make the window searched for
/// occurrences as wide as a hostile calendar likes.
pub const FARTHEST: i64 = 366 * DAY;

/// One reminder that fell due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    pub uid: String,
    /// The event's title, as the calendar wrote it: hostile text.
    pub summary: String,
    /// The occurrence's start, an instant.
    pub start: i64,
    /// When the reminder fell due, an instant.
    pub due: i64,
    /// Seconds from now until the start; less than zero once it has begun.
    pub until: i64,
}

/// What the rule is asked about: the events held, the zones they name, the
/// reader's zone, and the lead an event with no alarm of its own is given.
pub struct Held<'a> {
    pub events: &'a [Event],
    pub zones: &'a BTreeMap<String, Zone>,
    pub local: &'a Zone,
    /// Seconds before the start.
    pub lead: i64,
}

/// The reminders that fell due after `checked` and up to and including
/// `now`, earliest first. An event `declined` says yes to has none; who
/// the reader is, is the caller's to know.
pub fn due(held: &Held, declined: &dyn Fn(&Event) -> bool, checked: i64, now: i64) -> Vec<Reminder> {
    if now <= checked {
        return Vec::new();
    }
    let late = now.saturating_sub(checked) > LATE;
    let from = if late { now - LATE } else { checked };
    let mut out = Vec::new();
    for event in held.events.iter().filter(|e| !e.cancelled && !declined(e)) {
        let mut alarms: Vec<i64> = if !event.alarms.is_empty() {
            event.alarms.iter().copied().filter(|a| (-FARTHEST..=FARTHEST).contains(a)).collect()
        } else if event.start.is_day() {
            Vec::new()
        } else {
            vec![held.lead.clamp(-FARTHEST, FARTHEST)]
        };
        alarms.sort_unstable();
        alarms.dedup();
        let (Some(&nearest), Some(&farthest)) = (alarms.first(), alarms.last()) else { continue };
        // A start whose reminder can fall due in (from, now] lies in this.
        for start in starts(held, event, from.saturating_add(nearest), now.saturating_add(farthest)) {
            for alarm in &alarms {
                let due = start - alarm;
                if from < due && due <= now && (!late || start > now) {
                    let (uid, summary) = (event.uid.clone(), event.summary.clone());
                    out.push(Reminder { uid, summary, start, due, until: start - now });
                }
            }
        }
    }
    out.sort_by(|a, b| (a.due, a.start, &a.uid).cmp(&(b.due, b.start, &b.uid)));
    out
}

/// The instants after `after` and up to `upto` at which `event` begins:
/// each occurrence of a repeat that is neither left out nor stood in for
/// by an event of its own, as `occurrences_between` places them.
fn starts(held: &Held, event: &Event, after: i64, upto: i64) -> Vec<i64> {
    let instant = |m: &Moment| instant(m, held.zones, held.local).filter(|&at| after < at && at <= upto);
    let Some(Repeat::Rule(rule)) = event.repeat.as_ref().filter(|_| event.instead_of.is_none()) else {
        return instant(&event.start).into_iter().collect();
    };
    // Two days either side covers any zone's distance from UTC.
    let walls = occurrences(event.start.wall(), rule, after - 2 * DAY, upto + 2 * DAY);
    let mut out = Vec::new();
    for wall in walls {
        let start = at_wall(&event.start, wall);
        let replaced = held.events.iter().any(|o| o.uid == event.uid && o.instead_of.as_ref() == Some(&start));
        if !replaced && !event.excluded.contains(&start) {
            out.extend(instant(&start));
        }
    }
    out
}

/// The same moment at another wall time: one occurrence of a repeat.
fn at_wall(moment: &Moment, wall: i64) -> Moment {
    match moment {
        Moment::Day(_) => Moment::Day(wall.div_euclid(DAY)),
        Moment::Utc(_) => Moment::Utc(wall),
        Moment::Wall { zone, .. } => Moment::Wall { wall, zone: zone.clone() },
    }
}

/// The instant a moment names. A day begins at the reader's midnight; a
/// time with no zone is on the reader's clock; one naming a zone nothing
/// defines is no instant, and so has no reminder (ADR-061).
pub fn instant(moment: &Moment, zones: &BTreeMap<String, Zone>, local: &Zone) -> Option<i64> {
    match moment {
        Moment::Utc(at) => Some(*at),
        Moment::Day(day) => {
            let wall = day.checked_mul(DAY)?;
            Some(wall - local.offset_at_wall(wall)?)
        }
        Moment::Wall { wall, zone: None } => Some(wall - local.offset_at_wall(*wall)?),
        Moment::Wall { wall, zone: Some(zone) } => Some(wall - zones.get(zone)?.offset_at_wall(*wall)?),
    }
}

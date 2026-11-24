//! Events as a calendar holds them (ADR-054), and the pure rules over
//! them: which instant a written time is (ADR-061) and on which days a
//! repeating event falls (ADR-060).
//!
//! A "wall" time here is what a clock on the wall of some zone shows, held
//! as seconds as if it were UTC. Repeats are worked out in wall time, so a
//! meeting at half past nine stays at half past nine when the clocks
//! change, and only then turned into instants.

use std::collections::BTreeMap;

use crate::Address;

pub mod date;
mod rules;
#[cfg(test)]
mod tests;
mod words;

pub use self::rules::{Standing, links_of, standing};
pub use self::words::{alarm_words, repeat_words, rule_words};

pub const DAY: i64 = 86_400;

/// Days from 1970-01-01 to a date in the proleptic Gregorian calendar.
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    era * 146_097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year - 719_468
}

/// The year, month and day of a day number.
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day_of_year - (153 * shifted + 2) / 5 + 1)
}

/// 0 is Sunday. 1970-01-01 was a Thursday.
pub fn weekday_of(day: i64) -> i64 {
    (day + 4).rem_euclid(7)
}

pub fn days_in_month(year: i64, month: i64) -> i64 {
    days_from_civil(year + month / 12, month % 12 + 1, 1) - days_from_civil(year, month, 1)
}

/// The day number of the `nth` `weekday` of a month; a negative `nth`
/// counts from the month's end. It may fall outside the month when the
/// month has no such day, and the caller checks.
pub fn nth_weekday(year: i64, month: i64, nth: i64, weekday: i64) -> i64 {
    if nth > 0 {
        let first = days_from_civil(year, month, 1);
        first + (weekday - weekday_of(first)).rem_euclid(7) + 7 * (nth - 1)
    } else {
        let last = days_from_civil(year, month, 1) + days_in_month(year, month) - 1;
        last - (weekday_of(last) - weekday).rem_euclid(7) + 7 * (nth + 1)
    }
}

/// One period of a time zone: standard time, or daylight time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Period {
    /// When it first applied, as a wall time.
    pub start: i64,
    /// Seconds east of UTC while it applies.
    pub offset: i64,
    /// The month, and which weekday of it, on which it begins each year;
    /// `None` for a period that begins once, at `start`.
    pub yearly: Option<(i64, i64, i64)>,
}

/// A time zone as its periods. A calendar's own `VTIMEZONE` and the
/// system's zone are both turned into this (ADR-061).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Zone {
    pub periods: Vec<Period>,
}

impl Zone {
    pub fn fixed(offset: i64) -> Zone {
        Zone { periods: vec![Period { start: i64::MIN / 2, offset, yearly: None }] }
    }

    /// The wall times at which a period begins in the years around `year`.
    fn onsets(period: &Period, year: i64) -> Vec<i64> {
        match period.yearly {
            None => vec![period.start],
            Some((month, nth, weekday)) => {
                let time_of_day = period.start.rem_euclid(DAY);
                let each = (year - 1..=year + 1).map(|y| nth_weekday(y, month, nth, weekday) * DAY + time_of_day);
                each.filter(|&onset| onset >= period.start).collect()
            }
        }
    }

    /// The offset in force at a wall time: that of the period that began
    /// most recently before it. Both sides of the comparison are wall
    /// times, so the hour a clock change skips or repeats may land on
    /// either side of it.
    pub fn offset_at_wall(&self, wall: i64) -> Option<i64> {
        let year = civil_from_days(wall.div_euclid(DAY)).0;
        let began = |p: &Period| Zone::onsets(p, year).into_iter().filter(|&onset| onset <= wall).max();
        self.periods.iter().filter_map(|p| Some((began(p)?, p.offset))).max().map(|(_, offset)| offset)
    }

    /// The offset in force at an instant. A period's beginning is written
    /// on the clock of the period before it, so that is the offset that
    /// turns it into an instant.
    pub fn offset_at(&self, instant: i64) -> Option<i64> {
        let year = civil_from_days(instant.div_euclid(DAY)).0;
        let began = |p: &Period| {
            let instants = Zone::onsets(p, year)
                .into_iter()
                .map(|onset| onset - self.offset_at_wall(onset - 1).unwrap_or(p.offset));
            instants.filter(|&at| at <= instant).max()
        };
        self.periods.iter().filter_map(|p| Some((began(p)?, p.offset))).max().map(|(_, offset)| offset)
    }
}

/// When something happens, as its calendar wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Moment {
    /// A whole day, by its day number.
    Day(i64),
    /// An instant: seconds since the epoch.
    Utc(i64),
    /// A wall time in a named zone, or with no zone, which means wherever
    /// the reader is.
    Wall { wall: i64, zone: Option<String> },
}

impl Moment {
    /// The wall time this was written as; a day is its midnight.
    pub fn wall(&self) -> i64 {
        match self {
            Moment::Day(day) => day * DAY,
            Moment::Utc(at) | Moment::Wall { wall: at, .. } => *at,
        }
    }

    pub fn is_day(&self) -> bool {
        matches!(self, Moment::Day(_))
    }

    /// The same moment at another wall time: an occurrence of a repeat.
    pub fn at_wall(&self, wall: i64) -> Moment {
        match self {
            Moment::Day(_) => Moment::Day(wall.div_euclid(DAY)),
            Moment::Utc(_) => Moment::Utc(wall),
            Moment::Wall { zone, .. } => Moment::Wall { wall, zone: zone.clone() },
        }
    }

    /// The wall time in the reader's zone, or `None` when it names a zone
    /// nothing defines: such a time is shown as written and never placed
    /// (ADR-061). A day is the same day everywhere.
    pub fn local_wall(&self, zones: &BTreeMap<String, Zone>, local: &Zone) -> Option<i64> {
        match self {
            Moment::Day(day) => Some(day * DAY),
            Moment::Wall { wall, zone: None } => Some(*wall),
            Moment::Utc(at) => Some(at + local.offset_at(*at)?),
            Moment::Wall { wall, zone: Some(zone) } => {
                let at = wall - zones.get(zone)?.offset_at_wall(*wall)?;
                Some(at + local.offset_at(at)?)
            }
        }
    }
}

/// What a guest has said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    Nothing,
    Accepted,
    Tentative,
    Declined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guest {
    pub who: Address,
    pub said: Said,
}

/// What a calendar part asks its reader to do with the events in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Request,
    Reply,
    Cancel,
    Counter,
    /// Any other, `PUBLISH` included: events to show and nothing to answer.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// A recurrence rule of the kinds tuimex works out (ADR-060).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub freq: Freq,
    pub interval: i64,
    pub count: Option<u32>,
    /// The last wall time an occurrence may start at.
    pub until: Option<i64>,
    /// Weekdays (0 is Sunday), each with a position in the month, or 0 for
    /// every such weekday.
    pub by_day: Vec<(i64, i64)>,
    pub by_month_day: Vec<i64>,
}

/// How an event repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repeat {
    Rule(Rule),
    /// A rule outside what tuimex works out, as written. The event is
    /// shown on its first day and says so.
    Unread(String),
}

/// One event of a calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub uid: String,
    pub sequence: u32,
    /// For an event that stands in for one occurrence of a repeating one:
    /// the start that occurrence would have had.
    pub instead_of: Option<Moment>,
    pub summary: String,
    pub start: Moment,
    pub end: Option<Moment>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub organizer: Option<Address>,
    pub guests: Vec<Guest>,
    pub cancelled: bool,
    /// Addresses the event gives for joining, the conference's first, then
    /// its `URL`. They are text until assessed (ADR-018).
    pub links: Vec<String>,
    pub repeat: Option<Repeat>,
    /// Starts the rule would give that are left out.
    pub excluded: Vec<Moment>,
    /// Seconds before the start at which the event asks to be announced.
    pub alarms: Vec<i64>,
    /// What only some providers send, as label and value, in the order
    /// written: shown under More and never interpreted.
    pub more: Vec<(String, String)>,
}

/// A parsed calendar: its events and the zones it defines for them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Calendar {
    pub method: Option<Method>,
    pub events: Vec<Event>,
    pub zones: BTreeMap<String, Zone>,
}

/// The wall times at which a repeating event starts, from `from` up to but
/// not including `to`, in order. `start` is the first occurrence's wall
/// time. A rule that counts its occurrences is counted from the start; any
/// other is entered at `from` by arithmetic, so a rule from years ago
/// costs the same as one from today.
pub fn occurrences(start: i64, rule: &Rule, from: i64, to: i64) -> Vec<i64> {
    let interval = rule.interval.max(1);
    let (start_day, time) = (start.div_euclid(DAY), start.rem_euclid(DAY));
    let (year0, month0, dom0) = civil_from_days(start_day);
    let months0 = year0 * 12 + month0 - 1;
    // Weeks begin on Monday, the default a rule has when it does not say.
    let week0 = start_day - (weekday_of(start_day) + 6) % 7;

    let from_day = from.max(start).div_euclid(DAY);
    let (from_year, from_month, _) = civil_from_days(from_day);
    let first = if rule.count.is_some() {
        0
    } else {
        let elapsed = match rule.freq {
            Freq::Daily => from_day - start_day,
            Freq::Weekly => (from_day - week0).div_euclid(7),
            Freq::Monthly => from_year * 12 + from_month - 1 - months0,
            Freq::Yearly => from_year - year0,
        };
        (elapsed.div_euclid(interval) - 1).max(0)
    };

    let in_month = |months: i64| -> Vec<i64> {
        let (year, month) = (months.div_euclid(12), months.rem_euclid(12) + 1);
        let (first_day, length) = (days_from_civil(year, month, 1), days_in_month(year, month));
        let mut days: Vec<i64> = Vec::new();
        for &dom in &rule.by_month_day {
            let dom = if dom < 0 { length + 1 + dom } else { dom };
            days.extend((1..=length).contains(&dom).then(|| first_day + dom - 1));
        }
        for &(nth, weekday) in &rule.by_day {
            if nth == 0 {
                days.extend((first_day..first_day + length).filter(|&d| weekday_of(d) == weekday));
            } else {
                let day = nth_weekday(year, month, nth, weekday);
                days.extend((first_day..first_day + length).contains(&day).then_some(day));
            }
        }
        if rule.by_month_day.is_empty() && rule.by_day.is_empty() && dom0 <= length {
            days.push(first_day + dom0 - 1);
        }
        days
    };

    let mut out = Vec::new();
    let mut seen = 0u32;
    // Far more periods than any window on screen holds; it bounds a rule
    // whose days never exist, which would otherwise never reach `to`.
    for k in first..first + 5_000 {
        let step = k * interval;
        let mut days = match rule.freq {
            Freq::Daily => {
                let day = start_day + step;
                let wanted = rule.by_day.is_empty() || rule.by_day.iter().any(|&(_, w)| w == weekday_of(day));
                if wanted { vec![day] } else { Vec::new() }
            }
            Freq::Weekly => {
                let week = week0 + step * 7;
                if rule.by_day.is_empty() {
                    vec![week + (weekday_of(start_day) + 6) % 7]
                } else {
                    rule.by_day.iter().map(|&(_, w)| week + (w + 6) % 7).collect()
                }
            }
            Freq::Monthly => in_month(months0 + step),
            Freq::Yearly => in_month(months0 + step * 12),
        };
        days.sort_unstable();
        days.dedup();
        let period_start = days.first().map(|d| d * DAY + time);
        for day in days {
            let at = day * DAY + time;
            if at < start {
                continue;
            }
            if rule.until.is_some_and(|until| at > until) || rule.count.is_some_and(|count| seen >= count) {
                return out;
            }
            seen += 1;
            if at >= to {
                return out;
            }
            if at >= from {
                out.push(at);
            }
        }
        if period_start.is_some_and(|at| at >= to) {
            return out;
        }
    }
    out
}

/// One occurrence of an event, placed on the reader's clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence<'a> {
    pub event: &'a Event,
    /// Wall times in the reader's zone. An all-day event ends at the
    /// midnight after its last day.
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
}

/// Every occurrence that touches the days from `from_day` up to but not
/// including `to_day`, all-day events first within a start, then by start.
/// An event whose time names a zone nothing defines is left out: it has no
/// place on the clock, and the caller lists such events apart.
pub fn occurrences_between<'a>(
    events: &'a [Event],
    zones: &BTreeMap<String, Zone>,
    local: &Zone,
    from_day: i64,
    to_day: i64,
) -> Vec<Occurrence<'a>> {
    let (from, to) = (from_day * DAY, to_day * DAY);
    let mut out = Vec::new();
    for event in events.iter().filter(|e| !e.cancelled) {
        let length = event.end.as_ref().map_or(0, |end| end.wall() - event.start.wall()).max(0);
        let mut place = |start: &Moment| {
            let Some(begins) = start.local_wall(zones, local) else { return };
            let ends = begins + if start.is_day() { length.max(DAY) } else { length };
            if begins < to && ends.max(begins + 1) > from {
                out.push(Occurrence { event, start: begins, end: ends, all_day: start.is_day() });
            }
        };
        match &event.repeat {
            Some(Repeat::Rule(rule)) if event.instead_of.is_none() => {
                // A day either side, so an occurrence that a zone moves
                // across midnight is still found.
                let walls = occurrences(event.start.wall(), rule, from - length - DAY, to + DAY);
                for wall in walls {
                    let start = event.start.at_wall(wall);
                    let replaced =
                        events.iter().any(|other| other.uid == event.uid && other.instead_of.as_ref() == Some(&start));
                    if !replaced && !event.excluded.contains(&start) {
                        place(&start);
                    }
                }
            }
            _ => place(&event.start),
        }
    }
    out.sort_by_key(|o| (o.start, !o.all_day));
    out
}

/// The start of the occurrence of `event` that begins at the wall time
/// `start` on the reader's clock, as the event's own calendar writes it:
/// what names that occurrence to whoever keeps the event.
pub fn occurrence_at(event: &Event, zones: &BTreeMap<String, Zone>, local: &Zone, start: i64) -> Option<Moment> {
    let here = |moment: &Moment| moment.local_wall(zones, local) == Some(start);
    match &event.repeat {
        Some(Repeat::Rule(rule)) if event.instead_of.is_none() => {
            // Two days either side covers any zone's distance from the reader's.
            let walls = occurrences(event.start.wall(), rule, start - 2 * DAY, start + 2 * DAY);
            walls.into_iter().map(|wall| event.start.at_wall(wall)).find(here)
        }
        _ => Some(event.start.clone()).filter(here),
    }
}

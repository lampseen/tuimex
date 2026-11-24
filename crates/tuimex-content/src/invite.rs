//! A `text/calendar` part, as far as the invitation card shows it
//! (ADR-021). Not a calendar library: one event, the properties on the
//! card, and enough of a time zone definition to turn a local time into an
//! instant.

use std::collections::HashMap;

use tuimex_core::{Address, EventTime, Invite, Timestamp};

use crate::text::line;

#[cfg(test)]
mod tests;

struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, name: &str) -> Option<&str> {
        self.params.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

/// One period of a time zone: standard time, or daylight time.
struct Observance {
    /// When it first applied, as a wall-clock time.
    start: i64,
    /// Seconds east of UTC while it applies.
    offset: i64,
    /// The month, and which weekday of it, on which it begins each year.
    yearly: Option<(i64, i64, i64)>,
}

/// The first event in `calendar`, or `None` when there is none with a start.
pub fn invite(calendar: &str) -> Option<Invite> {
    let unfolded = calendar.replace("\r\n", "\n").replace("\n ", "").replace("\n\t", "");
    let mut open: Vec<String> = Vec::new();
    let mut event: Vec<Prop> = Vec::new();
    let mut events = 0;
    let mut zones: HashMap<String, Option<Vec<Observance>>> = HashMap::new();
    let (mut zone, mut period): (String, Vec<Prop>) = (String::new(), Vec::new());
    for prop in unfolded.lines().filter_map(prop) {
        let path = open.join("/");
        match prop.name.as_str() {
            "BEGIN" => {
                open.push(prop.value.trim().to_ascii_uppercase());
                events += usize::from(open.join("/") == "VCALENDAR/VEVENT");
            }
            "END" => {
                if matches!(path.as_str(), "VCALENDAR/VTIMEZONE/STANDARD" | "VCALENDAR/VTIMEZONE/DAYLIGHT") {
                    // One period that cannot be read makes the whole zone
                    // unknown: a guess at the offset is a wrong time.
                    let read = observance(&std::mem::take(&mut period));
                    match (zones.entry(zone.clone()).or_insert_with(|| Some(Vec::new())), read) {
                        (Some(known), Some(o)) => known.push(o),
                        (known, _) => *known = None,
                    }
                }
                open.pop();
            }
            "TZID" if path == "VCALENDAR/VTIMEZONE" => zone = prop.value.trim().to_string(),
            _ if path == "VCALENDAR/VEVENT" && events == 1 => event.push(prop),
            _ if path.starts_with("VCALENDAR/VTIMEZONE/") => period.push(prop),
            _ => {}
        }
    }

    let get = |name: &str| event.iter().find(|p| p.name == name);
    let text = |name: &str| get(name).map(|p| line(&unescape(&p.value)));
    let when = |name: &str| get(name).and_then(|p| event_time(p, &zones));
    let person = |p: &Prop| {
        let email = match p.value.get(..7) {
            Some(scheme) if scheme.eq_ignore_ascii_case("mailto:") => &p.value[7..],
            _ => &p.value,
        };
        Address { name: p.param("CN").map(line).filter(|n| !n.is_empty()), email: line(email) }
    };
    Some(Invite {
        start: when("DTSTART")?,
        end: when("DTEND"),
        uid: text("UID").unwrap_or_default(),
        summary: text("SUMMARY").unwrap_or_default(),
        location: text("LOCATION").filter(|l| !l.is_empty()),
        organizer: get("ORGANIZER").map(person),
        attendees: event.iter().filter(|p| p.name == "ATTENDEE").map(person).collect(),
        recurrence: get("RRULE").map(|p| recurrence(&p.value)),
    })
}

/// `NAME;PARAM=value;PARAM="quoted: value":value`.
fn prop(line: &str) -> Option<Prop> {
    let head_len = split_unquoted(line, ':').next()?.len();
    let value = line.get(head_len + 1..)?.to_string();
    let mut head = split_unquoted(&line[..head_len], ';');
    let name = head.next()?.trim().to_ascii_uppercase();
    let params =
        head.filter_map(|p| p.split_once('=')).map(|(n, v)| (n.to_ascii_uppercase(), v.trim_matches('"').to_string()));
    Some(Prop { name, params: params.collect(), value })
}

fn split_unquoted(s: &str, separator: char) -> impl Iterator<Item = &str> {
    let mut quoted = false;
    s.split(move |c| {
        quoted ^= c == '"';
        c == separator && !quoted
    })
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match (c == '\\').then(|| chars.next()) {
            None => out.push(c),
            Some(Some('n' | 'N')) => out.push(' '),
            Some(Some(escaped)) => out.push(escaped),
            Some(None) => {}
        }
    }
    out
}

/// `YYYYMMDD` or `YYYYMMDDTHHMMSS[Z]` as seconds, read as if UTC, and
/// whether it was a date alone and whether it said `Z`. Anything that does
/// not parse as one is not a time, and the caller has no event without it.
fn wall(value: &str) -> Option<(i64, bool, bool)> {
    let value = value.trim();
    let digits = |from: usize, len: usize| value.get(from..from + len)?.parse::<i64>().ok();
    let (year, month, day) = (digits(0, 4)?, digits(4, 2)?, digits(6, 2)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let midnight = days_from_civil(year, month, day) * 86_400;
    if value.len() == 8 {
        return Some((midnight, true, false));
    }
    let (hour, minute, second) = (digits(9, 2)?, digits(11, 2)?, digits(13, 2)?);
    let shape = value.as_bytes().get(8) == Some(&b'T') && matches!(&value[15..], "" | "Z" | "z");
    (shape && hour < 24 && minute < 60 && second < 61).then_some((
        midnight + hour * 3600 + minute * 60 + second,
        false,
        value.len() == 16,
    ))
}

/// Days from 1970-01-01 to a date in the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    era * 146_097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year - 719_468
}

fn event_time(prop: &Prop, zones: &HashMap<String, Option<Vec<Observance>>>) -> Option<EventTime> {
    let (seconds, date_only, utc) = wall(&prop.value)?;
    let zone = prop.param("TZID");
    let offset = zone.and_then(|z| zones.get(z)?.as_deref()).and_then(|periods| offset_at(periods, seconds));
    Some(match (date_only, utc, offset) {
        (true, _, _) => EventTime::Day(Timestamp(seconds)),
        (_, true, _) => EventTime::At(Timestamp(seconds)),
        (_, _, Some(offset)) => EventTime::At(Timestamp(seconds - offset)),
        _ => EventTime::Local { wall: Timestamp(seconds), zone: zone.map(line) },
    })
}

/// `None` for a period this parser cannot read, a number that does not
/// parse included; the caller then treats the whole zone as unknown.
fn observance(props: &[Prop]) -> Option<Observance> {
    let get = |name: &str| props.iter().find(|p| p.name == name).map(|p| p.value.trim());
    let to = get("TZOFFSETTO")?;
    let field = |from: usize| to.get(from..from + 2)?.parse::<i64>().ok();
    let offset = (field(1)? * 3600 + field(3)? * 60) * if to.starts_with('-') { -1 } else { 1 };
    let yearly = match get("RRULE") {
        None => None,
        Some(rule) => {
            let parts: HashMap<&str, &str> = rule.split(';').filter_map(|p| p.split_once('=')).collect();
            let day = parts.get("BYDAY")?;
            let weekday = DAYS.iter().position(|(code, _)| day.ends_with(code))?;
            let nth: i64 = day[..day.len() - 2].parse().ok()?;
            // A rule that ends or repeats otherwise describes a zone this
            // parser cannot place a date in.
            let simple = parts.get("FREQ") == Some(&"YEARLY") && parts.len() == 3 && nth != 0 && nth.abs() <= 5;
            Some((parts.get("BYMONTH")?.parse().ok().filter(|m| simple && (1..=12).contains(m))?, nth, weekday as i64))
        }
    };
    Some(Observance { start: wall(get("DTSTART")?)?.0, offset, yearly })
}

/// The offset in force at a wall-clock time: that of the period that began
/// most recently before it. The comparison is in wall-clock time on both
/// sides, so the hour a clock change skips or repeats may land on either.
fn offset_at(periods: &[Observance], at: i64) -> Option<i64> {
    let began = |p: &Observance| match p.yearly {
        None => Some(p.start).filter(|&s| s <= at),
        Some((month, nth, weekday)) => {
            let year = 1970 + at.div_euclid(31_556_952);
            let time_of_day = p.start.rem_euclid(86_400);
            let onsets = (year - 1..=year + 1).map(|y| nth_weekday(y, month, nth, weekday) * 86_400 + time_of_day);
            onsets.filter(|&o| o <= at && o >= p.start).max()
        }
    };
    periods.iter().filter_map(|p| Some((began(p)?, p.offset))).max().map(|(_, offset)| offset)
}

/// The day number of the `nth` `weekday` (0 is Sunday) of a month; a
/// negative `nth` counts from the month's end.
fn nth_weekday(year: i64, month: i64, nth: i64, weekday: i64) -> i64 {
    // 1970-01-01 was a Thursday.
    let weekday_of = |day: i64| (day + 4).rem_euclid(7);
    if nth > 0 {
        let first = days_from_civil(year, month, 1);
        first + (weekday - weekday_of(first)).rem_euclid(7) + 7 * (nth - 1)
    } else {
        let last = days_from_civil(year + month / 12, month % 12 + 1, 1) - 1;
        last - (weekday_of(last) - weekday).rem_euclid(7) + 7 * (nth + 1)
    }
}

const DAYS: [(&str, &str); 7] =
    [("SU", "Sun"), ("MO", "Mon"), ("TU", "Tue"), ("WE", "Wed"), ("TH", "Thu"), ("FR", "Fri"), ("SA", "Sat")];

/// A rule in words where it is simple, and "repeats" where it is not.
fn recurrence(rule: &str) -> String {
    let mut words = String::new();
    let (mut unit, mut every, mut tail) = ("", 1u32, String::new());
    for (name, value) in rule.split(';').filter_map(|p| p.split_once('=')) {
        match name.to_ascii_uppercase().as_str() {
            "FREQ" => {
                unit = match value.to_ascii_uppercase().as_str() {
                    "DAILY" => "day",
                    "WEEKLY" => "week",
                    "MONTHLY" => "month",
                    "YEARLY" => "year",
                    _ => return "repeats".into(),
                }
            }
            "INTERVAL" => match value.parse() {
                Ok(n) if n > 0 => every = n,
                _ => return "repeats".into(),
            },
            "BYDAY" => {
                let names: Option<Vec<&str>> = value
                    .split(',')
                    .map(|d| DAYS.iter().find(|(code, _)| d.eq_ignore_ascii_case(code)).map(|(_, n)| *n))
                    .collect();
                match names {
                    Some(names) => words = format!(" on {}", names.join(", ")),
                    None => return "repeats".into(),
                }
            }
            "COUNT" if value.parse::<u32>().is_ok() => tail = format!(", {value} times"),
            "UNTIL" => match wall(value).and_then(|_| Some((value.get(..4)?, value.get(4..6)?, value.get(6..8)?))) {
                Some((y, m, d)) => tail = format!(", until {y}-{m}-{d}"),
                None => return "repeats".into(),
            },
            "WKST" => {}
            _ => return "repeats".into(),
        }
    }
    match (unit, every) {
        ("", _) => "repeats".into(),
        ("day", 1) => format!("daily{words}{tail}"),
        (unit, 1) => format!("{unit}ly{words}{tail}"),
        (unit, n) => format!("every {n} {unit}s{words}{tail}"),
    }
}

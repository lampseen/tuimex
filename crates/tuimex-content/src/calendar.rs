//! iCalendar text to plain data (ADR-007): a `text/calendar` part of a
//! message, a calendar fetched as a feed, one event from a CalDAV server.
//! Not a calendar library: the properties tuimex shows or acts on, and
//! enough of a time zone definition to turn a local time into an instant.
//! Anything it cannot read is left out and never guessed at.

use std::collections::{BTreeMap, HashMap};

use tuimex_core::calendar::{
    Calendar, DAY, Event, Freq, Guest, Method, Moment, Period, Repeat, Rule, Said, Zone, days_from_civil,
};
use tuimex_core::{Address, EventTime, Invite, Timestamp};

use crate::text::{clean, line, push_unique};

#[cfg(test)]
mod tests;
mod write;

pub use write::{cancel, counter, event_text, new_uid, reply, request, with_answer};

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

/// The deepest component read is a period of a zone or an alarm of an
/// event, three down; and the longest name read has nine letters.
const DEPTH: usize = 4;
const NAME: usize = 12;

/// The properties of one `VEVENT`, and the triggers of the alarms in it.
#[derive(Default)]
struct Raw {
    props: Vec<Prop>,
    triggers: Vec<String>,
}

/// Every event of `text` that has a start, the zones it defines and what
/// it asks for. Text that is not a calendar is a calendar with no events.
pub fn calendar(text: &str) -> Calendar {
    let unfolded = text.replace("\r\n", "\n").replace("\n ", "").replace("\n\t", "");
    // What is open, as far down as anything read here sits. Deeper levels
    // are only counted, and a name is cut short, so the path built for
    // each line stays small however a calendar nests or names things.
    let mut open: Vec<String> = Vec::new();
    let mut deeper = 0usize;
    let mut raws: Vec<Raw> = Vec::new();
    let mut method = None;
    // A zone one of whose periods cannot be read is unknown: a guess at
    // the offset is a wrong time.
    let mut zones: HashMap<String, Option<Vec<Period>>> = HashMap::new();
    let (mut zone, mut period): (String, Vec<Prop>) = (String::new(), Vec::new());
    for prop in unfolded.lines().filter_map(prop) {
        let path = open.join("/");
        match prop.name.as_str() {
            "BEGIN" if open.len() >= DEPTH => deeper += 1,
            "END" if deeper > 0 => deeper -= 1,
            _ if deeper > 0 => {}
            "BEGIN" => {
                open.push(prop.value.trim().chars().take(NAME).collect::<String>().to_ascii_uppercase());
                if open.join("/") == "VCALENDAR/VEVENT" {
                    raws.push(Raw::default());
                }
            }
            "END" => {
                if matches!(path.as_str(), "VCALENDAR/VTIMEZONE/STANDARD" | "VCALENDAR/VTIMEZONE/DAYLIGHT") {
                    let read = observance(&std::mem::take(&mut period));
                    match (zones.entry(zone.clone()).or_insert_with(|| Some(Vec::new())), read) {
                        (Some(known), Some(o)) => known.push(o),
                        (known, _) => *known = None,
                    }
                }
                open.pop();
            }
            "METHOD" if path == "VCALENDAR" => {
                method = Some(match prop.value.trim().to_ascii_uppercase().as_str() {
                    "REQUEST" => Method::Request,
                    "REPLY" => Method::Reply,
                    "CANCEL" => Method::Cancel,
                    "COUNTER" => Method::Counter,
                    _ => Method::Other,
                });
            }
            "TZID" if path == "VCALENDAR/VTIMEZONE" => zone = line(&prop.value),
            "TRIGGER" if path == "VCALENDAR/VEVENT/VALARM" => {
                if let Some(raw) = raws.last_mut() {
                    raw.triggers.push(prop.value);
                }
            }
            _ if path == "VCALENDAR/VEVENT" => {
                if let Some(raw) = raws.last_mut() {
                    raw.props.push(prop);
                }
            }
            _ if path.starts_with("VCALENDAR/VTIMEZONE/") => period.push(prop),
            _ => {}
        }
    }
    let zones: BTreeMap<String, Zone> =
        zones.into_iter().filter_map(|(name, periods)| Some((name, Zone { periods: periods? }))).collect();
    Calendar { method, events: raws.iter().filter_map(event).collect(), zones }
}

/// The first event in `calendar` as the invitation card shows it, or
/// `None` when the first event has no start.
pub fn invite(calendar_text: &str) -> Option<Invite> {
    let unfolded = calendar_text.replace("\r\n", "\n").replace("\n ", "").replace("\n\t", "");
    let first = first_event_text(&unfolded)?;
    let parsed = calendar(calendar_text);
    // The card is of the first event; one without a start is no
    // invitation, whatever follows it.
    let event = parsed
        .events
        .first()
        .filter(|_| first.lines().filter_map(prop).any(|p| p.name == "DTSTART" && moment(&p).is_some()))?;
    let shown = |moment: &Moment| match moment {
        Moment::Day(day) => EventTime::Day(Timestamp(day * DAY)),
        Moment::Utc(at) => EventTime::At(Timestamp(*at)),
        Moment::Wall { wall, zone } => {
            let offset = zone.as_ref().and_then(|z| parsed.zones.get(z)).and_then(|z| z.offset_at_wall(*wall));
            match offset {
                Some(offset) => EventTime::At(Timestamp(wall - offset)),
                None => EventTime::Local { wall: Timestamp(*wall), zone: zone.clone() },
            }
        }
    };
    let rule = first.lines().filter_map(prop).find(|p| p.name == "RRULE");
    Some(Invite {
        start: shown(&event.start),
        end: event.end.as_ref().map(shown),
        uid: event.uid.clone(),
        summary: event.summary.clone(),
        location: event.location.clone(),
        organizer: event.organizer.clone(),
        attendees: event.guests.iter().map(|g| g.who.clone()).collect(),
        recurrence: rule.map(|p| recurrence(&p.value)),
    })
}

/// The lines between the first `BEGIN:VEVENT` and its `END`.
fn first_event_text(unfolded: &str) -> Option<&str> {
    let upper = unfolded.to_ascii_uppercase();
    let begin = upper.find("BEGIN:VEVENT")?;
    let end = upper[begin..].find("END:VEVENT").map_or(unfolded.len(), |i| begin + i);
    unfolded.get(begin..end)
}

fn person(p: &Prop) -> Address {
    let email = match p.value.get(..7) {
        Some(scheme) if scheme.eq_ignore_ascii_case("mailto:") => &p.value[7..],
        _ => &p.value,
    };
    Address { name: p.param("CN").map(line).filter(|n| !n.is_empty()), email: line(email) }
}

/// An event from its properties, or `None` without a start that reads.
fn event(raw: &Raw) -> Option<Event> {
    let get = |name: &str| raw.props.iter().find(|p| p.name == name);
    let all = |name: &'static str| raw.props.iter().filter(move |p| p.name == name);
    let text = |name: &str| get(name).map(|p| line(&unescape(&p.value, ' '))).filter(|t| !t.is_empty());
    let value = |name: &str| get(name).map(|p| line(&p.value).to_ascii_lowercase()).filter(|v| !v.is_empty());
    let start = get("DTSTART").and_then(moment)?;
    let end = get("DTEND").and_then(moment).or_else(|| {
        let length = duration(&get("DURATION")?.value)?;
        Some(match &start {
            Moment::Day(day) => Moment::Day(day + length.div_euclid(DAY)),
            Moment::Utc(at) => Moment::Utc(at + length),
            Moment::Wall { wall, zone } => Moment::Wall { wall: wall + length, zone: zone.clone() },
        })
    });

    let mut links = Vec::new();
    for name in ["CONFERENCE", "X-GOOGLE-CONFERENCE", "URL"] {
        for p in all(name) {
            let url = line(&p.value);
            if !url.is_empty() {
                push_unique(&mut links, &url);
            }
        }
    }
    let said = |p: &Prop| match p.param("PARTSTAT").map(str::to_ascii_uppercase).as_deref() {
        Some("ACCEPTED") => Said::Accepted,
        Some("TENTATIVE") => Said::Tentative,
        Some("DECLINED") => Said::Declined,
        _ => Said::Nothing,
    };
    let mut more = Vec::new();
    let mut note = |label: &str, value: Option<String>| more.extend(value.map(|v| (label.to_string(), v)));
    note("Status", value("STATUS"));
    note("Shown as", value("TRANSP").map(|v| if v == "transparent" { "free".into() } else { "busy".into() }));
    note("Privacy", value("CLASS"));
    note("Changed", get("LAST-MODIFIED").and_then(|p| stamp(&p.value)));

    Some(Event {
        uid: text("UID").unwrap_or_default(),
        sequence: get("SEQUENCE").and_then(|p| p.value.trim().parse().ok()).unwrap_or(0),
        instead_of: get("RECURRENCE-ID").and_then(moment),
        summary: text("SUMMARY").unwrap_or_default(),
        location: text("LOCATION"),
        description: get("DESCRIPTION").map(|p| lines(&unescape(&p.value, '\n'))).filter(|d| !d.is_empty()),
        organizer: get("ORGANIZER").map(person),
        guests: all("ATTENDEE").map(|p| Guest { who: person(p), said: said(p) }).collect(),
        cancelled: value("STATUS").as_deref() == Some("cancelled"),
        links,
        repeat: get("RRULE")
            .map(|p| rule(&p.value, &start).map_or_else(|| Repeat::Unread(line(&p.value)), Repeat::Rule)),
        excluded: all("EXDATE")
            .flat_map(|p| p.value.split(',').filter_map(|v| moment_of(v, p.param("TZID"))).collect::<Vec<_>>())
            .collect(),
        alarms: raw.triggers.iter().filter_map(|t| duration(t)).filter(|&d| d <= 0).map(|d| -d).collect(),
        more,
        start,
        end,
    })
}

/// Text of several lines, each cleaned, with the blank ones at its ends
/// dropped.
fn lines(text: &str) -> String {
    let cleaned: Vec<String> = clean(text).lines().map(line).collect();
    cleaned.join("\n").trim_matches('\n').to_string()
}

fn moment(prop: &Prop) -> Option<Moment> {
    moment_of(&prop.value, prop.param("TZID"))
}

fn moment_of(value: &str, zone: Option<&str>) -> Option<Moment> {
    let (seconds, date_only, utc) = wall(value)?;
    Some(match (date_only, utc) {
        (true, _) => Moment::Day(seconds.div_euclid(DAY)),
        (_, true) => Moment::Utc(seconds),
        _ => Moment::Wall { wall: seconds, zone: zone.map(line).filter(|z| !z.is_empty()) },
    })
}

/// A UTC stamp as `2026-11-23 14:05`, for showing.
fn stamp(value: &str) -> Option<String> {
    let v = value.trim();
    wall(v)?;
    Some(format!("{}-{}-{} {}:{}", v.get(..4)?, v.get(4..6)?, v.get(6..8)?, v.get(9..11)?, v.get(11..13)?))
}

/// `[+-]P[nW][nD][T[nH][nM][nS]]` in seconds. Months and years have no
/// fixed length and are not read.
fn duration(value: &str) -> Option<i64> {
    let v = value.trim().to_ascii_uppercase();
    let (sign, rest) = match v.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, v.strip_prefix('+').unwrap_or(&v)),
    };
    let rest = rest.strip_prefix('P')?;
    let (mut total, mut number, mut in_time, mut any) = (0i64, String::new(), false, false);
    for c in rest.chars() {
        match c {
            '0'..='9' => number.push(c),
            'T' if number.is_empty() => in_time = true,
            _ => {
                let n: i64 = std::mem::take(&mut number).parse().ok()?;
                let unit = match (c, in_time) {
                    ('W', false) => 7 * DAY,
                    ('D', false) => DAY,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
                total = total.checked_add(n.checked_mul(unit)?)?;
                any = true;
            }
        }
    }
    (any && number.is_empty()).then_some(sign * total)
}

/// A rule of the kinds ADR-060 names, or `None` for any other: the caller
/// keeps the text and the event is shown once.
fn rule(text: &str, start: &Moment) -> Option<Rule> {
    let mut rule =
        Rule { freq: Freq::Daily, interval: 1, count: None, until: None, by_day: Vec::new(), by_month_day: Vec::new() };
    let (mut freq, mut by_month, mut week_start) = (None, None, 1);
    for (name, value) in text.split(';').filter(|p| !p.trim().is_empty()).map(|p| p.split_once('=').unwrap_or((p, "")))
    {
        let value = value.trim().to_ascii_uppercase();
        match name.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                freq = Some(match value.as_str() {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                });
            }
            "INTERVAL" => rule.interval = value.parse().ok().filter(|&n| n > 0)?,
            "COUNT" => rule.count = Some(value.parse().ok()?),
            // A date alone means through that day.
            "UNTIL" => {
                rule.until = Some(wall(&value).map(|(at, date_only, _)| if date_only { at + DAY - 1 } else { at })?)
            }
            "BYDAY" => {
                for day in value.split(',') {
                    let code = day.get(day.len().checked_sub(2)?..)?;
                    let weekday = DAYS.iter().position(|(c, _)| *c == code)? as i64;
                    let nth = match &day[..day.len() - 2] {
                        "" => 0,
                        n => n.parse::<i64>().ok().filter(|n| *n != 0 && n.abs() <= 5)?,
                    };
                    rule.by_day.push((nth, weekday));
                }
            }
            "BYMONTHDAY" => {
                for dom in value.split(',') {
                    rule.by_month_day.push(dom.parse::<i64>().ok().filter(|d| *d != 0 && d.abs() <= 31)?);
                }
            }
            "BYMONTH" => by_month = Some(value.parse::<i64>().ok()?),
            "WKST" => week_start = DAYS.iter().position(|(c, _)| *c == value)? as i64,
            _ => return None,
        }
    }
    rule.freq = freq?;
    let positioned = rule.by_day.iter().any(|&(nth, _)| nth != 0);
    let start_month = tuimex_core::calendar::civil_from_days(start.wall().div_euclid(DAY)).1;
    let reads = match rule.freq {
        Freq::Daily => !positioned && rule.by_month_day.is_empty() && by_month.is_none(),
        // Which day a week begins on changes a rule only when weeks are
        // skipped; such a rule is worked out for weeks that begin on Monday.
        Freq::Weekly => {
            !positioned && rule.by_month_day.is_empty() && by_month.is_none() && (rule.interval == 1 || week_start == 1)
        }
        Freq::Monthly => by_month.is_none(),
        // A yearly rule is worked out within the month it began in.
        Freq::Yearly => by_month.is_none_or(|m| m == start_month),
    };
    reads.then_some(rule)
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

/// Undoes the escapes of a text value; a written line break becomes
/// `newline`.
fn unescape(value: &str, newline: char) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match (c == '\\').then(|| chars.next()) {
            None => out.push(c),
            Some(Some('n' | 'N')) => out.push(newline),
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

/// `None` for a period this parser cannot read, a number that does not
/// parse included; the caller then treats the whole zone as unknown.
fn observance(props: &[Prop]) -> Option<Period> {
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
    Some(Period { start: wall(get("DTSTART")?)?.0, offset, yearly })
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

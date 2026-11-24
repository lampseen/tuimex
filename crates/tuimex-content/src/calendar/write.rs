//! Plain data to iCalendar text: an event as tuimex stores it and PUTs it
//! (RFC 5545), and the messages that answer, propose, invite and cancel
//! (RFC 5546). Every value here came from an event, which is hostile text,
//! so each is escaped, quoted or refused before it reaches a line, and no
//! value can end the line it is on.

use std::collections::BTreeMap;

use tuimex_core::Address;
use tuimex_core::calendar::{
    DAY, Event, Freq, Moment, Period, Repeat, Rule, Said, Zone, civil_from_days, days_from_civil, nth_weekday,
};

use super::{DAYS, NAME, prop, split_unquoted};
use crate::text::line;

#[cfg(test)]
mod tests;

const CRLF: &str = "\r\n";

/// RFC 5545 §3.1: a line is folded once it would pass 75 octets.
const FOLD: usize = 75;

/// One whole calendar holding `event`, as a local event is stored and as an
/// event is PUT to a CalDAV server. `stamp` is the `DTSTAMP`, in seconds
/// since the epoch.
pub fn event_text(event: &Event, zones: &BTreeMap<String, Zone>, stamp: i64) -> String {
    let mut out = Out::new(None, event_zones(event), zones);
    out.event(event, stamp, Guests::AsSaid);
    out.finish()
}

/// `me`'s answer to `event`, for the organizer. `None` when `me` has no
/// address that can be written.
pub fn reply(event: &Event, zones: &BTreeMap<String, Zone>, me: &Address, said: Said, stamp: i64) -> Option<String> {
    let me = attendee(me, &[("PARTSTAT", partstat(said).to_string())])?;
    let mut out = Out::new(Some("REPLY"), identity_zones(event, &event.start), zones);
    out.begin_event(event, stamp, event.sequence);
    out.moment("DTSTART", &event.start);
    out.recurrence(event);
    out.organizer(event);
    out.text("SUMMARY", &event.summary);
    out.line(&me);
    out.end_event();
    Some(out.finish())
}

/// `me`'s proposal of another time for `event`. `None` when `me` has no
/// address that can be written.
pub fn counter(
    event: &Event,
    zones: &BTreeMap<String, Zone>,
    me: &Address,
    start: Moment,
    end: Option<Moment>,
    stamp: i64,
) -> Option<String> {
    let me = attendee(me, &[("PARTSTAT", "TENTATIVE".to_string())])?;
    let mut used = identity_zones(event, &start);
    used.extend(end.iter().flat_map(zone_of));
    let mut out = Out::new(Some("COUNTER"), used, zones);
    out.begin_event(event, stamp, event.sequence);
    out.moment("DTSTART", &start);
    if let Some(end) = &end {
        out.moment("DTEND", end);
    }
    out.recurrence(event);
    out.organizer(event);
    out.text("SUMMARY", &event.summary);
    out.line(&me);
    out.end_event();
    Some(out.finish())
}

/// The invitation to an event the user made, for its guests: the whole
/// event, each guest asked to answer.
pub fn request(event: &Event, zones: &BTreeMap<String, Zone>, stamp: i64) -> String {
    let mut out = Out::new(Some("REQUEST"), event_zones(event), zones);
    out.event(event, stamp, Guests::Asked);
    out.finish()
}

/// The cancellation of an event the user organized, for its guests.
pub fn cancel(event: &Event, zones: &BTreeMap<String, Zone>, stamp: i64) -> String {
    let mut out = Out::new(Some("CANCEL"), identity_zones(event, &event.start), zones);
    out.begin_event(event, stamp, event.sequence.saturating_add(1));
    out.moment("DTSTART", &event.start);
    out.recurrence(event);
    // Not asked for by RFC 5546, but without it a guest's calendar names
    // the cancelled event as one with no title.
    out.text("SUMMARY", &event.summary);
    out.line("STATUS:CANCELLED");
    out.organizer(event);
    for guest in &event.guests {
        out.lines(attendee(&guest.who, &[]));
    }
    out.end_event();
    out.finish()
}

/// `text` with `me`'s answer set on each event that invites `me`, and every
/// other byte as it was, so that what a provider wrote and tuimex does not
/// read survives the answer being PUT back. `None` when no event invites
/// `me`.
pub fn with_answer(text: &str, me: &Address, said: Said) -> Option<String> {
    let want = line(without_mailto(&me.email));
    if want.is_empty() {
        return None;
    }
    let newline = if text.contains(CRLF) { CRLF } else { "\n" };
    let physical: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(text.len() + 32);
    let mut open: Vec<String> = Vec::new();
    let mut found = false;
    let mut i = 0;
    while i < physical.len() {
        let folded = 1 + physical[i + 1..].iter().take_while(|p| p.starts_with([' ', '\t'])).count();
        let pieces = &physical[i..i + folded];
        i += folded;
        let logical: String = pieces
            .iter()
            .enumerate()
            .map(|(k, piece)| {
                let content = piece.strip_suffix('\n').map_or(*piece, |p| p.strip_suffix('\r').unwrap_or(p));
                if k == 0 { content } else { content.get(1..).unwrap_or("") }
            })
            .collect();
        let Some(p) = prop(&logical) else {
            out.extend(pieces.iter().copied());
            continue;
        };
        match p.name.as_str() {
            "BEGIN" => open.push(p.value.trim().chars().take(NAME).collect::<String>().to_ascii_uppercase()),
            "END" => {
                open.pop();
            }
            _ => {}
        }
        let in_event = open.len() == 2 && open[0] == "VCALENDAR" && open[1] == "VEVENT";
        let invited = p.name == "ATTENDEE" && in_event && line(without_mailto(&p.value)).eq_ignore_ascii_case(&want);
        if !invited {
            out.extend(pieces.iter().copied());
            continue;
        }
        found = true;
        let Some(answered) = answered(&logical, said) else {
            out.extend(pieces.iter().copied());
            continue;
        };
        let last = pieces.last().copied().unwrap_or("");
        let end = if last.ends_with(CRLF) {
            CRLF
        } else if last.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        out.push_str(&fold(&answered, newline));
        out.push_str(end);
    }
    found.then_some(out)
}

/// Sixteen random bytes as a UID. The caller supplies them: this crate has
/// no source of randomness.
pub fn new_uid(random: [u8; 16]) -> String {
    let mut uid: String = random.iter().map(|b| format!("{b:02x}")).collect();
    uid.push_str("@tuimex");
    uid
}

/// An `ATTENDEE` line with its `PARTSTAT` set to `said`, every other
/// parameter and the value kept as written; `None` when it already says
/// that.
fn answered(logical: &str, said: Said) -> Option<String> {
    let head_len = split_unquoted(logical, ':').next().map_or(0, str::len);
    let (head, rest) = logical.split_at(head_len);
    let wanted = format!("PARTSTAT={}", partstat(said));
    let mut segments: Vec<&str> = split_unquoted(head, ';').collect();
    let mut old = Vec::new();
    for segment in segments.iter_mut().skip(1) {
        if segment.split_once('=').is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case("PARTSTAT")) {
            old.push(std::mem::replace(segment, &wanted));
        }
    }
    let same =
        |segment: &&str| segment.split_once('=').is_some_and(|(_, v)| v.trim().eq_ignore_ascii_case(partstat(said)));
    if !old.is_empty() && old.iter().all(same) {
        return None;
    }
    if old.is_empty() {
        segments.push(&wanted);
    }
    Some(format!("{}{rest}", segments.join(";")))
}

fn without_mailto(value: &str) -> &str {
    let value = value.trim();
    match value.get(..7) {
        Some(scheme) if scheme.eq_ignore_ascii_case("mailto:") => &value[7..],
        _ => value,
    }
}

fn partstat(said: Said) -> &'static str {
    match said {
        Said::Nothing => "NEEDS-ACTION",
        Said::Accepted => "ACCEPTED",
        Said::Tentative => "TENTATIVE",
        Said::Declined => "DECLINED",
    }
}

/// How an event's guests are written: with what each has said, or each
/// asked to answer.
#[derive(Clone, Copy)]
enum Guests {
    AsSaid,
    Asked,
}

/// A calendar being written, line by line, each folded as it is added.
struct Out<'a> {
    text: String,
    zones: &'a BTreeMap<String, Zone>,
}

impl<'a> Out<'a> {
    fn new(method: Option<&str>, used: Vec<&str>, zones: &'a BTreeMap<String, Zone>) -> Out<'a> {
        let mut out = Out { text: String::new(), zones };
        out.line("BEGIN:VCALENDAR");
        out.line("PRODID:-//tuimex//EN");
        out.line("VERSION:2.0");
        if let Some(method) = method {
            out.line(&format!("METHOD:{method}"));
        }
        // Keyed by the name as written, so two names that clean to the same
        // text give one definition.
        let mut defined: BTreeMap<String, &Zone> = BTreeMap::new();
        for name in used {
            if let Some(zone) = zones.get(name).filter(|z| !z.periods.is_empty()) {
                defined.entry(param_text(name)).or_insert(zone);
            }
        }
        for (name, zone) in defined {
            out.zone(&name, zone);
        }
        out
    }

    fn line(&mut self, content: &str) {
        self.text.push_str(&fold(content, CRLF));
        self.text.push_str(CRLF);
    }

    fn lines(&mut self, content: Option<String>) {
        if let Some(content) = content {
            self.line(&content);
        }
    }

    fn finish(mut self) -> String {
        self.line("END:VCALENDAR");
        self.text
    }

    fn text(&mut self, name: &str, value: &str) {
        self.line(&format!("{name}:{}", escape(value)));
    }

    fn moment(&mut self, name: &str, moment: &Moment) {
        self.line(&moment_line(name, moment));
    }

    fn begin_event(&mut self, event: &Event, stamp: i64, sequence: u32) {
        self.line("BEGIN:VEVENT");
        self.text("UID", &event.uid);
        self.line(&format!("DTSTAMP:{}Z", date_time(stamp)));
        self.line(&format!("SEQUENCE:{sequence}"));
    }

    fn end_event(&mut self) {
        self.line("END:VEVENT");
    }

    fn recurrence(&mut self, event: &Event) {
        if let Some(instead_of) = &event.instead_of {
            self.moment("RECURRENCE-ID", instead_of);
        }
    }

    fn organizer(&mut self, event: &Event) {
        if let Some(organizer) = &event.organizer {
            self.lines(person("ORGANIZER", organizer, &[]));
        }
    }

    /// Every field of `event` the reader reads, but what it notes under
    /// More.
    fn event(&mut self, event: &Event, stamp: i64, guests: Guests) {
        self.begin_event(event, stamp, event.sequence);
        self.moment("DTSTART", &event.start);
        if let Some(end) = &event.end {
            self.moment("DTEND", end);
        }
        self.recurrence(event);
        self.text("SUMMARY", &event.summary);
        if let Some(location) = &event.location {
            self.text("LOCATION", location);
        }
        if let Some(description) = &event.description {
            self.text("DESCRIPTION", description);
        }
        self.organizer(event);
        for guest in &event.guests {
            let params = match guests {
                Guests::AsSaid => vec![("PARTSTAT", partstat(guest.said).to_string())],
                Guests::Asked => vec![("RSVP", "TRUE".to_string()), ("PARTSTAT", partstat(Said::Nothing).to_string())],
            };
            self.lines(attendee(&guest.who, &params));
        }
        if event.cancelled {
            self.line("STATUS:CANCELLED");
        }
        // An event has one `URL` and any number of conferences, and the
        // reader lists the conferences first; so the last link is the URL.
        if let Some((url, conferences)) = event.links.split_last() {
            for link in conferences {
                self.line(&format!("CONFERENCE;VALUE=URI:{}", line(link)));
            }
            self.line(&format!("URL:{}", line(url)));
        }
        match &event.repeat {
            Some(Repeat::Rule(rule)) => self.line(&format!("RRULE:{}", rule_text(rule, &event.start, self.zones))),
            Some(Repeat::Unread(text)) => self.line(&format!("RRULE:{}", line(text))),
            None => {}
        }
        for excluded in &event.excluded {
            self.moment("EXDATE", excluded);
        }
        for &before in &event.alarms {
            self.line("BEGIN:VALARM");
            self.line("ACTION:DISPLAY");
            self.text("DESCRIPTION", if event.summary.is_empty() { "Reminder" } else { &event.summary });
            self.line(&format!("TRIGGER:{}", duration(before.checked_neg().unwrap_or(i64::MAX))));
            self.line("END:VALARM");
        }
        self.end_event();
    }

    /// A `VTIMEZONE` from the zone's periods, so another program can place
    /// a time written in it.
    fn zone(&mut self, name: &str, zone: &Zone) {
        let lowest = zone.periods.iter().map(|p| p.offset).min().unwrap_or(0);
        self.line("BEGIN:VTIMEZONE");
        self.line(&format!("TZID:{name}"));
        for period in &zone.periods {
            let kind = if period.offset > lowest { "DAYLIGHT" } else { "STANDARD" };
            let start = first_written(period);
            let onset = match period.yearly {
                Some((month, nth, weekday)) => {
                    let next_year = civil_from_days(start.div_euclid(DAY)).0 + 1;
                    nth_weekday(next_year, month, nth, weekday) * DAY + start.rem_euclid(DAY)
                }
                None => start,
            };
            let from = zone.offset_at_wall(onset - 1).unwrap_or(period.offset);
            self.line(&format!("BEGIN:{kind}"));
            self.line(&format!("DTSTART:{}", date_time(start)));
            self.line(&format!("TZOFFSETFROM:{}", offset(from)));
            self.line(&format!("TZOFFSETTO:{}", offset(period.offset)));
            if let Some((month, nth, weekday)) = period.yearly {
                let code = usize::try_from(weekday).ok().and_then(|w| DAYS.get(w)).map_or("", |(code, _)| *code);
                self.line(&format!("RRULE:FREQ=YEARLY;BYMONTH={month};BYDAY={nth}{code}"));
            }
            self.line(&format!("END:{kind}"));
        }
        self.line("END:VTIMEZONE");
    }
}

/// A period's start as a `VTIMEZONE` can write it. A period that has
/// applied since before any calendar was kept (a system zone's first one
/// begins at the far past) is written from 1601, the year Windows counts
/// from and where other programs expect such a beginning.
fn first_written(period: &Period) -> i64 {
    let earliest = days_from_civil(1601, 1, 1) * DAY;
    if period.start >= earliest {
        return period.start;
    }
    match period.yearly {
        Some((month, nth, weekday)) => nth_weekday(1601, month, nth, weekday) * DAY + period.start.rem_euclid(DAY),
        None => earliest,
    }
}

/// The zones the event's own times name.
fn event_zones(event: &Event) -> Vec<&str> {
    let mut used = identity_zones(event, &event.start);
    used.extend(event.end.iter().chain(&event.excluded).flat_map(zone_of));
    used
}

/// The zones of `start` and of the occurrence the event stands in for.
fn identity_zones<'a>(event: &'a Event, start: &'a Moment) -> Vec<&'a str> {
    std::iter::once(start).chain(&event.instead_of).flat_map(zone_of).collect()
}

fn zone_of(moment: &Moment) -> Option<&str> {
    match moment {
        Moment::Wall { zone: Some(zone), .. } => Some(zone),
        _ => None,
    }
}

fn moment_line(name: &str, moment: &Moment) -> String {
    match moment {
        Moment::Day(day) => format!("{name};VALUE=DATE:{}", date(*day)),
        Moment::Utc(at) => format!("{name}:{}Z", date_time(*at)),
        Moment::Wall { wall, zone } => match zone.as_deref().map(param_text).filter(|z| !z.is_empty()) {
            Some(zone) => format!("{name};TZID={}:{}", quoted(&zone), date_time(*wall)),
            None => format!("{name}:{}", date_time(*wall)),
        },
    }
}

/// `ATTENDEE` for `who` with `params` after its name, or `None` when its
/// address cannot be written.
fn attendee(who: &Address, params: &[(&str, String)]) -> Option<String> {
    person("ATTENDEE", who, params)
}

fn person(name: &str, who: &Address, params: &[(&str, String)]) -> Option<String> {
    let email = address(&who.email)?;
    let mut out = name.to_string();
    let cn = who.name.as_deref().map(param_text).filter(|n| !n.is_empty());
    for (param, value) in cn.map(|n| ("CN", n)).iter().chain(params) {
        out.push_str(&format!(";{param}={}", quoted(value)));
    }
    out.push_str(&format!(":mailto:{email}"));
    Some(out)
}

/// An address that can follow `mailto:` as it is: one `@` with something
/// either side, and only the characters an address is made of. Anything
/// else (a space, a quote, a line break, a `:` or `;` that would start
/// another part of the line, text that is not ASCII) is refused.
fn address(email: &str) -> Option<&str> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || "!#$%&'*+-/=?^_`{|}~.@".contains(c);
    let (local, domain) = email.split_once('@')?;
    let fine = !local.is_empty() && !domain.is_empty() && !domain.contains('@') && email.chars().all(allowed);
    fine.then_some(email)
}

/// A parameter value on one line. A `"` cannot be written inside a quoted
/// value, so it becomes `'`.
fn param_text(value: &str) -> String {
    line(value).replace('"', "'")
}

/// `value` in quotes when it holds a character that would otherwise end
/// the parameter or the line's head.
fn quoted(value: &str) -> String {
    if value.contains([';', ':', ',']) { format!("\"{value}\"") } else { value.to_string() }
}

/// A text value escaped (RFC 5545 §3.3.11). A line break is written as
/// `\n`, and every other control character is dropped, so no value can
/// end its line.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push('\t'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// `content` folded at 75 octets, never inside a character; each line
/// after the first begins with a space, which unfolding removes.
fn fold(content: &str, newline: &str) -> String {
    let mut out = String::with_capacity(content.len() + content.len() / 32);
    let (mut from, mut room) = (0, FOLD);
    for (i, c) in content.char_indices() {
        if i + c.len_utf8() - from > room {
            out.push_str(&content[from..i]);
            out.push_str(newline);
            out.push(' ');
            from = i;
            room = FOLD - 1;
        }
    }
    out.push_str(&content[from..]);
    out
}

fn rule_text(rule: &Rule, start: &Moment, zones: &BTreeMap<String, Zone>) -> String {
    let freq = match rule.freq {
        Freq::Daily => "DAILY",
        Freq::Weekly => "WEEKLY",
        Freq::Monthly => "MONTHLY",
        Freq::Yearly => "YEARLY",
    };
    let mut parts = vec![format!("FREQ={freq}")];
    if rule.interval != 1 {
        parts.push(format!("INTERVAL={}", rule.interval));
    }
    if let Some(count) = rule.count {
        parts.push(format!("COUNT={count}"));
    }
    if let Some(until) = rule.until {
        // RFC 5545 asks for a date when the start is a day, an instant
        // when the start names a zone, and a floating time when it does
        // not. The limit is held on the clock the event repeats on, so
        // for a zone that is known it is turned back into the instant.
        let written = match start {
            Moment::Day(_) if until.rem_euclid(DAY) == DAY - 1 => date(until.div_euclid(DAY)),
            Moment::Wall { zone: None, .. } => date_time(until),
            Moment::Wall { zone: Some(zone), .. } => {
                let offset = zones.get(zone).and_then(|z| z.offset_at_wall(until)).unwrap_or(0);
                format!("{}Z", date_time(until - offset))
            }
            _ => format!("{}Z", date_time(until)),
        };
        parts.push(format!("UNTIL={written}"));
    }
    // Without a month, a yearly rule's days fall in every month; the
    // reader works a yearly rule out in the month the event began in.
    if rule.freq == Freq::Yearly && !(rule.by_day.is_empty() && rule.by_month_day.is_empty()) {
        parts.push(format!("BYMONTH={}", civil_from_days(start.wall().div_euclid(DAY)).1));
    }
    let days: Vec<String> = rule
        .by_day
        .iter()
        .filter_map(|&(nth, weekday)| {
            let (code, _) = DAYS.get(usize::try_from(weekday).ok()?)?;
            Some(if nth == 0 { (*code).to_string() } else { format!("{nth}{code}") })
        })
        .collect();
    if !days.is_empty() {
        parts.push(format!("BYDAY={}", days.join(",")));
    }
    if !rule.by_month_day.is_empty() {
        let days: Vec<String> = rule.by_month_day.iter().map(i64::to_string).collect();
        parts.push(format!("BYMONTHDAY={}", days.join(",")));
    }
    parts.join(";")
}

/// `YYYYMMDD`. The day is held to a range far wider than any calendar's,
/// so the arithmetic of a nonsense day cannot overflow.
fn date(day: i64) -> String {
    let (year, month, day) = civil_from_days(day.clamp(-100_000_000, 100_000_000));
    format!("{year:04}{month:02}{day:02}")
}

/// `YYYYMMDDTHHMMSS`, of seconds read as if UTC.
fn date_time(at: i64) -> String {
    let seconds = at.rem_euclid(DAY);
    format!("{}T{:02}{:02}{:02}", date(at.div_euclid(DAY)), seconds / 3600, seconds % 3600 / 60, seconds % 60)
}

/// `±HHMM`, with seconds only when there are some.
fn offset(seconds: i64) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let n = seconds.unsigned_abs();
    let mut out = format!("{sign}{:02}{:02}", n / 3600, n % 3600 / 60);
    if !n.is_multiple_of(60) {
        out.push_str(&format!("{:02}", n % 60));
    }
    out
}

/// A length as `[-]P[nD][T[nH][nM][nS]]`.
fn duration(seconds: i64) -> String {
    let n = seconds.unsigned_abs();
    if n == 0 {
        return "PT0S".to_string();
    }
    let day = DAY.unsigned_abs();
    let (days, rest) = (n / day, n % day);
    let mut out = String::from(if seconds < 0 { "-P" } else { "P" });
    if days > 0 {
        out.push_str(&format!("{days}D"));
    }
    if rest > 0 {
        out.push('T');
        for (count, unit) in [(rest / 3600, 'H'), (rest % 3600 / 60, 'M'), (rest % 60, 'S')] {
            if count > 0 {
                out.push_str(&format!("{count}{unit}"));
            }
        }
    }
    out
}

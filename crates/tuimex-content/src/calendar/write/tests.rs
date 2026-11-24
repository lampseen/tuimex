use super::*;
use crate::calendar::calendar;
use tuimex_core::calendar::{Calendar, Guest, Method};

const HOUR: i64 = 3600;
const STAMP: i64 = 1_795_000_000;

// The files under `tests/calendars/`, copied: no source of this crate may
// read a file or include one (ADR-007).
const GOOGLE: &str = r#"BEGIN:VCALENDAR
PRODID:-//Google Inc//Google Calendar 70.9054//EN
VERSION:2.0
CALSCALE:GREGORIAN
BEGIN:VTIMEZONE
TZID:America/New_York
X-LIC-LOCATION:America/New_York
BEGIN:DAYLIGHT
TZOFFSETFROM:-0500
TZOFFSETTO:-0400
TZNAME:EDT
DTSTART:19700308T020000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU
END:DAYLIGHT
BEGIN:STANDARD
TZOFFSETFROM:-0400
TZOFFSETTO:-0500
TZNAME:EST
DTSTART:19701101T020000
RRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
DTSTART;TZID=America/New_York:20261102T093000
DTEND;TZID=America/New_York:20261102T094500
RRULE:FREQ=WEEKLY;WKST=SU;UNTIL=20261231T235959Z;BYDAY=MO,TU,WE,TH,FR
DTSTAMP:20261123T140500Z
ORGANIZER;CN=Lars Lindqvist:mailto:lars@example.org
UID:0a1b2c3d4e5f6g7h8i9j@google.example.com
ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;CN=Robin
  Vale;X-NUM-GUESTS=0:mailto:robin.vale@example.org
ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=
 TRUE;CN=Mei Tanaka;X-NUM-GUESTS=0:mailto:mei@example.org
X-GOOGLE-CONFERENCE:https://meet.example.org/r/standup
CREATED:20260901T100000Z
DESCRIPTION:Join: https://meet.example.org/r/standup\n\nOr dial +47 21 00 00
  00\, code 4471
LAST-MODIFIED:20261123T140500Z
LOCATION:
SEQUENCE:1
STATUS:CONFIRMED
SUMMARY:Standup
TRANSP:OPAQUE
BEGIN:VALARM
ACTION:DISPLAY
DESCRIPTION:This is an event reminder
TRIGGER:-P0DT0H10M0S
END:VALARM
END:VEVENT
BEGIN:VEVENT
DTSTART;TZID=America/New_York:20261126T110000
DTEND;TZID=America/New_York:20261126T111500
DTSTAMP:20261123T140500Z
ORGANIZER;CN=Lars Lindqvist:mailto:lars@example.org
UID:0a1b2c3d4e5f6g7h8i9j@google.example.com
RECURRENCE-ID;TZID=America/New_York:20261126T093000
SEQUENCE:2
STATUS:CONFIRMED
SUMMARY:Standup (late, holiday)
TRANSP:OPAQUE
END:VEVENT
END:VCALENDAR
"#;
const PROTON: &str = r#"BEGIN:VCALENDAR
PRODID:-//Proton AG//ProtonCalendar 1.0.0//EN
VERSION:2.0
CALSCALE:GREGORIAN
METHOD:PUBLISH
X-WR-CALNAME:Personal
X-WR-TIMEZONE:Europe/Oslo
BEGIN:VTIMEZONE
TZID:Europe/Oslo
LAST-MODIFIED:20260101T000000Z
X-LIC-LOCATION:Europe/Oslo
BEGIN:DAYLIGHT
TZNAME:CEST
TZOFFSETFROM:+0100
TZOFFSETTO:+0200
DTSTART:19700329T020000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU
END:DAYLIGHT
BEGIN:STANDARD
TZNAME:CET
TZOFFSETFROM:+0200
TZOFFSETTO:+0100
DTSTART:19701025T030000
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
UID:kq3Vx0aT9z8m@proton.example.net
DTSTAMP:20261120T091500Z
DTSTART;TZID=Europe/Oslo:20261124T110000
DTEND;TZID=Europe/Oslo:20261124T114500
SUMMARY:Dentist
LOCATION:Kirkegata 12
SEQUENCE:0
STATUS:CONFIRMED
X-PM-SESSION-KEY:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
X-PM-SHARED-EVENT-ID:Zm9vYmFy
END:VEVENT
BEGIN:VEVENT
UID:w8Lp2c5Nn1Yd@proton.example.net
DTSTAMP:20261001T120000Z
DTSTART;TZID=Europe/Oslo:20261006T180000
DTEND;TZID=Europe/Oslo:20261006T200000
SUMMARY:Climbing
RRULE:FREQ=WEEKLY;BYDAY=TU
EXDATE;TZID=Europe/Oslo:20261027T180000
SEQUENCE:2
BEGIN:VALARM
ACTION:DISPLAY
TRIGGER:-PT30M
END:VALARM
END:VEVENT
BEGIN:VEVENT
UID:c1Ab7n3Qe5Rt@proton.example.net
DTSTAMP:20261101T080000Z
DTSTART;VALUE=DATE:20261127
DTEND;VALUE=DATE:20261130
SUMMARY:Cabin weekend
LOCATION:Ridge saddle hut
DESCRIPTION:Two cars.\nGreta drives the first\, Robin the second.
ORGANIZER;CN=Tomas Ferreira:mailto:tomas@example.net
ATTENDEE;CN=Robin Vale;PARTSTAT=ACCEPTED:mailto:robin@example.com
ATTENDEE;CN=Greta Holm;PARTSTAT=NEEDS-ACTION:mailto:greta@example.com
END:VEVENT
END:VCALENDAR
"#;

fn at(year: i64, month: i64, day: i64, hour: i64) -> i64 {
    days_from_civil(year, month, day) * DAY + hour * HOUR
}

fn who(email: &str, name: Option<&str>) -> Address {
    Address { name: name.map(str::to_string), email: email.to_string() }
}

/// Europe/Berlin by its rule, as a calendar defines it.
fn berlin() -> Zone {
    Zone {
        periods: vec![
            Period { start: at(1970, 3, 29, 2), offset: 2 * HOUR, yearly: Some((3, -1, 0)) },
            Period { start: at(1970, 10, 25, 3), offset: HOUR, yearly: Some((10, -1, 0)) },
        ],
    }
}

fn zones() -> BTreeMap<String, Zone> {
    BTreeMap::from([("Europe/Berlin".to_string(), berlin()), ("Asia/Kolkata".to_string(), Zone::fixed(19_800))])
}

fn wall(at: i64, zone: Option<&str>) -> Moment {
    Moment::Wall { wall: at, zone: zone.map(str::to_string) }
}

fn plain(start: Moment) -> Event {
    Event {
        uid: "plain@example.com".into(),
        sequence: 0,
        instead_of: None,
        summary: "Plain".into(),
        start,
        end: None,
        location: None,
        description: None,
        organizer: None,
        guests: Vec::new(),
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

fn rule(freq: Freq) -> Rule {
    Rule { freq, interval: 1, count: None, until: None, by_day: Vec::new(), by_month_day: Vec::new() }
}

/// An event with every field the reader reads.
fn full() -> Event {
    Event {
        uid: "full-1@example.com".into(),
        sequence: 4,
        instead_of: Some(wall(at(2026, 11, 10, 9), Some("Europe/Berlin"))),
        summary: "Plan, review; and \\ ship — ünïcödé ✓".into(),
        start: wall(at(2026, 11, 10, 11), Some("Europe/Berlin")),
        end: Some(wall(at(2026, 11, 10, 12), Some("Mars/Olympus"))),
        location: Some("Room 4; floor 2, east".into()),
        description: Some("Agenda:\n\n1. Budget, costs; risks\n2. \\ backslash".into()),
        organizer: Some(who("ada@example.com", Some("Ada Example"))),
        guests: vec![
            Guest { who: who("grace@example.org", Some("Grace; Hopper: Admiral")), said: Said::Accepted },
            Guest { who: who("lin@example.net", None), said: Said::Declined },
            Guest { who: who("kim@example.org", Some("Kim")), said: Said::Tentative },
            Guest { who: who("sam@example.org", None), said: Said::Nothing },
        ],
        cancelled: true,
        links: vec!["https://meet.example.org/r/one".into(), "https://example.org/event?a=1;b=2,c".into()],
        repeat: Some(Repeat::Rule(Rule { count: Some(10), by_day: vec![(0, 1), (0, 3)], ..rule(Freq::Weekly) })),
        excluded: vec![
            wall(at(2026, 11, 17, 11), Some("Europe/Berlin")),
            Moment::Utc(at(2026, 11, 24, 10)),
            wall(at(2026, 12, 1, 11), None),
            Moment::Day(days_from_civil(2026, 12, 8)),
        ],
        alarms: vec![600, 0, 2 * DAY + 90],
        more: Vec::new(),
    }
}

/// The one event `text` holds, with what the reader notes under More
/// left out: it is not written back.
fn read_one(text: &str) -> Event {
    let mut parsed = calendar(text);
    assert_eq!(parsed.events.len(), 1, "{text}");
    let mut event = parsed.events.remove(0);
    event.more.clear();
    event
}

fn round_trip(event: &Event, zones: &BTreeMap<String, Zone>) {
    let text = event_text(event, zones, STAMP);
    assert_well_formed(&text);
    let mut want = event.clone();
    want.more.clear();
    assert_eq!(read_one(&text), want, "{text}");
}

/// Every line ends in CRLF and is at most 75 octets.
fn assert_well_formed(text: &str) {
    assert!(text.ends_with(CRLF), "{text}");
    for physical in text.split_terminator(CRLF) {
        assert!(physical.len() <= FOLD, "{} octets: {physical}", physical.len());
        assert!(!physical.contains(['\r', '\n']), "a bare line break: {physical:?}");
    }
}

/// Each line unfolded, as name and parameter names.
fn shape(text: &str) -> Vec<(String, Vec<String>)> {
    let unfolded = text.replace("\r\n ", "");
    unfolded
        .split_terminator(CRLF)
        .map(|l| {
            let p = prop(l).unwrap_or_else(|| panic!("not a property: {l}"));
            (p.name, p.params.into_iter().map(|(n, _)| n).collect())
        })
        .collect()
}

fn names(text: &str) -> Vec<String> {
    shape(text).into_iter().map(|(n, _)| n).collect()
}

/// The unfolded lines between `BEGIN:VEVENT` and its end.
fn event_lines(text: &str) -> Vec<String> {
    let unfolded = text.replace("\r\n ", "");
    let lines: Vec<&str> = unfolded.split_terminator(CRLF).collect();
    let begin = lines.iter().position(|l| *l == "BEGIN:VEVENT").unwrap_or(0);
    let end = lines.iter().position(|l| *l == "END:VEVENT").unwrap_or(lines.len());
    lines[begin + 1..end].iter().map(|l| l.to_string()).collect()
}

fn event_names(text: &str) -> Vec<String> {
    event_lines(text).iter().filter_map(|l| prop(l)).map(|p| p.name).collect()
}

#[test]
fn every_field_the_reader_reads_comes_back() {
    round_trip(&full(), &zones());
    let mut not_cancelled = full();
    not_cancelled.cancelled = false;
    not_cancelled.instead_of = Some(Moment::Utc(at(2026, 11, 10, 8)));
    round_trip(&not_cancelled, &zones());
}

#[test]
fn each_kind_of_moment_comes_back() {
    let day = days_from_civil(2026, 7, 1);
    for (start, end) in [
        (Moment::Day(day), Some(Moment::Day(day + 3))),
        (Moment::Utc(at(2026, 7, 1, 10)), Some(Moment::Utc(at(2026, 7, 1, 11) + 59))),
        (wall(at(2026, 7, 1, 10), None), None),
        (wall(at(2026, 7, 1, 10), Some("Europe/Berlin")), Some(wall(at(2026, 7, 1, 11), Some("Europe/Berlin")))),
        (wall(at(2026, 7, 1, 10), Some("Asia/Kolkata")), None),
        (wall(at(2026, 7, 1, 10), Some("Nowhere/Known")), None),
        (wall(at(1999, 12, 31, 23) + 3599, Some("Has space, comma; and: colon")), None),
        (Moment::Day(days_from_civil(1, 1, 1)), Some(Moment::Day(days_from_civil(9999, 12, 31)))),
    ] {
        round_trip(&Event { end, ..plain(start) }, &zones());
    }
}

#[test]
fn a_zone_that_is_defined_is_written_so_another_program_can_place_the_time() {
    let event = Event { end: Some(wall(at(2026, 7, 1, 11), Some("Asia/Kolkata"))), ..full() };
    let text = event_text(&event, &zones(), STAMP);
    let parsed = calendar(&text);
    assert_eq!(parsed.zones["Europe/Berlin"], berlin(), "{text}");
    assert!(text.contains(
        "BEGIN:DAYLIGHT\r\nDTSTART:19700329T020000\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\n\
         RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\n"
    ));
    assert!(text.contains(
        "BEGIN:STANDARD\r\nDTSTART:19701025T030000\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\n\
         RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\n"
    ));
    // A zone in force since the far past is written from 1601, and keeps
    // its offset.
    assert!(text.contains("TZID:Asia/Kolkata\r\nBEGIN:STANDARD\r\nDTSTART:16010101T000000\r\n"), "{text}");
    assert_eq!(parsed.zones["Asia/Kolkata"].offset_at_wall(at(2026, 7, 1, 10)), Some(19_800));
    assert!(!parsed.zones.contains_key("Mars/Olympus"), "a zone nothing defines is named and not invented");
    assert!(event_text(&full(), &zones(), STAMP).contains("\r\nDTEND;TZID=Mars/Olympus:20261110T120000\r\n"));
    assert_eq!(text.matches("BEGIN:VTIMEZONE").count(), 2, "each zone once");
    assert!(!event_text(&plain(Moment::Utc(0)), &zones(), STAMP).contains("VTIMEZONE"), "only the zones used");
}

#[test]
fn a_zone_with_seconds_in_its_offset_keeps_them() {
    let odd = Zone { periods: vec![Period { start: at(1900, 1, 1, 0), offset: -(HOUR + 30 * 60 + 15), yearly: None }] };
    let zones = BTreeMap::from([("Odd/Seconds".to_string(), odd)]);
    let text = event_text(&plain(wall(at(2026, 7, 1, 10), Some("Odd/Seconds"))), &zones, STAMP);
    assert!(text.contains("TZOFFSETTO:-013015\r\n"), "{text}");
}

#[test]
fn each_rule_the_reader_works_out_comes_back() {
    let start = wall(at(2026, 11, 10, 9), Some("Europe/Berlin"));
    for (start, rule) in [
        (start.clone(), rule(Freq::Daily)),
        (start.clone(), Rule { interval: 3, by_day: vec![(0, 1), (0, 2)], ..rule(Freq::Daily) }),
        (start.clone(), Rule { interval: 2, count: Some(10), ..rule(Freq::Weekly) }),
        (start.clone(), Rule { by_day: (1..=5).map(|w| (0, w)).collect(), ..rule(Freq::Weekly) }),
        (start.clone(), Rule { until: Some(at(2026, 12, 31, 23) + 3599), ..rule(Freq::Weekly) }),
        (start.clone(), Rule { by_day: vec![(2, 2)], ..rule(Freq::Monthly) }),
        (start.clone(), Rule { by_day: vec![(-1, 5)], ..rule(Freq::Monthly) }),
        (start.clone(), Rule { by_month_day: vec![15, -1], ..rule(Freq::Monthly) }),
        (start.clone(), Rule { by_month_day: vec![10], ..rule(Freq::Yearly) }),
        (start.clone(), Rule { by_day: vec![(4, 4)], ..rule(Freq::Yearly) }),
        (start.clone(), Rule { interval: 4, ..rule(Freq::Yearly) }),
        (Moment::Day(days_from_civil(2026, 11, 10)), Rule { until: Some(at(2027, 1, 1, 0) - 1), ..rule(Freq::Weekly) }),
        (Moment::Day(days_from_civil(2026, 11, 10)), Rule { until: Some(at(2027, 1, 1, 12)), ..rule(Freq::Weekly) }),
        (Moment::Utc(at(2026, 11, 10, 9)), Rule { until: Some(at(2027, 1, 1, 9)), ..rule(Freq::Daily) }),
        (wall(at(2026, 11, 10, 9), None), Rule { until: Some(at(2027, 1, 1, 9)), ..rule(Freq::Daily) }),
    ] {
        round_trip(&Event { repeat: Some(Repeat::Rule(rule)), ..plain(start) }, &zones());
    }
}

#[test]
fn a_rule_is_written_as_rfc_5545_asks() {
    let text = |start: Moment, rule: Rule| {
        let event = Event { repeat: Some(Repeat::Rule(rule)), ..plain(start) };
        event_lines(&event_text(&event, &zones(), STAMP)).into_iter().find(|l| l.starts_with("RRULE:"))
    };
    let until = Some(at(2026, 12, 31, 23) + 3599);
    assert_eq!(
        text(Moment::Day(days_from_civil(2026, 11, 10)), Rule { until, ..rule(Freq::Weekly) }).as_deref(),
        Some("RRULE:FREQ=WEEKLY;UNTIL=20261231"),
        "a date for a day"
    );
    assert_eq!(
        text(wall(0, Some("Europe/Berlin")), Rule { until, ..rule(Freq::Weekly) }).as_deref(),
        Some("RRULE:FREQ=WEEKLY;UNTIL=20261231T225959Z"),
        "the instant, for a start in a zone: a second before midnight in Berlin's winter is 22:59:59 UTC"
    );
    assert_eq!(
        text(wall(0, None), Rule { until, ..rule(Freq::Weekly) }).as_deref(),
        Some("RRULE:FREQ=WEEKLY;UNTIL=20261231T235959"),
        "floating for a floating start"
    );
    assert_eq!(
        text(Moment::Utc(at(2026, 11, 10, 9)), Rule { by_month_day: vec![10], ..rule(Freq::Yearly) }).as_deref(),
        Some("RRULE:FREQ=YEARLY;BYMONTH=11;BYMONTHDAY=10"),
        "a yearly rule names its month, or its days fall in every month"
    );
}

#[test]
fn a_rule_tuimex_does_not_work_out_goes_back_as_it_came() {
    for written in
        ["FREQ=HOURLY", "FREQ=YEARLY;BYSETPOS=-1;BYDAY=FR", "FREQ=WEEKLY;BYWEEKNO=2", "FREQ=MONTHLY;BYMONTH=3"]
    {
        let event = Event { repeat: Some(Repeat::Unread(written.into())), ..plain(Moment::Utc(at(2026, 11, 10, 9))) };
        round_trip(&event, &zones());
        assert!(event_text(&event, &zones(), STAMP).contains(&format!("\r\nRRULE:{written}\r\n")));
    }
}

#[test]
fn what_the_reader_gives_from_a_written_calendar_comes_back() {
    let text = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Berlin\r\n\
        BEGIN:DAYLIGHT\r\nDTSTART:19700329T020000\r\nTZOFFSETTO:+0200\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\n\
        BEGIN:STANDARD\r\nDTSTART:19701025T030000\r\nTZOFFSETTO:+0100\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\n\
        END:VTIMEZONE\r\n\
        BEGIN:VEVENT\r\nUID:a\\,b@example.com\r\nDTSTART;TZID=Europe/Berlin:20261103T090000\r\nDURATION:PT1H30M\r\n\
        RRULE:freq=weekly;wkst=su;until=20261231\r\nEXDATE;TZID=Europe/Berlin:20261110T090000,20261117T090000\r\n\
        SUMMARY:  Lunch\\, then\\nwork \\\\ \r\n play\u{1b}[2J\r\nDESCRIPTION:Agenda:\\n\\n 1. Budget \\n2. Hiring\\n\r\n\
        ORGANIZER;CN=\"Example; Ada: the first\":MAILTO:ada@example.com\r\n\
        ATTENDEE;PARTSTAT=declined;RSVP=TRUE:grace@example.org\r\nATTENDEE;CN=\"\":mailto:pat@example.org\r\n\
        URL:https://example.org/event\r\nCONFERENCE;VALUE=URI:https://meet.example.org/r/one\r\n\
        X-GOOGLE-CONFERENCE:https://meet.example.org/r/two\r\nSTATUS:CANCELLED\r\nCLASS:PRIVATE\r\n\
        BEGIN:VALARM\r\nTRIGGER:-P1DT2H\r\nEND:VALARM\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:b@example.com\r\nDTSTART;VALUE=DATE:20261103\r\nRRULE:FREQ=MONTHLY;BYDAY=-1FR;COUNT=3\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:c@example.com\r\nDTSTART:20261103T090000Z\r\nRECURRENCE-ID:20261102T090000Z\r\n\
        RRULE:FREQ=YEARLY;BYSETPOS=1\r\nSEQUENCE:7\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let parsed = calendar(text);
    assert_eq!(parsed.events.len(), 3);
    for event in &parsed.events {
        round_trip(event, &parsed.zones);
    }
}

#[test]
fn the_provider_files_come_back_event_by_event() {
    for file in [GOOGLE, PROTON] {
        let parsed: Calendar = calendar(file);
        assert!(!parsed.events.is_empty());
        for event in &parsed.events {
            round_trip(event, &parsed.zones);
            let again = calendar(&event_text(event, &parsed.zones, STAMP));
            for (name, zone) in &again.zones {
                assert_eq!(Some(zone), parsed.zones.get(name), "{name}");
            }
        }
    }
}

#[test]
fn long_text_is_folded_at_75_octets_and_unfolds_to_itself() {
    let long = format!("{}{}", "é".repeat(37), "x✓".repeat(250));
    let description = format!("{}\n{}", "word ".repeat(100).trim_end(), "日本語のテキスト".repeat(40));
    let event = Event { summary: long.clone(), description: Some(description.clone()), ..plain(Moment::Utc(0)) };
    let text = event_text(&event, &zones(), STAMP);
    assert_well_formed(&text);
    assert!(text.lines().count() > 20, "{text}");
    let back = read_one(&text);
    assert_eq!((back.summary, back.description), (long, Some(description)));
    // Folding at every offset: no character is split, whatever its width.
    for shift in 0..4 {
        let summary = format!("{}{}", "a".repeat(shift), "€😀ü".repeat(60));
        let text = event_text(&Event { summary: summary.clone(), ..plain(Moment::Utc(0)) }, &zones(), STAMP);
        assert_well_formed(&text);
        assert_eq!(read_one(&text).summary, summary);
    }
    assert_eq!(fold(&"a".repeat(75), CRLF), "a".repeat(75), "75 octets fit on one line");
    assert_eq!(fold(&"a".repeat(76), CRLF), format!("{}\r\n a", "a".repeat(75)));
}

#[test]
fn a_500_character_line_comes_back() {
    let location: String = (0..500).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
    round_trip(&Event { location: Some(location), ..plain(Moment::Utc(0)) }, &zones());
}

#[test]
fn guests_are_written_with_what_each_has_said() {
    let text = event_text(&full(), &zones(), STAMP);
    let attendees: Vec<String> = event_lines(&text).into_iter().filter(|l| l.starts_with("ATTENDEE")).collect();
    assert_eq!(
        attendees,
        vec![
            "ATTENDEE;CN=\"Grace; Hopper: Admiral\";PARTSTAT=ACCEPTED:mailto:grace@example.org",
            "ATTENDEE;PARTSTAT=DECLINED:mailto:lin@example.net",
            "ATTENDEE;CN=Kim;PARTSTAT=TENTATIVE:mailto:kim@example.org",
            "ATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:sam@example.org",
        ]
    );
}

#[test]
fn a_stored_event_has_no_method_and_says_who_wrote_it() {
    let text = event_text(&full(), &zones(), STAMP);
    assert!(text.starts_with("BEGIN:VCALENDAR\r\nPRODID:-//tuimex//EN\r\nVERSION:2.0\r\n"), "{text}");
    assert!(text.ends_with("END:VEVENT\r\nEND:VCALENDAR\r\n"));
    assert_eq!(calendar(&text).method, None);
    assert!(!names(&text).contains(&"METHOD".to_string()));
    let (y, m, d) = civil_from_days(STAMP.div_euclid(DAY));
    let s = STAMP.rem_euclid(DAY);
    let stamp = format!("DTSTAMP:{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z", s / 3600, s % 3600 / 60, s % 60);
    assert!(event_lines(&text).contains(&stamp), "{text}");
}

#[test]
fn a_reply_carries_the_event_s_identity_and_one_answer() {
    let me = who("kim@example.org", Some("Kim"));
    for (said, partstat) in [
        (Said::Accepted, "ACCEPTED"),
        (Said::Tentative, "TENTATIVE"),
        (Said::Declined, "DECLINED"),
        (Said::Nothing, "NEEDS-ACTION"),
    ] {
        let text = reply(&full(), &zones(), &me, said, STAMP).unwrap();
        assert_well_formed(&text);
        assert_eq!(
            event_names(&text),
            ["UID", "DTSTAMP", "SEQUENCE", "DTSTART", "RECURRENCE-ID", "ORGANIZER", "SUMMARY", "ATTENDEE"]
        );
        let lines = event_lines(&text);
        assert!(lines.contains(&format!("ATTENDEE;CN=Kim;PARTSTAT={partstat}:mailto:kim@example.org")), "{text}");
        assert!(lines.contains(&"SEQUENCE:4".to_string()));
        let parsed = calendar(&text);
        assert_eq!(parsed.method, Some(Method::Reply));
        let e = &parsed.events[0];
        assert_eq!(
            (e.uid.as_str(), &e.start, &e.instead_of),
            ("full-1@example.com", &full().start, &full().instead_of)
        );
        assert_eq!(e.guests, vec![Guest { who: me.clone(), said }]);
        assert_eq!(e.organizer, full().organizer);
        assert!(parsed.zones.contains_key("Europe/Berlin"), "the zone the start names is defined");
    }
    let bare = reply(&plain(Moment::Utc(0)), &zones(), &me, Said::Accepted, STAMP).unwrap();
    assert_eq!(event_names(&bare), ["UID", "DTSTAMP", "SEQUENCE", "DTSTART", "SUMMARY", "ATTENDEE"]);
    assert!(!bare.contains("VTIMEZONE"));
}

#[test]
fn an_answer_from_an_address_that_cannot_be_written_is_not_made() {
    for email in [
        "",
        "kim",
        "@example.org",
        "kim@",
        "kim@@example.org",
        "kim smith@example.org",
        "kim@example.org\r\nATTENDEE:mailto:evil@example.net",
        "kim@example.org:x",
        "kim;x@example.org",
        "kim,x@example.org",
        "<kim@example.org>",
        "\"kim\"@example.org",
        "kïm@example.org",
    ] {
        let me = who(email, None);
        assert_eq!(reply(&full(), &zones(), &me, Said::Accepted, STAMP), None, "{email:?}");
        assert_eq!(counter(&full(), &zones(), &me, Moment::Utc(0), None, STAMP), None, "{email:?}");
    }
    for email in ["kim@example.org", "kim.o'neil+cal@sub.example.org", "a!#$%&*/=?^_`{|}~-@example.net"] {
        assert!(reply(&full(), &zones(), &who(email, None), Said::Accepted, STAMP).is_some(), "{email}");
    }
}

#[test]
fn a_counter_proposes_a_time_as_one_tentative_guest() {
    let me = who("kim@example.org", None);
    let start = wall(at(2026, 11, 12, 15), Some("Asia/Kolkata"));
    let end = Moment::Utc(at(2026, 11, 12, 11));
    let text = counter(&full(), &zones(), &me, start.clone(), Some(end.clone()), STAMP).unwrap();
    assert_well_formed(&text);
    assert_eq!(
        event_names(&text),
        ["UID", "DTSTAMP", "SEQUENCE", "DTSTART", "DTEND", "RECURRENCE-ID", "ORGANIZER", "SUMMARY", "ATTENDEE"]
    );
    let parsed = calendar(&text);
    assert_eq!(parsed.method, Some(Method::Counter));
    let e = &parsed.events[0];
    assert_eq!((&e.start, &e.end, &e.instead_of), (&start, &Some(end), &full().instead_of));
    assert_eq!(e.guests, vec![Guest { who: me.clone(), said: Said::Tentative }]);
    assert_eq!(e.sequence, 4);
    assert_eq!(parsed.zones.keys().collect::<Vec<_>>(), ["Asia/Kolkata", "Europe/Berlin"]);
    let no_end = counter(&plain(Moment::Utc(0)), &zones(), &me, Moment::Utc(HOUR), None, STAMP).unwrap();
    assert!(!event_names(&no_end).contains(&"DTEND".to_string()));
}

#[test]
fn a_request_is_the_whole_event_with_each_guest_asked() {
    let mut event = full();
    event.cancelled = false;
    let text = request(&event, &zones(), STAMP);
    assert_well_formed(&text);
    let parsed = calendar(&text);
    assert_eq!(parsed.method, Some(Method::Request));
    let mut back = parsed.events[0].clone();
    back.more.clear();
    let asked: Vec<Guest> = event.guests.iter().map(|g| Guest { who: g.who.clone(), said: Said::Nothing }).collect();
    assert_eq!(back, Event { guests: asked, ..event.clone() });
    let attendees: Vec<String> = event_lines(&text).into_iter().filter(|l| l.starts_with("ATTENDEE")).collect();
    assert_eq!(
        attendees[0],
        "ATTENDEE;CN=\"Grace; Hopper: Admiral\";RSVP=TRUE;PARTSTAT=NEEDS-ACTION:mailto:grace@example.org"
    );
    assert_eq!(attendees[1], "ATTENDEE;RSVP=TRUE;PARTSTAT=NEEDS-ACTION:mailto:lin@example.net");
    assert_eq!(attendees.len(), 4);
    assert!(event_names(&text).contains(&"ORGANIZER".to_string()));
}

#[test]
fn a_cancel_names_the_event_one_version_on_with_every_guest() {
    let mut event = full();
    event.cancelled = false;
    let text = cancel(&event, &zones(), STAMP);
    assert_well_formed(&text);
    assert_eq!(
        event_names(&text),
        [
            "UID",
            "DTSTAMP",
            "SEQUENCE",
            "DTSTART",
            "RECURRENCE-ID",
            "SUMMARY",
            "STATUS",
            "ORGANIZER",
            "ATTENDEE",
            "ATTENDEE",
            "ATTENDEE",
            "ATTENDEE"
        ]
    );
    let parsed = calendar(&text);
    assert_eq!(parsed.method, Some(Method::Cancel));
    let e = &parsed.events[0];
    assert!(e.cancelled);
    assert_eq!((e.sequence, e.uid.as_str()), (5, "full-1@example.com"));
    assert_eq!(
        e.guests.iter().map(|g| g.who.clone()).collect::<Vec<_>>(),
        event.guests.iter().map(|g| g.who.clone()).collect::<Vec<_>>()
    );
    assert_eq!(e.organizer, event.organizer);
    let last = Event { sequence: u32::MAX, ..plain(Moment::Utc(0)) };
    assert!(cancel(&last, &zones(), STAMP).contains(&format!("SEQUENCE:{}\r\n", u32::MAX)));
}

#[test]
fn hostile_text_adds_no_line_no_parameter_and_no_property() {
    let evil = "x\r\nATTENDEE:mailto:evil@example.net\nEND:VEVENT\r\nBEGIN:VEVENT;\"a\":b,c\\";
    let hostile = Event {
        uid: format!("u{evil}"),
        summary: evil.into(),
        location: Some(evil.into()),
        description: Some(evil.into()),
        start: wall(at(2026, 7, 1, 10), Some(evil)),
        end: Some(wall(at(2026, 7, 1, 11), Some("Europe/Berlin"))),
        instead_of: Some(wall(at(2026, 7, 1, 9), Some("a\"b;c:d,e"))),
        organizer: Some(who("ada@example.com", Some(evil))),
        guests: vec![
            Guest { who: who("grace@example.org", Some("\"Grace\"; ROLE=CHAIR: x,y")), said: Said::Accepted },
            Guest { who: who("evil@example.net\r\nATTENDEE:mailto:x@example.net", None), said: Said::Accepted },
            Guest { who: who("evil@example.net;RSVP=TRUE", None), said: Said::Accepted },
        ],
        links: vec![format!("https://example.org/{evil}")],
        repeat: Some(Repeat::Unread(format!("FREQ=HOURLY{evil}"))),
        excluded: vec![wall(at(2026, 7, 8, 10), Some(evil))],
        alarms: vec![600],
        ..plain(Moment::Utc(0))
    };
    let benign = Event {
        uid: "u".into(),
        summary: "x".into(),
        location: Some("x".into()),
        description: Some("x".into()),
        start: wall(at(2026, 7, 1, 10), Some("x")),
        instead_of: Some(wall(at(2026, 7, 1, 9), Some("y"))),
        organizer: Some(who("ada@example.com", Some("x"))),
        guests: vec![Guest { who: who("grace@example.org", Some("x")), said: Said::Accepted }],
        links: vec!["https://example.org/".into()],
        repeat: Some(Repeat::Unread("FREQ=HOURLY".into())),
        excluded: vec![wall(at(2026, 7, 8, 10), Some("x"))],
        ..hostile.clone()
    };
    let me = who("kim@example.org", Some(evil));
    for (name, written, plain) in [
        ("stored", event_text(&hostile, &zones(), STAMP), event_text(&benign, &zones(), STAMP)),
        ("request", request(&hostile, &zones(), STAMP), request(&benign, &zones(), STAMP)),
        ("cancel", cancel(&hostile, &zones(), STAMP), cancel(&benign, &zones(), STAMP)),
        (
            "reply",
            reply(&hostile, &zones(), &me, Said::Declined, STAMP).unwrap(),
            reply(&benign, &zones(), &me, Said::Declined, STAMP).unwrap(),
        ),
        (
            "counter",
            counter(&hostile, &zones(), &me, hostile.start.clone(), None, STAMP).unwrap(),
            counter(&benign, &zones(), &me, benign.start.clone(), None, STAMP).unwrap(),
        ),
    ] {
        assert_well_formed(&written);
        assert_eq!(shape(&written), shape(&plain), "{name}: {written}");
    }

    let back = read_one(&event_text(&hostile, &zones(), STAMP));
    let flat = "x ATTENDEE:mailto:evil@example.net END:VEVENT BEGIN:VEVENT;\"a\":b,c\\";
    assert_eq!(back.summary, flat, "the summary is the text, minus the line breaks the reader turns to spaces");
    assert_eq!(back.uid, format!("u{flat}"));
    assert_eq!(back.location.as_deref(), Some(flat));
    assert_eq!(
        back.description.as_deref(),
        Some("x\nATTENDEE:mailto:evil@example.net\nEND:VEVENT\nBEGIN:VEVENT;\"a\":b,c\\")
    );
    let zone = Some("x ATTENDEE:mailto:evil@example.net END:VEVENT BEGIN:VEVENT;'a':b,c\\".to_string());
    assert_eq!(back.start, Moment::Wall { wall: at(2026, 7, 1, 10), zone: zone.clone() }, "a \" in a zone is a '");
    assert_eq!(back.excluded, vec![Moment::Wall { wall: at(2026, 7, 8, 10), zone }]);
    assert_eq!(back.instead_of, Some(wall(at(2026, 7, 1, 9), Some("a'b;c:d,e"))));
    assert_eq!(back.organizer.and_then(|o| o.name).as_deref(), Some(flat.replace('"', "'").as_str()));
    assert_eq!(
        back.guests,
        vec![Guest { who: who("grace@example.org", Some("'Grace'; ROLE=CHAIR: x,y")), said: Said::Accepted }]
    );
    assert_eq!(back.links, vec![format!("https://example.org/{flat}")]);
    assert_eq!(back.repeat, Some(Repeat::Unread(format!("FREQ=HOURLY{flat}"))));
}

#[test]
fn a_summary_cannot_smuggle_an_attendee() {
    let event = Event { summary: "x\r\nATTENDEE:mailto:evil@example.net".into(), ..plain(Moment::Utc(0)) };
    let text = event_text(&event, &zones(), STAMP);
    assert!(event_lines(&text).contains(&"SUMMARY:x\\nATTENDEE:mailto:evil@example.net".to_string()), "{text}");
    let back = read_one(&text);
    assert_eq!(back.summary, "x ATTENDEE:mailto:evil@example.net");
    assert!(back.guests.is_empty());
}

#[test]
fn a_uid_is_sixteen_bytes_in_hex() {
    let random: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa, 0xb, 0xc, 0xd, 0xfe, 0xff];
    assert_eq!(new_uid(random), "000102030405060708090a0b0c0dfeff@tuimex");
}

#[test]
fn a_length_is_written_as_the_reader_reads_it() {
    for seconds in [0, 1, 59, 600, 3600, 5400, DAY, DAY + 3661, 7 * DAY, -600, -DAY - 1] {
        assert_eq!(super::super::duration(&duration(seconds)), Some(seconds), "{seconds}");
    }
    assert_eq!(duration(-600), "-PT10M");
    assert_eq!(duration(DAY + 2 * HOUR), "P1DT2H");
}

// with_answer

fn me() -> Address {
    who("Mei@Example.org", None)
}

/// The lines of `before` and `after` that differ, unfolded.
fn changed(before: &str, after: &str) -> Vec<(String, String)> {
    let unfold = |t: &str| t.replace("\r\n", "\n").replace("\n ", "").replace("\n\t", "");
    let (b, a) = (unfold(before), unfold(after));
    assert_eq!(b.lines().count(), a.lines().count());
    b.lines().zip(a.lines()).filter(|(x, y)| x != y).map(|(x, y)| (x.to_string(), y.to_string())).collect()
}

#[test]
fn an_answer_changes_one_line_of_the_provider_s_text() {
    let after = with_answer(GOOGLE, &me(), Said::Accepted).unwrap();
    assert_eq!(
        changed(GOOGLE, &after),
        vec![(
            "ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=Mei Tanaka;X-NUM-GUESTS=0:mailto:mei@example.org".to_string(),
            "ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;RSVP=TRUE;CN=Mei Tanaka;X-NUM-GUESTS=0:mailto:mei@example.org".to_string(),
        )]
    );
    // Byte for byte: everything before and after the changed line is the
    // file's own, folding and line ends included.
    let at = GOOGLE.find("ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION").unwrap();
    let end = at + GOOGLE[at..].find("X-GOOGLE-CONFERENCE").unwrap();
    assert_eq!(after[..at], GOOGLE[..at]);
    let tail = &GOOGLE[end..];
    assert!(after.ends_with(tail));
    let new = &after[at..after.len() - tail.len()];
    assert!(!new.contains('\r'), "the file's own line ends: {new:?}");
    assert!(new.split_terminator('\n').all(|l| l.len() <= FOLD), "{new}");
    let parsed = calendar(&after);
    assert_eq!(parsed.events[0].guests[1].said, Said::Accepted);
    assert_eq!(parsed.events[0].guests[0].said, Said::Accepted, "Robin's answer is Robin's");
}

#[test]
fn an_answer_is_added_where_there_was_none_and_replaces_one_that_was() {
    let text = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:u@example.com\r\nDTSTART:20260701T100000Z\r\n\
        ATTENDEE;RSVP=TRUE;CN=\"Mei; T\":MAILTO:MEI@EXAMPLE.ORG\r\n\
        ATTENDEE;partstat=declined:mailto:robin@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let after = with_answer(text, &me(), Said::Tentative).unwrap();
    assert_eq!(
        changed(text, &after),
        vec![(
            "ATTENDEE;RSVP=TRUE;CN=\"Mei; T\":MAILTO:MEI@EXAMPLE.ORG".to_string(),
            "ATTENDEE;RSVP=TRUE;CN=\"Mei; T\";PARTSTAT=TENTATIVE:MAILTO:MEI@EXAMPLE.ORG".to_string()
        )]
    );
    assert!(after.contains("\r\nATTENDEE;RSVP=TRUE;CN=\"Mei; T\";PARTSTAT=TENTATIVE:MAILTO:MEI@EXAMPLE.ORG\r\n"));
    let robin = who("mailto:Robin@example.org", None);
    let again = with_answer(text, &robin, Said::Nothing).unwrap();
    assert!(again.contains("\r\nATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:robin@example.org\r\n"), "{again}");
    assert_eq!(with_answer(text, &robin, Said::Declined).unwrap(), text, "the same answer changes nothing");
}

#[test]
fn an_answer_reaches_the_series_and_each_moved_occurrence() {
    let text = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:s@example.com\r\nDTSTART:20260701T100000Z\r\nRRULE:FREQ=DAILY\r\n\
        ATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:mei@example.org\r\n\
        BEGIN:VALARM\r\nACTION:EMAIL\r\nTRIGGER:-PT5M\r\nATTENDEE:mailto:mei@example.org\r\nEND:VALARM\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:s@example.com\r\nRECURRENCE-ID:20260703T100000Z\r\nDTSTART:20260703T120000Z\r\n\
        ATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:mei@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let after = with_answer(text, &me(), Said::Declined).unwrap();
    assert_eq!(changed(text, &after).len(), 2, "{after}");
    assert!(
        after.contains("VALARM\r\nACTION:EMAIL\r\nTRIGGER:-PT5M\r\nATTENDEE:mailto:mei@example.org\r\n"),
        "an alarm's address is not a guest"
    );
    assert!(calendar(&after).events.iter().all(|e| e.guests[0].said == Said::Declined));
}

#[test]
fn no_answer_is_made_for_who_is_not_invited() {
    assert_eq!(with_answer(GOOGLE, &who("kim@example.org", None), Said::Accepted), None);
    assert_eq!(with_answer(GOOGLE, &who("", None), Said::Accepted), None);
    assert_eq!(with_answer(PROTON, &me(), Said::Accepted), None);
    assert_eq!(with_answer("", &me(), Said::Accepted), None);
    assert_eq!(with_answer("ATTENDEE:mailto:mei@example.org", &me(), Said::Accepted), None, "not in an event");
    assert_eq!(with_answer(GOOGLE, &who("lars@example.org", None), Said::Accepted), None, "the organizer is no guest");
}

#[test]
fn an_answer_on_a_proton_file_keeps_its_bare_line_ends() {
    let greta = who("greta@example.com", None);
    let after = with_answer(PROTON, &greta, Said::Accepted).unwrap();
    assert!(after.contains("\nATTENDEE;CN=Greta Holm;PARTSTAT=ACCEPTED:mailto:greta@example.com\n"), "{after}");
    assert_eq!(changed(PROTON, &after).len(), 1);
    assert_eq!(after.len(), PROTON.len() + "ACCEPTED".len() - "NEEDS-ACTION".len());
}

#[test]
fn a_long_answered_line_is_folded_and_unfolds_to_itself() {
    let name = "Mei ".repeat(40);
    let text = format!(
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:u@example.com\r\nDTSTART:20260701T100000Z\r\n\
         ATTENDEE;CN=\"{name}\":mailto:mei@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR"
    );
    let after = with_answer(&text, &me(), Said::Accepted).unwrap();
    assert!(after.split("\r\n").all(|l| l.len() <= FOLD), "{after}");
    assert!(after.ends_with("\r\nEND:VCALENDAR"), "no line end added where there was none");
    assert_eq!(changed(&text, &after).len(), 1);
    assert_eq!(calendar(&after).events[0].guests[0].said, Said::Accepted);
}

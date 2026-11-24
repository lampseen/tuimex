use super::*;

const HOUR: i64 = 3600;

/// Seconds since the epoch for a UTC date and hour.
fn utc(year: i64, month: i64, day: i64, hour: i64) -> Timestamp {
    Timestamp(days_from_civil(year, month, day) * 86_400 + hour * HOUR)
}

fn event(lines: &str) -> String {
    format!("BEGIN:VCALENDAR\r\n{ZONES}BEGIN:VEVENT\r\nUID:u@example.com\r\n{lines}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")
}

/// Central Europe by its real rule, New York by its real rule, a zone with
/// one fixed offset, and one whose rule this parser does not read.
const ZONES: &str = "BEGIN:VTIMEZONE\r\nTZID:Europe/Berlin\r\n\
    BEGIN:DAYLIGHT\r\nDTSTART:19700329T020000\r\nTZOFFSETTO:+0200\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\n\
    BEGIN:STANDARD\r\nDTSTART:19701025T030000\r\nTZOFFSETTO:+0100\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\n\
    END:VTIMEZONE\r\n\
    BEGIN:VTIMEZONE\r\nTZID:America/New_York\r\n\
    BEGIN:DAYLIGHT\r\nDTSTART:20070311T020000\r\nTZOFFSETTO:-0400\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU\r\nEND:DAYLIGHT\r\n\
    BEGIN:STANDARD\r\nDTSTART:20071104T020000\r\nTZOFFSETTO:-0500\r\nRRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU\r\nEND:STANDARD\r\n\
    END:VTIMEZONE\r\n\
    BEGIN:VTIMEZONE\r\nTZID:Asia/Kolkata\r\nBEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETTO:+0530\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
    BEGIN:VTIMEZONE\r\nTZID:Odd/Rule\r\n\
    BEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETTO:+0100\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYMONTHDAY=25\r\nEND:STANDARD\r\n\
    END:VTIMEZONE\r\n";

#[test]
fn a_start_becomes_an_instant_only_when_the_invitation_says_which() {
    let local =
        |h: i64, zone: Option<&str>| EventTime::Local { wall: utc(2026, 7, 1, h), zone: zone.map(str::to_string) };
    for (dtstart, want) in [
        ("DTSTART:20260701T100000Z", EventTime::At(utc(2026, 7, 1, 10))),
        ("DTSTART;TZID=Europe/Berlin:20260701T100000", EventTime::At(utc(2026, 7, 1, 8))),
        ("DTSTART;TZID=Europe/Berlin:20261201T100000", EventTime::At(utc(2026, 12, 1, 9))),
        // The last Sundays of March and October 2026 are the 29th and 25th.
        ("DTSTART;TZID=Europe/Berlin:20260328T120000", EventTime::At(utc(2026, 3, 28, 11))),
        ("DTSTART;TZID=Europe/Berlin:20260329T120000", EventTime::At(utc(2026, 3, 29, 10))),
        ("DTSTART;TZID=Europe/Berlin:20261024T120000", EventTime::At(utc(2026, 10, 24, 10))),
        ("DTSTART;TZID=Europe/Berlin:20261025T120000", EventTime::At(utc(2026, 10, 25, 11))),
        ("DTSTART;TZID=Europe/Berlin:20260101T120000", EventTime::At(utc(2026, 1, 1, 11))),
        // The second Sunday of March 2026 is the 8th; the first of November the 1st.
        ("DTSTART;TZID=America/New_York:20260307T120000", EventTime::At(utc(2026, 3, 7, 17))),
        ("DTSTART;TZID=America/New_York:20260308T120000", EventTime::At(utc(2026, 3, 8, 16))),
        ("DTSTART;TZID=America/New_York:20261101T120000", EventTime::At(utc(2026, 11, 1, 17))),
        ("DTSTART;TZID=\"Asia/Kolkata\":20260701T053000", EventTime::At(utc(2026, 7, 1, 0))),
        ("DTSTART;TZID=Odd/Rule:20260701T100000", local(10, Some("Odd/Rule"))),
        ("DTSTART;TZID=Nowhere/Known:20260701T100000", local(10, Some("Nowhere/Known"))),
        ("DTSTART:20260701T100000", local(10, None)),
        ("DTSTART;VALUE=DATE:20260701", EventTime::Day(utc(2026, 7, 1, 0))),
    ] {
        assert_eq!(invite(&event(dtstart)).unwrap().start, want, "{dtstart}");
    }
}

#[test]
fn what_is_not_an_event_with_a_start_is_no_invitation() {
    for calendar in [
        String::new(),
        "not a calendar".to_string(),
        "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nDTSTART:20260701T100000Z\r\nEND:VTODO\r\nEND:VCALENDAR".to_string(),
        event("SUMMARY:No start"),
        event("DTSTART:yesterday"),
        event("DTSTART:20261301T100000Z"),
        event("DTSTART:20260701T250000Z"),
        event("DTSTART:20260701X100000Z"),
    ] {
        assert_eq!(invite(&calendar), None, "{calendar}");
    }
}

#[test]
fn only_the_first_event_and_only_its_own_properties_are_read() {
    let calendar = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20260701T100000Z\nSUMMARY:First\n\
        BEGIN:VALARM\nSUMMARY:Alarm\nEND:VALARM\nEND:VEVENT\n\
        BEGIN:VEVENT\nDTSTART:20260702T100000Z\nSUMMARY:Second\nLOCATION:Elsewhere\nEND:VEVENT\nEND:VCALENDAR\n";
    let invite = invite(calendar).unwrap();
    assert_eq!((invite.summary.as_str(), invite.location, invite.uid.as_str()), ("First", None, ""));
}

#[test]
fn text_is_unescaped_unfolded_and_cleaned() {
    let invite = invite(&event(
        "DTSTART:20260701T100000Z\r\nSUMMARY:Lunch\\, then\\nwork \\\\ \r\n play\u{1b}[2J\r\nLOCATION:\r\n\
         ORGANIZER;CN=\"Example; Ada: the first\":MAILTO:ada@example.com\r\nATTENDEE:grace@example.org",
    ))
    .unwrap();
    assert_eq!(invite.summary, "Lunch, then work \\ play[2J");
    assert_eq!(invite.location, None);
    let organizer = invite.organizer.unwrap();
    assert_eq!(
        (organizer.name.as_deref(), organizer.email.as_str()),
        (Some("Example; Ada: the first"), "ada@example.com")
    );
    assert_eq!(invite.attendees[0].email, "grace@example.org");
}

#[test]
fn a_rule_is_put_in_words_or_called_repeats() {
    for (rule, want) in [
        ("FREQ=DAILY", "daily"),
        ("FREQ=WEEKLY", "weekly"),
        ("FREQ=MONTHLY", "monthly"),
        ("FREQ=YEARLY", "yearly"),
        ("FREQ=WEEKLY;INTERVAL=2", "every 2 weeks"),
        ("FREQ=DAILY;INTERVAL=3", "every 3 days"),
        ("FREQ=WEEKLY;BYDAY=MO,WE,FR", "weekly on Mon, Wed, Fri"),
        ("FREQ=WEEKLY;WKST=MO;BYDAY=TU;COUNT=10", "weekly on Tue, 10 times"),
        ("FREQ=MONTHLY;UNTIL=20261231T235959Z", "monthly, until 2026-12-31"),
        ("FREQ=MONTHLY;BYDAY=1MO", "repeats"),
        ("FREQ=MONTHLY;BYMONTHDAY=15", "repeats"),
        ("FREQ=YEARLY;BYSETPOS=-1;BYDAY=FR", "repeats"),
        ("FREQ=HOURLY", "repeats"),
        ("FREQ=WEEKLY;INTERVAL=0", "repeats"),
        ("FREQ=WEEKLY;UNTIL=soon", "repeats"),
        ("INTERVAL=2", "repeats"),
        ("", "repeats"),
    ] {
        assert_eq!(recurrence(rule), want, "{rule}");
    }
}

fn only(lines: &str) -> Event {
    let mut events = calendar(&event(lines)).events;
    assert_eq!(events.len(), 1, "{lines}");
    events.remove(0)
}

fn wall_at(year: i64, month: i64, day: i64, hour: i64) -> i64 {
    utc(year, month, day, hour).0
}

#[test]
fn a_calendar_gives_every_event_that_has_a_start_and_says_what_it_asks() {
    let text = "BEGIN:VCALENDAR\nMETHOD:request\nBEGIN:VEVENT\nUID:a\nDTSTART:20260701T100000Z\nEND:VEVENT\n\
        BEGIN:VEVENT\nUID:no-start\nSUMMARY:Lost\nEND:VEVENT\n\
        BEGIN:VTODO\nUID:todo\nDTSTART:20260701T100000Z\nEND:VTODO\n\
        BEGIN:VEVENT\nUID:b\nDTSTART;VALUE=DATE:20260702\nEND:VEVENT\nEND:VCALENDAR\n";
    let parsed = calendar(text);
    assert_eq!(parsed.method, Some(Method::Request));
    assert_eq!(parsed.events.iter().map(|e| e.uid.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert_eq!(parsed.events[1].start, Moment::Day(days_from_civil(2026, 7, 2)));
    for (written, want) in
        [("REPLY", Method::Reply), ("Cancel", Method::Cancel), ("COUNTER", Method::Counter), ("PUBLISH", Method::Other)]
    {
        assert_eq!(calendar(&format!("BEGIN:VCALENDAR\nMETHOD:{written}\nEND:VCALENDAR")).method, Some(want));
    }
    assert_eq!(calendar("not a calendar"), Calendar::default());
    assert_eq!(calendar("BEGIN:VCALENDAR\nEND:VCALENDAR").method, None);
}

#[test]
fn a_time_keeps_the_zone_it_was_written_in() {
    let parsed =
        calendar(&event("DTSTART;TZID=Europe/Berlin:20260701T100000\r\nDTEND;TZID=Mars/Olympus:20260701T110000"));
    let e = &parsed.events[0];
    assert_eq!(e.start, Moment::Wall { wall: wall_at(2026, 7, 1, 10), zone: Some("Europe/Berlin".into()) });
    assert_eq!(e.end, Some(Moment::Wall { wall: wall_at(2026, 7, 1, 11), zone: Some("Mars/Olympus".into()) }));
    assert_eq!(
        parsed.zones.keys().collect::<Vec<_>>(),
        vec!["America/New_York", "Asia/Kolkata", "Europe/Berlin"],
        "a zone with a period that cannot be read is not defined"
    );
    assert_eq!(parsed.zones["Europe/Berlin"].offset_at_wall(wall_at(2026, 7, 1, 10)), Some(2 * HOUR));
    assert_eq!(only("DTSTART:20260701T100000").start, Moment::Wall { wall: wall_at(2026, 7, 1, 10), zone: None });
    assert_eq!(only("DTSTART:20260701T100000Z").start, Moment::Utc(wall_at(2026, 7, 1, 10)));
}

#[test]
fn an_end_may_be_written_as_a_length() {
    assert_eq!(
        only("DTSTART:20260701T100000Z\r\nDURATION:PT1H30M").end,
        Some(Moment::Utc(wall_at(2026, 7, 1, 10) + 5400))
    );
    assert_eq!(only("DTSTART;VALUE=DATE:20260701\r\nDURATION:P2D").end, Some(Moment::Day(days_from_civil(2026, 7, 3))));
    assert_eq!(only("DTSTART:20260701T100000Z\r\nDURATION:P1M").end, None, "a month has no fixed length");
    for (written, want) in [
        ("PT15M", Some(900)),
        ("-PT10M", Some(-600)),
        ("P1W", Some(7 * 86_400)),
        ("P1DT2H3M4S", Some(86_400 + 7200 + 180 + 4)),
        ("+PT0S", Some(0)),
        ("PT", None),
        ("P", None),
        ("10M", None),
        ("PT5", None),
        ("P1H", None),
        ("PT99999999999999999999S", None),
        ("PT9223372036854775807H", None),
    ] {
        assert_eq!(duration(written), want, "{written}");
    }
}

#[test]
fn guests_are_read_with_what_each_has_said() {
    let e = only(
        "DTSTART:20260701T100000Z\r\nORGANIZER;CN=Ada Example:mailto:ada@example.com\r\n\
         ATTENDEE;PARTSTAT=ACCEPTED;CN=Grace:mailto:grace@example.org\r\nATTENDEE;PARTSTAT=declined:mailto:lin@example.net\r\n\
         ATTENDEE;PARTSTAT=TENTATIVE:mailto:kim@example.org\r\nATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:sam@example.org\r\n\
         ATTENDEE:mailto:pat@example.org",
    );
    assert_eq!(e.organizer.unwrap().email, "ada@example.com");
    let said: Vec<(&str, Said)> = e.guests.iter().map(|g| (g.who.email.as_str(), g.said)).collect();
    assert_eq!(
        said,
        vec![
            ("grace@example.org", Said::Accepted),
            ("lin@example.net", Said::Declined),
            ("kim@example.org", Said::Tentative),
            ("sam@example.org", Said::Nothing),
            ("pat@example.org", Said::Nothing),
        ]
    );
}

#[test]
fn what_an_event_carries_beside_its_time_is_read() {
    let e = only(
        "DTSTART:20260701T100000Z\r\nSEQUENCE:3\r\nRECURRENCE-ID:20260701T090000Z\r\nSTATUS:CONFIRMED\r\nTRANSP:OPAQUE\r\nCLASS:PRIVATE\r\n\
         LAST-MODIFIED:20261123T140500Z\r\nDESCRIPTION:Agenda:\\n\\n 1. Budget\u{1b}[2J \\n2. Hiring\\n\r\n\
         URL:https://example.org/event\r\nCONFERENCE;VALUE=URI:https://meet.example.org/r/one\r\nX-GOOGLE-CONFERENCE:https://meet.example.org/r/one\r\n\
         BEGIN:VALARM\r\nTRIGGER:-PT10M\r\nEND:VALARM\r\nBEGIN:VALARM\r\nTRIGGER;VALUE=DATE-TIME:20260701T090000Z\r\nEND:VALARM\r\n\
         BEGIN:VALARM\r\nTRIGGER:PT5M\r\nEND:VALARM\r\nBEGIN:VALARM\r\nTRIGGER:PT0S\r\nEND:VALARM",
    );
    assert_eq!(e.sequence, 3);
    assert_eq!(e.instead_of, Some(Moment::Utc(wall_at(2026, 7, 1, 9))));
    assert_eq!(e.description.as_deref(), Some("Agenda:\n\n1. Budget[2J\n2. Hiring"));
    assert_eq!(
        e.links,
        vec!["https://meet.example.org/r/one", "https://example.org/event"],
        "the conference first, each once"
    );
    assert_eq!(
        e.alarms,
        vec![600, 0],
        "an alarm before the start or at it; one after it or at a fixed time is not a reminder"
    );
    assert!(!e.cancelled);
    let more: Vec<(&str, &str)> = e.more.iter().map(|(l, v)| (l.as_str(), v.as_str())).collect();
    assert_eq!(
        more,
        vec![("Status", "confirmed"), ("Shown as", "busy"), ("Privacy", "private"), ("Changed", "2026-11-23 14:05")]
    );
    assert!(only("DTSTART:20260701T100000Z\r\nSTATUS:cancelled").cancelled);
    let bare = only("DTSTART:20260701T100000Z");
    assert_eq!((bare.sequence, bare.description, bare.more.len(), bare.links.len()), (0, None, 0, 0));
}

#[test]
fn a_rule_tuimex_works_out_is_read_and_any_other_is_kept_as_written() {
    let read = |rule: &str| only(&format!("DTSTART:20261110T090000Z\r\nRRULE:{rule}")).repeat.unwrap();
    let plain =
        |freq| Rule { freq, interval: 1, count: None, until: None, by_day: Vec::new(), by_month_day: Vec::new() };
    assert_eq!(read("FREQ=DAILY"), Repeat::Rule(plain(Freq::Daily)));
    assert_eq!(
        read("FREQ=WEEKLY;WKST=SU;BYDAY=MO,TU,WE,TH,FR"),
        Repeat::Rule(Rule { by_day: (1..=5).map(|w| (0, w)).collect(), ..plain(Freq::Weekly) })
    );
    assert_eq!(
        read("freq=weekly;interval=2;count=10"),
        Repeat::Rule(Rule { interval: 2, count: Some(10), ..plain(Freq::Weekly) })
    );
    assert_eq!(read("FREQ=MONTHLY;BYDAY=2TU"), Repeat::Rule(Rule { by_day: vec![(2, 2)], ..plain(Freq::Monthly) }));
    assert_eq!(read("FREQ=MONTHLY;BYDAY=-1FR"), Repeat::Rule(Rule { by_day: vec![(-1, 5)], ..plain(Freq::Monthly) }));
    assert_eq!(
        read("FREQ=MONTHLY;BYMONTHDAY=15,-1"),
        Repeat::Rule(Rule { by_month_day: vec![15, -1], ..plain(Freq::Monthly) })
    );
    assert_eq!(
        read("FREQ=YEARLY;BYMONTH=11;BYMONTHDAY=10"),
        Repeat::Rule(Rule { by_month_day: vec![10], ..plain(Freq::Yearly) })
    );
    assert_eq!(
        read("FREQ=WEEKLY;UNTIL=20261231T235959Z"),
        Repeat::Rule(Rule { until: Some(wall_at(2026, 12, 31, 23) + 3599), ..plain(Freq::Weekly) })
    );
    assert_eq!(
        read("FREQ=WEEKLY;UNTIL=20261231"),
        Repeat::Rule(Rule { until: Some(wall_at(2027, 1, 1, 0) - 1), ..plain(Freq::Weekly) }),
        "a date alone means through that day"
    );
    for odd in [
        "FREQ=HOURLY",
        "FREQ=YEARLY;BYSETPOS=-1;BYDAY=FR",
        "FREQ=YEARLY;BYMONTH=3",
        "FREQ=MONTHLY;BYMONTH=3",
        "FREQ=WEEKLY;BYDAY=2TU",
        "FREQ=DAILY;BYMONTHDAY=1",
        "FREQ=WEEKLY;INTERVAL=2;WKST=SU",
        "FREQ=WEEKLY;INTERVAL=0",
        "FREQ=WEEKLY;COUNT=many",
        "FREQ=WEEKLY;UNTIL=soon",
        "FREQ=MONTHLY;BYDAY=9MO",
        "FREQ=MONTHLY;BYDAY=M",
        "FREQ=MONTHLY;BYMONTHDAY=40",
        "INTERVAL=2",
        "FREQ=WEEKLY;BYWEEKNO=2",
    ] {
        assert_eq!(read(odd), Repeat::Unread(odd.to_string()), "{odd}");
    }
    assert_eq!(only("DTSTART:20261110T090000Z").repeat, None);
}

#[test]
fn starts_a_rule_would_give_can_be_left_out() {
    let e = only(
        "DTSTART;TZID=Europe/Berlin:20261103T090000\r\nRRULE:FREQ=WEEKLY\r\n\
         EXDATE;TZID=Europe/Berlin:20261110T090000,20261117T090000\r\nEXDATE:20261124T080000Z,nonsense",
    );
    let berlin = |day: i64| Moment::Wall { wall: wall_at(2026, 11, day, 9), zone: Some("Europe/Berlin".into()) };
    assert_eq!(e.excluded, vec![berlin(10), berlin(17), Moment::Utc(wall_at(2026, 11, 24, 8))]);
}

#[test]
fn a_calendar_written_to_do_harm_is_only_text() {
    // Components that never close, one inside another, properties with no
    // name or no value: an event is only what sits directly in a calendar.
    let unbalanced = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nBEGIN:VEVENT\nDTSTART:20260701T100000Z\nSUMMARY\n:\n;\nEND:VCALENDAR\n\
        BEGIN:VEVENT\nDTSTART:20260701T100000Z\nEND:VEVENT\n";
    assert!(calendar(unbalanced).events.is_empty());
    let e = only(
        "DTSTART;TZID=../../etc/shadow:20260701T100000\r\nSUMMARY:\u{202e}gnp.exe\u{1b}]0;title\u{7}\r\n\
         LOCATION:\u{0}\r\nURL:javascript:alert(1)\r\nATTENDEE;CN=\"a",
    );
    assert_eq!(e.summary, "gnp.exe]0;title", "control and direction characters are gone");
    assert_eq!(e.location, None);
    assert_eq!(
        e.start,
        Moment::Wall { wall: wall_at(2026, 7, 1, 10), zone: Some("../../etc/shadow".into()) },
        "a zone's name is kept as text; nothing here uses it as a path"
    );
    assert_eq!(e.links, vec!["javascript:alert(1)"], "a link is text until it is assessed");
    // Depth and length: neither recursion nor a long line is a way in.
    let deep = format!("{}DTSTART:20260701T100000Z\n", "BEGIN:VEVENT\n".repeat(20_000));
    assert!(calendar(&deep).events.is_empty());
    let long = format!(
        "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20260701T100000Z\nSUMMARY:{}\nEND:VEVENT\nEND:VCALENDAR",
        "A".repeat(2_000_000)
    );
    assert_eq!(calendar(&long).events[0].summary.len(), 2_000_000);
}

#[test]
fn a_series_ended_at_an_instant_ends_on_the_clock_it_repeats_on() {
    // New York, 09:30 daily. The provider cut the series short before the
    // 26th by writing the second before that occurrence, in UTC: 14:29:59.
    // Read as a time on New York's clock it would be after 09:30, and the
    // 26th would come back.
    let cut = only("DTSTART;TZID=America/New_York:20261123T093000\r\nRRULE:FREQ=DAILY;UNTIL=20261126T142959Z");
    let Some(Repeat::Rule(rule)) = cut.repeat else { panic!("a daily rule is one tuimex works out") };
    assert_eq!(rule.until, Some(wall_at(2026, 11, 26, 9) + 29 * 60 + 59));
    // A zone the calendar does not define cannot be converted, and a start
    // with no zone or in UTC needs no conversion.
    for start in ["DTSTART;TZID=Mars/Olympus:20261123T093000", "DTSTART:20261123T093000", "DTSTART:20261123T093000Z"] {
        let Some(Repeat::Rule(rule)) = only(&format!("{start}\r\nRRULE:FREQ=DAILY;UNTIL=20261126T142959Z")).repeat
        else {
            panic!()
        };
        assert_eq!(rule.until, Some(wall_at(2026, 11, 26, 14) + 29 * 60 + 59), "{start}");
    }
}

#[test]
fn a_zone_s_name_is_unescaped_where_it_is_defined_as_where_it_is_used() {
    let text = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Office\\, second floor\r\n\
        BEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETTO:+0300\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
        BEGIN:VEVENT\r\nDTSTART;TZID=\"Office, second floor\":20260701T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let parsed = calendar(text);
    let Moment::Wall { zone: Some(zone), .. } = &parsed.events[0].start else { panic!("a time in a named zone") };
    assert_eq!(parsed.zones[zone].offset_at_wall(0), Some(3 * HOUR));
}

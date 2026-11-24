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

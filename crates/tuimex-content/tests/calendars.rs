//! Calendars as providers write them, under `tests/calendars/`: synthetic
//! files in the shape of a Proton feed and of an event Google serves over
//! CalDAV (ADR-056, ADR-062). The shapes are from those formats as
//! documented and seen; nothing here was fetched from either provider.

// A test that cannot read its inputs should fail loudly, which is what
// unwrap does; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use tuimex_content::calendar;
use tuimex_core::calendar::{DAY, Freq, Method, Moment, Repeat, Said, Zone, days_from_civil, occurrences_between};

const FILES: [&str; 2] = ["google-repeating.ics", "proton-feed.ics"];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/calendars")
}

fn read(file: &str) -> String {
    fs::read_to_string(dir().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn at(year: i64, month: i64, day: i64, hour: i64, minute: i64) -> i64 {
    days_from_civil(year, month, day) * DAY + hour * 3_600 + minute * 60
}

#[test]
fn every_calendar_file_has_a_test() {
    let mut found: Vec<String> =
        fs::read_dir(dir()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    found.sort();
    assert_eq!(found, FILES);
}

#[test]
fn every_address_in_the_calendars_is_under_an_example_domain() {
    for file in FILES {
        for (i, _) in read(file).match_indices('@') {
            let text = read(file);
            let host: String =
                text[i + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')).collect();
            let allowed = ["example.com", "example.org", "example.net"];
            assert!(allowed.iter().any(|d| host == *d || host.ends_with(&format!(".{d}"))), "{file}: {host}");
        }
    }
}

#[test]
fn a_calendar_file_cut_off_at_any_byte_is_read_without_a_panic() {
    for file in FILES {
        let bytes = fs::read(dir().join(file)).unwrap();
        for end in 0..bytes.len() {
            calendar(&String::from_utf8_lossy(&bytes[..end]));
        }
    }
}

#[test]
fn proton_feed() {
    let parsed = calendar(&read("proton-feed.ics"));
    assert_eq!(parsed.method, Some(Method::Other), "a published calendar asks for no answer");
    assert_eq!(parsed.zones.keys().collect::<Vec<_>>(), vec!["Europe/Oslo"]);
    let names: Vec<&str> = parsed.events.iter().map(|e| e.summary.as_str()).collect();
    assert_eq!(names, vec!["Dentist", "Climbing", "Cabin weekend"]);

    let dentist = &parsed.events[0];
    assert_eq!(dentist.start, Moment::Wall { wall: at(2026, 11, 24, 11, 0), zone: Some("Europe/Oslo".into()) });
    assert_eq!(dentist.location.as_deref(), Some("Kirkegata 12"));
    assert!(dentist.more.iter().all(|(label, _)| !label.contains("PM")), "Proton's own keys are not shown");

    let climbing = &parsed.events[1];
    assert!(
        matches!(&climbing.repeat, Some(Repeat::Rule(rule)) if rule.freq == Freq::Weekly && rule.by_day == vec![(0, 2)])
    );
    assert_eq!(
        climbing.excluded,
        vec![Moment::Wall { wall: at(2026, 10, 27, 18, 0), zone: Some("Europe/Oslo".into()) }]
    );
    assert_eq!(climbing.alarms, vec![1_800]);

    let cabin = &parsed.events[2];
    assert_eq!(
        (&cabin.start, &cabin.end),
        (&Moment::Day(days_from_civil(2026, 11, 27)), &Some(Moment::Day(days_from_civil(2026, 11, 30))))
    );
    assert_eq!(cabin.description.as_deref(), Some("Two cars.\nGreta drives the first, Robin the second."));
    assert_eq!(cabin.guests.iter().map(|g| g.said).collect::<Vec<_>>(), vec![Said::Accepted, Said::Nothing]);

    // Seen from New York across both clock changes: Oslo's 18:00 is
    // noon before Europe's change and after America's. The 27th, the one
    // Tuesday between the two, is the day the calendar leaves out.
    let new_york = calendar(&read("google-repeating.ics")).zones.remove("America/New_York").unwrap();
    let seen = occurrences_between(
        &parsed.events,
        &parsed.zones,
        &new_york,
        days_from_civil(2026, 10, 19),
        days_from_civil(2026, 11, 11),
    );
    let climbs: Vec<i64> = seen.iter().filter(|o| o.event.summary == "Climbing").map(|o| o.start).collect();
    assert_eq!(climbs, vec![at(2026, 10, 20, 12, 0), at(2026, 11, 3, 12, 0), at(2026, 11, 10, 12, 0)]);
}

#[test]
fn google_repeating() {
    let parsed = calendar(&read("google-repeating.ics"));
    assert_eq!(parsed.method, None);
    assert_eq!(parsed.events.len(), 2, "the series and the one day that was moved");
    let (series, moved) = (&parsed.events[0], &parsed.events[1]);
    assert_eq!(series.uid, moved.uid);
    assert_eq!(
        moved.instead_of,
        Some(Moment::Wall { wall: at(2026, 11, 26, 9, 30), zone: Some("America/New_York".into()) })
    );
    let Some(Repeat::Rule(rule)) = &series.repeat else { panic!("the weekday rule is one tuimex works out") };
    // The end is written as an instant, 23:59:59 UTC, which on New York's
    // clock, the one the series repeats on, is 18:59:59.
    assert_eq!((rule.freq, rule.by_day.len(), rule.until), (Freq::Weekly, 5, Some(at(2026, 12, 31, 18, 59) + 59)));

    // Folded lines are one line again, parameters and all.
    let guests: Vec<(&str, Option<&str>, Said)> =
        series.guests.iter().map(|g| (g.who.email.as_str(), g.who.name.as_deref(), g.said)).collect();
    assert_eq!(
        guests,
        vec![
            ("robin.vale@example.org", Some("Robin Vale"), Said::Accepted),
            ("mei@example.org", Some("Mei Tanaka"), Said::Nothing)
        ]
    );
    assert_eq!(series.links, vec!["https://meet.example.org/r/standup"]);
    assert_eq!(
        series.description.as_deref(),
        Some("Join: https://meet.example.org/r/standup\n\nOr dial +47 21 00 00 00, code 4471")
    );
    assert_eq!(series.location, None, "an empty location is none");
    assert_eq!(series.alarms, vec![600]);

    // The week of the moved day, seen in the zone it was written in: the
    // 26th is at 11:00 and nowhere at 09:30.
    let zone: &Zone = &parsed.zones["America/New_York"];
    let week = occurrences_between(
        &parsed.events,
        &parsed.zones,
        zone,
        days_from_civil(2026, 11, 23),
        days_from_civil(2026, 11, 28),
    );
    let starts: Vec<i64> = week.iter().map(|o| o.start).collect();
    assert_eq!(
        starts,
        vec![
            at(2026, 11, 23, 9, 30),
            at(2026, 11, 24, 9, 30),
            at(2026, 11, 25, 9, 30),
            at(2026, 11, 26, 11, 0),
            at(2026, 11, 27, 9, 30)
        ]
    );
    // Seen from UTC, after America's clocks have gone back: 14:30.
    let utc = occurrences_between(
        &parsed.events,
        &BTreeMap::from([("America/New_York".to_string(), zone.clone())]),
        &Zone::fixed(0),
        days_from_civil(2026, 11, 23),
        days_from_civil(2026, 11, 24),
    );
    assert_eq!(utc[0].start, at(2026, 11, 23, 14, 30));
}

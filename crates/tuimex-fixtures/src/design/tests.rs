use std::collections::HashSet;

use tuimex_core::calendar::{Zone, days_from_civil, links_of, occurrences_between};
use tuimex_core::{Url, thread};

use super::{
    DEC_5, FILTER, FRI, HALDEN, MARKED, NOW, THU, UNFOLDED, arrived, calendar, calendars, filed, inbox, outbox,
};

fn reserved(host: &str) -> bool {
    ["example.com", "example.org", "example.net"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// CLAUDE.md: fixtures are synthetic. An event names people and hosts as a
/// message does.
#[test]
fn every_address_and_host_of_the_calendar_is_under_a_reserved_example_domain() {
    let mut seen = 0;
    for event in calendars().into_iter().flat_map(|kept| kept.events) {
        let people = event.organizer.iter().chain(event.guests.iter().map(|guest| &guest.who));
        let mut hosts: Vec<String> = people.filter_map(|a| a.email.split('@').nth(1).map(str::to_string)).collect();
        hosts.extend(event.uid.split('@').nth(1).map(str::to_string));
        hosts.extend(links_of(&event).iter().map(|link| Url::parse(&link.url).host));
        hosts.extend(event.location.iter().filter(|place| place.contains('.')).cloned());
        for host in &hosts {
            assert!(reserved(host), "{}: {host} is not a reserved domain", event.summary);
        }
        seen += hosts.len();
    }
    assert!(seen > 40, "read only {seen} hosts from the calendar");
}

/// The counts the design's Calendars panel shows, and the days its
/// screens name.
#[test]
fn the_calendars_hold_the_design_s_events_on_the_design_s_days() {
    let held: Vec<(&str, bool, usize)> = calendars().iter().map(|k| (k.account, k.local, k.events.len())).collect();
    assert_eq!(held, [("personal", false, 5), ("personal", true, 2), ("work", false, 7), ("work", true, 1)]);
    assert_eq!(calendar::day(1, 1), days_from_civil(2026, 1, 1));
    let named = (days_from_civil(2026, 11, 26), days_from_civil(2026, 11, 27), days_from_civil(2026, 12, 5));
    assert_eq!((THU, FRI, DEC_5), named);
    assert_eq!(NOW.0.div_euclid(86_400), days_from_civil(2026, 11, 23));
    let on = |day: i64| -> Vec<String> {
        let all = calendars();
        let (zones, utc) = (Default::default(), Zone::fixed(0));
        let found = all.iter().flat_map(|kept| occurrences_between(&kept.events, &zones, &utc, day, day + 1));
        found.map(|o| o.event.summary.clone()).collect()
    };
    assert_eq!(on(THU), ["Standup", "Q4 roadmap review", "Vendor call", "Staging migration window"]);
    assert_eq!(on(days_from_civil(2026, 11, 28)), ["Cabin weekend"], "no standup on a Saturday");
    assert_eq!(on(days_from_civil(2027, 1, 4)), [] as [&str; 0], "the standup ends with the year");
}

/// CLAUDE.md: fixtures are synthetic.
#[test]
fn every_address_is_under_a_reserved_example_domain() {
    for mail in inbox().into_iter().chain(outbox()).chain(filed()) {
        let e = &mail.envelope;
        let mut hosts: Vec<&str> =
            e.from.iter().chain(&e.to).chain(&e.cc).filter_map(|a| a.email.split('@').nth(1)).collect();
        hosts.extend(mail.sender_was.iter().chain(&e.message_id).filter_map(|a| a.split('@').nth(1)));
        assert!(!hosts.is_empty());
        for host in hosts {
            let host = host.trim_end_matches('>');
            assert!(reserved(host), "{}: {host} is not a reserved domain", e.subject);
        }
    }
}

#[test]
fn a_key_names_one_message_and_every_screen_s_state_names_a_message() {
    let all = inbox();
    let keys: HashSet<&str> = all.iter().map(|m| m.envelope.id.key.as_str()).collect();
    assert_eq!(keys.len(), all.len());
    for key in MARKED.iter().chain(&FILTER.1).chain(&[HALDEN, UNFOLDED.0, UNFOLDED.1]) {
        assert!(keys.contains(key), "{key} is not in the mailbox");
    }
}

/// The brief's mailbox: one thread five deep with a branch, one of three,
/// the rest single.
#[test]
fn the_threads_are_the_ones_the_design_draws() {
    let envelopes: Vec<_> = inbox().into_iter().map(|m| m.envelope).collect();
    let mut sizes: Vec<usize> = thread(&envelopes).iter().map(|t| t.members.len()).collect();
    sizes.sort_unstable();
    assert_eq!(sizes.len(), 24);
    assert_eq!(sizes[22..], [3, 5]);
    assert!(sizes[..22].iter().all(|n| *n == 1));
    let cabin = thread(&envelopes).into_iter().find(|t| t.root.key == "cabin-1").unwrap();
    let shape: Vec<(&str, u16)> = cabin.members.iter().map(|n| (n.id.key.as_str(), n.depth)).collect();
    assert_eq!(shape, [("cabin-1", 0), ("cabin-2", 1), ("cabin-3", 2), ("cabin-5", 3), ("cabin-4", 1)]);
}

#[test]
fn nothing_is_dated_after_now_and_the_first_sync_holds_less() {
    assert!(inbox().iter().chain(&outbox()).all(|m| m.envelope.date <= NOW));
    assert_eq!((inbox().len(), arrived().len()), (30, 23));
}

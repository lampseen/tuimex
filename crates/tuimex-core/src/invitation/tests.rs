use super::*;
use crate::calendar::{Guest, days_from_civil};

const HOUR: i64 = 3_600;

fn person(email: &str) -> Address {
    Address { name: None, email: email.to_string() }
}

fn oslo(day: i64, hour: i64, minute: i64) -> Moment {
    let wall = days_from_civil(2026, 11, day) * DAY + hour * HOUR + minute * 60;
    Moment::Wall { wall, zone: Some("Europe/Oslo".to_string()) }
}

/// Planning, organized by Ada, with Grace and Robin invited.
fn event() -> Event {
    let guest = |email: &str| Guest { who: person(email), said: Said::Nothing };
    Event {
        uid: "planning@example.com".into(),
        sequence: 2,
        instead_of: None,
        summary: "Planning".into(),
        start: oslo(26, 10, 0),
        end: Some(oslo(26, 11, 30)),
        location: None,
        description: None,
        organizer: Some(Address { name: Some("Ada".into()), email: "ada@example.com".into() }),
        guests: vec![guest("grace@example.org"), guest("robin@example.net")],
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

fn ada() -> Address {
    Address { name: Some("Ada".into()), email: "ada@example.com".into() }
}

const WHEN: &str = "Thu 26 Nov 2026, 10:00 to 11:30 (Europe/Oslo)";

/// Each choice, for an account invited under its first identity, under
/// its second, and not at all.
#[test]
fn each_answer_is_one_mail_to_the_organizer_from_the_invited_identity() {
    let propose = Answer::Propose { start: oslo(26, 14, 0), end: Some(oslo(26, 15, 30)) };
    let accounts: [(&[&str], &str, bool); 3] = [
        (&["grace@example.org", "grace.home@example.org"], "grace@example.org", false),
        (&["work@example.org", "Robin@Example.NET"], "Robin@Example.NET", false),
        (&["nobody@example.org", "other@example.org"], "nobody@example.org", true),
    ];
    for (identities, me, uninvited) in accounts {
        let identities: Vec<Address> = identities.iter().map(|e| person(e)).collect();
        for (choice, subject, body, carries, record) in [
            (
                Answer::Accept,
                "Accepted: Planning",
                format!("I will be at Planning, {WHEN}."),
                Carries::Reply(Said::Accepted),
                Record::Said(Said::Accepted),
            ),
            (
                Answer::Tentative,
                "Tentative: Planning",
                format!("I may be at Planning, {WHEN}."),
                Carries::Reply(Said::Tentative),
                Record::Said(Said::Tentative),
            ),
            (
                Answer::Decline,
                "Declined: Planning",
                format!("I will not be at Planning, {WHEN}."),
                Carries::Reply(Said::Declined),
                Record::Said(Said::Declined),
            ),
            (
                propose.clone(),
                "Proposed new time: Planning",
                "Could we move this to Thu 26 Nov 2026, 14:00 to 15:30 (Europe/Oslo)? \
                 It is currently Thu 26 Nov 2026, 10:00 to 11:30 (Europe/Oslo)."
                    .to_string(),
                Carries::Counter { start: oslo(26, 14, 0), end: Some(oslo(26, 15, 30)) },
                Record::Proposed { start: oslo(26, 14, 0), end: Some(oslo(26, 15, 30)) },
            ),
        ] {
            let got = answer(&event(), &identities, &choice).unwrap();
            let want = Outgoing { from: person(me), to: vec![ada()], subject: subject.into(), body, carries };
            assert_eq!(got, Answering { mail: Some(want), record, me: person(me), uninvited }, "{me} {choice:?}");
        }
    }
}

#[test]
fn ignoring_sends_nothing() {
    for identities in [vec![person("grace@example.org")], vec![person("nobody@example.org")]] {
        let got = answer(&event(), &identities, &Answer::Ignore).unwrap();
        assert_eq!((got.mail, got.record), (None, Record::Ignored));
    }
    // Ignoring needs nobody to answer.
    let alone = Event { organizer: None, ..event() };
    assert_eq!(answer(&alone, &[person("grace@example.org")], &Answer::Ignore).unwrap().mail, None);
}

#[test]
fn the_reply_goes_to_the_organizer_alone_however_many_are_invited() {
    let mut crowded = event();
    crowded.guests =
        (0..40).map(|n| Guest { who: person(&format!("guest{n}@example.net")), said: Said::Accepted }).collect();
    crowded.guests.push(Guest { who: person("grace@example.org"), said: Said::Nothing });
    for choice in [Answer::Accept, Answer::Decline, Answer::Propose { start: oslo(27, 9, 0), end: None }] {
        let mail = answer(&crowded, &[person("grace@example.org")], &choice).unwrap().mail.unwrap();
        assert_eq!(mail.to, [ada()], "{choice:?}");
    }
}

#[test]
fn what_cannot_be_answered_says_why() {
    let me = [person("grace@example.org")];
    assert_eq!(answer(&event(), &[], &Answer::Accept), Err(Unanswerable::NoIdentity));
    assert_eq!(answer(&Event { organizer: None, ..event() }, &me, &Answer::Accept), Err(Unanswerable::NoOrganizer));
    let blank = Event { organizer: Some(person(" ")), ..event() };
    assert_eq!(answer(&blank, &me, &Answer::Accept), Err(Unanswerable::NoOrganizer));
    let own = [person("ADA@example.com")];
    assert_eq!(answer(&event(), &own, &Answer::Accept), Err(Unanswerable::Organized));
}

/// A summary is the sender's text; in a subject, a line break would start
/// a header of their choosing.
#[test]
fn a_summary_becomes_one_line_of_a_subject() {
    let hostile = Event { summary: "Lunch\r\nBcc: eve@example.net\u{1b}[2J\tnow".into(), ..event() };
    let mail = answer(&hostile, &[person("grace@example.org")], &Answer::Accept).unwrap().mail.unwrap();
    assert_eq!(mail.subject, "Accepted: Lunch Bcc: eve@example.net [2J now");
    assert!(!mail.subject.contains(char::is_control) && !mail.body.contains(char::is_control));
    let untitled = Event { summary: " \r\n ".into(), ..event() };
    let mail = answer(&untitled, &[person("grace@example.org")], &Answer::Decline).unwrap().mail.unwrap();
    assert_eq!(mail.subject, "Declined: (no title)");
}

#[test]
fn a_time_is_said_on_the_clock_it_was_written_on() {
    let day = |d: i64| Moment::Day(days_from_civil(2026, 11, d));
    let utc = |d: i64, h: i64| Moment::Utc(days_from_civil(2026, 11, d) * DAY + h * HOUR);
    let floating = Moment::Wall { wall: days_from_civil(2026, 11, 26) * DAY + 9 * HOUR, zone: None };
    for (start, end, said) in [
        (day(26), None, "Thu 26 Nov 2026"),
        (day(26), Some(day(27)), "Thu 26 Nov 2026"),
        (day(26), Some(day(29)), "Thu 26 Nov 2026 to Sat 28 Nov 2026"),
        (utc(26, 9), Some(utc(26, 10)), "Thu 26 Nov 2026, 09:00 to 10:00 (UTC)"),
        (utc(30, 22), Some(utc(31, 1)), "Mon 30 Nov 2026, 22:00 to Tue 1 Dec 2026, 01:00 (UTC)"),
        (floating.clone(), None, "Thu 26 Nov 2026, 09:00"),
        (oslo(26, 10, 0), Some(utc(26, 12)), "Thu 26 Nov 2026, 10:00 (Europe/Oslo) to 12:00 (UTC)"),
        (oslo(26, 10, 0), Some(day(27)), "Thu 26 Nov 2026, 10:00 (Europe/Oslo) to Fri 27 Nov 2026"),
    ] {
        assert_eq!(when(&start, end.as_ref()), said, "{start:?} {end:?}");
    }
}

#[test]
fn an_event_made_here_is_sent_to_every_guest_but_its_organizer() {
    let mine = [person("other@example.org"), person("ADA@example.com")];
    let mut made = event();
    made.guests.push(Guest { who: person("Grace@Example.org"), said: Said::Nothing });
    made.guests.push(Guest { who: person("ada@example.com"), said: Said::Accepted });
    let invited = invite(&made, &mine).unwrap();
    assert_eq!(invited.from, person("ADA@example.com"));
    assert_eq!(invited.to, [person("grace@example.org"), person("robin@example.net")]);
    assert_eq!(invited.subject, "Invitation: Planning");
    assert_eq!(invited.body, format!("You are invited to Planning, {WHEN}."));
    assert_eq!(invited.carries, Carries::Request);
    let cancelled = cancel(&made, &mine).unwrap();
    assert_eq!((cancelled.to, cancelled.carries), (invited.to, Carries::Cancel));
    assert_eq!(cancelled.subject, "Cancelled: Planning");
    assert_eq!(cancelled.body, format!("Planning, {WHEN}, is cancelled."));

    assert_eq!(invite(&made, &[person("grace@example.org")]), Err(Unanswerable::NotOrganized));
    let alone = Event { guests: vec![Guest { who: ada(), said: Said::Accepted }], ..event() };
    assert_eq!(cancel(&alone, &mine), Err(Unanswerable::NoGuests));
}

/// What a later message is, who it is to, and what it may change.
type Row<'a> = (&'a str, &'a [Address], Method, Event, &'a Address, Later);

/// ADR-057: a later message is believed only from whoever the held event
/// says may send it.
#[test]
fn what_a_later_message_may_change() {
    let grace = person("grace@example.org");
    let robin = person("robin@example.net");
    let mallory = person("mallory@example.net");
    let shouting = person("ADA@EXAMPLE.COM");
    let organizer = ada();
    let as_guest = [grace.clone()];
    let as_organizer = [ada()];
    let newer = Event { sequence: 3, ..event() };
    let answering = |guests: &[(&Address, Said)]| Event {
        guests: guests.iter().map(|(who, said)| Guest { who: (*who).clone(), said: *said }).collect(),
        ..event()
    };
    let moved = Event { start: oslo(27, 9, 0), end: Some(oslo(27, 10, 0)), ..answering(&[(&robin, Said::Tentative)]) };
    let other_uid = Event { uid: "other@example.com".into(), ..newer.clone() };
    let one_occurrence = Event { instead_of: Some(oslo(26, 10, 0)), ..newer.clone() };
    let claims_organizer = Event { organizer: Some(mallory.clone()), ..newer.clone() };
    let nothing = Later::Nothing;
    use Method::{Cancel, Counter, Other, Reply, Request};
    use Unchanged::*;

    let rows: Vec<Row> = vec![
        ("a newer version from the organizer", &as_guest, Request, newer.clone(), &organizer, Later::Update),
        ("the organizer's address in capitals", &as_guest, Request, newer.clone(), &shouting, Later::Update),
        ("the same version again", &as_guest, Request, event(), &organizer, nothing(NotNewer)),
        ("an older version", &as_guest, Request, Event { sequence: 1, ..event() }, &organizer, nothing(NotNewer)),
        ("a cancellation from the organizer", &as_guest, Cancel, event(), &organizer, Later::Strike),
        ("a newer cancellation", &as_guest, Cancel, newer.clone(), &shouting, Later::Strike),
        ("an older cancellation", &as_guest, Cancel, Event { sequence: 1, ..event() }, &organizer, nothing(NotNewer)),
        ("a cancellation from a stranger", &as_guest, Cancel, newer.clone(), &mallory, nothing(NotFromOrganizer)),
        ("a newer version from a guest", &as_guest, Request, newer.clone(), &robin, nothing(NotFromOrganizer)),
        (
            "a version naming another organizer",
            &as_guest,
            Request,
            claims_organizer,
            &organizer,
            nothing(NotFromOrganizer),
        ),
        ("another UID", &as_guest, Cancel, other_uid, &organizer, nothing(OtherEvent)),
        ("one occurrence of the event", &as_guest, Cancel, one_occurrence, &organizer, nothing(OtherEvent)),
        (
            "a guest's answer",
            &as_organizer,
            Reply,
            answering(&[(&grace, Said::Accepted)]),
            &grace,
            Later::Answered { guest: grace.clone(), said: Said::Accepted },
        ),
        (
            "a guest's answer that also answers for another",
            &as_organizer,
            Reply,
            answering(&[(&robin, Said::Accepted), (&grace, Said::Declined)]),
            &grace,
            Later::Answered { guest: grace.clone(), said: Said::Declined },
        ),
        (
            "an answer claiming to be another guest's",
            &as_organizer,
            Reply,
            answering(&[(&robin, Said::Declined)]),
            &grace,
            nothing(NotFromGuest),
        ),
        (
            "an answer from someone not invited",
            &as_organizer,
            Reply,
            answering(&[(&mallory, Said::Accepted)]),
            &mallory,
            nothing(NotFromGuest),
        ),
        (
            "an answer to an older version",
            &as_organizer,
            Reply,
            Event { sequence: 1, ..answering(&[(&grace, Said::Accepted)]) },
            &grace,
            nothing(NotNewer),
        ),
        (
            "an answer to an event someone else organizes",
            &as_guest,
            Reply,
            answering(&[(&robin, Said::Accepted)]),
            &robin,
            nothing(NotOrganizedHere),
        ),
        (
            "a guest's proposal",
            &as_organizer,
            Counter,
            moved.clone(),
            &robin,
            Later::Proposed { guest: robin.clone(), start: oslo(27, 9, 0), end: Some(oslo(27, 10, 0)) },
        ),
        ("a stranger's proposal", &as_organizer, Counter, moved, &mallory, nothing(NotFromGuest)),
        ("an event published", &as_guest, Other, newer.clone(), &organizer, nothing(Unasked)),
    ];
    for (row, mine, method, incoming, sender, want) in rows {
        assert_eq!(later(&event(), mine, method, &incoming, sender), want, "{row}");
    }

    let unnamed = Event { uid: String::new(), ..event() };
    let both_unnamed = later(&unnamed, &as_guest, Cancel, &unnamed, &organizer);
    assert_eq!(both_unnamed, nothing(OtherEvent), "two events with no UID are not one event");

    let unorganized = Event { organizer: None, ..event() };
    let anyone = later(&unorganized, &as_guest, Request, &newer, &mallory);
    assert_eq!(anyone, nothing(NotFromOrganizer), "an event with no organizer is changed by nobody");
}

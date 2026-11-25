use super::*;
use crate::calendar::{Guest, Moment};
use crate::{Address, Warning, assess};

fn event() -> Event {
    Event {
        uid: "e".into(),
        sequence: 0,
        instead_of: None,
        summary: "e".into(),
        start: Moment::Day(0),
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

fn guest(email: &str, said: Said) -> Guest {
    Guest { who: Address { name: None, email: email.to_string() }, said }
}

/// ADR-057: "you" is an identity of the account the event came to.
#[test]
fn the_answer_is_the_one_the_receiving_account_s_guest_gave() {
    let work = ["robin.vale@example.org", "desk@example.org"];
    let personal = ["robin@example.com"];
    let table = [
        (Said::Nothing, Standing::Needs),
        (Said::Accepted, Standing::Accepted),
        (Said::Tentative, Standing::Tentative),
        (Said::Declined, Standing::Declined),
    ];
    for (said, want) in table {
        let asked = Event {
            guests: vec![guest("lars@example.org", Said::Accepted), guest("Robin.Vale@example.org", said)],
            ..event()
        };
        assert_eq!(standing(&asked, &work), Some(want), "{said:?}");
        assert_eq!(standing(&asked, &personal), None, "another account's identity is not the guest");
    }
    let second = Event { guests: vec![guest("desk@example.org", Said::Tentative)], ..event() };
    assert_eq!(standing(&second, &work), Some(Standing::Tentative), "any identity of that account");
    assert_eq!(standing(&event(), &work), None, "an event with no guests asked nobody");
    assert_eq!(standing(&second, &[]), None);
}

/// The event's own links, its location, its notes, and the addresses that
/// must come out, in order.
type Case =
    (&'static str, &'static [&'static str], Option<&'static str>, Option<&'static str>, &'static [&'static str]);

/// ADR-054: the conference property, then the URL, then an address in the
/// location, then those in the notes.
#[test]
fn the_meeting_s_link_is_first_and_each_address_is_offered_once() {
    const MEET: &str = "https://meet.example.org/r/q4";
    const PAGE: &str = "https://cal.example.org/e/1";
    let table: [Case; 7] = [
        ("nothing to offer", &[], Some("Room Fjord"), Some("Bring the pre-read."), &[]),
        ("the conference before the URL", &[MEET, PAGE], None, None, &[MEET, PAGE]),
        ("the location after the properties", &[PAGE], Some(MEET), None, &[PAGE, MEET]),
        (
            "the notes last, less their punctuation",
            &[],
            Some("Room Fjord, or https://meet.example.org/r/q4."),
            Some("Agenda: <https://cal.example.org/e/1>, questions to mailto:imani@example.org"),
            &[MEET, PAGE, "mailto:imani@example.org"],
        ),
        (
            "an address said twice is one link",
            &[MEET],
            Some(MEET),
            Some("Join at https://meet.example.org/r/q4"),
            &[MEET],
        ),
        ("a time and a label are not addresses", &[], Some("Note: 10:30 sharp"), Some("see page 2"), &[]),
        (
            "a scheme nothing opens is still listed",
            &[],
            None,
            Some("call sip://room.example.org"),
            &["sip://room.example.org"],
        ),
    ];
    for (name, own, location, notes, want) in table {
        let event = Event {
            links: own.iter().map(|url| url.to_string()).collect(),
            location: location.map(str::to_string),
            description: notes.map(str::to_string),
            ..event()
        };
        let links = links_of(&event);
        assert_eq!(links.iter().map(|link| link.url.as_str()).collect::<Vec<_>>(), want, "{name}");
        assert_eq!(links.iter().map(|link| link.n).collect::<Vec<_>>(), (1..=want.len()).collect::<Vec<_>>(), "{name}");
    }
}

/// ADR-018: every link is assessed against what the event says of it.
#[test]
fn a_meeting_held_somewhere_other_than_the_event_says_is_confirmed_first() {
    let lying = Event {
        links: vec!["https://bank.example.com.device-check.example.net/meet".to_string()],
        location: Some("bank.example.com".to_string()),
        ..event()
    };
    let link = &links_of(&lying)[0];
    let found = assess(&link.url, &link.shown);
    assert_eq!(found.host, "bank.example.com.device-check.example.net");
    assert_eq!(found.warnings, [Warning::TextNamesAnotherHost]);

    let honest = Event {
        links: vec!["https://meet.example.org/r/q4".to_string()],
        location: Some("Room Fjord, or meet.example.org".to_string()),
        description: Some("Dial in: https://198.51.100.7/room and javascript://x".to_string()),
        ..event()
    };
    let links = links_of(&honest);
    let found: Vec<_> = links.iter().map(|link| assess(&link.url, &link.shown)).collect();
    assert!(
        found[0].warnings.is_empty() && !found[0].blocked,
        "a room's name is no host, and the host named is its own"
    );
    assert_eq!(found[1].warnings, [Warning::BareIpAddress]);
    assert!(found[2].blocked, "neither the web nor mail");
}

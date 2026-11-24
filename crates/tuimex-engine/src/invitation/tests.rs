//! An answer and an event's mail, handed off through a running engine:
//! held in the right Outbox, stopped by `u`, refused from an address that
//! is not the account's.

use std::collections::BTreeMap;
use std::time::Duration;

use tuimex_core::calendar::{Event, Guest, Moment, Said};
use tuimex_core::invitation::{self, Record};
use tuimex_core::{Action, Address, Answer, Ask, Draft, Event as Told, MessageId};

use super::{About, Answered};
use crate::Account;
use crate::tests::{PASSWORD, QUIET, Running, the_account, world};

fn person(email: &str) -> Address {
    Address { name: None, email: email.to_string() }
}

/// Organized by someone else, with the account's own address and forty
/// others invited.
fn invitation() -> Event {
    let mut guests = vec![Guest { who: person("ada@example.org"), said: Said::Nothing }];
    guests.extend((0..40).map(|n| Guest { who: person(&format!("g{n}@example.net")), said: Said::Nothing }));
    Event {
        uid: "planning@example.com".into(),
        sequence: 0,
        instead_of: None,
        summary: "Planning".into(),
        start: Moment::Utc(1_796_115_600),
        end: Some(Moment::Utc(1_796_119_200)),
        location: None,
        description: None,
        organizer: Some(person("organizer@example.com")),
        guests,
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

/// Bytes enough for the engine to file and send; the real builder is the
/// binary's, and its own tests show what it makes of a draft.
fn build(draft: &Draft) -> Result<Vec<u8>, String> {
    let to: Vec<&str> = draft.to.iter().map(|a| a.email.as_str()).collect();
    let calendar = draft.calendar.as_ref().map_or("", |c| c.text.as_str());
    let head = format!("From: {}\r\nTo: {}\r\nSubject: {}", draft.from.email, to.join(", "), draft.subject);
    Ok(format!("{head}\r\nContent-Type: text/calendar\r\n\r\n{calendar}").into_bytes())
}

fn running(name: &str, delay: u64) -> Running {
    let account = Account { send_delay: Duration::from_secs(delay), ..the_account() };
    let r = Running::over(world(name, QUIET), account, PASSWORD);
    r.settle();
    r
}

impl Running {
    fn answered_with(&self, event: &Event, identities: &[Address], choice: Answer) -> Answered {
        let answering = invitation::answer(event, identities, &choice).unwrap();
        let zones = BTreeMap::new();
        let about = About { event, zones: &zones, answers: None, stamp: 0 };
        self.serial.set(self.serial.get() + 1);
        self.engine.answer(self.serial.get(), &answering, &about, &build).unwrap()
    }

    fn held_in(&self, id: &MessageId) -> String {
        let id = id.clone();
        self.world.store.run(move |s| Ok(s.stored(&id)?.0)).unwrap()
    }
}

#[test]
fn an_answer_waits_in_the_outbox_of_the_account_it_came_to_and_u_takes_it_back() {
    let r = running("answer-held", 3600);
    let answered = r.answered_with(&invitation(), &[person("ada@example.org")], Answer::Accept);
    assert_eq!(answered.record, Record::Said(Said::Accepted));
    let held = answered.held.unwrap();
    assert_eq!(held.account.0, "work");
    assert_eq!(r.said(), Ok("Sending to 1 recipient in 3600s".to_string()));
    assert_eq!(r.held_in(&held), "Outbox");
    r.engine.act(Ask { serial: 99, action: Action::Undo, on: vec![held.clone()] });
    assert_eq!(r.said(), Ok("Not sent: back in Drafts".to_string()));
    assert_eq!(r.held_in(&held), "Drafts");
    assert!(r.post.handed().is_empty());
    r.engine.stop();
}

#[test]
fn the_answer_is_handed_to_the_organizer_alone_from_the_invited_address() {
    let r = running("answer-sent", 0);
    let proposal = Answer::Propose { start: Moment::Utc(1_796_202_000), end: None };
    let held = r.answered_with(&invitation(), &[person("ada@example.org")], proposal).held.unwrap();
    assert_eq!(r.said(), Ok("Sending to 1 recipient in 0s".to_string()));
    let sent = r.wait_for(|e| match e {
        Told::Sent { id, .. } => Some(Ok(id.clone())),
        Told::SendFailed { reason, .. } => Some(Err(reason.clone())),
        _ => None,
    });
    assert_eq!(sent, Ok(held));
    let handed = r.post.handed();
    assert_eq!(handed.len(), 1);
    assert_eq!(
        (handed[0].from.as_str(), handed[0].to.as_slice()),
        ("ada@example.org", &["organizer@example.com".to_string()][..])
    );
    let message = String::from_utf8_lossy(&handed[0].message).into_owned();
    assert!(
        message.contains("Subject: Proposed new time: Planning") && message.contains("METHOD:COUNTER"),
        "{message}"
    );
    r.engine.stop();
}

#[test]
fn ignoring_hands_nothing_off() {
    let r = running("answer-ignored", 3600);
    let answered = r.answered_with(&invitation(), &[person("ada@example.org")], Answer::Ignore);
    assert_eq!(answered, Answered { held: None, record: Record::Ignored });
    assert!(r.world.here("Outbox").is_empty() && r.world.here("Drafts").is_empty());
    assert!(r.events.recv_timeout(Duration::from_millis(200)).is_err(), "nothing was asked, so nothing is said");
    r.engine.stop();
}

/// ADR-057: an answer leaves from the account the invitation came to, and
/// the engine's check of the sender is the one every message passes.
#[test]
fn an_answer_from_an_address_of_another_account_is_refused() {
    let r = running("answer-elsewhere", 3600);
    let mut event = invitation();
    event.guests.push(Guest { who: person("grace@example.net"), said: Said::Nothing });
    let held = r.answered_with(&event, &[person("grace@example.net")], Answer::Decline).held.unwrap();
    let refused =
        Err(("\"grace@example.net\" is not an address this account sends from".into(), "edit the draft".into()));
    assert_eq!(r.said(), refused);
    assert_eq!(r.held_in(&held), "Drafts");
    assert!(r.world.here("Outbox").is_empty());
    r.engine.stop();
}

#[test]
fn an_event_made_here_is_sent_to_its_guests_and_so_is_its_cancellation() {
    let r = running("event-mail", 0);
    let mut made = invitation();
    made.organizer = Some(person("ada@example.org"));
    made.guests = vec![
        Guest { who: person("grace@example.net"), said: Said::Nothing },
        Guest { who: person("robin@example.com"), said: Said::Nothing },
    ];
    let mine = [person("ada@example.org")];
    let zones = BTreeMap::new();
    let about = About { event: &made, zones: &zones, answers: None, stamp: 0 };
    for (n, mail) in
        [invitation::invite(&made, &mine).unwrap(), invitation::cancel(&made, &mine).unwrap()].iter().enumerate()
    {
        r.engine.send_mail(n as u64 + 1, mail, &about, &build).unwrap();
        assert_eq!(r.said(), Ok("Sending to 2 recipients in 0s".to_string()));
        r.wait_for(|e| matches!(e, Told::Sent { .. }).then_some(()));
    }
    let handed = r.post.handed();
    let methods: Vec<bool> = handed
        .iter()
        .zip(["METHOD:REQUEST", "METHOD:CANCEL"])
        .map(|(letter, method)| String::from_utf8_lossy(&letter.message).contains(method))
        .collect();
    assert_eq!(methods, [true, true]);
    for letter in &handed {
        assert_eq!(
            (letter.from.as_str(), letter.to.as_slice()),
            ("ada@example.org", &["grace@example.net".to_string(), "robin@example.com".to_string()][..])
        );
    }
    r.engine.stop();
}

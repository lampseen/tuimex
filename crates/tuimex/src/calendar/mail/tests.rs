//! Invitations as mail brings them, over the rig the calendar's asks are
//! carried out on: mail delivered to the fixtures' server, synced by a
//! real engine, and looked at as the worker looks at it.

use tuimex_core::calendar::{Guest, Moment, Standing, standing};
use tuimex_core::{AccountId, Action, Answer, EventAnswer, Stored};
use tuimex_engine::calendar::FEED as FEED_KEY;
use tuimex_ui::calendar::Calendar;

use super::*;
use crate::calendar::Handed;
use crate::calendar::carry::tests::{Kind, ME, Rig, id, invitation, mail};

const LARS: &str = "Lars Holm <lars@example.com>";

fn request(n: u32, uid: &str, sequence: u32) -> Vec<u8> {
    mail(n, LARS, "REQUEST", &invitation(uid, sequence, Some("REQUEST"), ""))
}

fn cancel(n: u32, from: &str, uid: &str, sequence: u32) -> Vec<u8> {
    mail(n, from, "CANCEL", &invitation(uid, sequence, Some("CANCEL"), "STATUS:CANCELLED\r\n"))
}

fn look(rig: &mut Rig) -> Vec<String> {
    let post = Post { engines: &rig.engines, store: &rig.store };
    rig.reader.look(&post)
}

fn answer(uid: &str, answer: Answer) -> Action {
    Action::AnswerEvent(Box::new(EventAnswer { event: id(true, uid), answer, message: None }))
}

/// Each event shown, with whether its shelf is local and how the reader
/// stands in it.
fn events(calendar: &Calendar) -> Vec<(bool, String, Option<Standing>)> {
    let shelves = calendar.shelves.iter();
    let events = shelves.flat_map(|shelf| shelf.events.iter().map(|event| (shelf.local, event)));
    events.map(|(local, event)| (local, event.uid.clone(), standing(event, &[ME]))).collect()
}

fn waiting(local: bool, uid: &str) -> (bool, String, Option<Standing>) {
    (local, uid.to_string(), Some(Standing::Needs))
}

#[test]
fn an_invitation_that_is_in_mail_alone_is_on_the_calendar_until_it_is_answered_or_ignored() {
    let mut rig = Rig::new("waiting", Kind::None);
    rig.deliver(&[request(1, "planning@example.com", 0), request(2, "lunch@example.com", 0)], 2);
    assert_eq!(look(&mut rig), Vec::<String>::new(), "an invitation waiting is nothing to announce");
    let (calendar, _) = rig.shown();
    assert_eq!(events(&calendar), [waiting(true, "planning@example.com"), waiting(true, "lunch@example.com")]);
    let came_in: Vec<_> = calendar.came_in.iter().map(|(uid, id)| (uid.as_str(), id.account.0.as_str())).collect();
    assert_eq!(came_in, [("planning@example.com", "work"), ("lunch@example.com", "work")]);
    assert!(rig.files().is_empty(), "it is shown, and stored nowhere until it is answered");
    assert_eq!(rig.reader.event(&id(true, "lunch@example.com")).map(|e| e.summary.as_str()), Some("Planning"));
    assert_eq!(look(&mut rig), Vec::<String>::new());
    assert!(rig.handed().is_empty(), "looked at again with nothing new, nothing is handed over");

    let reply = rig.ask(answer("planning@example.com", Answer::Accept)).unwrap().expect("the organizer is mailed");
    let (head, part) = rig.part_of(&reply);
    assert!(head.contains("In-Reply-To: <inv1@example.com>"), "it threads under the invitation: {head}");
    assert!(part.contains("PARTSTAT=ACCEPTED"), "{part}");
    assert_eq!(rig.files().len(), 1, "answered, it is an event of the account's local calendar");
    let accepted = (true, "planning@example.com".to_string(), Some(Standing::Accepted));
    assert_eq!(events(&rig.shown().0), [accepted.clone(), waiting(true, "lunch@example.com")], "and is there once");

    assert_eq!(rig.ask(answer("lunch@example.com", Answer::Ignore)), Ok(None));
    let (calendar, _) = rig.shown();
    assert_eq!(events(&calendar), std::slice::from_ref(&accepted), "ignored, it is not drawn");
    assert_eq!(calendar.ignored, [(AccountId("work".into()), "lunch@example.com".to_string())]);
    assert_eq!(rig.ask(Action::UndoEvent(2)), Ok(None));
    assert_eq!(events(&rig.shown().0), [accepted, waiting(true, "lunch@example.com")]);
    assert_eq!(rig.ask(Action::UndoEvent(1)), Ok(Some(reply)));
    let both = [waiting(true, "planning@example.com"), waiting(true, "lunch@example.com")];
    assert_eq!(events(&rig.shown().0), both, "taken back, it waits in mail again");
}

#[test]
fn an_invitation_its_provider_s_calendar_holds_is_one_event_and_one_for_a_written_calendar_is_shown_in_it() {
    let mut rig = Rig::new("held", Kind::Feed);
    rig.deliver(&[request(1, "planning@example.com", 0)], 1);
    look(&mut rig);
    assert_eq!(events(&rig.shown().0), [waiting(true, "planning@example.com")]);
    rig.calendars().put("work", FEED_KEY, "", None, &invitation("planning@example.com", 0, None, "")).unwrap();
    rig.reader.read().unwrap();
    assert_eq!(events(&rig.shown().0), [waiting(false, "planning@example.com")], "the provider's copy is the event");

    let mut written = Rig::new("held-written", Kind::CalDav { write: true });
    written.deliver(&[request(1, "planning@example.com", 0)], 1);
    look(&mut written);
    let shown = [waiting(false, "planning@example.com")];
    assert_eq!(events(&written.shown().0), shown, "where an answer will put it");
    let asked = EventAnswer { event: id(false, "planning@example.com"), answer: Answer::Accept, message: None };
    written.ask(Action::AnswerEvent(Box::new(asked))).unwrap();
    assert_eq!(written.dav.texts().len(), 1, "answered, it is written at the provider");
    assert!(written.files().is_empty());
}

#[test]
fn a_run_reads_the_messages_that_are_new_and_those_still_waiting_and_no_others() {
    let mut rig = Rig::new("bounded", Kind::None);
    let uids = ["a@example.com", "b@example.com", "c@example.com"];
    rig.deliver(&[request(1, uids[0], 0), request(2, uids[1], 0), request(3, uids[2], 0)], 3);
    look(&mut rig);
    rig.ask(answer(uids[0], Answer::Accept)).unwrap();
    rig.ask(answer(uids[1], Answer::Ignore)).unwrap();

    // What a later run begins with: the cache, and nothing in memory.
    let cache = rig.root.join("cache");
    rig.reader.mail = Mail::open(&cache);
    look(&mut rig);
    let read: Vec<_> = rig.reader.mail.read.keys().collect();
    assert_eq!(read.len(), 1, "only the one that still waits is read: {read:?}");
    assert_eq!(events(&rig.shown().0)[1], waiting(true, uids[2]));

    // Without the cache every invitation is read once more, and what
    // they say of events already held is nothing new.
    std::fs::remove_file(cache.join(FILE)).unwrap();
    rig.reader.mail = Mail::open(&cache);
    assert_eq!(look(&mut rig), Vec::<String>::new());
    assert_eq!(rig.reader.mail.read.len(), 3);
    assert!(cache.join(FILE).is_file(), "and the cache is written again");
}

#[test]
fn an_update_and_a_cancellation_from_the_organizer_are_applied_once_and_a_stranger_s_change_nothing() {
    let mut rig = Rig::new("later", Kind::None);
    rig.deliver(&[request(1, "planning@example.com", 0)], 1);
    look(&mut rig);
    rig.ask(answer("planning@example.com", Answer::Accept)).unwrap();
    let held = |rig: &Rig| content::calendar(&rig.files()[0].text).events[0].clone();

    let moved = invitation("planning@example.com", 1, Some("REQUEST"), "").replace("Planning", "Planning, moved");
    rig.deliver(&[mail(2, LARS, "REQUEST", &moved)], 2);
    assert_eq!(look(&mut rig), ["Planning was changed by its organizer"]);
    let event = held(&rig);
    assert_eq!((event.summary.as_str(), event.sequence), ("Planning, moved", 1));
    assert_eq!(standing(&event, &[ME]), Some(Standing::Needs), "a new version is a new question");
    assert_eq!(look(&mut rig), Vec::<String>::new(), "it is said once");

    // Mail is hostile: a cancellation is believed from the organizer the
    // event names, and from nobody else (ADR-057).
    rig.deliver(&[cancel(3, "Mallory <mallory@example.net>", "planning@example.com", 2)], 3);
    assert_eq!(look(&mut rig), Vec::<String>::new());
    assert_eq!(held(&rig).summary, "Planning, moved");

    rig.deliver(&[cancel(4, LARS, "planning@example.com", 2)], 4);
    let said = look(&mut rig);
    assert_eq!(said, ["Planning, moved was cancelled by its organizer and is removed from work, local"]);
    assert!(rig.files().is_empty());
    assert!(events(&rig.shown().0).is_empty(), "and what was cancelled does not wait for an answer again");
}

#[test]
fn a_guest_s_reply_to_an_event_organized_here_is_kept_and_said() {
    let mut rig = Rig::new("replies", Kind::None);
    let who = |email: &str| Address { name: None, email: email.into() };
    let event = tuimex_core::calendar::Event {
        uid: String::new(),
        sequence: 0,
        instead_of: None,
        summary: "Offsite".into(),
        start: Moment::Utc(1_795_687_200),
        end: None,
        location: None,
        description: None,
        organizer: Some(who(ME)),
        guests: vec![Guest { who: who("imani@example.com"), said: Said::Nothing }],
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    };
    let stored = Stored { account: AccountId("work".into()), local: true, event, replaces: None, invite: false };
    rig.ask(Action::SaveEvent(Box::new(stored))).unwrap();
    let uid = content::calendar(&rig.files()[0].text).events[0].uid.clone();
    let reply = |from: &str| {
        format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REPLY\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nSEQUENCE:0\r\n\
             DTSTART:20261126T100000Z\r\nORGANIZER:mailto:{ME}\r\nATTENDEE;PARTSTAT=ACCEPTED:mailto:{from}\r\n\
             END:VEVENT\r\nEND:VCALENDAR\r\n"
        )
    };
    // Somebody the event does not invite answers for themselves.
    rig.deliver(&[mail(1, "mallory@example.net", "REPLY", &reply("mallory@example.net"))], 1);
    assert_eq!(look(&mut rig), Vec::<String>::new());
    rig.deliver(&[mail(2, "Imani Castell <imani@example.com>", "REPLY", &reply("imani@example.com"))], 2);
    assert_eq!(look(&mut rig), ["imani@example.com accepted Offsite"]);
    let guests = content::calendar(&rig.files()[0].text).events[0].guests.clone();
    assert_eq!(guests, [Guest { who: who("imani@example.com"), said: Said::Accepted }]);
    assert_eq!(events(&rig.shown().0).len(), 1, "a reply is not an invitation, and waits for nothing");
}

#[test]
fn a_message_read_tells_the_card_its_event_when_it_is_an_invitation_to_answer() {
    let rig = Rig::new("card", Kind::None);
    let message = |key: &str| MessageId { account: AccountId("work".into()), key: key.into() };
    assert!(rig.reader.invited(&message("a"), &request(1, "planning@example.com", 0)));
    assert!(rig.reader.invited(&message("b"), &cancel(2, LARS, "planning@example.com", 1)));
    assert!(!rig.reader.invited(&message("c"), b"From: grace@example.net\r\nSubject: Hello\r\n\r\nNo calendar.\r\n"));
    let told = |handed: Handed| match handed {
        Handed::Invited(id, invited) => (id.key, invited.map(|invited| invited.event.uid.clone())),
        Handed::Calendar { .. } => panic!("no calendar was read"),
    };
    let handed: Vec<_> = rig.handed().into_iter().map(told).collect();
    let none = |key: &str| (key.to_string(), None);
    assert_eq!(handed, [("a".to_string(), Some("planning@example.com".to_string())), none("b"), none("c")]);
}

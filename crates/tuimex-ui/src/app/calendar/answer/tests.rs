use std::collections::BTreeMap;

use tuimex_core::calendar::{DAY, Event as Held, Moment};
use tuimex_core::{AccountId, Action, Address, Answer, EventAnswer, EventId, MessageId};
use tuimex_fixtures::design as fx;

use crate::app::App;
use crate::app::tests::{after, app as fresh, foot, press, reading, screen};
use crate::calendar::Invited;
use crate::calendar::tests::design;

/// Thursday's day, and `j` to the roadmap review.
const ROADMAP: &str = "gclll\rj";

fn roadmap() -> Held {
    design().shelves[2].events.iter().find(|event| event.summary == "Q4 roadmap review").unwrap().clone()
}

fn named(local: bool) -> EventId {
    let uid = "roadmap@design.example.net".to_string();
    EventId { account: AccountId("work".to_string()), local, uid, occurrence: None }
}

fn answered(event: EventId, answer: Answer, message: Option<MessageId>) -> Action {
    Action::AnswerEvent(Box::new(EventAnswer { event, answer, message }))
}

fn actions(app: &mut App) -> Vec<Action> {
    app.asks().into_iter().map(|ask| ask.action).collect()
}

/// The invitation's mail open in the reader, with the event its calendar
/// part carries handed over beside it.
fn card(event: Held) -> App {
    let mut app = reading("invite", "");
    app.set_invited(fx::id("work", "invite"), Some(Invited { event, zones: BTreeMap::new() }));
    app
}

/// ADR-057: the result says whose calendar took the answer, and `local`
/// when that is not the one its provider keeps.
#[test]
fn an_answer_says_where_the_event_went() {
    let mut app = after(&format!("{ROADMAP}ia"));
    assert_eq!(foot(&mut app).1, "✓ Accepted Q4 roadmap review · in work, local · u to undo");
    let asked = [answered(named(false), Answer::Accept, None)];
    assert_eq!(actions(&mut app), asked);
    let mut app = fresh();
    app.set_writable_calendars(vec![AccountId("work".to_string())]);
    press(&mut app, &format!("{ROADMAP}id"));
    assert_eq!(foot(&mut app).1, "✓ Declined Q4 roadmap review · in work · u to undo");
    assert!(screen(&mut app, 120, 40)[4].ends_with("declined"));
    assert_eq!(
        foot(&mut after(&format!("{ROADMAP}it"))).1,
        "✓ Tentatively accepted Q4 roadmap review · in work, local · u to undo"
    );
}

/// An answer kept local because nobody has tried writing the account's
/// own CalDAV calendar says what would try. Kept local for any other
/// reason, a feed among them, it says what it always did.
#[test]
fn an_answer_kept_local_for_want_of_a_write_check_says_setup_can_try_one() {
    let mut app = fresh();
    app.set_unwritten_calendars(vec![AccountId("work".to_string())]);
    press(&mut app, &format!("{ROADMAP}ia"));
    let setup = "tuimex setup can try writing to its calendar";
    assert_eq!(foot(&mut app).1, format!("✓ Accepted Q4 roadmap review · in work, local · {setup} · u to undo"));
    assert_eq!(actions(&mut app), [answered(named(false), Answer::Accept, None)], "what is asked is the same");
    let mut app = fresh();
    app.set_unwritten_calendars(vec![AccountId("personal".to_string())]);
    press(&mut app, &format!("{ROADMAP}ia"));
    assert_eq!(foot(&mut app).1, "✓ Accepted Q4 roadmap review · in work, local · u to undo", "another account's");
}

#[test]
fn an_answer_names_the_mail_the_invitation_came_in_when_that_is_known() {
    let mut calendar = design();
    calendar.came_in.push(("roadmap@design.example.net".to_string(), fx::id("work", "invite")));
    let mut app = fresh();
    app.set_calendar(calendar);
    press(&mut app, &format!("{ROADMAP}ia"));
    let message = Some(fx::id("work", "invite"));
    assert_eq!(actions(&mut app), [answered(named(false), Answer::Accept, message)]);
}

#[test]
fn p_asks_for_a_time_says_what_the_words_mean_and_enter_proposes_it() {
    let mut app = after(&format!("{ROADMAP}ip"));
    assert_eq!(foot(&mut app), (String::new(), "Propose:".to_string()));
    press(&mut app, "fri 9:00");
    assert_eq!(foot(&mut app).0, "proposes Fri 27 Nov 09:00 to 10:30", "for as long as it lasts");
    press(&mut app, "\x7f\x7f\x7f\x7f\x7f");
    assert_eq!(foot(&mut app).0, "proposes Fri 27 Nov 10:00 to 11:30", "a day alone keeps the event's time");
    assert!(app.asks().is_empty(), "nothing is sent while it is typed");
    press(&mut app, "\r");
    let wall = |hour: i64, minute: i64| Moment::Wall { wall: fx::FRI * DAY + hour * 3600 + minute * 60, zone: None };
    let answer = Answer::Propose { start: wall(10, 0), end: Some(wall(11, 30)) };
    assert_eq!(actions(&mut app), [answered(named(false), answer, None)]);
    assert_eq!(foot(&mut app).1, "✓ Proposed Fri 27 Nov 10:00 to Imani Castell · u to undo");
    let rows = screen(&mut app, 120, 40);
    assert!(
        rows[4].ends_with("proposed Fri 10:00") && rows[4].contains("10:00-11:30"),
        "it keeps its time: {}",
        rows[4]
    );
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · Q4 roadmap review is not answered");
    assert!(screen(&mut app, 120, 40)[4].ends_with("needs answer"));
}

#[test]
fn words_that_name_no_date_propose_nothing_and_esc_gives_up() {
    let mut app = after(&format!("{ROADMAP}ipsoon\r"));
    assert_eq!(foot(&mut app).1, "E i: not a date: \"soon\" · 2026-12-05, 5 dec, fri, +2w");
    press(&mut app, "ipfri\x1b");
    assert!(app.asks().is_empty());
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 2 of 4 · 1 to answer");
}

#[test]
fn x_ignores_sends_nothing_the_organizer_would_see_and_u_brings_the_event_back() {
    let mut app = after(&format!("{ROADMAP}ix"));
    assert_eq!(
        foot(&mut app),
        ("Thu 26 Nov · 3 events".to_string(), "✓ Ignored Q4 roadmap review · nothing sent · u to undo".to_string())
    );
    assert_eq!(actions(&mut app), [answered(named(false), Answer::Ignore, None)]);
    press(&mut app, "2");
    assert!(!screen(&mut app, 120, 40).iter().any(|row| row.contains("Q4 roadmap")), "it is in no view");
    press(&mut app, "3u");
    assert_eq!(
        foot(&mut app),
        ("Thu 26 Nov · 2 of 4 · 1 to answer".to_string(), "✓ Undone · Q4 roadmap review is back".to_string())
    );
}

#[test]
fn the_calendar_s_invite_command_gives_the_same_answers() {
    let mut app = after(&format!("{ROADMAP}:invite decline\r"));
    assert_eq!(foot(&mut app).1, "✓ Declined Q4 roadmap review · in work, local · u to undo");
    press(&mut app, ":invite propose\r");
    assert_eq!(foot(&mut app).1, "Propose:");
    assert_eq!(foot(&mut after("gc:invite accept\r")).1, "E :invite: no event is selected");
    assert_eq!(
        foot(&mut after("gc:invite maybe\r")).1,
        "E :invite: it takes accept, tentative, decline, propose or ignore · Tab completes"
    );
}

#[test]
fn an_event_the_reader_organizes_or_was_not_asked_to_says_why_it_is_not_answered() {
    assert_eq!(foot(&mut after("gcl\rji")).1, "E i: nobody asked for an answer to this event", "the dentist");
    let mut calendar = design();
    let own = calendar.shelves[2].events.iter_mut().find(|event| event.summary == "Q4 roadmap review").unwrap();
    own.organizer = Some(Address { name: None, email: "Robin.Vale@example.org".to_string() });
    let mut app = fresh();
    app.set_calendar(calendar);
    press(&mut app, &format!("{ROADMAP}i"));
    assert_eq!(foot(&mut app).1, "E i: you organize this event");
    press(&mut app, ":invite accept\r");
    assert_eq!(foot(&mut app).1, "E :invite: you organize this event");
    assert!(app.asks().is_empty());
}

/// The reader's card answers through the same code, with the event the
/// message carries.
#[test]
fn the_reader_s_card_answers_the_event_its_message_carries() {
    let mut app = card(roadmap());
    press(&mut app, "ia");
    assert_eq!(foot(&mut app).1, "✓ Accepted Q4 roadmap review · in work, local · u to undo");
    let message = Some(fx::id("work", "invite"));
    let asked = [answered(named(false), Answer::Accept, message)];
    assert_eq!(actions(&mut app), asked, "the calendar's own copy is the one answered");
    press(&mut app, "gclll");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events", "and the calendar no longer waits for it");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "E undo: nothing to undo", "it was answered in mail, where u takes it back");
    press(&mut app, "gmu");
    assert_eq!(foot(&mut app).1, "✓ Undone · Q4 roadmap review is not answered");
    assert_eq!(actions(&mut app).len(), 1);
    press(&mut app, "gc");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events · 1 to answer");
}

#[test]
fn u_in_mail_takes_back_the_answer_and_what_was_done_to_mail_each_in_its_turn() {
    let mut app = card(roadmap());
    press(&mut app, "sit");
    assert_eq!(foot(&mut app).1, "✓ Tentatively accepted Q4 roadmap review · in work, local · u to undo");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · Q4 roadmap review is not answered");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · 1 message unflagged");
}

#[test]
fn an_invitation_no_calendar_holds_yet_goes_into_the_account_s_local_calendar() {
    let mut without = design();
    without.shelves[2].events.retain(|event| event.summary != "Q4 roadmap review");
    let mut app = card(roadmap());
    app.set_calendar(without);
    press(&mut app, "ia");
    let message = Some(fx::id("work", "invite"));
    assert_eq!(actions(&mut app), [answered(named(true), Answer::Accept, message)]);
    press(&mut app, "gclll\rj");
    let row = screen(&mut app, 120, 40)[4].clone();
    assert!(row.contains("10:00-11:30  Q4 roadmap review") && row.ends_with("local"), "{row}");
}

#[test]
fn the_card_proposes_a_time_from_the_reader_and_ignores() {
    let mut app = card(roadmap());
    press(&mut app, "ipfri 9:00");
    assert_eq!(foot(&mut app), ("proposes Fri 27 Nov 09:00 to 10:30".to_string(), "Propose: fri 9:00".to_string()));
    press(&mut app, "\r");
    assert_eq!(foot(&mut app).1, "✓ Proposed Fri 27 Nov 09:00 to Imani Castell · u to undo");
    let mut app = card(roadmap());
    press(&mut app, ":invite ignore\r");
    assert_eq!(foot(&mut app).1, "✓ Ignored Q4 roadmap review · nothing sent · u to undo");
}

#[test]
fn a_card_whose_event_was_not_handed_over_says_it_could_not_be_read() {
    for (keys, name) in [("ia", "i"), ("ip", "i"), ("ix", "i"), (":invite accept\r", ":invite")] {
        let mut app = reading("invite", keys);
        assert_eq!(foot(&mut app).1, format!("E {name}: this message's invitation could not be read"), "{keys}");
        assert!(app.asks().is_empty());
    }
    assert_eq!(
        foot(&mut after("\r:invite maybe\r")).1,
        "E :invite: it takes accept, tentative, decline, propose or ignore · Tab completes"
    );
    assert_eq!(foot(&mut after(":invite accept\r")).1, "E :invite: no message is open · Enter opens one");
}

#[test]
fn the_card_refuses_an_event_the_account_organizes_and_says_when_it_answers_uninvited() {
    let mine = Address { name: None, email: "robin.vale@example.org".to_string() };
    let mut app = card(Held { organizer: Some(mine), ..roadmap() });
    app.set_calendar(crate::calendar::Calendar::empty(fx::NOW));
    press(&mut app, "ia");
    assert_eq!(foot(&mut app).1, "E i: you organize this event");
    let mut others = roadmap();
    others.guests.remove(0);
    let mut app = card(others);
    app.set_calendar(crate::calendar::Calendar::empty(fx::NOW));
    press(&mut app, "ia");
    assert_eq!(foot(&mut app).1, "✓ Accepted Q4 roadmap review · in work, local as robin.vale@example.org · u to undo");
}

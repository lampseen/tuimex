use tuimex_core::calendar::{DAY, Event as Held, Moment};
use tuimex_core::{AccountId, Action, Answer, Event, EventAnswer, EventId, Stored};
use tuimex_fixtures::design as fx;

use crate::app::App;
use crate::app::tests::{after, foot, press, screen};
use crate::calendar::tests::design;

/// Thursday's day with its first event selected: `j` is the roadmap
/// review and `jj` the vendor call.
const THU: &str = "gclll\r";
const NEEDS: &str = "Thu 26 Nov · 2 of 4 · 1 to answer";

fn work(id: &str, occurrence: Option<Moment>) -> EventId {
    let uid = format!("{id}@design.example.net");
    EventId { account: AccountId("work".to_string()), local: false, uid, occurrence }
}

/// What was asked for since this was last called, and the serial of the
/// last of it.
fn asked(app: &mut App) -> (Vec<Action>, u64) {
    let asks = app.asks();
    let serial = asks.last().map_or(0, |ask| ask.serial);
    (asks.into_iter().map(|ask| ask.action).collect(), serial)
}

fn refuse(app: &mut App, serial: u64, reason: &str) {
    app.event(Event::ActionFailed { serial, reason: reason.to_string(), hint: String::new() });
}

fn titles(app: &mut App) -> Vec<String> {
    let rows = screen(app, 120, 40);
    let day = rows[3..].iter().take_while(|row| !row.is_empty());
    let title =
        |row: &String| row.chars().skip(28).collect::<String>().split("  ").next().unwrap_or_default().to_string();
    day.map(title).collect()
}

#[test]
fn d_deletes_at_once_and_u_brings_the_event_back_with_the_cursor() {
    let mut app = after(&format!("{THU}jjd"));
    assert_eq!(asked(&mut app).0, [Action::DeleteEvent(Box::new(work("vendor", None)))]);
    assert_eq!(titles(&mut app), ["Standup", "Q4 roadmap review", "Staging migration window"]);
    let deleted = ("Thu 26 Nov · 3 events · 1 to answer".to_string(), "✓ Deleted Vendor call · u to undo".to_string());
    assert_eq!(foot(&mut app), deleted, "nothing is selected where the event was");
    press(&mut app, "u");
    let (asks, _) = asked(&mut app);
    assert_eq!(asks, [Action::UndoEvent(1)], "the undo names the ask it takes back (ADR-040)");
    assert_eq!(
        foot(&mut app),
        ("Thu 26 Nov · 3 of 4 · 1 to answer".to_string(), "✓ Undone · Vendor call is back".to_string())
    );
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "E undo: nothing to undo");
}

#[test]
fn u_takes_back_the_newest_change_first_and_never_reaches_into_mail() {
    let mut app = after(&format!("e{THU}jiajd"));
    assert_eq!(titles(&mut app), ["Standup", "Q4 roadmap review", "Staging migration window"]);
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · Vendor call is back");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · Q4 roadmap review is not answered");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "E undo: nothing to undo", "what was archived in mail is mail's to take back");
    press(&mut app, "gmu");
    assert_eq!(foot(&mut app).1, "✓ Undone · 1 message back in Inbox");
    let undone: Vec<Action> = asked(&mut app).0.into_iter().filter(|a| matches!(a, Action::UndoEvent(_))).collect();
    assert_eq!(undone.len(), 2);
}

#[test]
fn u_in_mail_does_not_take_back_what_was_done_in_the_calendar() {
    let mut app = after(&format!("{THU}jjdgmu"));
    assert_eq!(foot(&mut app).1, "E undo: nothing to undo");
    press(&mut app, "gc");
    assert_eq!(titles(&mut app).len(), 3, "the vendor call is still deleted");
}

/// A calendar read before the change was carried out is older than the
/// change, which is made of it again; one read after is the truth.
#[test]
fn a_calendar_read_while_a_change_is_not_carried_out_does_not_undo_it() {
    let mut app = after(&format!("{THU}jjd"));
    let (_, serial) = asked(&mut app);
    app.set_calendar(design());
    assert_eq!(titles(&mut app).len(), 3, "the vendor call stays deleted");
    app.calendar_carried(serial - 1);
    app.set_calendar(design());
    assert_eq!(titles(&mut app).len(), 3, "an older ask carried out does not settle it");
    app.calendar_carried(serial);
    app.set_calendar(design());
    assert_eq!(titles(&mut app).len(), 4, "carried out, the calendar read is what there is");
}

#[test]
fn a_calendar_read_after_u_and_before_the_undo_is_carried_out_keeps_what_u_put_back() {
    let mut app = after(&format!("{THU}jjd"));
    let (_, serial) = asked(&mut app);
    let mut without = design();
    without.shelves[2].events.retain(|event| event.summary != "Vendor call");
    press(&mut app, "u");
    app.calendar_carried(serial);
    app.set_calendar(without.clone());
    assert_eq!(titles(&mut app).len(), 4, "the delete was carried out and its undo was not");
    app.set_calendar(design());
    assert_eq!(titles(&mut app).len(), 4, "neither was");
    let (_, undo) = asked(&mut app);
    app.calendar_carried(undo);
    app.set_calendar(without);
    assert_eq!(titles(&mut app).len(), 3);
}

#[test]
fn a_refusal_puts_back_what_that_change_did_and_says_why() {
    let mut app = after(&format!("{THU}jiajd"));
    let (_, delete) = asked(&mut app);
    refuse(&mut app, delete - 1, "the reply could not be filed");
    assert_eq!(foot(&mut app).1, "E i: the reply could not be filed · Q4 roadmap review is not answered");
    assert_eq!(titles(&mut app), ["Standup", "Q4 roadmap review", "Staging migration window"], "only the answer");
    assert!(screen(&mut app, 120, 40)[4].ends_with("needs answer"));
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · Vendor call is back", "u reaches the change that stood");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "E undo: nothing to undo", "and not the one refused");
    app.set_calendar(design());
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 3 of 4 · 1 to answer", "a refused change is not made again");
}

#[test]
fn a_refused_undo_leaves_the_change_as_it_was() {
    let mut app = after(&format!("{THU}jjdu"));
    let (_, undo) = asked(&mut app);
    refuse(&mut app, undo, "its provider has already removed it");
    assert_eq!(foot(&mut app).1, "E undo: its provider has already removed it · nothing was taken back");
    assert_eq!(titles(&mut app).len(), 3);
}

#[test]
fn a_refusal_of_a_change_already_taken_back_changes_nothing() {
    let mut app = after(&format!("{THU}jjdu"));
    let (_, undo) = asked(&mut app);
    refuse(&mut app, undo - 1, "work's calendar is read-only");
    assert_eq!(foot(&mut app).1, "✓ Undone · Vendor call is back");
    assert_eq!(titles(&mut app).len(), 4);
}

/// ADR-060: one day of an event that repeats is asked about first.
#[test]
fn d_on_a_repeating_event_asks_and_this_one_leaves_the_other_days() {
    let mut app = after(&format!("{THU}d"));
    assert_eq!(foot(&mut app).1, "Delete which?  t this one · a all of them · Esc cancel");
    assert!(app.asks().is_empty());
    press(&mut app, "t");
    let thursday = Moment::Wall { wall: fx::THU * DAY + 9 * 3600 + 30 * 60, zone: None };
    assert_eq!(asked(&mut app).0, [Action::DeleteEvent(Box::new(work("standup", Some(thursday))))]);
    assert_eq!(foot(&mut app).1, "✓ Deleted Standup on Thu 26 Nov · u to undo");
    assert_eq!(titles(&mut app).len(), 3);
    press(&mut app, "l");
    assert!(titles(&mut app).contains(&"Standup".to_string()), "Friday's is still there");
    press(&mut app, "hu");
    assert_eq!(
        foot(&mut app),
        ("Thu 26 Nov · 1 of 4 · 1 to answer".to_string(), "✓ Undone · Standup is back".to_string())
    );
}

#[test]
fn all_of_them_deletes_every_day_and_any_other_key_deletes_none() {
    let mut app = after(&format!("{THU}da"));
    assert_eq!(asked(&mut app).0, [Action::DeleteEvent(Box::new(work("standup", None)))]);
    assert_eq!(foot(&mut app).1, "✓ Deleted Standup · u to undo");
    press(&mut app, "l");
    assert!(!titles(&mut app).contains(&"Standup".to_string()), "Friday's too");
    for other in ["\x1b", "x", "\r"] {
        let mut app = after(&format!("{THU}d{other}"));
        assert!(app.asks().is_empty(), "{other:?}");
        assert_eq!(foot(&mut app).0, "Thu 26 Nov · 1 of 4 · 1 to answer");
    }
}

#[test]
fn e_on_a_repeating_event_asks_and_this_one_is_saved_standing_in_for_its_day() {
    let mut app = after(&format!("{THU}E"));
    assert_eq!(foot(&mut app).1, "Edit which?  t this one · a all of them · Esc cancel");
    press(&mut app, "t");
    let rows = screen(&mut app, 120, 40);
    assert_eq!((rows[0].as_str(), rows[4].as_str()), ("  Standup", "    Starts   Thu 26 Nov 2026 09:30"));
    assert_eq!(rows[7], "    Repeats  never", "the one day is no rule");
    press(&mut app, " at the desks^s");
    let thursday = Moment::Wall { wall: fx::THU * DAY + 9 * 3600 + 30 * 60, zone: None };
    let (asks, _) = asked(&mut app);
    let [Action::SaveEvent(stored)] = asks.as_slice() else { panic!("{asks:?}") };
    assert_eq!(stored.replaces, Some(work("standup", Some(thursday.clone()))));
    let event = &stored.event;
    assert_eq!(
        (event.summary.as_str(), &event.instead_of, &event.start),
        ("Standup at the desks", &Some(thursday.clone()), &thursday)
    );
    assert_eq!((&event.repeat, event.uid.as_str()), (&None, "standup@design.example.net"));
    assert_eq!(
        foot(&mut app),
        ("Thu 26 Nov · 1 of 4 · 1 to answer".to_string(), "✓ Changed Standup at the desks · u to undo".to_string())
    );
    press(&mut app, "l");
    assert!(titles(&mut app).contains(&"Standup".to_string()), "the other days keep their title");
    press(&mut app, "hu");
    assert_eq!(foot(&mut app).1, "✓ Undone · Standup at the desks is as it was");
    assert_eq!(titles(&mut app)[0], "Standup");
}

#[test]
fn all_of_them_opens_the_event_with_its_rule() {
    let mut app = after(&format!("{THU}Ea"));
    assert_eq!(screen(&mut app, 120, 40)[7], "    Repeats  every weekday");
    press(&mut app, "!^s");
    let (asks, _) = asked(&mut app);
    let [Action::SaveEvent(stored)] = asks.as_slice() else { panic!("{asks:?}") };
    assert_eq!((stored.replaces.clone(), &stored.event.instead_of), (Some(work("standup", None)), &None));
    press(&mut app, "l");
    assert!(titles(&mut app).contains(&"Standup!".to_string()), "every day has the new title");
}

#[test]
fn a_saved_event_is_in_the_calendar_at_once_selected_and_u_removes_it() {
    let mut app = after("gclllcDentist^s");
    let (asks, serial) = asked(&mut app);
    let [Action::SaveEvent(stored)] = asks.as_slice() else { panic!("{asks:?}") };
    let Stored { account, local, event, replaces, invite } = &**stored;
    assert_eq!((account.0.as_str(), *local, replaces, *invite), ("personal", true, &None, false));
    assert_eq!((event.summary.as_str(), event.uid.as_str()), ("Dentist", ""), "its keeper gives it its UID");
    assert_eq!(foot(&mut app).1, "✓ Added Dentist to personal, local · u to undo");
    press(&mut app, "\r");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[3].starts_with("▌   personal   09:00-10:00  Dentist") && rows[3].ends_with("local"), "{}", rows[3]);
    // Its keeper's copy, under the UID the keeper gave it, is the same event.
    let mut read = design();
    let stored = Held { uid: "k7@tuimex.example.net".to_string(), ..event.clone() };
    read.shelves[1].events.push(stored);
    app.set_calendar(read.clone());
    assert_eq!(titles(&mut app).len(), 5, "not put a second time over the copy read back");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 1 of 5 · 1 to answer", "and the cursor is on the keeper's copy");
    app.calendar_carried(serial);
    app.set_calendar(read);
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 1 of 5 · 1 to answer");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · Dentist is gone");
    assert_eq!(titles(&mut app).len(), 4, "the keeper's copy leaves the screen at once");
    assert_eq!(asked(&mut app).0, [Action::UndoEvent(serial)]);
}

#[test]
fn an_event_with_guests_is_saved_to_be_mailed_once_that_is_agreed_to() {
    let mut app = after("gclllcOffsite\t\t\t\t\t\t\tlars@example.org^s");
    assert!(app.asks().is_empty(), "asked first");
    press(&mut app, "y");
    let (asks, _) = asked(&mut app);
    let [Action::SaveEvent(stored)] = asks.as_slice() else { panic!("{asks:?}") };
    assert!(stored.invite && stored.event.guests.len() == 1);
}

#[test]
fn an_answer_changes_only_the_reader_s_own_word_in_the_event_held() {
    let mut app = after(&format!("{THU}jit"));
    let roadmap = app.cal.model.shelves[2].events.iter().find(|e| e.summary == "Q4 roadmap review").unwrap();
    let said: Vec<_> = roadmap.guests.iter().map(|guest| guest.said).collect();
    use tuimex_core::calendar::Said::{Accepted, Declined, Nothing, Tentative};
    assert_eq!(said, [Tentative, Accepted, Declined, Nothing]);
    let (asks, _) = asked(&mut app);
    assert_eq!(
        asks,
        [Action::AnswerEvent(Box::new(EventAnswer {
            event: work("roadmap", None),
            answer: Answer::Tentative,
            message: None
        }))]
    );
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 2 of 4");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).0, NEEDS);
}

use tuimex_core::calendar::DAY;
use tuimex_core::{AccountId, Action, Event, EventId, Timestamp};
use tuimex_fixtures::design as fx;

use crate::app::App;
use crate::app::tests::{after, app as fresh, foot, press, screen};
use crate::calendar::Due;

/// The bank's account review on Friday, whose link's text names another
/// host than it opens, with its links listed.
const LYING: &str = "gcI\rIo";
const ASKED: &str = "▲ Text says bank.example.com; opens bank.example.com.device-check.example.net. Open?";
const OPENS: &str = "opens https://bank.example.com.device-check.example.net/meet";

fn review() -> EventId {
    let uid = "review@design.example.net".to_string();
    EventId { account: AccountId("personal".to_string()), local: false, uid, occurrence: None }
}

fn asked(app: &mut App) -> (Vec<Action>, u64) {
    let asks = app.asks();
    let serial = asks.last().map_or(0, |ask| ask.serial);
    (asks.into_iter().map(|ask| ask.action).collect(), serial)
}

/// The Halden call, which begins today at three.
fn halden() -> Due {
    Due {
        account: AccountId("work".to_string()),
        uid: "halden-call@design.example.net".to_string(),
        summary: "Halden contract call".to_string(),
        start: Timestamp(fx::NOW.0.div_euclid(DAY) * DAY + 15 * 3600),
    }
}

fn right(app: &mut App) -> String {
    screen(app, 120, 40)[38].split("   ").last().unwrap_or_default().trim().to_string()
}

#[test]
fn a_link_opens_with_its_real_address_on_the_status_line() {
    let mut app = after("gclll\rjo\r");
    let roadmap =
        EventId { uid: "roadmap@design.example.net".to_string(), account: AccountId("work".to_string()), ..review() };
    assert_eq!(asked(&mut app).0, [Action::OpenEventLink(Box::new(roadmap.clone()), 1)]);
    let opening = ("opens https://meet.example.org/r/q4-roadmap".to_string(), "Opening meet.example.org".to_string());
    assert_eq!(foot(&mut app), opening);
    press(&mut app, "j");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 3 of 4 · 1 to answer", "until the next key");
    let mut app = after("gclll\rjo1");
    assert_eq!(asked(&mut app).0, [Action::OpenEventLink(Box::new(roadmap), 1)], "a digit opens that one");
}

/// ADR-018: a link whose text names another host is confirmed first.
#[test]
fn a_deceptive_link_is_asked_about_and_opens_only_on_yes() {
    let mut app = after(&format!("{LYING}\r"));
    assert_eq!(foot(&mut app), (OPENS.to_string(), format!("{ASKED}  y yes · n no · A ask the agent")));
    assert!(app.asks().is_empty(), "nothing opens before the answer");
    press(&mut app, "y");
    assert_eq!(asked(&mut app).0, [Action::OpenEventLink(Box::new(review()), 1)]);
    assert_eq!(foot(&mut app), (OPENS.to_string(), "Opening bank.example.com.device-check.example.net".to_string()));
    for no in ["n", "\x1b", "x"] {
        let mut app = after(&format!("{LYING}\r{no}"));
        assert!(app.asks().is_empty(), "{no:?} leaves it closed");
        assert_eq!(foot(&mut app).0, "Fri 27 Nov · 3 of 4 · 1 to answer");
    }
}

#[test]
fn the_agent_is_asked_about_a_link_as_in_mail_and_opens_nothing() {
    let mut app = after(&format!("{LYING}\rA"));
    let (asks, serial) = asked(&mut app);
    assert_eq!(asks, [Action::AgentEventLink(Box::new(review()), 1)]);
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[33], "  Links");
    assert!(rows[36].contains("Asking the agent · it reads the address and the message"), "{}", rows[36]);
    assert_eq!(foot(&mut app).1, format!("{ASKED}  y yes · n no"), "the question stays, without the agent's key");
    app.event(Event::AgentAsked { serial, replaced: "1 address".to_string() });
    assert_eq!(right(&mut app), "agent saw placeholders for 1 address");
    app.event(Event::AgentAdvice { serial, advice: "against opening".to_string(), why: "not the bank's".to_string() });
    let rows = screen(&mut app, 120, 40);
    assert!(rows[35].ends_with("Agent advises against opening · not the bank's"), "{}", rows[35]);
    press(&mut app, "n");
    assert!(app.asks().is_empty());
    assert_eq!(foot(&mut app).0, "Fri 27 Nov · 3 of 4 · 1 to answer", "the list and the advice close with the answer");
    // From the list itself, and when the agent cannot be run.
    let mut app = after(&format!("{LYING}A"));
    let (asks, serial) = asked(&mut app);
    assert_eq!(asks, [Action::AgentEventLink(Box::new(review()), 1)]);
    app.event(Event::ActionFailed { serial, reason: "it timed out".to_string(), hint: String::new() });
    assert!(screen(&mut app, 120, 40)[36].contains("E :agent link: it timed out"));
    let mut plain = fresh();
    plain.set_up(fx::identities(), false);
    press(&mut plain, &format!("{LYING}\r"));
    assert_eq!(foot(&mut plain).1, format!("{ASKED}  y yes · n no"), "no agent is offered where none is set up");
}

#[test]
fn a_link_that_could_not_be_opened_says_why() {
    let mut app = after("gclll\rjo\r");
    let (_, serial) = asked(&mut app);
    app.event(Event::ActionFailed {
        serial,
        reason: "o: no browser could be started".to_string(),
        hint: String::new(),
    });
    assert_eq!(foot(&mut app).1, "E o: no browser could be started");
}

/// ADR-058: a reminder is on the status line wherever the reader is.
#[test]
fn a_reminder_shows_in_mail_and_in_the_calendar_until_the_event_begins() {
    let mut app = fresh();
    app.set_reminder(Some(halden()));
    let reminder = "▲ Halden contract call in 40 min · o for its link · sending in 6s · 1 not sent";
    assert_eq!(right(&mut app), reminder, "in the list");
    press(&mut app, "\r");
    assert!(screen(&mut app, 120, 40)[38].ends_with(reminder), "in the reader");
    press(&mut app, "gc");
    assert_eq!(right(&mut app), reminder, "in the calendar");
    assert!(app.tick(Timestamp(halden().start.0 - 61)), "a minute less is a frame");
    assert!(right(&mut app).starts_with("▲ Halden contract call in 2 min"));
    assert!(app.tick(halden().start));
    assert!(right(&mut app).starts_with("sending in"), "it has begun");
    app.set_reminder(Some(halden()));
    assert!(right(&mut app).starts_with("sending in"), "one for an event that has begun is not shown");
}

#[test]
fn o_from_mail_opens_the_reminded_event_s_links_and_the_reminder_goes() {
    let mut app = fresh();
    press(&mut app, "o");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Inbox"), "with no reminder the list has no such key");
    app.set_reminder(Some(halden()));
    press(&mut app, "jo");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[2], "  Today · Mon 23 November");
    assert_eq!(rows[35..37], ["  Links", "▌    1  meet.example.org   the meeting"]);
    assert_eq!(foot(&mut app).0, "opens https://meet.example.org/r/halden");
    assert_eq!(right(&mut app), "sending in 6s · 1 not sent", "the reminder was acted on");
    press(&mut app, "\r");
    assert_eq!(foot(&mut app).1, "Opening meet.example.org");
    press(&mut app, "gm");
    assert!(foot(&mut app).0.starts_with("2 of 24"), "mail is where it was");
}

#[test]
fn o_in_the_calendar_is_the_selected_event_s_and_the_reminder_s_only_where_none_is_selected() {
    let mut app = fresh();
    app.set_reminder(Some(halden()));
    press(&mut app, "gclll\rjjjo");
    assert_eq!(foot(&mut app).1, "E o: this event has no link", "the migration window is selected");
    assert!(right(&mut app).starts_with("▲ Halden"), "and the reminder stays");
    press(&mut app, "\x1bgto");
    assert_eq!(
        screen(&mut app, 120, 40)[36],
        "▌    1  meet.example.org   the meeting",
        "from the month, narrowed to personal"
    );
    assert_eq!(foot(&mut app).0, "opens https://meet.example.org/r/halden");
    let mut app = fresh();
    app.set_reminder(Some(Due { uid: "gone@example.net".to_string(), ..halden() }));
    press(&mut app, "gco");
    assert_eq!(foot(&mut app).1, "E o: no event is selected", "a reminder of an event no calendar shows");
}

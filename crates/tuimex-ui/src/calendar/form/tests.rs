use tuimex_core::calendar::{DAY, Event, Freq, Guest, Moment, Repeat, Rule, Said, Zone};
use tuimex_core::{AccountId, Address};
use tuimex_fixtures::design as fx;

use super::{Field, Form, Saved, Target};
use crate::palette::Tok;

const HOUR: i64 = 3600;

fn target(account: &str, local: bool) -> Target {
    Target { account: AccountId(account.to_string()), local }
}

fn targets() -> Vec<Target> {
    vec![target("personal", true), target("work", false), target("work", true)]
}

/// A new event on Thursday the 26th at nine, three days after today.
fn fresh() -> Form {
    Form::new(targets(), 0, fx::THU * DAY + 9 * HOUR, "", fx::THU - 3)
}

fn type_in(form: &mut Form, text: &str) {
    for c in text.chars() {
        assert!(form.edit_with(|typed| {
            typed.push(c);
            true
        }));
    }
}

fn to(form: &mut Form, field: Field) {
    while form.focus != field {
        form.step(true);
    }
}

fn saved(form: &Form) -> Result<Saved, String> {
    form.saved(&Zone::fixed(0), &fx::identities())
}

fn event(form: &Form) -> Event {
    saved(form).unwrap().event
}

/// The design's event with this id, and the calendar that holds it.
fn design(id: &str) -> (Target, Event) {
    let uid = format!("{id}@design.example.net");
    for kept in fx::calendars() {
        if let Some(event) = kept.events.into_iter().find(|e| e.uid == uid) {
            return (target(kept.account, kept.local), event);
        }
    }
    panic!("no event {id}");
}

/// The form opened on a design event that is not all day, as the day it
/// starts on draws it.
fn editing(id: &str) -> (Event, Form) {
    let (at, event) = design(id);
    let (start, end) = (event.start.wall(), event.end.as_ref().unwrap().wall());
    let form = Form::edit(targets(), at, &event, (start, end, false), fx::THU - 3);
    (event, form)
}

fn text(line: &[crate::span::Span]) -> String {
    line.iter().map(|span| span.text.as_str()).collect()
}

#[test]
fn a_new_event_is_an_hour_long_with_no_uid_until_it_is_stored() {
    let mut form = fresh();
    type_in(&mut form, "Offsite planning");
    let saved = saved(&form).unwrap();
    assert_eq!((saved.to, saved.replaces, saved.invite), (target("personal", true), None, false));
    let event = saved.event;
    assert_eq!((event.uid.as_str(), event.summary.as_str()), ("", "Offsite planning"));
    let nine = fx::THU * DAY + 9 * HOUR;
    assert_eq!((event.start, event.end), (Moment::Utc(nine), Some(Moment::Utc(nine + HOUR))));
    assert_eq!((event.repeat, event.alarms, event.location, event.description), (None, vec![600], None, None));
}

#[test]
fn a_new_time_is_the_instant_the_reader_s_clock_shows() {
    let form = fresh();
    let nine = fx::THU * DAY + 9 * HOUR;
    let saved = form.saved(&Zone::fixed(HOUR), &fx::identities()).unwrap();
    assert_eq!(saved.event.start, Moment::Utc(nine - HOUR), "nine in a zone an hour east is eight in UTC");
}

#[test]
fn typing_goes_into_the_focused_field_and_a_choice_takes_none() {
    let mut form = fresh();
    type_in(&mut form, "Planning");
    for (field, typed) in [(Field::Where, "Room Fjord"), (Field::Guests, "imani@example.org")] {
        to(&mut form, field);
        type_in(&mut form, typed);
    }
    assert_eq!(
        (form.title.as_str(), form.place.as_str(), form.guests.as_str()),
        ("Planning", "Room Fjord", "imani@example.org")
    );
    for field in [Field::Calendar, Field::AllDay, Field::Repeats, Field::Reminder] {
        to(&mut form, field);
        assert!(!form.edit_with(|typed| {
            typed.push('x');
            true
        }));
    }
    // A date's words start empty: what is typed replaces the date shown.
    to(&mut form, Field::Starts);
    type_in(&mut form, "fri");
    assert_eq!(form.starts_words.as_deref(), Some("fri"));
    assert!(form.dirty);
}

#[test]
fn each_choice_steps_both_ways_and_round_and_a_typed_field_has_none() {
    let mut form = fresh();
    assert!(!form.choose(true), "the title has nothing to choose from");
    to(&mut form, Field::Calendar);
    form.choose(true);
    assert_eq!(form.calendar, 1);
    form.choose(false);
    form.choose(false);
    assert_eq!(form.calendar, 2, "back from the first is the last");
    to(&mut form, Field::AllDay);
    form.choose(true);
    assert!(form.all_day);
    form.choose(false);
    assert!(!form.all_day);

    let labels = |form: &Form, field| {
        let mut form = form.clone();
        to(&mut form, field);
        let first = text(&form.value(field)[0]);
        let mut all = vec![first.clone()];
        loop {
            form.choose(true);
            let now = text(&form.value(field)[0]);
            if now == first {
                return all;
            }
            all.push(now);
        }
    };
    assert_eq!(labels(&form, Field::Repeats), ["never", "daily", "weekly", "every weekday", "monthly", "yearly"]);
    let reminders = labels(&form, Field::Reminder);
    assert_eq!(reminders[0], "10 minutes before", "a new event is reminded ten minutes before");
    let mut sorted = reminders.clone();
    sorted.rotate_right(3);
    assert_eq!(
        sorted,
        [
            "none",
            "at the start",
            "5 minutes before",
            "10 minutes before",
            "15 minutes before",
            "30 minutes before",
            "1 hour before",
            "1 day before"
        ]
    );
}

#[test]
fn each_reminder_is_its_lead_in_seconds() {
    let mut form = fresh();
    to(&mut form, Field::Reminder);
    let mut leads = Vec::new();
    for _ in 0..8 {
        form.choose(true);
        leads.push(event(&form).alarms);
    }
    let want: [&[i64]; 8] = [&[900], &[1800], &[3600], &[86_400], &[], &[0], &[300], &[600]];
    assert_eq!(leads, want);
}

#[test]
fn each_repeats_choice_becomes_its_rule_and_keeps_its_hour_on_the_wall() {
    let rule = |freq, by_day: Vec<(i64, i64)>| {
        Some(Repeat::Rule(Rule { freq, interval: 1, count: None, until: None, by_day, by_month_day: Vec::new() }))
    };
    let mut form = fresh();
    to(&mut form, Field::Repeats);
    let want = [
        rule(Freq::Daily, vec![]),
        rule(Freq::Weekly, vec![]),
        rule(Freq::Weekly, (1..=5).map(|weekday| (0, weekday)).collect()),
        rule(Freq::Monthly, vec![]),
        rule(Freq::Yearly, vec![]),
        None,
    ];
    for want in want {
        form.choose(true);
        let event = event(&form);
        assert_eq!(event.repeat, want);
        let nine = fx::THU * DAY + 9 * HOUR;
        let start = if want.is_some() { Moment::Wall { wall: nine, zone: None } } else { Moment::Utc(nine) };
        assert_eq!(event.start, start);
    }
}

#[test]
fn a_date_is_typed_as_words_and_moving_the_start_moves_the_end() {
    let mut form = fresh();
    to(&mut form, Field::Starts);
    assert_eq!(text(&form.left()), "starts Thu 26 Nov 2026 09:00");
    type_in(&mut form, "fri 14:00");
    assert_eq!(text(&form.left()), "starts Fri 27 Nov 2026 14:00");
    form.step(true);
    let two = fx::FRI * DAY + 14 * HOUR;
    assert_eq!((form.starts, form.ends, form.starts_words.as_deref()), (two, two + HOUR, None));
    assert_eq!(text(&form.value(Field::Starts)[0]), "Fri 27 Nov 2026 14:00");
    // A time alone is on the day the field held.
    type_in(&mut form, "16:30");
    assert_eq!(text(&form.left()), "ends Fri 27 Nov 2026 16:30");
    form.step(true);
    assert_eq!(form.ends, fx::FRI * DAY + 16 * HOUR + 30 * 60);
    assert_eq!(text(&form.left()), "not saved");
}

/// Bryan: "I shouldn't have to type anything except the time." What is
/// typed is merged with what the field held.
#[test]
fn a_time_alone_keeps_the_date_and_a_date_alone_keeps_the_time() {
    let at = |day: i64, hour: i64, minute: i64| day * DAY + hour * HOUR + minute * 60;
    let mut form = fresh();
    to(&mut form, Field::Starts);
    for (words, minute) in [("14:30", 870), ("1430", 870), ("2pm", 840), ("2:30pm", 870), ("9", 540), ("930", 570)] {
        form.starts_words = Some(words.to_string());
        assert_eq!(form.meaning(Field::Starts), Some(fx::THU * DAY + minute * 60), "{words}");
    }
    form.starts_words = None;
    type_in(&mut form, "1430");
    assert_eq!(text(&form.value(Field::Starts)[0]), "1430", "while it is typed the field shows what is typed");
    assert_eq!(text(&form.left()), "starts Thu 26 Nov 2026 14:30");
    form.step(true);
    assert_eq!((form.starts, form.ends), (at(fx::THU, 14, 30), at(fx::THU, 15, 30)), "and the end moved as far");
    assert_eq!(text(&form.value(Field::Starts)[0]), "Thu 26 Nov 2026 14:30", "left, it is written whole again");
    form.step(false);
    type_in(&mut form, "fri");
    form.step(true);
    assert_eq!((form.starts, form.ends), (at(fx::FRI, 14, 30), at(fx::FRI, 15, 30)), "a date alone keeps the time");
    // An end over midnight keeps its own day when only its time is typed,
    // and a step is from the day the field held.
    type_in(&mut form, "sat 1:00");
    form.step(true);
    form.step(false);
    type_in(&mut form, "2:15");
    assert_eq!(form.meaning(Field::Ends), Some(at(fx::FRI + 1, 2, 15)));
    form.ends_words = Some("+1d".to_string());
    assert_eq!(form.meaning(Field::Ends), Some(at(fx::FRI + 2, 1, 0)));
    form.ends_words = Some("sun 18:00".to_string());
    assert_eq!(form.meaning(Field::Ends), Some(at(fx::FRI + 2, 18, 0)), "both typed change both");
}

/// Backspace on a date that has not been typed into leaves no words, and
/// no words are no change.
#[test]
fn a_date_left_empty_keeps_what_it_held() {
    let mut form = fresh();
    to(&mut form, Field::Starts);
    assert!(form.edit_with(|typed| {
        typed.pop();
        true
    }));
    assert_eq!(text(&form.value(Field::Starts)[0]), "", "nothing is typed yet");
    assert_eq!(text(&form.left()), "starts Thu 26 Nov 2026 09:00", "and the status line says what it still is");
    assert!(saved(&form).is_ok(), "it can be saved as it is");
    form.step(true);
    assert_eq!((form.starts, form.starts_words.as_deref()), (fx::THU * DAY + 9 * HOUR, None));
    assert_eq!(text(&form.value(Field::Starts)[0]), "Thu 26 Nov 2026 09:00");
}

/// A time typed into an event that lasts all day says it does not; a
/// date typed there leaves it lasting all day.
#[test]
fn a_time_typed_into_an_all_day_event_ends_its_being_all_day() {
    let mut form = fresh();
    to(&mut form, Field::AllDay);
    form.choose(true);
    to(&mut form, Field::Starts);
    type_in(&mut form, "fri");
    assert_eq!(text(&form.left()), "starts Fri 27 Nov 2026");
    form.step(true);
    form.step(false);
    assert!(form.all_day, "a date alone moves the day");
    type_in(&mut form, "14:30");
    assert_eq!(text(&form.left()), "starts Fri 27 Nov 2026 14:30", "the status line says the time it will have");
    form.step(true);
    assert!(!form.all_day);
    assert_eq!(text(&form.value(Field::AllDay)[0]), "no");
    assert_eq!((form.starts, form.ends), (fx::FRI * DAY + 14 * HOUR + 30 * 60, fx::FRI * DAY + 15 * HOUR + 30 * 60));
}

#[test]
fn words_that_name_no_date_are_marked_and_refused() {
    let mut form = fresh();
    to(&mut form, Field::Starts);
    type_in(&mut form, "someday");
    assert_eq!(text(&form.left()), "not a date or a time");
    form.step(true);
    assert_eq!(form.starts_words.as_deref(), Some("someday"), "the words stay");
    let shown = &form.value(Field::Starts)[0][0];
    assert_eq!((shown.text.as_str(), shown.fg), ("someday", Tok::Err));
    assert_eq!(saved(&form), Err("Starts: not a date: \"someday\"".to_string()));
    form.step(false);
    form.edit_with(|typed| {
        typed.clear();
        typed.push_str("sat");
        true
    });
    to(&mut form, Field::Ends);
    type_in(&mut form, "+1x");
    assert_eq!(saved(&form), Err("Ends: not a date: \"+1x\"".to_string()));
}

#[test]
fn an_end_before_the_start_is_refused() {
    let mut form = fresh();
    to(&mut form, Field::Ends);
    type_in(&mut form, "08:00");
    assert_eq!(saved(&form), Err("it ends before it starts".to_string()));
    let mut form = fresh();
    to(&mut form, Field::AllDay);
    form.choose(true);
    to(&mut form, Field::Ends);
    type_in(&mut form, "25 nov");
    assert_eq!(saved(&form), Err("it ends before it starts".to_string()), "nor an all-day event's last day");
}

#[test]
fn an_all_day_event_is_whole_days_and_ends_at_the_midnight_after_its_last() {
    let mut form = fresh();
    to(&mut form, Field::AllDay);
    form.choose(true);
    assert_eq!(text(&form.value(Field::AllDay)[0]), "yes");
    assert_eq!(text(&form.value(Field::Starts)[0]), "Thu 26 Nov 2026");
    to(&mut form, Field::Ends);
    type_in(&mut form, "sat");
    let days = event(&form);
    assert_eq!((days.start, days.end), (Moment::Day(fx::THU), Some(Moment::Day(fx::THU + 3))));
    // Turned off again, it has the hours it had.
    to(&mut form, Field::AllDay);
    form.choose(true);
    assert_eq!(event(&form).end, Some(Moment::Utc((fx::THU + 2) * DAY + 10 * HOUR)));
}

#[test]
fn guests_are_addresses_by_comma_or_by_line_and_one_that_is_not_is_named() {
    let mut form = fresh();
    to(&mut form, Field::Guests);
    type_in(&mut form, "Imani Castell <imani@example.org>, lars");
    assert_eq!(saved(&form), Err("\"lars\" is not an address".to_string()));
    form.guests = "imani@example.org\nlars@example.org".to_string();
    let guests: Vec<String> = event(&form).guests.iter().map(|g| g.who.email.clone()).collect();
    assert_eq!(guests, ["imani@example.org", "lars@example.org"]);
    assert_eq!(form.value(Field::Guests).len(), 2, "a row to each");
}

#[test]
fn guests_are_mailed_from_the_account_by_whoever_organizes() {
    let mut form = fresh();
    to(&mut form, Field::Guests);
    type_in(&mut form, "imani@example.org");
    let saved = saved(&form).unwrap();
    assert!(saved.invite);
    let me = Address { name: Some("Robin Vale".to_string()), email: "robin@example.com".to_string() };
    assert_eq!(saved.event.organizer, Some(me), "personal's own address organizes");
    assert_eq!(saved.event.guests[0].said, Said::Nothing);
    // Robin is a guest of Imani's roadmap review, and a change to it is
    // Robin's alone.
    let (_, mut form) = editing("roadmap");
    type_in(&mut form, " (prep)");
    assert!(!self::saved(&form).unwrap().invite);
}

#[test]
fn an_edit_changes_only_what_was_changed() {
    let (was, form) = editing("roadmap");
    assert_eq!(form.heading, "Q4 roadmap review");
    let unchanged = saved(&form).unwrap();
    assert_eq!(unchanged.event, was, "a form saved as it opened is the event as it was");
    assert_eq!(unchanged.replaces, Some((target("work", false), was.uid.clone())));
    assert_eq!(unchanged.to, target("work", false), "the event's own calendar is offered though it is read only");

    let mut form = form;
    type_in(&mut form, " (moved)");
    to(&mut form, Field::Starts);
    type_in(&mut form, "11:00");
    to(&mut form, Field::Guests);
    type_in(&mut form, ", greta@example.net");
    let event = event(&form);
    let start = Moment::Wall { wall: fx::THU * DAY + 11 * HOUR, zone: None };
    let end = Moment::Wall { wall: fx::THU * DAY + 12 * HOUR + 30 * 60, zone: None };
    assert_eq!((event.start.clone(), event.end.clone()), (start, Some(end)), "moved, as long as it was");
    let new = Guest { who: Address { name: None, email: "greta@example.net".to_string() }, said: Said::Nothing };
    let guests = [was.guests.clone(), vec![new]].concat();
    assert_eq!(event.guests, guests, "each guest's answer is kept");
    let want = Event {
        summary: "Q4 roadmap review (moved)".to_string(),
        start: event.start.clone(),
        end: event.end.clone(),
        guests,
        ..was
    };
    assert_eq!(event, want, "UID, sequence, links, More and the rest are as they were");
}

#[test]
fn a_rule_of_a_choice_s_shape_is_that_choice_and_any_other_is_its_own() {
    let (was, mut form) = editing("standup");
    assert_eq!(text(&form.value(Field::Repeats)[0]), "every weekday");
    type_in(&mut form, "!");
    assert_eq!(event(&form).repeat, was.repeat, "its last day is kept");

    let (_, event) = design("standup");
    let rule = Rule { freq: Freq::Weekly, interval: 2, count: None, until: None, by_day: vec![], by_month_day: vec![] };
    let every_other = Some(Repeat::Rule(rule));
    let event = Event { repeat: every_other.clone(), ..event };
    let wall = event.start.wall();
    let form = Form::edit(targets(), target("work", false), &event, (wall, wall + 900, false), fx::THU - 3);
    assert_eq!(text(&form.value(Field::Repeats)[0]), "its own rule");
    assert_eq!(saved(&form).unwrap().event.repeat, every_other);
    let unread = Event { repeat: Some(Repeat::Unread("FREQ=HOURLY".to_string())), ..design("standup").1 };
    let form = Form::edit(targets(), target("work", false), &unread, (wall, wall + 900, false), fx::THU - 3);
    assert_eq!(text(&form.value(Field::Repeats)[0]), "its own rule");
}

#[test]
fn an_all_day_event_opens_on_its_days_and_its_reminder_as_it_was() {
    let (at, event) = design("cabin");
    let event = Event { alarms: vec![2 * HOUR, 0], ..event };
    let form = Form::edit(targets(), at, &event, (fx::FRI * DAY, (fx::FRI + 3) * DAY, true), fx::THU - 3);
    assert_eq!(text(&form.value(Field::Starts)[0]), "Fri 27 Nov 2026");
    assert_eq!(text(&form.value(Field::Ends)[0]), "Sun 29 Nov 2026");
    assert_eq!(text(&form.value(Field::Reminder)[0]), "2 hours before and 1 more");
    assert_eq!(saved(&form).unwrap().event, event);
}

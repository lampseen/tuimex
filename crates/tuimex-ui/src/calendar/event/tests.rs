use tuimex_core::calendar::{Event, Freq, Guest, Repeat, Rule, Said};
use tuimex_core::{AccountId, Address};
use tuimex_fixtures::design as fx;

use super::*;
use crate::calendar::Calendar;
use crate::calendar::parts::View;
use crate::calendar::tests::design;
use crate::list::Preview;

fn text(line: &Line) -> String {
    line.iter().map(|span| span.text.as_str()).collect::<String>().trim_end().to_string()
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(text).collect()
}

fn scene<'a>(calendar: &'a Calendar, accounts: &'a [AccountId]) -> Scene<'a> {
    Scene {
        calendar,
        view: View::Day,
        cursor: fx::THU,
        picked: None,
        focus: false,
        accounts,
        colours: &[Tok::Link, Tok::Head],
        words: Tok::Agent,
        narrowed: None,
        needle: None,
        search: None,
        preview: Preview::Auto,
        panel: Vec::new(),
        left: None,
        right: Line::new(),
        command: None,
    }
}

fn accounts() -> [AccountId; 2] {
    [AccountId("personal".to_string()), AccountId("work".to_string())]
}

/// The design's calendar with the roadmap review changed by `change`.
fn with_roadmap(change: impl Fn(Event) -> Event) -> Calendar {
    let mut calendar = design();
    for event in calendar.shelves.iter_mut().flat_map(|shelf| shelf.events.iter_mut()) {
        if event.uid.starts_with("roadmap@") {
            *event = change(event.clone());
        }
    }
    calendar
}

fn roadmap(calendar: &Calendar) -> Entry<'_> {
    calendar.days(fx::THU, fx::THU + 1, None).on(fx::THU)[1].clone()
}

#[test]
fn the_body_scrolls_no_further_than_its_last_row() {
    let (calendar, accounts) = (design(), accounts());
    let (o, entry) = (scene(&calendar, &accounts), roadmap(&calendar));
    let tall = screen(&o, &entry, At { top: 5, more: true }, 120, 40);
    assert_eq!((tall.top, tall.last_top), (0, 0), "all of it fits, so nothing scrolls");
    assert_eq!(tall.lines.len(), 40);

    let short = screen(&o, &entry, At { top: 999, more: true }, 120, 20);
    let rows = body(&entry, true, 120).len();
    assert_eq!(short.room, 20 - 10 - 3, "the header block's ten rows and the three under the body");
    assert_eq!(short.last_top, rows - short.room);
    assert_eq!(short.top, short.last_top, "a top past the end is kept to the end");
    assert_eq!(short.lines.len(), 20);
    assert_eq!(
        text(&short.lines[9 + short.room]),
        "      Changed  23 Nov 14:05, by the organizer",
        "the last row shows"
    );

    let from = screen(&o, &entry, At { top: 2, more: false }, 120, 20);
    assert_eq!(from.top, 2.min(from.last_top));
    let first = &body(&entry, false, 120)[from.top];
    assert_eq!(text(&from.lines[10]), text(&[vec![gap(INDENT)], first.clone()].concat()));
}

#[test]
fn a_long_guest_list_stops_after_its_rows_and_says_how_many_more() {
    let many = |event: Event| {
        let guest = |n: usize| Guest {
            who: Address { name: Some(format!("Guest Number {n}")), email: format!("guest{n}@example.com") },
            said: Said::Accepted,
        };
        Event { guests: [event.guests.clone(), (0..40).map(guest).collect()].concat(), ..event }
    };
    let (calendar, accounts) = (with_roadmap(many), accounts());
    let (o, entry) = (scene(&calendar, &accounts), roadmap(&calendar));
    let rows = texts(&fields(&o, &entry, 74));
    let who: Vec<&String> = rows.iter().filter(|row| row.starts_with("Who") || row.starts_with("         ")).collect();
    assert_eq!(who.len(), reader::ROWS.1, "{rows:#?}");
    assert!(who[who.len() - 1].ends_with(" more"), "{rows:#?}");
    assert!(who[who.len() - 1].contains(", and "), "{rows:#?}");
    let after = rows.iter().position(|row| row.starts_with("Answer")).unwrap_or(0);
    assert_eq!(rows[after], "Answer   not answered", "the fields after Who still follow it");
}

#[test]
fn an_event_with_nothing_but_a_time_has_its_when_and_its_calendar_only() {
    let bare = |event: Event| Event {
        location: None,
        description: None,
        organizer: None,
        guests: Vec::new(),
        links: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
        ..event
    };
    let (calendar, accounts) = (with_roadmap(bare), accounts());
    let (o, entry) = (scene(&calendar, &accounts), roadmap(&calendar));
    assert_eq!(
        texts(&fields(&o, &entry, 114)),
        ["When     Thu 26 Nov 2026, 10:00 to 11:30 · in 3 days", "Calendar work · its own calendar, read 20s ago"],
        "no Where, no Join, no Who, no Answer, no Reminder"
    );
    assert!(body(&entry, true, 120).is_empty(), "no notice, no notes and no More, folded or not");
    let drawn = texts(&screen(&o, &entry, At::default(), 120, 40).lines);
    assert!(drawn[0].starts_with("  Q4 roadmap review  ") && drawn[0].ends_with("  2 of 4 on Thu 26 Nov"));
    assert!(drawn[5..37].iter().all(String::is_empty), "{drawn:#?}");
    assert_eq!(drawn[39], "  E edit · d delete · Esc back", "nothing to answer, no link and no mail to open");
}

#[test]
fn notes_with_no_notice_follow_the_header_directly() {
    let answered = |event: Event| {
        let guests = event.guests.iter().map(|g| Guest { said: Said::Accepted, ..g.clone() }).collect();
        Event { guests, more: Vec::new(), ..event }
    };
    let (calendar, accounts) = (with_roadmap(answered), accounts());
    let entry = roadmap(&calendar);
    let rows = texts(&body(&entry, false, 120));
    assert!(rows[0].starts_with("The pre-read"), "{rows:#?}");
    assert_eq!(rows.last().map(String::as_str), Some("· Headcount, once the budget is in"), "and no More after them");
    let o = scene(&calendar, &accounts);
    let drawn = texts(&screen(&o, &entry, At::default(), 120, 40).lines);
    assert_eq!(drawn[39], "  o links · E edit · d delete · Esc back", "answered, so no i; the mail is not known");
}

#[test]
fn more_folded_names_its_fields_and_unfolded_lists_them() {
    let (calendar, accounts) = (design(), accounts());
    let entry = roadmap(&calendar);
    let folded = texts(&more(&entry, false, 114));
    assert_eq!(folded, ["", "▸ More · status, shown as, privacy, by phone, changed"]);
    let open = texts(&more(&entry, true, 114));
    assert_eq!(open[1], "▾ More");
    assert_eq!(open[2], "  Status   confirmed");
    assert_eq!(open.len(), 2 + 5);
    let o = scene(&calendar, &accounts);
    let local = &calendar.days(fx::THU, fx::THU + 1, None).on(fx::THU)[3].clone();
    let notice = texts(&body(local, false, 120));
    assert_eq!(notice, ["▲ Only in tuimex · work's own calendar does not have it · :export writes it out"]);
    assert_eq!(place(&o, local), "4 of 4 on Thu 26 Nov");
}

#[test]
fn the_fields_only_this_screen_shows_say_the_answer_the_repeat_and_the_reminders() {
    let weekly = |event: Event| Event {
        repeat: Some(Repeat::Rule(Rule {
            freq: Freq::Weekly,
            interval: 1,
            count: Some(10),
            until: None,
            by_day: vec![(0, 4)],
            by_month_day: Vec::new(),
        })),
        alarms: vec![600, 86_400],
        ..event
    };
    let (calendar, accounts) = (with_roadmap(weekly), accounts());
    let (o, entry) = (scene(&calendar, &accounts), roadmap(&calendar));
    let rows = texts(&fields(&o, &entry, 114));
    assert_eq!(
        rows[5..],
        ["Answer   not answered", "Repeats  weekly on Thu, 10 times", "Reminder 10 minutes before, 1 day before"]
    );
}

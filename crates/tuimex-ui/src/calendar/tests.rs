use tuimex_core::calendar::{DAY, Event, Moment, Standing, days_from_civil};
use tuimex_core::{AccountId, Timestamp};
use tuimex_fixtures::design as fx;

use super::parts::{self, Look, Scene, View};
use super::{Calendar, Proposed, Shelf, month};
use crate::list::Preview;
use crate::palette::Tok;
use crate::span::{Line, run};

/// The design's `CALENDARS` and `EVENTS`, as `tuimex --fixtures` hands
/// them over: read when the design's `read` says, at its `NOW`.
pub(crate) fn design() -> Calendar {
    let shelf = |kept: fx::Kept| Shelf {
        account: AccountId(kept.account.to_string()),
        local: kept.local,
        name: None,
        read: kept.read.map(|ago| Timestamp(fx::NOW.0 - i64::from(ago))),
        failed: None,
        events: kept.events,
    };
    let shelves = fx::calendars().into_iter().map(shelf).collect();
    Calendar { shelves, identities: fx::identities(), ..Calendar::empty(fx::NOW) }
}

fn account(name: &str) -> AccountId {
    AccountId(name.to_string())
}

fn titles(calendar: &Calendar, day: i64, only: Option<&AccountId>) -> Vec<String> {
    calendar.days(day, day + 1, only).on(day).iter().map(|entry| entry.event.summary.clone()).collect()
}

fn text(line: &Line) -> String {
    line.iter().map(|span| span.text.as_str()).collect::<String>().trim_end().to_string()
}

/// A scene over `calendar` with the cursor on `cursor`, as two accounts
/// and the built-in palette draw it.
fn scene<'a>(calendar: &'a Calendar, accounts: &'a [AccountId], cursor: i64) -> Scene<'a> {
    Scene {
        calendar,
        view: View::Month,
        cursor,
        picked: None,
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

fn both() -> [AccountId; 2] {
    [account("personal"), account("work")]
}

#[test]
fn a_day_lists_what_lasts_all_day_first_and_then_by_the_clock() {
    let calendar = design();
    assert_eq!(calendar.today(), days_from_civil(2026, 11, 23));
    let friday = ["Library books due", "Cabin weekend", "Account review", "Standup"];
    assert_eq!(titles(&calendar, fx::FRI, None), friday, "of two that begin together, the one that ends sooner");
    assert_eq!(titles(&calendar, fx::FRI + 2, None), ["Cabin weekend"], "its third and last day");
    assert_eq!(titles(&calendar, fx::FRI + 3, None), ["Standup"], "an end is the day after the last");
    let work = account("work");
    assert_eq!(titles(&calendar, fx::FRI, Some(&work)), ["Standup"]);
    let days = calendar.days(fx::THU, fx::THU + 3, None);
    assert_eq!(days.on(fx::THU).len() + days.on(fx::FRI).len() + days.on(fx::FRI + 1).len(), days.all().count());
    assert!(days.on(fx::THU - 1).is_empty() && days.on(fx::THU + 3).is_empty(), "a day outside the run has nothing");
}

#[test]
fn an_event_over_midnight_is_on_both_days_and_one_that_ends_at_midnight_on_one() {
    let mut calendar = design();
    let at = |day: i64, hour: i64| Moment::Wall { wall: day * DAY + hour * 3600, zone: None };
    let event = |title: &str, start: Moment, end: Moment| Event {
        summary: title.to_string(),
        uid: title.to_string(),
        start,
        end: Some(end),
        ..calendar.shelves[1].events[0].clone()
    };
    let (sat, sun) = (fx::FRI + 1, fx::FRI + 2);
    calendar.shelves[1].events =
        vec![event("Late", at(sat, 22), at(sun, 2)), event("Evening", at(sat, 20), at(sun, 0))];
    assert_eq!(titles(&calendar, sat, None), ["Cabin weekend", "Evening", "Late"]);
    assert_eq!(titles(&calendar, sun, None), ["Cabin weekend", "Late"]);
}

/// ADR-057: an answer is the receiving account's; ADR-055: local says so.
#[test]
fn an_entry_says_whose_it_is_where_it_is_kept_and_how_the_reader_stands_to_it() {
    let mut calendar = design();
    let days = calendar.days(fx::THU, fx::THU + 1, None);
    let said: Vec<(&str, bool, Option<Standing>, bool)> =
        days.on(fx::THU).iter().map(|e| (e.account.0.as_str(), e.local, e.standing, e.waiting())).collect();
    let want = [
        ("work", false, Some(Standing::Accepted), false),
        ("work", false, Some(Standing::Needs), true),
        ("work", false, Some(Standing::Tentative), false),
        ("work", true, Some(Standing::Accepted), false),
    ];
    assert_eq!(said, want);
    let roadmap = days.on(fx::THU)[1].clone();
    assert_eq!(calendar.find(&roadmap.pick(), None).as_ref(), Some(&roadmap), "a cursor finds its event again");
    assert!(roadmap.matches("fjord") && roadmap.matches("q4") && !roadmap.matches("standup"));
    assert!(
        titles(&calendar, fx::FRI, None).len() == 4
            && calendar.days(fx::FRI, fx::FRI + 1, None).on(fx::FRI)[0].standing.is_none()
    );

    let proposed =
        Proposed { account: account("work"), uid: roadmap.event.uid.clone(), start: fx::THU * DAY + 14 * 3600 };
    let (picked, review) = (roadmap.pick(), calendar.shelves[0].events[2].uid.clone());
    calendar.proposed.push(proposed);
    calendar.ignored.push((account("personal"), review));
    let found = calendar.find(&picked, None).unwrap();
    assert!(found.proposed.is_some() && !found.waiting(), "a time proposed is an answer of a kind");
    assert_eq!(
        titles(&calendar, fx::FRI, None),
        ["Library books due", "Cabin weekend", "Standup"],
        "ignored is not drawn"
    );
}

#[test]
fn a_hidden_calendar_is_on_no_day_and_the_panel_still_lists_it() {
    let mut calendar = design();
    let personal = account("personal");
    calendar.toggle(&personal, true);
    assert!(calendar.is_hidden(&personal, true) && !calendar.is_hidden(&personal, false));
    assert_eq!(titles(&calendar, fx::FRI, None), ["Cabin weekend", "Account review", "Standup"]);
    let listed: Vec<(&str, bool, usize, bool)> =
        calendar.sources().iter().map(|s| (s.account.0.as_str(), s.local, s.events, s.hidden)).collect();
    let want = [
        ("personal", false, 5, false),
        ("personal", true, 2, true),
        ("work", false, 7, false),
        ("work", true, 1, false),
    ];
    assert_eq!(listed, want);
    calendar.toggle(&personal, true);
    assert_eq!(titles(&calendar, fx::FRI, None).len(), 4);
}

#[test]
fn an_account_with_several_calendars_is_one_row_as_old_as_its_oldest_reading() {
    let mut calendar = design();
    let mut second = calendar.shelves[2].clone();
    (second.name, second.read, second.failed) =
        (Some("Holidays".to_string()), Some(Timestamp(fx::NOW.0 - 3 * 3600)), Some("offline".to_string()));
    second.events.truncate(2);
    calendar.shelves.push(second);
    let work = &calendar.sources()[2];
    assert_eq!((work.events, work.read), (9, Some(Timestamp(fx::NOW.0 - 3 * 3600))));
    let unread: Vec<(&str, Option<Timestamp>)> =
        calendar.unread().iter().map(|(a, read)| (a.0.as_str(), *read)).collect();
    assert_eq!(unread, [("work", Some(Timestamp(fx::NOW.0 - 3 * 3600)))]);
    let right = parts::alerts(
        &calendar,
        vec![run("1 not sent", Tok::Err)],
        Some(("Halden contract call", "10 min".to_string())),
        true,
    );
    let want = "▲ Halden contract call in 10 min · o for its link · 1 not sent · work's calendar not read · last read 3h \
                ago · reminders off";
    assert_eq!(text(&right), want);
    assert_eq!(text(&parts::alerts(&design(), Line::new(), None, false)), "", "what is normal says nothing");
}

#[test]
fn a_row_is_the_account_the_hours_the_title_and_what_is_not_normal() {
    let (calendar, accounts) = (design(), both());
    let o = scene(&calendar, &accounts, fx::THU);
    let days = o.days(fx::THU, fx::FRI + 1);
    let rows = |width: usize| -> Vec<String> {
        days.all().map(|entry| text(&parts::event_row(&o, entry, width, Look::default()))).collect()
    };
    let wide = rows(85);
    assert_eq!(wide[1], format!("    work       10:00-11:30  Q4 roadmap review{}needs answer", " ".repeat(26)));
    assert_eq!(
        wide[5], "    personal   all day      Cabin weekend  Ridge saddle hut",
        "a place, where there is no link"
    );
    let narrow = rows(45);
    assert_eq!(narrow[1], "    work  10:00    Q4 roadm…   needs answer", "the longest word is never the thing cut");
    assert_eq!(narrow[5], "    pers  all day  Cabin weekend  Ridge sa…");

    let selected = parts::event_row(&o, &days.on(fx::THU)[1], 85, Look { selected: true, faint: false });
    assert!(selected.iter().all(|span| span.bg == Some(Tok::Sel)) && selected[0].text == "▌");
    let title = |line: &Line| line.iter().find(|span| span.text.starts_with("Q4")).cloned().unwrap();
    assert!(title(&selected).bold, "it needs you");
    let faint = parts::event_row(&o, &days.on(fx::THU)[1], 85, Look { selected: false, faint: true });
    assert!(faint.iter().all(|span| span.text.trim().is_empty() || span.fg == Tok::Faint) && !title(&faint).bold);

    let one = [account("work")];
    let alone = scene(&calendar, &one, fx::THU);
    let row = parts::event_row(&alone, &days.on(fx::THU)[0], 85, Look::default());
    assert_eq!(text(&row), "    09:30-09:45  Standup", "one account has no column to say which");
}

#[test]
fn what_has_ended_is_dim_and_the_words_say_each_answer() {
    let (mut calendar, accounts) = (design(), both());
    let o = scene(&calendar, &accounts, calendar.today());
    let today = o.days(calendar.today(), calendar.today() + 1);
    let (standup, call) = (&today.on(calendar.today())[0], &today.on(calendar.today())[1]);
    let title = |entry: &super::Entry| {
        let row = parts::event_row(&o, entry, 85, Look::default());
        row.iter().find(|span| span.text == entry.event.summary).map(|span| span.fg)
    };
    assert_eq!(
        (title(standup), title(call)),
        (Some(Tok::Dim), Some(Tok::Fg)),
        "09:45 has passed at 14:20; 16:00 has not"
    );

    let roadmap = calendar.shelves[2].events[4].uid.clone();
    calendar.proposed.push(Proposed { account: account("work"), uid: roadmap, start: fx::THU * DAY + 14 * 3600 });
    calendar.shelves[3].events[0].guests[0].said = tuimex_core::calendar::Said::Declined;
    let days = calendar.days(fx::THU, fx::THU + 1, None);
    let words =
        |at: usize| parts::event_words(&days.on(fx::THU)[at]).into_iter().map(|(word, _)| word).collect::<Vec<_>>();
    assert_eq!(words(1), ["proposed Thu 14:00"]);
    assert_eq!(words(2), ["tentative"]);
    assert_eq!(words(3), ["declined", "local"]);
    let o = scene(&calendar, &accounts, fx::THU);
    let row = text(&parts::event_row(&o, &days.on(fx::THU)[3], 85, Look::default()));
    assert!(row.ends_with("Staging migration window               declined · local"), "{row}");
    let narrow = text(&parts::event_row(&o, &days.on(fx::THU)[3], 45, Look::default()));
    assert!(narrow.ends_with("declined"), "what does not fit whole is left out: {narrow}");
}

#[test]
fn a_list_of_days_names_the_nearest_passes_over_the_empty_and_ends_on_no_label() {
    let (calendar, accounts) = (design(), both());
    let today = calendar.today();
    let o = scene(&calendar, &accounts, today);
    let rows: Vec<String> = parts::agenda(&o, today, 12, 85, None).iter().map(text).collect();
    assert_eq!(rows[0], "  Today · Mon 23 November");
    assert_eq!(rows[4], "  Tomorrow · Tue 24 November");
    assert_eq!(rows[8], "  Wed 25 November");
    assert_eq!(rows[11], "", "Thursday's label would have no row under it");
    assert!(parts::agenda(&o, today, 12, 85, None).iter().all(|row| crate::span::cells(row) == 85));

    let rows: Vec<String> = parts::agenda(&o, today - 1, 6, 85, None).iter().map(text).collect();
    assert_eq!(rows[..4], ["  Sun 22 November", "    Nothing on this day.", "", "  Today · Mon 23 November"]);
    let found = Scene { needle: Some("gear"), search: Some("Gear"), ..o.clone() };
    let rows: Vec<String> = parts::agenda(&found, today - 1, 6, 85, None).iter().map(text).collect();
    assert_eq!(rows[0], "  Sat 5 December", "a search begins on the first day that has a match");
    assert!(rows[1].contains("Gear swap") && rows[2].is_empty());
    let finding = Scene { needle: Some("call"), ..o.clone() };
    let lines = parts::agenda(&finding, today, 6, 85, None);
    let (standup, call) = (&lines[1], &lines[2]);
    assert!(standup.iter().all(|span| span.text.trim().is_empty() || span.fg == Tok::Faint), "no match is faint");
    assert!(call.iter().any(|span| span.text == "call" && span.underline), "a match is underlined where it matches");
}

#[test]
fn the_month_marks_each_day_with_how_much_is_on_it_and_holds_the_months_that_fit_whole() {
    let (calendar, accounts) = (design(), both());
    let o = scene(&calendar, &accounts, fx::THU);
    let rows: Vec<String> = month::screen(&o, 120, 40).iter().map(text).collect();
    let grid = |row: &str| row.split('│').next().unwrap_or("").trim_end().to_string();
    assert_eq!(grid(&rows[2]), "     November 2026");
    assert_eq!(grid(&rows[8]), "     23: 24: 25:▌26⋮ 27⋮ 28∙ 29∙");
    let year = |row: &&String| grid(row).ends_with(" 2026") || grid(row).ends_with(" 2027");
    let named = |rows: &[String]| rows[2..].iter().filter(year).count();
    assert_eq!(named(&rows), 4, "November to February in 35 rows");
    let short: Vec<String> = month::screen(&o, 120, 24).iter().map(text).collect();
    assert_eq!(named(&short), 2, "and two in 19");
    let none: Vec<String> = month::screen(&o, 120, 9).iter().map(text).collect();
    assert_eq!(named(&none), 0, "a month that does not fit whole is not begun");
    assert!(none[2].contains("Thu 26 November"), "the list of days still is");
}

/// The design's `cal-event`, `cal-event-local` and `cal-day-wide`, where
/// these rows are the event's header block.
#[test]
fn an_event_s_fields_say_when_where_what_it_opens_which_calendar_and_who() {
    let (calendar, accounts) = (design(), both());
    let o = scene(&calendar, &accounts, fx::THU);
    let days = o.days(fx::THU, fx::FRI + 3);
    let fields = |entry: &super::Entry, brief: bool, width: usize| -> Vec<String> {
        parts::event_fields(&o, entry, brief, width).iter().map(text).collect()
    };
    let roadmap = [
        "When     Thu 26 Nov 2026, 10:00 to 11:30 · in 3 days",
        "Where    Room Fjord",
        "Join     meet.example.org https://meet.example.org/r/q4-roadmap",
        "Calendar work · its own calendar, read 20s ago",
        "Who      Imani Castell organizer, you no answer, Lars Lindqvist yes, Mei Tanaka no, Dmitri Volkov no answer",
    ];
    assert_eq!(fields(&days.on(fx::THU)[1], false, 114), roadmap);
    let beside = [
        "When     Thu 26 Nov 2026, 10:00 to 11:30 · in 3 days",
        "Where    Room Fjord",
        "Join     meet.example.org https://meet.example.org/r/q4-roadm…",
        "Calendar work · its own calendar, read 20s ago",
        "Who      Imani Castell organizer, you no answer,",
        "         Lars Lindqvist yes, Mei Tanaka no, and 1 more",
    ];
    assert_eq!(fields(&days.on(fx::THU)[1], true, 62), beside);
    let migration = [
        "When     Thu 26 Nov 2026, 20:00 to 23:00 · in 3 days",
        "Calendar work · local, only in tuimex",
        "Who      Lars Lindqvist organizer, you yes",
    ];
    assert_eq!(fields(&days.on(fx::THU)[3], false, 114), migration);
    let cabin = ["When     Fri 27 Nov to Sun 29 Nov 2026, all day · in 5 days", "Where    Ridge saddle hut"];
    assert_eq!(fields(&days.on(fx::FRI + 1)[0], true, 114)[..2], cabin, "its second day, counted from that day");
    let library = &days.on(fx::FRI)[0];
    assert_eq!(
        fields(library, true, 114),
        ["When     Fri 27 Nov 2026, all day · in 4 days", "Calendar personal · local, only in tuimex"]
    );
    let lying = fields(&days.on(fx::FRI)[2], true, 114);
    assert_eq!(lying[1], "Where    bank.example.com");
    assert_eq!(
        lying[2],
        "Join     bank.example.com.device-check.example.net https://bank.example.com.device-check.example.net/meet"
    );
    let rows = parts::event_fields(&o, &days.on(fx::THU)[1], false, 114);
    assert!(rows.iter().all(|row| crate::span::cells(row) == 114));
    let warned: String = rows[4].iter().filter(|span| span.fg == Tok::Warn).map(|span| span.text.as_str()).collect();
    assert_eq!(warned, "no answer", "your own missing answer is the one in the warning colour");
}

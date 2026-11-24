use tuimex_core::AccountId;
use tuimex_core::calendar::{DAY, Event, Moment};
use tuimex_fixtures::design as fx;

use super::{Lane, columns, explode, first_day, lanes, overlay, rows_of, screen, window};
use crate::calendar::parts::{Scene, View};
use crate::calendar::{Calendar, Picked, Shelf};
use crate::list::Preview;
use crate::palette::Tok;
use crate::span::{Line, cells, gap, run};

fn at(day: i64, hour: i64, minute: i64) -> Moment {
    Moment::Wall { wall: day * DAY + hour * 3600 + minute * 60, zone: None }
}

fn event(title: &str, start: Moment, end: Moment) -> Event {
    Event {
        uid: format!("{title}@week.example.net"),
        sequence: 0,
        instead_of: None,
        summary: title.to_string(),
        start,
        end: Some(end),
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

fn holding(events: Vec<Event>) -> Calendar {
    let shelf = Shelf { account: account(), local: false, name: None, read: None, failed: None, events };
    Calendar { shelves: vec![shelf], ..Calendar::empty(fx::NOW) }
}

fn account() -> AccountId {
    AccountId("work".to_string())
}

fn scene<'a>(calendar: &'a Calendar, accounts: &'a [AccountId], picked: Option<&'a Picked>) -> Scene<'a> {
    Scene {
        calendar,
        view: View::Week,
        cursor: fx::THU,
        picked,
        accounts,
        colours: &[Tok::Link],
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

fn text(line: &Line) -> String {
    line.iter().map(|span| span.text.as_str()).collect::<String>().trim_end().to_string()
}

/// The screen's rows as text, with the selected entry picked by title.
fn drawn(calendar: &Calendar, pick: Option<(&str, i64)>, width: usize, height: usize) -> Vec<String> {
    let picked = pick.and_then(|(title, day)| {
        let days = calendar.days(day, day + 1, None);
        days.on(day).iter().find(|entry| entry.event.summary == title).map(|entry| entry.pick())
    });
    let accounts = [account()];
    let o = scene(calendar, &accounts, picked.as_ref());
    screen(&o, width, height).iter().map(text).collect()
}

fn row_of(rows: &[String], label: &str) -> usize {
    rows.iter().position(|row| row.starts_with(&format!("  {label}"))).unwrap_or_else(|| panic!("no {label} row"))
}

#[test]
fn three_that_overlap_each_step_in_and_two_that_begin_together_halve_the_column() {
    let (placed, count) = lanes(&[(600, 690), (660, 720), (680, 750)]);
    let stepped = |lane| Lane { lane, shares: true, abreast: false };
    assert_eq!(placed, [stepped(0), stepped(1), stepped(2)]);
    assert_eq!(count, 3);

    let (placed, count) = lanes(&[(600, 660), (600, 690), (720, 750)]);
    let beside = |lane| Lane { lane, shares: true, abreast: true };
    assert_eq!(placed, [beside(0), beside(1), Lane { lane: 0, shares: false, abreast: false }]);
    assert_eq!(count, 2, "a lane is free again once what held it has ended");

    let calendar = holding(vec![
        event("One", at(fx::THU, 10, 0), at(fx::THU, 11, 30)),
        event("Two", at(fx::THU, 10, 30), at(fx::THU, 12, 0)),
        event("Three", at(fx::THU, 11, 0), at(fx::THU, 12, 30)),
    ]);
    let rows = drawn(&calendar, None, 120, 40);
    let (ten, (_, col)) = (row_of(&rows, "10:00"), columns(120));
    let thursday = 2 + 7 + 3 * col;
    let cell = |y: usize| rows[y].chars().skip(thursday).take(col).collect::<String>();
    assert_eq!(cell(ten).trim_end(), "│One");
    assert_eq!(cell(ten + 1).trim_end(), "│ │Two", "a blank cell parts it from the one it is drawn over");
    assert_eq!(cell(ten + 2).trim_end(), "│ │ │Three");
}

#[test]
fn an_event_over_midnight_is_drawn_on_each_day_for_its_part_of_it() {
    let late = event("Late", at(fx::THU, 22, 0), at(fx::FRI, 2, 0));
    let calendar = holding(vec![late]);
    let days = calendar.days(fx::THU, fx::THU + 2, None);
    assert_eq!(rows_of(&days.on(fx::THU)[0], fx::THU), (22 * 60, 24 * 60));
    assert_eq!(rows_of(&days.on(fx::FRI)[0], fx::FRI), (0, 2 * 60));

    let rows = drawn(&calendar, Some(("Late", fx::FRI)), 120, 40);
    let midnight = row_of(&rows, "00:00");
    assert_eq!(midnight, 3, "selected on its second day, the window begins at midnight");
    let friday = |y: usize| rows[y].chars().skip(2 + 7 + 4 * columns(120).1).collect::<String>();
    assert!(friday(midnight).starts_with("▌Late"), "{}", rows[midnight]);
    assert!(friday(midnight + 1).starts_with("▌22:00-02:00"), "{}", rows[midnight + 1]);
}

#[test]
fn the_window_moves_to_a_selected_event_before_it_or_after_it_and_stays_in_the_day() {
    assert_eq!(window(20, None), 8 * 60);
    assert_eq!(window(20, Some((8 * 60, 9 * 60))), 8 * 60, "one inside it moves nothing");
    assert_eq!(window(20, Some((6 * 60 + 30, 7 * 60))), 6 * 60, "back to the hour it begins in");
    assert_eq!(window(20, Some((20 * 60, 21 * 60 + 30))), 12 * 60, "on to the hour after it ends");
    assert_eq!(window(20, Some((23 * 60, 24 * 60))), 14 * 60);
    assert_eq!(window(60, None), 0, "a window longer than the day begins at midnight");

    let calendar = holding(vec![
        event("Early", at(fx::THU, 6, 30), at(fx::THU, 7, 0)),
        event("Late", at(fx::THU, 21, 0), at(fx::THU, 23, 30)),
    ]);
    assert!(drawn(&calendar, None, 120, 30)[3].starts_with("  08:00"));
    let early = drawn(&calendar, Some(("Early", fx::THU)), 120, 30);
    assert!(early[3].starts_with("  06:00") && early[4].contains("▌Early"), "{early:?}");
    let late = drawn(&calendar, Some(("Late", fx::THU)), 120, 30);
    let last = late.len() - 4;
    assert!(late[last - 1].starts_with("  23:00") && late[last - 1].contains('▌'), "{late:?}");
    assert!(late[last].is_empty(), "the window ends on the hour after it ends: {late:?}");
}

#[test]
fn a_week_with_nothing_all_day_has_no_all_day_rows() {
    let calendar = holding(vec![event("Standup", at(fx::THU, 9, 0), at(fx::THU, 9, 15))]);
    let rows = drawn(&calendar, None, 120, 30);
    assert!(rows.iter().all(|row| !row.contains("all day")));
    assert!(rows[2].contains("Mon 23") && rows[3].starts_with("  08:00"), "the clock follows the days");
    assert_eq!(rows.len(), 30);
}

#[test]
fn under_the_week_s_width_three_days_show_around_the_cursor_and_every_row_fits() {
    assert_eq!(columns(120), (7, 15));
    assert_eq!(columns(119), (3, 36));
    assert_eq!(columns(80), (3, 23));
    assert_eq!(first_day(fx::THU, 7), fx::THU - 3, "a week begins on Monday");
    assert_eq!(first_day(fx::THU, 3), fx::THU - 1);

    let calendar = crate::calendar::tests::design();
    let accounts = [account(), AccountId("personal".to_string())];
    for width in 80..=120 {
        let o = Scene { colours: &[Tok::Link, Tok::Head], ..scene(&calendar, &accounts, None) };
        let lines = screen(&o, width, 24);
        assert_eq!(lines.len(), 24, "at {width}");
        for line in &lines[2..lines.len() - 3] {
            assert_eq!(cells(line), width, "at {width}: {}", text(line));
        }
        let days = if width < 120 { "Wed 25" } else { "Mon 23" };
        assert!(text(&lines[2]).trim_start().starts_with(days), "at {width}");
    }
}

#[test]
fn a_block_drawn_over_half_a_wide_character_leaves_a_space_and_the_column_its_width() {
    let mut cell = explode(&[run("会議 notes", Tok::Fg)]);
    overlay(&mut cell, 1, &[gap(1), run("x", Tok::Fg)]);
    assert_eq!(text(&cell), "  x  notes");
    assert_eq!(cells(&cell), 10);
}

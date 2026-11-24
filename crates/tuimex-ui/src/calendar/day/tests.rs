use tuimex_core::AccountId;
use tuimex_fixtures::design as fx;

use super::screen;
use crate::calendar::parts::{Scene, View};
use crate::calendar::tests::design;
use crate::calendar::{Calendar, Picked};
use crate::list::Preview;
use crate::palette::Tok;
use crate::span::{Line, cells};

fn text(line: &Line) -> String {
    line.iter().map(|span| span.text.as_str()).collect::<String>().trim_end().to_string()
}

fn accounts() -> [AccountId; 2] {
    [AccountId("personal".to_string()), AccountId("work".to_string())]
}

fn scene<'a>(calendar: &'a Calendar, accounts: &'a [AccountId], cursor: i64, picked: Option<&'a Picked>) -> Scene<'a> {
    Scene {
        calendar,
        view: View::Day,
        cursor,
        picked,
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

/// The invitation on Thursday, which the design selects.
fn roadmap(calendar: &Calendar) -> Picked {
    calendar.days(fx::THU, fx::THU + 1, None).on(fx::THU)[1].pick()
}

/// Where the pane is on a screen of this size: under a divider, beside a
/// bar, or nowhere.
fn place(o: &Scene, width: usize, height: usize) -> &'static str {
    let lines: Vec<String> = screen(o, width, height).iter().map(text).collect();
    match () {
        _ if lines.iter().any(|line| line.starts_with('─')) => "below",
        _ if lines[2..lines.len() - 3].iter().all(|line| line.contains('│')) => "right",
        _ => "off",
    }
}

#[test]
fn a_day_with_nothing_on_it_says_so_and_selects_nothing() {
    let (calendar, accounts) = (design(), accounts());
    // The Sunday before today: nothing is on it, and the days after it
    // follow its label.
    let sunday = calendar.today() - 1;
    let o = scene(&calendar, &accounts, sunday, None);
    let lines: Vec<String> = screen(&o, 120, 40).iter().map(text).collect();
    assert_eq!(lines.len(), 40);
    assert!(lines[0].ends_with("Sun 22 November 2026"), "{}", lines[0]);
    assert_eq!(lines[2..6], ["  Sun 22 November", "    Nothing on this day.", "", "  Today · Mon 23 November"]);
    assert!(!lines.iter().any(|line| line.starts_with('─')), "no event, no pane");
    assert_eq!(lines[38], "  Sun 22 Nov · 0 events");
    assert!(lines[39].starts_with("  Enter open") && lines[39].contains("c new event"), "{}", lines[39]);
}

#[test]
fn the_list_never_ends_on_a_label() {
    let (calendar, accounts) = (design(), accounts());
    let picked = roadmap(&calendar);
    let o = scene(&calendar, &accounts, fx::THU, Some(&picked));
    for height in 12..=60 {
        for width in [80, 120, 180] {
            let lines = screen(&o, width, height);
            assert_eq!(lines.len(), height);
            let list: Vec<String> = lines[2..height - 3]
                .iter()
                .map(|line| text(line).split('│').next().unwrap_or("").trim_end().to_string())
                .take_while(|line| !line.starts_with('─'))
                .collect();
            let last = list.iter().rev().find(|line| !line.is_empty());
            let label = last.is_some_and(|line| line.starts_with("  ") && !line.starts_with("   "));
            assert!(!label, "{width}x{height} ends on {last:?}");
        }
    }
}

#[test]
fn the_pane_is_below_under_160_columns_and_beside_from_there() {
    let (calendar, accounts) = (design(), accounts());
    let picked = roadmap(&calendar);
    let o = scene(&calendar, &accounts, fx::THU, Some(&picked));
    assert_eq!(place(&o, 159, 40), "below");
    assert_eq!(place(&o, 160, 40), "right");
    assert_eq!(place(&o, 160, 29), "off", "too short a terminal has no pane");
    assert_eq!(place(&o, 120, 30), "below");
    let lines = screen(&o, 160, 40);
    assert!(lines[2..37].iter().all(|line| cells(line) == 160), "every row of the list and pane is the screen's width");
    let beside = text(&lines[2]);
    assert!(beside.ends_with("│  Q4 roadmap review"), "{beside}");

    let off = Scene { preview: Preview::Off, ..o.clone() };
    assert_eq!(place(&off, 180, 45), "off");
    let below = Scene { preview: Preview::Below, ..o.clone() };
    assert_eq!(place(&below, 180, 45), "below");
    let right = Scene { preview: Preview::Right, ..o.clone() };
    assert_eq!(place(&right, 120, 40), "right", "asked for, and the list still fits beside it");
    let none = scene(&calendar, &accounts, fx::THU, None);
    assert_eq!(place(&none, 180, 45), "off", "nothing selected, nothing shown");
}

#[test]
fn a_search_with_no_match_lists_nothing_and_counts_none() {
    let (calendar, accounts) = (design(), accounts());
    let today = calendar.today();
    let o = Scene { needle: Some("zzz"), search: Some("zzz"), ..scene(&calendar, &accounts, today, None) };
    let lines: Vec<String> = screen(&o, 120, 40).iter().map(text).collect();
    assert_eq!(lines[0], "  Month   Week   Day   Search zzz");
    assert!(lines[2..37].iter().all(String::is_empty), "not even the cursor's day is named");
    assert_eq!(lines[38], "  0 events match \"zzz\"");
    assert_eq!(lines[39], "  Enter open · n N next match · Esc back · ? help");

    let one = Scene { needle: Some("gear"), search: Some("Gear"), ..o.clone() };
    let lines: Vec<String> = screen(&one, 120, 40).iter().map(text).collect();
    assert_eq!(lines[2], "  Sat 5 December", "it begins on the first day that has a match");
    assert_eq!(lines[38], "  1 event matches \"Gear\"");

    // Standup repeats every weekday and is one event.
    let many = Scene { needle: Some("standup"), search: Some("standup"), ..o.clone() };
    let lines: Vec<String> = screen(&many, 120, 40).iter().map(text).collect();
    assert_eq!(lines[38], "  1 event matches \"standup\"");
}

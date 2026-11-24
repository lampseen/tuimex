use crate::app::App;
use crate::app::tests::{after, foot, press, screen};
use crate::calendar::parts::View;

/// Enter on Thursday, `j` to the roadmap review, and Enter on it.
const ROADMAP: &str = "gclll\rj\r";
const NEEDS: &str = "    ▲ Needs your answer · asked by Imani Castell";

fn opened(keys: &str) -> App {
    after(&format!("{ROADMAP}{keys}"))
}

/// The row that says what is folded under More, at 120 by 40.
fn more_row(app: &mut App) -> String {
    let rows = screen(app, 120, 40);
    rows.into_iter().find(|row| row.trim_start().starts_with(['▸', '▾'])).unwrap_or_default()
}

/// The first row of the body at 120 by 20, under the ten of the header.
fn first(app: &mut App) -> String {
    screen(app, 120, 20)[10].clone()
}

#[test]
fn zo_unfolds_more_and_zc_folds_it() {
    let mut app = opened("");
    assert_eq!(more_row(&mut app), "    ▸ More · status, shown as, privacy, by phone, changed");
    press(&mut app, "zo");
    assert_eq!(more_row(&mut app), "    ▾ More");
    assert!(screen(&mut app, 120, 40).contains(&"      By phone +47 21 00 00 00, code 4471".to_string()));
    press(&mut app, "zc");
    assert!(more_row(&mut app).starts_with("    ▸ More"));
}

#[test]
fn an_event_with_nothing_under_more_says_so() {
    // The first event on Thursday is the standup, which sends nothing more.
    assert_eq!(foot(&mut after("gclll\r\rzo")).1, "E zo: this event has nothing under More");
}

/// The body scrolls as the reader's does, and is kept to what the last
/// drawing could show however far a key asks.
#[test]
fn the_event_scrolls_within_its_body() {
    let mut app = opened("zo");
    assert_eq!(first(&mut app), NEEDS);
    press(&mut app, "j");
    assert_eq!(first(&mut app), "", "the blank row under the notice");
    press(&mut app, "G");
    let end = screen(&mut app, 120, 20);
    assert_eq!(end[16], "      Changed  23 Nov 14:05, by the organizer", "the last row at the bottom");
    press(&mut app, "jjj^d");
    assert_eq!(screen(&mut app, 120, 20), end, "nothing past the end");
    press(&mut app, "gg");
    assert_eq!(first(&mut app), NEEDS);
    press(&mut app, "k^u");
    assert_eq!(first(&mut app), NEEDS, "nothing before the top");
    press(&mut app, "^d");
    assert_ne!(first(&mut app), NEEDS, "half a page down");
}

#[test]
fn a_key_that_scrolls_says_when_all_of_the_event_is_already_in_sight() {
    let mut app = opened("");
    screen(&mut app, 120, 40);
    press(&mut app, "j");
    assert_eq!(foot(&mut app).1, "j: all of the event is on the screen");
}

/// J and K step to the events either side without leaving the screen,
/// and each is shown from its own top with More folded.
#[test]
fn j_and_k_step_through_events_and_each_starts_at_its_top() {
    let mut app = opened("zo");
    screen(&mut app, 120, 20);
    press(&mut app, "G");
    press(&mut app, "J");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[0].starts_with("  Vendor call") && rows[0].ends_with("3 of 4 on Thu 26 Nov"), "{}", rows[0]);
    press(&mut app, "K");
    let rows = screen(&mut app, 120, 20);
    assert!(rows[0].starts_with("  Q4 roadmap review"), "{}", rows[0]);
    assert_eq!(rows[10], NEEDS, "from the top");
    assert!(more_row(&mut app).starts_with("    ▸ More"), "and folded");
}

#[test]
fn esc_returns_to_the_day_it_was_opened_from() {
    let mut app = opened("");
    assert!(app.cal.event.open);
    press(&mut app, "\x1b");
    assert!(!app.cal.event.open);
    assert_eq!(app.cal.view, View::Day);
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 2 of 4 · 1 to answer", "with the event still chosen");
}

#[test]
fn v_says_the_mail_an_event_came_in_is_not_kept_yet() {
    let said = foot(&mut opened("v")).1;
    assert!(said.starts_with("E v: not built yet"), "{said}");
}

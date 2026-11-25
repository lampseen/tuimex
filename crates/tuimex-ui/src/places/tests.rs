use std::path::Path;

use super::{GAP, PLACES, Place, bar};
use crate::palette::Tok;
use crate::span::Line;

fn text(line: &Line) -> String {
    line.iter().map(|span| span.text.as_str()).collect::<String>().trim_end().to_string()
}

/// What is drawn as the place in view: bold, in the ordinary foreground.
fn in_view(line: &Line) -> String {
    line.iter().filter(|span| span.fg == Tok::Fg && span.bold).map(|span| span.text.as_str()).collect()
}

/// ADR-002: the places, their keys and their names are the design's.
#[test]
fn the_places_are_the_design_s() {
    let js = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/tui.js")).unwrap();
    let table = js.split("const LAYOUT = {").nth(1).unwrap().split("\n};").next().unwrap();
    assert!(table.contains(&format!("places: {{ gap: {GAP}, list: [")));
    let designed: Vec<&str> = table.lines().filter(|line| line.contains("{ place: '")).map(str::trim).collect();
    let ours: Vec<String> = PLACES
        .iter()
        .map(|p| {
            format!("{{ place: '{}', key: '{}', name: '{}', short: '{}' }},", p.place.name(), p.key, p.name, p.short)
        })
        .collect();
    assert_eq!(ours, designed);
}

#[test]
fn the_bar_names_each_place_by_its_function_key_and_marks_the_one_in_view() {
    let mail = bar(Place::Mail, true, 120);
    assert_eq!(text(&mail), "  F1 Mail   F2 Calendar");
    assert_eq!(in_view(&mail), "F1 Mail");
    let calendar = bar(Place::Calendar, true, 120);
    assert_eq!(text(&calendar), "  F1 Mail   F2 Calendar", "the row is the same in both places");
    assert_eq!(in_view(&calendar), "F2 Calendar");
    // The place not in view is dim, key and name together.
    let other: Vec<(&str, Tok, bool)> =
        mail.iter().filter(|s| s.text.contains("F2")).map(|s| (&*s.text, s.fg, s.bold)).collect();
    assert_eq!(other, [("F2 Calendar", Tok::Dim, false)]);
    assert!(mail.iter().all(|span| span.bg.is_none()), "nothing is filled, so sixteen colours lose nothing");
}

/// The tabs under the bar carry the screen's accent, and a second accented
/// row over them would read as one block with it.
#[test]
fn the_bar_draws_nothing_in_the_accent() {
    for here in [Place::Mail, Place::Calendar] {
        for keys in [true, false] {
            assert!(bar(here, keys, 120).iter().all(|span| span.fg != Tok::Acc));
        }
    }
}

/// Where letters are typed the function keys are not read, and a key the
/// bar named there would be a key that does nothing.
#[test]
fn the_bar_names_no_key_where_the_keys_are_not_read() {
    let typing = bar(Place::Calendar, false, 120);
    assert_eq!(text(&typing), "  Mail   Calendar");
    assert_eq!(in_view(&typing), "Calendar");
}

#[test]
fn a_narrow_bar_shortens_the_place_not_in_view_and_then_leaves_its_key() {
    assert_eq!(text(&bar(Place::Mail, true, 25)), "  F1 Mail   F2 Calendar");
    assert_eq!(text(&bar(Place::Mail, true, 24)), "  F1 Mail   F2 Cal");
    assert_eq!(text(&bar(Place::Mail, true, 20)), "  F1 Mail   F2 Cal");
    assert_eq!(text(&bar(Place::Mail, true, 19)), "  F1 Mail   F2");
    // The place in view keeps its name whatever the other is cut to.
    assert_eq!(text(&bar(Place::Calendar, true, 24)), "  F1   F2 Calendar");
    assert_eq!(in_view(&bar(Place::Calendar, true, 19)), "F2 Calendar");
    assert_eq!(text(&bar(Place::Mail, false, 19)), "  Mail   Calendar");
    assert_eq!(text(&bar(Place::Mail, false, 18)), "  Mail   Cal");
    for width in 0..40 {
        let cells: usize = bar(Place::Calendar, true, width).iter().map(|span| span.text.chars().count()).sum();
        assert_eq!(cells, width, "the bar is the width it is given");
    }
}

use std::path::Path;

use super::{GAP, PLACES, PROGRAM, Place, bar};
use crate::palette::Tok;
use crate::span::Line;

fn text(line: &Line) -> String {
    line.iter().map(|span| span.text.as_str()).collect::<String>().trim_end().to_string()
}

/// What is drawn as the place in view: the accent and bold, as a tab is.
fn in_view(line: &Line) -> String {
    line.iter().filter(|span| span.fg == Tok::Acc && span.bold).map(|span| span.text.as_str()).collect()
}

/// ADR-002: the places, their keys and their names are the design's.
#[test]
fn the_places_are_the_design_s() {
    let js = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/tui.js")).unwrap();
    let table = js.split("const LAYOUT = {").nth(1).unwrap().split("\n};").next().unwrap();
    assert!(table.contains(&format!("places: {{ program: '{PROGRAM}', gap: {GAP}, list: [")));
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
    assert_eq!(text(&mail), "  tuimex   F1 Mail   F2 Calendar");
    assert_eq!(in_view(&mail), "F1 Mail");
    let calendar = bar(Place::Calendar, true, 120);
    assert_eq!(text(&calendar), "  tuimex   F1 Mail   F2 Calendar", "the row is the same in both places");
    assert_eq!(in_view(&calendar), "F2 Calendar");
    // The key of the place not in view is the colour a hint's key is.
    let other: Vec<(&str, Tok)> = mail
        .iter()
        .filter(|s| s.text.contains("F2") || s.text.contains("Calendar"))
        .map(|s| (&*s.text, s.fg))
        .collect();
    assert_eq!(other, [("F2", Tok::Fg), (" Calendar", Tok::Dim)]);
    assert!(mail.iter().all(|span| span.bg.is_none()), "nothing is filled, so sixteen colours lose nothing");
}

/// Where letters are typed the function keys are not read, and a key the
/// bar named there would be a key that does nothing.
#[test]
fn the_bar_names_no_key_where_the_keys_are_not_read() {
    let typing = bar(Place::Calendar, false, 120);
    assert_eq!(text(&typing), "  tuimex   Mail   Calendar");
    assert_eq!(in_view(&typing), "Calendar");
}

#[test]
fn a_narrow_bar_shortens_the_place_not_in_view_and_then_leaves_its_key() {
    assert_eq!(text(&bar(Place::Mail, true, 34)), "  tuimex   F1 Mail   F2 Calendar");
    assert_eq!(text(&bar(Place::Mail, true, 33)), "  tuimex   F1 Mail   F2 Cal");
    assert_eq!(text(&bar(Place::Mail, true, 29)), "  tuimex   F1 Mail   F2 Cal");
    assert_eq!(text(&bar(Place::Mail, true, 28)), "  tuimex   F1 Mail   F2");
    // The place in view keeps its name whatever the other is cut to.
    assert_eq!(text(&bar(Place::Calendar, true, 33)), "  tuimex   F1   F2 Calendar");
    assert_eq!(in_view(&bar(Place::Calendar, true, 28)), "F2 Calendar");
    assert_eq!(text(&bar(Place::Mail, false, 24)), "  tuimex   Mail   Cal");
    for width in 0..40 {
        let cells: usize = bar(Place::Calendar, true, width).iter().map(|span| span.text.chars().count()).sum();
        assert_eq!(cells, width, "the bar is the width it is given");
    }
}

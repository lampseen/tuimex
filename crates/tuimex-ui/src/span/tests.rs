use super::{Span, cells, fill, fit, gap, highlight, left_right, markup, run, wrap};
use crate::palette::Tok;

fn text(line: &[Span]) -> String {
    line.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn fit_pads_a_short_line_and_leaves_an_exact_one() {
    assert_eq!(fit(&[run("ab", Tok::Dim)], 4), vec![run("ab", Tok::Dim), gap(2)]);
    assert_eq!(fit(&[run("abcd", Tok::Dim)], 4), vec![run("abcd", Tok::Dim)]);
}

#[test]
fn fit_cuts_with_an_ellipsis_in_the_style_of_what_it_replaces() {
    let line = [run("abc", Tok::Dim), run("defgh", Tok::Err).bold()];
    assert_eq!(fit(&line, 6), vec![run("abc", Tok::Dim), run("de", Tok::Err).bold(), run("…", Tok::Err).bold()]);
    assert_eq!(fit(&line, 4), vec![run("abc", Tok::Dim), run("…", Tok::Dim)]);
    assert_eq!(fit(&line, 1), vec![run("…", Tok::Fg)]);
    assert_eq!(fit(&line, 0), vec![]);
}

#[test]
fn the_ellipsis_is_never_underlined() {
    let hit = Span { underline: true, ..run("match", Tok::Fg) };
    assert!(!fit(&[hit], 3).last().unwrap().underline);
}

#[test]
fn a_width_is_cells_not_characters() {
    let wide = [run("日本語のメール", Tok::Fg)];
    assert_eq!(cells(&wide), 14);
    let cut = fit(&wide, 6);
    assert_eq!(text(&cut), "日本… ");
    assert_eq!(cells(&cut), 6);
    assert_eq!(cells(&[run("a\u{1b}b", Tok::Fg)]), 2, "an escape character takes no cell");
}

#[test]
fn left_right_cuts_the_left_when_they_would_meet() {
    let line = left_right(&[run("a long left side", Tok::Fg)], &[run("right", Tok::Dim)], 12);
    assert_eq!(text(&line), "a long…right");
    assert_eq!(text(&left_right(&[run("left", Tok::Fg)], &[run("right", Tok::Dim)], 12)), "left   right");
    assert_eq!(text(&left_right(&[run("left", Tok::Fg)], &[run("right", Tok::Dim)], 3)), "right");
}

#[test]
fn fill_keeps_a_background_a_span_already_has() {
    let line = fill(vec![run("a", Tok::Fg), run(" ", Tok::Bg).on(Tok::Acc)], Tok::Sel);
    assert_eq!(line, vec![run("a", Tok::Fg).on(Tok::Sel), run(" ", Tok::Bg).on(Tok::Acc)]);
}

#[test]
fn markup_reads_a_token_and_its_flags() {
    assert_eq!(
        markup("{ok|+} Done{dim| - u to undo} {fg.b|now} {warn.u|x}"),
        vec![
            run("+", Tok::Ok),
            run(" Done", Tok::Fg),
            run(" - u to undo", Tok::Dim),
            run(" ", Tok::Fg),
            run("now", Tok::Fg).bold(),
            run(" ", Tok::Fg),
            Span { underline: true, ..run("x", Tok::Warn) },
        ]
    );
}

#[test]
fn a_brace_that_opens_no_tag_is_text() {
    assert_eq!(markup("a {b} {C|d} {e|f"), vec![run("a {b} {C|d} {e|f", Tok::Fg)]);
    assert_eq!(markup(""), vec![]);
}

#[test]
fn wrap_never_splits_a_word() {
    assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
    assert_eq!(wrap("incomprehensibilities at last", 5), ["incomprehensibilities", "at", "last"]);
    assert_eq!(wrap("", 5), [""; 0]);
}

#[test]
fn highlight_marks_every_occurrence_whatever_its_case() {
    let hit = |t: &str| Span { underline: true, ..run(t, Tok::Fg).bold() };
    assert_eq!(
        highlight("Nov and nov, NOV", Some("nov"), run("", Tok::Dim)),
        vec![hit("Nov"), run(" and ", Tok::Dim), hit("nov"), run(", ", Tok::Dim), hit("NOV")]
    );
    assert_eq!(highlight("November", Some("NOV"), run("", Tok::Dim).bold())[1], run("ember", Tok::Dim).bold());
    assert_eq!(highlight("none here", Some("nov"), run("", Tok::Dim)), vec![run("none here", Tok::Dim)]);
    assert_eq!(highlight("no", Some("nov"), run("", Tok::Dim)), vec![run("no", Tok::Dim)]);
    assert_eq!(highlight("plain", None, run("", Tok::Fg)), vec![run("plain", Tok::Fg)]);
}

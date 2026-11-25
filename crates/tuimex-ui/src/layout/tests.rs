use std::path::Path;

use super::*;

/// ADR-002: the numbers are the design's, read from its `LAYOUT`.
#[test]
fn the_numbers_are_the_design_s_layout() {
    let js = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/tui.js")).unwrap();
    let table = js.split("const LAYOUT = {").nth(1).unwrap().split("\n};").next().unwrap();
    let has = |line: String| assert!(table.contains(&line), "the design's LAYOUT has no `{line}`");
    has(format!("margin: {MARGIN},"));
    has(format!("indent: {INDENT},"));
    has(format!(
        "row: {{ account: {{ most: {ACCOUNT_MOST}, gap: {ACCOUNT_GAP}, short: {ACCOUNT_SHORT}, shortGap: \
         {ACCOUNT_SHORT_GAP} }}, from: {FROM}, fromGap: {FROM_GAP}, wordsGap: {WORDS_GAP}, words: {WORDS}, date: \
         {DATE}, dateGap: {DATE_GAP} }},"
    ));
    has(format!("wordForms: {{ full: {}, mid: {}, tight: {} }},", WORD_FORMS[0], WORD_FORMS[1], WORD_FORMS[2]));
    has(format!("measure: {MEASURE},"));
    has(format!(
        "preview: {{ besideFrom: {PREVIEW_BESIDE_FROM}, minHeight: {PREVIEW_MIN_HEIGHT}, share: 0.{PREVIEW_SHARE}, \
         least: {PREVIEW_LEAST}, most: {PREVIEW_MOST}, height: (rows) => Math.floor(rows * 0.45) }},"
    ));
    has(format!(
        "calendar: {{ cell: {CAL_CELL}, weekFrom: {CAL_WEEK_FROM}, slot: {CAL_SLOT}, step: {CAL_STEP}, dayFrom: \
         '{CAL_DAY_FROM}', axis: {CAL_AXIS}, time: {CAL_TIME}, timeNarrow: {CAL_TIME_NARROW}, words: {CAL_WORDS}, \
         wordsNarrow: {CAL_WORDS_NARROW}, narrow: {CAL_NARROW} }},"
    ));
    let steps: Vec<String> = table
        .lines()
        .filter_map(|line| {
            let column = line.split("{ column: '").nth(1)?.split('\'').next()?;
            let to = line.split("to: ").nth(1)?.split(',').next()?;
            let under = line.split("under: ").nth(1)?.split(',').next()?;
            Some(format!("{column} {to} {under}"))
        })
        .collect();
    let ours: Vec<String> = COLLAPSE
        .iter()
        .map(|(cut, under)| match cut {
            Cut::Words(to) => format!("words {to} {under}"),
            Cut::From(to) => format!("from {to} {under}"),
            Cut::AccountShort => format!("account 'short' {under}"),
            Cut::Date(to) => format!("date {to} {under}"),
            Cut::Account(to) => format!("account {to} {under}"),
        })
        .collect();
    assert_eq!(ours, steps);
    let list = |tokens: &[Tok]| tokens.iter().map(|t| format!("'{}'", t.name())).collect::<Vec<_>>().join(", ");
    has(format!(
        "accounts: {{ builtin: [{}], pool: [{}], last: [{}], never: ['acc', 'err', 'agent'], beside: [{}], floor: \
         {ACCOUNTS_FLOOR}, margin: {ACCOUNTS_MARGIN} }},",
        list(&ACCOUNTS_BUILTIN),
        list(&ACCOUNTS_POOL),
        list(&ACCOUNTS_LAST),
        list(&ACCOUNTS_BESIDE)
    ));
    has(format!(
        "words: {{ builtin: '{}', pool: [{}], never: ['acc', 'err', 'warn'], beside: [{}], floor: {WORDS_FLOOR}, \
         onSel: {WORDS_ON_SEL}, bright: ['{}', '{}'] }},",
        WORDS_BUILTIN.name(),
        list(&WORDS_POOL),
        list(&WORDS_BESIDE),
        WORDS_BRIGHT[0],
        WORDS_BRIGHT[1]
    ));
}

#[test]
fn the_preview_below_takes_45_percent_of_the_rows_rounded_down() {
    assert_eq!(preview_height(40), 18);
    assert_eq!(preview_height(45), 20);
    assert_eq!(preview_height(24), 10);
}

#[test]
fn the_preview_beside_takes_its_share_of_the_width_within_its_bounds() {
    assert_eq!([160, 180, 200, 240].map(preview_width), [62, 68, 76, 82]);
}

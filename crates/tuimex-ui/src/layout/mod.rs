//! The design's `LAYOUT`: the numbers every screen is laid out by. A test
//! reads that table to say these are its values.

use crate::palette::Tok;

/// Headings start at `MARGIN` and what they head at `INDENT`, with the
/// cursor and the mark in the cells before it.
pub const MARGIN: usize = 2;
pub const INDENT: usize = 4;

/// A list row's columns after the indent, each parted from the next by a
/// gap: the account, as wide as the longest account name up to
/// `ACCOUNT_MOST` letters, the sender, the subject, the words about the row
/// and the date. The subject takes what they leave.
pub const ACCOUNT_MOST: usize = 12;
pub const ACCOUNT_GAP: usize = 3;
pub const ACCOUNT_SHORT: usize = 4;
pub const ACCOUNT_SHORT_GAP: usize = 2;
pub const FROM: usize = 20;
pub const FROM_GAP: usize = 2;
pub const WORDS_GAP: usize = 3;
pub const WORDS: usize = 22;
pub const DATE: usize = 6;
pub const DATE_GAP: usize = 2;

/// The narrowest column each form of the words is written in: `4 messages
/// · 3 files`, `4 msg · 3 files`, `4m 3f`.
pub const WORD_FORMS: [usize; 3] = [22, 15, 9];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cut {
    Words(usize),
    From(usize),
    AccountShort,
    Date(usize),
    Account(usize),
}

/// Applied in order, each while the subject is narrower than its second
/// number: the words give way first, so the subject is cut last.
pub const COLLAPSE: [(Cut, usize); 7] = [
    (Cut::Words(15), 48),
    (Cut::Words(9), 36),
    (Cut::From(16), 30),
    (Cut::AccountShort, 30),
    (Cut::Words(0), 24),
    (Cut::Date(0), 24),
    (Cut::Account(0), 24),
];

/// The accents accounts take, in the order of the config, with the
/// built-in palette.
pub const ACCOUNTS_BUILTIN: [Tok; 4] = [Tok::Link, Tok::Head, Tok::Ok, Tok::Warn];
/// With a loaded palette the order is worked out from its colours
/// (`palette::account_order`): the accents it is chosen among, those that
/// take a step only when the best of the rest is under `ACCOUNTS_FLOOR`,
/// and the neutrals an account name sits beside. One token displaces
/// another only by more than `ACCOUNTS_MARGIN`.
pub const ACCOUNTS_POOL: [Tok; 3] = [Tok::Link, Tok::Head, Tok::Ok];
pub const ACCOUNTS_LAST: [Tok; 1] = [Tok::Warn];
pub const ACCOUNTS_BESIDE: [Tok; 2] = [Tok::Fg, Tok::Dim];
pub const ACCOUNTS_FLOOR: f64 = 6.0;
pub const ACCOUNTS_MARGIN: f64 = 1.0;

/// The words of a row with the built-in palette. With a loaded one they
/// take the accent of `WORDS_POOL` no configured account wears that is
/// best separated from `WORDS_BESIDE` and the account colours, among those
/// with a contrast of `WORDS_ON_SEL` to one on the selected row's fill;
/// when the best is under `WORDS_FLOOR` they fall back to brightness, the
/// numbers and the `WORDS_BRIGHT` words in `fg` and the rest in `dim`.
pub const WORDS_BUILTIN: Tok = Tok::Agent;
pub const WORDS_POOL: [Tok; 4] = [Tok::Agent, Tok::Link, Tok::Head, Tok::Ok];
pub const WORDS_BESIDE: [Tok; 3] = [Tok::Fg, Tok::Dim, Tok::Warn];
pub const WORDS_FLOOR: f64 = 6.0;
pub const WORDS_ON_SEL: f64 = 4.5;
pub const WORDS_BRIGHT: [&str; 2] = ["invite", "replied"];

/// The preview is beside the list from this many columns and below it
/// otherwise, and hidden in a terminal shorter than `PREVIEW_MIN_HEIGHT`.
pub const PREVIEW_BESIDE_FROM: usize = 160;
pub const PREVIEW_MIN_HEIGHT: usize = 30;
/// Beside the list it takes this share of the width, in hundredths, within
/// these bounds, so the list gains from every column past full measure.
pub const PREVIEW_SHARE: usize = 38;
pub const PREVIEW_LEAST: usize = 62;
pub const PREVIEW_MOST: usize = 82;

pub fn preview_width(width: usize) -> usize {
    (width * PREVIEW_SHARE / 100).clamp(PREVIEW_LEAST, PREVIEW_MOST)
}

/// Its rows when it is below the list, the divider included: 45% of the
/// terminal's.
pub fn preview_height(rows: usize) -> usize {
    rows * 45 / 100
}

/// Body text wraps here however wide the terminal is.
pub const MEASURE: usize = 76;

#[cfg(test)]
mod tests;

//! Sections and the query grammar's terms (PLAN §6.1).

use crate::Timestamp;

/// A saved query shown as a tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub short: String,
    pub query: Query,
    pub sort: Sort,
    pub threaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    NewestFirst,
    OldestFirst,
}

/// Every term must match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub terms: Vec<Term>,
}

impl Query {
    /// Never fails: a word that is not a term the grammar knows, or whose
    /// value does not fit its term, is free text (PLAN §6.1). Double quotes
    /// keep spaces inside one word, as in `subject:"board minutes"`.
    pub fn parse(input: &str) -> Query {
        let terms = words(input)
            .iter()
            .filter_map(|word| {
                let keyed = word.split_once(':').filter(|(key, _)| !key.contains('"'));
                let term = keyed.and_then(|(key, value)| term(key, &value.replace('"', "")));
                term.or_else(|| Some(word.replace('"', "")).filter(|t| !t.is_empty()).map(Term::Text))
            })
            .collect();
        Query { terms }
    }
}

/// Splits on whitespace that is outside double quotes, keeping the quotes
/// so the caller can tell `"from:x"` from `from:x`.
fn words(input: &str) -> Vec<String> {
    let mut words = vec![String::new()];
    let mut quoted = false;
    for c in input.chars() {
        quoted ^= c == '"';
        match words.last_mut() {
            Some(word) if quoted || !c.is_whitespace() => word.push(c),
            _ => words.push(String::new()),
        }
    }
    words.retain(|w| !w.is_empty());
    words
}

fn term(key: &str, value: &str) -> Option<Term> {
    if value.is_empty() {
        return None;
    }
    let text = || value.to_string();
    Some(match key.to_ascii_lowercase().as_str() {
        "in" => Term::In(text()),
        "account" => Term::Account(text()),
        "is" => Term::Is(match value.to_ascii_lowercase().as_str() {
            "unread" => Is::Unread,
            "read" => Is::Read,
            "flagged" => Is::Flagged,
            "answered" => Is::Answered,
            "draft" => Is::Draft,
            "signed" => Is::Signed,
            "encrypted" => Is::Encrypted,
            "agent-draft" => Is::AgentDraft,
            _ => return None,
        }),
        "has" => Term::Has(match value.to_ascii_lowercase().as_str() {
            "attachment" => Has::Attachment,
            "invite" => Has::Invite,
            "unsubscribe" => Has::Unsubscribe,
            _ => return None,
        }),
        "from" => Term::From(text()),
        "to" => Term::To(text()),
        "cc" => Term::Cc(text()),
        "subject" => Term::Subject(text()),
        "after" => Term::After(when(value)?),
        "before" => Term::Before(when(value)?),
        "larger" => Term::Larger(size(value)?),
        _ => return None,
    })
}

/// `7d`, or `2026-11-23` meaning that day's first second in UTC.
fn when(value: &str) -> Option<When> {
    if let Some(days) = value.strip_suffix('d') {
        return days.parse().ok().map(When::DaysAgo);
    }
    let mut parts = value.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days_in_month = match m {
        2 => 28 + i64::from(leap),
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => return None,
    };
    if !(1..=9999).contains(&y) || !(1..=days_in_month).contains(&d) {
        return None;
    }
    // Days since the epoch in the proleptic Gregorian calendar, counting
    // the year from March so the leap day is the last of its year.
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let days = y * 365 + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + d - 1 - 719_468;
    Some(When::At(Timestamp(days * 86_400)))
}

/// Bytes, or a count with `k`, `m` or `g` (optionally `b` after it), in
/// powers of 1024 as mutt's size patterns are.
fn size(value: &str) -> Option<u64> {
    let lower = value.to_ascii_lowercase();
    let lower = lower.strip_suffix('b').filter(|rest| rest.ends_with(['k', 'm', 'g'])).unwrap_or(&lower);
    let (digits, shift) = match lower.as_bytes().last()? {
        b'k' => (&lower[..lower.len() - 1], 10),
        b'm' => (&lower[..lower.len() - 1], 20),
        b'g' => (&lower[..lower.len() - 1], 30),
        _ => (lower, 0),
    };
    digits.parse::<u64>().ok()?.checked_mul(1 << shift)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// A role's name or a folder's.
    In(String),
    Account(String),
    Is(Is),
    Has(Has),
    From(String),
    To(String),
    Cc(String),
    Subject(String),
    After(When),
    Before(When),
    /// Bytes: a message this size or smaller does not match.
    Larger(u64),
    /// Free text over subject, sender and body; an unknown term lands here.
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Is {
    Unread,
    Read,
    Flagged,
    Answered,
    Draft,
    Signed,
    Encrypted,
    AgentDraft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Has {
    Attachment,
    Invite,
    Unsubscribe,
}

/// `after:` and `before:` take a date or an age such as `7d`. An age stays
/// an age so a saved section means "the last week" every day it is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    At(Timestamp),
    DaysAgo(u32),
}

#[cfg(test)]
mod tests;

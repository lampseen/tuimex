//! How an event repeats and when it reminds, in the words its screen
//! writes them in. The wording is the reader's card's (`recurrence` in
//! `tuimex-content`) where the two say the same thing, and the design's
//! where it has its own: five weekdays a week are "every weekday".

use super::{DAY, Freq, Repeat, Rule, civil_from_days};

#[cfg(test)]
mod tests;

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const NTH: [&str; 5] = ["first", "second", "third", "fourth", "fifth"];

/// `first`, `second`, `last`, `second to last`: a position in a month,
/// counted from its end when it is negative.
fn nth(n: i64) -> String {
    let named = |n: i64| usize::try_from(n - 1).ok().and_then(|at| NTH.get(at)).map(|s| s.to_string());
    match n {
        -1 => "last".to_string(),
        n if n < 0 => format!("{} to last", named(-n).unwrap_or_else(|| ordinal(-n))),
        n => named(n).unwrap_or_else(|| ordinal(n)),
    }
}

/// `1st`, `2nd`, `11th`, `23rd`.
fn ordinal(n: i64) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn weekday(day: i64) -> &'static str {
    usize::try_from(day.rem_euclid(7)).ok().and_then(|at| DAYS.get(at)).copied().unwrap_or("")
}

/// Monday to Friday, each once and with no position: the design's
/// `every weekday`.
fn weekdays(rule: &Rule) -> bool {
    let mut days: Vec<i64> = rule.by_day.iter().filter(|(n, _)| *n == 0).map(|(_, day)| *day).collect();
    days.sort_unstable();
    days.dedup();
    days == [1, 2, 3, 4, 5] && days.len() == rule.by_day.len() && rule.by_month_day.is_empty()
}

/// The days a monthly or yearly rule names in its month: `the second
/// Tue`, `the 15th`, `the last day`, `every Mon`.
fn in_month(rule: &Rule) -> Vec<String> {
    let days = rule.by_month_day.iter().map(|&dom| match dom {
        -1 => "the last day".to_string(),
        dom if dom < 0 => format!("the {} day", nth(dom)),
        dom => format!("the {}", ordinal(dom)),
    });
    let weekdays = rule.by_day.iter().map(|&(n, day)| match n {
        0 => format!("every {}", weekday(day)),
        n => format!("the {} {}", nth(n), weekday(day)),
    });
    days.chain(weekdays).collect()
}

/// A rule in words: `every weekday`, `weekly on Tue`, `every 2 weeks`,
/// `monthly on the second Tue`, `monthly on the last day`, `yearly`, each
/// with `, 10 times` or `, until 31 Dec 2026` when it ends.
pub fn rule_words(rule: &Rule) -> String {
    let every = rule.interval.max(1);
    let (unit, often) = match rule.freq {
        Freq::Daily => ("day", "daily"),
        Freq::Weekly => ("week", "weekly"),
        Freq::Monthly => ("month", "monthly"),
        Freq::Yearly => ("year", "yearly"),
    };
    let named = match rule.freq {
        Freq::Daily | Freq::Weekly => {
            let mut days: Vec<&str> = rule.by_day.iter().map(|(_, day)| weekday(*day)).collect();
            days.dedup();
            days.join(", ")
        }
        Freq::Monthly | Freq::Yearly => in_month(rule).join(", "),
    };
    let mut words = match (every, named.is_empty()) {
        _ if every == 1 && matches!(rule.freq, Freq::Daily | Freq::Weekly) && weekdays(rule) => {
            "every weekday".to_string()
        }
        (1, true) => often.to_string(),
        (1, false) => format!("{often} on {named}"),
        (n, true) => format!("every {n} {unit}s"),
        (n, false) => format!("every {n} {unit}s on {named}"),
    };
    match rule.count {
        Some(1) => words.push_str(", once"),
        Some(n) => words.push_str(&format!(", {n} times")),
        None => {}
    }
    if let Some(until) = rule.until {
        let (year, month, dom) = civil_from_days(until.div_euclid(DAY));
        let month = usize::try_from(month - 1).ok().and_then(|at| MONTHS.get(at)).copied().unwrap_or("");
        words.push_str(&format!(", until {dom} {month} {year}"));
    }
    words
}

/// How an event repeats, in words. A rule tuimex does not work out is
/// shown on the event's first day alone (ADR-060), and this says why.
pub fn repeat_words(repeat: &Repeat) -> String {
    match repeat {
        Repeat::Rule(rule) => rule_words(rule),
        Repeat::Unread(_) => "repeats in a way tuimex does not work out".to_string(),
    }
}

/// A reminder `before` seconds ahead of the start, in the largest unit
/// that says it whole: `10 minutes before`, `1 hour before`, `at the
/// start`, `1 day before`. A negative lead is after the start.
pub fn alarm_words(before: i64) -> String {
    if before == 0 {
        return "at the start".to_string();
    }
    let lead = before.unsigned_abs();
    let units: [(u64, &str); 4] = [(7 * 86_400, "week"), (86_400, "day"), (3_600, "hour"), (60, "minute")];
    let (n, unit) = units
        .iter()
        .find(|(size, _)| lead.is_multiple_of(*size))
        .map_or((lead, "second"), |(size, unit)| (lead / size, *unit));
    let side = if before > 0 { "before" } else { "after the start" };
    format!("{n} {unit}{} {side}", if n == 1 { "" } else { "s" })
}

//! A day as a person types one: `2026-12-05`, `5 dec`, `fri`, `+2w`, each
//! with a time after it when one is wanted. It is the same grammar wherever
//! a date is asked for, so the words mean one thing.

use super::{civil_from_days, days_from_civil, days_in_month, weekday_of};

#[cfg(test)]
mod tests;

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];
/// In the order of `weekday_of`.
const WEEKDAYS: [&str; 7] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

/// What the words named: a day, and a minute of it when they gave a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Named {
    pub day: i64,
    pub minute: Option<i64>,
}

/// Which of `names` a word is: the whole name, or its first three letters
/// or more.
fn named(names: &[&str], word: &str) -> Option<usize> {
    names.iter().position(|name| word.len() >= 3 && name.starts_with(word))
}

/// `14:00` as a minute of the day.
fn minute(word: &str) -> Option<i64> {
    let (hour, minute) = word.split_once(':')?;
    let digits =
        |text: &str, least: usize| (least..=2).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit());
    if !digits(hour, 1) || !digits(minute, 2) {
        return None;
    }
    let (hour, minute) = (hour.parse::<i64>().ok()?, minute.parse::<i64>().ok()?);
    (hour < 24 && minute < 60).then_some(hour * 60 + minute)
}

fn date(year: i64, month: i64, dom: i64) -> Option<i64> {
    ((1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&dom))
        .then(|| days_from_civil(year, month, dom))
}

/// A day `months` whole months from `from`, on the same day of the month,
/// or on the month's last when it is shorter.
pub fn months_on(from: i64, months: i64) -> i64 {
    let (year, month, dom) = civil_from_days(from);
    let count = year * 12 + month - 1 + months;
    let (year, month) = (count.div_euclid(12), count.rem_euclid(12) + 1);
    days_from_civil(year, month, dom.min(days_in_month(year, month)))
}

/// The day `words` name, whatever their case, or `None` when they name
/// none. A date without a year and a weekday are the first such day on or
/// after `today`; a step (`+2w`, `-3d`, in days, weeks, months or years)
/// is taken from `from`, which is where the reader is looking. A time
/// alone is a time on `from`.
pub fn parse(words: &str, today: i64, from: i64) -> Option<Named> {
    let lower = words.trim().to_lowercase();
    let mut parts: Vec<&str> = lower.split_whitespace().collect();
    let minute = parts.last().and_then(|last| minute(last));
    if minute.is_some() {
        parts.pop();
    }
    let number = |text: &str| text.parse::<i64>().ok().filter(|_| text.bytes().all(|b| b.is_ascii_digit()));
    let day = match parts.as_slice() {
        [] => minute.map(|_| from),
        [one] => {
            let iso: Vec<&str> = one.split('-').collect();
            let step = one.strip_prefix('+').map(|rest| (1, rest)).or(one.strip_prefix('-').map(|rest| (-1, rest)));
            if let Some((sign, rest)) = step {
                let unit = rest.chars().last()?;
                let count = sign * number(&rest[..rest.len() - unit.len_utf8()])?;
                match unit {
                    'd' => Some(from + count),
                    'w' => Some(from + 7 * count),
                    'm' => Some(months_on(from, count)),
                    'y' => Some(months_on(from, 12 * count)),
                    _ => None,
                }
            } else if let [year, month, dom] = iso.as_slice() {
                date(number(year)?, number(month)?, number(dom)?)
            } else {
                let weekday = i64::try_from(named(&WEEKDAYS, one)?).ok()?;
                Some(today + (weekday - weekday_of(today)).rem_euclid(7))
            }
        }
        [dom, month, rest @ ..] if rest.len() <= 1 => {
            let (dom, month) = (number(dom)?, i64::try_from(named(&MONTHS, month)?).ok()? + 1);
            match rest.first() {
                Some(year) => date(number(year)?, month, dom),
                None => {
                    let year = civil_from_days(today).0;
                    // 29 February is looked for in the years that have one.
                    (year..year + 9).filter_map(|year| date(year, month, dom)).find(|day| *day >= today)
                }
            }
        }
        _ => None,
    };
    day.map(|day| Named { day, minute })
}

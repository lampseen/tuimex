//! Dates as the list words them: the design's `groupOf` and `shortDate`,
//! and the spans of time its status line counts in.
//!
//! A timestamp is read in the zone `set_zone` was given, and as UTC when
//! it was given none: nothing in this crate finds out where the reader is.

use std::sync::OnceLock;

use tuimex_core::Timestamp;

/// Seconds east of UTC at an instant. It is asked for each time shown, so
/// a message from the summer and one from the winter are each read as the
/// clock on the wall then stood.
static ZONE: OnceLock<fn(i64) -> i32> = OnceLock::new();

/// Has every time drawn after this read in the zone `offset` answers for.
/// The zone is the process's, as `TZ` is, and is set once, before the first
/// frame. Nothing under test and nothing in `--fixtures` sets it, so the
/// design's goldens, which are written in UTC, are compared with UTC.
pub fn set_zone(offset: fn(i64) -> i32) {
    // A second call keeps the first zone: a clock that changed its mind
    // between two rows would be worse than one that is merely not this one.
    let _ = ZONE.set(offset);
}

/// The seconds a wall clock in the reader's zone shows at `t`.
fn wall(t: Timestamp) -> i64 {
    t.0 + ZONE.get().map_or(0, |offset| i64::from(offset(t.0)))
}

/// The design's `LAYOUT.groups`.
pub const GROUPS: [&str; 4] = ["Today", "Yesterday", "Past 7 days", "Earlier"];

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn day(t: Timestamp) -> i64 {
    wall(t).div_euclid(86_400)
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The year, the month from 1, and the day of it, for a count of days
/// since 1970: Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, usize, i64) {
    let z = days + 719_468;
    let (era, day_of_era) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month as usize, day_of_year - (153 * shifted + 2) / 5 + 1)
}

/// The day and the month, `23 Nov`.
pub fn date(t: Timestamp) -> String {
    let (_, month, dom) = civil(day(t));
    format!("{dom} {}", MONTHS[month - 1])
}

/// The day in full, `Mon 23 Nov 2026`.
pub fn long_date(t: Timestamp) -> String {
    // 1 January 1970 was a Thursday.
    let weekday = DAYS[(day(t) + 4).rem_euclid(7) as usize];
    format!("{weekday} {} {}", date(t), civil(day(t)).0)
}

/// Which of `GROUPS` a message of that date falls under.
pub fn group_of(date: Timestamp, now: Timestamp) -> usize {
    match day(now) - day(date) {
        age if age <= 0 => 0,
        1 => 1,
        2..=7 => 2,
        _ => 3,
    }
}

/// The time of day, `09:05`.
pub fn clock(t: Timestamp) -> String {
    let minutes = wall(t).rem_euclid(86_400) / 60;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// The time, when the row sits under the label of its own day; otherwise
/// the day and the month.
pub fn short_date(date: Timestamp, group: usize, now: Timestamp) -> String {
    if day(now) - day(date) <= 1 && group_of(date, now) == group { clock(date) } else { self::date(date) }
}

/// The day, the month and the time, as a notice says when: `23 Nov 13:52`.
pub fn stamp(t: Timestamp) -> String {
    format!("{} {}", date(t), clock(t))
}

/// A span of time in its largest whole unit, as the status line counts a
/// send delay and the age of a sync.
pub fn span_of(seconds: u32) -> String {
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        _ => format!("{}h", seconds / 3600),
    }
}

#[cfg(test)]
mod tests;

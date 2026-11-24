use tuimex_core::Timestamp;

use super::{date, group_of, long_date, short_date, span_of};

/// 2026-11-23 14:20 UTC, the design's `NOW`.
const NOW: Timestamp = Timestamp(1_795_443_600);

fn ago(days: i64, hour: i64, minute: i64) -> Timestamp {
    Timestamp(NOW.0 - NOW.0 % 86_400 - days * 86_400 + hour * 3600 + minute * 60)
}

#[test]
fn a_date_falls_under_the_label_of_its_age_in_days() {
    for (days, group) in [(-1, 0), (0, 0), (1, 1), (2, 2), (7, 2), (8, 3), (400, 3)] {
        assert_eq!(group_of(ago(days, 23, 59), NOW), group, "{days} days ago");
    }
}

#[test]
fn a_row_under_the_label_of_its_own_day_shows_the_time() {
    assert_eq!(short_date(ago(0, 9, 5), 0, NOW), "09:05");
    assert_eq!(short_date(ago(1, 20, 44), 1, NOW), "20:44");
    assert_eq!(short_date(ago(1, 18, 10), 0, NOW), "22 Nov", "yesterday's message in a thread grouped under Today");
    assert_eq!(short_date(ago(2, 12, 12), 2, NOW), "21 Nov");
}

#[test]
fn the_day_and_month_are_right_across_month_and_leap_year_ends() {
    let at = |secs: i64| short_date(Timestamp(secs), 3, NOW);
    assert_eq!(at(0), "1 Jan");
    assert_eq!(at(951_782_400), "29 Feb", "2000-02-29");
    assert_eq!(at(1_709_251_199), "29 Feb", "the last second of 2024-02-29");
    assert_eq!(at(1_709_251_200), "1 Mar");
    assert_eq!(at(1_767_225_599), "31 Dec", "the last second of 2025");
    assert_eq!(at(-86_400), "31 Dec", "1969");
}

#[test]
fn a_span_is_counted_in_its_largest_whole_unit() {
    assert_eq!(
        [span_of(6), span_of(59), span_of(60), span_of(119), span_of(3599), span_of(7300)],
        ["6s", "59s", "1m", "1m", "59m", "2h"]
    );
}

#[test]
fn a_day_in_full_has_its_weekday_and_year() {
    assert_eq!(long_date(NOW), "Mon 23 Nov 2026");
    assert_eq!(long_date(Timestamp(0)), "Thu 1 Jan 1970");
    assert_eq!(long_date(Timestamp(-1)), "Wed 31 Dec 1969");
    assert_eq!(long_date(Timestamp(951_782_400)), "Tue 29 Feb 2000");
    assert_eq!(long_date(Timestamp(4_102_444_800)), "Fri 1 Jan 2100");
    assert_eq!(date(ago(1, 20, 44)), "22 Nov");
}

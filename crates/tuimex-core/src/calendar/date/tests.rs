use super::*;

fn day(year: i64, month: i64, dom: i64) -> i64 {
    days_from_civil(year, month, dom)
}

/// Today is Monday 23 November 2026 and the reader is looking at
/// Thursday the 26th, as in the design's `cal-goto`.
#[test]
fn the_words_of_a_date_name_one_day() {
    let (today, from) = (day(2026, 11, 23), day(2026, 11, 26));
    type Date = (i64, i64, i64);
    let table: [(&str, Option<Date>); 30] = [
        ("2026-12-05", Some((2026, 12, 5))),
        ("2027-2-9", Some((2027, 2, 9))),
        ("5 dec", Some((2026, 12, 5))),
        ("5 DEC", Some((2026, 12, 5))),
        (" 5  december ", Some((2026, 12, 5))),
        ("23 nov", Some((2026, 11, 23))),
        ("22 nov", Some((2027, 11, 22))),
        ("22 nov 2026", Some((2026, 11, 22))),
        ("29 feb", Some((2028, 2, 29))),
        ("fri", Some((2026, 11, 27))),
        ("Friday", Some((2026, 11, 27))),
        ("mon", Some((2026, 11, 23))),
        ("sun", Some((2026, 11, 29))),
        ("+2w", Some((2026, 12, 10))),
        ("-3d", Some((2026, 11, 23))),
        ("+1m", Some((2026, 12, 26))),
        ("+3m", Some((2027, 2, 26))),
        ("-1y", Some((2025, 11, 26))),
        ("+0d", Some((2026, 11, 26))),
        ("", None),
        ("soon", None),
        ("fr", None),
        ("31 nov", None),
        ("2026-13-01", None),
        ("2026-02-30", None),
        ("+2", None),
        ("+w", None),
        ("5 dec 2026 again", None),
        ("dec 5", None),
        ("5", None),
    ];
    for (words, want) in table {
        let got = parse(words, today, from).map(|named| civil_from_days(named.day));
        assert_eq!(got, want, "{words:?}");
        assert_eq!(parse(words, today, from).and_then(|named| named.minute), None, "{words:?}");
    }
}

#[test]
fn a_step_of_months_ends_on_the_month_s_last_day_when_it_is_shorter() {
    let from = day(2027, 1, 31);
    assert_eq!(parse("+1m", from, from).map(|n| civil_from_days(n.day)), Some((2027, 2, 28)));
    assert_eq!(parse("+1y", day(2028, 2, 29), day(2028, 2, 29)).map(|n| civil_from_days(n.day)), Some((2029, 2, 28)));
}

/// The design's `cal-propose`: a weekday that is the day itself stays on
/// it, and the time rides along.
#[test]
fn a_time_after_the_day_is_kept_and_a_time_alone_is_on_the_day_looked_at() {
    let thu = day(2026, 11, 26);
    assert_eq!(parse("thu 14:00", thu, thu), Some(Named { day: thu, minute: Some(14 * 60) }));
    assert_eq!(parse("5 dec 9:05", thu, thu), Some(Named { day: day(2026, 12, 5), minute: Some(545) }));
    assert_eq!(parse("14:00", day(2026, 11, 23), thu), Some(Named { day: thu, minute: Some(840) }));
    for wrong in ["thu 24:00", "thu 14:60", "thu 14:5x", "thu 1400", "thu 14:000"] {
        assert_eq!(parse(wrong, thu, thu), None, "{wrong}");
    }
}

#[test]
fn a_time_is_read_by_the_twelve_hour_clock_too_and_alone_without_its_colon() {
    let thu = day(2026, 11, 26);
    assert_eq!(parse("fri 2pm", thu, thu), Some(Named { day: thu + 1, minute: Some(14 * 60) }));
    assert_eq!(parse("2:30PM", thu, thu), Some(Named { day: thu, minute: Some(870) }));
    for (words, minute) in [("12am", 0), ("12pm", 720), ("9am", 540), ("11:59pm", 1439), ("9", 540), ("0", 0)] {
        assert_eq!(time(words), Some(minute), "{words}");
    }
    assert_eq!((time("930"), time("1430"), time(" 14:30 ")), (Some(570), Some(870), Some(870)));
    for wrong in ["13pm", "0am", "24", "960", "2460", "14300", "pm", "2 pm", "fri", "fri 2pm", ""] {
        assert_eq!(time(wrong), None, "{wrong}");
    }
    assert_eq!(parse("5 dec 2027", thu, thu).map(|n| n.minute), Some(None), "after a date, digits are its year");
    assert_eq!(parse("9", thu, thu), None, "and where a day is asked for an hour alone is none");
}

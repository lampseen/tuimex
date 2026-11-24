use super::*;

const HOUR: i64 = 3_600;

fn at(year: i64, month: i64, day: i64, hour: i64) -> i64 {
    days_from_civil(year, month, day) * DAY + hour * HOUR
}

fn rule(freq: Freq) -> Rule {
    Rule { freq, interval: 1, count: None, until: None, by_day: Vec::new(), by_month_day: Vec::new() }
}

/// Central Europe by its real rule: the last Sundays of March and October.
fn berlin() -> Zone {
    Zone {
        periods: vec![
            Period { start: at(1970, 3, 29, 2), offset: 2 * HOUR, yearly: Some((3, -1, 0)) },
            Period { start: at(1970, 10, 25, 3), offset: HOUR, yearly: Some((10, -1, 0)) },
        ],
    }
}

fn event(uid: &str, start: Moment, end: Option<Moment>) -> Event {
    Event {
        uid: uid.into(),
        sequence: 0,
        instead_of: None,
        summary: uid.into(),
        start,
        end,
        location: None,
        description: None,
        organizer: None,
        guests: Vec::new(),
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

/// A rule's name, its start, the rule, the window, and the dates it must give.
type Case = (&'static str, i64, Rule, i64, i64, Vec<(i64, i64, i64)>);

fn days(walls: &[i64]) -> Vec<(i64, i64, i64)> {
    walls.iter().map(|w| civil_from_days(w.div_euclid(DAY))).collect()
}

#[test]
fn a_day_number_and_its_date_are_each_other_s_inverse() {
    for day in (-800_000..800_000).step_by(997) {
        let (year, month, dom) = civil_from_days(day);
        assert_eq!(days_from_civil(year, month, dom), day, "{year}-{month}-{dom}");
    }
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(days_from_civil(2028, 2, 29)), (2028, 2, 29));
    assert_eq!(weekday_of(days_from_civil(2026, 11, 23)), 1, "23 November 2026 is a Monday");
    assert_eq!(days_in_month(2026, 2), 28);
    assert_eq!(days_in_month(2028, 2), 29);
    assert_eq!(days_in_month(2026, 12), 31);
}

#[test]
fn a_zone_s_offset_is_found_from_either_side_of_the_clock() {
    let zone = berlin();
    assert_eq!(zone.offset_at_wall(at(2026, 7, 1, 10)), Some(2 * HOUR));
    assert_eq!(zone.offset_at_wall(at(2026, 12, 1, 10)), Some(HOUR));
    // 08:00 UTC is 10:00 in summer and 09:00 in winter.
    assert_eq!(zone.offset_at(at(2026, 7, 1, 8)), Some(2 * HOUR));
    assert_eq!(zone.offset_at(at(2026, 12, 1, 8)), Some(HOUR));
    // The change on 25 October 2026 happens at 01:00 UTC.
    assert_eq!(zone.offset_at(at(2026, 10, 25, 0)), Some(2 * HOUR));
    assert_eq!(zone.offset_at(at(2026, 10, 25, 1)), Some(HOUR));
    assert_eq!(Zone::fixed(-5 * HOUR).offset_at(at(2026, 1, 1, 0)), Some(-5 * HOUR));
    assert_eq!(Zone::default().offset_at(0), None, "a zone with no periods has no offset to give");
}

#[test]
fn a_written_time_is_placed_only_when_its_zone_is_known() {
    let zones = BTreeMap::from([("Europe/Berlin".to_string(), berlin())]);
    let new_york = Zone::fixed(-5 * HOUR);
    let wall = |zone: Option<&str>| Moment::Wall { wall: at(2026, 12, 1, 15), zone: zone.map(str::to_string) };
    assert_eq!(wall(Some("Europe/Berlin")).local_wall(&zones, &new_york), Some(at(2026, 12, 1, 9)));
    assert_eq!(
        wall(None).local_wall(&zones, &new_york),
        Some(at(2026, 12, 1, 15)),
        "no zone is the reader's own clock"
    );
    assert_eq!(wall(Some("Mars/Olympus")).local_wall(&zones, &new_york), None);
    assert_eq!(Moment::Utc(at(2026, 12, 1, 15)).local_wall(&zones, &new_york), Some(at(2026, 12, 1, 10)));
    let day = days_from_civil(2026, 12, 1);
    assert_eq!(Moment::Day(day).local_wall(&zones, &new_york), Some(day * DAY), "a day is the same day everywhere");
}

#[test]
fn each_kind_of_rule_gives_its_days() {
    let from = at(2026, 11, 1, 0);
    let to = at(2027, 3, 1, 0);
    let table: Vec<Case> = vec![
        (
            "daily",
            at(2026, 11, 27, 9),
            rule(Freq::Daily),
            from,
            at(2026, 12, 1, 0),
            vec![(2026, 11, 27), (2026, 11, 28), (2026, 11, 29), (2026, 11, 30)],
        ),
        (
            "every weekday",
            at(2026, 11, 2, 9),
            Rule { by_day: (1..=5).map(|w| (0, w)).collect(), ..rule(Freq::Weekly) },
            at(2026, 11, 26, 0),
            at(2026, 12, 2, 0),
            vec![(2026, 11, 26), (2026, 11, 27), (2026, 11, 30), (2026, 12, 1)],
        ),
        (
            "every second week, on the day it began",
            at(2026, 11, 3, 9),
            Rule { interval: 2, ..rule(Freq::Weekly) },
            from,
            at(2026, 12, 20, 0),
            vec![(2026, 11, 3), (2026, 11, 17), (2026, 12, 1), (2026, 12, 15)],
        ),
        (
            "three times and no more",
            at(2026, 11, 3, 9),
            Rule { count: Some(3), ..rule(Freq::Weekly) },
            from,
            to,
            vec![(2026, 11, 3), (2026, 11, 10), (2026, 11, 17)],
        ),
        (
            "counted from its start, not from the window",
            at(2026, 10, 6, 9),
            Rule { count: Some(6), ..rule(Freq::Weekly) },
            from,
            to,
            vec![(2026, 11, 3), (2026, 11, 10)],
        ),
        (
            "until a day, that day included",
            at(2026, 11, 3, 9),
            Rule { until: Some(at(2026, 11, 17, 9)), ..rule(Freq::Weekly) },
            from,
            to,
            vec![(2026, 11, 3), (2026, 11, 10), (2026, 11, 17)],
        ),
        (
            "the second Tuesday of the month",
            at(2026, 11, 10, 9),
            Rule { by_day: vec![(2, 2)], ..rule(Freq::Monthly) },
            from,
            to,
            vec![(2026, 11, 10), (2026, 12, 8), (2027, 1, 12), (2027, 2, 9)],
        ),
        (
            "the last Friday of the month",
            at(2026, 11, 27, 9),
            Rule { by_day: vec![(-1, 5)], ..rule(Freq::Monthly) },
            from,
            to,
            vec![(2026, 11, 27), (2026, 12, 25), (2027, 1, 29), (2027, 2, 26)],
        ),
        (
            "the 31st, in the months that have one",
            at(2026, 10, 31, 9),
            rule(Freq::Monthly),
            from,
            to,
            vec![(2026, 12, 31), (2027, 1, 31)],
        ),
        (
            "the last day of the month",
            at(2026, 11, 30, 9),
            Rule { by_month_day: vec![-1], ..rule(Freq::Monthly) },
            from,
            to,
            vec![(2026, 11, 30), (2026, 12, 31), (2027, 1, 31), (2027, 2, 28)],
        ),
        (
            "yearly",
            at(2020, 12, 24, 18),
            rule(Freq::Yearly),
            from,
            at(2029, 1, 1, 0),
            vec![(2026, 12, 24), (2027, 12, 24), (2028, 12, 24)],
        ),
        (
            "29 February, in leap years",
            at(2024, 2, 29, 9),
            rule(Freq::Yearly),
            from,
            at(2033, 1, 1, 0),
            vec![(2028, 2, 29), (2032, 2, 29)],
        ),
    ];
    for (name, start, rule, from, to, want) in table {
        let got = occurrences(start, &rule, from, to);
        assert_eq!(days(&got), want, "{name}");
        assert!(got.iter().all(|w| w.rem_euclid(DAY) == start.rem_euclid(DAY)), "{name}: an occurrence left its hour");
    }
}

#[test]
fn a_rule_from_long_ago_is_entered_at_the_window() {
    // Forty years of days: walked from the start this would be 14,600
    // steps before the first one wanted, more than the loop allows.
    let got = occurrences(at(1986, 1, 1, 7), &rule(Freq::Daily), at(2026, 11, 26, 0), at(2026, 11, 28, 0));
    assert_eq!(days(&got), vec![(2026, 11, 26), (2026, 11, 27)]);
}

#[test]
fn a_repeat_keeps_its_hour_when_the_clocks_change() {
    let zones = BTreeMap::from([("Europe/Berlin".to_string(), berlin())]);
    let mut standup =
        event("standup", Moment::Wall { wall: at(2026, 10, 19, 9), zone: Some("Europe/Berlin".into()) }, None);
    standup.end = Some(Moment::Wall { wall: at(2026, 10, 19, 9) + 900, zone: Some("Europe/Berlin".into()) });
    standup.repeat = Some(Repeat::Rule(rule(Freq::Weekly)));
    let events = [standup];
    // The reader is in Berlin too: 09:00 before the change and after it.
    let here =
        occurrences_between(&events, &zones, &berlin(), days_from_civil(2026, 10, 19), days_from_civil(2026, 11, 3));
    assert_eq!(
        here.iter().map(|o| o.start).collect::<Vec<_>>(),
        vec![at(2026, 10, 19, 9), at(2026, 10, 26, 9), at(2026, 11, 2, 9)]
    );
    assert!(here.iter().all(|o| o.end - o.start == 900 && !o.all_day));
    // A reader on UTC sees it move from 07:00 to 08:00.
    let utc = occurrences_between(
        &events,
        &zones,
        &Zone::fixed(0),
        days_from_civil(2026, 10, 19),
        days_from_civil(2026, 11, 3),
    );
    assert_eq!(
        utc.iter().map(|o| o.start).collect::<Vec<_>>(),
        vec![at(2026, 10, 19, 7), at(2026, 10, 26, 8), at(2026, 11, 2, 8)]
    );
}

#[test]
fn an_occurrence_left_out_or_replaced_is_not_drawn_twice() {
    let nine = |day: i64| Moment::Wall { wall: at(2026, 11, day, 9), zone: None };
    let mut weekly = event("weekly", nine(3), Some(Moment::Wall { wall: at(2026, 11, 3, 10), zone: None }));
    weekly.repeat = Some(Repeat::Rule(rule(Freq::Weekly)));
    weekly.excluded = vec![nine(10)];
    // The one on the 17th was moved to the 18th at two.
    let mut moved = event("weekly", Moment::Wall { wall: at(2026, 11, 18, 14), zone: None }, None);
    moved.instead_of = Some(nine(17));
    let events = [weekly, moved];
    let got = occurrences_between(
        &events,
        &BTreeMap::new(),
        &Zone::fixed(0),
        days_from_civil(2026, 11, 1),
        days_from_civil(2026, 11, 30),
    );
    assert_eq!(
        got.iter().map(|o| o.start).collect::<Vec<_>>(),
        vec![at(2026, 11, 3, 9), at(2026, 11, 18, 14), at(2026, 11, 24, 9)]
    );
}

#[test]
fn days_are_touched_by_what_runs_over_them() {
    let day = |d: i64| days_from_civil(2026, 11, d);
    let cabin = event("cabin", Moment::Day(day(27)), Some(Moment::Day(day(30))));
    let one_day = event("library", Moment::Day(day(27)), None);
    let late = event("late", Moment::Utc(at(2026, 11, 26, 23)), Some(Moment::Utc(at(2026, 11, 27, 1))));
    let mut gone = event("gone", Moment::Day(day(27)), None);
    gone.cancelled = true;
    let nowhere = event("nowhere", Moment::Wall { wall: at(2026, 11, 27, 9), zone: Some("Mars/Olympus".into()) }, None);
    let mut odd = event("odd", Moment::Day(day(27)), None);
    odd.repeat = Some(Repeat::Unread("FREQ=SECONDLY".into()));
    let events = [cabin, one_day, late, gone, nowhere, odd];
    let on = |d: i64| -> Vec<&str> {
        occurrences_between(&events, &BTreeMap::new(), &Zone::fixed(0), day(d), day(d) + 1)
            .iter()
            .map(|o| o.event.uid.as_str())
            .collect()
    };
    assert_eq!(on(26), vec!["late"]);
    assert_eq!(on(27), vec!["late", "cabin", "library", "odd"], "what began the night before comes first");
    assert_eq!(on(29), vec!["cabin"], "an all-day event ends at the midnight after its last day");
    assert_eq!(on(30), Vec::<&str>::new());
    assert_eq!(on(28), vec!["cabin"], "a rule tuimex does not work out is shown on its first day only");
}

use super::*;
use crate::calendar::days_from_civil;

fn rule(freq: Freq, interval: i64) -> Rule {
    Rule { freq, interval, count: None, until: None, by_day: Vec::new(), by_month_day: Vec::new() }
}

fn on(freq: Freq, by_day: &[(i64, i64)]) -> Rule {
    Rule { by_day: by_day.to_vec(), ..rule(freq, 1) }
}

/// The wording `recurrence` gives the reader's card for the same rules,
/// where the two overlap, and the design's `every weekday`.
#[test]
fn a_rule_is_said_as_the_card_and_the_design_say_it() {
    let weekdays: Vec<(i64, i64)> = (1..=5).map(|day| (0, day)).collect();
    let until = days_from_civil(2026, 12, 31) * DAY + 9 * 3600 + 30 * 60;
    let table: Vec<(Rule, &str)> = vec![
        (rule(Freq::Daily, 1), "daily"),
        (rule(Freq::Weekly, 1), "weekly"),
        (rule(Freq::Monthly, 1), "monthly"),
        (rule(Freq::Yearly, 1), "yearly"),
        (rule(Freq::Weekly, 2), "every 2 weeks"),
        (rule(Freq::Daily, 3), "every 3 days"),
        (on(Freq::Weekly, &[(0, 2)]), "weekly on Tue"),
        (on(Freq::Weekly, &[(0, 1), (0, 3), (0, 5)]), "weekly on Mon, Wed, Fri"),
        (on(Freq::Weekly, &weekdays), "every weekday"),
        (on(Freq::Daily, &weekdays), "every weekday"),
        (Rule { interval: 2, ..on(Freq::Weekly, &weekdays) }, "every 2 weeks on Mon, Tue, Wed, Thu, Fri"),
        (on(Freq::Weekly, &[(0, 1), (0, 2), (0, 3), (0, 4)]), "weekly on Mon, Tue, Wed, Thu"),
        (on(Freq::Monthly, &[(2, 2)]), "monthly on the second Tue"),
        (on(Freq::Monthly, &[(-1, 5)]), "monthly on the last Fri"),
        (on(Freq::Monthly, &[(-2, 1)]), "monthly on the second to last Mon"),
        (on(Freq::Monthly, &[(0, 3)]), "monthly on every Wed"),
        (Rule { by_month_day: vec![-1], ..rule(Freq::Monthly, 1) }, "monthly on the last day"),
        (Rule { by_month_day: vec![1, 15], ..rule(Freq::Monthly, 1) }, "monthly on the 1st, the 15th"),
        (Rule { by_month_day: vec![22, 23, 11], ..rule(Freq::Monthly, 1) }, "monthly on the 22nd, the 23rd, the 11th"),
        (Rule { by_month_day: vec![-2], ..rule(Freq::Monthly, 3) }, "every 3 months on the second to last day"),
        (rule(Freq::Yearly, 2), "every 2 years"),
        (Rule { count: Some(10), ..on(Freq::Weekly, &[(0, 2)]) }, "weekly on Tue, 10 times"),
        (Rule { count: Some(1), ..rule(Freq::Daily, 1) }, "daily, once"),
        (Rule { until: Some(until), ..rule(Freq::Monthly, 1) }, "monthly, until 31 Dec 2026"),
        (Rule { until: Some(until), ..on(Freq::Weekly, &weekdays) }, "every weekday, until 31 Dec 2026"),
        (Rule { interval: 0, ..rule(Freq::Weekly, 1) }, "weekly"),
    ];
    for (rule, want) in table {
        assert_eq!(rule_words(&rule), want, "{rule:?}");
    }
}

/// ADR-060: a rule outside the set is not guessed at, and says so.
#[test]
fn a_rule_tuimex_does_not_work_out_says_so() {
    assert_eq!(repeat_words(&Repeat::Unread("FREQ=HOURLY".into())), "repeats in a way tuimex does not work out");
    assert_eq!(repeat_words(&Repeat::Rule(rule(Freq::Yearly, 1))), "yearly");
}

#[test]
fn a_reminder_is_said_in_its_largest_whole_unit() {
    for (before, want) in [
        (600, "10 minutes before"),
        (60, "1 minute before"),
        (3600, "1 hour before"),
        (5400, "90 minutes before"),
        (7200, "2 hours before"),
        (0, "at the start"),
        (86_400, "1 day before"),
        (2 * 86_400, "2 days before"),
        (7 * 86_400, "1 week before"),
        (30, "30 seconds before"),
        (-300, "5 minutes after the start"),
    ] {
        assert_eq!(alarm_words(before), want, "{before}");
    }
}

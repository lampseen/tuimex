use std::collections::BTreeMap;

use super::*;
use crate::calendar::{Freq, Period, Rule, days_from_civil};

const HOUR: i64 = 3_600;
const MINUTE: i64 = 60;
const LEAD: i64 = 10 * MINUTE;

fn at(year: i64, month: i64, day: i64, hour: i64, minute: i64) -> i64 {
    days_from_civil(year, month, day) * DAY + hour * HOUR + minute * MINUTE
}

/// Central Europe by its real rule: the clocks go back on the last Sunday
/// of October, which in 2026 is the 25th.
fn berlin() -> Zone {
    Zone {
        periods: vec![
            Period { start: at(1970, 3, 29, 2, 0), offset: 2 * HOUR, yearly: Some((3, -1, 0)) },
            Period { start: at(1970, 10, 25, 3, 0), offset: HOUR, yearly: Some((10, -1, 0)) },
        ],
    }
}

fn event(uid: &str, start: Moment) -> Event {
    Event {
        uid: uid.into(),
        sequence: 0,
        instead_of: None,
        summary: format!("{uid} title"),
        start,
        end: None,
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

fn daily(mut event: Event) -> Event {
    let rule = Rule { freq: Freq::Daily, interval: 1, count: None, until: None, by_day: vec![], by_month_day: vec![] };
    event.repeat = Some(Repeat::Rule(rule));
    event
}

fn with(mut event: Event, change: impl FnOnce(&mut Event)) -> Event {
    change(&mut event);
    event
}

/// Asks as the event loop does: from `from` to `to`, `step` at a time,
/// each look starting where the last one ended.
fn sweep(held: &Held, from: i64, to: i64, step: i64) -> Vec<Reminder> {
    let declined = |e: &Event| e.uid.starts_with("declined");
    let (mut checked, mut out) = (from, Vec::new());
    while checked < to {
        let now = (checked + step).min(to);
        out.extend(due(held, &declined, checked, now));
        checked = now;
    }
    out
}

struct Case {
    name: &'static str,
    events: Vec<Event>,
    local: Zone,
    from: i64,
    to: i64,
    step: i64,
    /// Each reminder raised, as its event and when it fell due.
    raised: Vec<(&'static str, i64)>,
}

fn cases() -> Vec<Case> {
    let utc = Zone::fixed(0);
    let ten = Moment::Utc(at(2026, 11, 24, 10, 0));
    let morning = (at(2026, 11, 24, 9, 0), at(2026, 11, 24, 11, 0));
    let berlin_nine = Moment::Wall { wall: at(2026, 10, 23, 9, 0), zone: Some("Europe/Berlin".into()) };
    let floating_nine = Moment::Wall { wall: at(2026, 10, 23, 9, 0), zone: None };
    let around_the_change = (at(2026, 10, 24, 0, 0), at(2026, 10, 26, 0, 0));
    let three_days = (at(2026, 11, 23, 0, 0), at(2026, 11, 26, 0, 0));
    let daily_ten = daily(event("a", Moment::Utc(at(2026, 11, 23, 10, 0))));
    vec![
        Case {
            name: "an event with no alarm is reminded the lead before it",
            events: vec![event("a", ten.clone())],
            local: utc.clone(),
            from: morning.0,
            to: morning.1,
            step: MINUTE,
            raised: vec![("a", at(2026, 11, 24, 9, 50))],
        },
        Case {
            name: "an event's own alarms are used in place of the lead, each once",
            events: vec![with(event("a", ten.clone()), |e| e.alarms = vec![15 * MINUTE, MINUTE, MINUTE])],
            local: utc.clone(),
            from: morning.0,
            to: morning.1,
            step: MINUTE,
            raised: vec![("a", at(2026, 11, 24, 9, 45)), ("a", at(2026, 11, 24, 9, 59))],
        },
        Case {
            name: "looks that never land on the instant it falls due still raise it once",
            events: vec![event("a", ten.clone())],
            local: utc.clone(),
            from: morning.0,
            to: morning.1,
            step: 7,
            raised: vec![("a", at(2026, 11, 24, 9, 50))],
        },
        Case {
            name: "each occurrence of a repeating event is its own reminder",
            events: vec![daily_ten.clone()],
            local: utc.clone(),
            from: three_days.0,
            to: three_days.1,
            step: MINUTE,
            raised: vec![
                ("a", at(2026, 11, 23, 9, 50)),
                ("a", at(2026, 11, 24, 9, 50)),
                ("a", at(2026, 11, 25, 9, 50)),
            ],
        },
        Case {
            name: "an occurrence left out has none, and one moved is reminded where it went",
            events: vec![
                with(daily_ten.clone(), |e| e.excluded = vec![Moment::Utc(at(2026, 11, 24, 10, 0))]),
                with(event("a", Moment::Utc(at(2026, 11, 25, 14, 0))), |e| {
                    e.instead_of = Some(Moment::Utc(at(2026, 11, 25, 10, 0)));
                }),
            ],
            local: utc.clone(),
            from: three_days.0,
            to: three_days.1,
            step: MINUTE,
            raised: vec![("a", at(2026, 11, 23, 9, 50)), ("a", at(2026, 11, 25, 13, 50))],
        },
        Case {
            name: "an occurrence cancelled on its own has none",
            events: vec![
                daily_ten.clone(),
                with(event("a", Moment::Utc(at(2026, 11, 24, 10, 0))), |e| {
                    e.instead_of = Some(Moment::Utc(at(2026, 11, 24, 10, 0)));
                    e.cancelled = true;
                }),
            ],
            local: utc.clone(),
            from: three_days.0,
            to: three_days.1,
            step: MINUTE,
            raised: vec![("a", at(2026, 11, 23, 9, 50)), ("a", at(2026, 11, 25, 9, 50))],
        },
        Case {
            name: "an all-day event has no reminder unless it carries an alarm",
            events: vec![
                event("bare", Moment::Day(days_from_civil(2026, 11, 24))),
                with(event("alarmed", Moment::Day(days_from_civil(2026, 11, 24))), |e| e.alarms = vec![HOUR]),
            ],
            local: utc.clone(),
            from: three_days.0,
            to: three_days.1,
            step: MINUTE,
            raised: vec![("alarmed", at(2026, 11, 23, 23, 0))],
        },
        Case {
            name: "a cancelled event and a declined one have none",
            events: vec![with(event("cancelled", ten.clone()), |e| e.cancelled = true), event("declined", ten.clone())],
            local: utc.clone(),
            from: morning.0,
            to: morning.1,
            step: MINUTE,
            raised: vec![],
        },
        Case {
            name: "a time in a zone nothing defines is no instant and has none",
            events: vec![event("a", Moment::Wall { wall: at(2026, 11, 24, 10, 0), zone: Some("Nowhere".into()) })],
            local: utc.clone(),
            from: morning.0,
            to: morning.1,
            step: MINUTE,
            raised: vec![],
        },
        Case {
            name: "a 09:00 in Berlin is reminded at 08:50 Berlin time on either side of the clocks going back",
            events: vec![daily(event("a", berlin_nine))],
            local: utc.clone(),
            from: around_the_change.0,
            to: around_the_change.1,
            step: MINUTE,
            // 06:50 UTC in summer time, 07:50 UTC once the clocks are back.
            raised: vec![("a", at(2026, 10, 24, 6, 50)), ("a", at(2026, 10, 25, 7, 50))],
        },
        Case {
            name: "a 09:00 with no zone is on the reader's clock, which goes back too",
            events: vec![daily(event("a", floating_nine))],
            local: berlin(),
            from: around_the_change.0,
            to: around_the_change.1,
            step: MINUTE,
            raised: vec![("a", at(2026, 10, 24, 6, 50)), ("a", at(2026, 10, 25, 7, 50))],
        },
        Case {
            name: "after a gap only what fell due in the last five minutes, for an event not begun, is raised",
            events: vec![
                // Due 10:00, four minutes ago, and it begins at 10:10.
                event("recent", Moment::Utc(at(2026, 11, 24, 10, 10))),
                // Due 09:52, more than five minutes ago.
                event("stale", Moment::Utc(at(2026, 11, 24, 10, 2))),
                // Due 10:01, but it began at 10:03.
                with(event("begun", Moment::Utc(at(2026, 11, 24, 10, 3))), |e| e.alarms = vec![2 * MINUTE]),
            ],
            local: utc.clone(),
            from: at(2026, 11, 24, 9, 0),
            to: at(2026, 11, 24, 10, 4),
            step: 64 * MINUTE,
            raised: vec![("recent", at(2026, 11, 24, 10, 0))],
        },
        Case {
            name: "a gap of exactly the bound is no gap, and raises even what has begun",
            events: vec![with(event("begun", Moment::Utc(at(2026, 11, 24, 9, 2))), |e| e.alarms = vec![MINUTE])],
            local: utc.clone(),
            from: at(2026, 11, 24, 9, 0),
            to: at(2026, 11, 24, 9, 5),
            step: LATE,
            raised: vec![("begun", at(2026, 11, 24, 9, 1))],
        },
    ]
}

#[test]
fn each_rule_of_a_reminder_holds() {
    let mut zones = BTreeMap::new();
    zones.insert("Europe/Berlin".to_string(), berlin());
    for case in cases() {
        let held = Held { events: &case.events, zones: &zones, local: &case.local, lead: LEAD };
        let raised: Vec<(String, i64)> =
            sweep(&held, case.from, case.to, case.step).into_iter().map(|r| (r.uid, r.due)).collect();
        let wanted: Vec<(String, i64)> = case.raised.iter().map(|(uid, due)| (uid.to_string(), *due)).collect();
        assert_eq!(raised, wanted, "{}", case.name);
    }
}

#[test]
fn a_reminder_carries_the_title_the_start_and_how_long_until_it() {
    let (zones, local) = (BTreeMap::new(), Zone::fixed(0));
    let events = vec![event("a", Moment::Utc(at(2026, 11, 24, 10, 0)))];
    let held = Held { events: &events, zones: &zones, local: &local, lead: LEAD };
    let now = at(2026, 11, 24, 9, 51);
    let raised = due(&held, &|_| false, now - 2 * MINUTE, now);
    let start = at(2026, 11, 24, 10, 0);
    let wanted = Reminder { uid: "a".into(), summary: "a title".into(), start, due: start - LEAD, until: 9 * MINUTE };
    assert_eq!(raised, vec![wanted]);
}

#[test]
fn a_look_that_does_not_move_forward_raises_nothing() {
    let (zones, local) = (BTreeMap::new(), Zone::fixed(0));
    let events = vec![event("a", Moment::Utc(at(2026, 11, 24, 10, 0)))];
    let held = Held { events: &events, zones: &zones, local: &local, lead: LEAD };
    let due_at = at(2026, 11, 24, 9, 50);
    assert_eq!(due(&held, &|_| false, due_at, due_at).len(), 0);
    assert_eq!(due(&held, &|_| false, due_at + 1, due_at - 1).len(), 0, "a clock set back raises nothing");
}

#[test]
fn an_alarm_a_year_and_more_from_its_start_is_not_worked_out() {
    let (zones, local) = (BTreeMap::new(), Zone::fixed(0));
    let start = at(2026, 11, 24, 10, 0);
    let events = vec![with(daily(event("a", Moment::Utc(start))), |e| e.alarms = vec![i64::MAX, i64::MIN, 0])];
    let held = Held { events: &events, zones: &zones, local: &local, lead: LEAD };
    let raised = due(&held, &|_| false, start - 1, start);
    assert_eq!(raised.iter().map(|r| r.due).collect::<Vec<_>>(), vec![start]);
}

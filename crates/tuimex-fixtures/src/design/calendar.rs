//! The design's calendar: `CALENDARS` and `EVENTS` of `design/tui.js`,
//! each event in the calendar the design keeps it in. A time there is the
//! clock of wherever the reader is, so every moment here is written with no
//! zone and reads the same in any.

use tuimex_core::calendar::{DAY, Event, Freq, Guest, Moment, Repeat, Rule, Said};

use super::{BANK, DMITRI, IMANI, LARS, ME, ME_WORK, MEI, PERSONAL, PRIYA, Person, WORK, address};

/// One of `CALENDARS`, with the events of `EVENTS` it holds, in that
/// table's order.
#[derive(Debug, Clone)]
pub struct Kept {
    pub account: &'static str,
    /// What tuimex alone holds (ADR-055); otherwise the account's own.
    pub local: bool,
    /// Seconds since it was last read: the design's `read`.
    pub read: Option<u32>,
    pub events: Vec<Event>,
}

/// `thu`, `fri` and `dec5` of the design's `build()`: the days its screens
/// put the cursor on.
pub const THU: i64 = day(11, 26);
pub const FRI: i64 = THU + 1;
pub const DEC_5: i64 = day(12, 5);

/// A day of 2026 by its number: 2026-01-01 is day 20454.
pub(super) const fn day(month: i64, dom: i64) -> i64 {
    const BEFORE: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    20_454 + BEFORE[(month - 1) as usize] + dom - 1
}

fn clock(day: i64, hour: i64, minute: i64) -> Moment {
    Moment::Wall { wall: day * DAY + hour * 3600 + minute * 60, zone: None }
}

fn event(id: &str, title: &str, start: Moment, end: Moment) -> Event {
    Event {
        uid: format!("{id}@design.example.net"),
        sequence: 0,
        instead_of: None,
        summary: title.to_string(),
        start,
        end: Some(end),
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

/// An event from `from` to `to` on one day, each an hour and a minute.
fn timed(id: &str, title: &str, on: i64, from: (i64, i64), to: (i64, i64)) -> Event {
    event(id, title, clock(on, from.0, from.1), clock(on, to.0, to.1))
}

/// An event of `days` whole days. A calendar writes the day after the
/// last as its end.
fn all_day(id: &str, title: &str, on: i64, days: i64) -> Event {
    event(id, title, Moment::Day(on), Moment::Day(on + days))
}

/// The changes the design's table makes to an event, by name.
trait Change: Sized {
    fn at(self, place: &str) -> Self;
    fn meet(self, room: &str) -> Self;
    /// Who asked, the account's own address among the guests with what the
    /// design's `answer` says of it, then the others.
    fn asked(self, by: Person, you: (Person, Said), others: &[(Person, Said)]) -> Self;
    fn notes(self, notes: &str) -> Self;
    fn reminded(self) -> Self;
}

impl Change for Event {
    fn at(mut self, place: &str) -> Self {
        self.location = Some(place.to_string());
        self
    }
    fn meet(mut self, room: &str) -> Self {
        self.links = vec![format!("https://meet.example.org/r/{room}")];
        self
    }
    fn asked(mut self, by: Person, you: (Person, Said), others: &[(Person, Said)]) -> Self {
        let guest = |(who, said): (Person, Said)| Guest { who: address(who), said };
        self.organizer = Some(address(by));
        self.guests = [you].into_iter().chain(others.iter().copied()).map(guest).collect();
        self
    }
    fn notes(mut self, notes: &str) -> Self {
        self.description = Some(notes.to_string());
        self
    }
    /// `10 minutes before`.
    fn reminded(mut self) -> Self {
        self.alarms = vec![600];
        self
    }
}

/// `EVENTS`, each in the one of `CALENDARS` that holds it.
pub fn calendars() -> Vec<Kept> {
    let (yes, no, maybe, nothing) = (Said::Accepted, Said::Declined, Said::Tentative, Said::Nothing);
    let weekdays = Rule {
        freq: Freq::Weekly,
        interval: 1,
        count: None,
        until: Some(day(12, 31) * DAY + 9 * 3600 + 30 * 60),
        by_day: (1..=5).map(|weekday| (0, weekday)).collect(),
        by_month_day: Vec::new(),
    };
    let standup = Event {
        repeat: Some(Repeat::Rule(weekdays)),
        ..timed("standup", "Standup", day(11, 2), (9, 30), (9, 45)).meet("standup").asked(LARS, (ME_WORK, yes), &[])
    };
    let roadmap = Event {
        more: [
            ("Status", "confirmed"),
            ("Shown as", "busy"),
            ("Privacy", "default"),
            ("By phone", "+47 21 00 00 00, code 4471"),
            ("Changed", "23 Nov 14:05, by the organizer"),
        ]
        .map(|(label, value)| (label.to_string(), value.to_string()))
        .to_vec(),
        ..timed("roadmap", "Q4 roadmap review", THU, (10, 0), (11, 30))
            .at("Room Fjord")
            .meet("q4-roadmap")
            .asked(IMANI, (ME_WORK, nothing), &[(LARS, yes), (MEI, no), (DMITRI, nothing)])
            .reminded()
            .notes(
                "The pre-read is the roadmap document from last week. Please answer by Wednesday so I can book the \
                 right room.\n\n* What shipped in Q3 and what slipped\n* The three bets for Q4\n* Headcount, once the \
                 budget is in",
            )
    };
    let review = Event {
        links: vec!["https://bank.example.com.device-check.example.net/meet".to_string()],
        ..timed("review", "Account review", FRI, (9, 0), (9, 30)).at("bank.example.com").asked(BANK, (ME, nothing), &[])
    };
    let kept = |account, local, read, events| Kept { account, local, read, events };
    vec![
        kept(
            PERSONAL,
            false,
            Some(120),
            vec![
                timed("climbing", "Climbing", day(11, 17), (18, 0), (20, 0)).at("the gym on Mill street"),
                timed("dentist", "Dentist", day(11, 24), (11, 0), (11, 45)).at("Kirkegata 12"),
                review,
                all_day("cabin", "Cabin weekend", FRI, 3).at("Ridge saddle hut"),
                timed("bookclub", "Book club", day(12, 3), (19, 0), (21, 0)).at("at Greta's"),
            ],
        ),
        kept(
            PERSONAL,
            true,
            None,
            vec![
                all_day("library", "Library books due", FRI, 1),
                timed("swap", "Gear swap", DEC_5, (10, 0), (13, 0)).at("Fernway hall"),
            ],
        ),
        kept(
            WORK,
            false,
            Some(20),
            vec![
                standup,
                timed("flames", "Lunch and learn: reading flame graphs", day(11, 20), (12, 0), (13, 0)).asked(
                    DMITRI,
                    (ME_WORK, yes),
                    &[],
                ),
                timed("halden-call", "Halden contract call", day(11, 23), (15, 0), (16, 0))
                    .meet("halden")
                    .asked(PRIYA, (ME_WORK, yes), &[(DMITRI, yes)])
                    .reminded()
                    .notes(
                        "Payment terms and the support clause. Draft v3 is attached to Priya's mail of this morning.",
                    ),
                timed("one-to-one", "1:1 with Imani", day(11, 25), (13, 0), (13, 30)).asked(IMANI, (ME_WORK, yes), &[]),
                roadmap,
                timed("vendor", "Vendor call", THU, (11, 0), (12, 0)).meet("vendor").asked(MEI, (ME_WORK, maybe), &[]),
                all_day("oncall", "On call", day(12, 7), 7),
            ],
        ),
        kept(
            WORK,
            true,
            None,
            vec![timed("migration", "Staging migration window", THU, (20, 0), (23, 0)).asked(
                LARS,
                (ME_WORK, yes),
                &[],
            )],
        ),
    ]
}

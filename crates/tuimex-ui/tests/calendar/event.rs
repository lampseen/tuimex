//! The event screen's outputs: `cal-event`, `cal-event-80-ascii`,
//! `cal-event-more`, `cal-event-answer`, `cal-event-local`, and the step of
//! `cal-zoom` that is the event, `cal-zoom-2`. Each is Enter on Thursday,
//! the event chosen with `j`, and Enter again.

use tuimex_fixtures::design as fx;
use tuimex_ui::app::App;
use tuimex_ui::span::Line;

use super::{DEFAULT, TO_THURSDAY, both, calendar, check, cut, over};

pub const DRAWN: &[&str] =
    &["cal-event", "cal-event-80-ascii", "cal-event-more", "cal-event-answer", "cal-event-local", "cal-zoom-2"];

/// The design's calendar, knowing the mail the roadmap's invitation came
/// in: the design's `mail: 'invite'`, which is what offers `v`.
fn typed(keys: &str) -> App {
    let mut calendar = calendar();
    let invite = fx::inbox().into_iter().find(|msg| msg.envelope.id.key == "invite").map(|msg| msg.envelope.id);
    calendar.came_in.extend(invite.map(|id| ("roadmap@design.example.net".to_string(), id)));
    over(calendar, keys)
}

/// The roadmap review, the second event on Thursday.
fn roadmap(more: &str) -> App {
    typed(&format!("{TO_THURSDAY}\rj\r{more}"))
}

pub fn draw(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    Some(match name.strip_suffix("-80-ascii").unwrap_or(name) {
        "cal-event" => roadmap("").lines(width, height),
        "cal-event-more" => roadmap("zo").lines(width, height),
        "cal-event-answer" => roadmap("i").lines(width, height),
        "cal-event-local" => typed(&format!("{TO_THURSDAY}\rjjj\r")).lines(width, height),
        "cal-zoom-2" => cut(roadmap("").lines(120, 40), 12),
        _ => return None,
    })
}

#[test]
fn cal_event() {
    both("cal-event");
    check(DEFAULT, "cal-event-80-ascii");
    both("cal-event-more");
    both("cal-event-answer");
    both("cal-event-local");
}

/// The step of the flow that is the event's screen.
#[test]
fn cal_event_steps() {
    both("cal-zoom-2");
}

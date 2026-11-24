//! The event's own screen: what is open, and the keys of the `event`
//! table. The event shown is always the selected one, so `J`, `K` and `I`
//! change it without leaving the screen.

use super::key_name;
use crate::app::App;
use crate::calendar::Entry;
use crate::calendar::parts::Scene;
use crate::say;
use crate::span::Line;

#[derive(Debug, Default)]
pub(in crate::app) struct State {
    open: bool,
}

/// The keys `event.actions` names, as the calendar's own bindings: each
/// does there what it does here.
const ACTIONS: [(&str, usize); 7] = [
    ("calendar.links", 0),
    ("calendar.answer", 0),
    ("calendar.edit", 0),
    ("calendar.delete", 0),
    ("calendar.event", 0),
    ("calendar.event", 1),
    ("calendar.waiting", 0),
];

impl App {
    pub(super) fn event_is_open(&self) -> bool {
        self.cal.event.open
    }

    /// Enter on the selected event.
    pub(super) fn event_open(&mut self) {
        self.cal.event.open = true;
    }

    /// Esc. Says whether there was a screen to close.
    pub(super) fn event_close(&mut self) -> bool {
        std::mem::take(&mut self.cal.event.open)
    }

    /// A key of the `event` table; `i` is which of the binding's keys.
    pub(super) fn event_run(&mut self, id: &str, i: usize) {
        match ACTIONS.get(i).filter(|_| id == "event.actions") {
            Some((action, which)) => self.place_run(action, *which),
            None => self.say(say::not_built(key_name(id, i), "the calendar's event view")),
        }
    }

    pub(super) fn event_lines(&self, o: &Scene, entry: &Entry, width: usize, height: usize) -> Vec<Line> {
        crate::calendar::event::screen(o, entry, width, height)
    }
}

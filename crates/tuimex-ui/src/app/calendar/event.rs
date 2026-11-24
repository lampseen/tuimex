//! The event's own screen: what is open, and the keys of the `event`
//! table. The event shown is always the selected one, so `J`, `K` and `I`
//! change it without leaving the screen.

use std::cell::Cell;

use super::key_name;
use crate::app::App;
use crate::calendar::Entry;
use crate::calendar::event::{self, At};
use crate::calendar::parts::Scene;
use crate::say;
use crate::span::Line;

/// The scroll is kept to what the last drawing allowed, which only the
/// drawing knows: the screen is drawn through `&self`, so what it learns
/// goes in cells.
#[derive(Debug, Default)]
pub(in crate::app) struct State {
    open: bool,
    more: bool,
    top: Cell<usize>,
    last_top: Cell<usize>,
    room: Cell<usize>,
}

impl State {
    /// The event as it first shows: from its top, with More folded.
    fn rewind(&mut self) {
        self.more = false;
        self.top.set(0);
    }
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
        self.cal.event.rewind();
        self.cal.event.open = true;
    }

    /// Esc. Says whether there was a screen to close.
    pub(super) fn event_close(&mut self) -> bool {
        std::mem::take(&mut self.cal.event.open)
    }

    /// A key of the `event` table; `i` is which of the binding's keys.
    pub(super) fn event_run(&mut self, id: &str, i: usize) {
        let state = &mut self.cal.event;
        match id {
            "event.scroll" => {
                let (top, last_top) = (state.top.get(), state.last_top.get());
                if last_top == 0 {
                    let said = format!("{}: all of the event is on the screen", key_name(id, i));
                    return self.say(say::plain(said));
                }
                let half = (state.room.get() / 2).max(1);
                state.top.set(match i {
                    0 => (top + 1).min(last_top),
                    1 => top.saturating_sub(1),
                    2 => (top + half).min(last_top),
                    3 => top.saturating_sub(half),
                    4 => 0,
                    _ => last_top,
                });
            }
            "event.fold" => {
                let has_more = self.picked_entry().is_some_and(|entry| !entry.event.more.is_empty());
                if !has_more {
                    let said = format!("{}: this event has nothing under More", key_name(id, i));
                    return self.say(say::err(said));
                }
                self.cal.event.more = i == 0;
            }
            "event.mail" => self.event_mail(key_name(id, i)),
            _ => {
                let Some((action, which)) = ACTIONS.get(i).filter(|_| id == "event.actions") else { return };
                let before = self.cal.picked.clone();
                self.place_run(action, *which);
                // Another event is shown from its own top.
                if self.cal.picked != before {
                    self.cal.event.rewind();
                }
            }
        }
    }

    /// `v`: the mail the event came in, in the reader. The calendar is
    /// left as it is, so `gc` returns to the event.
    fn event_mail(&mut self, key: &str) {
        let came_in = |entry: Entry| {
            let known = self.cal.model.came_in.iter().find(|(uid, _)| *uid == entry.event.uid);
            known.map(|(_, id)| id.clone())
        };
        match self.picked_entry().and_then(came_in) {
            None => self.say(say::err(format!("{key}: this event did not come by mail"))),
            Some(id) if !self.mailbox.mail.iter().any(|m| m.envelope.id == id) => {
                self.say(say::err(format!("{key}: the mail it came in is no longer here")));
            }
            Some(id) => {
                self.calendar_close();
                self.read(id, None);
            }
        }
    }

    pub(super) fn event_lines(&self, o: &Scene, entry: &Entry, width: usize, height: usize) -> Vec<Line> {
        let state = &self.cal.event;
        let drawn = event::screen(o, entry, At { top: state.top.get(), more: state.more }, width, height);
        state.top.set(drawn.top);
        state.last_top.set(drawn.last_top);
        state.room.set(drawn.room);
        drawn.lines
    }
}

#[cfg(test)]
mod tests;

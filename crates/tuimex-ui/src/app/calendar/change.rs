//! What changes the calendar and what takes a change back: `d`, and `u`
//! (ADR-015, ADR-055).

use super::key_name;
use crate::app::App;
use crate::say;

impl App {
    /// `d` on the selected event.
    pub(super) fn event_delete(&mut self) {
        let key = key_name("calendar.delete", 0);
        match self.picked_entry() {
            None => self.say(say::err(format!("{key}: no event is selected"))),
            Some(_) => self.say(say::not_built(key, "deleting an event")),
        }
    }

    /// `u` in the calendar. What was done to mail is mail's to take back,
    /// from mail, and nothing done here can be yet.
    pub(super) fn calendar_undo(&mut self) {
        self.say(say::err("undo: nothing to undo"));
    }
}

//! The form for a new event or a change to one: whether it is open, and
//! the keys of the `event-form` table. While it is open no other table is
//! read, so a letter is a letter typed.

use super::key_name;
use crate::app::App;
use crate::calendar::parts::Scene;
use crate::keymap::Key;
use crate::say;
use crate::span::Line;

#[derive(Debug, Default)]
pub(in crate::app) struct State {
    open: bool,
}

impl App {
    pub(super) fn form_is_open(&self) -> bool {
        self.cal.form.open
    }

    /// `c` and `:event [title]` for a new event on the cursor's day, `E`
    /// for the selected one, which has to be there.
    pub(super) fn form_open(&mut self, edit: bool, _title: &str) {
        if edit && self.picked_entry().is_none() {
            return self.say(say::err(format!("{}: no event is selected", key_name("calendar.edit", 0))));
        }
        self.cal.form.open = true;
    }

    /// A key of the `event-form` table; `i` is which of the binding's keys.
    pub(super) fn form_run(&mut self, id: &str, i: usize) {
        match id {
            "event-form.close" => self.cal.form.open = false,
            _ => self.say(say::not_built(key_name(id, i), "the calendar's event form")),
        }
    }

    /// A key the form's table does not name, which is what a field takes
    /// as typing. There is no field for it to reach until the form draws
    /// them.
    pub(super) fn form_typed(&mut self, _key: Key) {}

    pub(super) fn form_lines(&self, o: &Scene, width: usize, height: usize) -> Vec<Line> {
        crate::calendar::form::screen(o, "New event", width, height)
    }
}

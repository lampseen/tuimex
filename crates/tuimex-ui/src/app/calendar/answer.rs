//! Answering an invitation from the calendar (ADR-057): the question `i`
//! asks of the selected event, which is the one the reader asks of an
//! invitation's mail, and what each answer does.

use super::{Panel, key_name};
use crate::app::App;
use crate::say;

impl App {
    /// `i`: the question, of an event somebody asked the reader to.
    pub(super) fn answer_ask(&mut self) {
        let key = key_name("calendar.answer", 0);
        match self.picked_entry().map(|entry| entry.standing.is_some()) {
            None => self.say(say::err(format!("{key}: no event is selected"))),
            Some(false) => self.say(say::err(format!("{key}: nobody asked for an answer to this event"))),
            Some(true) => self.cal.panel = Panel::Answer,
        }
    }

    /// A key of the `invite` table, pressed at the question.
    pub(super) fn answer_run(&mut self, _id: &str) {
        self.say(say::not_built(":invite", "invitations (M10)"));
    }

    /// `:invite <answer>` while the calendar is open.
    pub(super) fn answer_command(&mut self, _answer: &str) {
        self.say(say::not_built(":invite", "invitations (M10)"));
    }
}

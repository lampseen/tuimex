//! Opening one of an event's links (ADR-018, ADR-054). The panel that
//! lists them, with the host each really opens, is the calendar's; what
//! happens when one is chosen is here.

use crate::app::App;
use crate::say;

impl App {
    /// Enter or a digit over the links: link `n` of the selected event,
    /// counted from 1, which the panel has checked is there.
    pub(super) fn event_link_open(&mut self, _n: usize) {
        self.say(say::not_built("o", "joining a meeting from the calendar"));
    }

    /// `A` over the links: the agent's reading of the one chosen.
    pub(super) fn event_link_agent(&mut self) {
        self.say(say::not_built("A", "the agent's reading of an event's link"));
    }
}

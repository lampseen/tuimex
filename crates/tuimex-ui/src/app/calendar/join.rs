//! Opening one of an event's links (ADR-018, ADR-054). The panel that
//! lists them, with the host each really opens, is the calendar's; what
//! happens when one is chosen is here, and is what happens to a link of a
//! message: the same assessment, the same question, the same agent.

use tuimex_core::calendar::links_of;
use tuimex_core::{Action, EventId, Start};

use super::change::named;
use super::{Panel, key_name};
use crate::app::read::{Advised, question};
use crate::app::{App, Mode};
use crate::reader::{self, Look};
use crate::say;

#[cfg(test)]
mod tests;

impl App {
    /// Link `n` of the selected event, counted from 1, and what is known
    /// of it before it opens.
    fn event_link(&self, n: usize) -> Option<(Box<EventId>, String, Look)> {
        let entry = self.picked_entry()?;
        let link = links_of(entry.event).into_iter().find(|link| link.n == n)?;
        Some((Box::new(named(&entry)), link.url.clone(), reader::look(&link, &[])))
    }

    /// Enter or a digit over the links: refused when it is neither web
    /// nor mail, confirmed first when anything about it deceives, and
    /// otherwise opened with its real address on the status line.
    pub(super) fn event_link_open(&mut self, n: usize) {
        let key = key_name("calendar.links", 0);
        let Some((event, url, look)) = self.event_link(n) else { return };
        self.cal.panel = Panel::None;
        if look.blocked {
            let kind = if look.scheme.is_empty() {
                "a link to no host is".to_string()
            } else {
                format!("{} links are", look.scheme)
            };
            self.say(say::err(format!("{key} {n}: {kind} not opened")).then("only http, https and mailto"));
        } else if look.mailto {
            // Written for review in mail, never sent from here (ADR-008).
            self.calendar_close();
            self.compose(Start::MailTo(url));
        } else if look.warning.is_some() {
            self.mode = Mode::Confirm(question(&look, self.agent), Action::OpenEventLink(event, n));
        } else {
            self.event_link_confirmed(Action::OpenEventLink(event, n));
        }
    }

    /// The link is to open: asked for outright, or `y` at its question.
    pub(in crate::app) fn event_link_confirmed(&mut self, action: Action) {
        let Action::OpenEventLink(_, n) = &action else { return };
        let Some((_, _, look)) = self.event_link(*n) else { return };
        self.outside(action);
        self.cal.joining = Some(look.url);
        self.say(say::plain(format!("Opening {}", look.host)));
    }

    /// `A` over the links, and at the question a deceptive one asks: the
    /// agent's reading of link `n`, which joins the question when it comes
    /// (ADR-031). The agent opens nothing.
    pub(in crate::app) fn event_link_asked(&mut self, n: usize) {
        let Some((event, _, look)) = self.event_link(n) else { return };
        if !self.agent {
            return self.agent_menu();
        }
        let serial = self.ask(Action::AgentEventLink(event.clone(), n), Vec::new());
        (self.cal.panel, self.cal.advised) = (Panel::Links(n - 1), Advised::asking(serial));
        self.mode = Mode::Confirm(question(&look, false), Action::OpenEventLink(event, n));
    }

    /// `A` over the links: the agent's reading of the one chosen.
    pub(super) fn event_link_agent(&mut self) {
        if let Panel::Links(at) = self.cal.panel {
            self.event_link_asked(at + 1);
        }
    }
}

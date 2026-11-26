//! The pointer in the calendar (ADR-065). A press selects what it is on
//! and on what is already selected is Enter; the wheel is `j` and `k`. Each
//! goes through the path the key takes, so the keys stay the one account of
//! what the calendar does (ADR-054).

use super::{Panel, View};
use crate::app::App;
use crate::app::mouse::NOTCH;
use crate::spot::Target;

impl App {
    pub(in crate::app) fn calendar_clicked(&mut self, target: Target) {
        // What a key clears before it is read.
        (self.said, self.fix, self.cal.joining) = (None, None, None);
        match (target, self.cal.panel) {
            _ if self.form_is_open() => self.form_clicked(target),
            (Target::CalEntry(at), Panel::Calendars(on)) if at == on => self.sources_run("calendars.toggle", 0),
            (Target::CalEntry(at), Panel::Calendars(_)) => self.cal.panel = Panel::Calendars(at),
            (Target::CalEntry(at), Panel::Links(on)) if at == on => self.event_links_run("links.choose", 2),
            (Target::CalEntry(at), Panel::Links(_)) => self.cal.panel = Panel::Links(at),
            // The event's screen takes none of the calendar's keys, and a
            // panel or a question takes them all.
            _ if self.event_is_open() || self.cal.panel != Panel::None => {}
            (Target::CalView(view), _) if view != self.cal.view => {
                let at = View::ALL.iter().position(|other| *other == view).unwrap_or(0);
                self.place_run("calendar.view-number", at);
            }
            (Target::Day(day), _) => self.day_clicked(day),
            (Target::Occasion(at), _) => {
                let Some(picked) = self.cal.shown.get(at).cloned() else { return };
                if !self.on_month() && self.cal.picked.as_ref() == Some(&picked) {
                    return self.place_run("calendar.open", 0);
                }
                // From the grid the focus goes to the list first, as Alt-l
                // takes it there.
                if self.on_month() {
                    (self.cal.focus, self.cal.listed_from) = (true, Some(self.day()));
                }
                self.pick(picked);
            }
            _ => {}
        }
    }

    /// A day in the month's grid or at the head of the week's column. The
    /// day the cursor is on is selected while no event is.
    fn day_clicked(&mut self, day: i64) {
        let selected = day == self.day() && self.picked_entry().is_none();
        match self.cal.view {
            View::Month if selected && !self.cal.focus => self.place_run("calendar.open", 0),
            View::Month => {
                self.place_run("calendar.focus-grid", 0);
                self.go(day);
            }
            _ if selected => self.place_run("calendar.open", 0),
            _ if day != self.day() => self.go(day),
            _ => {}
        }
    }

    /// A field of the form takes the focus as Tab brings it there, and the
    /// one that has it is left as it is.
    fn form_clicked(&mut self, target: Target) {
        let (Target::FormField(to), Some(at)) = (target, self.form_field()) else { return };
        let fields = crate::calendar::form::FIELDS.len();
        (0..(to + fields - at) % fields).for_each(|_| self.form_run("event-form.field", 0));
    }

    /// A notch of the wheel over the month's grid is a week, in a list of
    /// events or the week an event, and on the event's screen `NOTCH`
    /// lines. Over the part of the month without the focus it is nothing,
    /// as the same key would move the other part.
    pub(in crate::app) fn calendar_wheeled(&mut self, over: Option<Target>, down: bool) {
        (self.said, self.fix, self.cal.joining) = (None, None, None);
        if self.form_is_open() || self.cal.panel != Panel::None {
            return;
        }
        let i = usize::from(!down);
        if self.event_is_open() {
            return (0..NOTCH).for_each(|_| self.event_run("event.scroll", i));
        }
        let month = self.cal.view == View::Month && !self.cal.searching;
        match over {
            Some(Target::CalGrid | Target::Day(_)) if self.on_month() => self.place_run("calendar.move", i),
            Some(Target::CalList | Target::Occasion(_)) if self.in_list() => self.place_run("calendar.move", i),
            _ if !month => self.place_run("calendar.move", i),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;

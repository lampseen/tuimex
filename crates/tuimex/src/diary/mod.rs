//! The loop's side of the calendar: each calendar the worker sends is
//! handed to the interface, and the same events are looked at once a
//! second for a reminder that has fallen due (ADR-058).
//!
//! Nothing here waits (ADR-006). A calendar arrives already built, and a
//! look at the clock is arithmetic over events held in memory.

use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;

use tuimex_core::calendar::{Event, Standing, Zone, standing};
use tuimex_core::reminder::Reminder;
use tuimex_core::{AccountId, Event as Told, Timestamp};
use tuimex_ui::app::App;
use tuimex_ui::calendar::Calendar;

use crate::notify::{Reminders, Tick};

#[cfg(test)]
mod tests;

/// The events of the calendar last handed over, as reminders are asked
/// about them. The interface owns the calendar it was given and offers no
/// way to read it back, so what reminders need of it is kept beside it.
#[derive(Default)]
struct Held {
    /// Every shelf's events, one after another.
    events: Vec<Event>,
    /// For each of `events`, whether the account whose shelf held it has
    /// declined it.
    declined: Vec<bool>,
    zones: BTreeMap<String, Zone>,
    zone: Zone,
}

impl Held {
    fn of(calendar: &Calendar) -> Held {
        let mut held = Held { zones: calendar.zones.clone(), zone: calendar.zone.clone(), ..Held::default() };
        for shelf in &calendar.shelves {
            let mine = calendar.identities.iter().filter(|identity| identity.account == shelf.account);
            let mine: Vec<&str> = mine.map(|identity| identity.address.email.as_str()).collect();
            for event in &shelf.events {
                held.events.push(event.clone());
                held.declined.push(standing(event, &mine) == Some(Standing::Declined));
            }
        }
        held
    }

    /// Whether `event`, which is one of `events`, was declined. It is
    /// known by where it is and not by what it says: two accounts asked
    /// to one meeting hold events that are equal, and one may have
    /// declined what the other accepted (ADR-057).
    fn declined(&self, event: &Event) -> bool {
        let from = std::ptr::from_ref(event).addr().checked_sub(self.events.as_ptr().addr());
        let at = from.map(|bytes| bytes / size_of::<Event>());
        at.and_then(|at| self.declined.get(at)).copied().unwrap_or(false)
    }
}

pub struct Diary {
    arriving: Receiver<Calendar>,
    /// `None` where nothing reminds: `--fixtures`, which has no clock.
    reminders: Option<Reminders>,
    held: Held,
    /// Whether a calendar has been handed over. Until one has, the clock
    /// is not looked at: the first look raises what fell due just before
    /// it, and that look has to have the events to find it in.
    handed: bool,
    /// The second the clock was last looked at.
    looked: Option<i64>,
    /// What fell due at the last look that found anything.
    due: Vec<Reminder>,
}

/// Says `text` on the command line, as the worker says that the store
/// could not be read.
fn say(app: &mut App, text: String) {
    app.event(Told::Notice { account: AccountId(String::new()), error: true, text, hint: String::new() });
}

impl Diary {
    pub fn new(arriving: Receiver<Calendar>, reminders: Option<Reminders>) -> Diary {
        Diary { arriving, reminders, held: Held::default(), handed: false, looked: None, due: Vec::new() }
    }

    /// Before the first frame: the switch as it was last left, and any
    /// calendar that is already waiting.
    pub fn begin(&mut self, app: &mut App) {
        if let Some(reminders) = &self.reminders {
            app.set_reminders(reminders.is_on());
        }
        self.hand(app);
    }

    /// Hands the interface the newest calendar that has arrived; whether
    /// one had.
    pub fn hand(&mut self, app: &mut App) -> bool {
        let Some(calendar) = self.arriving.try_iter().last() else { return false };
        (self.held, self.handed) = (Held::of(&calendar), true);
        app.set_calendar(calendar);
        true
    }

    /// After a key: when it turned reminders off or on, the choice is
    /// kept for the next run. One that could not be kept holds for this
    /// run, and the command line says so.
    pub fn switched(&mut self, app: &mut App) {
        let Some(reminders) = self.reminders.as_mut() else { return };
        if app.reminders() != reminders.is_on()
            && let Err(why) = reminders.set(app.reminders())
        {
            say(app, why);
        }
    }

    /// Looks at the clock, once in any second: what has fallen due is
    /// raised by the notifier and kept, and a notifier that cannot raise
    /// one is said once. Whether anything was said.
    pub fn tick(&mut self, app: &mut App, now: Timestamp) -> bool {
        let Some(reminders) = self.reminders.as_mut().filter(|_| self.handed) else { return false };
        if self.looked.replace(now.0) == Some(now.0) {
            return false;
        }
        let held = &self.held;
        let Tick { due, failure } =
            reminders.tick(now.0, &held.events, &held.zones, &held.zone, &|event| held.declined(event));
        if !due.is_empty() {
            self.due = due;
        }
        let said = failure.is_some();
        failure.into_iter().for_each(|why| say(app, why));
        said
    }
}

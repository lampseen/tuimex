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
use tuimex_ui::calendar::{Calendar, Due};
use tuimex_ui::say;

use crate::calendar::Handed;
use crate::notify::{Reminders, Tick};

#[cfg(test)]
mod tests;

/// The events of the calendar last handed over, as reminders are asked
/// about them: one run of events, where the interface keeps a shelf for
/// each calendar. What the reader has hidden or ignored since is asked of
/// the interface at each look.
#[derive(Default)]
struct Held {
    /// Every shelf's events, one after another.
    events: Vec<Event>,
    /// For each of `events`, whether the account whose shelf held it has
    /// declined it.
    declined: Vec<bool>,
    /// For each of `events`, the calendar whose shelf held it: the
    /// account, and whether it is its local one.
    of: Vec<(AccountId, bool)>,
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
                held.of.push((shelf.account.clone(), shelf.local));
            }
        }
        held
    }

    /// Where `event`, which is one of `events`, is among them. It is
    /// known by where it is and not by what it says: two accounts asked
    /// to one meeting hold events that are equal, and one may have
    /// declined what the other accepted (ADR-057).
    fn at(&self, event: &Event) -> Option<usize> {
        let from = std::ptr::from_ref(event).addr().checked_sub(self.events.as_ptr().addr());
        from.map(|bytes| bytes / size_of::<Event>()).filter(|at| *at < self.events.len())
    }

    /// Whether the event at `at` reminds nobody: it was declined, its
    /// calendar is hidden, or it is an invitation the reader ignored.
    fn silent(&self, at: usize, shown: &Calendar) -> bool {
        let (Some(declined), Some((account, local))) = (self.declined.get(at), self.of.get(at)) else { return true };
        let ignored = |uid: &String| shown.ignored.iter().any(|(whose, ignored)| whose == account && ignored == uid);
        *declined || shown.is_hidden(account, *local) || self.events.get(at).is_some_and(|event| ignored(&event.uid))
    }
}

pub struct Diary {
    arriving: Receiver<Handed>,
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
    /// Something fell due since the bell last rang.
    ring: bool,
    /// The calendars the last one handed over could not read, and why,
    /// so that each failure is noted once and not at every reading.
    failed: Vec<(AccountId, String)>,
}

/// Says `text` on the command line, as the worker says that the store
/// could not be read.
fn say(app: &mut App, text: String) {
    app.event(Told::Notice { account: AccountId(String::new()), error: true, text, hint: String::new() });
}

impl Diary {
    pub fn new(arriving: Receiver<Handed>, reminders: Option<Reminders>) -> Diary {
        Diary {
            arriving,
            reminders,
            held: Held::default(),
            handed: false,
            looked: None,
            due: Vec::new(),
            ring: false,
            failed: Vec::new(),
        }
    }

    /// Whether something fell due that the next frame shows and the bell
    /// then rings for (ADR-058).
    pub fn rings(&self) -> bool {
        self.ring
    }

    /// Whether the bell is to ring now: asked after a frame, and true
    /// once for each look that found something due.
    pub fn rang(&mut self) -> bool {
        std::mem::take(&mut self.ring)
    }

    /// Before the first frame: the switch as it was last left, and any
    /// calendar that is already waiting.
    pub fn begin(&mut self, app: &mut App) {
        if let Some(reminders) = &self.reminders {
            app.set_reminders(reminders.is_on());
        }
        self.hand(app);
    }

    /// Hands the interface what has arrived: the event of each message
    /// read, and the newest calendar, with how far the asks it was read
    /// after had come. Why a calendar could not be read goes to
    /// `:messages` when it is first said. Whether anything had arrived.
    pub fn hand(&mut self, app: &mut App) -> bool {
        let (mut newest, mut any) = (None, false);
        for handed in self.arriving.try_iter() {
            any = true;
            match handed {
                Handed::Invited(id, invited) => app.set_invited(id, invited.map(|invited| *invited)),
                Handed::Calendar { calendar, carried } => newest = Some((*calendar, carried)),
            }
        }
        let Some((calendar, carried)) = newest else { return any };
        let failures = calendar.shelves.iter().filter_map(|shelf| Some((shelf.account.clone(), shelf.failed.clone()?)));
        let mut failed: Vec<(AccountId, String)> = failures.collect();
        failed.dedup();
        for (account, reason) in failed.iter().filter(|failure| !self.failed.contains(failure)) {
            app.note(calendar.now, say::err(format!("calendar: {}: {reason}", account.0)));
        }
        (self.held, self.handed, self.failed) = (Held::of(&calendar), true, failed);
        app.calendar_carried(carried);
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
        let (held, shown) = (&self.held, app.calendar());
        let silent = |event: &Event| held.at(event).is_none_or(|at| held.silent(at, shown));
        let Tick { due, failure } = reminders.tick(now.0, &held.events, &held.zones, &held.zone, &silent);
        // The status line has room for one: the one that begins first,
        // in the first calendar that holds it and reminds of it.
        let first = due.iter().min_by_key(|reminder| reminder.start).and_then(|reminder| {
            let at =
                (0..held.events.len()).find(|&at| held.events[at].uid == reminder.uid && !held.silent(at, shown))?;
            let (account, uid, summary) = (held.of[at].0.clone(), reminder.uid.clone(), reminder.summary.clone());
            Some(Due { account, uid, summary, start: Timestamp(reminder.start) })
        });
        if !due.is_empty() {
            self.due = due;
        }
        let said = failure.is_some();
        if first.is_some() {
            self.ring = true;
            app.set_reminder(first);
        }
        failure.into_iter().for_each(|why| say(app, why));
        said
    }
}

//! What changes the calendar and what takes a change back (ADR-015,
//! ADR-040, ADR-055). A change is made of the calendar held here at once
//! and handed on as an `Ask`; until whoever keeps the calendars has carried
//! it out, it is made again of each calendar read, which is older than it.
//! `u` and a refusal put back what it changed. Every kind of change, an
//! answer, a delete or a save, goes through `change` and is taken back the
//! same way.

use tuimex_core::calendar::{DAY, Event, Repeat, occurrence_at};
use tuimex_core::{Action, EventId};

use super::{Panel, Which, key_name};
use crate::app::App;
use crate::calendar::change::{Item, Patch};
use crate::calendar::form::Target;
use crate::calendar::parts::day_short;
use crate::calendar::{Entry, Picked, Shelf};
use crate::glyphs::DOT;
use crate::say;

#[cfg(test)]
mod tests;

/// Something done to the calendar.
#[derive(Clone, Debug)]
struct Change {
    serial: u64,
    /// The key or command that did it, as a refusal names it.
    name: String,
    did: Patch,
    back: Patch,
    /// What is true of the calendar once it is taken back, as `u` and a
    /// refusal say it.
    restored: String,
    /// Done from the reader, where `u` in mail takes it back; `u` in the
    /// calendar takes back what was done in the calendar (ADR-054).
    in_mail: bool,
    cursor: (Option<i64>, Option<Picked>),
    /// `u` has taken it back, so a refusal of it has nothing to put back.
    undone: bool,
}

#[derive(Debug, Default)]
pub(in crate::app) struct Changes {
    /// Asked for and not known to be carried out, oldest first.
    unsettled: Vec<Change>,
    /// What `u` reverses, newest last.
    undo: Vec<Change>,
}

/// An event's title, as a result line names it.
pub(super) fn title(event: &Event) -> String {
    let flat = event.summary.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() { "(no title)".to_string() } else { flat }
}

/// An entry's event as an ask names it, whole.
pub(super) fn named(entry: &Entry) -> EventId {
    let (account, uid) = (entry.account.clone(), entry.event.uid.clone());
    EventId { account, local: entry.local, uid, occurrence: entry.event.instead_of.clone() }
}

impl App {
    /// Every calendar ask up to the serial `after` has been carried out,
    /// and the calendars handed over from now on hold what each did.
    /// Whoever keeps the calendars says so before handing over the first
    /// that does.
    pub fn calendar_carried(&mut self, after: u64) {
        self.cal.changes.unsettled.retain(|change| change.serial > after);
    }

    /// Makes what is not yet carried out of the calendar just read. The
    /// cursor stays on an event made here when the copy read is its
    /// keeper's, under the UID the keeper gave it.
    pub(super) fn changes_again(&mut self) {
        let (changes, model) = (&self.cal.changes, &mut self.cal.model);
        for change in &changes.unsettled {
            model.patch(&change.did, true);
        }
        if let Some(picked) = self.cal.picked.as_mut().filter(|picked| model.find(picked, None).is_none()) {
            let made = changes.undo.iter().chain(&changes.unsettled);
            let stored = made.filter_map(|change| model.stored_as(&change.did, &picked.uid)).next();
            picked.uid = stored.map_or_else(|| picked.uid.clone(), str::to_string);
        }
    }

    /// Makes `did` of the calendar at once, hands `action` on, and keeps
    /// what takes it back. `restored` completes "Undone ·".
    pub(super) fn change(&mut self, name: &str, action: Action, did: Patch, restored: String, in_mail: bool) {
        let cursor = (self.cal.cursor, self.cal.picked.clone());
        let back = self.cal.model.patch(&did, false);
        let serial = self.ask(action, Vec::new());
        let change = Change { serial, name: name.to_string(), did, back, restored, in_mail, cursor, undone: false };
        self.cal.changes.unsettled.push(change.clone());
        self.cal.changes.undo.push(change);
    }

    /// The serial of the newest change `u` in mail would take back.
    pub(in crate::app) fn answered_in_mail(&self) -> u64 {
        self.cal.changes.undo.iter().rev().find(|change| change.in_mail).map_or(0, |change| change.serial)
    }

    /// `u`: the last change made in the calendar, or with `in_mail` from
    /// the reader, is taken back here and whoever carried it out is asked
    /// to take it back too.
    pub(in crate::app) fn calendar_undo(&mut self, in_mail: bool) {
        let Some(at) = self.cal.changes.undo.iter().rposition(|change| change.in_mail == in_mail) else {
            return self.say(say::err("undo: nothing to undo"));
        };
        let change = self.cal.changes.undo.remove(at);
        let again = self.cal.model.patch(&change.back, false);
        if let Some(waiting) = self.cal.changes.unsettled.iter_mut().find(|c| c.serial == change.serial) {
            waiting.undone = true;
        }
        let serial = self.ask(Action::UndoEvent(change.serial), Vec::new());
        if !in_mail {
            (self.cal.cursor, self.cal.picked) = change.cursor.clone();
        }
        self.say(say::ok("Undone").then(change.restored));
        let undo = Change {
            serial,
            name: "undo".to_string(),
            did: change.back,
            back: again,
            restored: "nothing was taken back".to_string(),
            undone: false,
            ..change
        };
        self.cal.changes.unsettled.push(undo);
    }

    /// A refusal of the ask `serial`. When it is a change to the calendar,
    /// what that one change did is put back, `u` no longer reaches it, and
    /// the reason is said; whether it was one is returned.
    pub(super) fn change_refused(&mut self, serial: u64, reason: &str, hint: &str) -> bool {
        let changes = &mut self.cal.changes;
        let stacked = changes.undo.iter().position(|c| c.serial == serial).map(|at| changes.undo.remove(at));
        let waiting = changes.unsettled.iter().position(|c| c.serial == serial).map(|at| changes.unsettled.remove(at));
        let Some(change) = waiting.or(stacked) else { return false };
        if !change.undone {
            self.cal.model.patch(&change.back, false);
            let said = say::err(format!("{}: {reason}", change.name));
            let hint = if hint.is_empty() { String::new() } else { format!(" {DOT} {hint}") };
            self.say(said.then(format!("{}{hint}", change.restored)));
        }
        true
    }

    /// Whether an entry is one day of an event that has others: it
    /// repeats by a rule tuimex works out, or it stands in for one day of
    /// an event that does.
    fn repeats(&self, entry: &Entry) -> bool {
        let ruled = |event: &&Event| matches!(event.repeat, Some(Repeat::Rule(_))) && event.instead_of.is_none();
        self.all_of(entry).iter().any(ruled)
    }

    /// Every event of an entry's UID in the calendar that holds it: the
    /// event, and each one standing in for a day of it.
    fn all_of(&self, entry: &Entry) -> Vec<&Event> {
        let held = |shelf: &&Shelf| shelf.account == *entry.account && shelf.local == entry.local;
        let shelves = self.cal.model.shelves.iter().filter(held);
        shelves.flat_map(|shelf| &shelf.events).filter(|event| event.uid == entry.event.uid).collect()
    }

    /// `d` and `E` on the selected event: one that repeats is first asked
    /// which of its days is meant. Says whether there is an event to act
    /// on with nothing asked.
    fn unasked(&mut self, which: Which, key: &str) -> bool {
        match self.picked_entry().map(|entry| self.repeats(&entry)) {
            None => self.say(say::err(format!("{key}: no event is selected"))),
            Some(true) => self.cal.panel = Panel::Which(which),
            Some(false) => return true,
        }
        false
    }

    pub(super) fn event_delete(&mut self) {
        if self.unasked(Which::Delete, key_name("calendar.delete", 0)) {
            self.event_delete_which(false);
        }
    }

    pub(super) fn event_edit(&mut self) {
        if self.unasked(Which::Edit, key_name("calendar.edit", 0)) {
            self.form_open_which(false);
        }
    }

    /// The event the selected entry's day is of, standing alone: as the
    /// calendar writes that one occurrence, in place of the rule.
    pub(super) fn one_day(&self, entry: &Entry) -> Option<Event> {
        let model = &self.cal.model;
        if entry.event.instead_of.is_some() {
            return Some(entry.event.clone());
        }
        let start = occurrence_at(entry.event, &model.zones, &model.zone, entry.start)?;
        let length = entry.event.end.as_ref().map_or(0, |end| end.wall() - entry.event.start.wall());
        let end = entry.event.end.as_ref().map(|end| end.at_wall(start.wall() + length));
        let instead_of = Some(start.clone());
        Some(Event { start, end, instead_of, repeat: None, excluded: Vec::new(), ..entry.event.clone() })
    }

    /// Deletes the selected event: with `one`, the day selected alone,
    /// which the event that repeats then leaves out.
    pub(super) fn event_delete_which(&mut self, one: bool) {
        let Some(entry) = self.picked_entry() else { return };
        let (at, what) = (Target { account: entry.account.clone(), local: entry.local }, title(entry.event));
        let gone =
            |event: &Event| Item::Event { at: at.clone(), event: Box::new(event.clone()), held: false, unsaved: false };
        let day = self.one_day(&entry).and_then(|day| day.instead_of).filter(|_| one);
        let (items, said) = match &day {
            Some(day) => {
                let mut items: Vec<Item> =
                    entry.event.instead_of.is_some().then(|| gone(entry.event)).into_iter().collect();
                for event in self.all_of(&entry).into_iter().filter(|event| event.instead_of.is_none()) {
                    let mut event = Box::new(event.clone());
                    event.excluded.push(day.clone());
                    items.push(Item::Event { at: at.clone(), event, held: true, unsaved: false });
                }
                (items, format!("Deleted {what} on {}", day_short(entry.start.div_euclid(DAY))))
            }
            None => (self.all_of(&entry).into_iter().map(gone).collect(), format!("Deleted {what}")),
        };
        let action = Action::DeleteEvent(Box::new(EventId { occurrence: day, ..named(&entry) }));
        self.change(key_name("calendar.delete", 0), action, Patch(items), format!("{what} is back"), false);
        self.say(say::ok(said).key("u to undo"));
    }
}

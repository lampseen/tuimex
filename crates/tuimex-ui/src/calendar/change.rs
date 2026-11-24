//! A change to the calendar as the interface holds it, said as what each
//! thing it touches is to be afterwards and not as what to do to it. Said
//! that way it can be done again over a calendar read since, which may or
//! may not hold it yet, and doing it gives back the change that undoes it
//! (ADR-015, ADR-040).

use tuimex_core::AccountId;
use tuimex_core::calendar::Event;

use super::form::Target;
use super::{Calendar, Proposed, Shelf};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// The calendar holds `event`, or with `held` false no event of its UID
    /// standing in for the occurrence it does. `unsaved` is an event made
    /// here, whose UID its keeper has yet to give it: the copy read back
    /// is known by its title and its time.
    Event {
        at: Target,
        event: Box<Event>,
        held: bool,
        unsaved: bool,
    },
    /// The time the reader proposed for an event, or none.
    Proposed {
        account: AccountId,
        uid: String,
        start: Option<i64>,
    },
    Ignored {
        account: AccountId,
        uid: String,
        ignored: bool,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patch(pub Vec<Item>);

impl Calendar {
    /// Where an event is: the shelf, and its place there.
    fn slot(&self, at: &Target, same: impl Fn(&Event) -> bool) -> Option<(usize, usize)> {
        let of = |shelf: &&Shelf| shelf.account == at.account && shelf.local == at.local;
        self.shelves.iter().enumerate().filter(|(_, shelf)| of(shelf)).find_map(|(s, shelf)| {
            let e = shelf.events.iter().position(&same)?;
            Some((s, e))
        })
    }

    /// The UID its keeper gave an unsaved event `patch` put, once a
    /// calendar read since holds the keeper's copy.
    pub fn stored_as(&self, patch: &Patch, unsaved: &str) -> Option<&str> {
        patch.0.iter().find_map(|item| match item {
            Item::Event { at, event, unsaved: true, .. } if event.uid == unsaved => {
                let alike = |e: &Event| (&e.summary, &e.start, &e.end) == (&event.summary, &event.start, &event.end);
                self.slot(at, alike).map(|(s, e)| self.shelves[s].events[e].uid.as_str())
            }
            _ => None,
        })
    }

    /// Makes everything `patch` names what it says, and returns the patch
    /// that puts each back. `again` is for a calendar read since the change
    /// was first made: an unsaved event its keeper has since stored is then
    /// already there, under the keeper's UID, and is left as read.
    pub fn patch(&mut self, patch: &Patch, again: bool) -> Patch {
        let mut back = Vec::new();
        for item in &patch.0 {
            match item {
                Item::Event { at, event, held, unsaved } => {
                    let named = |e: &Event| e.uid == event.uid && e.instead_of == event.instead_of;
                    let alike =
                        |e: &Event| (&e.summary, &e.start, &e.end) == (&event.summary, &event.start, &event.end);
                    let (kept, at) = (*held, at.clone());
                    let stored = || self.slot(&at, alike).filter(|_| *unsaved && (again || !kept));
                    match (self.slot(&at, named).or_else(stored), kept) {
                        (Some((s, e)), true) if named(&self.shelves[s].events[e]) => {
                            let was = Box::new(std::mem::replace(&mut self.shelves[s].events[e], (**event).clone()));
                            back.push(Item::Event { at, event: was, held: true, unsaved: false });
                        }
                        (Some(_), true) => {}
                        (Some((s, e)), false) => {
                            let was = Box::new(self.shelves[s].events.remove(e));
                            back.push(Item::Event { at, event: was, held: true, unsaved: false });
                        }
                        (None, true) => {
                            let s = self.slot_shelf(&at);
                            self.shelves[s].events.push((**event).clone());
                            back.push(Item::Event { at, event: event.clone(), held: false, unsaved: *unsaved });
                        }
                        (None, false) => {}
                    }
                }
                Item::Proposed { account, uid, start } => {
                    let at = self.proposed.iter().position(|p| p.account == *account && p.uid == *uid);
                    let was = at.map(|at| self.proposed.remove(at).start);
                    self.proposed.extend(start.map(|start| Proposed {
                        account: account.clone(),
                        uid: uid.clone(),
                        start,
                    }));
                    back.push(Item::Proposed { account: account.clone(), uid: uid.clone(), start: was });
                }
                Item::Ignored { account, uid, ignored } => {
                    let at = self.ignored.iter().position(|(a, u)| a == account && u == uid);
                    back.push(Item::Ignored { account: account.clone(), uid: uid.clone(), ignored: at.is_some() });
                    match (at, ignored) {
                        (Some(at), false) => drop(self.ignored.remove(at)),
                        (None, true) => self.ignored.push((account.clone(), uid.clone())),
                        _ => {}
                    }
                }
            }
        }
        // Each is put back before the ones made before it.
        back.reverse();
        Patch(back)
    }

    /// The first shelf of that calendar, made when the account has none.
    fn slot_shelf(&mut self, at: &Target) -> usize {
        let found = self.shelves.iter().position(|shelf| shelf.account == at.account && shelf.local == at.local);
        found.unwrap_or_else(|| {
            let (account, local) = (at.account.clone(), at.local);
            self.shelves.push(Shelf { account, local, name: None, read: None, failed: None, events: Vec::new() });
            self.shelves.len() - 1
        })
    }
}

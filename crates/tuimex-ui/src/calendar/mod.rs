//! The calendar as the interface holds it (ADR-054): every calendar of
//! every account, in memory, and the days the views ask it for. Nothing
//! here reads a calendar or keeps one: whoever does hands the whole of it
//! over, as the mailbox is handed over.
//!
//! `parts` is what every view is drawn from; `month`, `week`, `day`,
//! `event` and `form` are the views, and none of them names another.

pub mod day;
pub mod event;
pub mod form;
pub mod month;
pub mod parts;
pub mod week;

use std::collections::BTreeMap;

use tuimex_core::calendar::{DAY, Event, Standing, Zone, occurrences_between, standing};
use tuimex_core::{AccountId, Identity, Timestamp};

#[cfg(test)]
pub(crate) mod tests;

/// One calendar as it is held: an account's own, read from the address the
/// account gave, or what tuimex alone holds for that account (ADR-055). An
/// account whose provider keeps several has a shelf for each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shelf {
    pub account: AccountId,
    pub local: bool,
    /// The provider's name for it, where it gave one.
    pub name: Option<String>,
    /// When it was last read whole. A local one is never read from
    /// anywhere.
    pub read: Option<Timestamp>,
    /// Why the last attempt to read it failed, when it did; its events are
    /// then as old as `read`.
    pub failed: Option<String>,
    pub events: Vec<Event>,
}

/// A time the reader proposed for an event, which it is shown as until the
/// organizer answers (ADR-057): the event, and the wall time proposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposed {
    pub account: AccountId,
    pub uid: String,
    pub start: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub now: Timestamp,
    /// The reader's own zone, which every time is shown in (ADR-061).
    pub zone: Zone,
    /// The zones the calendars defined and the system's, by name.
    pub zones: BTreeMap<String, Zone>,
    pub shelves: Vec<Shelf>,
    /// Who the accounts are, as `App::set_up` takes them: "you" among an
    /// event's guests is an identity of the account whose shelf holds it.
    pub identities: Vec<Identity>,
    /// The calendars switched off, each an account and whether it is the
    /// local one.
    pub hidden: Vec<(AccountId, bool)>,
    pub proposed: Vec<Proposed>,
    /// Invitations the reader ignored, by account and UID: they are not
    /// drawn and wait for no answer (ADR-057).
    pub ignored: Vec<(AccountId, String)>,
}

/// One event on one day, as a view draws it. An event that runs over
/// several days is an entry on each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<'a> {
    pub event: &'a Event,
    pub account: &'a AccountId,
    pub local: bool,
    pub calendar: Option<&'a str>,
    pub day: i64,
    /// Wall times in the reader's zone; an all-day event ends at the
    /// midnight after its last day.
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub standing: Option<Standing>,
    /// The wall time the reader proposed instead, while that stands.
    pub proposed: Option<i64>,
}

/// The name of an entry that outlives the borrow it was found under: what
/// the cursor remembers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub account: AccountId,
    pub local: bool,
    pub uid: String,
    pub start: i64,
    pub day: i64,
}

impl Entry<'_> {
    /// Asked, and neither answered nor answered with another time.
    pub fn waiting(&self) -> bool {
        self.standing == Some(Standing::Needs) && self.proposed.is_none()
    }

    /// Whether its title or its place holds `needle`, which is in lower
    /// case.
    pub fn matches(&self, needle: &str) -> bool {
        let text = format!("{} {}", self.event.summary, self.event.location.as_deref().unwrap_or(""));
        text.to_lowercase().contains(needle)
    }

    pub fn pick(&self) -> Picked {
        let (account, uid) = (self.account.clone(), self.event.uid.clone());
        Picked { account, local: self.local, uid, start: self.start, day: self.day }
    }

    pub fn is(&self, picked: &Picked) -> bool {
        self.day == picked.day
            && self.start == picked.start
            && self.local == picked.local
            && *self.account == picked.account
            && self.event.uid == picked.uid
    }
}

/// What is on each of a run of days: all-day events first, then by the
/// clock.
#[derive(Debug, Clone)]
pub struct Days<'a> {
    from: i64,
    days: Vec<Vec<Entry<'a>>>,
}

impl<'a> Days<'a> {
    /// What is on `day`; nothing on a day outside the run.
    pub fn on(&self, day: i64) -> &[Entry<'a>] {
        usize::try_from(day - self.from).ok().and_then(|at| self.days.get(at)).map_or(&[], Vec::as_slice)
    }

    /// Every entry of the run, in the order the days draw them.
    pub fn all(&self) -> impl DoubleEndedIterator<Item = &Entry<'a>> {
        self.days.iter().flatten()
    }
}

/// One row of the Calendars panel: an account's own calendars taken
/// together, or its local one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source<'a> {
    pub account: &'a AccountId,
    pub local: bool,
    pub events: usize,
    /// The oldest of its last readings: what is shown is at least that old.
    pub read: Option<Timestamp>,
    pub hidden: bool,
}

impl Calendar {
    /// No calendars, at `now`, on a clock that is UTC.
    pub fn empty(now: Timestamp) -> Calendar {
        Calendar {
            now,
            zone: Zone::fixed(0),
            zones: BTreeMap::new(),
            shelves: Vec::new(),
            identities: Vec::new(),
            hidden: Vec::new(),
            proposed: Vec::new(),
            ignored: Vec::new(),
        }
    }

    /// The seconds the reader's wall clock shows now. A zone that cannot
    /// say its offset is read as UTC, as mail's dates are.
    pub fn wall(&self) -> i64 {
        self.now.0 + self.zone.offset_at(self.now.0).unwrap_or(0)
    }

    pub fn today(&self) -> i64 {
        self.wall().div_euclid(DAY)
    }

    pub fn is_hidden(&self, account: &AccountId, local: bool) -> bool {
        self.hidden.iter().any(|(hidden, which)| hidden == account && *which == local)
    }

    /// Shows a calendar that is hidden and hides one that is shown.
    pub fn toggle(&mut self, account: &AccountId, local: bool) {
        match self.hidden.iter().position(|(hidden, which)| hidden == account && *which == local) {
            Some(at) => {
                self.hidden.remove(at);
            }
            None => self.hidden.push((account.clone(), local)),
        }
    }

    /// What is on each day from `from` up to but not including `to`, in
    /// the calendars that are shown, of `account` alone when one is given.
    /// An event whose zone nothing defines is on no day (ADR-061).
    pub fn days(&self, from: i64, to: i64, account: Option<&AccountId>) -> Days<'_> {
        let mut days: Vec<Vec<Entry>> = vec![Vec::new(); usize::try_from(to - from).unwrap_or(0)];
        let shown = |shelf: &&Shelf| {
            account.is_none_or(|only| *only == shelf.account) && !self.is_hidden(&shelf.account, shelf.local)
        };
        for shelf in self.shelves.iter().filter(shown) {
            let mine: Vec<&str> = self
                .identities
                .iter()
                .filter(|identity| identity.account == shelf.account)
                .map(|identity| identity.address.email.as_str())
                .collect();
            for found in occurrences_between(&shelf.events, &self.zones, &self.zone, from, to) {
                let event = found.event;
                if self.ignored.iter().any(|(account, uid)| *account == shelf.account && *uid == event.uid) {
                    continue;
                }
                let proposed = self.proposed.iter().find(|p| p.account == shelf.account && p.uid == event.uid);
                // An end is the first moment after: an event that ends at
                // midnight is not on the day that begins there.
                let last = (found.end - 1).max(found.start).div_euclid(DAY);
                for day in found.start.div_euclid(DAY).max(from)..=last.min(to - 1) {
                    let Some(slot) = usize::try_from(day - from).ok().and_then(|at| days.get_mut(at)) else {
                        continue;
                    };
                    slot.push(Entry {
                        event,
                        account: &shelf.account,
                        local: shelf.local,
                        calendar: shelf.name.as_deref(),
                        day,
                        start: found.start,
                        end: found.end,
                        all_day: found.all_day,
                        standing: standing(event, &mine),
                        proposed: proposed.map(|p| p.start),
                    });
                }
            }
        }
        // Of two that begin together the one that ends sooner is first;
        // beyond that the calendars' own order stands.
        days.iter_mut().for_each(|day| day.sort_by_key(|entry| (!entry.all_day, entry.start, entry.end)));
        Days { from, days }
    }

    /// The entry a cursor remembers, when it is still there to be drawn.
    pub fn find(&self, picked: &Picked, account: Option<&AccountId>) -> Option<Entry<'_>> {
        let days = self.days(picked.day, picked.day + 1, account);
        days.on(picked.day).iter().find(|entry| entry.is(picked)).cloned()
    }

    /// The rows of the Calendars panel, in the order the calendars were
    /// handed over.
    pub fn sources(&self) -> Vec<Source<'_>> {
        let mut sources: Vec<Source> = Vec::new();
        for shelf in &self.shelves {
            let at = sources.iter().position(|s| *s.account == shelf.account && s.local == shelf.local);
            let at = at.unwrap_or_else(|| {
                let hidden = self.is_hidden(&shelf.account, shelf.local);
                sources.push(Source {
                    account: &shelf.account,
                    local: shelf.local,
                    events: 0,
                    read: shelf.read,
                    hidden,
                });
                sources.len() - 1
            });
            sources[at].events += shelf.events.len();
            sources[at].read = match (sources[at].read, shelf.read) {
                (Some(one), Some(other)) => Some(one.min(other)),
                (one, other) => one.or(other),
            };
        }
        sources
    }

    /// The accounts with a calendar that could not be read, each once,
    /// with when it last was.
    pub fn unread(&self) -> Vec<(&AccountId, Option<Timestamp>)> {
        let mut unread: Vec<(&AccountId, Option<Timestamp>)> = Vec::new();
        for shelf in self.shelves.iter().filter(|shelf| shelf.failed.is_some()) {
            if !unread.iter().any(|(account, _)| **account == shelf.account) {
                unread.push((&shelf.account, shelf.read));
            }
        }
        unread
    }
}

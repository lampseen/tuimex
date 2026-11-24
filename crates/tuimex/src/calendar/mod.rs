//! The calendar between what is kept and the interface (ADR-054): what
//! the store holds of every account, made into the one calendar the
//! interface draws, each time what is held or how a calendar stands with
//! its provider changes.
//!
//! All of it runs on the worker's thread (ADR-006). Reading the store,
//! parsing it and opening the system's zone files are work, and the loop
//! is only ever sent the calendar they come to.
//!
//! `carry` does what the reader asks of an event, `mail` finds the
//! invitations no calendar holds yet, and `own` keeps what the reader
//! decided that no event's file says.

mod carry;
mod mail;
mod own;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};

use anyhow::Result;
use tuimex_core::calendar::{Event as Held, Moment, Zone, joined};
use tuimex_core::{AccountId, Event, Identity, MessageId, Timestamp};
use tuimex_engine::calendar::{Configured, EVERY, Https, Kept, Runner, Wire};
use tuimex_engine::{Keyring, SystemKeyring, zone};
use tuimex_ui::calendar::{Calendar, Invited, Proposed, Shelf};

/// The rig the calendar's asks are tested on, for the tests of what is
/// built over them.
#[cfg(test)]
pub(crate) use self::carry::tests as rig;
pub use self::carry::{Post, Refusal};

use crate::config::Config;
use crate::paths::Dirs;
use crate::{live, sync};

#[cfg(test)]
mod tests;

/// What every building of the calendar is told and no store holds: who
/// the accounts are, the reader's own zone, and where the system keeps
/// the zones a calendar may name without defining (ADR-061).
#[derive(Debug, Clone)]
pub struct About {
    pub identities: Vec<Identity>,
    pub zone: Zone,
    pub zone_dirs: Vec<PathBuf>,
}

/// A calendar that could not be read: whose, why, and when it last was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub account: AccountId,
    pub reason: String,
    pub read_at: Option<Timestamp>,
}

/// The zone names an event's times are written in.
fn named(event: &Held) -> impl Iterator<Item = &String> {
    let moments = [Some(&event.start), event.end.as_ref(), event.instead_of.as_ref()];
    moments.into_iter().flatten().chain(&event.excluded).filter_map(|moment| match moment {
        Moment::Wall { zone, .. } => zone.as_ref(),
        _ => None,
    })
}

/// Makes each account's local events one with its provider's copy of
/// them (ADR-055). A local shelf every event of which the provider also
/// holds is left with nothing to draw, and is not handed over.
fn join(shelves: &mut Vec<Shelf>, identities: &[Identity]) {
    let mut emptied = Vec::new();
    for at in 0..shelves.len() {
        if !shelves[at].local || shelves[at].events.is_empty() {
            continue;
        }
        let account = shelves[at].account.clone();
        let mine = identities.iter().filter(|identity| identity.account == account);
        let mine: Vec<&str> = mine.map(|identity| identity.address.email.as_str()).collect();
        let mut left = std::mem::take(&mut shelves[at].events);
        for own in shelves.iter_mut().filter(|shelf| !shelf.local && shelf.account == account) {
            left = joined(&mut own.events, left, &mine);
        }
        if left.is_empty() {
            emptied.push(at);
        }
        shelves[at].events = left;
    }
    let mut at = 0;
    shelves.retain(|_| {
        at += 1;
        !emptied.contains(&(at - 1))
    });
}

/// The interface's calendar from what is kept: a shelf for each thing
/// kept, the zones the calendars define and then the system's for a name
/// none of them does, and on each failed account's shelves why and when
/// it was last read. An account that failed and holds nothing is given an
/// empty shelf, so that the failure has somewhere to be said.
pub fn build(kept: Vec<Kept>, failed: &[Failed], about: &About, now: Timestamp) -> Calendar {
    let mut zones: BTreeMap<String, Zone> = BTreeMap::new();
    let mut shelves = Vec::new();
    for kept in kept {
        // Of two calendars that define one name differently the first
        // read is used: a name means one thing on one screen.
        for (name, zone) in kept.calendar.zones {
            zones.entry(name).or_insert(zone);
        }
        shelves.push(Shelf {
            account: AccountId(kept.account),
            local: kept.local,
            name: kept.name,
            read: kept.read_at.map(Timestamp),
            failed: None,
            events: kept.calendar.events,
        });
    }
    join(&mut shelves, &about.identities);
    for failure in failed {
        let mut own = shelves.iter_mut().filter(|shelf| !shelf.local && shelf.account == failure.account).peekable();
        if own.peek().is_none() {
            shelves.push(Shelf {
                account: failure.account.clone(),
                local: false,
                name: None,
                read: failure.read_at,
                failed: Some(failure.reason.clone()),
                events: Vec::new(),
            });
            continue;
        }
        for shelf in own {
            (shelf.failed, shelf.read) = (Some(failure.reason.clone()), failure.read_at.or(shelf.read));
        }
    }
    // Each name is looked for once, however many events are written in it.
    let undefined: BTreeSet<String> = shelves.iter().flat_map(|shelf| &shelf.events).flat_map(named).cloned().collect();
    zone::fill(&mut zones, undefined, &about.zone_dirs);
    Calendar { zone: about.zone.clone(), zones, shelves, identities: about.identities.clone(), ..Calendar::empty(now) }
}

/// What the worker sends the loop for the interface.
#[derive(Debug)]
pub enum Handed {
    /// Every calendar as it stood once every calendar ask up to the
    /// serial `carried` had been carried out or refused.
    Calendar { calendar: Box<Calendar>, carried: u64 },
    /// The event the calendar part of a message is for, for the reader's
    /// card; `None` when it has none that can be answered.
    Invited(MessageId, Option<Box<Invited>>),
}

/// The worker's side of the calendar: it reads what is kept, remembers
/// how each calendar stands with its provider, and sends the loop the
/// calendar those make.
pub struct Reader {
    data: PathBuf,
    cache: PathBuf,
    about: About,
    /// What the store held when it was last read, or `None` until it has
    /// been.
    kept: Option<Vec<Kept>>,
    failed: Vec<Failed>,
    shown: Sender<Handed>,
    /// The serial of the last ask the worker took, which every calendar
    /// handed over is read after.
    pub carried: u64,
    calendars: Vec<Configured>,
    /// The accounts whose calendar is written at its provider, and what
    /// reaches the provider. Without the second nothing is written there.
    writable: Vec<String>,
    wire: Option<(Box<dyn Wire + Send>, Box<dyn Keyring>)>,
    own: own::Own,
    /// Why what the reader had ignored and proposed could not be read at
    /// the start, when it could not: it is then never written over.
    own_unread: Option<String>,
    mail: mail::Mail,
    /// The calendar asks of this run that were carried out, oldest first.
    done: Vec<carry::Done>,
    /// What the calendar thread is started with, until it has been.
    runner: Option<Runner>,
    /// Asks the calendar thread for a pass now. The thread ends when
    /// this is dropped, which is when the worker ends.
    nudges: Option<Sender<()>>,
}

/// The accounts whose `[[calendar]]` says it is written at its provider.
pub fn writable(config: &Config) -> Vec<String> {
    let named = config.accounts.iter().map(|account| account.name.clone());
    named.filter(|name| config.writes(name)).collect()
}

/// The reader for the calendars the config names. With none there is
/// still one: an account with no calendar has its local one, and the
/// invitations in its mail. `offset` is the machine's seconds east of
/// UTC, for a machine whose zone file cannot be read; every calendar
/// built is sent on `shown`.
pub fn reader(config: &Config, dirs: &Dirs, offset: i64, shown: Sender<Handed>) -> Result<Reader> {
    let calendars = sync::calendars(config, None)?;
    let runner = Runner {
        calendars,
        wire: Box::new(Https::new(sync::PATIENCE)),
        keyring: Box::new(SystemKeyring),
        data: dirs.data.clone(),
        cache: dirs.cache.clone(),
        every: EVERY,
    };
    let about = About {
        identities: live::identities(config),
        zone: zone::local_zone(Path::new("/etc/localtime"), offset),
        zone_dirs: zone::zone_dirs(),
    };
    let wire: Box<dyn Wire + Send> = Box::new(Https::new(sync::PATIENCE));
    Ok(Reader::new(runner, about, shown).writing(writable(config), wire, Box::new(SystemKeyring)))
}

impl Reader {
    pub fn new(runner: Runner, about: About, shown: Sender<Handed>) -> Reader {
        let (data, cache) = (runner.data.clone(), runner.cache.clone());
        Reader {
            about,
            kept: None,
            failed: Vec::new(),
            shown,
            carried: 0,
            calendars: runner.calendars.clone(),
            writable: Vec::new(),
            wire: None,
            own: own::Own::default(),
            own_unread: None,
            mail: mail::Mail::open(&cache),
            done: Vec::new(),
            data,
            cache,
            runner: Some(runner),
            nudges: None,
        }
    }

    /// As it is, writing the calendars of `writable` at their provider
    /// through `wire`, signed in from `keyring`.
    pub fn writing(mut self, writable: Vec<String>, wire: Box<dyn Wire + Send>, keyring: Box<dyn Keyring>) -> Reader {
        (self.writable, self.wire) = (writable, Some((wire, keyring)));
        self
    }

    /// Hands over what is on disk, and then starts the calendar thread,
    /// which says how each fetch went on `events`. In that order, so that
    /// what is held is on screen before any provider has answered, and the
    /// store is made by one thread before a second opens it. The error is
    /// why the store could not be read; the thread is started all the same.
    pub fn start(&mut self, events: Sender<Event>) -> Result<(), String> {
        let own = own::Own::load(&self.data).map(|own| self.own = own);
        self.own_unread = own.clone().err();
        let read = own.and(self.read());
        if let Some(runner) = self.runner.take() {
            let (nudges, nudged) = mpsc::channel();
            // Not joined: see `Live::stop`.
            drop(tuimex_engine::calendar::start(runner, nudged, events));
            self.nudges = Some(nudges);
        }
        read
    }

    /// `:sync` reads the calendars too.
    pub fn nudge(&self) {
        if let Some(nudges) = &self.nudges {
            // A calendar thread that has gone found nobody listening for
            // its events, which is a program that is stopping.
            let _ = nudges.send(());
        }
    }

    /// Reads the store again and hands over what it now holds: after a
    /// fetch, and after anything that writes a local event. When it cannot
    /// be read, what was held stays on screen and the error says why.
    pub fn read(&mut self) -> Result<(), String> {
        self.load()?;
        self.hand();
        Ok(())
    }

    /// As `read`, with the invitations in mail worked out against what
    /// the store now holds: after anything that wrote an event. What mail
    /// had to say, and why the store could not be read when it could not.
    pub fn refresh(&mut self, post: &Post) -> (Vec<String>, Result<(), String>) {
        let loaded = self.load();
        let said = self.scan(post);
        self.hand();
        (said, loaded)
    }

    fn load(&mut self) -> Result<(), String> {
        let kept = tuimex_engine::calendar::kept(&self.data, &self.cache);
        self.kept = Some(kept.map_err(|error| format!("the calendar store: {error}"))?);
        Ok(())
    }

    /// What the calendar thread said of one account's calendar. One that
    /// was read is read here from the store, whether or not it changed:
    /// when it was last read is the store's to say, and a failure before
    /// it is over. One that failed keeps the events held and gains the
    /// reason. Any other event is not the calendar's.
    pub fn told(&mut self, event: &Event) -> Result<(), String> {
        match event {
            Event::CalendarRead { account, .. } => {
                self.failed.retain(|failure| failure.account != *account);
                self.read()
            }
            Event::CalendarFailed { account, reason, read_at } => {
                self.failed.retain(|failure| failure.account != *account);
                self.failed.push(Failed { account: account.clone(), reason: reason.clone(), read_at: *read_at });
                // A store that could not be read at the start is tried
                // again, so a failed fetch still shows what is held; the
                // failure is said whether or not that works.
                let read = if self.kept.is_none() { self.load() } else { Ok(()) };
                self.hand();
                read
            }
            _ => Ok(()),
        }
    }

    fn hand(&self) {
        let kept = self.kept.clone().unwrap_or_default();
        let now = Timestamp(chrono::Utc::now().timestamp());
        let mut calendar = build(kept, &self.failed, &self.about, now);
        self.dress(&mut calendar);
        // A loop that has gone is one that is stopping.
        let _ = self.shown.send(Handed::Calendar { calendar: Box::new(calendar), carried: self.carried });
    }

    /// Whether a calendar of `account` holds an event of that UID.
    fn holds(&self, account: &str, uid: &str) -> bool {
        let own = self.kept.iter().flatten().filter(|kept| kept.account == account);
        own.flat_map(|kept| &kept.calendar.events).any(|event| event.uid == uid)
    }

    /// Adds what no calendar's store holds: each invitation that is in
    /// mail alone, on a shelf of the calendar an answer would put it in,
    /// and what the reader ignored and proposed.
    fn dress(&self, calendar: &mut Calendar) {
        for pending in &self.mail.pending {
            let account = pending.account.as_str();
            if self.holds(account, &pending.event.uid) || self.own.ignores(account, &pending.event.uid) {
                continue;
            }
            for (name, zone) in &pending.zones {
                calendar.zones.entry(name.clone()).or_insert_with(|| zone.clone());
            }
            zone::fill(&mut calendar.zones, named(&pending.event).cloned(), &self.about.zone_dirs);
            calendar.shelves.push(Shelf {
                account: AccountId(account.to_string()),
                local: !self.writable.iter().any(|writable| writable == account),
                name: None,
                read: None,
                failed: None,
                events: vec![pending.event.clone()],
            });
        }
        let id = |account: &String| AccountId(account.clone());
        calendar.ignored = self.own.ignored.iter().map(|(account, uid)| (id(account), uid.clone())).collect();
        let proposed = self.own.proposed.iter();
        let proposed =
            proposed.map(|(account, uid, start)| Proposed { account: id(account), uid: uid.clone(), start: *start });
        calendar.proposed = proposed.collect();
        calendar.came_in = self.mail.came_in.clone();
    }

    /// Tells the reader's card which event the message `id` invites to.
    /// Whether the message carries an event at all.
    pub fn invited(&self, id: &MessageId, raw: &[u8]) -> bool {
        let part = mail::part(raw);
        let carries = part.is_some();
        let invited = part.filter(|part| part.method == tuimex_core::calendar::Method::Request);
        let invited = invited.map(|part| Box::new(Invited { event: part.event, zones: part.zones }));
        // A loop that has gone is one that is stopping.
        let _ = self.shown.send(Handed::Invited(id.clone(), invited));
        carries
    }
}

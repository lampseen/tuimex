//! Carrying out what the reader does in the calendar: an answer given, an
//! event saved or deleted, and each of them taken back (ADR-015, ADR-055,
//! ADR-057). It runs on the worker's thread, where waiting for a provider
//! holds up nothing that is drawn (ADR-006).
//!
//! An event is written at its provider only where the config says that
//! calendar is written (ADR-062). Anywhere else, what is done for an
//! account goes to the files tuimex keeps for it. Whatever is written,
//! the text that was there before is kept under the ask's serial, and
//! putting that text back is all that taking the ask back is.

use std::collections::{BTreeMap, BTreeSet};

use tuimex_content as content;
use tuimex_core::calendar::{Calendar as Parsed, Event, Guest, Moment, Zone};
use tuimex_core::invitation::{self, Outgoing, Record};
use tuimex_core::{Action, Address, Ask, Draft, Envelope, EventAnswer, EventId, MessageId, Query, Role, Sort, Stored};
use tuimex_engine::calendar::{self as provider, CalendarError, Source};
use tuimex_engine::invitation::About;
use tuimex_engine::{Engines, StoreHandle, resolve, zone};
use tuimex_store::calendar::Calendars;
use tuimex_store::maildir::new_key;

use super::own::Own;
use super::{Reader, named};
use crate::message;

#[cfg(test)]
pub(crate) mod tests;

/// How many asks are remembered for `u`, as the engine remembers mail's
/// (ADR-040).
const REMEMBERED: usize = 200;

/// Why an ask was not carried out, as `Event::ActionFailed` says it.
#[derive(Debug, PartialEq, Eq)]
pub struct Refusal {
    pub reason: String,
    pub hint: String,
}

impl From<String> for Refusal {
    fn from(reason: String) -> Refusal {
        Refusal { reason, hint: String::new() }
    }
}

impl From<CalendarError> for Refusal {
    fn from(error: CalendarError) -> Refusal {
        match error {
            // Either way the provider's copy is not the one read, and
            // reading it again is what lets the change be made of it.
            CalendarError::Changed | CalendarError::Unread => Refusal {
                reason: "the event changed at its provider since the calendar was read".to_string(),
                hint: ":sync, then do it again".to_string(),
            },
            other => Refusal::from(other.to_string()),
        }
    }
}

impl From<tuimex_store::Error> for Refusal {
    fn from(error: tuimex_store::Error) -> Refusal {
        Refusal::from(CalendarError::from(error))
    }
}

fn refuse<T>(reason: impl Into<String>) -> Result<T, Refusal> {
    Err(Refusal::from(reason.into()))
}

fn gone<T>(account: &str) -> Result<T, Refusal> {
    Err(Refusal {
        reason: format!("the event is no longer in {account}'s calendar"),
        hint: ":sync shows it as it is".to_string(),
    })
}

fn read_only<T>(account: &str, verb: &str) -> Result<T, Refusal> {
    Err(Refusal {
        reason: format!("{account}'s calendar is read here and never written, so the event stays as it is"),
        hint: format!("{verb} it where the calendar is kept"),
    })
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The mail side of an ask, which is the engines' and the mail store's.
pub struct Post<'a> {
    pub engines: &'a Engines,
    pub store: &'a StoreHandle,
}

impl Post<'_> {
    /// Hands `mail` to the Outbox of `account` under the ask `serial`,
    /// built as every message is (PLAN §4.4). The message held.
    fn send(&self, account: &str, serial: u64, mail: &Outgoing, about: &About) -> Result<MessageId, Refusal> {
        let Some(engine) = self.engines.of(account) else { return refuse(format!("no account {account}")) };
        let build = |draft: &Draft| message::build(draft, Vec::new(), now(), &new_key(), message::Bcc::Written);
        Ok(engine.send_mail(serial, mail, about, &build)?)
    }

    /// A message's bytes and what the index holds of it; `None` for one
    /// that has gone.
    pub fn message(&self, id: &MessageId) -> Option<(Vec<u8>, Option<Envelope>)> {
        let id = id.clone();
        self.store.run(move |s| Ok((s.read(&id)?, s.envelope(&id)?))).ok()
    }

    /// The messages in an Inbox that carry a calendar part, oldest first.
    /// A store that cannot be asked has none to show, and says why when
    /// mail is next read from it.
    pub fn invitations(&self) -> Vec<Envelope> {
        let query = Query::parse("has:invite in:inbox");
        let found = self.store.run(move |s| s.query(&query, tuimex_core::Timestamp(now()), Sort::NewestFirst, None));
        found
            .map(|mut found| {
                found.reverse();
                found
            })
            .unwrap_or_default()
    }

    /// Whether the message is in a folder with `role`. One that has gone
    /// is in none.
    pub fn is_in(&self, id: &MessageId, role: Role) -> bool {
        let id = id.clone();
        let found = self.store.run(move |s| {
            let (folder, _) = s.stored(&id)?;
            Ok(s.folders()?.iter().any(|f| f.account == id.account && f.name == folder && f.role == role))
        });
        found.unwrap_or(false)
    }

    /// `u` on mail an ask left in the Outbox: back to Drafts, and gone
    /// from there. False when the engine would not stop it, which the
    /// engine says itself under `serial`. The second step is taken only
    /// once the first is seen to have happened: a message still in the
    /// Outbox may be on its way to the server.
    fn unsend(&self, serial: u64, id: &MessageId) -> bool {
        let Some(engine) = self.engines.of(&id.account.0) else { return false };
        engine.act(Ask { serial, action: Action::Undo, on: vec![id.clone()] });
        if !self.is_in(id, Role::Drafts) {
            return false;
        }
        engine.act(Ask { serial, action: Action::Discard, on: vec![id.clone()] });
        true
    }
}

/// One thing a calendar holds, with the events in it: a file here, or
/// what one address at the provider serves.
pub(super) struct Thing {
    href: String,
    text: String,
    pub parsed: Parsed,
}

/// A calendar that is written at its provider.
struct Dav {
    url: String,
    user: String,
    password_cmd: Option<Vec<String>>,
}

/// Where a write for an account goes: its provider, or the files here.
pub(super) struct Site {
    account: String,
    dav: Option<Dav>,
}

/// What one thing held before an ask changed it; `None` when it was not
/// there.
#[derive(Debug, Clone)]
pub(super) struct Step {
    account: String,
    local: bool,
    href: String,
    was: Option<String>,
}

/// One ask carried out, and what taking it back needs.
#[derive(Debug, Default)]
pub struct Done {
    serial: u64,
    steps: Vec<Step>,
    /// What the reader had ignored and proposed before it, when it
    /// changed that.
    own: Option<Own>,
    /// The mail it left in the Outbox.
    mail: Option<MessageId>,
    /// That mail has left, so the ask can no longer be taken back.
    sent: bool,
}

/// Everything held of one of an account's calendars, the local one or
/// its provider's, with the first thing found to hold `uid`.
pub(super) fn find(store: &Calendars, account: &str, local: bool, uid: &str) -> Result<Option<Thing>, Refusal> {
    let held = store.held()?.into_iter().filter(|held| held.account == account && held.calendar.is_none() == local);
    Ok(held.map(thing).find(|thing| thing.parsed.events.iter().any(|event| event.uid == uid)))
}

fn thing(held: tuimex_store::calendar::Held) -> Thing {
    Thing { parsed: content::calendar(&held.text), href: held.href, text: held.text }
}

/// An event with one guest's answer set, the guest added when the event
/// does not list them.
fn answered(event: &Event, me: &Address, said: tuimex_core::calendar::Said) -> Event {
    let mut event = event.clone();
    match event.guests.iter_mut().find(|guest| guest.who.email.eq_ignore_ascii_case(&me.email)) {
        Some(guest) => guest.said = said,
        None => event.guests.push(Guest { who: me.clone(), said }),
    }
    event
}

/// The occurrence of `event` that would have begun at `start`, standing
/// alone: what a cancellation of that one day is about.
fn one_day(event: &Event, start: &Moment) -> Event {
    let length = event.end.as_ref().map(|end| end.wall() - event.start.wall());
    Event {
        start: start.clone(),
        end: length.map(|length| start.at_wall(start.wall() + length)),
        instead_of: Some(start.clone()),
        repeat: None,
        excluded: Vec::new(),
        ..event.clone()
    }
}

impl Reader {
    pub(super) fn store(&self) -> Result<Calendars, Refusal> {
        Ok(Calendars::open(&self.data, &self.cache)?)
    }

    /// The addresses of an account, its own first.
    pub(super) fn mine(&self, account: &str) -> Vec<Address> {
        let own = self.about.identities.iter().filter(|identity| identity.account.0 == account);
        own.map(|identity| identity.address.clone()).collect()
    }

    /// Where a write to that calendar of `account` goes. Its provider
    /// takes it only when the config says the calendar is written there.
    pub(super) fn site(&self, account: &str, local: bool) -> Site {
        let written = !local && self.writable.iter().any(|writable| writable == account);
        let configured = self.calendars.iter().find(|calendar| written && calendar.account == account);
        let dav = configured.and_then(|calendar| match &calendar.source {
            Source::CalDav { url, user, password_cmd } => {
                Some(Dav { url: url.clone(), user: user.clone(), password_cmd: password_cmd.clone() })
            }
            Source::Feed { .. } => None,
        });
        Site { account: account.to_string(), dav }
    }

    /// The zones an event's times name: as the calendars held define
    /// them, and else as the system does (ADR-061).
    fn zones(&self, event: &Event) -> BTreeMap<String, Zone> {
        let names: BTreeSet<&String> = named(event).collect();
        let defined = self.kept.iter().flatten().flat_map(|kept| &kept.calendar.zones);
        let mut zones = BTreeMap::new();
        for (name, zone) in defined.filter(|(name, _)| names.contains(name)) {
            zones.entry(name.clone()).or_insert_with(|| zone.clone());
        }
        zone::fill(&mut zones, names.into_iter().cloned(), &self.about.zone_dirs);
        zones
    }

    /// Makes `thing` hold `text`: a new thing when there is none, and with
    /// no text none at all. What it held is noted in `steps`, once for
    /// each thing, so that the first state an ask found is the one kept.
    pub(super) fn set(
        &self,
        store: &Calendars,
        steps: &mut Vec<Step>,
        site: &Site,
        thing: Option<&Thing>,
        text: Option<&str>,
    ) -> Result<(), Refusal> {
        let account = site.account.as_str();
        let href = match (&site.dav, thing, text) {
            (_, None, None) => return Ok(()),
            (None, None, Some(text)) => store.keep(account, text)?,
            (None, Some(thing), text) => {
                match text {
                    Some(text) => store.rewrite(account, &thing.href, text)?,
                    None => store.remove(account, &thing.href)?,
                }
                thing.href.clone()
            }
            (Some(dav), thing, text) => {
                let Some((wire, keyring)) = &self.wire else { return refuse("nothing here reaches the provider") };
                let password = resolve(account, dav.password_cmd.as_deref(), keyring.as_ref());
                let password = password.map_err(|e| e.to_string())?;
                let (wire, url, sign_in) = (wire.as_ref(), dav.url.as_str(), (dav.user.as_str(), &password));
                match (thing, text) {
                    (None, None) => return Ok(()),
                    (None, Some(text)) => {
                        // Named by a key made here: an event's UID is text
                        // from outside, and never becomes an address. A
                        // UID with nothing before its `@` is no name, and
                        // the write then refuses it.
                        let name = provider::new_uid()?;
                        let name = name.split('@').next().unwrap_or_default();
                        provider::put_new(wire, store, account, url, sign_in, text, name)?
                    }
                    (Some(thing), Some(text)) => {
                        provider::put_changed(wire, store, account, url, sign_in, &thing.href, text)?;
                        thing.href.clone()
                    }
                    (Some(thing), None) => {
                        provider::delete(wire, store, account, url, sign_in, &thing.href)?;
                        thing.href.clone()
                    }
                }
            }
        };
        let local = site.dav.is_none();
        if !steps.iter().any(|step| step.account == account && step.local == local && step.href == href) {
            steps.push(Step { account: account.to_string(), local, href, was: thing.map(|thing| thing.text.clone()) });
        }
        Ok(())
    }

    /// Puts back what `done` changed, newest first, and returns what
    /// would do it again.
    fn restore(&mut self, done: &Done) -> Result<Done, Refusal> {
        let store = self.store()?;
        let mut again = Done { serial: done.serial, mail: done.mail.clone(), ..Done::default() };
        for step in done.steps.iter().rev() {
            let site = self.site(&step.account, step.local);
            let held = store.held()?.into_iter().filter(|held| held.account == step.account);
            let there = held.filter(|held| held.calendar.is_none() == step.local).find(|held| held.href == step.href);
            self.set(&store, &mut again.steps, &site, there.map(thing).as_ref(), step.was.as_deref())?;
        }
        if let Some(own) = done.own.as_ref().filter(|own| **own != self.own) {
            again.own = Some(std::mem::replace(&mut self.own, own.clone()));
            // A file that was never written over has nothing to put back.
            if self.own_unread.is_none() {
                self.save_own()?;
            }
        }
        Ok(again)
    }

    /// Carries out a calendar ask, and remembers it for `u`. The message
    /// it left in the Outbox or took out of it, when there is one: its
    /// row is the interface's to read again. A refusal leaves nothing
    /// written: what was written before the step that failed is put back.
    pub fn carry(&mut self, ask: &Ask, post: &Post) -> Result<Option<MessageId>, Refusal> {
        let unsaved = |id: &EventId| id.uid.starts_with("unsaved ");
        let mut done = Done { serial: ask.serial, ..Done::default() };
        let carried = match &ask.action {
            Action::UndoEvent(serial) => return self.take_back(*serial, ask.serial, post),
            Action::AnswerEvent(answer) if !unsaved(&answer.event) => self.answer(answer, &mut done, post),
            Action::DeleteEvent(id) if !unsaved(id) => self.delete(id, &mut done, post),
            Action::SaveEvent(stored) if !stored.replaces.as_ref().is_some_and(unsaved) => {
                self.save(stored, &mut done, post)
            }
            _ => refuse("the event is still being saved; do it again in a moment"),
        };
        if let Err(mut refusal) = carried {
            if let Err(left) = self.restore(&done) {
                refusal.reason = format!("{}; what it had written is still there: {}", refusal.reason, left.reason);
            }
            return Err(refusal);
        }
        let mail = done.mail.clone();
        self.done.push(done);
        let over = self.done.len().saturating_sub(REMEMBERED);
        self.done.drain(..over);
        Ok(mail)
    }

    /// `u`: the ask `target` is taken back under the ask `serial`. The
    /// calendar first, so that mail whose calendar entry cannot be put
    /// back still leaves; then the mail, and when the engine will not
    /// stop that, the calendar is made to hold the answer again.
    fn take_back(&mut self, target: u64, serial: u64, post: &Post) -> Result<Option<MessageId>, Refusal> {
        let Some(at) = self.done.iter().position(|done| done.serial == target) else {
            return refuse("there is nothing of it left to take back");
        };
        if self.done[at].sent {
            return refuse("its mail has been sent, so it stands; answer again to change it");
        }
        let done = self.done.remove(at);
        let again = match self.restore(&done) {
            Ok(again) => again,
            Err(refusal) => {
                self.done.push(done);
                return Err(refusal);
            }
        };
        if let Some(mail) = done.mail.as_ref().filter(|mail| !post.unsend(serial, mail)) {
            let sent = !post.is_in(mail, Role::Outbox);
            let redone = self.restore(&again)?;
            self.done.push(Done { sent, ..redone });
        }
        Ok(done.mail)
    }

    /// The engine refused the ask `serial` after it was carried out here:
    /// its mail cannot leave from that account. What it wrote is put
    /// back, and the mail, which is a draft nobody wrote, goes. Whether
    /// it was a calendar ask.
    pub fn refused(&mut self, serial: u64, post: &Post) -> Result<bool, Refusal> {
        let Some(at) = self.done.iter().position(|done| done.serial == serial) else { return Ok(false) };
        let done = self.done.remove(at);
        if let Some((mail, engine)) =
            done.mail.as_ref().and_then(|mail| Some((mail, post.engines.of(&mail.account.0)?)))
        {
            engine.act(Ask { serial, action: Action::Discard, on: vec![mail.clone()] });
        }
        self.restore(&done).map(|_| true)
    }

    /// A message has left: an ask whose mail it was now stands.
    pub fn sent(&mut self, id: &MessageId) {
        self.done.iter_mut().filter(|done| done.mail.as_ref() == Some(id)).for_each(|done| done.sent = true);
    }

    /// Mail's own `u` was asked of `on`: the calendar entry of each ask
    /// whose mail went back to Drafts is taken back with it (ADR-057).
    /// Whether any was.
    pub fn unsent(&mut self, on: &[MessageId], post: &Post) -> Result<bool, Refusal> {
        let theirs = |done: &Done| done.mail.as_ref().is_some_and(|mail| on.contains(mail) && !done.sent);
        let mut any = false;
        while let Some(at) = self
            .done
            .iter()
            .position(|done| theirs(done) && done.mail.as_ref().is_some_and(|mail| post.is_in(mail, Role::Drafts)))
        {
            let done = self.done.remove(at);
            self.restore(&done)?;
            any = true;
        }
        Ok(any)
    }

    /// Keeps what the reader ignored and proposed, when an ask changed it.
    fn keep_own(&mut self, done: &mut Done) -> Result<(), Refusal> {
        if done.own.as_ref() == Some(&self.own) {
            done.own = None;
            return Ok(());
        }
        self.save_own()
    }

    /// Writes what the reader ignored and proposed. Where what an earlier
    /// run kept could not be read, it is not written over: the file is
    /// the reader's own, and whatever is wrong with it may be mended.
    pub(super) fn save_own(&self) -> Result<(), Refusal> {
        match &self.own_unread {
            Some(why) => Err(Refusal {
                reason: format!(
                    "what was ignored and proposed before could not be read, and is not written over ({why})"
                ),
                hint: "mend or remove that file, and start tuimex again".to_string(),
            }),
            None => Ok(self.own.save(&self.data)?),
        }
    }

    /// An answer (ADR-057): the calendar of the account it came to gains
    /// the event with the answer, and the organizer is mailed.
    fn answer(&mut self, asked: &EventAnswer, done: &mut Done, post: &Post) -> Result<(), Refusal> {
        let EventAnswer { event: id, answer, message } = asked;
        let (account, uid, stamp) = (id.account.0.as_str(), id.uid.as_str(), now());
        let store = self.store()?;
        let site = self.site(account, id.local);
        let shown = find(&store, account, id.local, uid)?;
        // A provider's calendar that is only read keeps no answer: the
        // answer is in the copy of the event held here (ADR-055).
        let copied = !id.local && site.dav.is_none();
        let copy = if copied { find(&store, account, true, uid)? } else { None };
        let target = if copied { copy.as_ref() } else { shown.as_ref() };
        let of = |thing: &Thing| {
            let named = |event: &&Event| event.uid == uid && event.instead_of == id.occurrence;
            let events = &thing.parsed.events;
            let event = events.iter().find(named).or_else(|| events.iter().find(|event| event.uid == uid));
            event.map(|event| (event.clone(), thing.parsed.zones.clone()))
        };
        let read = match shown.as_ref().or(copy.as_ref()) {
            Some(thing) => of(thing),
            None => self.mail.invitation(account, uid, message.as_ref(), post),
        };
        let Some((event, zones)) = read else { return gone(account) };
        let answering = invitation::answer(&event, &self.mine(account), answer).map_err(|why| why.to_string())?;
        done.own = Some(self.own.clone());
        match &answering.record {
            Record::Said(said) => {
                let answered = answered(&event, &answering.me, *said);
                // The text held is answered as it stands, so that what a
                // provider wrote and tuimex does not read is kept.
                let text = match target {
                    Some(thing) => content::with_answer(&thing.text, &answering.me, *said).unwrap_or_else(|| {
                        let same = |held: &Event| held.uid == uid && held.instead_of == event.instead_of;
                        let events = thing.parsed.events.iter().map(|e| if same(e) { &answered } else { e });
                        let events: Vec<Event> = events.cloned().collect();
                        content::events_text(&events, &thing.parsed.zones, stamp)
                    }),
                    None => content::event_text(&answered, &zones, stamp),
                };
                self.set(&store, &mut done.steps, &site, target, Some(&text))?;
                self.own.settle(account, uid);
            }
            Record::Proposed { start, .. } => {
                // The event is shown as proposed wherever it is held; one
                // that is in mail alone is kept, unanswered, to be shown.
                if shown.is_none() && copy.is_none() {
                    let text = content::event_text(&event, &zones, stamp);
                    self.set(&store, &mut done.steps, &site, None, Some(&text))?;
                }
                let wall = start.local_wall(&zones, &self.about.zone).unwrap_or(start.wall());
                self.own.settle(account, uid);
                self.own.proposed.push((account.to_string(), uid.to_string(), wall));
            }
            Record::Ignored if self.own.ignores(account, uid) => {}
            Record::Ignored => self.own.ignored.push((account.to_string(), uid.to_string())),
        }
        self.keep_own(done)?;
        if let Some(mail) = &answering.mail {
            let cited = message.clone().or_else(|| self.mail.came_in(uid, account));
            let answers = cited.and_then(|id| post.message(&id)?.1?.message_id);
            let about = About { event: &event, zones: &zones, answers: answers.as_deref(), stamp };
            done.mail = Some(post.send(account, done.serial, mail, &about)?);
        }
        Ok(())
    }

    /// A delete (ADR-055): the event and everything standing in for a day
    /// of it, or with an occurrence named that day alone. The guests of
    /// an event the account organizes are mailed that it is cancelled.
    fn delete(&mut self, id: &EventId, done: &mut Done, post: &Post) -> Result<(), Refusal> {
        let (account, uid, stamp) = (id.account.0.as_str(), id.uid.as_str(), now());
        let site = self.site(account, id.local);
        if !id.local && site.dav.is_none() {
            return read_only(account, "delete");
        }
        let store = self.store()?;
        let Some(thing) = find(&store, account, id.local, uid)? else { return gone(account) };
        let mut events = thing.parsed.events.clone();
        let whole = events.iter().find(|e| e.uid == uid && e.instead_of.is_none()).cloned();
        let cancelled = match &id.occurrence {
            None => {
                let any = events.iter().find(|e| e.uid == uid).cloned();
                events.retain(|e| e.uid != uid);
                whole.or(any)
            }
            Some(start) => {
                let stood_in = events.iter().find(|e| e.uid == uid && e.instead_of.as_ref() == Some(start)).cloned();
                events.retain(|e| !(e.uid == uid && e.instead_of.as_ref() == Some(start)));
                let series = events.iter_mut().filter(|e| e.uid == uid && e.instead_of.is_none());
                series.filter(|e| !e.excluded.contains(start)).for_each(|e| e.excluded.push(start.clone()));
                stood_in.or(whole.map(|event| one_day(&event, start)))
            }
        };
        let text = (!events.is_empty()).then(|| content::events_text(&events, &thing.parsed.zones, stamp));
        self.set(&store, &mut done.steps, &site, Some(&thing), text.as_deref())?;
        // No mail for an event somebody else organizes, or one with no
        // guests: there is nobody whose calendar this account answers for.
        if let Some(event) = cancelled
            && let Ok(mail) = invitation::cancel(&event, &self.mine(account))
        {
            let about = About { event: &event, zones: &thing.parsed.zones, answers: None, stamp };
            done.mail = Some(post.send(account, done.serial, &mail, &about)?);
        }
        Ok(())
    }

    /// A save out of the form (ADR-055): a new event under a UID made
    /// here, or one in place of the event it names, which leaves its old
    /// calendar when it is saved to another.
    fn save(&mut self, stored: &Stored, done: &mut Done, post: &Post) -> Result<(), Refusal> {
        let (account, stamp) = (stored.account.0.as_str(), now());
        let site = self.site(account, stored.local);
        let store = self.store()?;
        let mut event = stored.event.clone();
        let mut zones = self.zones(&event);
        match &stored.replaces {
            None => {
                event.uid = provider::new_uid()?;
                let text = content::event_text(&event, &zones, stamp);
                self.set(&store, &mut done.steps, &site, None, Some(&text))?;
            }
            Some(old) => {
                let from = self.site(&old.account.0, old.local);
                if !old.local && from.dav.is_none() {
                    return read_only(&old.account.0, "change");
                }
                let Some(thing) = find(&store, &old.account.0, old.local, &old.uid)? else {
                    return gone(&old.account.0);
                };
                event.uid.clone_from(&old.uid);
                // A guest's calendar takes a new version only when it is
                // numbered above the one it holds.
                if stored.invite {
                    event.sequence = event.sequence.saturating_add(1);
                }
                for (name, zone) in &thing.parsed.zones {
                    zones.entry(name.clone()).or_insert_with(|| zone.clone());
                }
                let (mut ours, rest): (Vec<Event>, Vec<Event>) =
                    thing.parsed.events.iter().cloned().partition(|e| e.uid == old.uid);
                ours.retain(|e| e.instead_of != event.instead_of);
                ours.push(event.clone());
                let moved = old.occurrence.is_none()
                    && (old.account != stored.account || from.dav.is_some() != site.dav.is_some());
                if moved {
                    let left = (!rest.is_empty()).then(|| content::events_text(&rest, &thing.parsed.zones, stamp));
                    self.set(&store, &mut done.steps, &from, Some(&thing), left.as_deref())?;
                    let text = content::events_text(&ours, &zones, stamp);
                    self.set(&store, &mut done.steps, &site, None, Some(&text))?;
                } else {
                    let all: Vec<Event> = rest.into_iter().chain(ours).collect();
                    let text = content::events_text(&all, &zones, stamp);
                    self.set(&store, &mut done.steps, &from, Some(&thing), Some(&text))?;
                }
            }
        }
        if stored.invite {
            let mail = invitation::invite(&event, &self.mine(account));
            let mail = mail.map_err(|why| format!("the guests were not mailed, so nothing was saved: {why}"))?;
            let about = About { event: &event, zones: &zones, answers: None, stamp };
            done.mail = Some(post.send(account, done.serial, &mail, &about)?);
        }
        Ok(())
    }

    /// The event an ask names, as it was last read: of a calendar held,
    /// or of an invitation that is in mail alone.
    pub fn event(&self, id: &EventId) -> Option<&Event> {
        let named = |event: &&Event| event.uid == id.uid && event.instead_of == id.occurrence;
        let kept = self.kept.iter().flatten().filter(|kept| kept.account == id.account.0 && kept.local == id.local);
        let pending = self.mail.pending.iter().filter(|pending| pending.account == id.account.0);
        kept.flat_map(|kept| &kept.calendar.events).find(named).or_else(|| pending.map(|p| &p.event).find(named))
    }

    /// Every event tuimex alone holds, as one calendar for another
    /// program to import (ADR-055); `None` when it holds none.
    pub fn exported(&self) -> Result<Option<String>, Refusal> {
        let (mut events, mut zones) = (Vec::new(), BTreeMap::new());
        for held in self.store()?.held()?.into_iter().filter(|held| held.calendar.is_none()) {
            let parsed = content::calendar(&held.text);
            events.extend(parsed.events);
            for (name, zone) in parsed.zones {
                zones.entry(name).or_insert(zone);
            }
        }
        let undefined: Vec<String> = events.iter().flat_map(named).cloned().collect();
        zone::fill(&mut zones, undefined, &self.about.zone_dirs);
        Ok((!events.is_empty()).then(|| content::events_text(&events, &zones, now())))
    }
}

//! Invitations as mail brings them (ADR-057). One that is in an Inbox and
//! in no calendar is still an event that needs an answer, and for an
//! account whose provider keeps only what was answered in its own apps
//! that is every invitation. A later message for an event held in a
//! local calendar is applied to it: the provider of any other event
//! applies its own.
//!
//! Mail is hostile, and all of this is decided by `invitation::later`:
//! who may change an event is whoever the event already names.
//!
//! What each message turned out to carry is kept under the cache
//! directory, so a start reads the messages that are new and the few
//! still waiting for an answer, and not every invitation ever received.
//! The file is JSON, a list of `[account, key, method, uid]`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use tuimex_content as content;
use tuimex_core::calendar::{Event, Method, Said, Zone};
use tuimex_core::invitation::{self, Later};
use tuimex_core::{Address, Envelope, MessageId};
use tuimex_store::files;

use super::Reader;
use super::carry::{Post, Refusal, find};

#[cfg(test)]
mod tests;

const FILE: &str = "invitations.json";

/// More than the list of any Inbox's invitations comes to.
const LARGEST: u64 = 16 << 20;

/// The event a message's calendar part is about, and what it asks.
#[derive(Debug, Clone)]
pub struct Part {
    pub method: Method,
    pub event: Event,
    pub zones: BTreeMap<String, Zone>,
}

/// The first event of the calendar part of a message; `None` for a
/// message that does not parse, has no such part, or holds no event in
/// it: each is a message that says nothing about any event.
pub fn part(raw: &[u8]) -> Option<Part> {
    let part = content::calendar_part(raw).ok()??;
    let parsed = content::calendar(&part.text);
    let event = parsed.events.into_iter().next()?;
    Some(Part { method: part.method, event, zones: parsed.zones })
}

/// An invitation that is in mail and in no calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub account: String,
    pub event: Event,
    pub zones: BTreeMap<String, Zone>,
}

pub struct Mail {
    file: PathBuf,
    /// What each message looked at carried: its method, as iCalendar
    /// writes it, and its event's UID. Both empty for one with no event.
    seen: BTreeMap<(String, String), (String, String)>,
    /// The parts read in this run.
    read: HashMap<MessageId, Option<Part>>,
    pub pending: Vec<Pending>,
    /// The message each invitation came in, by its event's UID.
    pub came_in: Vec<(String, MessageId)>,
}

fn word(method: Method) -> &'static str {
    match method {
        Method::Request => "REQUEST",
        Method::Reply => "REPLY",
        Method::Cancel => "CANCEL",
        Method::Counter => "COUNTER",
        Method::Other => "OTHER",
    }
}

/// Text from a calendar or a header as one line of a sentence.
fn flat(text: &str) -> String {
    let spaced: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn title(event: &Event) -> String {
    let title = flat(&event.summary);
    if title.is_empty() { "(no title)".to_string() } else { title }
}

fn key(envelope: &Envelope) -> (String, String) {
    (envelope.id.account.0.clone(), envelope.id.key.clone())
}

impl Mail {
    /// What an earlier run learned. A file that is not there or does not
    /// read is nothing learned: it is a cache, and the messages are read
    /// again.
    pub fn open(cache: &Path) -> Mail {
        let file = cache.join(FILE);
        let kept = files::read_own(&file, LARGEST).ok();
        let kept: Vec<(String, String, String, String)> =
            kept.and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        let seen = kept.into_iter().map(|(account, key, method, uid)| ((account, key), (method, uid))).collect();
        Mail { file, seen, read: HashMap::new(), pending: Vec::new(), came_in: Vec::new() }
    }

    fn save(&self) -> Result<(), String> {
        let rows: Vec<(&String, &String, &String, &String)> =
            self.seen.iter().map(|((account, key), (method, uid))| (account, key, method, uid)).collect();
        let failed = |e: &dyn std::fmt::Display| format!("{}: {e}", self.file.display());
        let bytes = serde_json::to_vec(&rows).map_err(|e| failed(&e))?;
        if let Some(dir) = self.file.parent() {
            files::private_dir(dir).map_err(|e| failed(&e))?;
        }
        files::replace(&self.file, &bytes).map_err(|e| failed(&e))
    }

    /// The calendar part of a message, read once in a run.
    fn part(&mut self, post: &Post, id: &MessageId) -> Option<Part> {
        if let Some(read) = self.read.get(id) {
            return read.clone();
        }
        let read = post.message(id).and_then(|(raw, _)| part(&raw));
        self.read.insert(id.clone(), read.clone());
        read
    }

    /// The message the invitation to an event of `account` came in.
    pub fn came_in(&self, uid: &str, account: &str) -> Option<MessageId> {
        let found = self.came_in.iter().find(|(held, id)| held == uid && id.account.0 == account);
        found.map(|(_, id)| id.clone())
    }

    /// The event an answer is for when no calendar holds it: the one
    /// waiting in mail, or the one the message named carries.
    pub fn invitation(
        &mut self,
        account: &str,
        uid: &str,
        message: Option<&MessageId>,
        post: &Post,
    ) -> Option<(Event, BTreeMap<String, Zone>)> {
        let waiting = self.pending.iter().find(|pending| pending.account == account && pending.event.uid == uid);
        if let Some(pending) = waiting {
            return Some((pending.event.clone(), pending.zones.clone()));
        }
        let message = message.filter(|id| id.account.0 == account)?;
        let part = self.part(post, message).filter(|part| part.method == Method::Request && part.event.uid == uid)?;
        Some((part.event, part.zones))
    }
}

impl Reader {
    /// As `scan`, handing the calendar over when what waits in mail is
    /// no longer what it was, or something was applied.
    pub fn look(&mut self, post: &Post) -> Vec<String> {
        let before = (self.mail.pending.clone(), self.mail.came_in.clone());
        let said = self.scan(post);
        if !said.is_empty() || before != (self.mail.pending.clone(), self.mail.came_in.clone()) {
            self.hand();
        }
        said
    }

    /// Looks at the invitations in every Inbox: each message not looked
    /// at before is read and applied to the event a local calendar holds
    /// of it, and what is in no calendar is worked out again. Returns
    /// what there is to say, each thing once: a message is applied once.
    pub fn scan(&mut self, post: &Post) -> Vec<String> {
        let found = post.invitations();
        let (mut said, mut learned) = (Vec::new(), false);
        for envelope in &found {
            if self.mail.seen.contains_key(&key(envelope)) {
                continue;
            }
            learned = true;
            let part = self.mail.part(post, &envelope.id);
            let carried = part.as_ref().map(|part| (word(part.method).to_string(), part.event.uid.clone()));
            self.mail.seen.insert(key(envelope), carried.unwrap_or_default());
            let (Some(part), Some(sender)) = (part, envelope.from.first()) else { continue };
            match self.later(&envelope.id.account.0, &part, sender) {
                Ok(told) => said.extend(told),
                Err(refusal) => {
                    said.push(format!("what mail said of {} was not kept: {}", title(&part.event), refusal.reason))
                }
            }
        }
        let known = self.mail.seen.len();
        let here: std::collections::BTreeSet<(String, String)> = found.iter().map(key).collect();
        self.mail.seen.retain(|key, _| here.contains(key));
        if (learned || self.mail.seen.len() != known)
            && let Err(why) = self.mail.save()
        {
            said.push(why);
        }
        // What was applied changed what is held, which decides below
        // which invitations are still in no calendar.
        if !said.is_empty()
            && let Err(why) = self.load()
        {
            said.push(why);
        }
        self.wait(post, &found);
        said
    }

    /// Works out which invitations are in mail alone, from the messages
    /// of each in the order they came: the request, then whatever its
    /// organizer sent after. A message is read only for an event no
    /// calendar holds and the reader has not ignored, so the work is as
    /// large as what is waiting, not as the Inbox.
    fn wait(&mut self, post: &Post, found: &[Envelope]) {
        let (mut pending, mut came_in): (Vec<Pending>, Vec<_>) = (Vec::new(), Vec::new());
        for envelope in found {
            let account = envelope.id.account.0.as_str();
            let Some((method, uid)) = self.mail.seen.get(&key(envelope)).cloned() else { continue };
            let request = method == word(Method::Request);
            if request {
                came_in.push((uid.clone(), envelope.id.clone()));
            }
            if uid.is_empty() || self.holds(account, &uid) || self.own.ignores(account, &uid) {
                continue;
            }
            let at = pending.iter().position(|p| p.account == account && p.event.uid == uid);
            // An invitation already answered whose event has since gone
            // was deleted by the reader, and does not come back.
            if at.is_none() && (!request || envelope.flags.answered) {
                continue;
            }
            let (Some(part), Some(sender)) = (self.mail.part(post, &envelope.id), envelope.from.first()) else {
                continue;
            };
            let waiting = Pending { account: account.to_string(), event: part.event.clone(), zones: part.zones };
            match at {
                None => pending.push(waiting),
                Some(at) => {
                    match invitation::later(&pending[at].event, &self.mine(account), part.method, &part.event, sender) {
                        Later::Update => pending[at] = waiting,
                        Later::Strike => drop(pending.remove(at)),
                        _ => {}
                    }
                }
            }
        }
        (self.mail.pending, self.mail.came_in) = (pending, came_in);
    }

    /// What a message does to the event a local calendar of `account`
    /// holds of it, and the sentence that says so; `None` when it holds
    /// none or the message changes nothing.
    fn later(&mut self, account: &str, part: &Part, sender: &Address) -> Result<Option<String>, Refusal> {
        let store = self.store()?;
        let incoming = &part.event;
        let Some(thing) = find(&store, account, true, &incoming.uid)? else { return Ok(None) };
        let same = |event: &Event| event.uid == incoming.uid && event.instead_of == incoming.instead_of;
        let Some(held) = thing.parsed.events.iter().find(|event| same(event)) else { return Ok(None) };
        let (title, mut events) = (title(held), thing.parsed.events.clone());
        let who = |guest: &Address| flat(guest.name.as_deref().filter(|name| !name.is_empty()).unwrap_or(&guest.email));
        let said = match invitation::later(held, &self.mine(account), part.method, incoming, sender) {
            Later::Update => {
                events.iter_mut().filter(|event| same(event)).for_each(|event| *event = incoming.clone());
                // A time proposed was for the version this one replaces.
                let proposed = self.own.proposed.len();
                self.own.settle(account, &incoming.uid);
                if self.own.proposed.len() != proposed {
                    self.save_own()?;
                }
                format!("{title} was changed by its organizer")
            }
            // The design draws no cancelled event, so it goes, and this
            // sentence is what says that it did.
            Later::Strike => {
                events.retain(|event| !same(event) && (incoming.instead_of.is_some() || event.uid != incoming.uid));
                format!("{title} was cancelled by its organizer and is removed from {account}, local")
            }
            Later::Answered { guest, said } => {
                let theirs = |event: &&mut Event| same(event);
                for event in events.iter_mut().filter(theirs) {
                    let listed = event.guests.iter_mut().find(|g| g.who.email.eq_ignore_ascii_case(&guest.email));
                    listed.into_iter().for_each(|listed| listed.said = said);
                }
                let how = match said {
                    Said::Accepted => "accepted",
                    Said::Tentative => "may come to",
                    Said::Declined => "declined",
                    Said::Nothing => "has not answered",
                };
                format!("{} {how} {title}", who(&guest))
            }
            Later::Proposed { guest, start, end } => {
                let when = invitation::when(&start, end.as_ref());
                return Ok(Some(format!("{} proposes {} for {title}", who(&guest), flat(&when))));
            }
            Later::Nothing(_) => return Ok(None),
        };
        let mut zones = thing.parsed.zones.clone();
        for (name, zone) in &part.zones {
            zones.entry(name.clone()).or_insert_with(|| zone.clone());
        }
        let text = (!events.is_empty()).then(|| content::events_text(&events, &zones, chrono::Utc::now().timestamp()));
        self.set(&store, &mut Vec::new(), &self.site(account, true), Some(&thing), text.as_deref())?;
        Ok(Some(said))
    }
}

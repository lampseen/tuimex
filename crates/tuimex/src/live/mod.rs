//! The interface over real accounts: one engine for each, the store they
//! share, and a thread between them and the event loop. The loop's thread
//! only ever sends on a channel and looks in one (ADR-006); reading the
//! store, building a message and opening a file are that other thread's.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tuimex_core::{
    AccountId, Action, Address, Ask, Body, Draft, Event, Folder, Hit, Identity, Listed, MessageId, Query, Role,
    Section, Sending, Sort, Timestamp,
};
use tuimex_engine::{Account, Connect, Engine, Engines, StoreHandle};
use tuimex_store::Store;
use tuimex_ui::mailbox::Mailbox;

use crate::config::Config;
use crate::outside::{self, Outside};
use crate::paths::Dirs;
use crate::term::{Backend, Refresh};
use crate::{message, sync};

/// How many asks are remembered for a refusal to name, as the engine
/// remembers them (ADR-040), and how many read messages are kept for the
/// reader's pictures and for `:agent link`.
const ASKS: usize = 200;
const READ: usize = 8;
/// The most a search answers with: more rows than this are not read, and
/// a narrower query finds the rest.
const FOUND: usize = 500;
/// How many of them are read for the words around a match in the body.
const EXCERPTS: usize = 50;

/// PLAN §6.1: the sections a config that names none is given.
const SECTIONS: [(&str, &str, &str); 6] = [
    ("Inbox", "Inbox", "in:inbox"),
    ("Unread", "Unread", "in:inbox is:unread"),
    ("Flagged", "Flag", "is:flagged"),
    ("Drafts", "Drafts", "in:drafts"),
    ("Sent", "Sent", "in:sent"),
    ("Archive", "Archive", "in:archive"),
];

fn now() -> Timestamp {
    Timestamp(chrono::Utc::now().timestamp())
}

/// The words around the first place `text` holds `needle`, which is in
/// lower case: the four before it, the word itself and the three after.
fn around(text: &str, needle: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let first = needle.split_whitespace().next()?;
    let at = words.iter().position(|word| word.to_lowercase().contains(first))?;
    Some(words[at.saturating_sub(4)..words.len().min(at + 4)].join(" "))
}

/// What the loop's thread hands over.
enum Job {
    Ask(Ask),
    Told(Event),
    Stop,
}

/// What it gets back.
enum News {
    Told(Event),
    Rows(Refresh),
    Read(Box<Read>),
    /// The accounts a message is leaving from as everything stops.
    Stopping(Vec<String>),
}

/// A message that was read: the names in its headers, its body as the
/// reader shows it, and its bytes.
struct Read {
    id: MessageId,
    names: Vec<String>,
    body: Body,
    raw: Arc<Vec<u8>>,
}

pub struct Live {
    jobs: Sender<Job>,
    news: Receiver<News>,
    worker: Option<JoinHandle<()>>,
    /// The last few messages read, newest last.
    read: Vec<Read>,
    rows: Vec<Refresh>,
    /// What the worker said after rows the loop has not yet taken.
    next: Option<News>,
    /// What could not be handed over, to be said with the next events.
    failed: Vec<Event>,
}

/// The sections of the config, or the defaults.
pub fn sections(config: &Config) -> Vec<Section> {
    let section = |name: &str, short: &str, query: &str, threaded: bool| Section {
        name: name.to_string(),
        short: short.to_string(),
        query: Query::parse(query),
        sort: Sort::NewestFirst,
        threaded,
    };
    match config.section.as_deref() {
        Some(named) if !named.is_empty() => {
            named.iter().map(|s| section(&s.name, &s.short, &s.query, s.threaded.unwrap_or(true))).collect()
        }
        _ => SECTIONS.iter().map(|(name, short, query)| section(name, short, query, true)).collect(),
    }
}

/// `You <you@example.com>`, or the bare address.
fn address(identity: &str) -> Address {
    match identity.rsplit_once('<').and_then(|(name, rest)| Some((name, rest.split_once('>')?.0))) {
        Some((name, email)) => {
            let name = name.trim().trim_matches('"').trim();
            Address { name: Some(name.to_string()).filter(|n| !n.is_empty()), email: email.trim().to_string() }
        }
        None => Address { name: None, email: identity.trim().to_string() },
    }
}

/// Who mail can be from: each account's address, then its identities.
pub fn identities(config: &Config) -> Vec<Identity> {
    let of = |account: &crate::config::Account| {
        let named = [&account.address].into_iter().chain(account.identities.iter().flatten());
        let id = AccountId(account.name.clone());
        named.map(|text| Identity { account: id.clone(), address: address(text) }).collect::<Vec<_>>()
    };
    config.accounts.iter().flat_map(of).collect()
}

/// The system's opener for the one thing opened from outside the
/// interface: the browser `tuimex setup` sends to Google's sign-in. Made
/// in this module like the interface's own, so that no test can make one.
pub fn opener() -> outside::System {
    outside::System
}

/// Opens the store and starts an engine for every account of the config.
/// Returns the mailbox as the store has it now, before any server has
/// answered, and the seconds a sent message is held for. `said` hears that
/// the store is about to be opened, and then how far a rebuild of its
/// index has come: whoever waits is shown it.
pub fn start(
    config: &Config,
    dirs: &Dirs,
    downloads: PathBuf,
    said: &mut dyn FnMut(Option<(usize, usize)>) -> Result<()>,
) -> Result<(Mailbox, Live, u32)> {
    if config.accounts.is_empty() {
        bail!("the config has no account; `tuimex setup` adds one, and `tuimex --fixtures` runs offline");
    }
    let (policy, delay) = (sync::policy(config.sync.as_ref())?, sync::send_delay(config.send.as_ref())?);
    let mut accounts: Vec<(Account, Box<dyn Connect>)> = Vec::new();
    for account in &config.accounts {
        let (account, reach) = sync::account(account, &policy, delay, &dirs.config)?;
        accounts.push((account, Box::new(reach)));
    }
    said(None)?;
    // A frame that cannot be drawn does not stop the mail being opened;
    // the loop that follows says what is wrong with the terminal.
    let mut rebuilt = |done, total| _ = said(Some((done, total)));
    let store = Store::open_saying(&dirs.data, &dirs.cache, tuimex_engine::parse, &mut rebuilt)
        .context("opening the mail store")?;
    let me = identities(config).into_iter().map(|identity| identity.address.email).collect();
    let cache = dirs.cache.clone();
    // The system's opener is made here and nowhere else: no test may
    // start a browser or a viewer.
    let outside = move || Outside::new(Box::new(outside::System), downloads, &cache);
    let (mailbox, live) = Live::over(store, accounts, sections(config), me, outside)?;
    Ok((mailbox, live, u32::try_from(delay.as_secs()).unwrap_or(u32::MAX)))
}

/// A message in an outbox is waiting to go: the store does not hold for
/// how long, so it is shown the whole delay.
fn held(mail: &mut [Listed], folders: &[Folder], delay: Duration) {
    let seconds = u32::try_from(delay.as_secs()).unwrap_or(u32::MAX);
    for m in mail {
        let its = |f: &&Folder| f.account == m.envelope.id.account && f.id == m.envelope.folder;
        if folders.iter().find(its).is_some_and(|f| f.role == Role::Outbox) {
            m.sending = Some(Sending::Held { seconds });
        }
    }
}

impl Live {
    /// As `start`, over accounts however they are reached.
    pub fn over(
        store: Store,
        accounts: Vec<(Account, Box<dyn Connect>)>,
        sections: Vec<Section>,
        me: Vec<String>,
        outside: impl FnOnce() -> Outside + Send + 'static,
    ) -> Result<(Mailbox, Live)> {
        let (folders, mut mail) = (store.folders()?, store.listed(None)?);
        let delay = accounts.first().map_or(Duration::ZERO, |(account, _)| account.send_delay);
        held(&mut mail, &folders, delay);
        let mailbox = Mailbox { now: now(), sections, folders, mail, synced: None, me };
        let store = StoreHandle::start(store);
        let ((jobs, queue), (told, news), (events, heard)) = (mpsc::channel(), mpsc::channel(), mpsc::channel());
        let start = |(account, connect)| Engine::start(account, connect, store.clone(), events.clone());
        let engines: Vec<Engine> = accounts.into_iter().map(start).collect();
        drop(events);
        let forward = jobs.clone();
        // Ends when the engines have, and with them every sender of events.
        thread::spawn(move || heard.into_iter().try_for_each(|event| forward.send(Job::Told(event))));
        let worker = Worker {
            engines: Engines::new(engines),
            delay,
            store,
            told,
            asked: Vec::new(),
            written: Vec::new(),
            read: Vec::new(),
            after: 0,
            owed: None,
        };
        // What opens things is made on the worker's thread, which is the
        // only one to open anything.
        let worker = thread::spawn(move || worker.run(&queue, &mut outside()));
        let live = Live {
            jobs,
            news,
            worker: Some(worker),
            read: Vec::new(),
            rows: Vec::new(),
            next: None,
            failed: Vec::new(),
        };
        Ok((mailbox, live))
    }

    /// Ends every engine and waits for them, and then for the store to
    /// have written all it was handed. A message being handed to its
    /// server is not cut short (ADR-035), so `waiting` hears which
    /// accounts that wait is for.
    pub fn stop(mut self, waiting: impl Fn(&[String])) {
        // A worker that has gone has stopped the engines already.
        if self.jobs.send(Job::Stop).is_ok() {
            let sending = self.news.iter().find_map(|news| match news {
                News::Stopping(accounts) => Some(accounts),
                _ => None,
            });
            sending.iter().filter(|accounts| !accounts.is_empty()).for_each(|accounts| waiting(accounts));
        }
        if let Some(worker) = self.worker.take() {
            // One that panicked said so on standard error, and holds
            // nothing that is not on disk.
            let _ = worker.join();
        }
    }
}

impl Backend for Live {
    fn ask(&mut self, ask: Ask) {
        let serial = ask.serial;
        if self.jobs.send(Job::Ask(ask)).is_err() {
            let reason = "the mail engine has stopped; restart tuimex".to_string();
            self.failed.push(Event::ActionFailed { serial, reason, hint: String::new() });
        }
    }

    fn events(&mut self) -> Vec<Event> {
        let mut events = std::mem::take(&mut self.failed);
        // The loop takes what is told and then the rows, so what was said
        // after rows waits for them: a refusal puts a row back, and rows
        // read before the refusal would take it away again.
        while let Some(news) = self.next.take().or_else(|| self.news.try_recv().ok()) {
            match news {
                News::Rows(rows) => self.rows.push(rows),
                later if !self.rows.is_empty() => {
                    self.next = Some(later);
                    break;
                }
                News::Told(event) => events.push(event),
                News::Read(read) => {
                    events.push(Event::BodyReady { id: read.id.clone(), body: Box::new(read.body.clone()) });
                    self.read.retain(|earlier| earlier.id != read.id);
                    self.read.push(*read);
                    let over = self.read.len().saturating_sub(READ);
                    self.read.drain(..over);
                }
                News::Stopping(_) => {}
            }
        }
        events
    }

    fn refreshes(&mut self) -> Vec<Refresh> {
        std::mem::take(&mut self.rows)
    }

    /// Always: mail arrives when a server says so, and no key brings it.
    fn working(&self) -> bool {
        true
    }

    fn now(&self) -> Option<Timestamp> {
        Some(now())
    }

    fn picture(&mut self, id: &MessageId, index: usize) -> Option<Vec<u8>> {
        let read = self.read.iter().find(|read| read.id == *id)?;
        tuimex_content::attachment(&read.raw, index).ok().map(|file| file.data)
    }

    fn shown(&mut self, id: &MessageId) -> Option<(Vec<String>, Body)> {
        self.read.iter().find(|read| read.id == *id).map(|read| (read.names.clone(), read.body.clone()))
    }
}

/// The thread between the engines and the loop.
struct Worker {
    engines: Engines,
    /// How long a sent message is held, as the engines hold it.
    delay: Duration,
    store: StoreHandle,
    told: Sender<News>,
    /// The messages each remembered ask named, by its serial.
    asked: Vec<(u64, Vec<MessageId>)>,
    /// What compose last handed over of each draft.
    written: Vec<Draft>,
    read: Vec<(MessageId, Body, Arc<Vec<u8>>)>,
    /// The serial of the last ask taken.
    after: u64,
    /// The messages whose rows the loop is owed, since when: `None` for
    /// every message. They are read once for all the jobs that were
    /// waiting together.
    owed: Option<(Instant, Option<Vec<MessageId>>)>,
}

/// The longest the loop waits for rows while jobs keep coming.
const OWED: Duration = Duration::from_millis(50);

impl Worker {
    fn tell(&self, news: News) {
        // A loop that has gone is one that is stopping; `Stop` follows.
        let _ = self.told.send(news);
    }

    fn refuse(&self, serial: u64, reason: impl Into<String>) {
        self.tell(News::Told(Event::ActionFailed { serial, reason: reason.into(), hint: String::new() }));
    }

    /// Owes the loop what the store holds of `ids`, or with none of
    /// everything.
    fn rows(&mut self, ids: Option<Vec<MessageId>>) {
        let (since, owed) = self.owed.take().unwrap_or_else(|| (Instant::now(), Some(Vec::new())));
        let both = owed.zip(ids).map(|(mut owed, ids)| {
            owed.extend(ids);
            owed
        });
        self.owed = Some((since, both));
    }

    /// Hands the loop the rows it is owed. Read after an engine was asked,
    /// they are read after the engine's change: the store's thread takes
    /// its work in order.
    fn hand_rows(&mut self) {
        let Some((_, mut ids)) = self.owed.take() else { return };
        // A message two jobs named is one row.
        let mut seen = std::collections::HashSet::new();
        ids.iter_mut().for_each(|ids| ids.retain(|id| seen.insert(id.clone())));
        let named = ids.clone();
        let read = self.store.run(move |s| Ok((s.listed(named.as_deref())?, s.folders()?)));
        match read {
            Ok((mut rows, folders)) => {
                held(&mut rows, &folders, self.delay);
                self.tell(News::Rows(Refresh { now: now(), after: self.after, asked: ids, rows, folders }));
            }
            Err(error) => {
                let (account, text) = (AccountId(String::new()), format!("the mail store: {error}"));
                self.tell(News::Told(Event::Notice { account, error: true, text, hint: String::new() }));
            }
        }
    }

    /// The draft `id` as the bytes of a message, filed in Drafts under the
    /// same name: with its `Bcc`, which the engine withholds from what it
    /// sends and addresses the envelope from (ADR-035).
    fn file(&self, id: &MessageId) -> Result<(), String> {
        let Some(draft) = self.written.iter().find(|draft| draft.id == *id) else { return Ok(()) };
        let engine = self.engines.of(&id.account.0).ok_or_else(|| format!("no account {}", id.account.0))?;
        let mut files = Vec::new();
        for file in &draft.files {
            if let Some(path) = &file.path {
                let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
                files.push(message::File { name: file.name.clone(), bytes });
            }
        }
        let unique = tuimex_store::maildir::new_key();
        let raw = message::build(draft, files, now().0, &unique, message::Bcc::Written)?;
        engine.save_draft(raw, Some(id.clone()), draft.origin);
        Ok(())
    }

    fn ask(&mut self, ask: Ask, outside: &mut Outside) {
        let serial = ask.serial;
        self.after = serial;
        match &ask.action {
            Action::Written(draft) => {
                self.written.retain(|d| d.id != draft.id);
                self.written.push((**draft).clone());
                return;
            }
            Action::Read => {
                let asked = |id: &MessageId| match self.engines.of(&id.account.0) {
                    Some(engine) => engine.body(id.clone(), outside::UNWRAPPED),
                    None => self.refuse(serial, format!("no account {}", id.account.0)),
                };
                return ask.on.iter().for_each(asked);
            }
            Action::OpenLink(_)
            | Action::SaveAttachment(..)
            | Action::OpenAttachment(_)
            | Action::Browser { .. }
            | Action::Unsubscribe => {
                let read = ask.on.first().and_then(|id| self.read.iter().find(|(read, ..)| read == id));
                let Some((_, body, raw)) = read else { return self.refuse(serial, "the message has not been read") };
                let told = outside.carry_out(&ask, body, Some(raw));
                return told.into_iter().for_each(|event| self.tell(News::Told(event)));
            }
            Action::Search(query) => return self.search(serial, query),
            Action::SaveDraft | Action::Send => {
                if let Some(why) = ask.on.iter().find_map(|id| self.file(id).err()) {
                    return self.refuse(serial, why);
                }
                if ask.action == Action::SaveDraft {
                    return self.rows(Some(ask.on));
                }
            }
            _ => {}
        }
        let whole = matches!(ask.action, Action::EmptyTrash(_) | Action::CreateArchive(_) | Action::CreateTrash(_));
        let on = ask.on.clone();
        self.asked.push((serial, on.clone()));
        let over = self.asked.len().saturating_sub(ASKS);
        self.asked.drain(..over);
        self.engines.act(ask);
        if whole || !on.is_empty() {
            self.rows((!whole).then_some(on));
        }
    }

    /// Asks the store's index for `query`, of every account and folder
    /// and through whole bodies, on the store's thread: the loop hears the
    /// answer when it is there, and waits for nothing. A message whose
    /// subject and sender do not hold the words is read for the words
    /// around them, the first `EXCERPTS` such messages only.
    fn search(&self, serial: u64, text: &str) {
        let query = Query::parse(text);
        let found = self.store.run(move |s| {
            let needle = query.terms.iter().find_map(|term| match term {
                tuimex_core::Term::Text(text) => Some(text.to_lowercase()),
                _ => None,
            });
            let mut read = 0;
            let mut hits = Vec::new();
            for e in s.query(&query, now(), Sort::NewestFirst, Some(FOUND))? {
                let shown = |text: &str| needle.as_ref().is_none_or(|n| text.to_lowercase().contains(n));
                let named = e.from.iter().any(|a| shown(&a.email) || a.name.as_deref().is_some_and(shown));
                let mut excerpt = None;
                if !shown(&e.subject) && !named && read < EXCERPTS {
                    read += 1;
                    // A message that cannot be read is still one found.
                    let body = s.read(&e.id).ok().and_then(|raw| outside::read(&raw).ok());
                    excerpt = body.zip(needle.as_ref()).and_then(|(body, needle)| around(&body.text, needle));
                }
                hits.push(Hit { id: e.id, excerpt });
            }
            Ok((hits, s.folders()?.iter().map(|f| f.total).sum()))
        });
        match found {
            Ok((hits, searched)) => self.tell(News::Told(Event::Found { serial, hits, searched })),
            Err(error) => self.refuse(serial, format!("the mail store: {error}")),
        }
    }

    fn told(&mut self, event: Event) {
        match event {
            // The interface said what an ask did when it was asked
            // (ADR-040); the engine's word for it is not said twice.
            Event::ActionDone { .. } => {}
            Event::Synced { ref changed, .. } => {
                let changed = changed.clone();
                self.tell(News::Told(event));
                if !changed.is_empty() {
                    self.rows(Some(changed));
                }
            }
            Event::ActionFailed { serial, .. } => {
                let on = self.asked.iter().find(|(asked, _)| *asked == serial).map(|(_, on)| on.clone());
                self.tell(News::Told(event));
                // Whatever the interface puts back, the store has the
                // last word on those messages.
                on.into_iter().for_each(|on| self.rows(Some(on)));
            }
            Event::Sent { ref id, ref answered } => {
                let rows = std::iter::once(id).chain(answered).cloned().collect();
                self.tell(News::Told(event));
                self.rows(Some(rows));
            }
            Event::SendFailed { ref id, .. } => {
                let id = id.clone();
                self.tell(News::Told(event));
                self.rows(Some(vec![id]));
            }
            Event::BodyReady { id, body } => self.read(id, body),
            other => self.tell(News::Told(other)),
        }
    }

    /// A body the engine read: with the sizes of its pictures, the names
    /// its headers hold, and its bytes, for what is asked of it next.
    fn read(&mut self, id: MessageId, body: Box<Body>) {
        let asked = id.clone();
        let stored = self.store.run(move |s| Ok((s.read(&asked)?, s.envelope(&asked)?)));
        let Ok((raw, envelope)) = stored else {
            // Gone since it was read: the body is still what was asked for.
            return self.tell(News::Told(Event::BodyReady { id, body }));
        };
        let body = outside::read(&raw).map_or(body, Box::new);
        let people = envelope.iter().flat_map(|e| e.from.iter().chain(&e.to).chain(&e.cc).chain(&e.reply_to));
        let names = people.filter_map(|a| a.name.clone()).collect();
        let raw = Arc::new(raw);
        self.read.retain(|(read, ..)| *read != id);
        self.read.push((id.clone(), (*body).clone(), raw.clone()));
        let over = self.read.len().saturating_sub(READ);
        self.read.drain(..over);
        self.tell(News::Read(Box::new(Read { id: id.clone(), names, body: *body, raw })));
        // A header block that was filled has an opening now.
        self.rows(Some(vec![id]));
    }

    fn run(mut self, queue: &Receiver<Job>, outside: &mut Outside) {
        loop {
            // A key held on an action is an ask for each repeat, and a
            // sync says what changed folder by folder: the rows for all
            // that is waiting are read once, and the loop takes them in
            // one refresh.
            let late = self.owed.as_ref().is_some_and(|(since, _)| since.elapsed() > OWED);
            let waiting = if late { None } else { queue.try_recv().ok() };
            let job = waiting.or_else(|| {
                self.hand_rows();
                queue.recv().ok()
            });
            match job {
                Some(Job::Ask(ask)) => self.ask(ask, outside),
                Some(Job::Told(event)) => self.told(event),
                Some(Job::Stop) | None => break,
            }
        }
        let sending = self.engines.sending().into_iter().map(String::from).collect();
        self.tell(News::Stopping(sending));
        self.engines.stop();
        // After the engines, which hand the store work until they end:
        // what was asked last is written before the program leaves.
        self.store.finish();
    }
}

#[cfg(test)]
mod tests;

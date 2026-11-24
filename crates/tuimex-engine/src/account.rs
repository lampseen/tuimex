//! One account's threads (PLAN §4.3, ADR-006): a connection that syncs
//! and tells the server what changed here, one that idles on the Inbox,
//! one that opens only to fetch a body the reader is waiting for, and the
//! thread that sends. They share nothing but channels.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tuimex_core::{
    AccountId, Action, Ask, Body, Deadline, Event, Interrupt, MessageId, Origin, RemoteError, RemoteWrite, Role,
};
use tuimex_store::Store;
use tuimex_store::maildir::new_key;

use crate::act::{self, Done, Last, Refusal};
use crate::outbox::Post;
use crate::sync::{self, Policy};
use crate::{Backoff, Error, Letter, Report, StoreHandle, replay};

/// RFC 2177: a server may drop a client that idles longer than half an
/// hour, so the idle is ended and begun again before that.
const IDLE_FOR: Duration = Duration::from_secs(29 * 60);
/// How often a server without IDLE is asked what changed.
const POLL: Duration = Duration::from_secs(2 * 60);
/// The body connection closes after this long unused.
const UNUSED: Duration = Duration::from_secs(60);
/// How many asks are remembered for `u` and for a late refusal to name.
/// One older than that is refused as a `Notice`, and `u` cannot reach it.
const REMEMBERED: usize = 200;

#[derive(Debug, Clone)]
pub struct Account {
    /// The config's name for the account, which is also its directory.
    pub name: String,
    /// The roles the config fills by hand (`folders.archive = "…"`).
    pub roles: Vec<(Role, String)>,
    pub policy: Policy,
    /// The addresses the account sends from: its own, and its identities.
    pub identities: Vec<String>,
    /// How long a message waits in the Outbox before it is sent.
    pub send_delay: Duration,
}

/// Reaches the account's servers: the real one resolves the account's
/// secret and speaks IMAP and SMTP, a test's hands out a fake.
pub trait Connect: Send + Sync {
    /// A signed-in connection.
    fn connect(&self) -> Result<Box<dyn RemoteWrite>, Error>;
    /// Hands a message to the server that sends the account's mail.
    fn send(&self, letter: &Letter) -> Result<(), Error>;
}

pub(crate) enum Poke {
    /// A pass over every folder.
    Sync,
    /// A pass over the folders the journal names.
    Replay,
    Stop,
}

pub(crate) struct Shared {
    pub(crate) account: Account,
    pub(crate) connect: Box<dyn Connect>,
    pub(crate) store: StoreHandle,
    events: Sender<Event>,
    stopping: AtomicBool,
    interrupts: Sender<Interrupt>,
    pokes: Sender<Poke>,
    pub(crate) posts: Sender<Post>,
    /// Set while a message is being handed to the server that sends.
    pub(crate) sending: AtomicBool,
    /// The messages claimed for sending and not yet filed or given up.
    /// Claimed and released on the store's thread alone, so a message is
    /// either still held when `u` arrives or already the server's.
    pub(crate) leaving: Arc<Mutex<HashSet<MessageId>>>,
    /// What this run's asks did, oldest first (ADR-040).
    done: Mutex<Vec<Done>>,
}

/// A running account. Dropping it without `stop` leaves its threads to
/// end when they next notice the channels have closed.
pub struct Engine {
    shared: Arc<Shared>,
    wanted: Sender<(MessageId, usize)>,
    halt: Sender<()>,
    interrupts: Receiver<Interrupt>,
    threads: Vec<JoinHandle<()>>,
}

impl Engine {
    /// Starts the account's threads; the first sync begins at once. What
    /// happens is reported on `events`.
    pub fn start(account: Account, connect: Box<dyn Connect>, store: StoreHandle, events: Sender<Event>) -> Engine {
        let ((interrupts, fired), (pokes, poked), (posts, posted)) =
            (mpsc::channel(), mpsc::channel(), mpsc::channel());
        let ((wanted, asked), (halt, halted)) = (mpsc::channel(), mpsc::channel());
        let (stopping, done) = (AtomicBool::new(false), Mutex::new(Vec::new()));
        let (sending, leaving) = (AtomicBool::new(false), Arc::default());
        let shared = Arc::new(Shared {
            account,
            connect,
            store,
            events,
            stopping,
            interrupts,
            pokes,
            posts,
            sending,
            leaving,
            done,
        });
        let (syncing, idling, reading, sending) = (shared.clone(), shared.clone(), shared.clone(), shared.clone());
        let threads = vec![
            thread::spawn(move || syncing.syncer(&poked)),
            thread::spawn(move || idling.idler(&halted)),
            thread::spawn(move || reading.reader(&asked)),
            thread::spawn(move || sending.sender(&posted)),
        ];
        Engine { shared, wanted, halt, interrupts: fired, threads }
    }

    pub fn account(&self) -> &str {
        &self.shared.account.name
    }

    /// Whether a message is being handed over for sending right now.
    /// `stop` waits for that to end, for as long as a silent server is
    /// given (ADR-035), so whoever stops says what the wait is for.
    pub fn sending(&self) -> bool {
        self.shared.sending.load(Ordering::SeqCst)
    }

    /// Asks for a sync pass now, which also cuts a backoff wait short.
    pub fn sync(&self) {
        self.shared.poke(Poke::Sync);
    }

    /// Does what `ask` asks to the messages it names, all of them this
    /// account's, and answers on the events: `ActionDone` once it is done
    /// here, `ActionFailed` naming the ask if it was not, or later if the
    /// server refuses it, and `Sent` or `SendFailed` for each message
    /// `Send` queued. `Undo` takes back the newest ask carried out over
    /// the messages it names (ADR-040). Nothing here waits (ADR-006): the
    /// work is the store's thread's, and telling the server is the syncing
    /// connection's.
    pub fn act(&self, ask: Ask) {
        if matches!(ask.action, Action::Sync { .. }) {
            return self.sync();
        }
        let (shared, serial) = (self.shared.clone(), ask.serial);
        if !self.shared.store.hand(move |store| shared.act(store, ask)) {
            self.shared.refuse(serial, Error::StoreStopped.into());
        }
    }

    /// Files `message` as a draft and names it at once; the file is there
    /// by the time anything asked afterwards is done. `replaces` is the
    /// version of the draft this one takes the place of, whose name it
    /// keeps, so that whoever wrote it goes on naming it as they did.
    pub fn save_draft(&self, message: Vec<u8>, replaces: Option<MessageId>, origin: Origin) -> MessageId {
        let account = AccountId(self.shared.account.name.clone());
        let id = MessageId { account, key: replaces.map_or_else(new_key, |old| old.key) };
        let (shared, saved) = (self.shared.clone(), id.clone());
        let save = move |store: &Store| {
            if let Err(e) = act::save_draft(store, &saved, &message, origin) {
                shared.notice(true, e.to_string(), String::new());
            }
        };
        if !self.shared.store.hand(save) {
            self.shared.notice(true, Error::StoreStopped.to_string(), String::new());
        }
        id
    }

    /// Asks for a message as the reader shows it at `width`. The answer is
    /// `BodyReady` or `BodyFailed`.
    pub fn body(&self, id: MessageId, width: usize) {
        if self.wanted.send((id.clone(), width)).is_err() {
            self.shared.emit(Event::BodyFailed { id, reason: "the engine has stopped".to_string() });
        }
    }

    /// Ends every thread and waits for them. A call blocked on the network
    /// is ended by closing its connection (ADR-006).
    pub fn stop(self) {
        let Engine { shared, wanted, halt, interrupts, threads } = self;
        shared.stopping.store(true, Ordering::SeqCst);
        shared.poke(Poke::Stop);
        // A sender that has already ended has nobody listening, which is
        // the aim. One in the middle of a message finishes handing it
        // over first: cut short, nobody would know whether it was sent.
        let _ = shared.posts.send(Post::Stop);
        drop((wanted, halt));
        interrupts.try_iter().for_each(|interrupt| interrupt.fire());
        for thread in threads {
            // A thread that panicked has already reported itself on
            // standard error, and there is nothing of its to clean up.
            let _ = thread.join();
        }
    }
}

/// Every account's engine behind one door, for an interface that shows
/// them as one list: an ask goes to the account of each message it names,
/// under the one serial, so each account answers for its share.
pub struct Engines {
    engines: Vec<Engine>,
}

impl Engines {
    pub fn new(engines: Vec<Engine>) -> Engines {
        Engines { engines }
    }

    /// The engine of one account, for what names no message: a draft to
    /// save, a sync.
    pub fn of(&self, account: &str) -> Option<&Engine> {
        self.engines.iter().find(|engine| engine.account() == account)
    }

    /// As `Engine::act`, for messages of any account. A sync is every
    /// account's.
    pub fn act(&self, ask: Ask) {
        if matches!(ask.action, Action::Sync { .. }) {
            return self.engines.iter().for_each(Engine::sync);
        }
        if let Action::EmptyTrash(AccountId(account))
        | Action::CreateArchive(AccountId(account))
        | Action::CreateTrash(AccountId(account)) = &ask.action
        {
            return match (self.of(account), self.engines.first()) {
                (Some(engine), _) => engine.act(ask),
                (None, Some(any)) => any.shared.refuse(ask.serial, Refusal::from(format!("no account {account}"))),
                (None, None) => {}
            };
        }
        for engine in &self.engines {
            let on: Vec<MessageId> = ask.on.iter().filter(|id| id.account.0 == engine.account()).cloned().collect();
            if !on.is_empty() {
                engine.act(Ask { serial: ask.serial, action: ask.action.clone(), on });
            }
        }
        let strangers = ask.on.iter().filter(|id| self.of(&id.account.0).is_none()).count();
        if let (true, Some(any)) = (strangers > 0, self.engines.first()) {
            let refusal = Refusal::from(format!("{} of no account in the config", act::count(strangers)));
            any.shared.refuse(ask.serial, refusal);
        }
    }

    /// The accounts with a message on its way out at this moment.
    pub fn sending(&self) -> Vec<&str> {
        self.engines.iter().filter(|engine| engine.sending()).map(Engine::account).collect()
    }

    pub fn stop(self) {
        self.engines.into_iter().for_each(Engine::stop);
    }
}

impl Shared {
    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    pub(crate) fn emit(&self, event: Event) {
        // Nobody is listening once the interface has gone, and the event
        // was for it alone.
        let _ = self.events.send(event);
    }

    pub(crate) fn poke(&self, poke: Poke) {
        // A syncer that has gone is one that was stopped; there is
        // nothing left to ask.
        let _ = self.pokes.send(poke);
    }

    pub(crate) fn refuse(&self, serial: u64, Refusal { reason, hint }: Refusal) {
        self.emit(Event::ActionFailed { serial, reason, hint });
    }

    fn notice(&self, error: bool, text: String, hint: String) {
        self.emit(Event::Notice { account: AccountId(self.account.name.clone()), error, text, hint });
    }

    /// A thread that panicked while it held this did so between two whole
    /// pushes or removals; what `u` would take back is still what it says.
    fn done(&self) -> MutexGuard<'_, Vec<Done>> {
        self.done.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What a pass reports, as events. A refusal names the ask that made
    /// the journal entries refused, which `u` can then no longer reach;
    /// entries no ask of this run made are refused as a `Notice`.
    fn report(&self, report: Report) {
        let (seqs, reason, hint) = match report {
            Report::Event(event) => return self.emit(event),
            Report::Refused { seqs, reason, hint } => (seqs, reason, hint),
        };
        let mut done = self.done();
        let refused =
            |d: &Done| matches!(&d.last, Some(Last::Changed(made)) if made.iter().any(|(_, e)| seqs.contains(&e.seq)));
        let mut serials: Vec<u64> = done.iter().filter(|d| refused(d)).map(|d| d.serial).collect();
        done.retain(|d| !refused(d));
        drop(done);
        serials.dedup();
        if serials.is_empty() {
            return self.notice(true, replay::unasked(&self.account.name, &reason), hint);
        }
        for serial in serials {
            self.emit(Event::ActionFailed { serial, reason: reason.clone(), hint: hint.clone() });
        }
    }

    /// One ask, on the store's thread.
    fn act(&self, store: &Store, Ask { serial, action, mut on }: Ask) {
        on.sort_by(|a, b| a.key.cmp(&b.key));
        let done = if action == Action::Undo {
            let mut all = self.done();
            let asked = all.iter().rposition(|d| d.on == on).map(|at| all.remove(at));
            drop(all);
            match asked.map(|d| d.last) {
                Some(Some(Last::Changed(made))) => act::undo(store, &made).map(|said| (said, None)).map_err(Into::into),
                Some(Some(Last::Held(held))) => self.unsend(store, held),
                Some(None) => Err(Refusal::from("there is nothing to undo")),
                // Mail an earlier run left waiting is no ask of this one.
                None => match act::held(store, &on) {
                    Ok(held) if !held.is_empty() => self.unsend(store, held),
                    Ok(_) => Err(Refusal::from("there is nothing to undo")),
                    Err(e) => Err(e.into()),
                },
            }
        } else if let Action::EmptyTrash(account) = &action {
            act::empty_trash(store, &self.account.name).map(|(deleted, kept)| {
                self.emit(Event::Emptied { serial, account: account.clone(), deleted, kept });
                let stay = if kept == 0 {
                    String::new()
                } else {
                    format!("; {} not yet on the server stay", act::count(kept))
                };
                (format!("Deleted {} from {}'s Trash{stay}", act::count(deleted), account.0), None)
            })
        } else {
            act::perform(store, &self.account, &action, &on)
        };
        match done {
            Ok((summary, last)) => {
                if let Some(Last::Held(held)) = &last {
                    // A sender that has stopped is an engine that has.
                    let _ = self.posts.send(Post::Queue(held.clone()));
                }
                if action != Action::Undo {
                    let mut all = self.done();
                    all.push(Done { serial, on, last });
                    let over = all.len().saturating_sub(REMEMBERED);
                    all.drain(..over);
                }
                self.emit(Event::ActionDone { summary });
                self.poke(Poke::Replay);
            }
            Err(refusal) => self.refuse(serial, refusal),
        }
    }

    /// A new connection, which `stop` can close from its own thread. One
    /// opened while stopping is closed here, since `stop` may already have
    /// fired every interrupt it was sent.
    fn connect(&self) -> Result<Box<dyn RemoteWrite>, Error> {
        let remote = self.connect.connect()?;
        let interrupt = remote.interrupt();
        if self.interrupts.send(interrupt.clone()).is_err() || self.stopping() {
            interrupt.fire();
        }
        Ok(remote)
    }

    /// Syncs once at the start and again whenever poked, and after a lost
    /// connection tries again by itself with backoff.
    fn syncer(&self, pokes: &Receiver<Poke>) {
        let (mut remote, mut backoff, mut whole) = (None, Backoff::default(), true);
        while !self.stopping() {
            let connection = match &mut remote {
                Some(remote) => Ok(remote),
                None => self.connect().map(|new| remote.insert(new)),
            };
            let tell = |report| self.report(report);
            let pass = connection.and_then(|r| match whole {
                true => sync::account(r.as_mut(), &self.store, &self.account, sync::now(), &tell),
                false => replay::account(r.as_mut(), &self.store, &self.account, &tell),
            });
            let wait = match pass {
                Ok(()) => {
                    backoff.reset();
                    None
                }
                Err(_) if self.stopping() => break,
                Err(error) => {
                    remote = None;
                    let retry_in = error.retries().then(|| backoff.wait());
                    let (account, msg) = (AccountId(self.account.name.clone()), error.advice(&self.account.name));
                    self.emit(Event::SyncError { account, msg, retry_in });
                    retry_in
                }
            };
            let first = match wait {
                None => pokes.recv().ok(),
                Some(wait) => match pokes.recv_timeout(wait) {
                    Err(RecvTimeoutError::Timeout) => Some(Poke::Sync),
                    other => other.ok(),
                },
            };
            // Pokes that piled up during one pass ask for one more pass,
            // not one each, and for a whole one if any of them did. A
            // `Stop` among them is not lost: `stopping` was set before it
            // was sent.
            let asked: Vec<Poke> = first.into_iter().chain(pokes.try_iter()).collect();
            if asked.is_empty() || asked.iter().any(|poke| matches!(poke, Poke::Stop)) {
                break;
            }
            // After a failure the pass is a whole one too: anything may
            // have changed while there was no connection.
            whole = remote.is_none() || asked.iter().any(|poke| matches!(poke, Poke::Sync));
        }
    }

    /// One step of watching the Inbox, and how long to rest before the
    /// next. Returning means "sync now": the connection has just opened,
    /// so anything may have changed while nobody watched; or the Inbox
    /// changed; or enough time has passed to look at the other folders.
    fn watch(&self, remote: &mut Option<Box<dyn RemoteWrite>>) -> Result<Duration, Error> {
        let Some(remote) = remote else {
            let mut new = self.connect()?;
            new.select("INBOX")?;
            *remote = Some(new);
            return Ok(Duration::ZERO);
        };
        if !remote.capabilities().idle {
            return Ok(POLL);
        }
        remote.idle(Deadline(Instant::now() + IDLE_FOR))?;
        Ok(Duration::ZERO)
    }

    fn idler(&self, halt: &Receiver<()>) {
        let (mut remote, mut backoff) = (None, Backoff::default());
        while !self.stopping() {
            let rest = match self.watch(&mut remote) {
                Ok(rest) => {
                    backoff.reset();
                    self.poke(Poke::Sync);
                    rest
                }
                // Not reported from here: the syncer signs in the same way
                // and reports the same refusal once. Nothing can be
                // watched until the user has changed something.
                Err(Error::Remote(RemoteError::Auth) | Error::Secret(_)) => break,
                // Nor is this: an outage is the syncer's to report, and a
                // watch that fails alone costs only the wait before it is
                // opened again, which then asks for a sync.
                Err(_) => {
                    remote = None;
                    backoff.wait()
                }
            };
            if !matches!(halt.recv_timeout(rest), Err(RecvTimeoutError::Timeout)) {
                break;
            }
        }
    }

    fn reader(&self, wanted: &Receiver<(MessageId, usize)>) {
        let mut remote = None;
        loop {
            match wanted.recv_timeout(UNUSED) {
                Ok((id, width)) => self.emit(match self.body(&mut remote, &id, width) {
                    Ok(body) => Event::BodyReady { id, body: Box::new(body) },
                    Err(error) => {
                        // Whatever went wrong, the next body starts on a
                        // connection in a known state.
                        remote = None;
                        Event::BodyFailed { id, reason: error.to_string() }
                    }
                }),
                Err(RecvTimeoutError::Timeout) => remote = None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    /// The message as the reader shows it, fetching its body first if only
    /// its headers are here.
    fn body(&self, remote: &mut Option<Box<dyn RemoteWrite>>, id: &MessageId, width: usize) -> Result<Body, Error> {
        let asked = id.clone();
        let (folder, stored, uid_validity) = self.store.run(move |s| {
            let (folder, stored) = s.stored(&asked)?;
            let uid_validity = s.position(&asked.account.0, &folder)?.uid_validity;
            Ok((folder, stored, uid_validity))
        })?;
        let asked = id.clone();
        if !stored.headers_only {
            let raw = self.store.run(move |s| s.read(&asked))?;
            return Ok(tuimex_content::body(&raw, width)?);
        }
        let remote = match remote {
            Some(remote) => remote,
            None => remote.insert(self.connect()?),
        };
        // A UID means something only under the UIDVALIDITY it was issued
        // with (ADR-014): under another it may be a different message.
        let selected = remote.select(&folder)?;
        let Some(uid) = stored.uid.filter(|_| Some(selected.uid_validity) == uid_validity) else {
            return Err(Error::Stale);
        };
        let raw = remote.body(uid)?;
        let body = tuimex_content::body(&raw, width)?;
        self.store.run(move |s| s.fill(&asked, &raw))?;
        Ok(body)
    }
}

#[cfg(test)]
mod tests;

//! A mail server in memory, and connections to it that implement `Remote`
//! (PLAN §4). A test holds the `FakeServer` to play everyone else: other
//! clients changing the mailbox, and a network that fails on cue.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use tuimex_core::{
    Capabilities, Deadline, Fetched, FlagChange, Flags, Interrupt, MoveOutcome, Remote, RemoteError, RemoteFolder,
    RemoteWrite, Selected, Timestamp, UidRange, Woken,
};

#[derive(Clone)]
struct Message {
    uid: u32,
    raw: Vec<u8>,
    flags: Flags,
    arrived: Timestamp,
    modseq: u64,
    /// Marked `\\Deleted`, and here until something purges it.
    deleted: bool,
}

struct Folder {
    name: String,
    attributes: Vec<String>,
    uid_validity: u32,
    uid_next: u32,
    /// Counts every change to the folder; a message's is its last change.
    modseq: u64,
    messages: Vec<Message>,
}

impl Folder {
    fn add(&mut self, raw: &[u8], flags: Flags, arrived: Timestamp) -> u32 {
        let uid = self.uid_next;
        (self.uid_next, self.modseq) = (uid + 1, self.modseq + 1);
        self.messages.push(Message { uid, raw: raw.to_vec(), flags, arrived, modseq: self.modseq, deleted: false });
        uid
    }

    fn message(&mut self, uid: u32) -> Result<&mut Message, RemoteError> {
        let missing = RemoteError::Refused(format!("no message {uid}"));
        self.messages.iter_mut().find(|m| m.uid == uid).ok_or(missing)
    }

    /// The messages a command naming `uids` acts on: a UID the folder
    /// does not have is passed over without a word, as IMAP has it.
    fn named(&mut self, uids: &[u32]) -> Vec<&mut Message> {
        self.modseq += 1;
        let mut named: Vec<&mut Message> = self.messages.iter_mut().filter(|m| uids.contains(&m.uid)).collect();
        named.iter_mut().for_each(|m| m.modseq = self.modseq);
        named
    }
}

struct State {
    password: String,
    capabilities: Capabilities,
    folders: Vec<Folder>,
    /// The commands still to be answered before every open connection is
    /// lost.
    drop_after: Option<usize>,
    /// How many times that has happened: a connection opened before the
    /// last time is a lost one.
    drops: u64,
    refused_connections: usize,
    refuse_moves: bool,
    /// What the name of a folder made here has to begin with.
    namespace: String,
    log: Vec<String>,
}

impl State {
    fn folder(&mut self, name: &str) -> Result<&mut Folder, RemoteError> {
        let missing = RemoteError::Refused(format!("no folder {name}"));
        self.folders.iter_mut().find(|f| f.name == name).ok_or(missing)
    }
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

/// The server and the hand that reaches into it. Cloning gives another
/// hand on the same server.
#[derive(Clone)]
pub struct FakeServer(Arc<Shared>);

impl FakeServer {
    pub fn new(password: &str, capabilities: Capabilities) -> FakeServer {
        let state = State {
            password: password.to_string(),
            capabilities,
            folders: Vec::new(),
            drop_after: None,
            drops: 0,
            refused_connections: 0,
            refuse_moves: false,
            namespace: String::new(),
            log: Vec::new(),
        };
        FakeServer(Arc::new(Shared { state: Mutex::new(state), changed: Condvar::new() }))
    }

    /// A panic in another test thread must not hide this test's own
    /// failure behind a poisoned lock; the state is plain data either way.
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes a change as another client would, and wakes whoever idles.
    fn change<T>(&self, folder: &str, change: impl FnOnce(&mut Folder) -> T) -> T {
        let mut state = self.state();
        let Some(found) = state.folders.iter_mut().find(|f| f.name == folder) else {
            panic!("the test's server has no folder {folder}")
        };
        found.modseq += 1;
        let made = change(found);
        self.0.changed.notify_all();
        made
    }

    /// `attributes` are SPECIAL-USE ones, such as `\Trash`.
    pub fn add_folder(&self, name: &str, attributes: &[&str], uid_validity: u32) {
        let attributes = attributes.iter().map(ToString::to_string).collect();
        let folder =
            Folder { name: name.to_string(), attributes, uid_validity, uid_next: 1, modseq: 1, messages: vec![] };
        self.state().folders.push(folder);
    }

    /// For a server that has no such folder, where a test's usual one does.
    /// Keeps a person's folders under `prefix`, as a server that has them
    /// all beneath the Inbox does with `INBOX.`: a folder is made nowhere
    /// else, and the prefix's last character is the delimiter.
    pub fn keep_folders_under(&self, prefix: &str) {
        self.state().namespace = prefix.to_string();
    }

    pub fn remove_folder(&self, name: &str) {
        self.state().folders.retain(|f| f.name != name);
    }

    /// The new message's UID.
    pub fn deliver(&self, folder: &str, raw: &[u8], flags: Flags, arrived: Timestamp) -> u32 {
        self.change(folder, |f| f.add(raw, flags, arrived))
    }

    pub fn remove(&self, folder: &str, uid: u32) {
        self.change(folder, |f| f.messages.retain(|m| m.uid != uid));
    }

    pub fn set_flags(&self, folder: &str, uid: u32, flags: Flags) {
        self.change(folder, |f| {
            let modseq = f.modseq;
            for message in f.messages.iter_mut().filter(|m| m.uid == uid) {
                (message.flags, message.modseq) = (flags, modseq);
            }
        });
    }

    pub fn flags(&self, folder: &str, uid: u32) -> Option<Flags> {
        self.state().folders.iter().find(|f| f.name == folder)?.messages.iter().find(|m| m.uid == uid).map(|m| m.flags)
    }

    /// Gives the folder a new UIDVALIDITY and numbers its messages again
    /// from one, as a server restored from a backup does.
    pub fn renumber(&self, folder: &str, uid_validity: u32) {
        self.change(folder, |f| {
            f.uid_validity = uid_validity;
            for (n, message) in (1..).zip(&mut f.messages) {
                message.uid = n;
            }
            f.uid_next = f.messages.len() as u32 + 1;
        });
    }

    /// Every connection open by then is lost once `commands` more have been
    /// answered. One opened afterwards works.
    pub fn drop_after(&self, commands: usize) {
        self.state().drop_after = Some(commands);
    }

    /// The next `times` attempts to connect find nobody listening.
    pub fn refuse_connections(&self, times: usize) {
        self.state().refused_connections = times;
    }

    pub fn refuse_moves(&self) {
        self.state().refuse_moves = true;
    }

    /// Every command answered since this was last asked, oldest first.
    pub fn take_log(&self) -> Vec<String> {
        std::mem::take(&mut self.state().log)
    }

    /// Signs in. A wrong password is refused without a word about it.
    pub fn connect(&self, password: &str) -> Result<FakeRemote, RemoteError> {
        let mut state = self.state();
        if state.refused_connections > 0 {
            state.refused_connections -= 1;
            return Err(RemoteError::Connection("nobody is listening".to_string()));
        }
        if state.password != password {
            return Err(RemoteError::Auth);
        }
        let (capabilities, opened) = (state.capabilities, state.drops);
        drop(state);
        let closed = Arc::new(AtomicBool::new(false));
        Ok(FakeRemote { server: self.clone(), selected: None, looked: 0, opened, closed, capabilities })
    }
}

pub struct FakeRemote {
    server: FakeServer,
    selected: Option<String>,
    /// The selected folder's modseq when this connection last looked. A
    /// change made since, even before an idle began, ends that idle, as a
    /// real server's queued notices do.
    looked: u64,
    /// The server's count of drops when this connection was opened.
    opened: u64,
    closed: Arc<AtomicBool>,
    capabilities: Capabilities,
}

fn lost() -> RemoteError {
    RemoteError::Connection("the connection was closed".to_string())
}

impl FakeRemote {
    /// The server's state, for one command: counted against the fault
    /// script, and logged if it is answered.
    fn command(&mut self, line: String) -> Result<MutexGuard<'_, State>, RemoteError> {
        let mut state = self.server.state();
        if state.drop_after == Some(0) {
            (state.drop_after, state.drops) = (None, state.drops + 1);
        }
        if self.closed.load(Ordering::SeqCst) || state.drops != self.opened {
            return Err(lost());
        }
        state.drop_after = state.drop_after.map(|left| left - 1);
        state.log.push(line);
        // Whoever idles looks again once this command lets go of the
        // state, and so sees whatever it changed.
        self.server.0.changed.notify_all();
        Ok(state)
    }

    fn in_selected(&mut self, line: String) -> Result<(MutexGuard<'_, State>, String), RemoteError> {
        let none = RemoteError::Protocol("no folder is selected".to_string());
        let name = self.selected.clone().ok_or(none)?;
        Ok((self.command(line)?, name))
    }
}

impl Remote for FakeRemote {
    fn folders(&mut self) -> Result<Vec<RemoteFolder>, RemoteError> {
        let special_use = self.capabilities.special_use;
        let state = self.command("folders".to_string())?;
        let delimiter = state.namespace.chars().last().or(Some('/'));
        let folder = |f: &Folder| RemoteFolder {
            name: f.name.clone(),
            delimiter,
            attributes: if special_use { f.attributes.clone() } else { Vec::new() },
        };
        Ok(state.folders.iter().map(folder).collect())
    }

    fn select(&mut self, folder: &str) -> Result<Selected, RemoteError> {
        let condstore = self.capabilities.condstore;
        let mut state = self.command(format!("select {folder}"))?;
        let found = state.folder(folder)?;
        let selected = Selected {
            uid_validity: found.uid_validity,
            uid_next: found.uid_next,
            highest_modseq: condstore.then_some(found.modseq),
        };
        let looked = found.modseq;
        drop(state);
        (self.selected, self.looked) = (Some(folder.to_string()), looked);
        Ok(selected)
    }

    fn envelopes(&mut self, uids: UidRange) -> Result<Vec<Fetched>, RemoteError> {
        let last = uids.last.unwrap_or(u32::MAX);
        let (mut state, name) = self.in_selected(format!("envelopes {}:{last}", uids.first))?;
        let fetched = |m: &Message| {
            let end = m.raw.windows(4).position(|w| w == b"\r\n\r\n").map_or(m.raw.len(), |at| at + 4);
            let size = u32::try_from(m.raw.len()).unwrap_or(u32::MAX);
            Fetched { uid: m.uid, headers: m.raw[..end].to_vec(), flags: m.flags, size, arrived: m.arrived }
        };
        Ok(state.folder(&name)?.messages.iter().filter(|m| (uids.first..=last).contains(&m.uid)).map(fetched).collect())
    }

    fn flags(&mut self, since: Option<u64>) -> Result<Vec<FlagChange>, RemoteError> {
        if since.is_some() && !self.capabilities.condstore {
            return Err(RemoteError::Refused("this server has no CONDSTORE".to_string()));
        }
        let line = since.map_or("flags".to_string(), |modseq| format!("flags since {modseq}"));
        let (mut state, name) = self.in_selected(line)?;
        let changed = state.folder(&name)?.messages.iter().filter(|m| since.is_none_or(|modseq| m.modseq > modseq));
        Ok(changed.map(|m| FlagChange { uid: m.uid, flags: m.flags }).collect())
    }

    fn uids(&mut self) -> Result<Vec<u32>, RemoteError> {
        let (mut state, name) = self.in_selected("uids".to_string())?;
        Ok(state.folder(&name)?.messages.iter().filter(|m| !m.deleted).map(|m| m.uid).collect())
    }

    fn body(&mut self, uid: u32) -> Result<Vec<u8>, RemoteError> {
        let (mut state, name) = self.in_selected(format!("body {uid}"))?;
        Ok(state.folder(&name)?.message(uid)?.raw.clone())
    }

    fn idle(&mut self, until: Deadline) -> Result<Woken, RemoteError> {
        if !self.capabilities.idle {
            return Err(RemoteError::Refused("this server has no IDLE".to_string()));
        }
        let (closed, wake, looked, opened) = (self.closed.clone(), self.server.0.clone(), self.looked, self.opened);
        let (mut state, name) = self.in_selected("idle".to_string())?;
        let now = loop {
            if closed.load(Ordering::SeqCst) || state.drops != opened {
                return Err(lost());
            }
            let now = state.folder(&name)?.modseq;
            let left = until.0.saturating_duration_since(Instant::now());
            if now != looked || left.is_zero() {
                break now;
            }
            state = wake.changed.wait_timeout(state, left).unwrap_or_else(PoisonError::into_inner).0;
        };
        drop(state);
        self.looked = now;
        Ok(if now == looked { Woken::TimedOut } else { Woken::Changed })
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn interrupt(&self) -> Interrupt {
        let (closed, server) = (self.closed.clone(), self.server.clone());
        Interrupt::new(move || {
            closed.store(true, Ordering::SeqCst);
            // Taken so that an idle between its check and its wait cannot
            // miss the wake.
            let _held = server.state();
            server.0.changed.notify_all();
        })
    }
}

impl RemoteWrite for FakeRemote {
    fn store_flags(&mut self, uids: &[u32], set: Flags, clear: Flags) -> Result<(), RemoteError> {
        let (mut state, name) = self.in_selected(format!("store {uids:?}"))?;
        for message in state.folder(&name)?.named(uids) {
            let f = &mut message.flags;
            f.seen = (f.seen || set.seen) && !clear.seen;
            f.flagged = (f.flagged || set.flagged) && !clear.flagged;
            f.answered = (f.answered || set.answered) && !clear.answered;
            f.draft = (f.draft || set.draft) && !clear.draft;
        }
        Ok(())
    }

    /// With MOVE the messages go at once. Without it they are copied and
    /// marked deleted, and purged by name only where UIDPLUS allows it.
    fn move_to(&mut self, uids: &[u32], target: &str) -> Result<MoveOutcome, RemoteError> {
        let (can_move, uidplus) = (self.capabilities.r#move, self.capabilities.uidplus);
        let verb = if can_move { "move" } else { "copy" };
        let (mut state, name) = self.in_selected(format!("{verb} {uids:?} {target}"))?;
        if state.refuse_moves {
            return Err(RemoteError::Refused("the move was refused".to_string()));
        }
        let moving: Vec<Message> = state.folder(&name)?.named(uids).into_iter().map(|m| m.clone()).collect();
        let to = state.folder(target)?;
        moving.iter().for_each(|m| _ = to.add(&m.raw, m.flags, m.arrived));
        if can_move {
            state.folder(&name)?.messages.retain(|m| !uids.contains(&m.uid));
            return Ok(MoveOutcome::Moved);
        }
        drop(state);
        self.in_selected(format!("store-deleted {uids:?}"))?
            .0
            .folder(&name)?
            .named(uids)
            .into_iter()
            .for_each(|m| m.deleted = true);
        if !uidplus {
            return Ok(MoveOutcome::CopiedAndFlagged);
        }
        let mut state = self.in_selected(format!("uid-expunge {uids:?}"))?.0;
        state.folder(&name)?.messages.retain(|m| !(m.deleted && uids.contains(&m.uid)));
        Ok(MoveOutcome::Moved)
    }

    fn delete(&mut self, uids: &[u32]) -> Result<bool, RemoteError> {
        let uidplus = self.capabilities.uidplus;
        let (mut state, name) = self.in_selected(format!("store-deleted {uids:?}"))?;
        if state.refuse_moves {
            return Err(RemoteError::Refused("the delete was refused".to_string()));
        }
        state.folder(&name)?.named(uids).into_iter().for_each(|m| m.deleted = true);
        drop(state);
        if uidplus {
            let mut state = self.in_selected(format!("uid-expunge {uids:?}"))?.0;
            state.folder(&name)?.messages.retain(|m| !(m.deleted && uids.contains(&m.uid)));
        }
        Ok(uidplus)
    }

    fn append(&mut self, folder: &str, message: &[u8], flags: Flags) -> Result<(), RemoteError> {
        let mut state = self.command(format!("append {folder}"))?;
        state.folder(folder)?.add(message, flags, Timestamp(0));
        Ok(())
    }

    fn create(&mut self, folder: &str) -> Result<(), RemoteError> {
        let mut state = self.command(format!("create {folder}"))?;
        if !folder.starts_with(&state.namespace) {
            return Err(RemoteError::Refused(format!("{folder} is outside the namespace {}", state.namespace)));
        }
        if state.folder(folder).is_err() {
            // Numbered as no folder a test makes by hand is.
            let uid_validity = 1000 + state.folders.len() as u32;
            let (name, attributes) = (folder.to_string(), Vec::new());
            state.folders.push(Folder { name, attributes, uid_validity, uid_next: 1, modseq: 1, messages: vec![] });
        }
        Ok(())
    }

    fn find(&mut self, message_id: &str) -> Result<Vec<u32>, RemoteError> {
        let (mut state, name) = self.in_selected(format!("find {message_id}"))?;
        let wanted = format!("<{message_id}>");
        let has = |m: &&Message| {
            let head = String::from_utf8_lossy(&m.raw);
            let head = head.split("\r\n\r\n").next().unwrap_or_default().to_string();
            head.lines().any(|l| l.to_ascii_lowercase().starts_with("message-id:") && l.contains(&wanted))
        };
        Ok(state.folder(&name)?.messages.iter().filter(|m| !m.deleted).filter(has).map(|m| m.uid).collect())
    }
}

#[cfg(test)]
mod tests;

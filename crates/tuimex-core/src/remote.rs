//! The mail server as the engine sees it (PLAN §4). Declared here so that
//! `tuimex-fixtures` can implement it without depending on the engine.

use std::sync::Arc;
use std::time::Instant;

use crate::{Flags, Timestamp};

/// One signed-in connection. Every call blocks (ADR-006); `interrupt` is
/// how another thread ends one.
pub trait Remote: Send {
    fn folders(&mut self) -> Result<Vec<RemoteFolder>, RemoteError>;
    fn select(&mut self, folder: &str) -> Result<Selected, RemoteError>;
    /// Headers, flags, size and arrival time of the messages in `uids`.
    /// Fetching must not mark a message seen.
    fn envelopes(&mut self, uids: UidRange) -> Result<Vec<Fetched>, RemoteError>;
    /// The flags of every message whose flags changed after `modseq`, on a
    /// server with CONDSTORE; with `None`, of every message.
    fn flags(&mut self, since: Option<u64>) -> Result<Vec<FlagChange>, RemoteError>;
    /// Every UID in the selected folder, ascending, to find what is new
    /// and what was removed elsewhere. A message marked `\Deleted` is one
    /// that was removed: it waits only for its server to purge it.
    fn uids(&mut self) -> Result<Vec<u32>, RemoteError>;
    /// The whole message. Fetching must not mark it seen.
    fn body(&mut self, uid: u32) -> Result<Vec<u8>, RemoteError>;
    /// Waits in the selected folder until something changes there or
    /// `until` passes.
    fn idle(&mut self, until: Deadline) -> Result<Woken, RemoteError>;
    fn capabilities(&self) -> &Capabilities;
    /// Ends this connection from another thread: the call it is blocked
    /// in, and every later one, fails with `Connection`.
    fn interrupt(&self) -> Interrupt;
}

/// What changes the server. Apart from `Remote`, so that a connection
/// which only reads can exist before one that writes does.
pub trait RemoteWrite: Remote {
    fn store_flags(&mut self, uids: &[u32], set: Flags, clear: Flags) -> Result<(), RemoteError>;
    /// Never a bare `EXPUNGE` (ADR-014). A UID the folder no longer has
    /// is passed over, as a server passes over it.
    fn move_to(&mut self, uids: &[u32], target: &str) -> Result<MoveOutcome, RemoteError>;
    /// Removes messages of the selected folder for good, by name: they
    /// are marked `\\Deleted` and purged with `UID EXPUNGE`, never a bare
    /// `EXPUNGE` (ADR-014). False when the server has no UIDPLUS, and is
    /// left to purge what was marked by itself.
    fn delete(&mut self, uids: &[u32]) -> Result<bool, RemoteError>;
    fn append(&mut self, folder: &str, message: &[u8], flags: Flags) -> Result<(), RemoteError>;
    /// The UIDs, ascending, of the selected folder's messages whose
    /// `Message-ID` is `message_id`. A move or an append numbers a message
    /// anew, and this is how it is found again on any server (ADR-034).
    fn find(&mut self, message_id: &str) -> Result<Vec<u32>, RemoteError>;
    /// Makes a folder. One that is there already is the folder asked for.
    fn create(&mut self, folder: &str) -> Result<(), RemoteError>;
}

#[derive(Clone)]
pub struct Interrupt(Arc<dyn Fn() + Send + Sync>);

impl Interrupt {
    pub fn new(close: impl Fn() + Send + Sync + 'static) -> Interrupt {
        Interrupt(Arc::new(close))
    }

    pub fn fire(&self) {
        (self.0)();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFolder {
    pub name: String,
    pub delimiter: Option<char>,
    /// SPECIAL-USE attributes as the server sent them, such as `\Trash`.
    pub attributes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selected {
    pub uid_validity: u32,
    pub uid_next: u32,
    pub highest_modseq: Option<u64>,
}

/// Inclusive; `last: None` reaches the newest message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UidRange {
    pub first: u32,
    pub last: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub uid: u32,
    /// The header block as bytes: only `tuimex-content` parses it (ADR-007).
    pub headers: Vec<u8>,
    pub flags: Flags,
    /// The whole message's size, which the headers are a small part of.
    pub size: u32,
    /// When the server received it, which its sender cannot choose.
    pub arrived: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagChange {
    pub uid: u32,
    pub flags: Flags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveOutcome {
    /// Gone from the source folder.
    Moved,
    /// A server without UIDPLUS or MOVE: copied and flagged `\Deleted`, and
    /// the server purges the original when it chooses to.
    CopiedAndFlagged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline(pub Instant);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Woken {
    Changed,
    TimedOut,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub uidplus: bool,
    pub r#move: bool,
    pub condstore: bool,
    pub special_use: bool,
    pub idle: bool,
    pub gmail: bool,
}

/// The kinds differ in what follows: a lost connection retries with
/// backoff, a failed sign-in never does (PLAN §4.2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteError {
    /// Carries none of the server's words: they answer a command that
    /// held the password (ADR-010).
    #[error("signing in failed")]
    Auth,
    #[error("connection lost: {0}")]
    Connection(String),
    #[error("the server refused: {0}")]
    Refused(String),
    #[error("unexpected answer from the server: {0}")]
    Protocol(String),
}

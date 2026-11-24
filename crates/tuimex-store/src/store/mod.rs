//! The Maildir, the index and the journals as one thing, so that the three
//! stay in step: every change goes to the Maildir first and the index
//! follows, and a change made here is in the journal before either.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

use tuimex_core::{
    AccountId, Address, Attachment, Envelope, Flags, Folder, Listed, MessageId, Query, Role, Security, Sort, Timestamp,
};

use crate::index::{Index, Position, Synced};
use crate::journal::{Change, Entry, Journal};
use crate::maildir::{Delivery, Maildir, Stored};
use crate::{At, Error, Result, files};

/// What a message's bytes say, as far as the index keeps it. Only
/// `tuimex-content` may work this out (ADR-007), and this crate cannot
/// depend on it, so whoever opens the store hands the parse in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub date: Timestamp,
    pub from: Vec<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub reply_to: Vec<Address>,
    pub subject: String,
    pub message_id: Option<String>,
    pub references: Vec<String>,
    pub has_attachments: bool,
    pub has_invite: bool,
    pub has_unsubscribe: bool,
    pub security: Security,
    /// The reader's text, for search.
    pub text: String,
    /// The files a list row counts: every attachment but an invitation's
    /// calendar part.
    pub attachments: Vec<Attachment>,
}

type Parse = dyn Fn(&[u8]) -> Parsed + Send;

/// What a rebuild found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rebuilt {
    pub messages: usize,
    /// Files whose key another file in the account already had. The first
    /// in folder order is indexed; these are left on disk and out of the
    /// index, for the caller to report.
    pub duplicates: Vec<PathBuf>,
}

/// The file in the data directory that says the mail is in use. What
/// says so is a lock the system holds on it for a running process and
/// drops when the process ends, however it ends: the file itself stays,
/// and one left by a crash means nothing.
const LOCK: &str = "tuimex.lock";

fn lock_file(data: &Path) -> Result<File> {
    files::private_dir(data).at(data)?;
    let path = data.join(LOCK);
    files::append(&path).at(&path)
}

/// The mail directory held by one process alone, for as long as this
/// lives: what moves an account's directory needs nothing else to have
/// the store open.
#[derive(Debug)]
pub struct Sole(#[expect(dead_code, reason = "held for the lock it carries, which goes when it is dropped")] File);

/// Takes the mail directory for this process alone, or says that
/// something has it open: an interface, a `tuimex sync`, an agent's call.
pub fn sole(data: &Path) -> Result<Sole> {
    let file = lock_file(data)?;
    match file.try_lock() {
        Ok(()) => Ok(Sole(file)),
        Err(TryLockError::WouldBlock) => Err(Error::InUse(data.to_path_buf())),
        Err(TryLockError::Error(e)) => Err(e).at(data),
    }
}

pub struct Store {
    maildir: Maildir,
    index: Index,
    parse: Box<Parse>,
    /// Shared with every other open store, and what `sole` waits out.
    _lock: File,
}

/// How many messages a rebuild indexes between saying how far it is.
const SAID_EVERY: usize = 500;

impl Store {
    /// `data` holds the mail and `cache` the index. An index that is
    /// missing, unreadable or from another version is rebuilt before this
    /// returns (ADR-013).
    pub fn open(data: &Path, cache: &Path, parse: impl Fn(&[u8]) -> Parsed + Send + 'static) -> Result<Store> {
        Store::open_saying(data, cache, parse, &mut |_, _| {})
    }

    /// As `open`, for whoever has someone waiting: a rebuild tells `said`
    /// how many messages it has indexed of how many, as it goes.
    pub fn open_saying(
        data: &Path,
        cache: &Path,
        parse: impl Fn(&[u8]) -> Parsed + Send + 'static,
        said: &mut dyn FnMut(usize, usize),
    ) -> Result<Store> {
        let lock = lock_file(data)?;
        match lock.try_lock_shared() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(Error::InUse(data.to_path_buf())),
            Err(TryLockError::Error(e)) => return Err(e).at(data),
        }
        files::private_dir(cache).at(cache)?;
        let index = Index::open(&cache.join("index.sqlite"))?;
        let store = Store { maildir: Maildir::new(data), index, parse: Box::new(parse), _lock: lock };
        if !store.index.is_current()? {
            store.rebuild_saying(said)?;
        }
        Ok(store)
    }

    /// Throws the index away and reads it again from the Maildir.
    pub fn rebuild(&self) -> Result<Rebuilt> {
        self.rebuild_saying(&mut |_, _| {})
    }

    fn rebuild_saying(&self, said: &mut dyn FnMut(usize, usize)) -> Result<Rebuilt> {
        let Store { maildir, index, parse, .. } = self;
        index.rebuild(|| {
            let mut rebuilt = Rebuilt::default();
            // The names are listed first, which is quick beside reading
            // the messages, so that what is said has a total.
            let mut folders = Vec::new();
            for account in maildir.accounts()? {
                for folder in maildir.folders(&account)? {
                    let stored = maildir.list(&account, &folder)?;
                    folders.push((account.clone(), folder, stored));
                }
            }
            let total = folders.iter().map(|(.., stored)| stored.len()).sum();
            let mut read = 0;
            for (account, folder, listed) in folders {
                let (role, uid_validity) = (maildir.role(&account, &folder)?, maildir.uid_validity(&account, &folder)?);
                index.put_folder(&account, &folder, role, uid_validity)?;
                for stored in listed {
                    if read % SAID_EVERY == 0 {
                        said(read, total);
                    }
                    read += 1;
                    let id = MessageId { account: AccountId(account.clone()), key: stored.key.clone() };
                    if index.locate(&id)?.is_some() {
                        rebuilt.duplicates.push(stored.path());
                        continue;
                    }
                    let raw = maildir.read(&stored)?;
                    index.insert(&account, &folder, &stored, size(&raw), &parse(&raw))?;
                    rebuilt.messages += 1;
                }
            }
            Ok(rebuilt)
        })
    }

    /// Makes a folder if it is not there, and records its role.
    pub fn ensure_folder(&self, account: &str, folder: &str, role: Role) -> Result<()> {
        self.maildir.ensure_folder(account, folder, role)?;
        self.index.put_folder(account, folder, role, self.maildir.uid_validity(account, folder)?)
    }

    /// Records the folder's UIDVALIDITY where a rebuild will find it.
    /// Deciding what a changed one means for the mail is the caller's.
    pub fn set_uid_validity(&self, account: &str, folder: &str, uid_validity: u32) -> Result<()> {
        self.maildir.set_uid_validity(account, folder, uid_validity)?;
        self.index.put_folder(account, folder, self.maildir.role(account, folder)?, Some(uid_validity))
    }

    /// What the account's server begins a person's folder names with
    /// (ADR-048), as its last listing recorded it.
    pub fn namespace(&self, account: &str) -> Result<String> {
        self.maildir.namespace(account)
    }

    pub fn set_namespace(&self, account: &str, prefix: &str) -> Result<()> {
        self.maildir.set_namespace(account, prefix)
    }

    pub fn set_modseq(&self, account: &str, folder: &str, modseq: u64) -> Result<()> {
        self.index.set_modseq(account, folder, modseq)
    }

    pub fn position(&self, account: &str, folder: &str) -> Result<Position> {
        self.index.position(account, folder)
    }

    pub fn folders(&self) -> Result<Vec<Folder>> {
        self.index.folders()
    }

    pub fn query(&self, query: &Query, now: Timestamp, sort: Sort, limit: Option<usize>) -> Result<Vec<Envelope>> {
        self.index.query(query, now, sort, limit)
    }

    /// Every message as a list row shows it, or with `only` the ones it
    /// names that are still here.
    pub fn listed(&self, only: Option<&[MessageId]>) -> Result<Vec<Listed>> {
        self.index.listed(only)
    }

    /// None when the account has no message with that key.
    pub fn envelope(&self, id: &MessageId) -> Result<Option<Envelope>> {
        self.index.envelope(id)
    }

    /// The account's messages with that `Message-ID`: more than one when
    /// a copy is in another folder.
    pub fn named(&self, account: &str, message_id: &str) -> Result<Vec<MessageId>> {
        self.index.named(account, message_id)
    }

    /// Files a new message and indexes it.
    pub fn deliver(&self, account: &str, folder: &str, delivery: &Delivery, raw: &[u8]) -> Result<MessageId> {
        let id = MessageId { account: AccountId(account.to_string()), key: delivery.key.clone() };
        if self.index.locate(&id)?.is_some() {
            return Err(Error::Exists { account: account.to_string(), key: id.key });
        }
        let stored = self.maildir.deliver(account, folder, delivery, raw)?;
        self.index.insert(account, folder, &stored, size(raw), &(self.parse)(raw))?;
        Ok(id)
    }

    /// A header block for a message whose body is still on the server
    /// (ADR-028), indexed by what its headers say.
    pub fn deliver_head(&self, account: &str, folder: &str, delivery: &Delivery, head: &[u8]) -> Result<MessageId> {
        let id = MessageId { account: AccountId(account.to_string()), key: delivery.key.clone() };
        if self.index.locate(&id)?.is_some() {
            return Err(Error::Exists { account: account.to_string(), key: id.key });
        }
        let stored = self.maildir.deliver_head(account, folder, delivery, head)?;
        self.index.insert(account, folder, &stored, size(head), &(self.parse)(head))?;
        Ok(id)
    }

    /// Gives a header block the whole message the server sent for it, and
    /// indexes what the body adds.
    pub fn fill(&self, id: &MessageId, raw: &[u8]) -> Result<()> {
        let (folder, stored) = self.stored(id)?;
        let stored = self.maildir.fill(&stored, raw)?;
        self.index.remove(id)?;
        self.index.insert(&id.account.0, &folder, &stored, size(raw), &(self.parse)(raw))
    }

    /// Every message in a folder that the server has given a UID.
    pub fn synced(&self, account: &str, folder: &str) -> Result<Vec<Synced>> {
        self.index.synced(account, folder)
    }

    /// The folder a message is in and its file.
    pub fn stored(&self, id: &MessageId) -> Result<(String, Stored)> {
        let missing = || Error::NoMessage { account: id.account.0.clone(), key: id.key.clone() };
        let at = self.index.locate(id)?.ok_or_else(missing)?;
        let stored = self.maildir.find(&id.account.0, &at.folder, &id.key, Some(&at.file))?.ok_or_else(missing)?;
        Ok((at.folder, stored))
    }

    /// The message's bytes, for `tuimex-content` to read.
    pub fn read(&self, id: &MessageId) -> Result<Vec<u8>> {
        self.maildir.read(&self.stored(id)?.1)
    }

    /// The Maildir's half of a change: the folder the message is in
    /// afterwards, and its file.
    fn rename(&self, id: &MessageId, folder: String, stored: &Stored, change: &Change) -> Result<(String, Stored)> {
        Ok(match change {
            Change::Flags { set, clear } => {
                (folder, self.maildir.set_flags(stored, both(either(stored.flags, *set), not(*clear)))?)
            }
            Change::Move { to } => (to.clone(), self.maildir.move_to(stored, &id.account.0, to)?),
            // `apply` and `change` remove the file before they get here.
            Change::Delete => (folder, stored.clone()),
        })
    }

    /// Makes a change the server already has, or one being replayed or
    /// undone: nothing is written to the journal.
    pub fn apply(&self, id: &MessageId, change: &Change) -> Result<()> {
        if *change == Change::Delete {
            return self.remove(id);
        }
        let (folder, stored) = self.stored(id)?;
        let (folder, stored) = self.rename(id, folder, &stored, change)?;
        self.index.refile(id, &folder, &stored)
    }

    /// Makes a change the server has not heard of. It is in the journal
    /// before anything else happens (ADR-014). The entry keeps only the
    /// flags that really changed, so swapping `set` and `clear` undoes it;
    /// a change that changes nothing has no entry.
    pub fn change(&self, id: &MessageId, change: Change) -> Result<Option<Entry>> {
        let (folder, stored) = self.stored(id)?;
        let change = match change {
            // A flag named in both ends cleared, as `apply` leaves it.
            Change::Flags { set, clear } => {
                Change::Flags { set: both(both(set, not(clear)), not(stored.flags)), clear: both(clear, stored.flags) }
            }
            to => to,
        };
        let nothing = Flags::default();
        if change == (Change::Flags { set: nothing, clear: nothing }) || change == (Change::Move { to: folder.clone() })
        {
            return Ok(None);
        }
        let journal = self.journal(&id.account.0)?;
        if change == Change::Delete {
            // Mail the server never numbered is nothing it can be told to
            // remove.
            let entry = stored.uid.map(|uid| journal.record(&id.key, &folder, Some(uid), change)).transpose()?;
            self.maildir.remove(&stored)?;
            self.index.remove(id)?;
            return Ok(entry);
        }
        let entry = journal.record(&id.key, &folder, stored.uid, change)?;
        let (folder, stored) = match self.rename(id, folder, &stored, &entry.change) {
            Ok(renamed) => renamed,
            Err(e) => {
                // The Maildir is as it was, so the server has nothing to
                // hear and the entry goes. The rename's error is the one to
                // report; an entry that cannot be taken back is replayed,
                // which makes a change that was asked for.
                let _ = journal.confirm(entry.seq);
                return Err(e);
            }
        };
        self.index.refile(id, &folder, &stored)?;
        Ok(Some(entry))
    }

    fn journal(&self, account: &str) -> Result<Journal> {
        Ok(Journal::new(&self.maildir.account_dir(account)?))
    }

    /// The changes an account's server has yet to confirm, oldest first.
    pub fn pending(&self, account: &str) -> Result<Vec<Entry>> {
        self.journal(account)?.pending()
    }

    /// Takes a change out of the journal once the server has answered.
    pub fn confirm(&self, account: &str, seq: u64) -> Result<()> {
        self.journal(account)?.confirm(seq)
    }

    /// Records the UID the server gave a message that was filed here first.
    pub fn assign_uid(&self, id: &MessageId, uid: u32) -> Result<()> {
        let (folder, stored) = self.stored(id)?;
        self.index.refile(id, &folder, &self.maildir.set_uid(&stored, uid)?)
    }

    /// Deletes a message here. Nothing is said to the server.
    pub fn remove(&self, id: &MessageId) -> Result<()> {
        let (_, stored) = self.stored(id)?;
        self.maildir.remove(&stored)?;
        self.index.remove(id)
    }
}

/// A message's size as the index holds it. One too large to count is
/// larger than any size a query can name.
fn size(raw: &[u8]) -> u32 {
    u32::try_from(raw.len()).unwrap_or(u32::MAX)
}

fn each(a: Flags, b: Flags, op: impl Fn(bool, bool) -> bool) -> Flags {
    Flags {
        seen: op(a.seen, b.seen),
        flagged: op(a.flagged, b.flagged),
        answered: op(a.answered, b.answered),
        draft: op(a.draft, b.draft),
    }
}

fn both(a: Flags, b: Flags) -> Flags {
    each(a, b, |a, b| a && b)
}

fn either(a: Flags, b: Flags) -> Flags {
    each(a, b, |a, b| a || b)
}

fn not(a: Flags) -> Flags {
    each(a, a, |a, _| !a)
}

#[cfg(test)]
mod tests;

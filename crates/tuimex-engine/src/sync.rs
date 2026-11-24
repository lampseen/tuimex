//! One sync pass (PLAN §4.3), and the rules of ADR-014 that reading a
//! server involves. The server is asked through `Remote`; everything kept
//! goes through the store's thread, in a few steps that each leave the
//! Maildir consistent, so a pass cut short anywhere is resumed by the next.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

use tuimex_core::{
    AccountId, Event, FlagChange, Flags, FolderId, MessageId, Origin, Remote, RemoteWrite, Role, Timestamp, UidRange,
};
use tuimex_store::journal::{Change, Entry};
use tuimex_store::maildir::{Delivery, new_key};
use tuimex_store::{Error as StoreError, Result as Stored, Store};

use crate::{Account, Connect, Error, Report, StoreHandle, replay, roles};

/// How many messages are fetched before any is filed. A pass that is cut
/// short loses at most this many fetches: progress is what is in the
/// Maildir.
const BATCH: usize = 100;

/// The `[sync]` table of the config, as numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Messages that arrived within this many seconds get their bodies.
    pub bodies: i64,
    /// A message larger than this many bytes arrives when it is opened.
    pub max_body: u32,
    /// Folders to sync besides every folder with a role.
    pub folders: Vec<String>,
}

impl Default for Policy {
    fn default() -> Policy {
        Policy { bodies: 30 * 86_400, max_body: 10_000_000, folders: Vec::new() }
    }
}

/// What one pass over a folder did here.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pass {
    /// Every message that arrived, changed or went.
    pub changed: Vec<MessageId>,
    /// Unread messages new to a folder that had been synced before: what
    /// is worth a notification, which a first sync's thousands are not.
    pub arrived: u32,
}

/// What the Maildir holds of a folder as a pass begins.
struct Local {
    /// UIDs not to fetch: those here, and those moved away or deleted
    /// here by a change the server has not heard of yet.
    known: BTreeSet<u32>,
    modseq: Option<u64>,
    /// False for a first sync, and for one that starts over.
    resumed: bool,
    dropped: Vec<MessageId>,
}

struct Arrival {
    uid: u32,
    flags: Flags,
    /// The whole message, or its header block.
    bytes: Vec<u8>,
    headers_only: bool,
}

fn id(account: &str, key: &str) -> MessageId {
    MessageId { account: AccountId(account.to_string()), key: key.to_string() }
}

/// Removes a message that may already be gone: a file another tool
/// deleted leaves nothing to remove, which is what was wanted.
fn remove(store: &Store, id: &MessageId) -> Stored<()> {
    match store.remove(id) {
        Err(StoreError::NoMessage { .. }) => Ok(()),
        other => other,
    }
}

/// Reads what is here, first throwing it away if the folder's UIDVALIDITY
/// is not the one its UIDs were issued under (ADR-014). A change still in
/// the journal names a UID that now means nothing, so it goes too, and a
/// message it had moved away comes back with the rest when the folder is
/// fetched again. Mail the server never numbered is left alone.
fn survey(store: &Store, account: &str, folder: &str, uid_validity: u32) -> Stored<Local> {
    let resumed = store.position(account, folder)?.uid_validity == Some(uid_validity);
    let mut dropped = Vec::new();
    let pending = |store: &Store| -> Stored<Vec<Entry>> {
        Ok(store.pending(account)?.into_iter().filter(|e| e.folder == folder && e.uid.is_some()).collect())
    };
    if !resumed {
        for entry in pending(store)? {
            store.confirm(account, entry.seq)?;
            if matches!(entry.change, Change::Move { .. }) {
                remove(store, &id(account, &entry.key))?;
                dropped.push(id(account, &entry.key));
            }
        }
        for message in store.synced(account, folder)? {
            remove(store, &id(account, &message.key))?;
            dropped.push(id(account, &message.key));
        }
        store.set_uid_validity(account, folder, uid_validity)?;
    }
    let gone_here =
        pending(store)?.into_iter().filter(|e| !matches!(e.change, Change::Flags { .. })).filter_map(|e| e.uid);
    let known = store.synced(account, folder)?.into_iter().map(|m| m.uid).chain(gone_here).collect();
    Ok(Local { known, modseq: store.position(account, folder)?.highest_modseq, resumed, dropped })
}

fn file(store: &Store, account: &str, folder: &str, arrivals: &[Arrival]) -> Stored<Vec<MessageId>> {
    let mut filed = Vec::new();
    for arrival in arrivals {
        let delivery =
            Delivery { key: new_key(), uid: Some(arrival.uid), origin: Origin::Server, flags: arrival.flags };
        filed.push(if arrival.headers_only {
            store.deliver_head(account, folder, &delivery, &arrival.bytes)?
        } else {
            store.deliver(account, folder, &delivery, &arrival.bytes)?
        });
    }
    Ok(filed)
}

fn each(a: Flags, b: Flags, op: impl Fn(bool, bool) -> bool) -> Flags {
    Flags {
        seen: op(a.seen, b.seen),
        flagged: op(a.flagged, b.flagged),
        answered: op(a.answered, b.answered),
        draft: op(a.draft, b.draft),
    }
}

/// The change that turns `from` into `to`.
fn towards(from: Flags, to: Flags) -> Change {
    Change::Flags { set: each(to, from, |to, from| to && !from), clear: each(from, to, |from, to| from && !to) }
}

/// Brings one message's flags into line with the server's, and says
/// whether they changed here. `pending` is what the journal holds for the
/// message. The rule is ADR-014's: flags changed only here wait in the
/// journal for the server; flags changed on both sides take the server's
/// value, except that `\Seen` set on either side stays set.
fn settle(store: &Store, id: &MessageId, here: Flags, there: Flags, pending: &[&Entry]) -> Stored<bool> {
    let mine: Vec<(u64, Flags, Flags)> = pending
        .iter()
        .filter_map(|e| match e.change {
            Change::Flags { set, clear } => Some((e.seq, set, clear)),
            Change::Move { .. } | Change::Delete => None,
        })
        .collect();
    // What the flags here were before the journal's changes: if the
    // server still says that, it has changed nothing.
    let agreed = mine
        .iter()
        .rev()
        .fold(here, |f, (_, set, clear)| each(each(f, *set, |f, set| f && !set), *clear, |f, clear| f || clear));
    if there == agreed {
        return Ok(false);
    }
    // Read from the journal and not from the file, so that a pass which
    // died between the steps below still knows the message was read here.
    let read_here = mine.iter().fold(false, |seen, (_, set, clear)| (seen || set.seen) && !clear.seen);
    store.apply(id, &towards(here, there))?;
    if read_here && !there.seen {
        // Journaled, because the server has yet to hear of it.
        store.change(id, Change::Flags { set: Flags { seen: true, ..Flags::default() }, clear: Flags::default() })?;
    }
    for (seq, ..) in mine {
        store.confirm(&id.account.0, seq)?;
    }
    Ok(true)
}

/// Removes what the server no longer has, settles the flags it reported,
/// and records how far its changes have been read.
fn reconcile(
    store: &Store,
    (account, folder): (&str, &str),
    server: &BTreeSet<u32>,
    changes: &BTreeMap<u32, Flags>,
    modseq: Option<u64>,
) -> Stored<Vec<MessageId>> {
    let pending: Vec<Entry> = store.pending(account)?.into_iter().filter(|e| e.folder == folder).collect();
    let mut changed = Vec::new();
    for message in store.synced(account, folder)? {
        let id = id(account, &message.key);
        let mine: Vec<&Entry> = pending.iter().filter(|e| e.key == message.key).collect();
        if !server.contains(&message.uid) {
            // Removed elsewhere: a change waiting for it has no message
            // left to be made to.
            for entry in mine {
                store.confirm(account, entry.seq)?;
            }
            remove(store, &id)?;
            changed.push(id);
        } else if let Some(there) = changes.get(&message.uid)
            && settle(store, &id, message.flags, *there, &mine)?
        {
            changed.push(id);
        }
    }
    if let Some(modseq) = modseq {
        store.set_modseq(account, folder, modseq)?;
    }
    Ok(changed)
}

/// One pass over a folder the store already has: what is new arrives,
/// what was removed elsewhere goes, flags follow the server. `now` decides
/// which messages are recent enough to bring their bodies. While a folder
/// is filled for the first time `progress` hears, after each batch, how
/// many of the server's messages are here.
pub fn folder(
    remote: &mut dyn Remote,
    store: &StoreHandle,
    (account, folder): (&str, &str),
    policy: &Policy,
    now: Timestamp,
    progress: &dyn Fn(u32, u32),
) -> Result<Pass, Error> {
    let at = || (account.to_string(), folder.to_string());
    let selected = remote.select(folder)?;
    let (a, f) = at();
    let local = store.run(move |s| survey(s, &a, &f, selected.uid_validity))?;
    let server: BTreeSet<u32> = remote.uids()?.into_iter().collect();
    let mut pass = Pass { changed: local.dropped, arrived: 0 };

    // Whatever the server has and the Maildir does not, lowest first: on
    // a first sync that is everything, and after an interrupted one it is
    // what the last pass did not reach.
    let wanted: Vec<u32> = server.difference(&local.known).copied().collect();
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let mut here = server.len() - wanted.len();
    for batch in wanted.chunks(BATCH) {
        let (Some(&first), Some(&last)) = (batch.first(), batch.last()) else { continue };
        let mut arrivals = Vec::new();
        // The range may span messages already here; they are not wanted.
        for fetched in remote.envelopes(UidRange { first, last: Some(last) })? {
            if !batch.contains(&fetched.uid) {
                continue;
            }
            let whole = fetched.size <= policy.max_body && fetched.arrived.0 >= now.0 - policy.bodies;
            let bytes = if whole { remote.body(fetched.uid)? } else { fetched.headers };
            arrivals.push(Arrival { uid: fetched.uid, flags: fetched.flags, bytes, headers_only: !whole });
        }
        if local.resumed {
            pass.arrived += arrivals.iter().filter(|a| !a.flags.seen).count() as u32;
        }
        let (a, f) = at();
        pass.changed.extend(store.run(move |s| file(s, &a, &f, &arrivals))?);
        here += batch.len();
        if !local.resumed {
            progress(count(here), count(server.len()));
        }
    }

    // With CONDSTORE and a modseq to count from, only what changed since;
    // nothing at all when the folder's modseq has not moved. Otherwise
    // every flag.
    let since = local.modseq.filter(|_| remote.capabilities().condstore);
    let changes = if since.is_some() && since == selected.highest_modseq { Vec::new() } else { remote.flags(since)? };
    let changes: BTreeMap<u32, Flags> = changes.into_iter().map(|FlagChange { uid, flags }| (uid, flags)).collect();
    let (a, f) = at();
    pass.changed.extend(store.run(move |s| reconcile(s, (&a, &f), &server, &changes, selected.highest_modseq))?);
    Ok(pass)
}

/// A folder's pass, then what the journal holds for it (PLAN §4.3),
/// announced as soon as it is done.
pub(crate) fn pass(
    remote: &mut dyn RemoteWrite,
    store: &StoreHandle,
    account: &Account,
    (name, role): (&str, Role),
    now: Timestamp,
    tell: &dyn Fn(Report),
) -> Result<(), Error> {
    let id = AccountId(account.name.clone());
    let progress = |done, total| {
        let (account, folder) = (id.clone(), FolderId(name.to_string()));
        tell(Report::Event(Event::FirstSync { account, folder, done, total }));
    };
    let mut pass = folder(remote, store, (&account.name, name), &account.policy, now, &progress)?;
    pass.changed.extend(replay::folder(remote, store, (&account.name, name), tell)?);
    let synced = Event::Synced { account: id.clone(), folder: FolderId(name.to_string()), changed: pass.changed };
    tell(Report::Event(synced));
    if role == Role::Inbox && pass.arrived > 0 {
        tell(Report::Event(Event::NewMail { account: id, count: pass.arrived }));
    }
    Ok(())
}

/// One pass over an account: every folder with a role, and those the
/// config names. A failure in a later folder does not hide what an
/// earlier one brought.
pub(crate) fn account(
    remote: &mut dyn RemoteWrite,
    store: &StoreHandle,
    account: &Account,
    now: Timestamp,
    tell: &dyn Fn(Report),
) -> Result<(), Error> {
    replay::unsynced(remote, store, &account.name, tell)?;
    let listed = remote.folders()?;
    let (a, prefix) = (account.name.clone(), roles::namespace(&listed));
    store.run(move |s| s.set_namespace(&a, &prefix))?;
    // Every folder is known here before the first is filled. A first sync
    // shows the Inbox long before it reaches the Archive, and a message
    // archived or trashed meanwhile needs the folder to be moved to: the
    // move then waits in the journal for the server (ADR-014).
    let mut known = Vec::new();
    for (name, role) in roles::assign(&listed, &account.roles, remote.capabilities().gmail) {
        if role == Role::Other && !account.policy.folders.contains(&name) {
            continue;
        }
        let (a, f) = (account.name.clone(), name.clone());
        match store.run(move |s| s.ensure_folder(&a, &f, role)) {
            // A name the Maildir cannot hold, such as one with an empty
            // level: the folder is left out and the rest still sync.
            Err(Error::Store(StoreError::Name(_))) => continue,
            made => made?,
        }
        known.push((name, role));
    }
    for (name, role) in &known {
        pass(remote, store, account, (name, *role), now, tell)?;
    }
    Ok(())
}

pub(crate) fn now() -> Timestamp {
    // A clock set before 1970 reads as 1970: every message is then old,
    // and none brings its body until it is opened.
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    Timestamp(i64::try_from(since.as_secs()).unwrap_or(i64::MAX))
}

/// One pass over an account on a connection of its own: what `tuimex
/// sync` runs. Nothing was asked of it, so a change the server refuses is
/// a `Notice`.
pub fn once(connect: &dyn Connect, store: &StoreHandle, account_: &Account, emit: &dyn Fn(Event)) -> Result<(), Error> {
    let tell = |report| match report {
        Report::Event(event) => emit(event),
        Report::Refused { reason, hint, .. } => {
            let text = replay::unasked(&account_.name, &reason);
            emit(Event::Notice { account: AccountId(account_.name.clone()), error: true, text, hint });
        }
    };
    account(connect.connect()?.as_mut(), store, account_, now(), &tell)
}

#[cfg(test)]
mod tests;

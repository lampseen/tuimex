//! Telling the server what was done here (ADR-014). A change leaves the
//! journal when the server has it; one the server refuses is undone here
//! and reported; and when the connection fails, everything not yet
//! confirmed is still in the journal for the next pass.

use std::collections::BTreeSet;

use tuimex_core::{AccountId, Event, Flags, FolderId, MessageId, MoveOutcome, RemoteError, RemoteWrite};
use tuimex_store::journal::{Change, Entry};
use tuimex_store::{Error as StoreError, Result as Stored, Store};

use crate::act::{OUTBOX, count, take_back, unless_gone, waits_in};
use crate::{Account, Error, Report, StoreHandle, sync};

fn id(account: &str, key: &str) -> MessageId {
    MessageId { account: AccountId(account.to_string()), key: key.to_string() }
}

fn pending(
    store: &StoreHandle,
    account: &str,
    wanted: impl Fn(&Entry) -> bool + Send + 'static,
) -> Result<Vec<Entry>, Error> {
    let account = account.to_string();
    store.run(move |s| Ok(s.pending(&account)?.into_iter().filter(|e| wanted(e)).collect()))
}

fn message_id(store: &StoreHandle, id: &MessageId) -> Result<Option<String>, Error> {
    let id = id.clone();
    store.run(move |s| Ok(s.envelope(&id)?.and_then(|envelope| envelope.message_id)))
}

/// Whether the server has never said under which UIDVALIDITY it numbers
/// the folder: one no pass has reached, or one that exists here alone.
fn unnumbered(store: &StoreHandle, account: &str, folder: &str) -> Result<bool, Error> {
    let (account, folder) = (account.to_string(), folder.to_string());
    store.run(move |s| Ok(s.position(&account, &folder)?.uid_validity.is_none()))
}

/// A refusal of a change no ask of this run made, as a `Notice` says it.
pub(crate) fn unasked(account: &str, why: &str) -> String {
    format!("{account} refused a change made earlier: {why}")
}

/// Takes a change the server now has out of the journal. If `u` took it
/// out first, while it was on its way, it is made here again and then
/// journaled the other way round, so that the server is told that too.
fn confirm(store: &Store, id: &MessageId, entry: &Entry) -> Stored<()> {
    match store.confirm(&id.account.0, entry.seq) {
        Err(StoreError::NoEntry(_)) => {
            let back = match &entry.change {
                Change::Flags { set, clear } => Change::Flags { set: *clear, clear: *set },
                Change::Move { .. } => Change::Move { to: entry.folder.clone() },
                // Nothing takes a delete out of the journal but its answer.
                Change::Delete => return Ok(()),
            };
            unless_gone(store.apply(id, &entry.change).and_then(|()| store.change(id, back).map(drop)))
        }
        other => other,
    }
}

/// Drops a change the server cannot be told of, because it has no such
/// message. A copy moved here on the strength of it goes too: it is in a
/// folder of the server's that the server will never show it in.
fn forget(store: &Store, id: &MessageId, entry: &Entry) -> Stored<()> {
    store.confirm(&id.account.0, entry.seq)?;
    match &entry.change {
        Change::Move { to } if waits_in(store, id, to)? => store.remove(id),
        _ => Ok(()),
    }
}

/// Gives a moved message the UID its new folder knows it by. With none,
/// the copy here goes and the folder's next pass brings the server's.
fn number(store: &Store, id: &MessageId, folder: &str, uid: Option<u32>) -> Stored<()> {
    if !waits_in(store, id, folder)? {
        return Ok(());
    }
    let Some(uid) = uid else { return store.remove(id) };
    // A server that keeps one message under several folders, as Gmail
    // does, may answer with a number a file here already has. That file is
    // this same message, and the one just moved is the one `u` remembers.
    if let Some(same) = store.synced(&id.account.0, folder)?.into_iter().find(|m| m.uid == uid) {
        store.remove(&MessageId { account: id.account.clone(), key: same.key })?;
    }
    store.assign_uid(id, uid)
}

/// Replays what the journal holds for a folder, oldest first, on a
/// connection that has it selected and has just been through its pass:
/// what the server changed meanwhile is settled before anything is sent
/// over it. Returns the messages this changed here.
pub(crate) fn folder(
    remote: &mut dyn RemoteWrite,
    store: &StoreHandle,
    (account, folder): (&str, &str),
    tell: &dyn Fn(Report),
) -> Result<Vec<MessageId>, Error> {
    let here = folder.to_string();
    let entries = pending(store, account, move |e| e.folder == here)?;
    let (mut changed, mut landed, mut left_behind, mut marked) = (Vec::new(), Vec::new(), false, false);
    // Messages changed alike go to the server in one command.
    for group in entries.chunk_by(|a, b| a.change == b.change) {
        let mut told = Vec::new();
        for entry in group {
            let id = id(account, &entry.key);
            // A message moved here by a change not yet replayed had no
            // UID when this one was written; it has one now.
            let uid = match entry.uid {
                Some(uid) => Some(uid),
                None => match message_id(store, &id)? {
                    Some(message_id) => remote.find(&message_id)?.pop(),
                    None => None,
                },
            };
            match uid {
                Some(uid) => told.push((id, entry.clone(), uid)),
                None => {
                    let entry = entry.clone();
                    changed.push(id.clone());
                    store.run(move |s| forget(s, &id, &entry))?;
                }
            }
        }
        let (Some((_, first, _)), uids) = (told.first(), told.iter().map(|t| t.2).collect::<Vec<_>>()) else {
            continue;
        };
        let answer = match &first.change {
            Change::Flags { set, clear } => remote.store_flags(&uids, *set, *clear).map(|()| None),
            Change::Move { to } => {
                // A folder made here and never seen on the server, as an
                // Archive the user agreed to: it is made there now.
                let made = if unnumbered(store, account, to)? { remote.create(to) } else { Ok(()) };
                made.and_then(|()| remote.move_to(&uids, to)).map(|outcome| Some((to.clone(), outcome)))
            }
            Change::Delete => remote.delete(&uids).map(|purged| {
                marked |= !purged;
                None
            }),
        };
        match answer {
            Ok(moved) => {
                let done = told.clone();
                store.run(move |s| done.iter().try_for_each(|(id, entry, _)| confirm(s, id, entry)))?;
                if let Some((to, outcome)) = moved {
                    left_behind |= outcome == MoveOutcome::CopiedAndFlagged;
                    landed.extend(told.into_iter().map(|(id, ..)| (to.clone(), id)));
                }
            }
            Err(RemoteError::Refused(why)) => {
                let undone = told.clone();
                store.run(move |s| {
                    undone.iter().try_for_each(|(id, entry, _)| match s.confirm(&id.account.0, entry.seq) {
                        // `u` had taken it back already.
                        Err(StoreError::NoEntry(_)) => Ok(()),
                        confirmed => confirmed.and_then(|()| take_back(s, id, entry)),
                    })
                })?;
                let hint = match first.change {
                    Change::Delete => format!("{} will come back to {folder}", count(told.len())),
                    _ => format!("{} back in {folder}", count(told.len())),
                };
                tell(Report::Refused { seqs: told.iter().map(|t| t.1.seq).collect(), reason: why, hint });
                changed.extend(told.into_iter().map(|(id, ..)| id));
            }
            Err(lost) => return Err(lost.into()),
        }
    }
    if marked {
        let text = format!("{account} cannot purge by name; it will purge what was deleted itself");
        let account = AccountId(account.to_string());
        tell(Report::Event(Event::Notice { account, error: false, text, hint: String::new() }));
    }
    if left_behind {
        let text = format!("{account} cannot remove what it has copied; it will purge the originals itself");
        tell(Report::Event(Event::Notice {
            account: AccountId(account.to_string()),
            error: false,
            text,
            hint: String::new(),
        }));
    }
    // A move numbers a message anew, and no server says the new number
    // in a way the IMAP client passes on: it is asked for (ADR-034).
    let targets: BTreeSet<&String> = landed.iter().map(|(to, _)| to).collect();
    for to in targets {
        let selected = remote.select(to)?;
        if unnumbered(store, account, to)? {
            // The UIDs about to be given to what landed there mean
            // something only under this, and the folder's own pass must
            // find them under it or it starts the folder over (ADR-014).
            let (a, f) = (account.to_string(), to.clone());
            store.run(move |s| s.set_uid_validity(&a, &f, selected.uid_validity))?;
        }
        for (_, id) in landed.iter().filter(|(target, _)| target == to) {
            let uid = match message_id(store, id)? {
                Some(message_id) => remote.find(&message_id)?.pop(),
                None => None,
            };
            let (id, to) = (id.clone(), to.clone());
            changed.push(id.clone());
            store.run(move |s| number(s, &id, &to, uid))?;
        }
    }
    Ok(changed)
}

/// What the journal holds for folders the server has never had. For the
/// Outbox that is each message the SMTP server has accepted: the entry is
/// the record that it was, and leaves only when the copy is filed where
/// PLAN §4.1 says. A change in any other such folder was made to mail that
/// exists here alone. Returns the folders that now hold something this
/// machine has yet to fetch.
pub(crate) fn unsynced(
    remote: &mut dyn RemoteWrite,
    store: &StoreHandle,
    account: &str,
    tell: &dyn Fn(Report),
) -> Result<BTreeSet<String>, Error> {
    let mut filed_in = BTreeSet::new();
    let name = account.to_string();
    let local = store.run(move |s| {
        let mut local = Vec::new();
        for entry in s.pending(&name)? {
            if s.position(&name, &entry.folder)?.uid_validity.is_none() {
                local.push(entry);
            }
        }
        Ok(local)
    })?;
    for entry in local {
        if entry.folder != OUTBOX {
            let id = id(account, &entry.key);
            store.run(move |s| forget(s, &id, &entry))?;
            continue;
        }
        let (id, read) = (id(account, &entry.key), id(account, &entry.key));
        let Change::Move { to } = entry.change.clone() else { continue };
        let found = store.run(move |s| match s.read(&read) {
            Ok(raw) => Ok(Some((raw, s.envelope(&read)?.and_then(|envelope| envelope.message_id)))),
            Err(StoreError::NoMessage { .. }) => Ok(None),
            Err(e) => Err(e),
        })?;
        let filed = match &found {
            None => Ok(()),
            // Gmail files what it sends, and a second copy would be a
            // duplicate.
            Some(_) if remote.capabilities().gmail => Ok(()),
            Some((raw, message_id)) => remote.select(&to).and_then(|_| {
                // A pass that ended between the append and its record
                // left the copy there already.
                let there = message_id.as_ref().map_or(Ok(Vec::new()), |message_id| remote.find(message_id))?;
                if there.is_empty() {
                    remote.append(&to, raw, Flags { seen: true, ..Flags::default() })
                } else {
                    Ok(())
                }
            }),
        };
        let (gone, seq) = (id.clone(), entry.seq);
        match filed {
            // The copy here goes: the folder's pass brings the server's,
            // under the number the server gave it.
            Ok(()) => {
                store.run(move |s| unless_gone(s.remove(&gone)).and_then(|()| s.confirm(&gone.account.0, seq)))?;
                let (account, folder) = (id.account.clone(), FolderId(to.clone()));
                tell(Report::Event(Event::Synced { account, folder, changed: vec![id] }));
                filed_in.insert(to);
            }
            // Never back to the Outbox, from where it would be sent again.
            Err(RemoteError::Refused(why)) => {
                store.run(move |s| s.confirm(&gone.account.0, seq))?;
                let text = format!("{account} would not keep a copy of what was sent: {why}");
                let hint = format!("it is in {to} on this machine alone");
                tell(Report::Event(Event::Notice { account: id.account.clone(), error: true, text, hint }));
            }
            Err(lost) => return Err(lost.into()),
        }
    }
    Ok(filed_in)
}

/// A pass over only the folders the journal names: what follows an
/// action, where a whole pass over the account would ask every folder for
/// everything it holds.
pub(crate) fn account(
    remote: &mut dyn RemoteWrite,
    store: &StoreHandle,
    account: &Account,
    tell: &dyn Fn(Report),
) -> Result<(), Error> {
    let mut folders = unsynced(remote, store, &account.name, tell)?;
    folders.extend(pending(store, &account.name, |e| e.folder != OUTBOX)?.into_iter().map(|e| e.folder));
    let roles = store.run(|s| s.folders())?;
    for folder in folders {
        let known = roles.iter().find(|f| f.account.0 == account.name && f.name == folder);
        if let Some(known) = known {
            sync::pass(remote, store, account, (&folder, known.role), sync::now(), tell)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

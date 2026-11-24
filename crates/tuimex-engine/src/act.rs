//! What the user asks for, done here at once (ADR-015). Every change is in
//! the journal before it is made (ADR-014), and telling the server is
//! `replay`'s. All of this runs on the store's thread, so two actions
//! never interleave.

use tuimex_core::{Action, Flags, FolderId, MessageId, Origin, Query, Role, Sort, Timestamp};
use tuimex_store::journal::{Change, Entry};
use tuimex_store::maildir::{Delivery, new_key};

use tuimex_store::{Error as StoreError, Result as Stored, Store};

use crate::{Account, smtp};

/// The folder mail waits in before it is sent. It exists here alone.
pub(crate) const OUTBOX: &str = "Outbox";
/// What an Archive or a Trash made here is called: the names `roles`
/// looks for on a server that marks none, so the next listing gives each
/// its role.
const ARCHIVE: &str = "Archive";
const TRASH: &str = "Trash";

/// What `u` takes back: the last thing done, as one step however many
/// messages it covered.
pub(crate) enum Last {
    Changed(Vec<(MessageId, Entry)>),
    /// Waiting in the Outbox for the send delay.
    Held(Vec<MessageId>),
}

/// One ask carried out, kept so that `u` can name it by its messages and a
/// late refusal by its serial (ADR-040).
pub(crate) struct Done {
    pub serial: u64,
    /// The messages asked for, in key order.
    pub on: Vec<MessageId>,
    /// None when the ask changed nothing: `u` then finds this, and not an
    /// older ask over the same messages.
    pub last: Option<Last>,
}

/// Why nothing was done, as `Event::ActionFailed` says it.
pub(crate) struct Refusal {
    pub reason: String,
    pub hint: String,
}

impl<E: std::fmt::Display> From<E> for Refusal {
    fn from(error: E) -> Refusal {
        Refusal { reason: error.to_string(), hint: String::new() }
    }
}

pub(crate) fn count(n: usize) -> String {
    if n == 1 { "1 message".to_string() } else { format!("{n} messages") }
}

/// A message that has gone since it was named leaves nothing to do to it.
pub(crate) fn unless_gone(done: Stored<()>) -> Stored<()> {
    match done {
        Err(StoreError::NoMessage { .. }) => Ok(()),
        other => other,
    }
}

fn folder_with(store: &Store, account: &str, role: Role) -> Stored<Option<String>> {
    Ok(store.folders()?.into_iter().find(|f| f.account.0 == account && f.role == role).map(|f| f.name))
}

/// The account's folder with `role`, made here under `name` when its
/// server has none: drafts and sent mail are kept either way.
pub(crate) fn folder_for(store: &Store, account: &str, role: Role, name: &str) -> Stored<String> {
    if let Some(folder) = folder_with(store, account, role)? {
        return Ok(folder);
    }
    store.ensure_folder(account, name, role)?;
    Ok(name.to_string())
}

fn drafts(store: &Store, account: &str) -> Stored<String> {
    folder_for(store, account, Role::Drafts, "Drafts")
}

/// Whether the message is where a move left it: in `folder`, with no UID
/// there yet.
pub(crate) fn waits_in(store: &Store, id: &MessageId, folder: &str) -> Stored<bool> {
    match store.stored(id) {
        Ok((at, stored)) => Ok(at == folder && stored.uid.is_none()),
        Err(StoreError::NoMessage { .. }) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Makes a change the journal recorded not to have happened here, and
/// gives a moved message back the UID it had. The server is not told:
/// this is for a change it never heard of, or refused.
pub(crate) fn take_back(store: &Store, id: &MessageId, entry: &Entry) -> Stored<()> {
    match &entry.change {
        Change::Flags { set, clear } => unless_gone(store.apply(id, &Change::Flags { set: *clear, clear: *set })),
        Change::Move { to } if waits_in(store, id, to)? => {
            store.apply(id, &Change::Move { to: entry.folder.clone() })?;
            entry.uid.map_or(Ok(()), |uid| store.assign_uid(id, uid))
        }
        // What was deleted is not here to put back: the folder's next
        // pass brings the server's again.
        Change::Move { .. } | Change::Delete => Ok(()),
    }
}

/// Reverses the last change (ADR-015). One the server has not heard of is
/// taken out of the journal and never reaches it; one it has applied is
/// journaled the other way round, like any other change.
pub(crate) fn undo(store: &Store, made: &[(MessageId, Entry)]) -> Stored<String> {
    for (id, entry) in made.iter().rev() {
        let account = &id.account.0;
        if store.pending(account)?.iter().any(|pending| pending.seq == entry.seq) {
            store.confirm(account, entry.seq)?;
            take_back(store, id, entry)?;
            continue;
        }
        let back = match &entry.change {
            Change::Flags { set, clear } => Change::Flags { set: *clear, clear: *set },
            Change::Move { .. } => Change::Move { to: entry.folder.clone() },
            Change::Delete => continue,
        };
        match store.change(id, back) {
            Err(StoreError::NoMessage { .. }) => {}
            other => drop(other?),
        }
    }
    Ok(match made.first() {
        Some((_, Entry { folder, change: Change::Move { .. }, .. })) => {
            format!("Undone: {} back in {folder}", count(made.len()))
        }
        _ => format!("Undone: {} as before", count(made.len())),
    })
}

/// Checks each draft and moves it to the Outbox. Nothing is moved unless
/// every one of them can be sent (ADR-015). A draft the server keeps
/// cannot be moved there, since the server has no Outbox: a copy written
/// here is what is sent, and the server's goes to its Trash through the
/// journal, where a move made here alone would leave it in Drafts to be
/// sent a second time.
fn hold(store: &Store, account: &Account, on: &[MessageId]) -> Result<(String, Option<Last>), Refusal> {
    let name = &account.name;
    let fix = |reason: String| Refusal { reason, hint: "edit the draft".to_string() };
    let (mut recipients, mut drafts) = (0, Vec::new());
    for id in on {
        let (folder, stored) = store.stored(id)?;
        let is_draft =
            store.folders()?.iter().any(|f| f.account.0 == *name && f.name == folder && f.role == Role::Drafts);
        if !is_draft && folder != OUTBOX {
            return Err(Refusal::from("only a draft can be sent"));
        }
        let raw = store.read(id)?;
        recipients += smtp::letter(&raw, &account.identities).map_err(|e| fix(e.to_string()))?.to.len();
        drafts.push((id, folder, stored, raw));
    }
    store.ensure_folder(name, OUTBOX, Role::Outbox)?;
    let mut held = Vec::new();
    for (id, folder, stored, raw) in drafts {
        if stored.uid.is_some() {
            let copy = Delivery { key: new_key(), uid: None, origin: stored.origin, flags: stored.flags };
            held.push(store.deliver(name, OUTBOX, &copy, &raw)?);
            if let Some(trash) = folder_with(store, name, Role::Trash)? {
                store.change(id, Change::Move { to: trash })?;
            }
            continue;
        }
        if folder != OUTBOX {
            store.apply(id, &Change::Move { to: OUTBOX.to_string() })?;
        }
        held.push(id.clone());
    }
    let people = if recipients == 1 { "1 recipient".to_string() } else { format!("{recipients} recipients") };
    Ok((format!("Sending to {people} in {}s", account.send_delay.as_secs()), Some(Last::Held(held))))
}

/// Which of `ids` are waiting in the Outbox.
pub(crate) fn held(store: &Store, ids: &[MessageId]) -> Stored<Vec<MessageId>> {
    let mut held = Vec::new();
    for id in ids {
        if waits_in(store, id, OUTBOX)? {
            held.push(id.clone());
        }
    }
    Ok(held)
}

/// Returns to Drafts whichever of `held` are still in the Outbox, where
/// they can be edited and sent again, and says how many that was.
pub(crate) fn release(store: &Store, account: &str, held: &[MessageId]) -> Stored<usize> {
    let (to, mut back) = (drafts(store, account)?, 0);
    for id in held {
        if waits_in(store, id, OUTBOX)? {
            store.apply(id, &Change::Move { to: to.clone() })?;
            back += 1;
        }
    }
    Ok(back)
}

/// Does what `action` asks of the messages `on` names: what to say of it,
/// and what `u` would now take back.
pub(crate) fn perform(
    store: &Store,
    account: &Account,
    action: &Action,
    on: &[MessageId],
) -> Result<(String, Option<Last>), Refusal> {
    let name = &account.name;
    let with_role = |role: Role, word: &str| {
        let none = Refusal {
            reason: format!("{name} has no {word} folder"),
            hint: format!("name one in the config: folders.{} = \"…\"", word.to_lowercase()),
        };
        folder_with(store, name, role)?.ok_or(none)
    };
    let flags = |seen: bool, flagged: bool, on: bool| {
        let these = Flags { seen, flagged, ..Flags::default() };
        if on {
            Change::Flags { set: these, clear: Flags::default() }
        } else {
            Change::Flags { set: Flags::default(), clear: these }
        }
    };
    let (change, verb, after) = match action {
        Action::Archive => (Change::Move { to: with_role(Role::Archive, "Archive")? }, "Archived", String::new()),
        Action::Trash => (Change::Move { to: with_role(Role::Trash, "Trash")? }, "Trashed", String::new()),
        Action::Move(FolderId(to)) => {
            if !store.folders()?.iter().any(|f| f.account.0 == *name && f.name == *to) {
                return Err(Refusal {
                    reason: format!("no folder \"{to}\" in {name}"),
                    hint: "Tab completes".to_string(),
                });
            }
            (Change::Move { to: to.clone() }, "Moved", format!(" to {to}"))
        }
        Action::Flag(on) => (flags(false, true, *on), if *on { "Flagged" } else { "Unflagged" }, String::new()),
        Action::Seen(on) => (flags(true, false, *on), "Marked", if *on { " read" } else { " unread" }.to_string()),
        Action::Send => return hold(store, account, on),
        Action::Discard => return discard(store, name, on),
        Action::CreateArchive(_) | Action::CreateTrash(_) => {
            let (role, folder, a) = match action {
                Action::CreateArchive(_) => (Role::Archive, ARCHIVE, "an"),
                _ => (Role::Trash, TRASH, "a"),
            };
            // Here at once, so that the move that follows has somewhere
            // to go; on the server when that move is replayed.
            if folder_with(store, name, role)?.is_none() {
                // Under the server's own prefix, or it would refuse the
                // folder and every move to it (ADR-048).
                store.ensure_folder(name, &format!("{}{folder}", store.namespace(name)?), role)?;
            }
            return Ok((format!("{name} has {a} {folder} folder"), None));
        }
        other => return Err(Refusal::from(format!("{other:?} is not something the engine does"))),
    };
    let mut made = Vec::new();
    for id in on {
        match store.change(id, change.clone()) {
            Ok(Some(entry)) => made.push((id.clone(), entry)),
            // Already so, or gone since the list was drawn: nothing was
            // changed, and there is nothing to take back.
            Ok(None) | Err(StoreError::NoMessage { .. }) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok((format!("{verb} {}{after}", count(made.len())), (!made.is_empty()).then_some(Last::Changed(made))))
}

/// Deletes every message in the account's Trash (ADR-041): gone here at
/// once, and from the server by name when it is next reached. A message
/// trashed so lately that it has no UID in the server's Trash is kept:
/// the server cannot be told to remove it yet, and would only send it
/// back.
/// Returns how many were deleted and how many kept.
pub(crate) fn empty_trash(store: &Store, account: &str) -> Result<(usize, usize), Refusal> {
    let none = || Refusal::from(format!("{account} has no Trash folder"));
    let trash = &folder_with(store, account, Role::Trash)?.ok_or_else(none)?;
    let all = store.query(&Query::parse("in:trash"), Timestamp(i64::MAX), Sort::NewestFirst, None)?;
    let servers = store.position(account, trash)?.uid_validity.is_some();
    let (mut gone, mut kept) = (0, 0);
    for envelope in all.iter().filter(|e| e.id.account.0 == account && e.folder.0 == *trash) {
        if servers && waits_in(store, &envelope.id, trash)? {
            kept += 1;
        } else {
            unless_gone(store.change(&envelope.id, Change::Delete).map(drop))?;
            gone += 1;
        }
    }
    Ok((gone, kept))
}

/// Deletes drafts the user gave up on. Only a draft: nothing else is
/// deleted without passing through the Trash (ADR-015).
fn discard(store: &Store, account: &str, on: &[MessageId]) -> Result<(String, Option<Last>), Refusal> {
    let folders = store.folders()?;
    let holds_drafts = |folder: &str| {
        folder == OUTBOX || folders.iter().any(|f| f.account.0 == account && f.name == folder && f.role == Role::Drafts)
    };
    for id in on {
        match store.stored(id) {
            Ok((folder, _)) if holds_drafts(&folder) => store.change(id, Change::Delete).map(drop)?,
            Ok(_) => return Err(Refusal::from("only a draft can be discarded")),
            // Never saved, or gone already.
            Err(StoreError::NoMessage { .. }) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok((format!("Discarded {}", count(on.len())), None))
}

/// Files a draft under `key`. A draft that already has that key is an
/// earlier version of it and goes: one the server keeps is deleted there
/// through the journal, since removing it here alone would only have the
/// next sync bring it back.
pub(crate) fn save_draft(store: &Store, id: &MessageId, raw: &[u8], origin: Origin) -> Stored<()> {
    let account = &id.account.0;
    unless_gone(store.change(id, Change::Delete).map(drop))?;
    let flags = Flags { draft: true, seen: true, ..Flags::default() };
    let delivery = Delivery { key: id.key.clone(), uid: None, origin, flags };
    store.deliver(account, &drafts(store, account)?, &delivery, raw).map(drop)
}

#[cfg(test)]
mod tests;

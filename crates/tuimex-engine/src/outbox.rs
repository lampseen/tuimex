//! The Outbox (PLAN §4.4): a message is a file there before it is sent,
//! waits out `send.delay` where `u` still stops it (ADR-015), and leaves
//! only through the journal, which is the record that the server took it.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tuimex_core::{Event, Flags, MessageId, Query, RemoteError, Role, Sort};
use tuimex_store::journal::Change;

use tuimex_store::Store;

use crate::account::{Poke, Shared};
use crate::act::{self, Last, OUTBOX, Refusal, folder_for, waits_in};
use crate::{Backoff, Error, smtp, sync};

pub(crate) enum Post {
    /// Now in the Outbox, to be sent after the delay.
    Queue(Vec<MessageId>),
    /// `u` returned these to Drafts: they are no longer waiting to go.
    Unsent(Vec<MessageId>),
    Stop,
}

impl Shared {
    /// Sends what is due, one message at a time.
    pub(crate) fn sender(&self, posts: &Receiver<Post>) {
        let name = self.account.name.clone();
        let left = self.store.run(move |s| {
            let all = s.query(&Query::parse("in:outbox"), sync::now(), Sort::NewestFirst, None)?;
            Ok(all.into_iter().map(|envelope| envelope.id).filter(|id| id.account.0 == name).collect::<Vec<_>>())
        });
        // Mail an earlier run left in the Outbox is held for the whole
        // delay again. A store that cannot be listed holds nothing to
        // send, and says why when something is next asked of it.
        let wait = |ids: Vec<MessageId>, wait: Duration| ids.into_iter().map(move |id| (id, Instant::now() + wait));
        let mut held: Vec<(MessageId, Instant)> = wait(left.unwrap_or_default(), self.account.send_delay).collect();
        let mut backoff = Backoff::default();
        loop {
            let post = match held.iter().map(|(_, due)| *due).min() {
                None => posts.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(due) => posts.recv_timeout(due.saturating_duration_since(Instant::now())),
            };
            match post {
                Ok(Post::Queue(ids)) => held.extend(wait(ids, self.account.send_delay)),
                // Sent again later, such a message waits its whole delay
                // from then and not what was left of this one.
                Ok(Post::Unsent(ids)) => held.retain(|(id, _)| !ids.contains(id)),
                Ok(Post::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    let now = Instant::now();
                    let (due, later) = std::mem::take(&mut held).into_iter().partition(|(_, at)| *at <= now);
                    held = later;
                    for (id, _) in due {
                        match self.dispatch(&id) {
                            Ok(None) => {}
                            Ok(Some(answered)) => {
                                backoff.reset();
                                self.emit(Event::Sent { id, answered });
                                self.poke(Poke::Replay);
                            }
                            Err(error) => {
                                // A lost connection mends itself; anything
                                // else waits for the user to send again.
                                if matches!(error, Error::Remote(RemoteError::Connection(_))) {
                                    held.push((id.clone(), Instant::now() + backoff.wait()));
                                }
                                let reason = match &error {
                                    Error::Remote(RemoteError::Refused(why)) => why.clone(),
                                    other => other.advice(&self.account.name),
                                };
                                self.emit(Event::SendFailed { id, reason });
                            }
                        }
                    }
                }
            }
        }
    }

    /// A thread that panicked while it held this did so between two whole
    /// claims; what is claimed is still what is on its way.
    fn leaving(&self) -> MutexGuard<'_, HashSet<MessageId>> {
        self.leaving.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `u` on held mail, on the store's thread: whichever of `held` the
    /// sending thread has not claimed goes back to Drafts at once, so that
    /// whoever reads the store next finds it there.
    pub(crate) fn unsend(&self, store: &Store, held: Vec<MessageId>) -> Result<(String, Option<Last>), Refusal> {
        let leaving = self.leaving();
        let (going, waiting): (Vec<MessageId>, Vec<MessageId>) = held.into_iter().partition(|id| leaving.contains(id));
        drop(leaving);
        match act::release(store, &self.account.name, &waiting)? {
            0 if !going.is_empty() => Err(Refusal::from("it is being handed to the server now")),
            0 => Err(Refusal::from("it has already been sent")),
            _ => {
                // A sender that has stopped is an engine that has.
                let _ = self.posts.send(Post::Unsent(waiting));
                Ok(("Not sent: back in Drafts".to_string(), None))
            }
        }
    }

    /// Hands one message to the server. None when it was no longer
    /// waiting to go; otherwise it is now the server's, and these are the
    /// messages it answers. The journal entry written when the server
    /// accepts is what keeps a later run from sending it again; a message
    /// that already has one is only moved on.
    fn dispatch(&self, id: &MessageId) -> Result<Option<Vec<MessageId>>, Error> {
        let (claim, leaving) = (id.clone(), self.leaving.clone());
        let raw = self.store.run(move |s| {
            if !waits_in(s, &claim, OUTBOX)? {
                return Ok(None);
            }
            leaving.lock().unwrap_or_else(PoisonError::into_inner).insert(claim.clone());
            let accepted = s.pending(&claim.account.0)?.into_iter().find(|e| e.key == claim.key && e.folder == OUTBOX);
            match accepted {
                Some(entry) => {
                    s.apply(&claim, &entry.change)?;
                    Ok(Some((None, mark_answered(s, &claim)?)))
                }
                None => s.read(&claim).map(|raw| Some((Some(raw), Vec::new()))),
            }
        })?;
        let Some((raw, answered)) = raw else { return Ok(None) };
        let handed = raw.map_or(Ok(answered), |raw| self.hand_over(id, &raw));
        self.leaving().remove(id);
        handed.map(Some)
    }

    /// Hands a claimed message to the server that sends, and files it.
    /// Returns the messages it answers.
    fn hand_over(&self, id: &MessageId, raw: &[u8]) -> Result<Vec<MessageId>, Error> {
        let letter = smtp::letter(raw, &self.account.identities)?;
        self.sending.store(true, Ordering::SeqCst);
        let sent = self.connect.send(&letter);
        self.sending.store(false, Ordering::SeqCst);
        sent?;
        let filed = id.clone();
        self.store.run(move |s| {
            let sent = folder_for(s, &filed.account.0, Role::Sent, "Sent")?;
            s.change(&filed, Change::Move { to: sent })?;
            let draft = Flags { draft: true, ..Flags::default() };
            s.apply(&filed, &Change::Flags { set: Flags::default(), clear: draft })?;
            mark_answered(s, &filed)
        })
    }
}

/// Marks what `sent` answers as answered: here at once, and on the server
/// through the journal (ADR-014). What it answers is the last message its
/// headers cite, wherever in the account a copy of that is. Returns the
/// messages this changed; a message that cites none answers none.
fn mark_answered(store: &Store, sent: &MessageId) -> tuimex_store::Result<Vec<MessageId>> {
    let cited = store.envelope(sent)?.and_then(|envelope| envelope.references.last().cloned());
    let Some(cited) = cited else { return Ok(Vec::new()) };
    let answered = Flags { answered: true, ..Flags::default() };
    let mut marked = Vec::new();
    for original in store.named(&sent.account.0, &cited)? {
        let change = Change::Flags { set: answered, clear: Flags::default() };
        if original != *sent && store.change(&original, change)?.is_some() {
            marked.push(original);
        }
    }
    Ok(marked)
}

#[cfg(test)]
mod tests;

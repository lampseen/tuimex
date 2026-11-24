//! An answer to an invitation, and an event's mail to its guests, handed
//! to the Outbox the way a sent draft is (ADR-057): filed as a draft, then
//! sent, so that it waits out the send delay where `u` stops it (ADR-015)
//! and leaves only from an address of the account whose Outbox holds it.
//! No calendar is written here: what the calendar should hold is returned
//! to the caller.

use std::collections::BTreeMap;

use tuimex_core::calendar::{Event, Zone};
use tuimex_core::invitation::{Answering, Carries, Outgoing, Record};
use tuimex_core::{AccountId, Action, Ask, CalendarPart, Draft, MessageId, Origin};
use tuimex_store::maildir::new_key;

use crate::Engine;

#[cfg(test)]
mod tests;

/// The event a mail is about, as its calendar part needs it.
pub struct About<'a> {
    pub event: &'a Event,
    /// The zones the event's times name, from the calendar it came in.
    pub zones: &'a BTreeMap<String, Zone>,
    /// The `Message-ID` of the invitation answered, so that the answer
    /// threads under it and marks it answered once sent.
    pub answers: Option<&'a str>,
    /// The `DTSTAMP`, in seconds since the epoch.
    pub stamp: i64,
}

/// Builds a draft's bytes. The one place mail is built is the binary's
/// (PLAN §4.4), which the engine does not depend on, so it is handed in.
pub type Build<'b> = &'b dyn Fn(&Draft) -> Result<Vec<u8>, String>;

/// An answer handed off: the message `u` takes back, by naming it in an
/// `Undo`, when one was sent, and what the calendar should hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    pub held: Option<MessageId>,
    pub record: Record,
}

/// The draft that carries `mail` for the account `account`: its words as
/// the text, and beside them the calendar part the mail carries.
pub fn draft(account: &str, mail: &Outgoing, about: &About) -> Result<Draft, String> {
    let About { event, zones, stamp, .. } = *about;
    let unwritable = || format!("{:?} cannot be written into a calendar", mail.from.email);
    let text = match &mail.carries {
        Carries::Reply(said) => tuimex_content::reply(event, zones, &mail.from, *said, stamp).ok_or_else(unwritable)?,
        Carries::Counter { start, end } => {
            tuimex_content::counter(event, zones, &mail.from, start.clone(), end.clone(), stamp)
                .ok_or_else(unwritable)?
        }
        Carries::Request => tuimex_content::request(event, zones, stamp),
        Carries::Cancel => tuimex_content::cancel(event, zones, stamp),
    };
    Ok(Draft {
        id: MessageId { account: AccountId(account.to_string()), key: new_key() },
        from: mail.from.clone(),
        to: mail.to.clone(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: mail.subject.clone(),
        body: format!("{}\n", mail.body),
        files: Vec::new(),
        in_reply_to: about.answers.map(str::to_string),
        references: about.answers.map(str::to_string).into_iter().collect(),
        origin: Origin::Local,
        calendar: Some(CalendarPart { method: mail.carries.method(), text }),
    })
}

impl Engine {
    /// Files `mail` as a draft of this account and sends it, under the
    /// ask `serial`: the engine then answers that ask as it answers any
    /// `Send`, with `ActionDone` once it is held, or `ActionFailed` when it
    /// cannot leave from this account. Returns the message held, which an
    /// `Undo` naming it takes back to Drafts within the send delay.
    pub fn send_mail(&self, serial: u64, mail: &Outgoing, about: &About, build: Build) -> Result<MessageId, String> {
        let raw = build(&draft(self.account(), mail, about)?)?;
        let id = self.save_draft(raw, None, Origin::Local);
        self.act(Ask { serial, action: Action::Send, on: vec![id.clone()] });
        Ok(id)
    }

    /// Hands off what answering an invitation comes to: its mail, when
    /// there is one. This engine must be that of the account the
    /// invitation came to, whose identities `answering` was worked out
    /// from; an answer from another account's address is refused when it
    /// is sent, as any mail is.
    pub fn answer(&self, serial: u64, answering: &Answering, about: &About, build: Build) -> Result<Answered, String> {
        let held = match &answering.mail {
            Some(mail) => Some(self.send_mail(serial, mail, about, build)?),
            None => None,
        };
        Ok(Answered { held, record: answering.record.clone() })
    }
}

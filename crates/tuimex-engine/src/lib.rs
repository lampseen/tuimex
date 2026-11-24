//! The engine: what keeps the Maildir in step with a mail server, through
//! the `Remote` trait and the store (PLAN §4). One thread per connection
//! and one for the store, talking over channels (ADR-006).

mod account;
mod act;
pub mod agent;
mod backoff;
mod imap;
pub mod oauth;
mod outbox;
pub mod pin;
mod replay;
mod roles;
mod secret;
mod smtp;
mod store;
pub mod sync;

use tuimex_core::{Event, Head, RemoteError, Security, Timestamp};
use tuimex_store::Parsed;

pub use account::{Account, Connect, Engine, Engines};
pub use backoff::Backoff;
pub use imap::{Credential, ImapAccount, Protocol, Seen, Server, certificate, credential, sign_in};
pub use roles::assign as roles;
pub use secret::{Keyring, Secret, SecretError, SystemKeyring, resolve};
pub use smtp::{Letter, check as smtp_check, letter};
pub use store::StoreHandle;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Remote(#[from] RemoteError),
    #[error(transparent)]
    Store(#[from] tuimex_store::Error),
    #[error("the store's thread has stopped")]
    StoreStopped,
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error(transparent)]
    Pin(#[from] pin::PinError),
    #[error(transparent)]
    Content(#[from] tuimex_content::Error),
    #[error(transparent)]
    Oauth(#[from] oauth::OauthError),
    #[error("the folder changed on the server; sync, then open the message again")]
    Stale,
    #[error("{0}")]
    Unsendable(String),
}

/// What a pass over a server has to tell whoever runs it.
pub(crate) enum Report {
    Event(Event),
    /// The server refused the journal entries `seqs`, which have been
    /// taken back here. Only the running account knows which ask made
    /// them, if any of this run did (ADR-040).
    Refused {
        seqs: Vec<u64>,
        reason: String,
        hint: String,
    },
}

impl Error {
    /// Whether trying again later can help. A refused sign-in or a missing
    /// password stays refused until the user changes something, and a
    /// failing disk is not the server's to mend (PLAN §4.2).
    /// The error as the status line or the command line says it. A refused
    /// sign-in says what to do about it and nothing the server said
    /// (PLAN §4.2).
    pub fn advice(&self, account: &str) -> String {
        match self {
            Error::Remote(RemoteError::Auth) => format!("signing in to {account} failed; run `tuimex setup`"),
            Error::Secret(e) => format!("{e}; run `tuimex setup`"),
            // Each of these says for itself what to run.
            Error::Oauth(e) => e.to_string(),
            other => format!("{account}: {other}"),
        }
    }

    pub fn retries(&self) -> bool {
        let outage = matches!(self, Error::Oauth(oauth::OauthError::Unreachable(_)));
        outage
            || matches!(
                self,
                Error::Remote(RemoteError::Connection(_) | RemoteError::Refused(_) | RemoteError::Protocol(_))
            )
    }
}

/// The columns a body is read at for the index: wide enough that a
/// paragraph is one line, since search does not care where lines break and
/// the list's preview wraps the opening to the screen itself.
const INDEX_WIDTH: usize = 10_000;

fn calendar(file: &tuimex_core::Attachment) -> bool {
    file.mime.eq_ignore_ascii_case("text/calendar")
}

/// What the index keeps of a message, for `Store::open`: the one place the
/// store's parse is tied to `tuimex-content` (ADR-007), so that every
/// opener of the store indexes alike. Bytes that are not a message are
/// indexed with nothing known about them, so the file is still listed.
pub fn parse(raw: &[u8]) -> Parsed {
    let nothing = |_| Head {
        date: None,
        from: Vec::new(),
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        subject: String::new(),
        message_id: None,
        references: Vec::new(),
        has_attachments: false,
        security: Security::None,
    };
    let head = tuimex_content::envelope(raw).unwrap_or_else(nothing);
    // A body that cannot be read is not searchable; opening it says why.
    let body = tuimex_content::body(raw, INDEX_WIDTH).ok();
    Parsed {
        // A message with no date it can be sorted by sorts before every
        // message that has one.
        date: head.date.unwrap_or(Timestamp(0)),
        from: head.from,
        to: head.to,
        cc: head.cc,
        bcc: head.bcc,
        reply_to: head.reply_to,
        subject: head.subject,
        message_id: head.message_id,
        references: head.references,
        // The header counts an invitation's calendar part as a file,
        // which a list row and `has:attachment` do not.
        has_attachments: head.has_attachments
            && !body
                .as_ref()
                .is_some_and(|b| b.invite.is_some() && b.attachments.iter().all(|a| calendar(a) || a.inline)),
        has_invite: body.as_ref().is_some_and(|b| b.invite.is_some()),
        has_unsubscribe: body.as_ref().is_some_and(|b| b.unsubscribe.is_some()),
        security: head.security,
        // An invitation's row says `invite`; its calendar part is not
        // also one of its files.
        attachments: body.iter().flat_map(|b| b.attachments.iter().filter(|a| !calendar(a)).cloned()).collect(),
        text: body.map(|b| b.text).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests;

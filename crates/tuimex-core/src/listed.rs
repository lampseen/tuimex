//! A message as the list shows it. The type lives here because the store
//! and the fixtures produce it and the UI draws it (ADR-001).

use crate::{Attachment, Envelope};

/// An envelope, and what a list row and the preview under it show that an
/// envelope does not carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub envelope: Envelope,
    /// The opening of the body. A reply's row in an unfolded thread shows
    /// it where a subject would only repeat the thread's.
    pub snippet: String,
    /// The files a row counts and the preview names. An invitation's
    /// calendar part is not one of them: its row says `invite` instead.
    pub attachments: Vec<Attachment>,
    pub invite: bool,
    /// Whether the message says how to leave its list (`has:unsubscribe`).
    pub unsubscribe: bool,
    /// The body as the reader shows it, once it has been read.
    pub text: Option<String>,
    /// The address earlier messages of its thread came from under the same
    /// name, when this one's differs (PLAN §6.5).
    pub sender_was: Option<String>,
    /// Set while the message is in the outbox.
    pub sending: Option<Sending>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sending {
    /// Held for the send delay, with the seconds left of it.
    Held {
        seconds: u32,
    },
    Failed {
        reason: String,
    },
}

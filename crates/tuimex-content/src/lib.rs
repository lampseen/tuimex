//! Hostile bytes to plain data: the only crate that parses a message
//! (ADR-007). It has no dependency that opens a file or a socket, so a
//! parsing bug can produce wrong text and nothing worse.

mod calendar;
mod css;
mod dav;
mod html;
mod message;
mod sanitise;
mod text;
mod trackers;

use tuimex_core::Link;

pub use calendar::{calendar, cancel, counter, event_text, events_text, invite, new_uid, reply, request, with_answer};
pub use dav::{DavItem, multistatus};
pub use message::{
    Payload, Structure, Submission, attachment, body, browser_copy, calendar_part, envelope, structure, submission,
    unsubscribe,
};
pub use sanitise::{InlineImage, sanitise};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("not a message: no header block was found")]
    NotAMessage,
    #[error("the message has no attachment {0}")]
    NoSuchAttachment(usize),
    #[error("the HTML could not be read: {0}")]
    Html(String),
    #[error("the calendar server's answer could not be read: {0}")]
    Dav(String),
}

/// HTML as the reader shows it, and the links its `[n]` marks stand for.
pub fn html_to_text(html: &str, width: usize) -> Result<(String, Vec<Link>), Error> {
    html::read(html, width).map(|page| (page.text, page.links))
}

/// The tracking services `html` would report to, by name; an unseen image
/// from a host nobody has a name for is listed by its host.
pub fn trackers(html: &str) -> Result<Vec<String>, Error> {
    html::read(html, 80).map(|page| page.trackers)
}

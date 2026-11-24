//! Hostile bytes to plain data: the only crate that parses a message
//! (ADR-007). It has no dependency that opens a file or a socket, so a
//! parsing bug can produce wrong text and nothing worse.

mod calendar;
mod css;
mod html;
mod message;
mod sanitise;
mod text;
mod trackers;

use tuimex_core::Link;

pub use calendar::{calendar, invite};
pub use message::{
    Payload, Structure, Submission, attachment, body, browser_copy, envelope, structure, submission, unsubscribe,
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

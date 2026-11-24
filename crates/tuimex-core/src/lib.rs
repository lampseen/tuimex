//! tuimex's domain: the data every crate passes around, and the pure rules
//! over it. Nothing here opens a file, a socket or a terminal, and no
//! workspace crate sits below this one (ADR-001).

mod action;
pub mod calendar;
mod compose;
mod link;
mod listed;
mod message;
mod policy;
mod query;
mod redact;
mod remote;
mod thread;

pub use action::{Action, Answer, Ask, Check, Event, Hit, Start};
pub use compose::{AgentAsk, Attached, CalendarPart, Draft, Identity, addresses, is_address, pieces, registrable};
pub use link::{Assessment, Url, Warning, assess};
pub use listed::{Listed, Sending};
pub use message::{
    AccountId, Address, Attachment, Body, Envelope, EventTime, Flags, Folder, FolderId, Head, Invite, Link, MessageId,
    Origin, Picture, Role, Security, Thread, ThreadNode, Timestamp, Unsubscribe,
};
pub use policy::{Denied, Grant, Grants, Request, allows};
pub use query::{Has, Is, Query, Section, Sort, Term, When};
pub use redact::{Links, Redacted, redact};
pub use remote::{
    Capabilities, Deadline, Fetched, FlagChange, Interrupt, MoveOutcome, Remote, RemoteError, RemoteFolder,
    RemoteWrite, Selected, UidRange, Woken,
};
pub use thread::{thread, thread_at};

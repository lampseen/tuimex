//! Synthetic mail for tests and for `tuimex --fixtures`. Every address is
//! under a reserved example domain.
//!
//! There are two mailboxes. `design` is the one `design/tui.js` draws,
//! which the list goldens and `tuimex --fixtures` show. `mailbox` is raw
//! messages beside the fields a correct parse of them gives, which the
//! store and the agent command line are tested against.
//!
//! `remote` is a mail server in memory and `google` is Google's sign-in on
//! a loopback port, for the engine and `tuimex setup` to be tested against.

pub mod design;
mod google;
mod mailbox;
mod remote;

pub use google::FakeGoogle;
pub use mailbox::{FOLDERS, Mail, NOW, mailbox};
pub use remote::{FakeRemote, FakeServer};

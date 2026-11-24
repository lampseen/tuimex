//! What tuimex keeps on disk: the Maildir, the index, the journal, and the
//! one module that creates files (ADR-011).

pub mod files;
pub mod index;
pub mod journal;
pub mod maildir;
mod store;

use std::io;
use std::path::{Path, PathBuf};

pub use store::{Parsed, Rebuilt, Sole, Store, sole};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("the index: {0}")]
    Index(#[from] rusqlite::Error),
    #[error("{0:?} cannot name an account, a folder or a message")]
    Name(String),
    #[error("there is no message {key} in {account}")]
    NoMessage { account: String, key: String },
    #[error("{account} already has a message {key}")]
    Exists { account: String, key: String },
    #[error("{}: line {line} is not a journal entry: {reason}", path.display())]
    Journal { path: PathBuf, line: usize, reason: String },
    #[error("the journal has no entry {0}")]
    NoEntry(u64),
    #[error("the mail under {} is in use by another tuimex", .0.display())]
    InUse(PathBuf),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Says which path an I/O error was about, which the error itself does not.
pub(crate) trait At<T> {
    fn at(self, path: &Path) -> Result<T>;
}

impl<T> At<T> for io::Result<T> {
    fn at(self, path: &Path) -> Result<T> {
        self.map_err(|source| Error::Io { path: path.to_path_buf(), source })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod testing;

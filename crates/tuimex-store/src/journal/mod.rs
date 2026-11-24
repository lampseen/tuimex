//! An account's `journal.jsonl`: the changes made here that its server has
//! not confirmed (ADR-014). An entry is on disk before the change it
//! describes is applied locally, so a crash at any point leaves either
//! nothing to do or a line saying what to do again.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tuimex_core::Flags;

use crate::{At, Error, Result, files};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Names the entry among those still pending.
    pub seq: u64,
    pub key: String,
    /// Where the message was before the change and its UID there, which
    /// is how the server knows it.
    pub folder: String,
    pub uid: Option<u32>,
    pub change: Change,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Flags {
        #[serde(with = "FlagsLine")]
        set: Flags,
        #[serde(with = "FlagsLine")]
        clear: Flags,
    },
    /// To another folder of the same account: archive and trash are moves.
    Move { to: String },
    /// Gone from here for good, and to be removed from the server by its
    /// UID (ADR-041). It cannot be taken back.
    Delete,
}

/// How `Flags` is written in a line; the type itself is `tuimex-core`'s.
#[derive(Serialize, Deserialize)]
#[serde(remote = "Flags")]
struct FlagsLine {
    seen: bool,
    flagged: bool,
    answered: bool,
    draft: bool,
}

pub struct Journal {
    path: PathBuf,
}

impl Journal {
    pub fn new(account_dir: &Path) -> Journal {
        Journal { path: account_dir.join("journal.jsonl") }
    }

    /// The entries, and whether the file stops short of a line's end. A
    /// last line that does not parse is not an entry: `record` returns only
    /// after its line is whole and on disk, so the change it described was
    /// never applied.
    fn load(&self) -> Result<(Vec<Entry>, bool)> {
        let text = match fs::read_to_string(&self.path) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok((Vec::new(), false)),
            other => other.at(&self.path)?,
        };
        let cut_short = !text.is_empty() && !text.ends_with('\n');
        let mut entries = Vec::new();
        let lines: Vec<&str> = text.split('\n').collect();
        for (n, line) in lines.iter().enumerate().filter(|(_, line)| !line.is_empty()) {
            match serde_json::from_str(line) {
                Ok(entry) => entries.push(entry),
                Err(_) if cut_short && n + 1 == lines.len() => {}
                Err(e) => return Err(Error::Journal { path: self.path.clone(), line: n + 1, reason: e.to_string() }),
            }
        }
        Ok((entries, cut_short))
    }

    fn rewrite(&self, entries: &[Entry]) -> Result<()> {
        let mut text = String::new();
        for entry in entries {
            text += &line(entry);
        }
        files::replace(&self.path, text.as_bytes()).at(&self.path)
    }

    /// Every unconfirmed change, oldest first: the order to replay them in.
    pub fn pending(&self) -> Result<Vec<Entry>> {
        Ok(self.load()?.0)
    }

    /// Writes the change down. The caller applies it only after this
    /// returns.
    pub fn record(&self, key: &str, folder: &str, uid: Option<u32>, change: Change) -> Result<Entry> {
        let (entries, cut_short) = self.load()?;
        if cut_short {
            // Appending would join the new line to the broken one.
            self.rewrite(&entries)?;
        }
        let seq = entries.iter().map(|e| e.seq).max().unwrap_or(0) + 1;
        let entry = Entry { seq, key: key.to_string(), folder: folder.to_string(), uid, change };
        let mut file = files::append(&self.path).at(&self.path)?;
        file.write_all(line(&entry).as_bytes()).and_then(|()| file.sync_all()).at(&self.path)?;
        Ok(entry)
    }

    /// The server has the change, or has refused it: either way it is no
    /// longer something to replay.
    pub fn confirm(&self, seq: u64) -> Result<()> {
        let (mut entries, _) = self.load()?;
        let before = entries.len();
        entries.retain(|e| e.seq != seq);
        if entries.len() == before {
            return Err(Error::NoEntry(seq));
        }
        self.rewrite(&entries)
    }
}

fn line(entry: &Entry) -> String {
    // Serialising cannot fail: an entry is strings, numbers and booleans,
    // with no map whose keys could be anything but strings.
    #[allow(clippy::expect_used)]
    let json = serde_json::to_string(entry).expect("a journal entry is plain data");
    json + "\n"
}

#[cfg(test)]
mod tests;

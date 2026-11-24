//! What the reader decided about invitations that no event's file says
//! (ADR-057): which they ignored, and the time they proposed for which.
//! It is the reader's own, like a local event, so it is one file beside
//! those under the data directory and is never in the cache.
//!
//! The file is JSON: `{"ignored": [[account, uid]], "proposed": [[account,
//! uid, start]]}`, where `start` is seconds on the reader's wall clock.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tuimex_store::calendar::LOCAL;
use tuimex_store::files;

#[cfg(test)]
mod tests;

/// Beside the accounts' directories, which hold events and no file of
/// this name: an event's file is a key and `.ics`.
pub const FILE: &str = "answers.json";

/// More than any reader's list of invitations comes to.
const LARGEST: u64 = 4 << 20;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Own {
    #[serde(default)]
    pub ignored: Vec<(String, String)>,
    #[serde(default)]
    pub proposed: Vec<(String, String, i64)>,
}

fn path(data: &Path) -> PathBuf {
    data.join(LOCAL).join(FILE)
}

impl Own {
    /// What was kept, or nothing when nothing ever was. A file that does
    /// not read is an error: starting from nothing would write over it.
    pub fn load(data: &Path) -> Result<Own, String> {
        let path = path(data);
        match files::read_own(&path, LARGEST) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Own::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn save(&self, data: &Path) -> Result<(), String> {
        let path = path(data);
        let failed = |e: std::io::Error| format!("{}: {e}", path.display());
        let bytes = serde_json::to_vec(self).map_err(|e| format!("{}: {e}", path.display()))?;
        files::private_dir(&data.join(LOCAL)).map_err(failed)?;
        files::replace(&path, &bytes).map_err(failed)
    }

    pub fn ignores(&self, account: &str, uid: &str) -> bool {
        self.ignored.iter().any(|(a, u)| a == account && u == uid)
    }

    /// Forgets the time proposed for an event: it was answered outright,
    /// or its organizer sent a new version.
    pub fn settle(&mut self, account: &str, uid: &str) {
        self.proposed.retain(|(a, u, _)| !(a == account && u == uid));
    }
}

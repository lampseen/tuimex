//! Calendars on disk (ADR-055). An event tuimex itself holds is one
//! iCalendar file under the data directory and is the truth. What was
//! fetched from a provider is rows in a database under the cache
//! directory, and can be deleted and fetched again.
//!
//! This module keeps text and never reads it: turning calendar text into
//! events is `tuimex-content`'s work (ADR-007).

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::index::open_cache;
use crate::maildir::new_key;
use crate::{At, Error, Result, files};

#[cfg(test)]
mod tests;

/// Stored in the database's `user_version`; any other value and the
/// tables are made again, empty, to be filled by the next fetch.
const VERSION: i32 = 1;

const SCHEMA: &str = "
DROP TABLE IF EXISTS resources;
DROP TABLE IF EXISTS calendars;
CREATE TABLE calendars (
    account TEXT NOT NULL,
    calendar TEXT NOT NULL,
    name TEXT,
    tag TEXT,
    read_at INTEGER NOT NULL,
    PRIMARY KEY (account, calendar)
);
CREATE TABLE resources (
    account TEXT NOT NULL,
    calendar TEXT NOT NULL,
    href TEXT NOT NULL,
    etag TEXT,
    text TEXT NOT NULL,
    PRIMARY KEY (account, calendar, href)
);
";

/// The directory under the data directory that holds local events. An
/// account's name cannot begin with a dot, so no account's mail is here.
pub const LOCAL: &str = ".calendars";

/// One calendar as last fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub account: String,
    /// The calendar's address at its provider, or `feed` for a feed.
    pub calendar: String,
    pub name: Option<String>,
    /// What the provider said identifies this state of the calendar.
    pub tag: Option<String>,
    /// Seconds since the epoch at which it was last read whole.
    pub read_at: i64,
}

/// Calendar text and where it is held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub account: String,
    /// `None` for an event that is only here.
    pub calendar: Option<String>,
    /// The address within the calendar, or the key of a local event.
    pub href: String,
    pub etag: Option<String>,
    pub text: String,
}

pub struct Calendars {
    conn: Connection,
    local: PathBuf,
}

impl Calendars {
    /// `data` holds local events and `cache` what was fetched.
    pub fn open(data: &Path, cache: &Path) -> Result<Calendars> {
        files::private_dir(cache).at(cache)?;
        let conn = open_cache(&cache.join("calendar.sqlite"))?;
        let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != VERSION {
            conn.execute_batch(SCHEMA)?;
            conn.pragma_update(None, "user_version", VERSION)?;
        }
        Ok(Calendars { conn, local: data.join(LOCAL) })
    }

    /// Records that a calendar was read whole, with the tag it had.
    pub fn read(&self, account: &str, calendar: &str, name: Option<&str>, tag: Option<&str>, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO calendars (account, calendar, name, tag, read_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (account, calendar) DO UPDATE SET name = ?3, tag = ?4, read_at = ?5",
            params![account, calendar, name, tag, now],
        )?;
        Ok(())
    }

    pub fn fetched(&self) -> Result<Vec<Fetched>> {
        let mut statement = self
            .conn
            .prepare("SELECT account, calendar, name, tag, read_at FROM calendars ORDER BY account, calendar")?;
        let rows = statement.query_map([], |row| {
            Ok(Fetched {
                account: row.get(0)?,
                calendar: row.get(1)?,
                name: row.get(2)?,
                tag: row.get(3)?,
                read_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Keeps one thing of a calendar, in place of what was held under the
    /// same address.
    pub fn put(&self, account: &str, calendar: &str, href: &str, etag: Option<&str>, text: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO resources (account, calendar, href, etag, text) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (account, calendar, href) DO UPDATE SET etag = ?4, text = ?5",
            params![account, calendar, href, etag, text],
        )?;
        Ok(())
    }

    /// Forgets one thing of a calendar; forgetting what is not held is not
    /// an error.
    pub fn drop_one(&self, account: &str, calendar: &str, href: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM resources WHERE account = ?1 AND calendar = ?2 AND href = ?3",
            params![account, calendar, href],
        )?;
        Ok(())
    }

    /// Forgets a calendar and everything held of it: one the account no
    /// longer has.
    pub fn forget(&self, account: &str, calendar: &str) -> Result<()> {
        self.conn.execute("DELETE FROM resources WHERE account = ?1 AND calendar = ?2", params![account, calendar])?;
        self.conn.execute("DELETE FROM calendars WHERE account = ?1 AND calendar = ?2", params![account, calendar])?;
        Ok(())
    }

    /// The address and tag of everything held of a calendar, which is what
    /// a fetch compares with what the provider lists.
    pub fn tags(&self, account: &str, calendar: &str) -> Result<Vec<(String, Option<String>)>> {
        let mut statement =
            self.conn.prepare("SELECT href, etag FROM resources WHERE account = ?1 AND calendar = ?2 ORDER BY href")?;
        let rows = statement.query_map(params![account, calendar], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn tag(&self, account: &str, calendar: &str) -> Result<Option<String>> {
        let found = self
            .conn
            .query_row(
                "SELECT tag FROM calendars WHERE account = ?1 AND calendar = ?2",
                params![account, calendar],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.flatten())
    }

    /// Everything held, fetched and local, by account.
    pub fn held(&self) -> Result<Vec<Held>> {
        let mut statement = self
            .conn
            .prepare("SELECT account, calendar, href, etag, text FROM resources ORDER BY account, calendar, href")?;
        let rows = statement.query_map([], |row| {
            Ok(Held {
                account: row.get(0)?,
                calendar: Some(row.get(1)?),
                href: row.get(2)?,
                etag: row.get(3)?,
                text: row.get(4)?,
            })
        })?;
        let mut held: Vec<Held> = rows.collect::<rusqlite::Result<_>>()?;
        for account in self.local_accounts()? {
            let dir = self.local.join(&account);
            let mut keys: Vec<String> = Vec::new();
            for entry in fs::read_dir(&dir).at(&dir)? {
                let name = entry.at(&dir)?.file_name();
                keys.extend(
                    name.to_str().and_then(|n| n.strip_suffix(".ics")).filter(|k| valid_key(k)).map(str::to_string),
                );
            }
            keys.sort();
            for key in keys {
                let path = dir.join(format!("{key}.ics"));
                // A file that is not text is not an event tuimex wrote;
                // it is left where it is and not shown.
                if let Ok(text) = fs::read_to_string(&path) {
                    held.push(Held { account: account.clone(), calendar: None, href: key, etag: None, text });
                }
            }
        }
        Ok(held)
    }

    fn local_accounts(&self) -> Result<Vec<String>> {
        let entries = match fs::read_dir(&self.local) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            other => other.at(&self.local)?,
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.at(&self.local)?;
            if entry.file_type().at(&self.local)?.is_dir() {
                names.extend(entry.file_name().into_string().ok().filter(|n| valid_account(n)));
            }
        }
        names.sort();
        Ok(names)
    }

    fn local_path(&self, account: &str, key: &str) -> Result<PathBuf> {
        if !valid_account(account) {
            return Err(Error::Name(account.to_string()));
        }
        if !valid_key(key) {
            return Err(Error::Name(key.to_string()));
        }
        Ok(self.local.join(account).join(format!("{key}.ics")))
    }

    /// Keeps a new local event and returns the key it was given. The key
    /// is tuimex's own: an event's UID is text from outside and never
    /// becomes a path.
    pub fn keep(&self, account: &str, text: &str) -> Result<String> {
        let key = new_key();
        let path = self.local_path(account, &key)?;
        let dir = self.local.join(account);
        files::private_dir(&dir).at(&dir)?;
        let mut file = files::create(&path).at(&path)?;
        file.write_all(text.as_bytes()).at(&path)?;
        file.sync_all().at(&path)?;
        Ok(key)
    }

    /// Puts new text in place of a local event's, whole or not at all.
    pub fn rewrite(&self, account: &str, key: &str, text: &str) -> Result<()> {
        let path = self.local_path(account, key)?;
        if !path.is_file() {
            return Err(Error::NoEvent { account: account.to_string(), key: key.to_string() });
        }
        files::replace(&path, text.as_bytes()).at(&path)
    }

    /// The text of a local event, which a delete hands back so that it
    /// can be undone.
    pub fn local(&self, account: &str, key: &str) -> Result<String> {
        let path = self.local_path(account, key)?;
        match fs::read_to_string(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => {
                Err(Error::NoEvent { account: account.to_string(), key: key.to_string() })
            }
            other => other.at(&path),
        }
    }

    pub fn remove(&self, account: &str, key: &str) -> Result<()> {
        let path = self.local_path(account, key)?;
        match fs::remove_file(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => {
                Err(Error::NoEvent { account: account.to_string(), key: key.to_string() })
            }
            other => other.at(&path),
        }
    }
}

fn valid_account(account: &str) -> bool {
    !account.is_empty() && !account.starts_with('.') && !account.contains(['/', '\0'])
}

/// A key as `new_key` makes them: letters, digits and dots, with no dot
/// first.
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with('.')
        && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

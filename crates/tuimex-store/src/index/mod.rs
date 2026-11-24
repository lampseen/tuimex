//! The SQLite index over every account's Maildir: envelopes, flags, what
//! threading needs, the search index and sync positions. It is a cache
//! (ADR-013): nothing is here that `rebuild` cannot work out again from the
//! Maildir, except a folder's modseq, whose loss costs one fetch of flags.

mod search;

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, Row, params};
use tuimex_core::{
    AccountId, Address, Attachment, Envelope, Flags, Folder, FolderId, Listed, MessageId, Origin, Role, Security,
    Timestamp,
};

use crate::maildir::{Stored, role_name, role_named};
use crate::{At, Error, Parsed, Result, files};

/// Stored in the database's `user_version`. An index that says anything
/// else, including one that was never filled, is rebuilt.
const VERSION: i32 = 4;

const SCHEMA: &str = "
DROP TABLE IF EXISTS search;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS folders;
CREATE TABLE folders (
    account TEXT NOT NULL,
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    uid_validity INTEGER,
    highest_modseq INTEGER,
    PRIMARY KEY (account, name)
);
CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    account TEXT NOT NULL,
    key TEXT NOT NULL,
    folder TEXT NOT NULL,
    file TEXT NOT NULL,
    uid INTEGER,
    date INTEGER NOT NULL,
    sender TEXT NOT NULL,
    recipients TEXT NOT NULL,
    cc TEXT NOT NULL,
    bcc TEXT NOT NULL,
    reply_to TEXT NOT NULL,
    subject TEXT NOT NULL,
    message_id TEXT,
    refs TEXT NOT NULL,
    seen INTEGER NOT NULL,
    flagged INTEGER NOT NULL,
    answered INTEGER NOT NULL,
    draft INTEGER NOT NULL,
    size INTEGER NOT NULL,
    attachments INTEGER NOT NULL,
    invite INTEGER NOT NULL,
    unsubscribe INTEGER NOT NULL,
    opening TEXT NOT NULL,
    files TEXT NOT NULL,
    security TEXT NOT NULL,
    origin TEXT NOT NULL,
    UNIQUE (account, key)
);
CREATE INDEX messages_by_folder ON messages (account, folder, date);
CREATE INDEX messages_by_date ON messages (date);
CREATE VIRTUAL TABLE search USING fts5 (
    subject, sender, recipients, cc, body,
    content = '', contentless_delete = 1,
    tokenize = 'unicode61 remove_diacritics 2'
);
";

/// The columns `envelope` reads, in the order it reads them.
const ENVELOPE: &str = "m.account, m.key, m.folder, m.date, m.sender, m.recipients, m.cc, m.subject, m.message_id, \
                        m.refs, m.seen, m.flagged, m.answered, m.draft, m.size, m.attachments, m.security, m.origin, m.bcc, \
                        m.reply_to";

const SECURITY: [(Security, &str); 4] = [
    (Security::None, "none"),
    (Security::Signed, "signed"),
    (Security::Encrypted, "encrypted"),
    (Security::Both, "both"),
];
const ORIGIN: [(Origin, &str); 3] = [(Origin::Server, "server"), (Origin::Local, "local"), (Origin::Agent, "agent")];

/// The word for `value` in `table`, and back. A word no table has reads as
/// the table's first entry, which is the one that claims nothing.
fn word<T: Copy + PartialEq>(table: &[(T, &'static str)], value: T) -> &'static str {
    table.iter().find(|(v, _)| *v == value).map_or(table[0].1, |(_, word)| word)
}

fn meant<T: Copy>(table: &[(T, &'static str)], word: &str) -> T {
    table.iter().find(|(_, w)| *w == word).map_or(table[0].0, |(value, _)| *value)
}

/// Between two list items, and between a name and its address. Neither can
/// be typed in a header, so they are dropped from whatever a parse returns.
const ITEM: char = '\u{1e}';
const PART: char = '\u{1f}';

fn clean(text: &str) -> String {
    text.replace([ITEM, PART], "")
}

fn pack(addresses: &[Address]) -> String {
    let one = |a: &Address| format!("{}{PART}{}", clean(a.name.as_deref().unwrap_or_default()), clean(&a.email));
    addresses.iter().map(one).collect::<Vec<_>>().join(&ITEM.to_string())
}

fn unpack(packed: &str) -> Vec<Address> {
    let one = |item: &str| {
        let (name, email) = item.split_once(PART).unwrap_or(("", item));
        Address { name: Some(name.to_string()).filter(|n| !n.is_empty()), email: email.to_string() }
    };
    packed.split(ITEM).filter(|item| !item.is_empty()).map(one).collect()
}

/// How much of a body's opening a list row and its preview are given.
const OPENING: usize = 600;

/// The first `OPENING` characters of the reader's text, less the space
/// around them.
fn opening(text: &str) -> &str {
    let text = text.trim();
    text.char_indices().nth(OPENING).map_or(text, |(end, _)| &text[..end])
}

fn pack_files(files: &[Attachment]) -> String {
    let one = |a: &Attachment| {
        format!("{}{PART}{}{PART}{}{PART}{}{PART}{}", a.index, clean(&a.name), clean(&a.mime), a.size, a.inline)
    };
    files.iter().map(one).collect::<Vec<_>>().join(&ITEM.to_string())
}

/// A file whose record does not read, as from an index another version
/// wrote, is left out: the index is a cache, and opening the message reads
/// the files from the message.
fn unpack_files(packed: &str) -> Vec<Attachment> {
    let one = |item: &str| {
        let mut parts = item.split(PART);
        let (index, name, mime) = (parts.next()?.parse().ok()?, parts.next()?, parts.next()?);
        let (size, inline) = (parts.next()?.parse().ok()?, parts.next()? == "true");
        Some(Attachment { index, name: name.to_string(), mime: mime.to_string(), size, inline })
    };
    packed.split(ITEM).filter_map(one).collect()
}

/// Where the Maildir keeps a message, as the index last saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub folder: String,
    pub file: String,
}

/// How far sync has got in one folder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    pub uid_validity: Option<u32>,
    pub highest_uid: Option<u32>,
    pub highest_modseq: Option<u64>,
}

/// One message the server knows, as sync compares it with the server's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    pub key: String,
    pub uid: u32,
    pub flags: Flags,
}

pub struct Index {
    conn: Connection,
}

impl Index {
    /// Opens the index at `path`, making an empty one if there is none. A
    /// file there that is not a database is replaced: it is a cache.
    pub fn open(path: &Path) -> Result<Index> {
        match Self::connect(path) {
            Err(Error::Index(rusqlite::Error::SqliteFailure(e, _))) if e.code == ErrorCode::NotADatabase => {
                for suffix in ["", "-wal", "-shm"] {
                    let mut file = path.as_os_str().to_owned();
                    file.push(suffix);
                    let file = PathBuf::from(file);
                    match fs::remove_file(&file) {
                        Err(e) if e.kind() != ErrorKind::NotFound => return Err(e).at(&file),
                        _ => {}
                    }
                }
                Self::connect(path)
            }
            other => other,
        }
    }

    fn connect(path: &Path) -> Result<Index> {
        match files::create(path) {
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // A link here would have SQLite write wherever it points.
                if !fs::symlink_metadata(path).at(path)?.is_file() {
                    return Err(std::io::Error::from(ErrorKind::InvalidInput)).at(path);
                }
            }
            made => drop(made.at(path)?),
        }
        // Without the create flag SQLite opens the owner-only file made
        // above and never makes one of its own; its WAL and shared-memory
        // files take their mode from the database's.
        let conn =
            Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        // The agent command line reads while a running tuimex writes: WAL
        // lets it, and the timeout covers the moments it cannot.
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(Index { conn })
    }

    /// False for an index that has to be rebuilt before it is read.
    pub fn is_current(&self) -> Result<bool> {
        Ok(self.conn.pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))? == VERSION)
    }

    /// Runs `work` so that all of it is in the index afterwards or none is.
    fn atomically<T>(&self, work: impl FnOnce() -> Result<T>) -> Result<T> {
        self.conn.execute_batch("SAVEPOINT step")?;
        let done = work();
        self.conn.execute_batch(if done.is_ok() { "RELEASE step" } else { "ROLLBACK TO step; RELEASE step" })?;
        done
    }

    /// Empties the index and lets `fill` write it again, as one change: a
    /// reader sees the old index until the new one is whole.
    pub fn rebuild<T>(&self, fill: impl FnOnce() -> Result<T>) -> Result<T> {
        self.atomically(|| {
            self.conn.execute_batch(SCHEMA)?;
            let filled = fill()?;
            self.conn.pragma_update(None, "user_version", VERSION)?;
            Ok(filled)
        })
    }

    /// Records a folder, or its new role and UIDVALIDITY. A modseq counts
    /// from one UIDVALIDITY, so it is forgotten when that changes.
    pub fn put_folder(&self, account: &str, name: &str, role: Role, uid_validity: Option<u32>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO folders (account, name, role, uid_validity) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (account, name) DO UPDATE SET
                 role = excluded.role,
                 highest_modseq = CASE WHEN uid_validity IS excluded.uid_validity THEN highest_modseq END,
                 uid_validity = excluded.uid_validity",
            params![account, name, role_name(role), uid_validity],
        )?;
        Ok(())
    }

    pub fn set_modseq(&self, account: &str, folder: &str, modseq: u64) -> Result<()> {
        // SQLite's integers are signed; no server counts past their range.
        let modseq = i64::try_from(modseq).unwrap_or(i64::MAX);
        self.conn.execute(
            "UPDATE folders SET highest_modseq = ?3 WHERE account = ?1 AND name = ?2",
            params![account, folder, modseq],
        )?;
        Ok(())
    }

    pub fn position(&self, account: &str, folder: &str) -> Result<Position> {
        let found = self.conn.query_row(
            "SELECT uid_validity, highest_modseq,
                    (SELECT MAX(uid) FROM messages m WHERE m.account = f.account AND m.folder = f.name)
             FROM folders f WHERE account = ?1 AND name = ?2",
            params![account, folder],
            |row| {
                // `set_modseq` wrote it, so it is never negative.
                let highest_modseq = row.get::<_, Option<i64>>(1)?.and_then(|n| u64::try_from(n).ok());
                Ok(Position { uid_validity: row.get(0)?, highest_modseq, highest_uid: row.get(2)? })
            },
        );
        Ok(found.optional()?.unwrap_or_default())
    }

    /// The folder's messages that have a UID, lowest first.
    pub fn synced(&self, account: &str, folder: &str) -> Result<Vec<Synced>> {
        let mut query = self.conn.prepare(
            "SELECT key, uid, seen, flagged, answered, draft FROM messages
             WHERE account = ?1 AND folder = ?2 AND uid IS NOT NULL ORDER BY uid",
        )?;
        let synced = query.query_map(params![account, folder], |row| {
            let flags = Flags { seen: row.get(2)?, flagged: row.get(3)?, answered: row.get(4)?, draft: row.get(5)? };
            Ok(Synced { key: row.get(0)?, uid: row.get(1)?, flags })
        })?;
        Ok(synced.collect::<rusqlite::Result<_>>()?)
    }

    /// Every folder with its counts, by account and then name.
    pub fn folders(&self) -> Result<Vec<Folder>> {
        let mut query = self.conn.prepare(
            "SELECT f.account, f.name, f.role, COUNT(m.id), COALESCE(SUM(m.seen = 0), 0)
             FROM folders f LEFT JOIN messages m ON m.account = f.account AND m.folder = f.name
             GROUP BY f.account, f.name ORDER BY f.account, f.name",
        )?;
        let folders = query.query_map([], |row| {
            let name: String = row.get(1)?;
            Ok(Folder {
                id: FolderId(name.clone()),
                account: AccountId(row.get(0)?),
                name,
                role: role_named(&row.get::<_, String>(2)?),
                total: row.get(3)?,
                unseen: row.get(4)?,
            })
        })?;
        Ok(folders.collect::<rusqlite::Result<_>>()?)
    }

    /// Adds a message. `size` is the file's, and the rest of what the
    /// index knows is the file's name and what the caller's parse returned.
    pub fn insert(&self, account: &str, folder: &str, stored: &Stored, size: u32, parsed: &Parsed) -> Result<()> {
        let (sender, recipients, cc) = (pack(&parsed.from), pack(&parsed.to), pack(&parsed.cc));
        let (bcc, reply_to) = (pack(&parsed.bcc), pack(&parsed.reply_to));
        let refs = parsed.references.iter().map(|r| clean(r)).collect::<Vec<_>>().join(&ITEM.to_string());
        self.atomically(|| {
            self.conn.execute(
                "INSERT INTO messages (account, key, folder, file, uid, date, sender, recipients, cc, subject,
                     message_id, refs, seen, flagged, answered, draft, size, attachments, invite, unsubscribe,
                     security, origin, opening, files, bcc, reply_to)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                     ?21, ?22, ?23, ?24, ?25, ?26)",
                params![
                    account,
                    stored.key,
                    folder,
                    stored.file(),
                    stored.uid,
                    parsed.date.0,
                    sender,
                    recipients,
                    cc,
                    parsed.subject,
                    parsed.message_id,
                    refs,
                    stored.flags.seen,
                    stored.flags.flagged,
                    stored.flags.answered,
                    stored.flags.draft,
                    size,
                    parsed.has_attachments,
                    parsed.has_invite,
                    parsed.has_unsubscribe,
                    word(&SECURITY, parsed.security),
                    word(&ORIGIN, stored.origin),
                    opening(&parsed.text),
                    pack_files(&parsed.attachments),
                    bcc,
                    reply_to,
                ],
            )?;
            self.conn.execute(
                "INSERT INTO search (rowid, subject, sender, recipients, cc, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![self.conn.last_insert_rowid(), parsed.subject, sender, recipients, cc, parsed.text],
            )?;
            Ok(())
        })
    }

    pub fn locate(&self, id: &MessageId) -> Result<Option<Located>> {
        let found = self.conn.query_row(
            "SELECT folder, file FROM messages WHERE account = ?1 AND key = ?2",
            params![id.account.0, id.key],
            |row| Ok(Located { folder: row.get(0)?, file: row.get(1)? }),
        );
        Ok(found.optional()?)
    }

    /// The message as a query would return it, for a caller that already
    /// has its id.
    pub fn envelope(&self, id: &MessageId) -> Result<Option<Envelope>> {
        let sql = format!("SELECT {ENVELOPE} FROM messages m WHERE m.account = ?1 AND m.key = ?2");
        Ok(self.conn.prepare_cached(&sql)?.query_row(params![id.account.0, id.key], envelope).optional()?)
    }

    /// The messages as a list row shows each, in no order: every one, or
    /// with `only` the ones it names that are still here.
    pub fn listed(&self, only: Option<&[MessageId]>) -> Result<Vec<Listed>> {
        let columns = format!("{ENVELOPE}, m.invite, m.unsubscribe, m.opening, m.files");
        let listed = |row: &Row| {
            let opening: String = row.get(22)?;
            // A row shows one line: the first that is not quoted from the
            // message answered.
            let line = opening.lines().map(str::trim).find(|line| !line.is_empty() && !line.starts_with('>'));
            Ok(Listed {
                envelope: envelope(row)?,
                snippet: line.unwrap_or_default().to_string(),
                attachments: unpack_files(&row.get::<_, String>(23)?),
                invite: row.get(20)?,
                unsubscribe: row.get(21)?,
                text: Some(opening).filter(|text| !text.is_empty()),
                sender_was: None,
                sending: None,
            })
        };
        let Some(only) = only else {
            let mut all = self.conn.prepare_cached(&format!("SELECT {columns} FROM messages m"))?;
            return Ok(all.query_map([], listed)?.collect::<rusqlite::Result<_>>()?);
        };
        let sql = format!("SELECT {columns} FROM messages m WHERE m.account = ?1 AND m.key = ?2");
        let mut one = self.conn.prepare_cached(&sql)?;
        let mut found = Vec::new();
        for id in only {
            found.extend(one.query_row(params![id.account.0, id.key], listed).optional()?);
        }
        Ok(found)
    }

    /// The account's messages whose `Message-ID` is `message_id`.
    pub fn named(&self, account: &str, message_id: &str) -> Result<Vec<MessageId>> {
        let mut named = self.conn.prepare_cached("SELECT key FROM messages WHERE account = ?1 AND message_id = ?2")?;
        let id = |row: &Row| Ok(MessageId { account: AccountId(account.to_string()), key: row.get(0)? });
        Ok(named.query_map(params![account, message_id], id)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Follows a rename in the Maildir: everything a file's name and place
    /// say about a message is its folder, UID and flags.
    pub fn refile(&self, id: &MessageId, folder: &str, stored: &Stored) -> Result<()> {
        let Flags { seen, flagged, answered, draft } = stored.flags;
        self.conn.execute(
            "UPDATE messages SET folder = ?3, file = ?4, uid = ?5, seen = ?6, flagged = ?7, answered = ?8, draft = ?9
             WHERE account = ?1 AND key = ?2",
            params![id.account.0, id.key, folder, stored.file(), stored.uid, seen, flagged, answered, draft],
        )?;
        Ok(())
    }

    pub fn remove(&self, id: &MessageId) -> Result<()> {
        self.atomically(|| {
            let who = params![id.account.0, id.key];
            self.conn.execute(
                "DELETE FROM search WHERE rowid = (SELECT id FROM messages WHERE account = ?1 AND key = ?2)",
                who,
            )?;
            self.conn.execute("DELETE FROM messages WHERE account = ?1 AND key = ?2", who)?;
            Ok(())
        })
    }
}

fn envelope(row: &Row) -> rusqlite::Result<Envelope> {
    Ok(Envelope {
        id: MessageId { account: AccountId(row.get(0)?), key: row.get(1)? },
        folder: FolderId(row.get(2)?),
        date: Timestamp(row.get(3)?),
        from: unpack(&row.get::<_, String>(4)?),
        to: unpack(&row.get::<_, String>(5)?),
        cc: unpack(&row.get::<_, String>(6)?),
        bcc: unpack(&row.get::<_, String>(18)?),
        reply_to: unpack(&row.get::<_, String>(19)?),
        subject: row.get(7)?,
        message_id: row.get(8)?,
        references: row.get::<_, String>(9)?.split(ITEM).filter(|r| !r.is_empty()).map(String::from).collect(),
        flags: Flags { seen: row.get(10)?, flagged: row.get(11)?, answered: row.get(12)?, draft: row.get(13)? },
        size: row.get(14)?,
        has_attachments: row.get(15)?,
        security: meant(&SECURITY, &row.get::<_, String>(16)?),
        origin: meant(&ORIGIN, &row.get::<_, String>(17)?),
    })
}

#[cfg(test)]
mod tests;

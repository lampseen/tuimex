//! One Maildir++ tree per account under the data directory (ADR-013): the
//! account's directory is its INBOX, and every other folder is a directory
//! beside `cur` whose name starts with a dot. Everything sync needs to
//! resume is here and not in the index: the UID in each file name, and each
//! folder's UIDVALIDITY and role in two small files.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tuimex_core::{Flags, Origin, Role};

use crate::{At, Error, Result, files};

const UID_VALIDITY: &str = "tuimex-uidvalidity";
const ROLE: &str = "tuimex-role";
const NAMESPACE: &str = "tuimex-namespace";
const ROLES: [(Role, &str); 8] = [
    (Role::Inbox, "inbox"),
    (Role::Sent, "sent"),
    (Role::Drafts, "drafts"),
    (Role::Archive, "archive"),
    (Role::Trash, "trash"),
    (Role::Junk, "junk"),
    (Role::Outbox, "outbox"),
    (Role::Other, "other"),
];

/// The word a role is written as, in the role file and in the index.
pub fn role_name(role: Role) -> &'static str {
    ROLES.iter().find(|(r, _)| *r == role).map_or("other", |(_, name)| name)
}

pub(crate) fn role_named(name: &str) -> Role {
    ROLES.iter().find(|(_, n)| *n == name).map_or(Role::Other, |(role, _)| *role)
}

pub struct Maildir {
    root: PathBuf,
}

/// What a new message is filed as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub key: String,
    pub uid: Option<u32>,
    pub origin: Origin,
    pub flags: Flags,
}

/// One message file, as its name and place describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// The part of the name no flag, UID or move changes.
    pub key: String,
    pub uid: Option<u32>,
    pub origin: Origin,
    pub flags: Flags,
    /// The file is the message's header block and nothing after it: the
    /// body is still on the server (ADR-028).
    pub headers_only: bool,
    /// Name fields and flag letters tuimex gives no meaning to. Another
    /// tool put them there, so every rename carries them along.
    fields: Vec<String>,
    letters: String,
    folder: PathBuf,
    /// `cur`, or `new` for a delivery by another tool that nothing has
    /// looked at yet.
    sub: &'static str,
    /// The name on disk, which is `name()` only for a file tuimex named.
    file: String,
}

impl Stored {
    /// The name tuimex gives this message when it next names its file:
    /// `key[,U=uid][,O=a|l][,H=1][,other fields]:2,flags`. The UID is where
    /// mbsync puts it. `O` says who wrote a message that did not come from
    /// the server, so an agent's draft is still marked as one after the
    /// index is rebuilt (ADR-016). `H` marks a header block whose body has
    /// not been fetched (ADR-028).
    pub fn name(&self) -> String {
        let mut name = self.key.clone();
        if let Some(uid) = self.uid {
            name += &format!(",U={uid}");
        }
        name += match self.origin {
            Origin::Server => "",
            Origin::Local => ",O=l",
            Origin::Agent => ",O=a",
        };
        if self.headers_only {
            name += ",H=1";
        }
        for field in &self.fields {
            name += ",";
            name += field;
        }
        let set =
            [(self.flags.draft, 'D'), (self.flags.flagged, 'F'), (self.flags.answered, 'R'), (self.flags.seen, 'S')];
        let mut letters: Vec<char> =
            self.letters.chars().chain(set.iter().filter(|(on, _)| *on).map(|(_, c)| *c)).collect();
        // The Maildir specification asks for flag letters in ASCII order.
        letters.sort_unstable();
        name + ":2," + &letters.into_iter().collect::<String>()
    }

    /// The file's name as it is on disk.
    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn path(&self) -> PathBuf {
        self.folder.join(self.sub).join(&self.file)
    }

    /// None for a name that is no message's, such as a dot file, and for
    /// one that is more than a name: it may be a name the index remembered,
    /// and the index is not trusted to stay inside the folder.
    fn from_name(name: &str, folder: &Path, sub: &'static str) -> Option<Stored> {
        if name.contains(['/', '\0']) {
            return None;
        }
        let (base, info) = name.split_once(':').unwrap_or((name, ""));
        let mut parts = base.split(',');
        let key = parts.next().filter(|key| valid_key(key))?;
        let letters = info.strip_prefix("2,").unwrap_or("");
        let mut stored = Stored {
            key: key.to_string(),
            uid: None,
            origin: Origin::Server,
            headers_only: false,
            flags: Flags {
                draft: letters.contains('D'),
                flagged: letters.contains('F'),
                answered: letters.contains('R'),
                seen: letters.contains('S'),
            },
            fields: Vec::new(),
            letters: letters.chars().filter(|c| !"DFRS".contains(*c)).collect(),
            folder: folder.to_path_buf(),
            sub,
            file: name.to_string(),
        };
        for field in parts {
            match (field.strip_prefix("U=").and_then(|uid| uid.parse().ok()), field) {
                (Some(uid), _) => stored.uid = Some(uid),
                (None, "O=l") => stored.origin = Origin::Local,
                (None, "O=a") => stored.origin = Origin::Agent,
                (None, "H=1") => stored.headers_only = true,
                (None, _) => stored.fields.push(field.to_string()),
            }
        }
        Some(stored)
    }
}

/// A key is one path component with none of the characters the rest of a
/// Maildir name is built from. It arrives from the command line as well as
/// from here, so it is checked wherever it is about to become a path.
fn valid_key(key: &str) -> bool {
    !key.is_empty() && !key.starts_with('.') && !key.contains(['/', ',', ':', '\0'])
}

/// A key no other delivery on this machine has: the time, the process, and
/// a count within the process. No host name, which a Maildir name usually
/// carries: nothing here needs to know it.
pub fn new_key() -> String {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    // A clock set before 1970 gives zero, and the process and count still
    // tell two keys apart.
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let (pid, n) = (std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed));
    format!("{}.M{}P{pid}Q{n}.tuimex", now.as_secs(), now.subsec_micros())
}

/// The directory a folder lives in, relative to its account: nothing for
/// the INBOX, otherwise a dot and the name with `/` as Maildir++'s `.`.
/// A dot, a `%` or a control character in the name is written `%XX`, so
/// no folder name can leave the account's directory and each has one
/// directory name that reads back to it.
fn folder_dir_name(folder: &str) -> Result<Option<String>> {
    if folder.eq_ignore_ascii_case("INBOX") {
        return Ok(None);
    }
    if folder.split('/').any(str::is_empty) {
        return Err(Error::Name(folder.to_string()));
    }
    let mut dir = String::from(".");
    let mut utf8 = [0; 4];
    for c in folder.chars() {
        match c {
            '/' => dir.push('.'),
            '.' | '%' => dir += &format!("%{:02X}", c as u32),
            c if c.is_control() => c.encode_utf8(&mut utf8).bytes().for_each(|b| dir += &format!("%{b:02X}")),
            c => dir.push(c),
        }
    }
    Ok(Some(dir))
}

/// The folder a directory under an account holds, or None for a directory
/// `folder_dir_name` could not have made.
fn folder_name(dir: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut rest = dir.strip_prefix('.')?.bytes();
    while let Some(b) = rest.next() {
        bytes.push(match b {
            b'.' => b'/',
            b'%' => {
                let hex = [rest.next()?, rest.next()?];
                u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?
            }
            b => b,
        });
    }
    let name = String::from_utf8(bytes).ok()?;
    (folder_dir_name(&name).ok()?.as_deref() == Some(dir)).then_some(name)
}

impl Maildir {
    /// `root` is the data directory; nothing is made until a folder is.
    pub fn new(root: &Path) -> Maildir {
        Maildir { root: root.to_path_buf() }
    }

    /// An account's directory, which also holds its journal.
    pub fn account_dir(&self, account: &str) -> Result<PathBuf> {
        if account.is_empty() || account.starts_with('.') || account.contains(['/', '\0']) {
            return Err(Error::Name(account.to_string()));
        }
        Ok(self.root.join(account))
    }

    fn folder_dir(&self, account: &str, folder: &str) -> Result<PathBuf> {
        let account = self.account_dir(account)?;
        Ok(folder_dir_name(folder)?.map_or(account.clone(), |dir| account.join(dir)))
    }

    /// Makes the folder if it is not there, and records its role.
    pub fn ensure_folder(&self, account: &str, folder: &str, role: Role) -> Result<()> {
        let dir = self.folder_dir(account, folder)?;
        for sub in ["cur", "new", "tmp"] {
            files::private_dir(&dir.join(sub)).at(&dir)?;
        }
        if self.role(account, folder)? != role {
            files::replace(&dir.join(ROLE), role_name(role).as_bytes()).at(&dir)?;
        }
        Ok(())
    }

    fn note(&self, account: &str, folder: &str, file: &str) -> Result<Option<String>> {
        let path = self.folder_dir(account, folder)?.join(file);
        match fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text.trim().to_string())),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).at(&path),
        }
    }

    pub fn role(&self, account: &str, folder: &str) -> Result<Role> {
        Ok(self.note(account, folder, ROLE)?.map_or(Role::Other, |name| role_named(&name)))
    }

    /// None until a sync has recorded one, and for a file that holds
    /// anything but a number: both mean the folder's UIDs cannot be trusted.
    pub fn uid_validity(&self, account: &str, folder: &str) -> Result<Option<u32>> {
        Ok(self.note(account, folder, UID_VALIDITY)?.and_then(|text| text.parse().ok()))
    }

    pub fn set_uid_validity(&self, account: &str, folder: &str, uid_validity: u32) -> Result<()> {
        let path = self.folder_dir(account, folder)?.join(UID_VALIDITY);
        files::replace(&path, uid_validity.to_string().as_bytes()).at(&path)
    }

    /// What the server's names for a person's own folders begin with, as
    /// last recorded: nothing until a listing has been, and on most
    /// servers after it. Kept with the mail, so that a folder can be
    /// named for its server while there is no connection to ask.
    pub fn namespace(&self, account: &str) -> Result<String> {
        let path = self.account_dir(account)?.join(NAMESPACE);
        match fs::read_to_string(&path) {
            Ok(prefix) => Ok(prefix),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e).at(&path),
        }
    }

    pub fn set_namespace(&self, account: &str, prefix: &str) -> Result<()> {
        if self.namespace(account)? == prefix {
            return Ok(());
        }
        let dir = self.account_dir(account)?;
        files::private_dir(&dir).at(&dir)?;
        files::replace(&dir.join(NAMESPACE), prefix.as_bytes()).at(&dir)
    }

    /// The names of the directories in `dir`, sorted, or nothing when `dir`
    /// does not exist.
    fn dirs_in(dir: &Path) -> Result<Vec<String>> {
        let entries = match fs::read_dir(dir) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            other => other.at(dir)?,
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.at(dir)?;
            // A name that is not UTF-8 is not one tuimex gave anything.
            if let (true, Ok(name)) = (entry.file_type().at(dir)?.is_dir(), entry.file_name().into_string()) {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    /// Every account on disk, sorted.
    pub fn accounts(&self) -> Result<Vec<String>> {
        let mut names = Self::dirs_in(&self.root)?;
        names.retain(|name| self.account_dir(name).is_ok());
        Ok(names)
    }

    /// Every folder of an account, the INBOX first.
    pub fn folders(&self, account: &str) -> Result<Vec<String>> {
        let dir = self.account_dir(account)?;
        let inbox = dir.join("cur").is_dir().then(|| "INBOX".to_string());
        let others = Self::dirs_in(&dir)?.into_iter().filter(|d| dir.join(d).join("cur").is_dir());
        Ok(inbox.into_iter().chain(others.filter_map(|d| folder_name(&d))).collect())
    }

    /// Files the message under `cur`, by way of `tmp` so that no reader
    /// ever sees part of it. Fails if the name is taken.
    pub fn deliver(&self, account: &str, folder: &str, delivery: &Delivery, raw: &[u8]) -> Result<Stored> {
        self.file(account, folder, delivery, raw, false)
    }

    /// Files a header block for a message whose body stays on the server
    /// until `fill` is given it.
    pub fn deliver_head(&self, account: &str, folder: &str, delivery: &Delivery, head: &[u8]) -> Result<Stored> {
        self.file(account, folder, delivery, head, true)
    }

    fn file(&self, account: &str, folder: &str, delivery: &Delivery, raw: &[u8], headers_only: bool) -> Result<Stored> {
        if !valid_key(&delivery.key) {
            return Err(Error::Name(delivery.key.clone()));
        }
        let Delivery { key, uid, origin, flags } = delivery.clone();
        let folder = self.folder_dir(account, folder)?;
        let (fields, letters, sub, file) = (Vec::new(), String::new(), "cur", String::new());
        let mut stored = Stored { key, uid, origin, flags, headers_only, fields, letters, folder, sub, file };
        stored.file = stored.name();
        let tmp = stored.folder.join("tmp").join(&stored.file);
        let written = files::create(&tmp)
            .and_then(|mut file| file.write_all(raw).and_then(|()| file.sync_all()))
            .and_then(|()| files::place(&tmp, &stored.path()));
        if written.is_err() {
            // The delivery's own error is the one to report. A file this
            // leaves in `tmp` is one no reader looks at.
            let _ = fs::remove_file(&tmp);
        }
        written.at(&stored.path())?;
        Ok(stored)
    }

    /// Gives a header block its whole message. The content changes before
    /// the name does, so a crash between the two leaves a whole message
    /// still marked as a header block, which costs one more fetch.
    pub fn fill(&self, message: &Stored, raw: &[u8]) -> Result<Stored> {
        let path = message.path();
        files::replace_through(&message.folder.join("tmp").join(&message.file), &path, raw).at(&path)?;
        self.rename(message, Stored { headers_only: false, ..message.clone() })
    }

    /// Every message in the folder, sorted by key. `tmp` is not read.
    pub fn list(&self, account: &str, folder: &str) -> Result<Vec<Stored>> {
        let folder = self.folder_dir(account, folder)?;
        let mut found = Vec::new();
        for sub in ["cur", "new"] {
            let dir = folder.join(sub);
            for entry in fs::read_dir(&dir).at(&dir)? {
                let entry = entry.at(&dir)?;
                // Only a regular file is a message: a link would be read
                // through to wherever it points. A name that is not UTF-8
                // is not one a Maildir writer makes.
                if let (true, Ok(name)) = (entry.file_type().at(&dir)?.is_file(), entry.file_name().into_string()) {
                    found.extend(Stored::from_name(&name, &folder, sub));
                }
            }
        }
        found.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(found)
    }

    /// The message with this key. `file` is the name it had in `cur` when
    /// last seen, which saves reading the directory when it still has it.
    pub fn find(&self, account: &str, folder: &str, key: &str, file: Option<&str>) -> Result<Option<Stored>> {
        let dir = self.folder_dir(account, folder)?;
        let known = file.and_then(|file| Stored::from_name(file, &dir, "cur")).filter(|s| s.key == key);
        // A name that cannot be looked at is treated as a name that has
        // changed: reading the directory finds the message or reports why not.
        if let Some(stored) = known.filter(|s| fs::symlink_metadata(s.path()).is_ok_and(|m| m.is_file())) {
            return Ok(Some(stored));
        }
        Ok(self.list(account, folder)?.into_iter().find(|s| s.key == key))
    }

    pub fn read(&self, message: &Stored) -> Result<Vec<u8>> {
        let path = message.path();
        fs::read(&path).at(&path)
    }

    pub fn remove(&self, message: &Stored) -> Result<()> {
        let path = message.path();
        fs::remove_file(&path).at(&path)
    }

    /// Every rename ends in `cur` under a name tuimex writes: `new` is for
    /// deliveries nothing has looked at, and its names carry no flags.
    fn rename(&self, from: &Stored, mut to: Stored) -> Result<Stored> {
        (to.sub, to.file) = ("cur", to.name());
        let (old, new) = (from.path(), to.path());
        if old != new {
            files::place(&old, &new).at(&new)?;
        }
        Ok(to)
    }

    /// A flag lives in the file name, so changing one renames the file.
    pub fn set_flags(&self, message: &Stored, flags: Flags) -> Result<Stored> {
        self.rename(message, Stored { flags, ..message.clone() })
    }

    /// Records the UID the server gave a message that was filed here first.
    pub fn set_uid(&self, message: &Stored, uid: u32) -> Result<Stored> {
        self.rename(message, Stored { uid: Some(uid), ..message.clone() })
    }

    /// Files the message in another folder of the same account. Its UID
    /// stays behind: a UID belongs to the folder that issued it.
    pub fn move_to(&self, message: &Stored, account: &str, folder: &str) -> Result<Stored> {
        let folder = self.folder_dir(account, folder)?;
        self.rename(message, Stored { uid: None, folder, ..message.clone() })
    }
}

#[cfg(test)]
mod tests;

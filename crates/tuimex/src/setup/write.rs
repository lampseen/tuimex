//! The one writer of the config file. `tuimex setup` adds to it, and a
//! rename changes an account's name where it stands (ADR-050): either
//! way what the user had is kept byte for byte, comments included. The
//! result is read back before it replaces the file, and no key written
//! here can hold a secret (ADR-010).

use std::path::Path;

use anyhow::{Context, Result, bail};
use tuimex_engine::Server;
use tuimex_store::files;

use crate::config::Config;
use crate::net::Auth;

/// An account as setup has it, ready to be written.
pub struct Entry {
    pub name: String,
    pub address: String,
    pub imap: Server,
    pub smtp: Server,
    pub auth: Auth,
    pub password_cmd: Vec<String>,
    /// The roles the mapping could not fill, by their config key.
    pub folders: Vec<(&'static str, String)>,
    /// What an agent may do: read, and write drafts.
    pub agent: (bool, bool),
}

/// A TOML string, quoted and escaped by the library that will read it.
fn quoted(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

fn server(server: &Server, address: &str, implicit: u16) -> String {
    let mut keys = format!("host = {}, port = {}", quoted(&server.host), server.port);
    // Only where it differs from what the port alone would mean.
    if server.starttls != (server.port != implicit) {
        keys += &format!(", starttls = {}", server.starttls);
    }
    if server.user != address {
        keys += &format!(", user = {}", quoted(&server.user));
    }
    format!("{{ {keys} }}")
}

/// The `[[account]]` table for `entry`.
pub fn account(entry: &Entry) -> String {
    let mut text = format!(
        "[[account]]\nname = {}\naddress = {}\nimap = {}\nsmtp = {}\n",
        quoted(&entry.name),
        quoted(&entry.address),
        server(&entry.imap, &entry.address, 993),
        server(&entry.smtp, &entry.address, 465),
    );
    text += match entry.auth {
        Auth::Password => "auth = \"password\"\n",
        Auth::Command => "auth = \"command\"\n",
        Auth::Gmail => "auth = \"gmail-oauth\"\n",
    };
    if entry.auth == Auth::Command {
        let words: Vec<String> = entry.password_cmd.iter().map(|word| quoted(word)).collect();
        text += &format!("password_cmd = [{}]\n", words.join(", "));
    }
    if !entry.folders.is_empty() {
        let roles: Vec<String> =
            entry.folders.iter().map(|(role, name)| format!("{role} = {}", quoted(name))).collect();
        text += &format!("folders = {{ {} }}\n", roles.join(", "));
    }
    let (read, draft) = entry.agent;
    if read || draft {
        text += &format!("\n[account.agent]\nread = {read}\nfolders = [\"INBOX\"]\ndraft = {draft}\n");
    }
    text
}

/// The `[[calendar]]` table of `account`: read over CalDAV at `url`, and
/// with `write` written there too; or with no address, a feed whose
/// address is in the keyring.
pub fn calendar(account: &str, url: Option<&str>, write: bool) -> String {
    let mut text = format!("[[calendar]]\naccount = {}\n", quoted(account));
    match url {
        Some(url) => text += &format!("kind = \"caldav\"\nurl = {}\n", quoted(url)),
        None => text += "kind = \"feed\"\n",
    }
    if write {
        text += "write = true\n";
    }
    text
}

/// The table that says `account`'s calendar, which is already in the
/// file, is written at its provider. A key cannot be added to a table
/// that is not the last without rewriting the file (ADR-043), so it is a
/// table of its own that adds to that one.
pub fn written(account: &str) -> String {
    format!("[[calendar]]\naccount = {}\nwrite = true\n", quoted(account))
}

/// `existing` with `tables` after it. An error when the result would not
/// be the same config with the tables added, which is the case for a file
/// laid out so that nothing can follow it (accounts written as one inline
/// array, say): it is then the user's to edit, and the error has the text.
pub fn appended(existing: &str, tables: &[String]) -> Result<String> {
    let mut text = existing.to_string();
    for table in tables {
        if !text.is_empty() {
            text += if text.ends_with('\n') { "\n" } else { "\n\n" };
        }
        text += table;
    }
    let by_hand = || format!("nothing was written; add this to the config by hand:\n\n{}", tables.join("\n"));
    let read = |text: &str| text.parse::<toml::Table>().ok();
    let (Some(before), Some(after)) = (read(existing), read(&text)) else { bail!(by_hand()) };
    let kept = before.iter().all(|(key, value)| match (value, after.get(key)) {
        (toml::Value::Array(old), Some(toml::Value::Array(new))) if key == "account" || key == "calendar" => {
            new.starts_with(old)
        }
        (old, new) => Some(old) == new,
    });
    if !kept || Config::parse(&text).is_err() {
        bail!(by_hand());
    }
    Ok(text)
}

/// One line of the config as a table of its own, when it is one key.
fn one_key(line: &str) -> Option<(String, toml::Value)> {
    line.parse::<toml::Table>().ok()?.into_iter().next()
}

/// `existing` with the account `old` named `new`: its `name` line, its
/// `[[calendar]]`'s `account` line, and each `[[section]]` query that has
/// the term `account:<old>`. Only the
/// value changes on those lines. An error, with nothing to write, when
/// the result read back is not the same config with exactly that
/// changed: a file laid out so that the name is not on a line of its own
/// is the user's to edit.
pub fn renamed(existing: &str, old: &str, new: &str) -> Result<String> {
    let term = |query: &str| {
        let words = query
            .split(' ')
            .map(|word| if word == format!("account:{old}") { format!("account:{new}") } else { word.to_string() });
        words.collect::<Vec<_>>().join(" ")
    };
    let (mut table, mut text) = (String::new(), String::new());
    for line in existing.split_inclusive('\n') {
        if line.trim_start().starts_with('[') {
            table = line.split('#').next().unwrap_or_default().trim().to_string();
        }
        let wanted = match (table.as_str(), one_key(line)) {
            ("[[account]]", Some((key, toml::Value::String(name)))) if key == "name" && name == old => {
                Some((name, new.to_string()))
            }
            ("[[calendar]]", Some((key, toml::Value::String(name)))) if key == "account" && name == old => {
                Some((name, new.to_string()))
            }
            ("[[section]]", Some((key, toml::Value::String(query)))) if key == "query" && term(&query) != query => {
                let now = term(&query);
                Some((query, now))
            }
            _ => None,
        };
        // The value as it is most often written: in quotes, with nothing
        // escaped. Written any other way it is left, and the check below
        // then fails.
        let written = wanted.and_then(|(was, now)| {
            let literal = [format!("\"{was}\""), format!("'{was}'")].into_iter().find(|l| line.contains(l.as_str()))?;
            Some(line.replacen(&literal, &quoted(&now), 1))
        });
        text += written.as_deref().unwrap_or(line);
    }
    let by_hand =
        || format!("nothing was written; in the config, change `name = \"{old}\"` to `name = \"{new}\"` by hand");
    let read = |text: &str| text.parse::<toml::Table>().ok();
    let (Some(mut expected), Some(after)) = (read(existing), read(&text)) else { bail!(by_hand()) };
    let tables = |key: &str, table: &toml::Table| -> Vec<toml::Value> {
        table.get(key).and_then(toml::Value::as_array).cloned().unwrap_or_default()
    };
    let mut accounts = tables("account", &expected);
    let is_old = |account: &toml::Value| account.get("name").and_then(toml::Value::as_str) == Some(old);
    if accounts.iter().filter(|account| is_old(account)).count() != 1 {
        bail!(by_hand());
    }
    for name in accounts.iter_mut().filter(|account| is_old(account)).filter_map(|account| account.get_mut("name")) {
        *name = toml::Value::String(new.to_string());
    }
    let mut sections = tables("section", &expected);
    for query in sections.iter_mut().filter_map(|section| section.get_mut("query")) {
        if let Some(now) = query.as_str().map(term) {
            *query = toml::Value::String(now);
        }
    }
    let mut calendars = tables("calendar", &expected);
    for name in calendars.iter_mut().filter_map(|calendar| calendar.get_mut("account")) {
        if name.as_str() == Some(old) {
            *name = toml::Value::String(new.to_string());
        }
    }
    expected.insert("account".to_string(), toml::Value::Array(accounts));
    if !sections.is_empty() {
        expected.insert("section".to_string(), toml::Value::Array(sections));
    }
    if !calendars.is_empty() {
        expected.insert("calendar".to_string(), toml::Value::Array(calendars));
    }
    if expected != after || Config::parse(&text).is_err() {
        bail!(by_hand());
    }
    Ok(text)
}

/// Puts `text` at `path` whole and owner-only (ADR-011). A config that is
/// a link, as a dotfiles manager makes, is written where it points.
pub fn save(path: &Path, text: &str, config_dir: &Path) -> Result<()> {
    if path.parent() == Some(config_dir) {
        files::private_dir(config_dir).with_context(|| format!("creating {}", config_dir.display()))?;
    }
    // A path that is not there yet is its own target.
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    files::replace(&target, text.as_bytes()).with_context(|| format!("writing {}", target.display()))
}

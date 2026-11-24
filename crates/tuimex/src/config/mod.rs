//! The config file (PLAN §11): what the user said, read whole or not at
//! all. A file that does not parse stops tuimex with the reason; it never
//! falls back to defaults.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tuimex_core::{AccountId, Grants};
use tuimex_ui::layout;
use tuimex_ui::list::Preview;
use tuimex_ui::palette::Tok;

/// `--config`, or `config.toml` in the config directory. The current
/// directory is never searched (ADR-011).
pub fn path(flag: Option<&Path>, config_dir: &Path) -> PathBuf {
    flag.map_or_else(|| config_dir.join("config.toml"), Path::to_path_buf)
}

/// Every table is typed, including the ones no code reads yet, so that a
/// key the plan does not name is an error today and not on the day its
/// table is first used.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(dead_code, reason = "tables held so a file using them is not refused; each is read from its own milestone on")]
pub struct Config {
    #[serde(default, rename = "account")]
    pub accounts: Vec<Account>,
    pub sync: Option<Sync>,
    pub send: Option<Send>,
    pub ui: Option<Ui>,
    notify: Option<unread::Notify>,
    pgp: Option<unread::Pgp>,
    pub paths: Option<Paths>,
    keys: Option<BTreeMap<String, String>>,
    pub section: Option<Vec<Section>>,
    pub agent: Option<AgentCommand>,
}

/// No key here can hold a secret (ADR-010).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub name: String,
    pub address: String,
    agent: Option<Agent>,
    pub identities: Option<Vec<String>>,
    pub imap: Option<Server>,
    pub smtp: Option<Server>,
    pub auth: Option<String>,
    pub password_cmd: Option<Vec<String>>,
    pub folders: Option<BTreeMap<String, String>>,
    /// The accent this account's name wears in the list, in place of its
    /// turn in the palette's order.
    pub color: Option<String>,
}

/// `[agent]`: the command `:agent` hands text to (ADR-031). Without the
/// table there is no such command. It names a built-in agent or gives a
/// command, never both.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCommand {
    /// A built-in agent, whose command tuimex supplies: `claude`.
    #[serde(rename = "use")]
    pub preset: Option<String>,
    /// Any other program and its arguments; no shell reads them.
    pub command: Option<Vec<String>>,
    /// Such as `90s` or `2m`.
    pub timeout: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub host: String,
    pub port: u16,
    /// Whether TLS begins with STARTTLS on a plain connection or with the
    /// first byte. Left out, ports 993 and 465 are the second and any
    /// other port the first. Neither value means "no TLS" (ADR-012).
    pub starttls: Option<bool>,
    /// The name to sign in with, where it is not the account's address.
    pub user: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sync {
    pub bodies: Option<String>,
    pub max_body: Option<String>,
    pub folders: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Send {
    pub delay: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Agent {
    #[serde(default)]
    read: bool,
    folders: Option<Vec<String>>,
    #[serde(default)]
    draft: bool,
}

impl Account {
    /// An account with no `[account.agent]` grants nothing (ADR-016).
    pub fn grants(&self) -> Grants {
        self.agent.as_ref().map_or_else(Grants::default, |agent| Grants {
            read: agent.read,
            folders: agent.folders.clone().unwrap_or_else(|| vec!["INBOX".to_string()]),
            draft: agent.draft,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(dead_code, reason = "keys held so a file using them is not refused")]
pub struct Ui {
    glyphs: Option<String>,
    colors: Option<String>,
    /// A palette file to draw with in place of the built-in one (ADR-003).
    pub palette: Option<String>,
    /// Where the preview sits: `auto`, `right`, `below` or `off`.
    pub preview: Option<String>,
}

/// `ui.preview`, which left out is `auto`. Any other word stops tuimex
/// with the four it takes.
pub fn preview(ui: Option<&Ui>) -> Result<Preview> {
    match ui.and_then(|ui| ui.preview.as_deref()) {
        None => Ok(Preview::Auto),
        Some(named) => Preview::from_name(named)
            .with_context(|| format!("ui.preview = {named:?}: it is one of auto, right, below, off")),
    }
}

/// `[[account]] color`, for each account that has one. Only an accent
/// accounts take in turn is lent: the others each say something else in a
/// row.
pub fn colours(accounts: &[Account]) -> Result<Vec<(AccountId, Tok)>> {
    let lent = [&layout::ACCOUNTS_BUILTIN[..], &layout::ACCOUNTS_POOL, &layout::ACCOUNTS_LAST].concat();
    let mut fixed = Vec::new();
    for account in accounts {
        let Some(named) = &account.color else { continue };
        let names: Vec<&str> = layout::ACCOUNTS_BUILTIN.iter().map(|t| t.name()).collect();
        let token = Tok::from_name(named).filter(|token| lent.contains(token)).with_context(|| {
            format!("account {}: color = {named:?}: it is one of {}", account.name, names.join(", "))
        })?;
        fixed.push((AccountId(account.name.clone()), token));
    }
    Ok(fixed)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    /// Where `:save` puts a file given no path. `~/` is the home directory.
    pub downloads: Option<String>,
}

/// A tab: a saved query (PLAN §6.1).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub name: String,
    pub short: String,
    pub query: String,
    pub threaded: Option<bool>,
}

mod unread {
    #![expect(dead_code, reason = "the shapes of tables no code reads yet")]

    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Notify {
        bell: Option<bool>,
        osc9: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Pgp {
        sign: Option<String>,
        key: Option<String>,
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading the config {}", path.display()))?;
        Config::parse(&text).with_context(|| format!("the config {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Config> {
        let config: Config = toml::from_str(text).map_err(|e| {
            let line = e.span().map_or(0, |span| text[..span.start.min(text.len())].matches('\n').count() + 1);
            anyhow::anyhow!("line {line}: {}", without_values(e.message()))
        })?;
        for (i, account) in config.accounts.iter().enumerate() {
            if config.accounts[..i].iter().any(|earlier| earlier.name == account.name) {
                bail!("two accounts are named {}", account.name);
            }
        }
        Ok(config)
    }

    pub fn account(&self, name: &str) -> Option<&Account> {
        self.accounts.iter().find(|account| account.name == name)
    }
}

/// What went wrong, with nothing the file held as a value. The parser's own
/// message quotes the value it choked on, and a password typed under the
/// wrong key would go from there to the terminal and the audit log
/// (ADR-010).
fn without_values(message: &str) -> String {
    if let Some(key) = message.strip_prefix("unknown field `").and_then(|rest| rest.split('`').next()) {
        format!("unknown key `{key}`")
    } else if message.starts_with("missing field `") {
        message.replace("field", "key")
    } else if let Some((_, expected)) = message.split_once(", expected ") {
        format!("expected {expected}")
    } else {
        "this is not TOML tuimex can read".to_string()
    }
}

#[cfg(test)]
mod tests;

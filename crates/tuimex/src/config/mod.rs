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
    pub notify: Option<Notify>,
    pgp: Option<unread::Pgp>,
    pub paths: Option<Paths>,
    keys: Option<BTreeMap<String, String>>,
    pub section: Option<Vec<Section>>,
    pub agent: Option<AgentCommand>,
    #[serde(default, rename = "calendar")]
    pub calendars: Vec<Calendar>,
}

/// `[[calendar]]`: one account's calendar (ADR-056, ADR-062). A table of
/// its own and not a key of the account's, since setup only appends and
/// an account's sub-table cannot follow a later account (ADR-043).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    pub account: String,
    /// `caldav` or `feed`. A table without it adds to the account's
    /// table above: setup can only append (ADR-043), so that is how it
    /// records what it found out later.
    pub kind: Option<String>,
    /// The CalDAV calendar's address. A feed has none here: its address
    /// is a secret (ADR-010).
    pub url: Option<String>,
    /// A feed's: argv, no shell, whose first line printed is the address.
    pub address_cmd: Option<Vec<String>>,
    /// Whether events are written at the provider (ADR-062). Left out it
    /// is false, and every write for the account goes to its local
    /// calendar: a calendar is not written on a path nobody has tried.
    pub write: Option<bool>,
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
    /// Whether the terminal reports the pointer to tuimex (ADR-065).
    pub mouse: Option<bool>,
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

/// `ui.mouse`, which left out is off: a terminal that reports the pointer
/// no longer selects text with it, and that is the user's to give up.
pub fn mouse(ui: Option<&Ui>) -> bool {
    ui.and_then(|ui| ui.mouse).unwrap_or(false)
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

/// `[notify]`. `bell` and `osc9` are for new mail (PLAN §6.6), which is
/// not built; the other three are an event's reminder (ADR-058).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notify {
    #[expect(dead_code, reason = "new mail does not notify yet")]
    bell: Option<bool>,
    #[expect(dead_code, reason = "new mail does not notify yet")]
    osc9: Option<bool>,
    /// Whether a first run starts with reminders on; the switch then
    /// remembers.
    pub events: Option<bool>,
    /// Such as `10m`: how long before its start an event with no alarm of
    /// its own is reminded.
    pub lead: Option<String>,
    /// `desktop`, `osc9` or `none`.
    pub via: Option<String>,
}

mod unread {
    #![expect(dead_code, reason = "the shapes of tables no code reads yet")]

    use serde::Deserialize;

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
        for (i, calendar) in config.calendars.iter().enumerate() {
            calendar.check(&config)?;
            let earlier = config.calendars[..i].iter().find(|earlier| earlier.account == calendar.account);
            let name = &calendar.account;
            match (earlier, &calendar.kind) {
                (Some(_), Some(_)) => {
                    bail!("account {name} has two [[calendar]] tables, and an account has one calendar")
                }
                (Some(first), None) if first.url.is_none() => {
                    bail!("[[calendar]] of {name}: `write` is for kind = \"caldav\"; a feed is only ever read")
                }
                (None, None) => bail!("[[calendar]] of {name}: it has no `kind`, and no table above it to add to"),
                _ => {}
            }
        }
        crate::notify::settings(config.notify.as_ref())?;
        Ok(config)
    }

    /// The `[[calendar]]` of the account `name`.
    pub fn calendar(&self, name: &str) -> Option<&Calendar> {
        self.calendars.iter().find(|calendar| calendar.account == name)
    }

    /// Whether the account's calendar is written at its provider: what the
    /// last of its tables to say `write` says.
    pub fn writes(&self, name: &str) -> bool {
        let mut said = self.calendars.iter().filter(|calendar| calendar.account == name).filter_map(|c| c.write);
        said.next_back().unwrap_or(false)
    }

    /// Whether the account's calendar is one setup could find out how to
    /// write: read over CalDAV, and not written. A feed never is.
    pub fn unwritten(&self, name: &str) -> bool {
        self.calendar(name).is_some_and(|calendar| calendar.url.is_some()) && !self.writes(name)
    }

    pub fn account(&self, name: &str) -> Option<&Account> {
        self.accounts.iter().find(|account| account.name == name)
    }
}

impl Calendar {
    /// Whether the table says one way to read one account's calendar.
    /// No error quotes `url`: on a table that should have had none, it is
    /// the feed's secret address.
    fn check(&self, config: &Config) -> Result<()> {
        let name = &self.account;
        let Some(account) = config.account(name) else {
            bail!("[[calendar]] account = {name:?}: there is no [[account]] of that name");
        };
        let at = format!("[[calendar]] of {name}");
        let Some(kind) = self.kind.as_deref() else {
            if self.write.is_none() || self.url.is_some() || self.address_cmd.is_some() {
                bail!("{at}: a table with no `kind` adds to the one above it, and holds `write` alone");
            }
            return Ok(());
        };
        match (kind, &self.url) {
            ("caldav", None) => bail!("{at}: kind = \"caldav\" needs a `url`, the calendar's address"),
            ("caldav", Some(url)) => {
                // Plain HTTP is for a server on this machine alone, which is
                // all the engine sends it to.
                let local = url.strip_prefix("http://").filter(|rest| rest.starts_with("127.0.0.1:"));
                let Some(rest) = url.strip_prefix("https://").or(local) else {
                    bail!("{at}: `url` is fetched over TLS only, so it begins https://");
                };
                let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
                if authority.is_empty() {
                    bail!("{at}: `url` names no host");
                }
                if authority.contains('@') {
                    bail!("{at}: `url` cannot carry a user name; the account's own is signed in with");
                }
                if self.address_cmd.is_some() {
                    bail!("{at}: `address_cmd` is a feed's; CalDAV signs in with the account's password");
                }
                if account.auth.as_deref() == Some("gmail-oauth") {
                    bail!(
                        "{at}: kind = \"caldav\" signs in with the account's password, and {name} signs in with \
                         Google (auth = \"gmail-oauth\"); kind = \"feed\" is the way for it"
                    );
                }
            }
            ("feed", Some(_)) => bail!(
                "{at}: a feed has no `url` in the config, since its address is a secret: it is kept in the \
                 keyring by `tuimex setup`, or printed by an `address_cmd`"
            ),
            ("feed", None) => {
                if self.address_cmd.as_ref().is_some_and(Vec::is_empty) {
                    bail!("{at}: `address_cmd` names no program");
                }
                if self.write.is_some() {
                    bail!("{at}: `write` is for kind = \"caldav\"; a feed is only ever read");
                }
            }
            _ => bail!("{at}: `kind` is one of \"caldav\", \"feed\""),
        }
        Ok(())
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

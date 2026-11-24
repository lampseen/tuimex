//! `tuimex sync`: one pass over each account, for a person or a timer
//! (PLAN §7). The only subcommand that connects to anything.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tuimex_core::Event;
use tuimex_engine::oauth::{Endpoints, Tokens};
use tuimex_engine::sync::Policy;
use tuimex_engine::{Account, ImapAccount, StoreHandle, SystemKeyring};
use tuimex_store::Store;

use crate::config::{self, Config};
use crate::net::{self, Auth};
use crate::paths::Dirs;

/// A number and its unit, such as `30d` or `10MB`, as a count of the
/// unit `units` gives a size for. `key` is the config's whole name for it.
fn amount(key: &str, text: &str, units: &[(&str, u64)]) -> Result<u64> {
    let digits = text.trim_end_matches(|c: char| !c.is_ascii_digit());
    let unit = units.iter().find(|(name, _)| name.eq_ignore_ascii_case(text[digits.len()..].trim()));
    let known = units.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", ");
    let (_, size) = unit.ok_or_else(|| anyhow!("{key} = {text:?}: the unit is one of {known}"))?;
    let count: u64 = digits.trim().parse().map_err(|_| anyhow!("{key} = {text:?} does not start with a number"))?;
    Ok(count.saturating_mul(*size))
}

/// The `[sync]` table as the engine takes it; what is left out keeps the
/// engine's default.
pub(crate) fn policy(sync: Option<&config::Sync>) -> Result<Policy> {
    let mut policy = Policy::default();
    let Some(sync) = sync else { return Ok(policy) };
    if let Some(bodies) = &sync.bodies {
        policy.bodies = i64::try_from(amount("sync.bodies", bodies, &[("d", 86_400)])?).unwrap_or(i64::MAX);
    }
    if let Some(max_body) = &sync.max_body {
        let units = [("KB", 1_000), ("MB", 1_000_000), ("GB", 1_000_000_000)];
        policy.max_body = u32::try_from(amount("sync.max_body", max_body, &units)?).unwrap_or(u32::MAX);
    }
    policy.folders = sync.folders.clone().unwrap_or_default();
    Ok(policy)
}

/// How long a message waits in the Outbox: `send.delay`, or ten seconds.
pub(crate) fn send_delay(send: Option<&config::Send>) -> Result<Duration> {
    let Some(delay) = send.and_then(|send| send.delay.as_ref()) else { return Ok(Duration::from_secs(10)) };
    Ok(Duration::from_secs(amount("send.delay", delay, &[("s", 1), ("m", 60)])?))
}

/// What the engine needs to reach one account, or why the config does not
/// say enough.
pub(crate) fn account(
    account: &config::Account,
    policy: &Policy,
    send_delay: Duration,
    config_dir: &Path,
) -> Result<(Account, ImapAccount)> {
    let name = &account.name;
    let (server, smtp) = net::servers(account)?;
    let roles = net::roles(account)?;
    let gmail = (net::auth(account)? == Auth::Gmail).then(|| Tokens::new(Endpoints::google()));
    // An identity is an address, or a name with the address in angle
    // brackets after it.
    let address = |identity: &String| {
        let inner = identity.rsplit_once('<').and_then(|(_, rest)| rest.split_once('>')).map(|(inner, _)| inner);
        inner.unwrap_or(identity).trim().to_string()
    };
    let identities = [&account.address].into_iter().chain(account.identities.iter().flatten()).map(address).collect();
    let reach = ImapAccount {
        name: name.clone(),
        server,
        smtp,
        password_cmd: account.password_cmd.clone(),
        gmail,
        keyring: Box::new(SystemKeyring),
        config_dir: config_dir.to_path_buf(),
    };
    Ok((Account { name: name.clone(), roles, policy: policy.clone(), identities, send_delay }, reach))
}

pub fn run(config: Option<&Path>, only: Option<&str>) -> Result<ExitCode> {
    let dirs = Dirs::from_env()?;
    let config = Config::load(&config::path(config, &dirs.config))?;
    let (policy, delay) = (policy(config.sync.as_ref())?, send_delay(config.send.as_ref())?);
    let chosen: Vec<&config::Account> = config.accounts.iter().filter(|a| only.is_none_or(|o| o == a.name)).collect();
    if chosen.is_empty() {
        bail!("{}", only.map_or("the config has no account".to_string(), |o| format!("the config has no account {o}")));
    }
    if let Some((old, new)) = crate::setup::rename::unfinished(&dirs.data)? {
        bail!(crate::setup::rename::interrupted(&old, &new));
    }
    let store = Store::open(&dirs.data, &dirs.cache, tuimex_engine::parse).context("opening the mail store")?;
    let store = StoreHandle::start(store);
    let mut failed = false;
    for chosen in chosen {
        let (account, reach) = account(chosen, &policy, delay, &dirs.config)?;
        let said = |event| match event {
            Event::Synced { account, folder, changed } if changed.is_empty() => {
                println!("{}/{}: nothing new", account.0, folder.0);
            }
            Event::Synced { account, folder, changed } => {
                println!("{}/{}: {} changed", account.0, folder.0, changed.len())
            }
            _ => {}
        };
        if let Err(error) = tuimex_engine::sync::once(&reach, &store, &account, &said) {
            eprintln!("E sync: {}", error.advice(&account.name));
            failed = true;
        }
    }
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

#[cfg(test)]
mod tests;

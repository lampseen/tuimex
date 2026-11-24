//! Passwords and tokens (PLAN §4.5, ADR-010). A secret is never in the
//! config: it comes from a command the user names, or from the keyring,
//! and nothing here can turn one into text by accident.

use std::ffi::OsStr;
use std::fmt;
use std::process::{Command, Stdio};

use zeroize::Zeroizing;

/// Zeroed when dropped. There is no `Display`, and `Debug` says only that
/// something is here, so a secret inside any struct that is logged or
/// printed stays out of the output.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(secret: String) -> Secret {
        Secret(Zeroizing::new(secret))
    }

    /// The secret itself, for the one call that hands it to a server.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// The system keyring, as far as the engine needs it. A trait so that a
/// test is given a keyring of its own and never opens the user's.
pub trait Keyring: Send + Sync {
    /// The password kept for `account`, or None when there is none. The
    /// error is the keyring's own words for why it could not be asked.
    fn password(&self, account: &str) -> Result<Option<Secret>, String>;
    /// Keeps `secret` under `entry`, in place of whatever was there.
    fn set(&self, entry: &str, secret: &Secret) -> Result<(), String>;
    /// Forgets `entry`; one that was not there is as gone as was wanted.
    /// A keyring that only reads, as most tests' do, removes nothing.
    fn remove(&self, _entry: &str) -> Result<(), String> {
        Err("this keyring removes nothing".to_string())
    }
}

/// The user's own keyring, where `tuimex setup` keeps an account's
/// password under the service `tuimex` and the account's name, and what
/// Gmail's sign-in needs under the names `oauth` gives (ADR-019).
pub struct SystemKeyring;

/// The variable that turns the system keyring off (ADR-053).
const SWITCH: &str = "TUIMEX_KEYRING";

/// Why no keyring service is to be asked, when the environment says so.
/// `none` is the one value there is, and it can only refuse: no other
/// store stands in, so a secret is kept nowhere it would not have been.
/// Any other value refuses too, so a misspelling is said and not passed
/// over. Unset, as in every run that did not ask for this, the answer is
/// None and the system's keyring is the one asked.
fn turned_off(value: Option<&OsStr>) -> Option<String> {
    match value {
        None => None,
        Some(v) if v == "none" => {
            Some(format!("no keyring service could be reached ({SWITCH} is none, so none was tried)"))
        }
        Some(v) => Some(format!("{SWITCH} is {v:?}, and the only value it takes is none; no keyring was tried")),
    }
}

/// The entry for `name`. Where no keyring service could be reached the
/// crate's own error says only that no store is set; why none is, is
/// what a person on a machine without one needs to read (ADR-010).
fn entry(name: &str) -> Result<keyring::Entry, String> {
    if let Some(why) = turned_off(std::env::var_os(SWITCH).as_deref()) {
        return Err(why);
    }
    keyring::Entry::new("tuimex", name).map_err(|error| match keyring::Entry::store_status() {
        Err(why) => format!("no keyring service could be reached ({why})"),
        Ok(()) => error.to_string(),
    })
}

impl Keyring for SystemKeyring {
    fn password(&self, account: &str) -> Result<Option<Secret>, String> {
        match entry(account)?.get_password() {
            Ok(password) => Ok(Some(Secret::new(password))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(other) => Err(other.to_string()),
        }
    }

    fn set(&self, name: &str, secret: &Secret) -> Result<(), String> {
        entry(name)?.set_password(secret.expose()).map_err(|e| e.to_string())
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(other) => Err(other.to_string()),
        }
    }
}

/// None of these carries what a command printed: a password manager that
/// fails may well have printed the password first.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("password_cmd for {account} names no program")]
    NoProgram { account: String },
    #[error("password_cmd for {account} could not run {program}: {source}")]
    Spawn { account: String, program: String, source: std::io::Error },
    #[error("password_cmd for {account} failed: {program} ended with {status}")]
    Failed { account: String, program: String, status: std::process::ExitStatus },
    #[error("password_cmd for {account} printed no password")]
    Empty { account: String },
    #[error("the keyring could not be read for {account}: {reason}")]
    Keyring { account: String, reason: String },
    #[error("there is no password for {account} in the keyring, and no password_cmd")]
    Missing { account: String },
}

/// The account's password: from `password_cmd` when the config has one,
/// otherwise from the keyring. A command that fails is an error and not a
/// reason to look in the keyring, where an older password may still be.
pub fn resolve(account: &str, password_cmd: Option<&[String]>, keyring: &dyn Keyring) -> Result<Secret, SecretError> {
    let account = account.to_string();
    let Some(argv) = password_cmd else {
        let found =
            keyring.password(&account).map_err(|reason| SecretError::Keyring { account: account.clone(), reason })?;
        return found.ok_or(SecretError::Missing { account });
    };
    let Some((program, arguments)) = argv.split_first() else { return Err(SecretError::NoProgram { account }) };
    // No shell: the config's words are the program's arguments as written.
    // Its standard error is dropped rather than shown, since a terminal
    // interface owns the screen and the text may hold the secret.
    let mut command = Command::new(program);
    command.args(arguments).stdin(Stdio::null()).stderr(Stdio::null());
    let output = match command.output() {
        Ok(output) => output,
        Err(source) => return Err(SecretError::Spawn { account, program: program.clone(), source }),
    };
    let printed = Zeroizing::new(output.stdout);
    if !output.status.success() {
        return Err(SecretError::Failed { account, program: program.clone(), status: output.status });
    }
    // The first line, as `pass show` and its kin print it; text that is
    // not UTF-8 is no password a server was given as text.
    let line = std::str::from_utf8(&printed).ok().and_then(|text| text.lines().next()).unwrap_or_default();
    if line.is_empty() {
        return Err(SecretError::Empty { account });
    }
    Ok(Secret::new(line.to_string()))
}

#[cfg(test)]
mod tests;

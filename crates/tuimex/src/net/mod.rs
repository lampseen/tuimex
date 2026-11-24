//! An account's servers as `tuimex setup` and `tuimex doctor` ask after
//! them: what certificate each presents, whether a sign-in is taken, and
//! what the config says of how to reach them. A trait, so that both are
//! tested without a network.

use anyhow::{Result, anyhow, bail};
use tuimex_core::{RemoteError, RemoteFolder, Role};
use tuimex_engine::{Credential, Protocol, Seen, Server};

use crate::config;

pub trait Net {
    /// What the server presents, without signing in.
    fn certificate(&self, server: &Server, protocol: Protocol) -> Result<Seen, RemoteError>;
    /// Signs in to IMAP and lists the folders; the flag is whether the
    /// server is Gmail, whose folders are mapped its own way (PLAN §4.1).
    fn folders(
        &self,
        server: &Server,
        account: &str,
        credential: &Credential,
        pinned: Option<Vec<u8>>,
    ) -> Result<(Vec<RemoteFolder>, bool), RemoteError>;
    /// Signs in to SMTP, sends nothing, and leaves.
    fn smtp(
        &self,
        server: &Server,
        account: &str,
        credential: &Credential,
        pinned: Option<Vec<u8>>,
    ) -> Result<(), tuimex_engine::Error>;
}

/// The network itself.
pub struct Live;

impl Net for Live {
    fn certificate(&self, server: &Server, protocol: Protocol) -> Result<Seen, RemoteError> {
        tuimex_engine::certificate(server, protocol)
    }

    fn folders(
        &self,
        server: &Server,
        account: &str,
        credential: &Credential,
        pinned: Option<Vec<u8>>,
    ) -> Result<(Vec<RemoteFolder>, bool), RemoteError> {
        let mut remote = tuimex_engine::sign_in(server, account, credential, pinned)?;
        let gmail = remote.capabilities().gmail;
        Ok((remote.folders()?, gmail))
    }

    fn smtp(
        &self,
        server: &Server,
        account: &str,
        credential: &Credential,
        pinned: Option<Vec<u8>>,
    ) -> Result<(), tuimex_engine::Error> {
        tuimex_engine::smtp_check(server, account, credential, pinned)
    }
}

/// How an account signs in (PLAN §11's `auth`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// A password in the keyring.
    Password,
    /// A password a command prints.
    Command,
    /// Google's sign-in (ADR-019).
    Gmail,
}

/// What `auth` says, or what `password_cmd` implies when it is left out.
pub fn auth(account: &config::Account) -> Result<Auth> {
    let name = &account.name;
    match (account.auth.as_deref(), account.password_cmd.is_some()) {
        (None | Some("password"), false) => Ok(Auth::Password),
        (None | Some("command"), true) => Ok(Auth::Command),
        (Some("command"), false) => bail!("account {name}: auth = \"command\" needs a `password_cmd`"),
        (Some("gmail-oauth"), false) => Ok(Auth::Gmail),
        (Some("password" | "gmail-oauth"), true) => {
            bail!("account {name}: `password_cmd` is set, and `auth` says the password comes from elsewhere")
        }
        (Some(_), _) => bail!("account {name}: auth is one of \"password\", \"command\", \"gmail-oauth\""),
    }
}

/// The account's IMAP server and, where the config has one, its SMTP
/// server. Left out, `starttls` is off on the two ports that are TLS from
/// the first byte and on everywhere else; neither value means no TLS
/// (ADR-012).
pub fn servers(account: &config::Account) -> Result<(Server, Option<Server>)> {
    let server = |from: &config::Server, implicit: u16| Server {
        host: from.host.clone(),
        port: from.port,
        starttls: from.starttls.unwrap_or(from.port != implicit),
        user: from.user.clone().unwrap_or_else(|| account.address.clone()),
    };
    let imap = account.imap.as_ref().ok_or_else(|| anyhow!("account {} has no `imap` server", account.name))?;
    Ok((server(imap, 993), account.smtp.as_ref().map(|smtp| server(smtp, 465))))
}

/// The roles a folder can be given by hand, with the key each has under
/// `folders` in the config (PLAN §4.1).
pub const ROLES: [(Role, &str); 5] = [
    (Role::Archive, "archive"),
    (Role::Trash, "trash"),
    (Role::Sent, "sent"),
    (Role::Drafts, "drafts"),
    (Role::Junk, "junk"),
];

/// The roles the account's config fills by hand.
pub fn roles(account: &config::Account) -> Result<Vec<(Role, String)>> {
    let mut roles = Vec::new();
    for (key, folder) in account.folders.iter().flatten() {
        let known = ROLES.iter().find(|(_, name)| name == key);
        let (role, _) = known
            .ok_or_else(|| anyhow!("account {}: folders.{key} is not a role a folder can be given", account.name))?;
        roles.push((*role, folder.clone()));
    }
    Ok(roles)
}

#[cfg(test)]
mod tests;

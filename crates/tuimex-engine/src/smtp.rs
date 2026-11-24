//! Sending (PLAN §4.4): who a message goes to, checked before it is queued
//! (ADR-015), and handing it to the account's server through `lettre`
//! (ADR-035).

use lettre::Address;
use lettre::address::Envelope;
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{SmtpConnection, TlsParameters};
use lettre::transport::smtp::extension::ClientId;
use tuimex_core::RemoteError;

use crate::imap::PATIENCE;
use crate::{Credential, Error, Server, pin};

/// A message as a server is handed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letter {
    pub from: String,
    /// Everyone in `To`, `Cc` and `Bcc`.
    pub to: Vec<String>,
    /// The bytes as they were written, without the `Bcc` header.
    pub message: Vec<u8>,
}

fn unsendable(why: String) -> Error {
    Error::Unsendable(why)
}

/// What sending `raw` from an account with these addresses would hand its
/// server, or why it cannot be sent. An entry that is not an address is an
/// error naming it, never one left out (ADR-015), and a message is sent
/// only from an address of the account whose Outbox it is in.
pub fn letter(raw: &[u8], identities: &[String]) -> Result<Letter, Error> {
    let written = tuimex_content::submission(raw)?;
    let [from] = written.from.as_slice() else {
        return Err(unsendable("the message is not from exactly one address".to_string()));
    };
    if !identities.iter().any(|own| own.eq_ignore_ascii_case(&from.email)) {
        return Err(unsendable(format!("{:?} is not an address this account sends from", from.email)));
    }
    if written.recipients.is_empty() {
        return Err(unsendable("the message names nobody to send it to".to_string()));
    }
    for one in written.recipients.iter().chain(&written.from) {
        if one.email.parse::<Address>().is_err() {
            let shown = if one.email.is_empty() { one.name.as_deref().unwrap_or_default() } else { &one.email };
            return Err(unsendable(format!("{shown:?} is not an address")));
        }
    }
    let to = written.recipients.into_iter().map(|one| one.email).collect();
    Ok(Letter { from: from.email.clone(), to, message: written.message })
}

fn lost(error: impl std::fmt::Display) -> Error {
    RemoteError::Connection(error.to_string()).into()
}

/// A connection to `server` over TLS, signed in as the account. With
/// STARTTLS a server that will not upgrade ends the attempt before
/// anything but the greeting has been exchanged (ADR-012).
fn session(
    server: &Server,
    account: &str,
    credential: &Credential,
    pinned: Option<Vec<u8>>,
) -> Result<SmtpConnection, Error> {
    // `lettre` takes no verifier of ours, and its own cannot accept the
    // certificate a server signed for itself. For a pinned account its
    // check of the chain and the name is therefore replaced, not removed:
    // the handshake still proves the server holds the certificate's key,
    // and the certificate is compared with the pinned one below, before
    // the password or a message is sent (ADR-035).
    let tls = TlsParameters::builder(server.host.clone())
        .dangerous_accept_invalid_certs(pinned.is_some())
        .dangerous_accept_invalid_hostnames(pinned.is_some())
        .build_rustls()
        .map_err(lost)?;
    let (hello, at) = (ClientId::default(), (server.host.as_str(), server.port));
    let implicit = (!server.starttls).then_some(&tls);
    let mut connection = SmtpConnection::connect(at, Some(PATIENCE), &hello, implicit, None).map_err(lost)?;
    if server.starttls {
        let refused = |e| lost(format!("the server would not start TLS, so nothing was sent to it: {e}"));
        connection.starttls(&tls, &hello).map_err(refused)?;
    }
    if let Some(pinned) = pinned {
        pin::check(account, &pinned, &connection.peer_certificate().map_err(lost)?).map_err(lost)?;
    }
    // A password, or for Gmail an access token as XOAUTH2 (ADR-019).
    let (mechanisms, secret): (&[Mechanism], _) = match credential {
        Credential::Password(password) => (&[Mechanism::Plain, Mechanism::Login], password),
        Credential::Bearer(token) => (&[Mechanism::Xoauth2], token),
    };
    let credentials = Credentials::new(server.user.clone(), secret.expose().to_string());
    // A refusal is reduced to its kind: its text answers a command that
    // held the password (ADR-010).
    connection.auth(mechanisms, &credentials).map_err(|e| {
        let refused = e.is_permanent() || e.is_transient() || e.is_client();
        if refused { RemoteError::Auth.into() } else { lost("the sign-in exchange could not be completed") }
    })?;
    Ok(connection)
}

/// Signs in, asks the server to do nothing, and leaves: proof that mail
/// could be handed over, without handing any. What `tuimex setup` asks
/// before it saves an account, and `tuimex doctor` after.
pub fn check(server: &Server, account: &str, credential: &Credential, pinned: Option<Vec<u8>>) -> Result<(), Error> {
    let mut connection = session(server, account, credential, pinned)?;
    if !connection.test_connected() {
        return Err(lost("the server signed the account in and then stopped answering"));
    }
    // The answer is in hand; a goodbye that goes unanswered changes nothing.
    let _ = connection.quit();
    Ok(())
}

/// Hands `letter` to `server`, signed in as the account.
pub(crate) fn send(
    server: &Server,
    account: &str,
    credential: &Credential,
    pinned: Option<Vec<u8>>,
    letter: &Letter,
) -> Result<(), Error> {
    let address =
        |text: &String| text.parse::<Address>().map_err(|_| unsendable(format!("{text:?} is not an address")));
    let to = letter.to.iter().map(address).collect::<Result<Vec<_>, _>>()?;
    let envelope = Envelope::new(Some(address(&letter.from)?), to).map_err(|e| unsendable(e.to_string()))?;
    let mut connection = session(server, account, credential, pinned)?;
    connection.send(&envelope, &letter.message).map_err(|e| match e.status() {
        Some(code) => {
            let words = std::error::Error::source(&e).map(ToString::to_string).unwrap_or_default();
            Error::from(RemoteError::Refused(format!("refused by {}: {code} {words}", server.host)))
        }
        None => lost(e),
    })?;
    // The message is the server's once it has said so; a goodbye that
    // goes unanswered changes nothing.
    let _ = connection.quit();
    Ok(())
}

#[cfg(test)]
mod tests;

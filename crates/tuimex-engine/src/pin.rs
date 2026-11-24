//! A pinned certificate (ADR-012): a server that signs its own, such as
//! Proton Bridge, is trusted by keeping the certificate the user accepted
//! beside the config, for that account alone. This is the part that needs
//! no network: the file, its fingerprint, and noticing that it changed.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};
use tuimex_store::files;

const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const END: &str = "-----END CERTIFICATE-----";

#[derive(Debug, thiserror::Error)]
pub enum PinError {
    #[error("{0:?} cannot name an account's certificate file")]
    Name(String),
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("{} does not hold a certificate", path.display())]
    NotACertificate { path: PathBuf },
    #[error(
        "the certificate {account}'s server presented is not the one that was pinned\n  pinned:    {pinned}\n  presented: {presented}\nif the server's certificate was renewed, run `tuimex setup` to accept the new one"
    )]
    Changed { account: String, pinned: String, presented: String },
}

/// The SHA-256 of the certificate as it crossed the wire, in the form
/// `openssl x509 -fingerprint -sha256` prints, so the two can be compared
/// by eye.
pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der).iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(":")
}

/// The account's file in the config directory. The name is one path
/// component, so no account name can place a certificate anywhere else.
fn path(config: &Path, account: &str) -> Result<PathBuf, PinError> {
    if account.is_empty() || account.starts_with('.') || account.contains(['/', '\0']) {
        return Err(PinError::Name(account.to_string()));
    }
    Ok(config.join(format!("{account}.pem")))
}

/// Keeps `der` as the one certificate `account`'s connections trust beyond
/// the system's roots. Accepting a renewed certificate replaces the old.
pub fn save(config: &Path, account: &str, der: &[u8]) -> Result<(), PinError> {
    let path = path(config, account)?;
    let lines: Vec<String> =
        STANDARD.encode(der).as_bytes().chunks(64).map(|l| String::from_utf8_lossy(l).into()).collect();
    let pem = format!("{BEGIN}\n{}\n{END}\n", lines.join("\n"));
    let written = files::private_dir(config).and_then(|()| files::replace(&path, pem.as_bytes()));
    written.map_err(|source| PinError::Io { path, source })
}

/// The pinned certificate, or None for an account that has none.
pub fn load(config: &Path, account: &str) -> Result<Option<Vec<u8>>, PinError> {
    let path = path(config, account)?;
    let pem = match fs::read_to_string(&path) {
        Ok(pem) => pem,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(PinError::Io { path, source }),
    };
    let inner = pem.split_once(BEGIN).and_then(|(_, rest)| rest.split_once(END)).map(|(inner, _)| inner);
    let encoded: Option<String> = inner.map(|i| i.chars().filter(|c| !c.is_whitespace()).collect());
    match encoded.and_then(|e| STANDARD.decode(e).ok()).filter(|der| !der.is_empty()) {
        Some(der) => Ok(Some(der)),
        None => Err(PinError::NotACertificate { path }),
    }
}

/// Whether the certificate a server presented is the pinned one. A
/// different one is never accepted quietly: the error names both
/// fingerprints, for the user to compare with the server's own.
pub fn check(account: &str, pinned: &[u8], presented: &[u8]) -> Result<(), PinError> {
    if pinned == presented {
        return Ok(());
    }
    let (pinned, presented) = (fingerprint(pinned), fingerprint(presented));
    Err(PinError::Changed { account: account.to_string(), pinned, presented })
}

#[cfg(test)]
mod tests;

//! A message being written, as it crosses from the screen to whatever
//! stores and sends it, and the rules over what the user typed into it.

use std::path::PathBuf;

use crate::{AccountId, Address, MessageId, Origin};

#[cfg(test)]
mod tests;

/// An address mail can be sent from, and the account that sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub account: AccountId,
    pub address: Address,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    pub name: String,
    pub size: u32,
    /// Where the file is. `None` for one the stored draft already carries.
    pub path: Option<PathBuf>,
}

/// Everything compose holds of one message. Recipients, identity and files
/// are only ever what the user chose (ADR-031).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub id: MessageId,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body: String,
    pub files: Vec<Attached>,
    /// The `Message-ID` answered, then every id its thread already listed.
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    /// `Agent` when an agent wrote or changed the body (ADR-016).
    pub origin: Origin,
}

/// What `:agent` hands over in compose (ADR-031). Every text here has been
/// through `redact`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAsk {
    Reply { thread: String, guidance: String },
    Tidy { body: String },
}

/// Whether `text` is a bare ASCII `local@domain`. Stricter than the
/// standard: no quoted local part and no comment, which nobody types and a
/// header could be bent with.
pub fn is_address(text: &str) -> bool {
    let plain = text.chars().all(|c| c.is_ascii_graphic() && !"<>,;:\"()[]\\".contains(c));
    let ends = |part: &str| !part.is_empty() && !part.starts_with('.') && !part.ends_with('.');
    plain && text.split_once('@').is_some_and(|(local, domain)| ends(local) && ends(domain) && !domain.contains('@'))
}

/// One recipient as typed: `ada@example.org`, `Ada <ada@example.org>` or
/// `"Lovelace, Ada" <ada@example.org>`.
fn address(piece: &str) -> Option<Address> {
    let Some((name, rest)) = piece.rsplit_once('<') else {
        return is_address(piece).then(|| Address { name: None, email: piece.to_string() });
    };
    let email = rest.strip_suffix('>')?.trim();
    let name = name.trim().trim_matches('"').trim();
    let plain = !name.chars().any(|c| c.is_control() || "<>\"".contains(c));
    (plain && is_address(email))
        .then(|| Address { name: Some(name.to_string()).filter(|n| !n.is_empty()), email: email.to_string() })
}

/// What was typed into To, Cc or Bcc, cut at the commas outside quotes.
pub fn pieces(typed: &str) -> Vec<&str> {
    let (mut out, mut from, mut quoted) = (Vec::new(), 0, false);
    for (i, c) in typed.char_indices() {
        quoted ^= c == '"';
        if c == ',' && !quoted {
            out.push(typed[from..i].trim());
            from = i + 1;
        }
    }
    out.push(typed[from..].trim());
    out
}

/// The recipients typed into one field. One that does not parse is an
/// error naming it, never one left out (ADR-015).
pub fn addresses(typed: &str) -> Result<Vec<Address>, String> {
    let named = pieces(typed).into_iter().filter(|piece| !piece.is_empty());
    named.map(|piece| address(piece).ok_or_else(|| format!("\"{piece}\" is not an address"))).collect()
}

/// The domain a person or company registered, as far as it can be told
/// without the public suffix list: the last two labels, or three under a
/// two-letter country code whose second level is a category (`co.uk`).
pub fn registrable(email: &str) -> String {
    let domain = email.rsplit('@').next().unwrap_or("").trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = domain.split('.').collect();
    let category = |label: &str| ["co", "com", "org", "net", "ac", "gov", "edu"].contains(&label);
    let keep = match labels.as_slice() {
        [.., second, top] if top.len() == 2 && category(second) => 3,
        _ => 2,
    };
    labels[labels.len().saturating_sub(keep)..].join(".")
}

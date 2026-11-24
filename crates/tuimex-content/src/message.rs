//! A message's bytes to the plain data the rest of tuimex is allowed to
//! see (ADR-007).

use mail_parser::{AddressList, HeaderName, Message, MessageParser, MessagePart, PartKind};
use tuimex_core::{Address, Attachment, Body, Head, Link, Security, Timestamp, Unsubscribe, Url};

use crate::sanitise::{IMAGE_TYPES, InlineImage, sanitise};
use crate::text::{clean, line, push_unique};
use crate::{Error, html, invite};

#[cfg(test)]
mod tests;

fn parse(raw: &[u8]) -> Result<Message<'_>, Error> {
    MessageParser::new().parse(raw).ok_or(Error::NotAMessage)
}

/// Everyone an address header names. One with a name and no address is
/// kept, so that whoever sends can refuse it by name instead of sending to
/// the rest (ADR-015).
fn people(list: Option<AddressList>) -> Vec<Address> {
    let boxes = list.into_iter().flat_map(|l| l.mailboxes());
    let people = boxes.map(|m| Address {
        name: m.name().map(line).filter(|n| !n.is_empty()),
        email: line(m.address().unwrap_or("")),
    });
    people.filter(|a| a.name.is_some() || !a.email.is_empty()).collect()
}

/// A message as it is handed to a server for sending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    pub from: Vec<Address>,
    /// Everyone in `To`, `Cc` and `Bcc`.
    pub recipients: Vec<Address>,
    /// The message without its `Bcc` header: the people named there
    /// receive it, and nobody is told that they did.
    pub message: Vec<u8>,
}

pub fn submission(raw: &[u8]) -> Result<Submission, Error> {
    let message = parse(raw)?;
    let hidden = message.headers().iter().filter(|h| h.name() == HeaderName::Bcc);
    let mut cuts: Vec<_> = hidden.map(|h| h.offset_field() as usize..h.offset_end() as usize).collect();
    let mut bytes = raw.to_vec();
    // Last first, so that the offsets of the earlier ones stay true.
    cuts.sort_by_key(|cut| std::cmp::Reverse(cut.start));
    cuts.into_iter().filter(|cut| cut.end <= raw.len()).for_each(|cut| drop(bytes.drain(cut)));
    // Every header of each name, where the parser's own accessors give
    // the last one alone.
    let named = |name: HeaderName<'static>| {
        let lists = message.headers().iter().filter(move |h| h.name() == name);
        lists.flat_map(|h| people(h.value().as_address())).collect::<Vec<_>>()
    };
    let recipients = [HeaderName::To, HeaderName::Cc, HeaderName::Bcc].into_iter().flat_map(named).collect();
    Ok(Submission { from: people(message.from()), recipients, message: bytes })
}

/// What the headers say. `raw` may be a whole message or its header block
/// alone, which is what a server sends for the list.
pub fn envelope(raw: &[u8]) -> Result<Head, Error> {
    let message = parse(raw)?;
    let mut references = Vec::new();
    let cited = message.references().into_iter().chain(message.in_reply_to());
    cited.flat_map(|ids| ids.iter()).for_each(|id| push_unique(&mut references, &line(id)));
    let root = message.root_part();
    Ok(Head {
        date: message.date().filter(|d| d.is_valid()).map(|d| Timestamp(d.to_timestamp())),
        from: people(message.from()),
        to: people(message.to()),
        cc: people(message.cc()),
        bcc: people(message.bcc()),
        reply_to: people(message.reply_to()),
        subject: line(message.subject().unwrap_or("")),
        message_id: message.message_id().map(line).filter(|id| !id.is_empty()),
        references,
        // A header block alone has no parts to count, and `multipart/mixed`
        // is what a message with an attachment is nearly always sent as.
        has_attachments: attachments(&message).any(|p| !p.is_inline())
            || (root.raw_body().is_empty() && root.is_content_type("multipart", "mixed")),
        security: security(&root),
    })
}

/// The message as the reader shows it and the agent reads it (ADR-017).
/// `text/plain` is preferred; HTML is read as text when there is no plain
/// part with anything in it. What the HTML would have loaded is reported
/// either way, because the browser copy is made from the HTML.
pub fn body(raw: &[u8], width: usize) -> Result<Body, Error> {
    let message = parse(raw)?;
    let page = html_of(&message).map(|h| html::read(&h, width)).transpose()?;
    let plain_parts = message.text_body().filter(|p| matches!(p.kind(), PartKind::Text) && !is_calendar(p));
    let plain = plain_parts.filter_map(|p| p.text()).map(|t| clean(&t)).collect::<Vec<_>>().join("\n\n");
    let (shown, links) = match &page {
        Some(page) if plain.trim().is_empty() => (page.text.clone(), page.links.clone()),
        _ => linkify(plain.trim()),
    };
    let page = page.unwrap_or_default();
    let described = attachments(&message).enumerate().map(|(index, part)| Attachment {
        index,
        name: safe_name(part.attachment_name().unwrap_or("")),
        mime: mime(&part),
        size: u32::try_from(part.decoded_len()).unwrap_or(u32::MAX),
        inline: part.is_inline(),
    });
    let calendar = own_parts(&message).find(is_calendar).and_then(|p| p.text());
    Ok(Body {
        text: shown,
        links,
        attachments: described.collect(),
        invite: calendar.and_then(|c| invite::invite(&c)),
        blocked_remote: page.remote,
        remote_images: page.images,
        image_hosts: page.image_hosts,
        loadable_images: page.loadable,
        trackers: page.trackers,
        unsubscribe: unsubscribe_of(&message),
        pictures: Vec::new(),
    })
}

/// An attachment as it would be saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    /// A file name and never a path; see `safe_name`.
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

/// The attachment `Body::attachments` lists at `index`.
pub fn attachment(raw: &[u8], index: usize) -> Result<Payload, Error> {
    let message = parse(raw)?;
    let part = attachments(&message).nth(index).ok_or(Error::NoSuchAttachment(index))?;
    Ok(Payload {
        name: safe_name(part.attachment_name().unwrap_or("")),
        mime: mime(&part),
        data: part.decoded().into_owned(),
    })
}

/// The message's HTML as the sealed page `:browser` opens (ADR-009), with
/// its attached pictures inlined; `None` when the message has no HTML.
/// `remote` is as for `sanitise`.
pub fn browser_copy(raw: &[u8], remote: &[String]) -> Result<Option<String>, Error> {
    let message = parse(raw)?;
    let images: Vec<InlineImage> = own_parts(&message)
        .filter(|p| IMAGE_TYPES.contains(&mime(p).as_str()))
        .filter_map(|p| {
            Some(InlineImage { cid: p.content_id()?.to_string(), mime: mime(&p), data: p.decoded().into_owned() })
        })
        .collect();
    Ok(html_of(&message).map(|html| sanitise(&html, &images, remote)))
}

/// Where `List-Unsubscribe` points, if it is somewhere tuimex will go: a
/// `mailto`, which is composed for review, or else an `https` link.
pub fn unsubscribe(raw: &[u8]) -> Result<Option<Unsubscribe>, Error> {
    parse(raw).map(|message| unsubscribe_of(&message))
}

fn unsubscribe_of(message: &Message) -> Option<Unsubscribe> {
    let targets: Vec<Url> =
        message.list_unsubscribe()?.mailboxes().filter_map(|m| m.address()).map(Url::parse).collect();
    let first = |scheme: &str| {
        let usable = |u: &&Url| u.scheme == scheme && !u.host.is_empty() && !u.text.contains(char::is_whitespace);
        targets.iter().find(usable).map(|u| u.text.clone())
    };
    first("mailto").map(Unsubscribe::Mailto).or_else(|| first("https").map(Unsubscribe::Https))
}

/// What `gpg` needs from a signed or encrypted message (ADR-020). Nothing
/// here verifies or decrypts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Structure {
    Plain,
    /// RFC 3156 `multipart/signed`: the first part exactly as it was sent,
    /// headers included, and the detached signature over it.
    Signed {
        signed: Vec<u8>,
        signature: Vec<u8>,
    },
    /// RFC 3156 `multipart/encrypted`: the OpenPGP message.
    Encrypted {
        ciphertext: Vec<u8>,
    },
}

pub fn structure(raw: &[u8]) -> Result<Structure, Error> {
    let message = parse(raw)?;
    let root = message.root_part();
    let mut parts = root.children();
    Ok(match (security(&root), parts.next(), parts.next()) {
        (Security::Signed, Some(signed), Some(signature)) => {
            Structure::Signed { signed: signed.raw().to_vec(), signature: signature.decoded().into_owned() }
        }
        (Security::Encrypted, Some(_), Some(ciphertext)) => {
            Structure::Encrypted { ciphertext: ciphertext.decoded().into_owned() }
        }
        _ => Structure::Plain,
    })
}

/// What the top-level type claims; only OpenPGP's two shapes count (S/MIME
/// is not in v1).
fn security(root: &MessagePart) -> Security {
    let protocol = root.content_type().and_then(|c| c.attribute("protocol")).unwrap_or("").to_ascii_lowercase();
    match protocol.as_str() {
        "application/pgp-signature" if root.is_content_type("multipart", "signed") => Security::Signed,
        "application/pgp-encrypted" if root.is_content_type("multipart", "encrypted") => Security::Encrypted,
        _ => Security::None,
    }
}

/// The parts of the message itself, leaving out those of any message
/// attached to it: a forwarded mail's attachments are not this mail's.
fn own_parts<'m>(message: &'m Message) -> impl Iterator<Item = MessagePart<'m>> {
    message.parts().filter(|p| p.message().id() == 0)
}

/// Every part that is not body text, in document order. This is the list
/// `Body::attachments` shows and `attachment` indexes, so it is made in one
/// place. The parts OpenPGP adds are structure, not attachments: a detached
/// signature is not a file anyone was sent, and what an encrypted message
/// carries is not known until `gpg` has opened it.
fn attachments<'m>(message: &'m Message) -> impl Iterator<Item = MessagePart<'m>> {
    let security = security(&message.root_part());
    // A header block alone is a container with nothing in it: its files
    // are not known until its body is here, and it is not one of them.
    let hollow = |p: &MessagePart| p.decoded_len() == 0 && p.content_type().is_some_and(|c| c.ctype() == "multipart");
    own_parts(message)
        .filter(move |p| !hollow(p))
        .filter(|p| {
            p.is_attachment() || matches!(p.kind(), PartKind::Binary | PartKind::InlineBinary | PartKind::Message(_))
        })
        .filter(move |p| match security {
            Security::Encrypted => false,
            Security::Signed => !p.is_content_type("application", "pgp-signature"),
            _ => true,
        })
}

fn is_calendar(part: &MessagePart) -> bool {
    part.is_content_type("text", "calendar")
}

/// The HTML body, with its parts run together so one pass numbers the links.
fn html_of(message: &Message) -> Option<String> {
    let parts: Vec<_> =
        message.html_body().filter(|p| matches!(p.kind(), PartKind::Html)).filter_map(|p| p.text()).collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// The declared type, if it is shaped like one.
fn mime(part: &MessagePart) -> String {
    let token = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '+' | '.'));
    match part.content_type().map(|c| (c.ctype(), c.subtype().unwrap_or(""))) {
        Some((kind, subtype)) if token(kind) && token(subtype) => format!("{kind}/{subtype}").to_ascii_lowercase(),
        _ => "application/octet-stream".into(),
    }
}

/// The longest name kept, in bytes: under every file system's limit with
/// room for a suffix when the name is taken.
const MAX_NAME: usize = 200;

/// A name a sender chose, reduced to one that can only ever be a file in
/// the directory the user picked: no directories, no leading dot, dash or
/// tilde (a hidden file, an option, a home directory), nothing a terminal
/// would act on.
pub(crate) fn safe_name(name: &str) -> String {
    let flat = line(name);
    let base = flat.rsplit(['/', '\\']).next().unwrap_or("");
    let base = base.trim_start_matches(['.', '-', '~', ' ']).trim_end_matches(['.', ' ']);
    let mut end = base.len().min(MAX_NAME);
    while !base.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 { "attachment".into() } else { base[..end].to_string() }
}

/// Numbers the web addresses in plain text, leaving each where it stands
/// so the text still reads as it was written.
fn linkify(text: &str) -> (String, Vec<Link>) {
    let lower = text.to_ascii_lowercase();
    let (mut out, mut links, mut at) = (String::new(), Vec::new(), 0);
    while let Some(found) = ["https://", "http://"].iter().filter_map(|s| lower[at..].find(s)).min() {
        let start = at + found;
        let stop = |c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"');
        let url = text[start..].split(stop).next().unwrap_or("");
        let url = url.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'']);
        let end = start + url.len();
        out.push_str(&text[at..end]);
        at = end;
        if !Url::parse(url).host.is_empty() {
            links.push(Link { n: links.len() + 1, url: url.into(), shown: url.into() });
            out.push_str(&format!(" [{}]", links.len()));
        }
    }
    out.push_str(&text[at..]);
    (out, links)
}

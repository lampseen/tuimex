//! A draft as the bytes of a message. The one place tuimex builds mail:
//! compose and the agent's `draft` both come here, so a header is written
//! one way (PLAN §4.4).

use mail_builder::MessageBuilder;
use mail_builder::headers::address::Address as Header;
use mail_builder::headers::raw::Raw;
use mail_builder::mime::{BodyPart, MimePart};
use tuimex_core::calendar::Method;
use tuimex_core::{Address, CalendarPart, Draft, is_address};

#[cfg(test)]
mod tests;

/// A file to attach, read by whoever knows where it is.
pub struct File {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Whether the bytes name the blind copies. The copy kept in Drafts must,
/// or reopening it loses them; the copy handed to a server must not, or
/// every recipient reads them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Bcc {
    Written,
    Withheld,
}

/// A message id as it may be written into a header, or None for one that
/// could not have been a message id: it came from a message, and a line
/// break in it would start a header of the sender's choosing.
fn header_id(id: &str) -> Option<String> {
    let id = id.trim().trim_start_matches('<').trim_end_matches('>');
    let plain = !id.is_empty() && id.chars().all(|c| c.is_ascii_graphic() && c != '<' && c != '>');
    plain.then(|| id.to_string())
}

/// The type a file is sent as, by its name: the few a mail reader shows in
/// place, and otherwise bytes to save.
fn mime(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map_or("", |(_, e)| e).to_ascii_lowercase();
    match extension.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "md" | "log" => "text/plain",
        "csv" => "text/csv",
        "ics" => "text/calendar",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// The header for a list of people, or the one among them a header cannot
/// carry (ADR-015).
fn header(people: &[Address]) -> Result<Header<'_>, String> {
    let mut list = Vec::new();
    for a in people {
        let name = a.name.as_deref().filter(|n| !n.is_empty());
        if !is_address(&a.email) || name.is_some_and(|n| n.contains(char::is_control)) {
            return Err(format!("{:?} is not an address", a.email));
        }
        list.push(Header::new_address(name, a.email.as_str()));
    }
    Ok(Header::new_list(list))
}

/// The message, dated `date`. Its id is `unique` at the domain already in
/// From: the builder's own would end in the machine's name, which no
/// recipient is owed.
pub fn build(draft: &Draft, files: Vec<File>, date: i64, unique: &str, bcc: Bcc) -> Result<Vec<u8>, String> {
    if draft.subject.contains(char::is_control) {
        return Err("the subject has a control character in it".into());
    }
    let from = header(std::slice::from_ref(&draft.from))?;
    let domain = draft.from.email.rsplit('@').next().unwrap_or_default();
    let mut message = MessageBuilder::new()
        .message_id(format!("{unique}@{domain}"))
        .date(date)
        .from(from)
        .to(header(&draft.to)?)
        .subject(draft.subject.as_str())
        .text_body(draft.body.as_str());
    if !draft.cc.is_empty() {
        message = message.cc(header(&draft.cc)?);
    }
    if !draft.bcc.is_empty() && bcc == Bcc::Written {
        message = message.bcc(header(&draft.bcc)?);
    }
    let ids: Vec<String> = draft.references.iter().filter_map(|id| header_id(id)).collect();
    if let Some(parent) = draft.in_reply_to.as_deref().and_then(header_id).or_else(|| ids.last().cloned()) {
        message = message.in_reply_to(parent).references(ids);
    }
    match &draft.calendar {
        None => {
            for file in files {
                message = message.attachment(mime(&file.name), file.name, file.bytes);
            }
        }
        Some(calendar) => message = message.body(with_calendar(&draft.body, calendar, files)),
    }
    message.write_to_vec().map_err(|e| format!("building the message: {e}"))
}

/// The text and the calendar as two forms of one message (RFC 6047), the
/// text first so that a reader with no calendar shows it; beside any
/// files, inside `multipart/mixed`.
fn with_calendar<'x>(body: &'x str, calendar: &'x CalendarPart, files: Vec<File>) -> MimePart<'x> {
    let method = match calendar.method {
        Method::Request => "REQUEST",
        Method::Reply => "REPLY",
        Method::Counter => "COUNTER",
        Method::Cancel => "CANCEL",
        Method::Other => "PUBLISH",
    };
    // The type goes in as raw text so that the builder sends the part as
    // base64, which carries its CRLFs and lines of any length byte for
    // byte. Given as a content type it would be judged as text and might
    // go as quoted-printable, whose line breaks each reader decodes in its
    // own way. Nothing in it came from a message: the method is one of
    // the five above.
    let kind = Raw::new(format!("text/calendar; method={method}; charset=utf-8"));
    let part = MimePart {
        headers: vec![("Content-Type".into(), kind.into())],
        contents: BodyPart::Binary(calendar.text.as_bytes().into()),
    };
    let alternative = MimePart::new("multipart/alternative", vec![MimePart::new("text/plain", body), part]);
    if files.is_empty() {
        return alternative;
    }
    let mut parts = vec![alternative];
    parts.extend(files.into_iter().map(|file| MimePart::new(mime(&file.name), file.bytes).attachment(file.name)));
    MimePart::new("multipart/mixed", parts)
}

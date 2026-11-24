//! How a draft starts: new, as a reply, as a forward, from a saved draft
//! or from a `mailto:` link. Where the plan and the design do not say,
//! this does what mutt does (PLAN §12): `> ` before each quoted line under
//! `On <date>, <name> wrote:`, a forward as the message between two marker
//! lines with its subject in brackets after the sender, and a reply to a
//! message of one's own going to whom that message went.

use tuimex_core::{Address, Attached, Identity, Listed, Origin, Timestamp, Url, is_address};

use super::{Compose, Field, Mark};
use crate::when::stamp;

/// What every start needs: who mail can be from, and the new draft's key.
pub struct Opening<'a> {
    pub identities: &'a [Identity],
    pub key: String,
}

/// An address as it is typed into a field. A name the field could not be
/// read back with is left out; the address is never changed.
pub fn written(a: &Address) -> String {
    let plain = |name: &&str| !name.is_empty() && !name.chars().any(|c| c.is_control() || "<>\"".contains(c));
    match a.name.as_deref().map(str::trim).filter(plain) {
        Some(name) if name.contains(',') => format!("\"{name}\" <{}>", a.email),
        Some(name) => format!("{name} <{}>", a.email),
        None => a.email.clone(),
    }
}

fn listed(people: &[Address]) -> String {
    people.iter().map(written).collect::<Vec<_>>().join(", ")
}

impl Opening<'_> {
    fn mine(&self, a: &Address) -> bool {
        self.identities.iter().any(|i| i.address.email.eq_ignore_ascii_case(&a.email))
    }

    /// The identity a message was sent to, or its account's first.
    fn receiving(&self, msg: &Listed) -> usize {
        let e = &msg.envelope;
        let of_account = |i: &&(usize, &Identity)| i.1.account == e.id.account;
        let addressed = |i: &&(usize, &Identity)| {
            e.to.iter().chain(&e.cc).chain(&e.from).any(|a| a.email.eq_ignore_ascii_case(&i.1.address.email))
        };
        let all: Vec<(usize, &Identity)> = self.identities.iter().enumerate().collect();
        let found = all.iter().filter(of_account).find(addressed).or_else(|| all.iter().find(of_account));
        found.map_or(0, |i| i.0)
    }

    /// A new message, from the first identity of `account` when one is
    /// given.
    pub fn blank(self, account: Option<&tuimex_core::AccountId>, to: &str) -> Compose {
        let identity = self.identities.iter().position(|i| Some(&i.account) == account).unwrap_or(0);
        Compose { to: to.to_string(), dirty: !to.is_empty(), ..Compose::new(self.key, identity) }
    }

    /// `r` and `R`. The reply goes where the sender asked for answers, or
    /// to the sender, and with `all` is copied to everyone else the message
    /// went to; neither list names the user.
    pub fn reply(self, msg: &Listed, all: bool) -> Compose {
        let e = &msg.envelope;
        // One's own is what this account sent. Mail from another of the
        // user's accounts was received like any other, and is answered
        // to where it came from.
        let sent_here = |a: &Address| {
            let here = self.identities.iter().filter(|i| i.account == e.id.account);
            here.into_iter().any(|i| i.address.email.eq_ignore_ascii_case(&a.email))
        };
        let own = e.from.iter().all(sent_here);
        let asked = if e.reply_to.is_empty() { &e.from } else { &e.reply_to };
        let to: Vec<Address> = if own { e.to.clone() } else { asked.clone() };
        let mut cc: Vec<Address> = Vec::new();
        let others = if own { e.cc.iter().chain(&[]) } else { e.to.iter().chain(&e.cc) };
        for a in others.filter(|_| all) {
            let known = |b: &Address| b.email.eq_ignore_ascii_case(&a.email);
            if !self.mine(a) && !to.iter().any(known) && !cc.iter().any(known) {
                cc.push(a.clone());
            }
        }
        let again = e.subject.get(..3).is_some_and(|re| re.eq_ignore_ascii_case("re:"));
        let name =
            e.from.first().map_or("someone", |a| a.name.as_deref().filter(|n| !n.is_empty()).unwrap_or(&a.email));
        let mut body = format!("On {}, {name} wrote:\n", stamp(e.date));
        for line in msg.text.as_deref().unwrap_or(&msg.snippet).lines() {
            body += if line.is_empty() { ">\n" } else { "> " };
            body += line;
            body += if line.is_empty() { "" } else { "\n" };
        }
        Compose {
            to: listed(&to),
            cc: listed(&cc),
            subject: if again { e.subject.clone() } else { format!("Re: {}", e.subject) },
            body,
            focus: Field::Body,
            in_reply_to: e.message_id.clone(),
            references: e.references.iter().chain(&e.message_id).cloned().collect(),
            ..Compose::new(self.key.clone(), self.receiving(msg))
        }
    }

    /// `f`: the message's text between two marker lines, under the headers
    /// a reader needs. Its files are not carried over.
    pub fn forward(self, msg: &Listed) -> Compose {
        let e = &msg.envelope;
        let sender = e.from.first().map(written).unwrap_or_default();
        let mut body = format!("\n----- Forwarded message from {sender} -----\n\n");
        body += &format!("Date: {}\nFrom: {sender}\nTo: {}\n", stamp(e.date), listed(&e.to));
        if !e.cc.is_empty() {
            body += &format!("Cc: {}\n", listed(&e.cc));
        }
        let text = msg.text.as_deref().unwrap_or(&msg.snippet);
        body += &format!("Subject: {}\n\n{}\n\n----- End forwarded message -----\n", e.subject, text.trim_end());
        let from = e.from.first().map_or("", |a| a.email.as_str());
        let subject = format!("[{from}: {}]", e.subject);
        Compose { subject, body, ..Compose::new(self.key.clone(), self.receiving(msg)) }
    }

    /// A draft in the mailbox, opened again with the cursor on From. An
    /// agent's is marked as one, from when it was filed.
    pub fn from_draft(self, msg: &Listed) -> Compose {
        let e = &msg.envelope;
        let from = |i: &Identity| e.from.iter().any(|a| a.email.eq_ignore_ascii_case(&i.address.email));
        let identity = self.identities.iter().position(|i| i.account == e.id.account && from(i));
        let file = |a: &tuimex_core::Attachment| Attached { name: a.name.clone(), size: a.size, path: None };
        Compose {
            to: listed(&e.to),
            cc: listed(&e.cc),
            bcc: listed(&e.bcc),
            subject: e.subject.clone(),
            body: msg.text.clone().unwrap_or_default(),
            files: msg.attachments.iter().map(file).collect(),
            focus: Field::From,
            in_reply_to: e.references.last().cloned(),
            references: e.references.clone(),
            agent: (e.origin == Origin::Agent).then_some(Mark { at: e.date, tidied: None }),
            stored: Some(e.id.clone()),
            ..Compose::new(e.id.key.clone(), identity.unwrap_or_else(|| self.receiving(msg)))
        }
    }

    /// A `mailto:` link: its address, and the `subject`, `cc` and `body` it
    /// names. Anything else it names is left out, a hidden `bcc` first of
    /// all, and the user still reads every field before the summary.
    pub fn mailto(self, account: Option<&tuimex_core::AccountId>, url: &str) -> Option<Compose> {
        let url = Url::parse(url);
        let rest = url.text.get(7..).filter(|_| url.scheme == "mailto")?;
        let (to, query) = rest.split_once('?').unwrap_or((rest, ""));
        let people = |text: &str| {
            let all: Vec<String> = decoded(text).split(',').map(|a| a.trim().to_string()).collect();
            all.into_iter().filter(|a| is_address(a)).collect::<Vec<_>>().join(", ")
        };
        let mut c = Compose { to: people(to), dirty: true, ..self.blank(account, "") };
        for (name, value) in query.split('&').filter_map(|pair| pair.split_once('=')) {
            match name.to_ascii_lowercase().as_str() {
                "subject" => c.subject = decoded(value).replace('\n', " "),
                "cc" => c.cc = people(value),
                "body" => c.body = decoded(value),
                _ => {}
            }
        }
        Some(c)
    }
}

/// Percent-decoded, with no control character but the line break.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 2;
            }
            (byte, _) => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).chars().filter(|c| *c == '\n' || !c.is_control()).collect()
}

/// The key of a draft nothing has stored yet.
pub fn key(now: Timestamp, serial: u64) -> String {
    format!("draft-{}-{serial}", now.0)
}

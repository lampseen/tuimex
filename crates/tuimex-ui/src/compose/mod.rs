//! A message being written: its state, and the screen as `design/tui.js`
//! draws it (`composeScreen`, `draftFields`, `draftBody`), ported and kept
//! behaviourally identical (ADR-002).

pub mod quote;
pub mod summary;

use tuimex_core::{
    AccountId, Address, Attached, Draft, Identity, Listed, MessageId, Origin, Redacted, Timestamp, addresses, pieces,
    thread as threads,
};

use crate::glyphs::{AGENT, CURSOR, DOT};
use crate::keymap;
use crate::list;
use crate::palette::Tok;
use crate::reader::size_text;
use crate::span::{Line, cells, fit, gap, run, wrap};
use crate::when::stamp;

#[cfg(test)]
mod tests;

/// The design's `LAYOUT`, as the list has it.
const MARGIN: usize = 2;
const INDENT: usize = 4;
const MEASURE: usize = 76;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    From,
    To,
    Cc,
    Bcc,
    Subject,
    /// Drawn with no cursor: the body is edited in `$EDITOR`, not here.
    Body,
}

pub const FIELDS: [Field; 6] = [Field::From, Field::To, Field::Cc, Field::Bcc, Field::Subject, Field::Body];

/// That an agent wrote the body or changed it (ADR-016), and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub at: Timestamp,
    /// After `:agent tidy`, the paragraphs it rewrote.
    pub tidied: Option<Vec<String>>,
}

/// The path being typed after `^a`, and the files it could go on to name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prompt {
    pub text: String,
    pub menu: Option<(Vec<String>, usize)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    Fields,
    Attach(Prompt),
    /// Esc was pressed on a draft with changes: keep or discard.
    Closing,
    Summary,
    /// The agent command is running for the `Ask` of this serial.
    Working {
        serial: u64,
        tidy: bool,
        messages: usize,
        redacted: Redacted,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compose {
    /// The draft's key; its account is the chosen identity's.
    pub key: String,
    /// Which of the identities it is from.
    pub identity: usize,
    /// To, Cc and Bcc as typed, checked when the summary is asked for.
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub body: String,
    pub files: Vec<Attached>,
    pub focus: Field,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub agent: Option<Mark>,
    /// The body and its mark as they were before the agent's text, which
    /// `u` puts back.
    pub own: Option<(String, Option<Mark>)>,
    /// What the agent was shown placeholders for, for the status line.
    pub replaced: Option<String>,
    pub saved: Option<Timestamp>,
    /// The id it is filed under in Drafts, once it has been.
    pub stored: Option<MessageId>,
    /// Changed since it was opened or saved.
    pub dirty: bool,
    pub stage: Stage,
}

/// What the screen shows that the draft does not hold.
pub struct View<'a> {
    pub identities: &'a [Identity],
    /// The left of the status line, in place of when the draft was saved.
    pub left: Option<Line>,
    /// The right of the status line when no agent has been asked.
    pub right: Line,
    /// What the command line holds, when it is not the hint.
    pub command: Option<Line>,
    pub panel: Vec<Line>,
}

impl Compose {
    pub fn new(key: String, identity: usize) -> Compose {
        Compose {
            key,
            identity,
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            body: String::new(),
            files: Vec::new(),
            focus: Field::To,
            in_reply_to: None,
            references: Vec::new(),
            agent: None,
            own: None,
            replaced: None,
            saved: None,
            stored: None,
            dirty: false,
            stage: Stage::Fields,
        }
    }

    /// The text of the field the cursor is in, when it is one typed here.
    pub fn typed(&mut self) -> Option<&mut String> {
        match self.focus {
            Field::To => Some(&mut self.to),
            Field::Cc => Some(&mut self.cc),
            Field::Bcc => Some(&mut self.bcc),
            Field::Subject => Some(&mut self.subject),
            Field::From | Field::Body => None,
        }
    }

    /// To, Cc and Bcc, or the first address among them that does not
    /// parse (ADR-015).
    pub fn recipients(&self) -> Result<[Vec<Address>; 3], String> {
        Ok([addresses(&self.to)?, addresses(&self.cc)?, addresses(&self.bcc)?])
    }

    /// The message as it is handed on, under the identity's account.
    pub fn draft(&self, identities: &[Identity]) -> Result<Draft, String> {
        let from = identities.get(self.identity).ok_or("there is no identity to send from")?;
        if self.subject.contains(char::is_control) {
            return Err("the subject has a control character in it".into());
        }
        let [to, cc, bcc] = self.recipients()?;
        Ok(Draft {
            id: MessageId { account: from.account.clone(), key: self.key.clone() },
            from: from.address.clone(),
            to,
            cc,
            bcc,
            subject: self.subject.clone(),
            body: self.body.clone(),
            files: self.files.clone(),
            in_reply_to: self.in_reply_to.clone(),
            references: self.references.clone(),
            origin: if self.agent.is_some() { Origin::Agent } else { Origin::Local },
            calendar: None,
        })
    }

    fn fields(&self, identities: &[Identity]) -> Vec<Line> {
        let caret = |on: bool| on.then(|| run(" ", Tok::Bg).on(Tok::Acc));
        let mut from = Line::new();
        for (i, identity) in identities.iter().enumerate() {
            let email = run(identity.address.email.as_str(), Tok::Dim);
            from.extend([if i == self.identity { run(email.text, Tok::Fg).bold() } else { email }, gap(3)]);
        }
        let mut rows = vec![field_row(self.focus == Field::From, "From", from)];
        for (field, label, typed) in
            [(Field::To, "To", &self.to), (Field::Cc, "Cc", &self.cc), (Field::Bcc, "Bcc", &self.bcc)]
        {
            let on = self.focus == field;
            let all = pieces(typed);
            let last = all.len() - 1;
            // A comma just typed opens the next line; one left behind does
            // not leave an empty line under the field.
            let shown = all.iter().enumerate().filter(|(i, piece)| !piece.is_empty() || *i == 0 || (on && *i == last));
            for (i, piece) in shown {
                let mut value = match addresses(piece).as_deref() {
                    Ok([one]) => person(one),
                    _ => vec![run(*piece, Tok::Fg)],
                };
                value.extend(caret(on && i == last));
                rows.push(field_row(on && i == 0, if i == 0 { label } else { "" }, value));
            }
        }
        let on = self.focus == Field::Subject;
        let mut subject = vec![run(self.subject.as_str(), Tok::Fg).bold()];
        subject.extend(caret(on));
        rows.push(field_row(on, "Subject", subject));
        rows.extend(file_rows(&self.files));
        rows
    }

    /// The body at the reading measure. A paragraph `:agent tidy` rewrote
    /// has the agent's mark in the gutter of each of its rows, and while
    /// the agent works one line stands where the body will be.
    fn body_rows(&self, measure: usize) -> Vec<Line> {
        if let Stage::Working { tidy, messages, .. } = &self.stage {
            let what = if *tidy { "Tidying your text" } else { "Writing a reply" };
            let mut line = vec![gap(INDENT), run(format!("{AGENT} {what}"), Tok::Agent)];
            let from = if *messages == 1 { "message".to_string() } else { format!("{messages} messages") };
            let from = if *messages == 1 { format!("the 1 {from}") } else { format!("the {from}") };
            line.extend((!tidy).then(|| run(format!(" {DOT} from {from} of the thread"), Tok::Dim)));
            return vec![line];
        }
        let tidied = self.agent.as_ref().and_then(|mark| mark.tidied.as_deref()).unwrap_or(&[]);
        let mut rows = Vec::new();
        for (i, paragraph) in self.body.split("\n\n").enumerate() {
            if i > 0 {
                rows.push(Line::new());
            }
            let changed = tidied.iter().any(|p| p == paragraph);
            for line in paragraph.lines() {
                let line = line.replace('\t', "    ");
                // A blank line stays one; `wrap` would drop it.
                let wrapped = if line.is_empty() { vec![String::new()] } else { wrap(&line, measure) };
                rows.extend(wrapped.into_iter().map(|text| {
                    let gutter =
                        if changed { vec![gap(MARGIN), run(AGENT, Tok::Agent), gap(1)] } else { vec![gap(INDENT)] };
                    [gutter, vec![run(text, Tok::Fg)]].concat()
                }));
            }
        }
        rows
    }

    pub fn screen(&self, view: &View, width: usize, height: usize) -> Vec<Line> {
        let mut head = vec![title(&self.subject), Line::new()];
        head.extend(self.fields(view.identities));
        head.push(Line::new());
        if let Some(mark) = self.agent.as_ref().filter(|_| !matches!(self.stage, Stage::Working { .. })) {
            let what = if mark.tidied.is_some() { "Tidied by an agent" } else { "Written by an agent" };
            let when = run(format!(" {DOT} {}", stamp(mark.at)), Tok::Dim);
            head.extend([vec![gap(INDENT), run(format!("{AGENT} {what}"), Tok::Agent), when], Line::new()]);
        }
        let head = head.iter().map(|row| fit(row, width)).collect();
        let body = self.body_rows(MEASURE.min(width.saturating_sub(INDENT + MARGIN)));
        let saved = || self.saved.map(|at| vec![run(format!("draft saved {}", crate::when::clock(at)), Tok::Dim)]);
        let left = view.left.clone().or_else(saved).unwrap_or_default();
        let right = match &self.replaced {
            Some(what) if what.is_empty() => vec![run("agent saw the text as written", Tok::Dim)],
            Some(what) => vec![run("agent saw placeholders for", Tok::Dim), run(format!(" {what}"), Tok::Fg)],
            None => view.right.clone(),
        };
        // The keys that choose who it is from are letters in a field that
        // is typed into, so they are named only on From.
        let identity = (self.focus == Field::From).then_some("compose.identity");
        let ids = identity.into_iter().chain(["compose.editor", "global.agent", "compose.summary", "compose.close"]);
        let keys: Vec<_> = [("Tab", "next field")].into_iter().chain(ids.map(keymap::hint)).collect();
        let hint = || list::hint(&keys);
        let status = list::status_row(width, &left, &right);
        let foot = [view.panel.clone(), vec![Line::new(), status, view.command.clone().unwrap_or_else(hint)]].concat();
        frame(head, body, foot, height)
    }
}

/// The subject over the fields, as the reader heads a message with it.
fn title(subject: &str) -> Line {
    list::heading(vec![if subject.is_empty() { run("New message", Tok::Dim) } else { run(subject, Tok::Fg).bold() }])
}

/// A name and its address, or the address alone.
pub(crate) fn person(a: &Address) -> Line {
    match a.name.as_deref().filter(|n| !n.is_empty()) {
        Some(name) => vec![run(format!("{name} "), Tok::Fg), run(format!("<{}>", a.email), Tok::Dim)],
        None => vec![run(a.email.as_str(), Tok::Fg)],
    }
}

/// One row of the fields: the cursor when it is the focused one, the label
/// and the value.
pub(crate) fn field_row(focused: bool, label: &str, value: Line) -> Line {
    let label = run(format!("{label:<9}"), if focused { Tok::Fg } else { Tok::Dim });
    let cursor = if focused { run(CURSOR, Tok::Acc) } else { gap(1) };
    [vec![cursor, gap(INDENT - 1), if focused { label.bold() } else { label }], value].concat()
}

/// A line to a file, so that none hides at the end of a long row.
pub(crate) fn file_rows(files: &[Attached]) -> Vec<Line> {
    let row = |(i, file): (usize, &Attached)| {
        let value =
            vec![run(format!("{:<28}", file.name), Tok::Fg), run(format!("{:>7}", size_text(file.size)), Tok::Dim)];
        field_row(false, if i == 0 { "Files" } else { "" }, value)
    };
    files.iter().enumerate().map(row).collect()
}

/// The design's `frame`: the head, as much of the body as fits, then the
/// foot at the bottom.
pub(crate) fn frame(head: Vec<Line>, mut body: Vec<Line>, foot: Vec<Line>, height: usize) -> Vec<Line> {
    let room = height.saturating_sub(head.len() + foot.len());
    body.truncate(room);
    while body.last().is_some_and(|line| cells(line) == 0) {
        body.pop();
    }
    body.resize(room, Line::new());
    [head, body, foot].concat()
}

/// A message id as two headers may spell it, with or without its brackets.
fn bare(id: &str) -> &str {
    id.trim().trim_start_matches('<').trim_end_matches('>')
}

/// The conversation the message with that `Message-ID` belongs to, oldest
/// first: what `:agent reply` reads and the summary compares recipients
/// with. Drafts and what is waiting to go are not part of it.
pub fn thread<'a>(mail: &'a [Listed], account: &AccountId, answered: Option<&str>) -> Vec<&'a Listed> {
    let Some(answered) = answered.map(bare) else { return Vec::new() };
    let received = |m: &&Listed| m.envelope.id.account == *account && !m.envelope.flags.draft && m.sending.is_none();
    let received: Vec<&Listed> = mail.iter().filter(received).collect();
    let Some(target) = received.iter().find(|m| m.envelope.message_id.as_deref().map(bare) == Some(answered)) else {
        return Vec::new();
    };
    let envelopes: Vec<_> = received.iter().map(|m| m.envelope.clone()).collect();
    let all = threads(&envelopes);
    let members = all.iter().find(|t| t.members.iter().any(|n| n.id == target.envelope.id));
    let within = |m: &&&Listed| members.is_some_and(|t| t.members.iter().any(|n| n.id == m.envelope.id));
    let mut thread: Vec<&Listed> = received.iter().filter(within).copied().collect();
    thread.sort_by_key(|m| m.envelope.date);
    thread
}

/// Everyone a thread's headers name, and `others`, each name once: the
/// names redaction looks for (ADR-031).
pub fn names(thread: &[&Listed], others: &[Address]) -> Vec<String> {
    let headers = thread.iter().flat_map(|m| m.envelope.from.iter().chain(&m.envelope.to).chain(&m.envelope.cc));
    let mut names: Vec<String> = Vec::new();
    for name in others.iter().chain(headers).filter_map(|a| a.name.clone()).filter(|n| !n.trim().is_empty()) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The thread as the reader shows it (ADR-017), for the agent: who is
/// replying, then each message's header block and text. A message not read
/// yet is its opening.
pub fn conversation(thread: &[&Listed], from: &Address) -> String {
    let list = |people: &[Address]| people.iter().map(quote::written).collect::<Vec<_>>().join(", ");
    let mut out = format!("The reply will be sent by {}.\n", quote::written(from));
    for m in thread {
        let e = &m.envelope;
        out += &format!("\nFrom: {}\nTo: {}\n", list(&e.from), list(&e.to));
        if !e.cc.is_empty() {
            out += &format!("Cc: {}\n", list(&e.cc));
        }
        out += &format!(
            "Date: {}\nSubject: {}\n\n{}\n",
            stamp(e.date),
            e.subject,
            m.text.as_deref().unwrap_or(&m.snippet)
        );
    }
    out
}

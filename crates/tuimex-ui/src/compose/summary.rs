//! The send summary (PLAN §6.4): the design's `summaryScreen`. One screen,
//! before anything leaves, of who receives what.

use tuimex_core::{Address, Draft, Listed, registrable};

use super::{INDENT, Mark, field_row, file_rows, frame, person};
use crate::glyphs::{AGENT, DOT, NOTICE};
use crate::list;
use crate::palette::Tok;
use crate::reader::size_text;
use crate::say::count;
use crate::span::{Line, cells, fit, gap, run};
use crate::when::{span_of, stamp};

/// The column the outside-the-thread mark starts at, after the indent and
/// the label.
const RECIPIENT: usize = 46;

/// What `gpg` will do to the message (ADR-020).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signing {
    Neither,
    /// Signed with `key`; `no_key` are the recipients it cannot be
    /// encrypted to.
    Signed {
        key: String,
        no_key: Vec<String>,
    },
}

pub struct Summary<'a> {
    pub draft: &'a Draft,
    pub agent: Option<&'a Mark>,
    /// The messages of the thread answered; empty for a new message.
    pub thread: &'a [&'a Listed],
    pub signing: &'a Signing,
    /// The seconds the message is held after `y`.
    pub delay: u32,
    /// The right of the status line.
    pub right: &'a Line,
}

/// Whether the address's registrable domain appears nowhere in the thread
/// being answered, which is what a redirected reply looks like.
fn outside(a: &Address, thread: &[&Listed]) -> bool {
    let headers = thread.iter().flat_map(|m| m.envelope.from.iter().chain(&m.envelope.to).chain(&m.envelope.cc));
    !thread.is_empty() && !headers.into_iter().any(|known| registrable(&known.email) == registrable(&a.email))
}

pub fn screen(o: &Summary, width: usize, height: usize) -> Vec<Line> {
    let draft = o.draft;
    let subject = run(draft.subject.as_str(), Tok::Fg).bold();
    let mut rows =
        vec![list::heading(vec![subject.clone()]), Line::new(), field_row(false, "From", person(&draft.from))];
    for (label, people) in [("To", &draft.to), ("Cc", &draft.cc), ("Bcc", &draft.bcc)] {
        for (i, a) in people.iter().enumerate() {
            let mut value = person(a);
            if outside(a, o.thread) {
                // A long address is never cut to make room for its mark.
                value = if cells(&value) < RECIPIENT { fit(&value, RECIPIENT) } else { [value, vec![gap(2)]].concat() };
                value.push(run(format!("{NOTICE} not in this thread"), Tok::Warn));
            }
            rows.push(field_row(false, if i == 0 { label } else { "" }, value));
        }
    }
    rows.push(field_row(false, "Subject", vec![subject]));
    rows.extend(file_rows(&draft.files));
    if !draft.files.is_empty() {
        let total = draft.files.iter().fold(0u32, |sum, file| sum.saturating_add(file.size));
        let value = vec![run(format!("{:<28}", "total"), Tok::Dim), run(format!("{:>7}", size_text(total)), Tok::Fg)];
        rows.push(field_row(false, "", value));
    }
    let then = |detail: String| run(format!(" {DOT} {detail}"), Tok::Dim);
    match o.signing {
        Signing::Neither => {
            rows.push(field_row(false, "Signing", vec![run("not signed", Tok::Fg), then("not encrypted".into())]))
        }
        Signing::Signed { key, no_key } => {
            rows.push(field_row(false, "Signing", vec![run("signed", Tok::Fg), then(format!("key {key}"))]));
            let unkeyed = |who: &String| {
                field_row(false, "", vec![run("not encrypted", Tok::Fg), then(format!("no key for {who}"))])
            };
            rows.extend(no_key.iter().map(unkeyed));
        }
    }
    if let Some(mark) = o.agent {
        let what = if mark.tidied.is_some() { "Tidied by an agent" } else { "Written by an agent" };
        // An agent's draft carries no file (ADR-016), so any here are the
        // user's.
        let files = if draft.files.is_empty() { String::new() } else { format!(" {DOT} files added by you") };
        let when = then(format!("{}{files}", stamp(mark.at)));
        rows.extend([Line::new(), vec![gap(INDENT), run(format!("{AGENT} {what}"), Tok::Agent), when]]);
    }
    let to = count(draft.to.len() + draft.cc.len() + draft.bcc.len(), "recipient");
    let ask = list::heading(vec![
        run(format!("Send to {to}?"), Tok::Fg),
        run("  ", Tok::Dim),
        run("y ", Tok::Fg),
        run(format!("send, held {} {DOT} ", span_of(o.delay)), Tok::Dim),
        run("n ", Tok::Fg),
        run("back", Tok::Dim),
    ]);
    let head = rows.iter().map(|row| fit(row, width)).collect();
    frame(head, Vec::new(), vec![Line::new(), list::status_row(width, &[], o.right), ask], height)
}

//! What surrounds the rows: the tabs, the status line and the command
//! line. The design's `tabsRow`, `statusRow`, `alerts`, `target`, `hint`.

use tuimex_core::{MessageId, Sending, Timestamp};

use super::{Fold, Row};
use crate::glyphs::{CURSOR, DOT};
use crate::keymap;
use crate::layout::{INDENT, MARGIN};
use crate::palette::Tok;
use crate::span::{Line, cells, fill, fit, gap, left_right, run};
use crate::when::{clock, span_of};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub name: String,
    /// Shown only when it is not zero: a tab has a number when the number
    /// means "this needs you".
    pub count: usize,
    /// What the count is of: `Fg` for mail that needs you, `Warn` for
    /// flagged mail, `Err` for failures.
    pub tone: Tok,
}

/// `search` is the query of a search being shown: it takes the place of
/// the active tab, after the others.
pub fn tabs_row(width: usize, tabs: &[Tab], active: &str, search: Option<&str>) -> Line {
    let mut line = vec![gap(MARGIN)];
    for tab in tabs {
        let on = tab.name == active && search.is_none();
        line.push(crate::span::Span { bold: on, ..run(tab.name.as_str(), if on { Tok::Acc } else { Tok::Dim }) });
        if tab.count > 0 {
            let count = run(tab.count.to_string(), tab.tone);
            line.extend([gap(1), if tab.tone == Tok::Fg { count.bold() } else { count }]);
        }
        line.push(gap(3));
    }
    if let Some(query) = search {
        line.extend([run("Search", Tok::Acc).bold(), gap(1), run(query, Tok::Fg)]);
    }
    fit(&line, width)
}

/// What is not normal about sending and syncing. Healthy accounts and an
/// empty outbox are not in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sync {
    /// The seconds until the held message goes.
    pub held: Option<u32>,
    /// How many messages were refused.
    pub failed: usize,
    pub accounts: Vec<(String, AccountSync)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountSync {
    /// `reason` is the engine's word for it. `retry` is in seconds, and
    /// `None` when trying again cannot help; `last` is `None` when no sync
    /// of this run has succeeded, since nothing kept says when one did.
    Failing {
        reason: String,
        retry: Option<u32>,
        last: Option<Timestamp>,
    },
    FirstSync {
        done: u32,
        total: u32,
    },
}

pub(crate) fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The right of the status line. Spans are built directly: an account's
/// name and a failure's reason are not this program's text.
pub fn alerts(sync: &Sync) -> Line {
    let mut parts: Vec<Line> = Vec::new();
    if let Some(seconds) = sync.held {
        parts.push(vec![run(format!("sending in {}", span_of(seconds)), Tok::Fg)]);
    }
    if sync.failed > 0 {
        parts.push(vec![run(format!("{} not sent", sync.failed), Tok::Err)]);
    }
    for (name, state) in &sync.accounts {
        parts.push(match state {
            AccountSync::Failing { reason, retry, last } => {
                let retry = retry.map(|seconds| format!(" {DOT} retry in {}", span_of(seconds)));
                let last = last.map(|at| format!(" {DOT} last synced {}", clock(at)));
                let then = [retry, last].into_iter().flatten().collect::<String>();
                vec![run(format!("{name} {reason}"), Tok::Err), run(then, Tok::Dim)]
            }
            AccountSync::FirstSync { done, total } => vec![
                run(format!("{name} first sync"), Tok::Fg),
                run(format!(" {DOT} {} of {}", thousands(*done), thousands(*total)), Tok::Dim),
            ],
        });
    }
    parts.join(&run(format!(" {DOT} "), Tok::Dim))
}

pub fn status_row(width: usize, left: &[crate::span::Span], right: &[crate::span::Span]) -> Line {
    let mut head = vec![gap(MARGIN)];
    head.extend_from_slice(left);
    let room = width.saturating_sub(cells(&head) + 2 + MARGIN);
    let mut tail = vec![gap(2)];
    tail.extend(fit(right, cells(right).min(room)));
    tail.push(gap(MARGIN));
    left_right(&head, &tail, width)
}

/// What an action would take, for the left of the status line.
pub fn target(row: &Row, marks: &[MessageId]) -> Line {
    if !marks.is_empty() {
        return vec![run(format!("{} marked", marks.len()), Tok::Fg)];
    }
    let what = match row.fold {
        Some(Fold::Folded) => format!("thread of {}", row.size),
        Some(Fold::Open) => "1 message of the thread".to_string(),
        None if row.reply => "1 message of the thread".to_string(),
        None => "1 message".to_string(),
    };
    vec![run(what, Tok::Fg)]
}

/// Where the cursor is among the rows, then `what`.
pub fn position(at: usize, total: usize, what: Line) -> Line {
    let mut line = vec![run(format!("{at} of {total} {DOT}"), Tok::Dim), gap(1)];
    line.extend(what);
    line
}

/// A line of the screen that starts at the margin.
pub fn heading(line: Line) -> Line {
    let mut out = vec![gap(MARGIN)];
    out.extend(line);
    out
}

/// A hint line: each key, then dimly what it does.
pub fn hint(entries: &[(&str, &str)]) -> Line {
    let mut line = Line::new();
    for (i, (key, does)) in entries.iter().enumerate() {
        if i > 0 {
            line.push(run(format!(" {DOT} "), Tok::Dim));
        }
        line.extend([run(format!("{key} "), Tok::Fg), run(*does, Tok::Dim)]);
    }
    heading(line)
}

/// The idle command line: a few keys for what is selected, as the keymap
/// names them. `search` is whether the rows are what a search found.
pub fn list_hint(row: Option<&Row>, marks: &[MessageId], search: bool) -> Line {
    let keys = |ids: &[&str]| ids.iter().map(|id| keymap::hint(id)).collect::<Vec<_>>();
    let entries = if !marks.is_empty() {
        [
            keys(&["list.archive", "list.trash", "list.move-to"]),
            vec![("Esc", "clear marks"), keymap::hint("global.help")],
        ]
        .concat()
    } else {
        let draft = row.is_some_and(|row| row.msg.envelope.flags.draft && row.msg.sending.is_none());
        match row.map(|row| (&row.msg.sending, row.fold, row.reply)) {
            None if search => keys(&["global.back"]),
            None => keys(&["list.compose", "global.help"]),
            Some((Some(Sending::Failed { .. }), ..)) => {
                [vec![("Enter", "edit and resend")], keys(&["list.trash", "global.help"])].concat()
            }
            Some((Some(Sending::Held { .. }), ..)) => vec![("u", "back to Drafts"), keymap::hint("global.help")],
            Some(_) if search => keys(&["list.open", "list.archive", "list.reply", "global.back", "global.help"]),
            Some(_) if draft => [vec![("Enter", "edit")], keys(&["list.trash", "global.help"])].concat(),
            Some((None, Some(Fold::Folded), _)) => {
                keys(&["list.open", "list.unfold", "list.archive", "list.reply", "global.help"])
            }
            Some((None, Some(Fold::Open), _) | (None, None, true)) => {
                keys(&["list.open", "list.fold", "list.archive", "list.reply", "global.help"])
            }
            Some((None, None, false)) => {
                keys(&["list.open", "list.archive", "list.trash", "list.reply", "global.help"])
            }
        }
    };
    hint(&entries)
}

/// A panel takes the rows above the status line: a blank row, a title, its
/// entries, with the cursor and the fill on the selected one.
pub fn panel(title: &str, entries: Vec<Line>, selected: Option<usize>, width: usize) -> Vec<Line> {
    let mut out = vec![Line::new(), heading(vec![run(title, Tok::Fg).bold()])];
    for (i, entry) in entries.into_iter().enumerate() {
        let on = selected == Some(i);
        let mut line = vec![if on { run(CURSOR, Tok::Acc) } else { gap(1) }, gap(INDENT - 1)];
        line.extend(entry);
        let line = fit(&line, width);
        out.push(if on { fill(line, Tok::Sel) } else { line });
    }
    out
}

/// The command line while something is typed on it: the text, then the
/// caret.
pub fn typed(text: &str) -> Line {
    vec![gap(MARGIN), run(text, Tok::Fg), run(" ", Tok::Bg).on(Tok::Acc)]
}

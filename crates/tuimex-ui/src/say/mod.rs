//! What the command line says: the design's `SAY`, in its one grammar. A
//! mark only when there is something to mark, what happened in plain words,
//! then a detail or the next key, dim, after a dot.
//!
//! Spans are built directly and never through markup: a folder's name and a
//! server's reason are not this program's text.

use crate::glyphs::{DOT, ELLIPSIS, OK};
use crate::keymap;
use crate::layout::MARGIN;
use crate::list::heading;
use crate::palette::Tok;
use crate::span::{Line, cells, fit_middle, run};
use crate::spot::{Spot, Target};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    head: Line,
    /// A file's path that ends the head. It is the one part a line too
    /// long for the screen may lose the middle of: what was made is named
    /// by how the path ends.
    path: Option<String>,
    tail: Option<String>,
    /// The tail is the next key to press, which `:messages` has no use for.
    key: bool,
}

impl Said {
    /// A detail that is part of what happened.
    pub fn then(self, detail: impl Into<String>) -> Said {
        Said { tail: Some(detail.into()), key: false, ..self }
    }

    /// The next useful key.
    pub fn key(self, hint: impl Into<String>) -> Said {
        Said { tail: Some(hint.into()), key: true, ..self }
    }

    fn with_tail(&self, path: Option<String>) -> Line {
        let mut line = self.head.clone();
        line.extend(path.map(|path| run(path, Tok::Fg)));
        line.extend(self.tail.iter().map(|tail| run(format!(" {DOT} {tail}"), Tok::Dim)));
        line
    }

    /// The command line, `width` cells wide.
    pub fn line(&self, width: usize) -> Line {
        let rest = cells(&heading(self.with_tail(None)));
        heading(self.with_tail(self.path.as_deref().map(|path| elided(path, width.saturating_sub(rest)))))
    }

    /// An entry of `:messages`.
    pub fn logged(&self) -> Line {
        if self.key { self.head.clone() } else { self.with_tail(self.path.clone()) }
    }
}

/// `path` in `room` cells: whole when it fits, and otherwise its start, an
/// ellipsis and its file name, which is given up last.
fn elided(path: &str, room: usize) -> String {
    let width = |text: &str| cells(&[run(text, Tok::Fg)]);
    if width(path) <= room {
        return path.to_string();
    }
    let name = &path[path.rfind('/').unwrap_or(0)..];
    let mut start = String::new();
    for c in path[..path.len() - name.len()].chars() {
        if width(&start) + width(name) + 1 + width(c.encode_utf8(&mut [0; 4])) > room {
            break;
        }
        start.push(c);
    }
    format!("{start}{ELLIPSIS}{name}")
}

/// Something done.
pub fn ok(text: impl Into<String>) -> Said {
    let head = vec![run(OK, Tok::Ok), run(format!(" {}", text.into()), Tok::Fg)];
    Said { head, path: None, tail: None, key: false }
}

/// A file written at `path`.
pub fn saved(path: String) -> Said {
    Said { path: Some(path), ..ok("Saved ") }
}

/// Something failed or refused. `text` names the command first.
pub fn err(text: impl Into<String>) -> Said {
    Said { head: vec![run(format!("E {}", text.into()), Tok::Err)], path: None, tail: None, key: false }
}

/// Something under way.
pub fn plain(text: impl Into<String>) -> Said {
    Said { head: vec![run(text, Tok::Fg)], path: None, tail: None, key: false }
}

/// `1 message`, `3 messages`.
pub fn count(n: usize, what: &str) -> String {
    if n == 1 { format!("1 {what}") } else { format!("{n} {what}s") }
}

/// A question and the keys that answer it, with what each does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub ask: Line,
    pub keys: Vec<(String, String)>,
}

impl Question {
    /// The command line: the keys always show, and a question too wide for
    /// the row loses its middle.
    pub fn line(&self, width: usize) -> Line {
        self.drawn(width).0
    }

    /// The command line, and the cells of each choice on it.
    pub fn drawn(&self, width: usize) -> (Line, Vec<Spot>) {
        let gap = || vec![run("  ", Tok::Dim)];
        let keys = cells(&choices(gap(), &self.keys).0);
        let ask = fit_middle(&heading(self.ask.clone()), width.saturating_sub(keys + MARGIN));
        choices([ask, gap()].concat(), &self.keys)
    }
}

/// `line`, then each key and what it does, parted by dots; with them the
/// cells each pair takes, which a press there answers with that key.
pub fn choices<K: AsRef<str>, D: AsRef<str>>(mut line: Line, keys: &[(K, D)]) -> (Line, Vec<Spot>) {
    let mut spots = Vec::new();
    for (i, (key, does)) in keys.iter().enumerate() {
        if i > 0 {
            line.push(run(format!(" {DOT} "), Tok::Dim));
        }
        let from = cells(&line);
        line.extend([run(format!("{} ", key.as_ref()), Tok::Fg), run(does.as_ref(), Tok::Dim)]);
        if let [one] = keymap::presses(key.as_ref())[..] {
            spots.push(Spot { row: 0, cols: from..cells(&line), target: Target::Answer(one) });
        }
    }
    (line, spots)
}

/// `ask`, answered by `keys`.
pub fn asked(ask: Line, keys: &[(&str, &str)]) -> Question {
    Question { ask, keys: keys.iter().map(|(key, does)| (key.to_string(), does.to_string())).collect() }
}

pub const YES_NO: [(&str, &str); 2] = [("y", "yes"), ("n", "no")];

/// A question, then its two keys.
pub fn ask(question: impl Into<String>) -> Question {
    asked(vec![run(question, Tok::Fg)], &YES_NO)
}

/// The screen before the first frame of the list, when there is no list to
/// draw yet: what is happening, and with `rebuilt` how far the index has
/// come, in messages of how many.
pub fn loading(rebuilt: Option<(usize, usize)>) -> Vec<Line> {
    let count = |n: usize| crate::list::thousands(u32::try_from(n).unwrap_or(u32::MAX));
    let progress = |(done, total)| format!("Rebuilding the index {DOT} {} of {}", count(done), count(total));
    let mut lines = vec![Line::new(), heading(vec![run("Opening mail", Tok::Fg)])];
    lines.extend(rebuilt.map(|rebuilt| heading(vec![run(progress(rebuilt), Tok::Dim)])));
    lines
}

/// What `i` asks over an invitation, in the reader and in the calendar:
/// each answer by its key in the keymap's `invite` table.
pub fn answer() -> Question {
    let ids = ["invite.accept", "invite.tentative", "invite.decline", "invite.propose", "invite.ignore"];
    let mut keys: Vec<(&str, &str)> = ids.iter().map(|id| keymap::hint(id)).collect();
    keys.push(("Esc", "cancel"));
    asked(vec![run("Answer?", Tok::Fg)], &keys)
}

/// What an event that repeats asks before it is deleted or changed: this
/// day's, or every day's. The first two keys are those answers, in that
/// order.
pub fn which_ones(verb: &str) -> Question {
    asked(vec![run(format!("{verb} which?"), Tok::Fg)], &[("t", "this one"), ("a", "all of them"), ("Esc", "cancel")])
}

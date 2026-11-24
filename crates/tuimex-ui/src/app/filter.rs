//! `/` over a long mailbox: what is typed is asked of the open section
//! once for each key, and only of the messages the text before that key
//! matched; a frame and the cursor are then worked out from the threads
//! that hold a match, without a row being made for any thread out of
//! sight.

use std::rc::Rc;

use tuimex_core::{AccountId, MessageId, Query};

use super::{App, Shown};
use crate::list::Row;
use crate::mailbox::narrows;

/// What one filter matches in the open section.
#[derive(Debug)]
pub(super) struct Level {
    query: Query,
    /// Each message matched, as its thread's place in the section and its
    /// own in the mail, in the order of the rows.
    hits: Rc<Vec<(usize, usize)>>,
    /// How many threads those are of.
    threads: usize,
}

/// What the filter matched as it was typed, each level asked of the one
/// before: a letter more looks through what the last matched, and a letter
/// erased is a level already here.
#[derive(Debug, Default)]
pub(super) struct Sifted {
    /// The mail's version and size, the tab and the account narrowed to.
    of: (u64, usize, usize, Option<AccountId>),
    levels: Vec<Rc<Level>>,
}

impl Level {
    /// The rows of a thread that match. Folded, a thread is one row, which
    /// matches when any message of it does.
    fn rows(&self, shown: &Shown, thread: usize) -> Vec<usize> {
        let from = self.hits.partition_point(|hit| hit.0 < thread);
        let hits = &self.hits[from..from + self.hits[from..].partition_point(|hit| hit.0 == thread)];
        let first = shown.row_of(thread);
        match shown.unfolded.iter().any(|(at, _)| *at == thread) {
            _ if hits.is_empty() => Vec::new(),
            false => vec![first],
            true => {
                let members = shown.threads[thread].members.iter().enumerate();
                members.filter(|(_, member)| hits.iter().any(|hit| hit.1 == member.0)).map(|(n, _)| first + n).collect()
            }
        }
    }

    /// How many rows match.
    pub fn count(&self, shown: &Shown) -> usize {
        let unfolded = shown.unfolded.iter().map(|(at, _)| self.rows(shown, *at).len().saturating_sub(1));
        self.threads + unfolded.sum::<usize>()
    }

    /// Which of `rows` match, by the message each opens; `first` is the
    /// number of the first of them.
    pub fn among(&self, shown: &Shown, first: usize, rows: &[Row]) -> Vec<MessageId> {
        let Some(last) = (first + rows.len()).checked_sub(1) else { return Vec::new() };
        let threads = shown.thread_of(first)..=shown.thread_of(last);
        let hit = threads.flat_map(|thread| self.rows(shown, thread));
        hit.filter_map(|row| Some(rows.get(row.checked_sub(first)?)?.msg.envelope.id.clone())).collect()
    }

    /// The first matching row at or after `from`, going round the end of
    /// the list.
    pub fn next(&self, shown: &Shown, from: usize) -> Option<usize> {
        let here = shown.thread_of(from);
        let after = self.hits.partition_point(|hit| hit.0 <= here);
        let later =
            || self.hits.get(after).or(self.hits.first()).and_then(|hit| self.rows(shown, hit.0).first().copied());
        self.rows(shown, here).into_iter().find(|row| *row >= from).or_else(later)
    }
}

impl App {
    /// What the filter matches in the open section, and the section's
    /// threads; nothing while no filter is typed, or the outbox or a
    /// search is shown.
    pub(super) fn sifted(&self) -> Option<(Rc<Level>, Shown)> {
        let (filter, section) = (self.filter.as_deref()?, self.sectioned()?);
        let shown = self.shown(self.tab(), section);
        let of = (self.version, self.mailbox.mail.len(), self.tab(), self.account.clone());
        let mut kept = self.sifted.borrow_mut();
        if kept.of != of {
            *kept = Sifted { of, levels: Vec::new() };
        }
        let query = Query::parse(filter);
        if let Some(same) = kept.levels.iter().rposition(|level| level.query == query) {
            kept.levels.truncate(same + 1);
            return Some((kept.levels[same].clone(), shown));
        }
        while kept.levels.last().is_some_and(|level| !narrows(&query, &level.query)) {
            kept.levels.pop();
        }
        let matches = self.mailbox.matcher(&query);
        let hit = |hit: &(usize, usize)| matches(&self.mailbox.mail[hit.1]);
        let hits: Vec<(usize, usize)> = match kept.levels.last() {
            Some(wider) => wider.hits.iter().copied().filter(hit).collect(),
            None => {
                let threads = shown.threads.iter().enumerate();
                threads
                    .flat_map(|(at, thread)| thread.members.iter().map(move |member| (at, member.0)))
                    .filter(hit)
                    .collect()
            }
        };
        let threads = hits.chunk_by(|a, b| a.0 == b.0).count();
        drop(matches);
        let level = Rc::new(Level { query, hits: Rc::new(hits), threads });
        kept.levels.push(level.clone());
        Some((level, shown))
    }
}

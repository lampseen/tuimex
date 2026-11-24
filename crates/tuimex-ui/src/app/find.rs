//! `/` and Enter: a search of all mail, which whoever holds the store
//! carries out and answers (PLAN §6.1). Its results take the place of the
//! open section until Esc.

use std::collections::HashMap;

use tuimex_core::{Action, Hit, MessageId};

use super::App;
use crate::list::{self, Row};
use crate::say::count;

#[derive(Debug)]
pub(super) struct Search {
    pub query: String,
    /// The ask that carries it out, which its answer names.
    serial: u64,
    /// What was found, once the answer is here, and how many messages
    /// were looked through.
    hits: Option<Vec<Hit>>,
    searched: u32,
    /// The cursor and the scroll of the section it was asked from.
    back: (usize, usize),
}

impl Search {
    /// What stands where the rows would be when there are none.
    pub fn empty(&self, accounts: usize) -> [String; 2] {
        match self.hits {
            None => ["Searching all mail.".to_string(), String::new()],
            Some(_) => [
                format!("Nothing matches \"{}\".", self.query),
                format!("Searched {} messages in {}.", list::thousands(self.searched), count(accounts, "account")),
            ],
        }
    }
}

impl App {
    /// Enter on a filter: asks for `query` of all mail and shows the
    /// search at once, empty until the answer comes. Nothing typed is
    /// nothing asked.
    pub(super) fn find(&mut self, query: &str) {
        if query.is_empty() {
            return;
        }
        let back = self.search.as_ref().map_or(self.sought_from, |search| search.back);
        let serial = self.ask(Action::Search(query.to_string()), Vec::new());
        self.search = Some(Search { query: query.to_string(), serial, hits: None, searched: 0, back });
        (self.selected, self.top, self.focus, self.anchor) = (0, 0, false, None);
        self.marks.clear();
    }

    /// The answer to a search, when it is the one being shown.
    pub(super) fn found_event(&mut self, serial: u64, hits: Vec<Hit>, searched: u32) {
        if let Some(search) = self.search.as_mut().filter(|search| search.serial == serial) {
            (search.hits, search.searched) = (Some(hits), searched);
            self.selected = 0;
        }
    }

    /// Esc: back to the section the search was asked from, with the cursor
    /// where it was.
    pub(super) fn close_search(&mut self) {
        if let Some(search) = self.search.take() {
            (self.selected, self.top) = search.back;
        }
    }

    /// The rows of the search being shown: each message found that is
    /// still known here and of the account the list is narrowed to,
    /// newest first.
    pub(super) fn found(&self) -> Option<Vec<Row<'_>>> {
        let search = self.search.as_ref()?;
        let hits: HashMap<&MessageId, Option<&str>> =
            search.hits.iter().flatten().map(|hit| (&hit.id, hit.excerpt.as_deref())).collect();
        let narrowed = |id: &MessageId| self.account.as_ref().is_none_or(|a| id.account == *a);
        let mut rows: Vec<Row> = self
            .mailbox
            .mail
            .iter()
            .filter(|m| narrowed(&m.envelope.id))
            .filter_map(|m| Some(list::found(m, self.mailbox.folder_of(m), *hits.get(&m.envelope.id)?)))
            .collect();
        rows.sort_by(|a, b| b.newest.cmp(&a.newest).then_with(|| a.msg.envelope.id.key.cmp(&b.msg.envelope.id.key)));
        Some(rows)
    }
}

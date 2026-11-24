//! The query grammar of PLAN §6.1, evaluated: each term is one condition,
//! and a message matches when every condition holds.

use rusqlite::params_from_iter;
use rusqlite::types::Value;
use tuimex_core::{Envelope, Has, Is, Query, Sort, Term, Timestamp, When};

use super::{ENVELOPE, Index, envelope};
use crate::Result;

/// What free text is looked for in (PLAN §6.1).
const FREE_TEXT: &str = "{subject sender body}";

#[derive(Default)]
struct Conditions {
    sql: Vec<String>,
    values: Vec<Value>,
    /// Conditions on text, which the search table answers together.
    text: Vec<String>,
}

impl Conditions {
    fn add(&mut self, sql: &str) {
        self.sql.push(sql.to_string());
    }

    fn bind(&mut self, sql: &str, value: impl Into<Value>) {
        self.add(sql);
        self.values.push(value.into());
    }

    /// The words of `text`, in order, the last one possibly unfinished so
    /// that a filter narrows as it is typed. The text is quoted whole: no
    /// character in it is search syntax. Text with no letter or digit has
    /// nothing the search table could hold, and asks for nothing.
    fn words(&mut self, columns: &str, text: &str) {
        if text.chars().any(char::is_alphanumeric) {
            self.text.push(format!("({columns} : \"{}\" *)", text.replace('"', "\"\"")));
        }
    }

    fn term(&mut self, term: &Term, now: Timestamp) {
        let instant = |when: &When| match when {
            When::At(at) => at.0,
            When::DaysAgo(days) => now.0 - i64::from(*days) * 86_400,
        };
        match term {
            Term::In(name) => {
                self.add(
                    "(m.folder = ? COLLATE NOCASE OR EXISTS (SELECT 1 FROM folders f
                      WHERE f.account = m.account AND f.name = m.folder AND f.role = ?))",
                );
                self.values.extend([Value::from(name.clone()), Value::from(name.to_lowercase())]);
            }
            Term::Account(name) => self.bind("m.account = ?", name.clone()),
            Term::Is(is) => self.add(match is {
                Is::Unread => "m.seen = 0",
                Is::Read => "m.seen = 1",
                Is::Flagged => "m.flagged = 1",
                Is::Answered => "m.answered = 1",
                Is::Draft => "m.draft = 1",
                Is::Signed => "m.security IN ('signed', 'both')",
                Is::Encrypted => "m.security IN ('encrypted', 'both')",
                Is::AgentDraft => "m.origin = 'agent'",
            }),
            Term::Has(has) => self.add(match has {
                Has::Attachment => "m.attachments = 1",
                Has::Invite => "m.invite = 1",
                Has::Unsubscribe => "m.unsubscribe = 1",
            }),
            Term::From(text) => self.words("sender", text),
            Term::To(text) => self.words("recipients", text),
            Term::Cc(text) => self.words("cc", text),
            Term::Subject(text) => self.words("subject", text),
            Term::Text(text) => self.words(FREE_TEXT, text),
            Term::After(when) => self.bind("m.date >= ?", instant(when)),
            Term::Before(when) => self.bind("m.date < ?", instant(when)),
            // SQLite's integers are signed, and no message is that large.
            Term::Larger(bytes) => self.bind("m.size > ?", i64::try_from(*bytes).unwrap_or(i64::MAX)),
        }
    }
}

impl Index {
    /// The messages `query` matches. `now` is what `after:7d` counts back
    /// from. Messages with one date come in a fixed order, so the same
    /// query over the same mail always answers the same.
    pub fn query(&self, query: &Query, now: Timestamp, sort: Sort, limit: Option<usize>) -> Result<Vec<Envelope>> {
        let mut conditions = Conditions::default();
        for term in &query.terms {
            conditions.term(term, now);
        }
        if !conditions.text.is_empty() {
            let all = conditions.text.join(" AND ");
            conditions.bind("m.id IN (SELECT rowid FROM search WHERE search MATCH ?)", all);
        }
        conditions.add("1");
        let order = match sort {
            Sort::NewestFirst => "DESC",
            Sort::OldestFirst => "ASC",
        };
        // SQLite reads a negative limit as none.
        conditions.values.push(Value::from(limit.map_or(-1, |n| i64::try_from(n).unwrap_or(i64::MAX))));
        let sql = format!(
            "SELECT {ENVELOPE} FROM messages m WHERE {} ORDER BY m.date {order}, m.account, m.key LIMIT ?",
            conditions.sql.join(" AND ")
        );
        let mut statement = self.conn.prepare_cached(&sql)?;
        let found = statement.query_map(params_from_iter(conditions.values), envelope)?;
        Ok(found.collect::<rusqlite::Result<_>>()?)
    }
}

//! The mail the screen is drawn from, held in memory, and the query grammar
//! of PLAN §6.1 asked of it. An action changes this at once (ADR-015); what
//! carries the action out hears of it afterwards.

use tuimex_core::{
    AccountId, Address, Folder, Has, Is, Listed, Origin, Query, Role, Section, Security, Term, Timestamp, When,
};

#[derive(Debug, Clone)]
pub struct Mailbox {
    pub now: Timestamp,
    /// The tabs, in order. The outbox is not one: its tab is there while a
    /// message is waiting to go or was refused.
    pub sections: Vec<Section>,
    pub folders: Vec<Folder>,
    /// Every message known, whatever its folder.
    pub mail: Vec<Listed>,
    /// Seconds since the last sync; none until there has been one.
    pub synced: Option<u32>,
    /// The addresses the accounts receive mail at, which an invitation
    /// calls "you".
    pub me: Vec<String>,
}

/// A role as `in:` names it.
fn role_name(role: Role) -> &'static str {
    match role {
        Role::Inbox => "inbox",
        Role::Sent => "sent",
        Role::Drafts => "drafts",
        Role::Archive => "archive",
        Role::Trash => "trash",
        Role::Junk => "junk",
        Role::Outbox => "outbox",
        Role::Other => "other",
    }
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty())
}

/// The words of a term's text, folded to lower case: what `has` looks for.
fn wanted(needle: &str) -> Vec<String> {
    words(needle).map(str::to_lowercase).collect()
}

/// Whether `word`, whatever its case, begins with `lower`; and with
/// `whole`, is it.
fn begins(word: &str, lower: &str, whole: bool) -> bool {
    let (mut word, mut lower) = (word.chars().flat_map(char::to_lowercase), lower.chars());
    loop {
        match (lower.next(), word.next()) {
            (None, rest) => return !whole || rest.is_none(),
            (Some(a), Some(b)) if a == b => {}
            _ => return false,
        }
    }
}

/// Whether `text` holds the words `wanted` next to each other and in
/// order, the last possibly unfinished, whatever their case: what the
/// index's search answers for the same term (PLAN §6.1), so that a filter
/// typed here and a query asked of the store find the same messages. No
/// words to look for are found anywhere.
fn has(text: &str, wanted: &[String]) -> bool {
    let Some((last, whole)) = wanted.split_last() else { return true };
    let mut run = 0;
    for word in words(text) {
        if run == whole.len() && begins(word, last, false) {
            return true;
        }
        // A word that breaks a run may itself begin the next one.
        run = match whole.get(run) {
            Some(next) if begins(word, next, true) => run + 1,
            _ => usize::from(whole.first().is_some_and(|first| begins(word, first, true))),
        };
    }
    false
}

fn named(people: &[Address], wanted: &[String]) -> bool {
    people.iter().any(|a| has(&a.email, wanted) || a.name.as_deref().is_some_and(|name| has(name, wanted)))
}

/// Whether every message `new` can match is one `old` matches, as far as
/// their terms alone say: true of a filter and the same filter a key
/// before, unless that key finished a term or began another kind of one.
pub(crate) fn narrows(new: &Query, old: &Query) -> bool {
    let text = |term: &Term| match term {
        Term::From(text) | Term::To(text) | Term::Cc(text) | Term::Subject(text) | Term::Text(text) => {
            Some((std::mem::discriminant(term), wanted(text)))
        }
        _ => None,
    };
    // What `has` finds for some words it finds for the same words with
    // the last made longer or more put after it.
    let longer = |now: &[String], was: &[String]| match was.split_last() {
        None => true,
        Some((last, whole)) => {
            now.len() >= was.len() && now[..whole.len()] == *whole && now[whole.len()].starts_with(last.as_str())
        }
    };
    old.terms.iter().all(|was| {
        new.terms.contains(was)
            || text(was).is_some_and(|(kind, was)| {
                was.is_empty() || new.terms.iter().filter_map(text).any(|(k, now)| k == kind && longer(&now, &was))
            })
    })
}

impl Mailbox {
    /// The account's folder with that role.
    pub fn folder(&self, account: &AccountId, role: Role) -> Option<&Folder> {
        self.folders.iter().find(|f| f.account == *account && f.role == role)
    }

    /// The folder a message is in, when the account lists it.
    pub fn folder_of(&self, m: &Listed) -> Option<&Folder> {
        self.folders.iter().find(|f| f.account == m.envelope.id.account && f.id == m.envelope.folder)
    }

    /// Whether every term of `query` holds for `m`.
    pub fn matches(&self, m: &Listed, query: &Query) -> bool {
        self.matcher(query)(m)
    }

    /// `matches` for one query asked of many messages: what the query
    /// looks for is worked out once.
    pub fn matcher<'a>(&'a self, query: &'a Query) -> impl Fn(&Listed) -> bool + 'a {
        let instant = |when: &When| match when {
            When::At(at) => at.0,
            When::DaysAgo(days) => self.now.0 - i64::from(*days) * 86_400,
        };
        let texts: Vec<Vec<String>> = query
            .terms
            .iter()
            .map(|term| match term {
                Term::From(text) | Term::To(text) | Term::Cc(text) | Term::Subject(text) | Term::Text(text) => {
                    wanted(text)
                }
                _ => Vec::new(),
            })
            .collect();
        move |m: &Listed| {
            let e = &m.envelope;
            query.terms.iter().zip(&texts).all(|(term, text)| match term {
                Term::In(name) => {
                    e.folder.0.eq_ignore_ascii_case(name)
                        || self.folder_of(m).is_some_and(|f| role_name(f.role).eq_ignore_ascii_case(name))
                }
                Term::Account(name) => e.id.account.0 == *name,
                Term::Is(is) => match is {
                    Is::Unread => !e.flags.seen,
                    Is::Read => e.flags.seen,
                    Is::Flagged => e.flags.flagged,
                    Is::Answered => e.flags.answered,
                    Is::Draft => e.flags.draft,
                    Is::Signed => matches!(e.security, Security::Signed | Security::Both),
                    Is::Encrypted => matches!(e.security, Security::Encrypted | Security::Both),
                    Is::AgentDraft => e.origin == Origin::Agent,
                },
                Term::Has(Has::Attachment) => e.has_attachments || m.attachments.iter().any(|a| !a.inline),
                Term::Has(Has::Invite) => m.invite,
                Term::Has(Has::Unsubscribe) => m.unsubscribe,
                Term::From(_) => named(&e.from, text),
                Term::To(_) => named(&e.to, text),
                Term::Cc(_) => named(&e.cc, text),
                Term::Subject(_) => has(&e.subject, text),
                Term::Text(_) => {
                    has(&e.subject, text)
                        || named(&e.from, text)
                        || has(&m.snippet, text)
                        || m.text.as_deref().is_some_and(|body| has(body, text))
                }
                Term::After(when) => e.date.0 >= instant(when),
                Term::Before(when) => e.date.0 < instant(when),
                Term::Larger(bytes) => u64::from(e.size) > *bytes,
            })
        }
    }
}

#[cfg(test)]
mod tests;

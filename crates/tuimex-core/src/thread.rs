//! Conversations: a reply sits with what it answers (PLAN §3).

use std::borrow::Borrow;
use std::collections::HashMap;

use crate::{Envelope, Thread, ThreadNode};

#[cfg(test)]
mod tests;

/// How long after a message a reply with no usable `References` is still
/// taken to answer it, on its subject alone. Without a limit every "Re:
/// Invoice" a mailbox ever held would be one conversation.
const SUBJECT_WINDOW: i64 = 30 * 24 * 60 * 60;

/// Groups by `References` and `In-Reply-To`, then by subject within a window.
/// Threads come out in the order their roots came in; replies under a
/// message are oldest first.
pub fn thread(envelopes: &[Envelope]) -> Vec<Thread> {
    let id = |i: usize| envelopes[i].id.clone();
    let node = |&(i, depth): &(usize, u16)| ThreadNode { id: id(i), depth };
    let named =
        |members: Vec<(usize, u16)>| Thread { root: id(members[0].0), members: members.iter().map(node).collect() };
    thread_at(envelopes).into_iter().map(named).collect()
}

/// As `thread`, naming each message by its place in `envelopes` beside its
/// depth: for a caller with tens of thousands of them, which a copy of
/// every id would slow. No thread is empty, and its root is first.
pub fn thread_at<E: Borrow<Envelope>>(envelopes: &[E]) -> Vec<Vec<(usize, u16)>> {
    let envelopes: Vec<&Envelope> = envelopes.iter().map(Borrow::borrow).collect();
    let subjects: Vec<(String, bool)> = envelopes.iter().map(|e| base_subject(&e.subject)).collect();
    let mut by_message_id: HashMap<&str, usize> = HashMap::new();
    let mut by_subject: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, e) in envelopes.iter().enumerate() {
        if let Some(id) = &e.message_id {
            by_message_id.entry(id).or_insert(i);
        }
        by_subject.entry(&subjects[i].0).or_default().push(i);
    }

    let mut parent: Vec<Option<usize>> = vec![None; envelopes.len()];
    for (i, e) in envelopes.iter().enumerate() {
        // The nearest ancestor that is actually here: the last reference is
        // the message answered, and the ones before it are its ancestors.
        let referenced = e.references.iter().rev().filter_map(|r| by_message_id.get(r.as_str()).copied());
        parent[i] = referenced.clone().find(|&p| !would_loop(&parent, p, i));
        if parent[i].is_some() {
            continue;
        }
        let (base, is_reply) = &subjects[i];
        if !is_reply || base.is_empty() {
            continue;
        }
        parent[i] = by_subject
            .get(base.as_str())
            .into_iter()
            .flatten()
            .copied()
            .filter(|&j| {
                let age = e.date.0.saturating_sub(envelopes[j].date.0);
                j != i && (0..=SUBJECT_WINDOW).contains(&age) && !would_loop(&parent, j, i)
            })
            .max_by_key(|&j| envelopes[j].date);
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); envelopes.len()];
    for (i, p) in parent.iter().enumerate() {
        if let Some(p) = *p {
            children[p].push(i);
        }
    }
    for replies in &mut children {
        // Newest first here, because the walk below pops from the end.
        replies.sort_by_key(|&i| std::cmp::Reverse((envelopes[i].date, i)));
    }

    let roots = parent.iter().enumerate().filter(|(_, p)| p.is_none());
    roots
        .map(|(root, _)| {
            let mut members = Vec::new();
            // A stack, not recursion: the depth of a thread is the sender's
            // to choose.
            let mut stack = vec![(root, 0u16)];
            while let Some((i, depth)) = stack.pop() {
                members.push((i, depth));
                stack.extend(children[i].iter().map(|&c| (c, depth.saturating_add(1))));
            }
            members
        })
        .collect()
}

/// Whether making `candidate` the parent of `child` would close a loop,
/// which two messages that each cite the other would otherwise do.
fn would_loop(parent: &[Option<usize>], candidate: usize, child: usize) -> bool {
    let mut at = Some(candidate);
    while let Some(i) = at {
        if i == child {
            return true;
        }
        at = parent[i];
    }
    false
}

/// The subject without its reply and forward prefixes, folded for
/// comparison, and whether it had any.
fn base_subject(subject: &str) -> (String, bool) {
    let mut rest = subject.trim();
    let mut is_reply = false;
    loop {
        let begins = |p: &&str| rest.get(..p.len()).is_some_and(|head| head.eq_ignore_ascii_case(p));
        let Some(prefix) = ["re:", "fwd:", "fw:", "aw:"].into_iter().find(begins) else { break };
        rest = rest[prefix.len()..].trim_start();
        is_reply = true;
    }
    (rest.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase(), is_reply)
}

use std::collections::HashSet;

use tuimex_core::thread;

use super::{FILTER, HALDEN, MARKED, NOW, UNFOLDED, arrived, filed, inbox, outbox};

/// CLAUDE.md: fixtures are synthetic.
#[test]
fn every_address_is_under_a_reserved_example_domain() {
    for mail in inbox().into_iter().chain(outbox()).chain(filed()) {
        let e = &mail.envelope;
        let mut hosts: Vec<&str> =
            e.from.iter().chain(&e.to).chain(&e.cc).filter_map(|a| a.email.split('@').nth(1)).collect();
        hosts.extend(mail.sender_was.iter().chain(&e.message_id).filter_map(|a| a.split('@').nth(1)));
        assert!(!hosts.is_empty());
        for host in hosts {
            let host = host.trim_end_matches('>');
            assert!(
                ["example.com", "example.org", "example.net"]
                    .iter()
                    .any(|d| host == *d || host.ends_with(&format!(".{d}"))),
                "{}: {host} is not a reserved domain",
                e.subject
            );
        }
    }
}

#[test]
fn a_key_names_one_message_and_every_screen_s_state_names_a_message() {
    let all = inbox();
    let keys: HashSet<&str> = all.iter().map(|m| m.envelope.id.key.as_str()).collect();
    assert_eq!(keys.len(), all.len());
    for key in MARKED.iter().chain(&FILTER.1).chain(&[HALDEN, UNFOLDED.0, UNFOLDED.1]) {
        assert!(keys.contains(key), "{key} is not in the mailbox");
    }
}

/// The brief's mailbox: one thread five deep with a branch, one of three,
/// the rest single.
#[test]
fn the_threads_are_the_ones_the_design_draws() {
    let envelopes: Vec<_> = inbox().into_iter().map(|m| m.envelope).collect();
    let mut sizes: Vec<usize> = thread(&envelopes).iter().map(|t| t.members.len()).collect();
    sizes.sort_unstable();
    assert_eq!(sizes.len(), 24);
    assert_eq!(sizes[22..], [3, 5]);
    assert!(sizes[..22].iter().all(|n| *n == 1));
    let cabin = thread(&envelopes).into_iter().find(|t| t.root.key == "cabin-1").unwrap();
    let shape: Vec<(&str, u16)> = cabin.members.iter().map(|n| (n.id.key.as_str(), n.depth)).collect();
    assert_eq!(shape, [("cabin-1", 0), ("cabin-2", 1), ("cabin-3", 2), ("cabin-5", 3), ("cabin-4", 1)]);
}

#[test]
fn nothing_is_dated_after_now_and_the_first_sync_holds_less() {
    assert!(inbox().iter().chain(&outbox()).all(|m| m.envelope.date <= NOW));
    assert_eq!((inbox().len(), arrived().len()), (30, 23));
}

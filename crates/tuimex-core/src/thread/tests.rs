use super::*;
use crate::{AccountId, Flags, FolderId, MessageId, Origin, Security, Timestamp};

const DAY: i64 = 24 * 60 * 60;

/// (key, day, subject, message id, references)
type Row = (&'static str, i64, &'static str, &'static str, &'static [&'static str]);

fn envelope((key, day, subject, message_id, references): Row) -> Envelope {
    Envelope {
        id: MessageId { account: AccountId("test".into()), key: key.into() },
        folder: FolderId("INBOX".into()),
        date: Timestamp(day * DAY),
        from: Vec::new(),
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        subject: subject.into(),
        message_id: (!message_id.is_empty()).then(|| message_id.into()),
        references: references.iter().map(|r| r.to_string()).collect(),
        flags: Flags::default(),
        size: 0,
        has_attachments: false,
        security: Security::None,
        origin: Origin::Server,
    }
}

/// Each thread as `key/depth` members, space separated.
fn shape(rows: &[Row]) -> Vec<String> {
    let envelopes: Vec<Envelope> = rows.iter().copied().map(envelope).collect();
    thread(&envelopes)
        .iter()
        .map(|t| {
            assert_eq!(t.root, t.members[0].id, "the root is the first member");
            t.members.iter().map(|m| format!("{}/{}", m.id.key, m.depth)).collect::<Vec<_>>().join(" ")
        })
        .collect()
}

#[test]
fn threads_follow_references_then_subject() {
    let table: [(&str, &[Row], &[&str]); 12] = [
        (
            "unrelated messages stand alone",
            &[("a", 1, "One", "a@example.com", &[]), ("b", 2, "Two", "b@example.com", &[])],
            &["a/0", "b/0"],
        ),
        (
            "a reply sits under what it answers",
            &[("a", 1, "Plan", "a@example.com", &[]), ("b", 2, "Re: Plan", "b@example.com", &["a@example.com"])],
            &["a/0 b/1"],
        ),
        (
            "the last reference is the parent, whatever order the mail came in",
            &[
                ("c", 3, "Re: Plan", "c@example.com", &["a@example.com", "b@example.com"]),
                ("a", 1, "Plan", "a@example.com", &[]),
                ("b", 2, "Re: Plan", "b@example.com", &["a@example.com"]),
            ],
            &["a/0 b/1 c/2"],
        ),
        (
            "a missing parent falls back to the nearest ancestor present",
            &[
                ("a", 1, "Plan", "a@example.com", &[]),
                ("c", 3, "Something else", "c@example.com", &["a@example.com", "gone@example.com"]),
            ],
            &["a/0 c/1"],
        ),
        (
            "replies to one message are oldest first",
            &[
                ("a", 1, "Plan", "a@example.com", &[]),
                ("late", 5, "Re: Plan", "l@example.com", &["a@example.com"]),
                ("early", 2, "Re: Plan", "e@example.com", &["a@example.com"]),
            ],
            &["a/0 early/1 late/1"],
        ),
        (
            "two messages citing each other do not loop",
            &[("a", 1, "X", "a@example.com", &["b@example.com"]), ("b", 2, "Y", "b@example.com", &["a@example.com"])],
            &["b/0 a/1"],
        ),
        ("a message citing itself is a root", &[("a", 1, "X", "a@example.com", &["a@example.com"])], &["a/0"]),
        (
            "a reply with no references joins by subject",
            &[("a", 1, "Plan", "a@example.com", &[]), ("b", 2, "RE:  plan", "b@example.com", &[])],
            &["a/0 b/1"],
        ),
        (
            "the same subject without a reply prefix is a new conversation",
            &[("a", 1, "Invoice", "a@example.com", &[]), ("b", 2, "Invoice", "b@example.com", &[])],
            &["a/0", "b/0"],
        ),
        (
            "a subject match outside the window is a new conversation",
            &[("a", 1, "Plan", "a@example.com", &[]), ("b", 40, "Re: Plan", "b@example.com", &[])],
            &["a/0", "b/0"],
        ),
        (
            "a subject match never reaches a later message",
            &[("a", 5, "Plan", "a@example.com", &[]), ("b", 2, "Re: Plan", "b@example.com", &[])],
            &["a/0", "b/0"],
        ),
        (
            "one message in two folders is two rows under the same parent",
            &[
                ("a", 1, "Plan", "a@example.com", &[]),
                ("b1", 2, "Re: Plan", "b@example.com", &["a@example.com"]),
                ("b2", 2, "Re: Plan", "b@example.com", &["a@example.com"]),
            ],
            &["a/0 b1/1 b2/1"],
        ),
    ];
    for (name, rows, want) in table {
        assert_eq!(shape(rows), want, "{name}");
    }
}

#[test]
fn a_chain_deeper_than_the_stack_is_threaded() {
    let n = 20_000;
    let envelopes: Vec<Envelope> = (0..n)
        .map(|i| {
            let mut e = envelope(("k", i, "Chain", "", &[]));
            e.id.key = format!("k{i}");
            e.message_id = Some(format!("{i}@example.com"));
            e.references = if i == 0 { Vec::new() } else { vec![format!("{}@example.com", i - 1)] };
            e
        })
        .collect();
    let threads = thread(&envelopes);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].members.len(), 20_000);
    assert_eq!(threads[0].members[19_999].depth, 19_999);
}

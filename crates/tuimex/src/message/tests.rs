use tuimex_core::{AccountId, Address, Draft, MessageId, Origin};

use super::{Bcc, File, build, header_id, mime};

fn person(name: &str, email: &str) -> Address {
    Address { name: Some(name.to_string()).filter(|n| !n.is_empty()), email: email.to_string() }
}

fn draft() -> Draft {
    Draft {
        id: MessageId { account: AccountId("work".into()), key: "draft-1".into() },
        from: person("Robin Vale", "robin.vale@example.org"),
        to: vec![person("Raman, Priya", "priya@example.org"), person("", "dmitri@example.org")],
        cc: vec![person("Zoë Åström", "zoe@example.net")],
        bcc: vec![person("", "audit@example.com")],
        subject: "Re: Contract draft v3".into(),
        body: "Hi Priya,\n\n45 days is fine.\n".into(),
        files: Vec::new(),
        in_reply_to: Some("<halden-3@design.example.net>".into()),
        references: vec!["<halden-1@design.example.net>".into(), "halden-3@design.example.net".into()],
        origin: Origin::Local,
    }
}

fn text(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw).into_owned()
}

/// Read back by the parser every message is read with (ADR-007).
#[test]
fn a_built_message_reads_back_as_the_draft_it_was() {
    let raw = build(&draft(), Vec::new(), 1_795_443_600, "k1", Bcc::Written).unwrap();
    let head = tuimex_content::envelope(&raw).unwrap();
    assert_eq!(head.from, [person("Robin Vale", "robin.vale@example.org")]);
    assert_eq!(
        head.to,
        [person("Raman, Priya", "priya@example.org"), Address { name: None, email: "dmitri@example.org".into() }]
    );
    assert_eq!(head.cc, [person("Zoë Åström", "zoe@example.net")]);
    assert_eq!(head.subject, "Re: Contract draft v3");
    assert_eq!(head.date.map(|d| d.0), Some(1_795_443_600));
    assert_eq!(head.message_id.as_deref(), Some("k1@example.org"), "the id is at From's domain, not the machine's");
    assert_eq!(head.references, ["halden-1@design.example.net", "halden-3@design.example.net"]);
    assert!(text(&raw).contains("In-Reply-To: <halden-3@design.example.net>"), "{}", text(&raw));
    assert!(!text(&raw).contains("localhost"));
    let body = tuimex_content::body(&raw, 80).unwrap();
    assert_eq!(body.text.trim(), "Hi Priya,\n\n45 days is fine.");
    assert_eq!(body.attachments, []);
}

#[test]
fn the_copy_for_a_server_does_not_name_the_blind_copies() {
    let kept = text(&build(&draft(), Vec::new(), 0, "k", Bcc::Written).unwrap());
    let sent = text(&build(&draft(), Vec::new(), 0, "k", Bcc::Withheld).unwrap());
    assert!(kept.contains("Bcc: <audit@example.com>") || kept.contains("Bcc: audit@example.com"), "{kept}");
    assert!(!sent.contains("audit@example.com") && !sent.to_lowercase().contains("bcc"), "{sent}");
}

#[test]
fn a_message_that_answers_nothing_has_no_threading_headers() {
    let new = Draft { in_reply_to: None, references: Vec::new(), ..draft() };
    let raw = text(&build(&new, Vec::new(), 0, "k", Bcc::Withheld).unwrap());
    assert!(!raw.contains("In-Reply-To") && !raw.contains("References"), "{raw}");
    let orphan = Draft { in_reply_to: None, ..draft() };
    let raw = text(&build(&orphan, Vec::new(), 0, "k", Bcc::Withheld).unwrap());
    assert!(raw.contains("In-Reply-To: <halden-3@design.example.net>"), "the last reference is the parent: {raw}");
}

#[test]
fn files_are_attached_under_their_names_with_a_type_by_extension() {
    let files = vec![
        File { name: "notes.txt".into(), bytes: b"twelve bytes".to_vec() },
        File { name: "data.bin".into(), bytes: vec![0, 159, 255] },
    ];
    let raw = build(&draft(), files, 0, "k", Bcc::Withheld).unwrap();
    let body = tuimex_content::body(&raw, 80).unwrap();
    let found: Vec<(&str, &str, u32)> =
        body.attachments.iter().map(|a| (a.name.as_str(), a.mime.as_str(), a.size)).collect();
    assert_eq!(found, [("notes.txt", "text/plain", 12), ("data.bin", "application/octet-stream", 3)]);
    assert_eq!(body.text.trim(), "Hi Priya,\n\n45 days is fine.", "the text is still the body");
    assert_eq!(
        (mime("a.PDF"), mime("photo.jpeg"), mime("Makefile")),
        ("application/pdf", "image/jpeg", "application/octet-stream")
    );
}

/// ADR-015, at the last place it can be checked.
#[test]
fn what_a_header_cannot_carry_is_refused_by_name() {
    let said = |draft: Draft| build(&draft, Vec::new(), 0, "k", Bcc::Written).unwrap_err();
    let bad = |email: &str| vec![person("", email)];
    assert_eq!(said(Draft { to: bad("priya@"), ..draft() }), "\"priya@\" is not an address");
    assert_eq!(
        said(Draft { cc: bad("a@example.org\r\nBcc: eve@example.net"), ..draft() }),
        "\"a@example.org\\r\\nBcc: eve@example.net\" is not an address"
    );
    assert_eq!(said(Draft { bcc: bad("x y@example.org"), ..draft() }), "\"x y@example.org\" is not an address");
    assert_eq!(said(Draft { from: person("Robin", "nobody"), ..draft() }), "\"nobody\" is not an address");
    let named = vec![person("Priya\r\nBcc: eve@example.net", "priya@example.org")];
    assert_eq!(said(Draft { to: named, ..draft() }), "\"priya@example.org\" is not an address");
    let subject = "Hello\r\nBcc: eve@example.net".to_string();
    assert_eq!(said(Draft { subject, ..draft() }), "the subject has a control character in it");
}

#[test]
fn an_id_that_would_break_out_of_its_header_is_not_written() {
    for (id, expected) in [
        ("<1.2@example.com>", Some("1.2@example.com")),
        ("1.2@example.com", Some("1.2@example.com")),
        (" <1.2@example.com> ", Some("1.2@example.com")),
        ("1.2@example.com>\r\nBcc: mallory@example.net", None),
        ("a b@example.com", None),
        ("a<b>@example.com", None),
        ("<>", None),
        ("", None),
    ] {
        assert_eq!(header_id(id).as_deref(), expected, "{id:?}");
    }
    let hostile =
        Draft { in_reply_to: Some("x@example.com>\r\nBcc: eve@example.net".into()), references: Vec::new(), ..draft() };
    let raw = text(&build(&hostile, Vec::new(), 0, "k", Bcc::Withheld).unwrap());
    assert!(!raw.contains("eve@example.net") && !raw.contains("In-Reply-To"), "{raw}");
}

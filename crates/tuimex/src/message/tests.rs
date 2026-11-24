use std::collections::BTreeMap;

use tuimex_core::calendar::{DAY, Event, Guest, Method, Moment, Said, days_from_civil};
use tuimex_core::invitation::answer;
use tuimex_core::{AccountId, Address, Answer, CalendarPart, Draft, MessageId, Origin};
use tuimex_engine::invitation::{About, draft as answer_draft};

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
        calendar: None,
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

/// The bytes of a draft with no calendar part, as they were before a draft
/// could carry one.
#[test]
fn a_draft_without_a_calendar_part_is_built_byte_for_byte_as_before() {
    let raw = build(&draft(), Vec::new(), 1_795_443_600, "k1", Bcc::Written).unwrap();
    let want = "Message-ID: <k1@example.org>\r\nDate: Mon, 23 Nov 2026 14:20:00 +0000\r\n\
                From: \"Robin Vale\" <robin.vale@example.org>\r\n\
                To: \"Raman, Priya\" <priya@example.org>, <dmitri@example.org>\r\n\
                Subject: Re: Contract draft v3\r\nCc: =?utf-8?B?Wm/DqyDDhXN0csO2bQ==?= <zoe@example.net>\r\n\
                Bcc: <audit@example.com>\r\nIn-Reply-To: <halden-3@design.example.net>\r\n\
                References: <halden-1@design.example.net> <halden-3@design.example.net>\r\nMIME-Version: 1.0\r\n\
                Content-Type: text/plain; charset=\"utf-8\"\r\nContent-Transfer-Encoding: 7bit\r\n\r\n\
                Hi Priya,\r\n\r\n45 days is fine.\r\n";
    assert_eq!(text(&raw), want);
}

/// An invitation whose title is long and not ASCII, as a calendar from
/// anywhere may send.
fn invitation() -> Event {
    let summary: String = "Årsmøte – planlegging ✓ ".chars().cycle().take(300).collect();
    let start = days_from_civil(2026, 12, 1) * DAY + 9 * 3_600;
    Event {
        uid: "planning-2026@example.com".into(),
        sequence: 0,
        instead_of: None,
        summary: summary.trim().to_string(),
        start: Moment::Utc(start),
        end: Some(Moment::Utc(start + 3_600)),
        location: Some("Møterom 4".into()),
        description: None,
        organizer: Some(person("Ada Example", "ada@example.com")),
        guests: vec![Guest { who: person("Robin Vale", "robin.vale@example.org"), said: Said::Nothing }],
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

/// The header block of a message.
fn head_of(raw: &str) -> &str {
    raw.split("\r\n\r\n").next().unwrap_or("")
}

/// Each method, alone and beside a file: the calendar text arrives as it
/// was given, under the type a calendar reads, and the text is still the
/// body.
#[test]
fn a_calendar_part_is_sent_beside_the_text_and_read_back_byte_for_byte() {
    let (event, zones, stamp) = (invitation(), BTreeMap::new(), 1_795_443_600);
    let me = person("Robin Vale", "robin.vale@example.org");
    let proposed = Moment::Utc(event.start.wall() + DAY);
    let texts = [
        (Method::Request, "REQUEST", tuimex_content::request(&event, &zones, stamp)),
        (Method::Reply, "REPLY", tuimex_content::reply(&event, &zones, &me, Said::Accepted, stamp).unwrap()),
        (Method::Counter, "COUNTER", tuimex_content::counter(&event, &zones, &me, proposed, None, stamp).unwrap()),
        (Method::Cancel, "CANCEL", tuimex_content::cancel(&event, &zones, stamp)),
    ];
    for (method, word, calendar) in texts {
        assert!(calendar.contains("\r\n ") && calendar.len() > 600, "folded, long and CRLF: {calendar}");
        for with_file in [false, true] {
            let carrying = Draft {
                subject: format!("{word}: {}", event.summary),
                body: "Svar på invitasjonen – takk.\n".into(),
                calendar: Some(CalendarPart { method, text: calendar.clone() }),
                ..draft()
            };
            let files = if with_file {
                vec![File { name: "agenda.txt".into(), bytes: b"1. budget\n".to_vec() }]
            } else {
                Vec::new()
            };
            let raw = build(&carrying, files, stamp, "k", Bcc::Withheld).unwrap();
            let shown = text(&raw);
            let outer = if with_file { "Content-Type: multipart/mixed" } else { "Content-Type: multipart/alternative" };
            assert!(head_of(&shown).contains(outer), "{word} {with_file}: {shown}");
            if with_file {
                assert!(shown.contains("Content-Type: multipart/alternative"), "{shown}");
            }
            let typed = format!(
                "\r\nContent-Type: text/calendar; method={word}; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n"
            );
            assert_eq!(shown.matches(typed.as_str()).count(), 1, "{word}: {shown}");
            assert!(shown.find("text/plain").unwrap() < shown.find("text/calendar").unwrap(), "the text comes first");

            let part = tuimex_content::calendar_part(&raw).unwrap().unwrap();
            assert_eq!((part.method, part.text.as_bytes()), (method, calendar.as_bytes()), "{word} {with_file}");
            let body = tuimex_content::body(&raw, 80).unwrap();
            assert_eq!(body.text.trim(), "Svar på invitasjonen – takk.");
            let names: Vec<&str> = body.attachments.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(names.contains(&"agenda.txt"), with_file, "{names:?}");
            assert_eq!(tuimex_content::envelope(&raw).unwrap().subject, carrying.subject);
            if method == Method::Request {
                let card = body.invite.unwrap();
                assert_eq!((card.uid.as_str(), card.summary.as_str()), (event.uid.as_str(), event.summary.as_str()));
            }
        }
    }
}

/// The answer as the engine hands it to the builder: from the invited
/// identity, to the organizer alone, threaded under the invitation, with a
/// subject the event's hostile title cannot add a header to.
#[test]
fn an_answer_is_built_as_one_message_to_the_organizer() {
    let mut event = invitation();
    event.summary = "Lunch\r\nBcc: eve@example.net\r\n\r\nforged body".into();
    event.guests.extend((0..40).map(|n| Guest { who: person("", &format!("g{n}@example.net")), said: Said::Nothing }));
    let identities = [person("", "work@example.org"), person("Robin Vale", "robin.vale@example.org")];
    let answering = answer(&event, &identities, &Answer::Tentative).unwrap();
    let zones = BTreeMap::new();
    let about = About { event: &event, zones: &zones, answers: Some("<invite-1@example.com>"), stamp: 0 };
    let carrying = answer_draft("work", answering.mail.as_ref().unwrap(), &about).unwrap();
    let raw = build(&carrying, Vec::new(), 0, "k", Bcc::Withheld).unwrap();
    let shown = text(&raw);

    let head = tuimex_content::envelope(&raw).unwrap();
    assert_eq!(head.from, [person("Robin Vale", "robin.vale@example.org")]);
    assert_eq!(head.to, [person("Ada Example", "ada@example.com")]);
    assert_eq!((head.cc.len(), head.bcc.len()), (0, 0));
    assert_eq!(head.subject, "Tentative: Lunch Bcc: eve@example.net forged body");
    assert!(!shown.lines().any(|l| l.to_ascii_lowercase().starts_with("bcc")), "{shown}");
    assert!(shown.contains("In-Reply-To: <invite-1@example.com>"), "{shown}");
    let sending = tuimex_content::submission(&raw).unwrap();
    assert_eq!(sending.recipients, [person("Ada Example", "ada@example.com")]);
    let part = tuimex_content::calendar_part(&raw).unwrap().unwrap();
    assert_eq!(part.method, Method::Reply);
    assert!(part.text.contains("PARTSTAT=TENTATIVE:mailto:robin.vale@example.org"), "{}", part.text);
}

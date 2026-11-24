use tuimex_core::Address;

use super::*;

fn address(name: Option<&str>, email: &str) -> Address {
    Address { name: name.map(str::to_string), email: email.to_string() }
}

#[test]
fn a_name_is_reduced_to_a_file_name() {
    for (name, want) in [
        ("report.pdf", "report.pdf"),
        ("../../etc/passwd", "passwd"),
        ("/etc/cron.d/job", "job"),
        ("C:\\Users\\ada\\run.bat", "run.bat"),
        ("..", "attachment"),
        ("...", "attachment"),
        ("", "attachment"),
        ("dir/", "attachment"),
        (".bashrc", "bashrc"),
        ("--help", "help"),
        ("~root", "root"),
        ("  spaced   out .txt ", "spaced out .txt"),
        ("trailing.", "trailing"),
        ("new\nline\t.txt", "new line .txt"),
        ("bell\u{7}.txt", "bell.txt"),
        ("invoice\u{202e}fdp.exe", "invoicefdp.exe"),
        ("r\u{e9}sum\u{e9}.pdf", "r\u{e9}sum\u{e9}.pdf"),
    ] {
        assert_eq!(safe_name(name), want, "{name:?}");
    }
    let long = safe_name(&"\u{e9}".repeat(300));
    assert!(long.len() <= MAX_NAME && long.chars().all(|c| c == '\u{e9}'), "cut on a character boundary");
}

#[test]
fn plain_text_addresses_are_numbered_in_place() {
    for (text, want, urls) in [
        ("no links here", "no links here", &[][..]),
        ("see https://example.com/a.", "see https://example.com/a [1].", &["https://example.com/a"][..]),
        ("<https://example.com/a>", "<https://example.com/a [1]>", &["https://example.com/a"]),
        ("(HTTP://EXAMPLE.COM/a)", "(HTTP://EXAMPLE.COM/a [1])", &["HTTP://EXAMPLE.COM/a"]),
        (
            "https://example.com/a https://example.org/b",
            "https://example.com/a [1] https://example.org/b [2]",
            &["https://example.com/a", "https://example.org/b"],
        ),
        ("https:// alone, and https://", "https:// alone, and https://", &[]),
        (
            "caf\u{e9} https://example.com/\u{e9}",
            "caf\u{e9} https://example.com/\u{e9} [1]",
            &["https://example.com/\u{e9}"],
        ),
    ] {
        let (out, links) = linkify(text);
        assert_eq!(out, want);
        assert_eq!(links.iter().map(|l| l.url.as_str()).collect::<Vec<_>>(), urls);
    }
}

#[test]
fn headers_are_decoded_and_cleaned() {
    let raw = b"From: =?UTF-8?Q?Ren=C3=A9e_Example?= <renee@example.com>\n\
        To: Team: ada@example.com, \"Hopper, Grace\" <grace@example.org>;, bare@example.net\n\
        Cc: <copy@example.com>\n\
        Bcc: quiet@example.net\n\
        Reply-To: Desk <desk@example.org>\n\
        Subject: =?ISO-8859-1?Q?Caf=E9?=\n  and\ta fold\n\
        Date: Tue, 24 Nov 2026 09:30:00 +0100\n\
        Message-ID: <three@example.com>\n\
        References: <one@example.com> <two@example.com>\n\
        In-Reply-To: <two@example.com>\n\n\
        Body.\n";
    let head = envelope(raw).unwrap();
    assert_eq!(head.from, [address(Some("Ren\u{e9}e Example"), "renee@example.com")]);
    assert_eq!(
        head.to,
        [
            address(None, "ada@example.com"),
            address(Some("Hopper, Grace"), "grace@example.org"),
            address(None, "bare@example.net"),
        ]
    );
    assert_eq!(head.cc, [address(None, "copy@example.com")]);
    assert_eq!(head.bcc, [address(None, "quiet@example.net")]);
    assert_eq!(head.reply_to, [address(Some("Desk"), "desk@example.org")]);
    assert_eq!(head.subject, "Caf\u{e9} and a fold");
    assert_eq!(head.date, Some(Timestamp(1_795_509_000)));
    assert_eq!(head.message_id.as_deref(), Some("three@example.com"));
    assert_eq!(head.references, ["one@example.com", "two@example.com"]);
    assert!(!head.has_attachments);
}

#[test]
fn a_message_without_headers_to_speak_of_still_has_an_envelope() {
    let head = envelope(b"Subject: only this\n\n").unwrap();
    assert_eq!((head.date, head.from.len(), head.message_id), (None, 0, None));
    let head = envelope(b"Date: not a date\nFrom: Ada Example\n\n").unwrap();
    assert_eq!(head.date, None);
    assert_eq!(head.from, [address(Some("Ada Example"), "")]);
}

#[test]
fn a_header_block_alone_is_enough_for_the_list() {
    let raw = b"From: ada@example.com\nSubject: With a file\nContent-Type: multipart/mixed; boundary=\"b\"\n\n";
    let head = envelope(raw).unwrap();
    assert_eq!(head.subject, "With a file");
    assert!(head.has_attachments, "multipart/mixed with no body to inspect is taken to carry one");
    let signed = b"Subject: s\nContent-Type: multipart/signed; protocol=\"application/pgp-signature\"; boundary=b\n\n";
    assert_eq!(envelope(signed).unwrap().security, Security::Signed);
    let smime = b"Subject: s\nContent-Type: multipart/signed; protocol=\"application/pkcs7-signature\"; boundary=b\n\n";
    assert_eq!(envelope(smime).unwrap().security, Security::None);
}

#[test]
fn bytes_that_are_no_message_are_an_error() {
    assert_eq!(envelope(b"").unwrap_err(), Error::NotAMessage);
    assert_eq!(body(b"", 72).unwrap_err(), Error::NotAMessage);
    assert_eq!(attachment(b"Subject: x\n\ntext\n", 0).unwrap_err(), Error::NoSuchAttachment(0));
}

#[test]
fn a_body_in_another_charset_and_encoding_is_text() {
    let raw = b"Subject: x\nContent-Type: text/plain; charset=iso-8859-1\nContent-Transfer-Encoding: quoted-printable\n\nCaf=E9 au lait=\n, s'il vous pla=EEt.\n";
    assert_eq!(body(raw, 72).unwrap().text, "Caf\u{e9} au lait, s'il vous pla\u{ee}t.");
    let raw = b"Subject: x\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: base64\n\nSGVsbG8sIHfDtnJsZC4K\n";
    assert_eq!(body(raw, 72).unwrap().text, "Hello, w\u{f6}rld.");
}

#[test]
fn several_html_parts_number_their_links_as_one_page() {
    let raw = b"Subject: x\nContent-Type: multipart/mixed; boundary=b\n\n\
        --b\nContent-Type: text/html\n\n<a href=\"https://example.com/1\">one</a>\n\
        --b\nContent-Type: text/html\n\n<a href=\"https://example.org/2\">two</a>\n--b--\n";
    let body = body(raw, 72).unwrap();
    assert!(body.text.contains("one [1]") && body.text.contains("two [2]"), "{:?}", body.text);
    assert_eq!(
        body.links.iter().map(|l| l.url.as_str()).collect::<Vec<_>>(),
        ["https://example.com/1", "https://example.org/2"]
    );
}

#[test]
fn a_submission_names_every_recipient_and_carries_no_bcc_header() {
    let raw = b"From: Ada <ada@example.com>\r\nTo: Grace <grace@example.net>, nobody at all\r\n\
        Bcc: Hidden <hidden@example.org>,\r\n second@example.org\r\nCc: cc@example.net\r\nSubject: Bcc: not a header\r\n\
        bcc: third@example.org\r\n\r\nBcc: a line of the body\r\n";
    let submission = submission(raw).unwrap();
    assert_eq!(submission.from, [address(Some("Ada"), "ada@example.com")]);
    let emails: Vec<&str> = submission.recipients.iter().map(|a| a.email.as_str()).collect();
    let named =
        ["grace@example.net", "", "cc@example.net", "hidden@example.org", "second@example.org", "third@example.org"];
    assert_eq!(emails, named, "an entry that is no address is still there to be refused");
    assert_eq!(
        String::from_utf8(submission.message).unwrap(),
        "From: Ada <ada@example.com>\r\nTo: Grace <grace@example.net>, nobody at all\r\n\
         Cc: cc@example.net\r\nSubject: Bcc: not a header\r\n\r\nBcc: a line of the body\r\n"
    );
}

#[test]
fn a_submission_without_a_bcc_is_the_bytes_as_written() {
    let raw = b"From: ada@example.com\nTo: grace@example.net\n\nbare line feeds, as written\n";
    assert_eq!(submission(raw).unwrap().message, raw);
    assert_eq!(submission(b""), Err(Error::NotAMessage));
}

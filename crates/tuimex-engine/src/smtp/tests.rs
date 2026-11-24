use super::letter;
use crate::Error;

const OWN: &str = "ada@example.org";

fn written(from: &str, to: &str) -> Vec<u8> {
    format!("From: {from}\r\nTo: {to}\r\nSubject: a draft\r\n\r\nhello\r\n").into_bytes()
}

fn refused(raw: &[u8]) -> String {
    match letter(raw, &[OWN.to_string(), "lovelace@example.net".to_string()]) {
        Err(Error::Unsendable(why)) => why,
        other => panic!("not refused as unsendable: {other:?}"),
    }
}

#[test]
fn a_letter_goes_to_everyone_named_from_an_address_of_the_account_without_its_bcc() {
    let raw = b"From: Ada <ADA@example.org>\r\nTo: Grace <grace@example.net>\r\nCc: cc@example.com\r\n\
        Bcc: quiet@example.net\r\nSubject: a draft\r\n\r\nhello\r\n";
    let letter = letter(raw, &[OWN.to_string()]).unwrap();
    assert_eq!(letter.from, "ADA@example.org");
    assert_eq!(letter.to, ["grace@example.net", "cc@example.com", "quiet@example.net"]);
    let sent = String::from_utf8(letter.message).unwrap();
    assert!(!sent.contains("quiet@example.net") && sent.contains("Cc: cc@example.com\r\nSubject"), "{sent}");
}

#[test]
fn an_address_that_does_not_parse_is_an_error_naming_it_and_never_left_out() {
    for (to, named) in [
        ("grace@example.net, nobody at all", "\"nobody at all\" is not an address"),
        ("grace@example.net, Grace <grace at example.net>", "is not an address"),
        ("grace@example.net, broken@", "\"broken@\" is not an address"),
        ("two@@example.net", "is not an address"),
    ] {
        let why = refused(&written(OWN, to));
        assert!(why.contains(named), "{to:?}: {why}");
    }
}

#[test]
fn a_message_is_sent_only_from_one_address_of_its_own_account_and_to_somebody() {
    assert_eq!(
        refused(&written("eve@example.com", "grace@example.net")),
        "\"eve@example.com\" is not an address this account sends from"
    );
    assert_eq!(
        refused(&written("ada@example.org, lovelace@example.net", "grace@example.net")),
        "the message is not from exactly one address"
    );
    assert_eq!(refused(b"To: grace@example.net\r\n\r\nhello\r\n"), "the message is not from exactly one address");
    assert_eq!(
        refused(b"From: ada@example.org\r\nSubject: to nobody\r\n\r\nhello\r\n"),
        "the message names nobody to send it to"
    );
    assert!(matches!(letter(b"", &[]), Err(Error::Content(_))));
}

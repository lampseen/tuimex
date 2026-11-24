use super::{addresses, is_address, pieces, registrable};
use crate::Address;

fn named(name: &str, email: &str) -> Address {
    Address { name: Some(name.to_string()).filter(|n| !n.is_empty()), email: email.to_string() }
}

#[test]
fn recipients_are_read_bare_named_and_quoted() {
    let typed = r#"ada@example.org, Grace Hopper <grace@example.net>,"Lovelace, Ada" <ada@example.com>, "#;
    let want = [
        named("", "ada@example.org"),
        named("Grace Hopper", "grace@example.net"),
        named("Lovelace, Ada", "ada@example.com"),
    ];
    assert_eq!(addresses(typed).unwrap(), want);
    assert_eq!(addresses("  ").unwrap(), []);
    assert_eq!(pieces("a, \"b, c\" <d>,"), ["a", "\"b, c\" <d>", ""]);
}

/// ADR-015: one that does not parse is an error naming it.
#[test]
fn an_address_that_does_not_parse_is_an_error_naming_it() {
    let table = [
        ("ada@example.org, grace@", "grace@"),
        ("ada", "ada"),
        ("Ada <ada@example.org", "Ada <ada@example.org"),
        ("Ada <ada at example.org>", "Ada <ada at example.org>"),
        ("a@b@example.org", "a@b@example.org"),
        ("ada@example.org\u{7}", "ada@example.org\u{7}"),
        ("Ada\r\nBcc: eve@example.net <ada@example.org>", "Ada\r\nBcc: eve@example.net <ada@example.org>"),
        ("<ada@example.org> <eve@example.net>", "<ada@example.org> <eve@example.net>"),
    ];
    for (typed, bad) in table {
        assert_eq!(addresses(typed), Err(format!("\"{}\" is not an address", bad.trim())), "{typed:?}");
    }
}

#[test]
fn only_a_bare_address_is_an_address() {
    for (text, expected) in [
        ("ada@example.com", true),
        ("ada.lovelace+notes@mail.example.org", true),
        ("x@localhost", true),
        ("ada", false),
        ("@example.com", false),
        ("ada@", false),
        ("ada@example.com@example.net", false),
        ("Ada <ada@example.com>", false),
        ("<ada@example.com>", false),
        ("ada@example.com,grace@example.net", false),
        ("ada@example.com\r\nBcc: mallory@example.net", false),
        ("ada @example.com", false),
        ("ada@exam\u{a0}ple.com", false),
        ("ada@.example.com", false),
        ("ada@example.com.", false),
        ("\"a\"@example.com", false),
        ("", false),
    ] {
        assert_eq!(is_address(text), expected, "{text:?}");
    }
}

#[test]
fn the_registrable_domain_is_what_two_addresses_of_one_organisation_share() {
    let table = [
        ("billing@halden.example.com", "example.com"),
        ("priya@example.org", "example.org"),
        ("A@Mail.EXAMPLE.org.", "example.org"),
        ("ada@dept.example.co.uk", "example.co.uk"),
        ("ada@other.co.uk", "other.co.uk"),
        ("ada@example.de", "example.de"),
        ("ada@localhost", "localhost"),
    ];
    for (email, want) in table {
        assert_eq!(registrable(email), want, "{email}");
    }
}

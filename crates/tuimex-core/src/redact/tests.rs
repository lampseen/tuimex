use super::{Links, redact};

const NAMES: [&str; 3] = ["Priya Raman", "Dmitri Volkov", "Robin Vale"];

/// Each row: the text, what leaves, and what the status line says left.
#[test]
fn the_table_of_what_is_replaced() {
    let table = [
        ("nothing to hide here", "nothing to hide here", ""),
        ("write to priya@example.org", "write to [address-1]", "1 address"),
        ("a@example.org, b@example.net and a@example.org", "[address-1], [address-2] and [address-1]", "2 addresses"),
        ("<priya.raman+x@mail.example.org>.", "<[address-1]>.", "1 address"),
        ("not one: a@b, @example.org, x@", "not one: a@b, @example.org, x@", ""),
        ("Priya Raman wrote", "[person-1] wrote", "1 person"),
        ("Hi Priya, ask Dmitri. Robin", "Hi [person-1a], ask [person-2a]. [person-3a]", "3 people"),
        ("Raman and Priya Raman and Priya", "[person-1b] and [person-1] and [person-1a]", "1 person"),
        ("Dmitri then Priya then Dmitri", "[person-1a] then [person-2a] then [person-1a]", "2 people"),
        ("Robinson and robin are not Robin's", "Robinson and robin are not [person-1a]'s", "1 person"),
        ("call +44 20 7946 0018 now", "call [number-1] now", "1 number"),
        ("call (020) 7946-0018.", "call ([number-1].", "1 number"),
        ("account 12345678, sort 12-34-56", "account [number-1], sort 12-34-56", "1 number"),
        ("ref 123456 and 12345", "ref [number-1] and 12345", "1 number"),
        ("on 2026-11-23 at 14:20, 45 days, 30 or 27th, v2", "on 2026-11-23 at 14:20, 45 days, 30 or 27th, v2", ""),
        ("see https://example.net/a?b=1. Then", "see [link-1]. Then", "1 link"),
        ("mailto:priya@example.org or HTTP://Example.org/x)", "[link-1] or [link-2])", "2 links"),
        (
            "Priya <priya@example.org> 5550100123 https://example.org",
            "[person-1a] <[address-1]> [number-1] [link-1]",
            "1 person, 1 address, 1 number, 1 link",
        ),
    ];
    for (text, leaves, says) in table {
        let redacted = redact(text, &NAMES, Links::Withheld);
        assert_eq!(redacted.text, leaves, "{text:?}");
        assert_eq!(redacted.replaced(), says, "{text:?}");
        assert_eq!(redacted.restore(&redacted.text), text, "restoring {text:?} is exact");
    }
}

#[test]
fn a_link_is_kept_when_the_agent_is_asked_about_one() {
    let text = "confirm at https://bank.example.com/confirm or write to help@example.net";
    let redacted = redact(text, &[], Links::Kept);
    assert_eq!(redacted.text, "confirm at https://bank.example.com/confirm or write to [address-1]");
    assert_eq!(redacted.replaced(), "1 address");
}

#[test]
fn what_returns_is_restored_wherever_the_agent_put_it() {
    let redacted = redact("Priya Raman <priya@example.org> asked Dmitri", &NAMES, Links::Withheld);
    assert_eq!(redacted.text, "[person-1] <[address-1]> asked [person-2a]");
    let returned = "Hi [person-1a], [person-2a] agrees. Reach [person-1] at [address-1][address-1].";
    assert_eq!(
        redacted.restore(returned),
        "Hi [person-1a], Dmitri agrees. Reach Priya Raman at priya@example.orgpriya@example.org."
    );
}

/// A placeholder that was never handed out stands for nothing.
#[test]
fn a_placeholder_the_agent_invents_is_left_as_text() {
    let redacted = redact("Hi Priya, write to priya@example.org", &NAMES, Links::Withheld);
    for invented in [
        "[person-9]",
        "[address-2]",
        "[person-1]",
        "[person-1b]",
        "[number-1]",
        "[link-1]",
        "[address-1",
        "[]",
        "[",
        "[address-01]",
        "[Address-1]",
    ] {
        let returned = format!("see {invented} there");
        assert_eq!(redacted.restore(&returned), returned, "{invented}");
    }
}

#[test]
fn text_that_is_not_ascii_is_cut_only_where_a_character_ends() {
    let text = "Zoë Åström (zoë@example.org) ☎ 5550100123 – «Zoë»";
    let redacted = redact(text, &["Zoë Åström"], Links::Withheld);
    assert_eq!(redacted.text, "[person-1] (zoë@example.org) ☎ [number-1] – «[person-1a]»");
    assert_eq!(redacted.restore(&redacted.text), text);
}

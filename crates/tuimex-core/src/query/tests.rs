use super::{Has, Is, Query, Term, When};
use crate::Timestamp;

fn parse(input: &str) -> Vec<Term> {
    Query::parse(input).terms
}

fn text(s: &str) -> Term {
    Term::Text(s.to_string())
}

#[test]
fn an_empty_query_has_no_terms() {
    assert_eq!(parse(""), []);
    assert_eq!(parse("  \t "), []);
}

#[test]
fn every_keyed_term_in_the_grammar_parses() {
    let table = [
        ("in:inbox", Term::In("inbox".into())),
        ("in:Lists/rust", Term::In("Lists/rust".into())),
        ("account:work", Term::Account("work".into())),
        ("is:unread", Term::Is(Is::Unread)),
        ("is:read", Term::Is(Is::Read)),
        ("is:flagged", Term::Is(Is::Flagged)),
        ("is:answered", Term::Is(Is::Answered)),
        ("is:draft", Term::Is(Is::Draft)),
        ("is:signed", Term::Is(Is::Signed)),
        ("is:encrypted", Term::Is(Is::Encrypted)),
        ("is:agent-draft", Term::Is(Is::AgentDraft)),
        ("has:attachment", Term::Has(Has::Attachment)),
        ("has:invite", Term::Has(Has::Invite)),
        ("has:unsubscribe", Term::Has(Has::Unsubscribe)),
        ("from:ada", Term::From("ada".into())),
        ("to:grace@example.org", Term::To("grace@example.org".into())),
        ("cc:team", Term::Cc("team".into())),
        ("subject:invoice", Term::Subject("invoice".into())),
        ("after:7d", Term::After(When::DaysAgo(7))),
        ("before:30d", Term::Before(When::DaysAgo(30))),
        ("after:2026-11-23", Term::After(When::At(Timestamp(1_795_392_000)))),
        ("before:1970-01-02", Term::Before(When::At(Timestamp(86_400)))),
        ("larger:500", Term::Larger(500)),
        ("larger:2k", Term::Larger(2048)),
        ("larger:10MB", Term::Larger(10 * 1024 * 1024)),
        ("larger:1g", Term::Larger(1 << 30)),
        ("minutes", text("minutes")),
    ];
    for (input, want) in table {
        assert_eq!(parse(input), [want], "{input}");
    }
}

#[test]
fn keys_and_fixed_values_ignore_case() {
    assert_eq!(parse("IS:Unread HAS:Attachment"), [Term::Is(Is::Unread), Term::Has(Has::Attachment)]);
}

#[test]
fn terms_are_kept_in_order() {
    assert_eq!(
        parse("in:inbox  is:unread quarterly from:ada"),
        [Term::In("inbox".into()), Term::Is(Is::Unread), text("quarterly"), Term::From("ada".into())]
    );
}

#[test]
fn quotes_keep_spaces_in_one_term() {
    assert_eq!(parse(r#"subject:"board minutes" x"#), [Term::Subject("board minutes".into()), text("x")]);
    assert_eq!(parse(r#""board minutes""#), [text("board minutes")]);
    assert_eq!(parse(r#""unclosed quote runs on"#), [text("unclosed quote runs on")]);
}

#[test]
fn a_quoted_key_is_free_text() {
    assert_eq!(parse(r#""from:ada""#), [text("from:ada")]);
}

#[test]
fn what_the_grammar_does_not_know_is_free_text() {
    for input in [
        "is:bogus",
        "has:teeth",
        "label:x",
        "from:",
        "after:soon",
        "after:2026-13-01",
        "after:2026-02-30",
        "before:2026-1",
        "after:-3d",
        "larger:big",
        "larger:10x",
        "larger:b",
        "larger:99999999999999999999g",
        "http://example.com/a",
    ] {
        assert_eq!(parse(input), [text(input)], "{input}");
    }
}

#[test]
fn a_leap_day_is_a_date_only_in_a_leap_year() {
    assert_eq!(parse("after:2028-02-29"), [Term::After(When::At(Timestamp(1_835_395_200)))]);
    assert_eq!(parse("after:2027-02-29"), [text("after:2027-02-29")]);
    assert_eq!(parse("after:2000-02-29"), [Term::After(When::At(Timestamp(951_782_400)))]);
    assert_eq!(parse("after:2100-02-29"), [text("after:2100-02-29")]);
}

#[test]
fn only_quotes_is_nothing() {
    assert_eq!(parse(r#""""#), []);
}

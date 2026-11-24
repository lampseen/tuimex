use tuimex_core::{Listed, Query};
use tuimex_fixtures::design as fx;

use super::{Mailbox, narrows, wanted};

fn has(text: &str, needle: &str) -> bool {
    super::has(text, &wanted(needle))
}

fn mailbox() -> Mailbox {
    let mail = fx::inbox().into_iter().chain(fx::outbox()).chain([fx::draft()]).collect();
    Mailbox {
        now: fx::NOW,
        sections: fx::sections(),
        folders: fx::folders(),
        mail,
        synced: Some(fx::SYNCED),
        me: fx::addresses(),
    }
}

fn keys(query: &str) -> Vec<String> {
    let mailbox = mailbox();
    let query = Query::parse(query);
    mailbox.mail.iter().filter(|m| mailbox.matches(m, &query)).map(|m| m.envelope.id.key.clone()).collect()
}

/// The design's `list-filter`: what `/nov` matches.
#[test]
fn free_text_is_found_in_the_subject_whatever_its_case() {
    assert_eq!(keys("in:inbox nov"), fx::FILTER.1);
}

#[test]
fn in_names_a_role_or_a_folder() {
    assert_eq!(keys("in:inbox").len(), fx::inbox().len());
    assert_eq!(keys("in:Drafts"), ["draft-agent"]);
    assert_eq!(keys("in:outbox"), ["out-rack", "out-headcount"], "a folder no account lists is still a name");
    assert_eq!(keys("in:archive"), [""; 0]);
}

#[test]
fn every_term_of_the_grammar_is_asked_of_the_message() {
    assert_eq!(keys("is:flagged"), ["halden-1", "payroll", "expense"]);
    assert_eq!(keys("in:inbox is:unread account:work").len(), 5);
    assert_eq!(keys("is:read is:answered"), ["migration", "budget", "bookclub"]);
    assert_eq!(keys("is:agent-draft"), ["draft-agent"]);
    assert_eq!(keys("is:draft"), ["draft-agent"]);
    assert_eq!(keys("is:signed"), ["halden-1", "vault"]);
    assert_eq!(keys("is:encrypted"), ["payroll"]);
    assert_eq!(keys("has:invite"), ["invite"]);
    assert_eq!(keys("has:attachment account:personal"), ["photos", "recipe", "statement"]);
    assert_eq!(keys("from:greta"), ["cabin-2", "photos", "bookclub"]);
    assert_eq!(keys("to:imani"), ["out-headcount"]);
    assert_eq!(keys("cc:billing"), ["draft-agent"]);
    assert_eq!(keys("subject:\"bike rack\""), ["rack", "out-rack"]);
    assert_eq!(keys("roof skis"), [""; 0], "each word is a term of its own, and all must hold");
    assert_eq!(keys("\"roof box\""), ["cabin-5"], "free text reaches the body");
    assert_eq!(keys("before:2026-11-13"), ["bookclub"]);
    assert_eq!(keys("after:1d in:inbox").len(), 16);
    assert_eq!(keys("larger:1k"), [""; 0], "the design's mail carries no size");
}

#[test]
fn has_unsubscribe_matches_what_says_how_to_leave_its_list() {
    assert_eq!(keys("has:unsubscribe"), [""; 0], "the design's mail names no list");
    let mut mailbox = mailbox();
    mailbox.mail[3].unsubscribe = true;
    let query = Query::parse("has:unsubscribe");
    let found: Vec<&Listed> = mailbox.mail.iter().filter(|m| mailbox.matches(m, &query)).collect();
    assert_eq!(found, [&mailbox.mail[3]]);
}

/// The index's search (`tuimex-store`, `index::search`) answers the same
/// way: `a_term_matches_whole_words_in_order_and_the_last_as_a_prefix`.
#[test]
fn text_is_matched_by_whole_words_in_order_the_last_as_a_prefix() {
    let found = |needle: &str| has("Re: Staging database migration window", needle);
    assert!(found("stag") && found("DATABASE migr") && found("staging database migration window"));
    assert!(!found("aging"), "not the middle of a word");
    assert!(!found("database window"), "nor words that are not neighbours");
    assert!(!found("migr database"), "and only the last may be unfinished");
    assert!(has("to be or to be sure", "to be sure"), "a run that breaks may begin again");
    assert!(found("...") && !has("", "x"), "nothing to look for is found anywhere");
}

/// What lets a key of a filter be asked only of what the keys before it
/// matched: where `narrows` says so, nothing the longer filter matches is
/// missed by the shorter.
#[test]
fn a_filter_narrows_the_one_before_it_only_where_it_can_match_nothing_more() {
    for (new, old, narrower) in [
        ("nov", "no", true),
        ("nov", "", true),
        ("nov", "nov", true),
        ("no", "nov", false),
        ("roof estimate", "roof", true),
        ("roof e", "roof est", false),
        ("\"roof est\"", "\"roof\"", true),
        ("\"roof est\"", "\"roo est\"", false),
        ("from:greta", "from:gre", true),
        ("from:greta", "greta", false),
        ("subject:n", "subject:", false),
        ("is:unread", "is:unrea", false),
        ("is:unread roof", "is:unread", true),
        ("roof", "is:unread", false),
        ("-", "", true),
        ("x", "-", true),
    ] {
        let (new_query, old_query) = (Query::parse(new), Query::parse(old));
        assert_eq!(narrows(&new_query, &old_query), narrower, "{new:?} after {old:?}");
    }
    let mailbox = mailbox();
    let typed =
        ["r", "ro", "roo", "roof", "roof ", "roof e", "from:g", "from:gr", "is:unread", "is:unread n", "no", "nov"];
    for old in typed {
        for new in typed {
            let (new_query, old_query) = (Query::parse(new), Query::parse(old));
            let more = mailbox.mail.iter().find(|m| mailbox.matches(m, &new_query) && !mailbox.matches(m, &old_query));
            assert!(!narrows(&new_query, &old_query) || more.is_none(), "{new:?} after {old:?} matches {more:?}");
        }
    }
}

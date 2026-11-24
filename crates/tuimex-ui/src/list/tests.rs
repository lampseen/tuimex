use tuimex_fixtures::design as fx;

use super::chrome::{AccountSync, Sync, alerts, list_hint};
use super::{Fold, columns, outbox, rows};
use crate::reader::size_text;
use crate::span::{Line, cells};

fn text(line: &Line) -> String {
    line.iter().map(|s| s.text.as_str()).collect()
}

fn accounts(names: &[&str]) -> Vec<tuimex_core::AccountId> {
    names.iter().map(|n| tuimex_core::AccountId(n.to_string())).collect()
}

/// The numbers are what the design's `columns` gives for each width.
#[test]
fn the_columns_collapse_in_the_design_s_order() {
    let two = accounts(&["work", "personal"]);
    let at = |width| {
        let c = columns(width, &two);
        (c.letters, c.account, c.from, c.subject, c.words, c.date)
    };
    assert_eq!(at(200), (8, 11, 20, 124, 22, 6));
    assert_eq!(at(130), (8, 11, 20, 54, 22, 6));
    assert_eq!(at(120), (8, 11, 20, 51, 15, 6), "the words take their middle form");
    assert_eq!(at(105), (8, 11, 20, 36, 15, 6));
    assert_eq!(at(100), (8, 11, 20, 37, 9, 6), "then their tight form");
    assert_eq!(at(90), (8, 11, 16, 31, 9, 6), "then the sender narrows");
    assert_eq!(at(86), (4, 6, 16, 32, 9, 6), "then the account is four letters");
    assert_eq!(at(78), (4, 6, 16, 24, 9, 6));
    assert_eq!(at(72), (4, 6, 16, 30, 0, 6), "then the words go");
    assert_eq!(at(64), (4, 6, 16, 30, 0, 0), "then the date");
    assert_eq!(at(56), (0, 0, 16, 28, 0, 0), "then the account");
    assert_eq!(at(10), (0, 0, 16, 0, 0, 0));
}

#[test]
fn the_account_column_is_as_wide_as_the_longest_name() {
    let trio = accounts(&["gmail-home", "gmail-work", "proton"]);
    let at = |width| {
        let c = columns(width, &trio);
        (c.letters, c.account, c.subject)
    };
    assert_eq!(at(120), (10, 13, 49));
    assert_eq!(at(90), (4, 6, 36));
    let long = accounts(&["a-name-longer-than-twelve", "b"]);
    assert_eq!(columns(200, &long).letters, 12, "and no wider than twelve letters");
}

#[test]
fn one_account_or_a_list_narrowed_to_one_has_no_account_column() {
    let at = |width| {
        let c = columns(width, &[]);
        (c.account, c.from, c.subject, c.words, c.date)
    };
    assert_eq!(at(120), (0, 20, 55, 22, 6));
    assert_eq!(at(112), (0, 20, 54, 15, 6));
    assert_eq!(at(80), (0, 16, 32, 9, 6));
    assert_eq!(at(60), (0, 16, 24, 0, 6));
}

#[test]
fn a_folded_thread_is_its_newest_message_under_its_root_s_subject() {
    let mail = fx::inbox();
    let all = rows(&mail, &[]);
    let halden = all.iter().find(|r| r.msg.envelope.id.key == fx::HALDEN).unwrap();
    assert_eq!(halden.fold, Some(Fold::Folded));
    assert_eq!(halden.subject, "Contract draft v3 for the Halden account");
    assert_eq!((halden.size, halden.files, halden.unread, halden.flagged), (3, 2, true, true));
    assert!(all.windows(2).all(|w| w[0].newest >= w[1].newest), "newest thread first");
}

#[test]
fn an_unfolded_thread_keeps_every_row_in_the_group_of_its_newest() {
    let mail = fx::inbox();
    let all = rows(&mail, &[fx::id("personal", fx::UNFOLDED.0)]);
    let cabin: Vec<_> = all.iter().filter(|r| r.msg.envelope.id.key.starts_with("cabin")).collect();
    assert_eq!(cabin.iter().map(|r| r.depth).collect::<Vec<_>>(), [0, 1, 2, 3, 1]);
    assert!(cabin.iter().all(|r| r.newest == cabin[3].msg.envelope.date));
    assert_eq!((cabin[0].fold, cabin[0].reply, cabin[1].fold, cabin[1].reply), (Some(Fold::Open), false, None, true));
    assert_eq!(cabin[1].subject, "I can take three if we leave by four.");
}

#[test]
fn the_hint_for_a_held_message_offers_to_take_it_back() {
    let held = fx::outbox();
    let rows = outbox(&held);
    assert_eq!(rows[0].from, "to Tomas Ferreira");
    assert_eq!(text(&list_hint(Some(&rows[0]), &[], false)), "  u back to Drafts · ? help");
}

#[test]
fn a_size_is_written_as_the_design_writes_them() {
    let mb = 1024.0 * 1024.0;
    for (bytes, want) in [
        (482 * 1024, "482 KB"),
        (9 * 1024, "9 KB"),
        (200, "1 KB"),
        ((2.4 * mb) as u32, "2.4 MB"),
        ((1.8 * mb) as u32, "1.8 MB"),
        (31 * 1024 * 1024, "31 MB"),
    ] {
        assert_eq!(size_text(bytes), want);
    }
}

#[test]
fn a_name_or_reason_with_markup_in_it_is_drawn_as_written() {
    let failing = AccountSync::Failing { reason: "{ok|fine}".into(), retry: Some(90), last: Some(fx::NOW) };
    let line = alerts(&Sync { held: None, failed: 0, accounts: vec![("w}ork".into(), failing)] });
    assert_eq!(text(&line), "w}ork {ok|fine} · retry in 1m · last synced 14:20");
    let stuck = AccountSync::Failing { reason: "cannot sign in".into(), retry: None, last: None };
    let line = alerts(&Sync { held: None, failed: 0, accounts: vec![("work".into(), stuck)] });
    assert_eq!(text(&line), "work cannot sign in", "what is not known is not said");
    assert_eq!(cells(&alerts(&Sync::default())), 0, "nothing wrong says nothing");
}

/// The design's `accountShort`: a name that must be cut first loses what
/// it shares with another account's.
#[test]
fn account_names_alike_at_their_start_stay_apart_when_cut() {
    let short = |names: &[&str], cells: usize| {
        let all = accounts(names);
        all.iter().map(|a| super::account_label(&all, a, cells)).collect::<Vec<_>>()
    };
    let trio = ["gmail-home", "gmail-work", "proton"];
    assert_eq!(short(&trio, 8), ["home", "work", "proton"]);
    assert_eq!(short(&trio, 4), ["home", "work", "prot"]);
    assert_eq!(short(&["personal", "work"], 8), ["personal", "work"]);
    assert_eq!(short(&["personal", "work"], 4), ["pers", "work"]);
    assert_eq!(short(&["mail", "main"], 4), ["mail", "main"], "a name that fits is whole");
    assert_eq!(short(&["me@a.example", "me@b.example"], 4), ["a.ex", "b.ex"]);
}

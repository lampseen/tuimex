use std::fs;

use tuimex_core::Flags;

use super::{Change, Entry, Journal};
use crate::testing::{mode, scratch};
use crate::{Error, files};

fn journal(name: &str) -> (Journal, std::path::PathBuf) {
    let dir = scratch(&format!("journal-{name}"));
    files::private_dir(&dir).unwrap();
    (Journal::new(&dir), dir.join("journal.jsonl"))
}

fn seen() -> Change {
    Change::Flags { set: Flags { seen: true, ..Flags::default() }, clear: Flags::default() }
}

fn archive() -> Change {
    Change::Move { to: "Archive".to_string() }
}

#[test]
fn an_account_with_no_journal_has_nothing_pending() {
    let (journal, path) = journal("none");
    assert_eq!(journal.pending().unwrap(), []);
    assert!(!path.exists(), "reading does not make the file");
}

#[test]
fn a_recorded_change_is_one_line_on_disk_when_record_returns() {
    let (journal, path) = journal("record");
    let entry = journal.record("k1", "INBOX", Some(42), archive()).unwrap();
    assert_eq!(
        entry,
        Entry { seq: 1, key: "k1".to_string(), folder: "INBOX".to_string(), uid: Some(42), change: archive() }
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{\"seq\":1,\"key\":\"k1\",\"folder\":\"INBOX\",\"uid\":42,\"change\":{\"move\":{\"to\":\"Archive\"}}}\n"
    );
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn a_flag_change_names_every_flag() {
    let (journal, path) = journal("flags");
    journal.record("k1", "INBOX", None, seen()).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{\"seq\":1,\"key\":\"k1\",\"folder\":\"INBOX\",\"uid\":null,\"change\":{\"flags\":{\
         \"set\":{\"seen\":true,\"flagged\":false,\"answered\":false,\"draft\":false},\
         \"clear\":{\"seen\":false,\"flagged\":false,\"answered\":false,\"draft\":false}}}}\n"
    );
    assert_eq!(journal.pending().unwrap()[0].change, seen());
}

#[test]
fn pending_is_every_unconfirmed_change_oldest_first_across_runs() {
    let (journal, path) = journal("order");
    journal.record("k1", "INBOX", Some(1), seen()).unwrap();
    journal.record("k2", "INBOX", Some(2), archive()).unwrap();
    let again = Journal::new(path.parent().unwrap());
    again.record("k1", "INBOX", Some(1), archive()).unwrap();
    let pending = again.pending().unwrap();
    assert_eq!(pending.iter().map(|e| (e.seq, e.key.as_str())).collect::<Vec<_>>(), [(1, "k1"), (2, "k2"), (3, "k1")]);
}

#[test]
fn a_confirmed_change_leaves_the_journal_and_the_rest_stay() {
    let (journal, path) = journal("confirm");
    for key in ["k1", "k2", "k3"] {
        journal.record(key, "INBOX", None, seen()).unwrap();
    }
    journal.confirm(2).unwrap();
    assert_eq!(journal.pending().unwrap().iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 3]);
    assert!(matches!(journal.confirm(2), Err(Error::NoEntry(2))), "an entry is confirmed once");
    journal.confirm(1).unwrap();
    journal.confirm(3).unwrap();
    assert_eq!(journal.pending().unwrap(), []);
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn a_new_entry_never_takes_a_pending_entrys_number() {
    let (journal, _) = journal("seq");
    for key in ["k1", "k2", "k3"] {
        journal.record(key, "INBOX", None, seen()).unwrap();
    }
    journal.confirm(1).unwrap();
    assert_eq!(journal.record("k4", "INBOX", None, seen()).unwrap().seq, 4);
}

#[test]
fn a_line_cut_short_by_a_crash_is_not_an_entry_and_the_next_one_is_not_joined_to_it() {
    let (journal, path) = journal("torn");
    journal.record("k1", "INBOX", None, seen()).unwrap();
    let whole = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("{whole}{{\"seq\":2,\"key\":\"k2\",\"fol")).unwrap();
    assert_eq!(journal.pending().unwrap().iter().map(|e| e.seq).collect::<Vec<_>>(), [1]);

    journal.record("k3", "INBOX", None, archive()).unwrap();
    let pending = journal.pending().unwrap();
    assert_eq!(pending.iter().map(|e| (e.seq, e.key.as_str())).collect::<Vec<_>>(), [(1, "k1"), (2, "k3")]);
    assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 2);
}

#[test]
fn a_whole_entry_missing_only_its_line_end_is_kept() {
    let (journal, path) = journal("no-newline");
    journal.record("k1", "INBOX", None, seen()).unwrap();
    let whole = fs::read_to_string(&path).unwrap();
    fs::write(&path, whole.trim_end()).unwrap();
    journal.record("k2", "INBOX", None, seen()).unwrap();
    assert_eq!(journal.pending().unwrap().iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn a_broken_line_that_is_not_the_last_is_an_error_naming_it() {
    let (journal, path) = journal("corrupt");
    journal.record("k1", "INBOX", None, seen()).unwrap();
    let whole = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("{whole}not json\n{whole}")).unwrap();
    assert!(matches!(journal.pending(), Err(Error::Journal { line: 2, .. })));
    assert!(
        journal.record("k2", "INBOX", None, seen()).is_err(),
        "nothing is appended to a journal that cannot be read"
    );
    assert!(matches!(journal.confirm(1), Err(Error::Journal { .. })));
    assert_eq!(fs::read_to_string(&path).unwrap(), format!("{whole}not json\n{whole}"), "and it is left as found");
}

#[test]
fn a_broken_last_line_that_ends_properly_is_an_error_too() {
    let (journal, path) = journal("corrupt-last");
    fs::write(&path, "not json\n").unwrap();
    assert!(matches!(journal.pending(), Err(Error::Journal { line: 1, .. })));
}

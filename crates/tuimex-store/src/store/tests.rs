use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tuimex_core::{AccountId, Flags, MessageId, Origin, Query, Role, Sort, Timestamp};

use super::{Rebuilt, Store, sole};
use crate::journal::Change;
use crate::maildir::Delivery;
use crate::testing::{mode, parsed, scratch};
use crate::{Error, Parsed};

const NOW: Timestamp = Timestamp(1_795_435_200);

/// Stands in for `tuimex-content`, which this crate may not depend on: the
/// message's first line is its subject and the rest its text. No header is
/// read, so nothing here is a second parser (ADR-007).
fn stand_in(raw: &[u8]) -> Parsed {
    let raw = String::from_utf8_lossy(raw);
    let (subject, text) = raw.split_once('\n').unwrap_or((&raw, ""));
    parsed(subject, text)
}

struct Fixture {
    store: Store,
    root: PathBuf,
    parses: Arc<AtomicUsize>,
}

fn open(root: &std::path::Path) -> (Store, Arc<AtomicUsize>) {
    let parses = Arc::new(AtomicUsize::new(0));
    let count = parses.clone();
    let parse = move |raw: &[u8]| {
        count.fetch_add(1, Ordering::Relaxed);
        stand_in(raw)
    };
    (Store::open(&root.join("data"), &root.join("cache"), parse).unwrap(), parses)
}

fn fixture(name: &str) -> Fixture {
    let root = scratch(&format!("store-{name}"));
    let (store, parses) = open(&root);
    for (folder, role) in [("INBOX", Role::Inbox), ("Archive", Role::Archive), ("Trash", Role::Trash)] {
        store.ensure_folder("work", folder, role).unwrap();
    }
    Fixture { store, root, parses }
}

impl Fixture {
    fn deliver(&self, key: &str, uid: Option<u32>) -> MessageId {
        let delivery = Delivery { key: key.to_string(), uid, origin: Origin::Server, flags: Flags::default() };
        self.store.deliver("work", "INBOX", &delivery, format!("subject {key}\nbody of {key}").as_bytes()).unwrap()
    }

    fn find(&self, query: &str) -> Vec<String> {
        let found = self.store.query(&Query::parse(query), NOW, Sort::NewestFirst, None).unwrap();
        found.into_iter().map(|e| e.id.key).collect()
    }

    fn names(&self, dir: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.root.join("data").join(dir))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
}

fn id(key: &str) -> MessageId {
    MessageId { account: AccountId("work".to_string()), key: key.to_string() }
}

fn set(flags: Flags) -> Change {
    Change::Flags { set: flags, clear: Flags::default() }
}

fn clear(flags: Flags) -> Change {
    Change::Flags { set: Flags::default(), clear: flags }
}

const SEEN: Flags = Flags { seen: true, flagged: false, answered: false, draft: false };
const FLAGGED: Flags = Flags { seen: false, flagged: true, answered: false, draft: false };

fn archive() -> Change {
    Change::Move { to: "Archive".to_string() }
}

#[test]
fn opening_makes_both_directories_owner_only_and_an_empty_index() {
    let f = fixture("open");
    assert_eq!(mode(&f.root.join("data")), 0o700);
    assert_eq!(mode(&f.root.join("cache")), 0o700);
    assert_eq!(mode(&f.root.join("cache/index.sqlite")), 0o600);
    assert_eq!(f.find(""), [""; 0]);
}

#[test]
fn a_delivered_message_is_in_the_maildir_and_the_index() {
    let f = fixture("deliver");
    let id = f.deliver("k1", Some(3));
    assert_eq!(id, self::id("k1"));
    assert_eq!(f.names("work/cur"), ["k1,U=3:2,"]);
    assert_eq!(f.find("in:inbox is:unread k1"), ["k1"]);
    assert_eq!(f.store.read(&id).unwrap(), b"subject k1\nbody of k1");
    let size = f.store.query(&Query::default(), NOW, Sort::NewestFirst, None).unwrap()[0].size;
    assert_eq!(size, 21, "the size is the file's");
}

#[test]
fn an_id_gives_the_envelope_a_query_would_and_nothing_for_another_account() {
    let f = fixture("envelope");
    let id = f.deliver("a1", Some(7));
    let by_query = f.store.query(&Query::parse("in:inbox"), NOW, Sort::NewestFirst, None).unwrap();
    assert_eq!(f.store.envelope(&id).unwrap().as_slice(), by_query.as_slice());
    let elsewhere = MessageId { account: AccountId("personal".to_string()), key: "a1".to_string() };
    assert_eq!(f.store.envelope(&elsewhere).unwrap(), None);
    assert_eq!(f.store.envelope(&self::id("a2")).unwrap(), None);
}

#[test]
fn a_key_an_account_already_has_is_refused_before_anything_is_written() {
    let f = fixture("deliver-twice");
    f.deliver("k1", Some(3));
    let other = Delivery { key: "k1".to_string(), uid: Some(4), origin: Origin::Server, flags: SEEN };
    assert!(matches!(f.store.deliver("work", "Archive", &other, b"again"), Err(Error::Exists { .. })));
    assert_eq!(f.names("work/.Archive/cur"), [""; 0]);
    assert_eq!(f.names("work/.Archive/tmp"), [""; 0]);
}

#[test]
fn a_change_made_here_is_journaled_and_made_in_the_maildir_and_the_index() {
    let f = fixture("change-flags");
    let id = f.deliver("k1", Some(3));
    let entry = f.store.change(&id, set(SEEN)).unwrap().unwrap();
    assert_eq!(
        (entry.key.as_str(), entry.folder.as_str(), entry.uid, &entry.change),
        ("k1", "INBOX", Some(3), &set(SEEN))
    );
    assert_eq!(f.store.pending("work").unwrap(), [entry]);
    assert_eq!(f.names("work/cur"), ["k1,U=3:2,S"]);
    assert_eq!(f.find("is:read"), ["k1"]);
}

#[test]
fn a_change_that_cannot_be_journaled_is_not_made() {
    let f = fixture("journal-first");
    let id = f.deliver("k1", Some(3));
    fs::create_dir(f.root.join("data/work/journal.jsonl")).unwrap();
    assert!(f.store.change(&id, set(SEEN)).is_err());
    assert!(f.store.change(&id, archive()).is_err());
    assert_eq!(f.names("work/cur"), ["k1,U=3:2,"], "the journal comes first, so its failure stops the change");
    assert_eq!(f.find("is:unread in:inbox"), ["k1"]);
}

#[test]
fn a_change_the_maildir_refuses_leaves_no_journal_entry() {
    let f = fixture("refused");
    let id = f.deliver("k1", Some(3));
    let nowhere = Change::Move { to: "No such folder".to_string() };
    assert!(matches!(f.store.change(&id, nowhere), Err(Error::Io { .. })));
    assert_eq!(f.store.pending("work").unwrap(), []);
    assert_eq!(f.find("in:inbox"), ["k1"]);
}

#[test]
fn a_journal_entry_holds_only_the_flags_that_changed_so_its_reverse_undoes_it() {
    let f = fixture("undo");
    let id = f.deliver("k1", Some(3));
    f.store.apply(&id, &set(SEEN)).unwrap();
    let both = Flags { seen: true, flagged: true, ..Flags::default() };
    let entry = f.store.change(&id, set(both)).unwrap().unwrap();
    assert_eq!(entry.change, set(FLAGGED), "seen was already set, and undoing must not clear it");

    let Change::Flags { set: was_set, clear: was_cleared } = entry.change else { panic!() };
    f.store.apply(&id, &Change::Flags { set: was_cleared, clear: was_set }).unwrap();
    assert_eq!(f.names("work/cur"), ["k1,U=3:2,S"]);

    let entry = f.store.change(&id, Change::Flags { set: FLAGGED, clear: both }).unwrap().unwrap();
    assert_eq!(entry.change, clear(SEEN), "a flag both set and cleared ends cleared");
    assert_eq!(f.names("work/cur"), ["k1,U=3:2,"]);
}

#[test]
fn a_change_that_changes_nothing_is_not_journaled() {
    let f = fixture("no-op");
    let id = f.deliver("k1", Some(3));
    assert_eq!(f.store.change(&id, clear(SEEN)).unwrap(), None);
    assert_eq!(f.store.change(&id, Change::Move { to: "INBOX".to_string() }).unwrap(), None);
    assert_eq!(f.store.pending("work").unwrap(), []);
    assert_eq!(f.names("work/cur"), ["k1,U=3:2,"], "and a move to where it is keeps its UID");
}

#[test]
fn a_move_is_journaled_with_where_the_server_still_has_the_message() {
    let f = fixture("move");
    let id = f.deliver("k1", Some(3));
    f.store.apply(&id, &set(SEEN)).unwrap();
    let entry = f.store.change(&id, archive()).unwrap().unwrap();
    assert_eq!((entry.folder.as_str(), entry.uid, &entry.change), ("INBOX", Some(3), &archive()));
    assert_eq!(f.names("work/cur"), [""; 0]);
    assert_eq!(f.names("work/.Archive/cur"), ["k1:2,S"]);
    assert_eq!(f.find("in:archive is:read"), ["k1"]);
    assert_eq!(f.find("in:inbox"), [""; 0]);

    let back = f.store.change(&id, Change::Move { to: "INBOX".to_string() }).unwrap().unwrap();
    assert_eq!((back.seq, back.folder.as_str(), back.uid), (2, "Archive", None));
}

#[test]
fn a_change_from_the_server_is_made_without_a_journal_entry() {
    let f = fixture("apply");
    let id = f.deliver("k1", Some(3));
    f.store.apply(&id, &set(FLAGGED)).unwrap();
    f.store.apply(&id, &Change::Move { to: "Trash".to_string() }).unwrap();
    assert_eq!(f.store.pending("work").unwrap(), []);
    assert_eq!(f.find("in:trash is:flagged"), ["k1"]);
}

#[test]
fn a_confirmed_change_leaves_the_journal() {
    let f = fixture("confirm");
    let id = f.deliver("k1", Some(3));
    let first = f.store.change(&id, set(SEEN)).unwrap().unwrap();
    let second = f.store.change(&id, archive()).unwrap().unwrap();
    f.store.confirm("work", first.seq).unwrap();
    assert_eq!(f.store.pending("work").unwrap(), [second]);
    assert!(matches!(f.store.confirm("work", first.seq), Err(Error::NoEntry(1))));
    assert_eq!(f.store.pending("personal").unwrap(), [], "each account has its own journal");
}

#[test]
fn the_journal_survives_a_restart_and_a_rebuild() {
    let f = fixture("journal-survives");
    let id = f.deliver("k1", Some(3));
    let entry = f.store.change(&id, archive()).unwrap().unwrap();
    let root = f.root.clone();
    drop(f);
    fs::remove_file(root.join("cache/index.sqlite")).unwrap();
    let (store, _) = open(&root);
    assert_eq!(store.pending("work").unwrap(), [entry]);
}

#[test]
fn a_uid_the_server_assigns_is_kept_in_the_name_and_the_index() {
    let f = fixture("assign-uid");
    let id = f.deliver("k1", None);
    assert_eq!(f.store.position("work", "INBOX").unwrap().highest_uid, None);
    f.store.assign_uid(&id, 44).unwrap();
    assert_eq!(f.names("work/cur"), ["k1,U=44:2,"]);
    assert_eq!(f.store.position("work", "INBOX").unwrap().highest_uid, Some(44));
}

#[test]
fn a_removed_message_is_gone_from_both() {
    let f = fixture("remove");
    let id = f.deliver("k1", Some(3));
    f.deliver("k2", Some(4));
    f.store.remove(&id).unwrap();
    assert_eq!(f.names("work/cur"), ["k2,U=4:2,"]);
    assert_eq!(f.find(""), ["k2"]);
    assert!(matches!(f.store.remove(&id), Err(Error::NoMessage { .. })));
}

#[test]
fn a_message_nobody_has_is_an_error_that_names_it() {
    let f = fixture("missing");
    let absent = id("../../etc/passwd");
    for result in [f.store.read(&absent).map(drop), f.store.apply(&absent, &set(SEEN)), f.store.assign_uid(&absent, 1)]
    {
        assert!(matches!(result, Err(Error::NoMessage { ref key, .. }) if key == "../../etc/passwd"));
    }
    assert!(matches!(f.store.change(&absent, set(SEEN)), Err(Error::NoMessage { .. })));
}

#[test]
fn a_file_another_tool_renamed_is_still_found() {
    let f = fixture("renamed-outside");
    let id = f.deliver("k1", Some(3));
    let cur = f.root.join("data/work/cur");
    fs::rename(cur.join("k1,U=3:2,"), cur.join("k1,U=3:2,S")).unwrap();
    assert_eq!(f.store.read(&id).unwrap(), b"subject k1\nbody of k1");
    f.store.apply(&id, &set(FLAGGED)).unwrap();
    assert_eq!(f.names("work/cur"), ["k1,U=3:2,FS"], "and the flag it was given outside is kept");
    assert_eq!(f.find("is:read is:flagged"), ["k1"]);
}

#[test]
fn a_file_another_tool_deleted_is_a_missing_message() {
    let f = fixture("deleted-outside");
    let id = f.deliver("k1", Some(3));
    fs::remove_file(f.root.join("data/work/cur/k1,U=3:2,")).unwrap();
    assert!(matches!(f.store.read(&id), Err(Error::NoMessage { .. })));
    assert!(matches!(f.store.change(&id, set(SEEN)), Err(Error::NoMessage { .. })));
    assert_eq!(f.store.pending("work").unwrap(), [], "nothing is journaled for a message that is not there");
}

#[test]
fn reopening_reads_the_index_and_parses_nothing() {
    let f = fixture("reopen");
    f.deliver("k1", Some(3));
    f.deliver("k2", Some(4));
    assert_eq!(f.parses.load(Ordering::Relaxed), 2);
    let root = f.root.clone();
    drop(f);
    let (store, parses) = open(&root);
    assert_eq!(parses.load(Ordering::Relaxed), 0, "a current index is not rebuilt");
    assert_eq!(store.query(&Query::default(), NOW, Sort::NewestFirst, None).unwrap().len(), 2);
}

/// Whoever waits for an index of an older version is told how far its
/// rebuild is, with the total known from the first word.
#[test]
fn a_rebuild_on_open_says_how_many_messages_it_has_indexed_of_how_many() {
    let f = fixture("rebuild-said");
    let count = super::SAID_EVERY * 2 + 3;
    (0..count).for_each(|n| _ = f.deliver(&format!("k{n:05}"), None));
    let root = f.root.clone();
    drop(f);
    let old = rusqlite::Connection::open(root.join("cache/index.sqlite")).unwrap();
    old.pragma_update(None, "user_version", 0).unwrap();
    drop(old);
    let mut said = Vec::new();
    let open = |said: &mut dyn FnMut(usize, usize)| {
        Store::open_saying(&root.join("data"), &root.join("cache"), stand_in, said).unwrap()
    };
    let store = open(&mut |done, total| said.push((done, total)));
    assert_eq!(said, [(0, count), (super::SAID_EVERY, count), (super::SAID_EVERY * 2, count)]);
    assert_eq!(store.listed(None).unwrap().len(), count);
    drop(store);
    open(&mut |done, total| panic!("a current index is not rebuilt, and {done} of {total} was said"));
}

#[test]
fn a_missing_index_is_rebuilt_from_the_maildir_on_open() {
    let f = fixture("rebuild-on-open");
    f.deliver("k1", Some(3));
    f.store.set_uid_validity("work", "INBOX", 77).unwrap();
    f.store.set_modseq("work", "INBOX", 900).unwrap();
    let root = f.root.clone();
    drop(f);
    for file in fs::read_dir(root.join("cache")).unwrap() {
        fs::remove_file(file.unwrap().path()).unwrap();
    }
    let (store, parses) = open(&root);
    assert_eq!(parses.load(Ordering::Relaxed), 1);
    let position = store.position("work", "INBOX").unwrap();
    assert_eq!(
        (position.uid_validity, position.highest_uid),
        (Some(77), Some(3)),
        "what sync resumes from is in the Maildir"
    );
    assert_eq!(position.highest_modseq, None, "the modseq is the one thing only the index had");
    let roles: Vec<Role> = store.folders().unwrap().into_iter().map(|f| f.role).collect();
    assert_eq!(roles, [Role::Archive, Role::Inbox, Role::Trash]);
}

#[test]
fn an_index_from_another_version_is_rebuilt_on_open() {
    let f = fixture("old-version");
    f.deliver("k1", Some(3));
    let root = f.root.clone();
    drop(f);
    let old = rusqlite::Connection::open(root.join("cache/index.sqlite")).unwrap();
    old.pragma_update(None, "user_version", 0).unwrap();
    old.execute_batch("DELETE FROM messages").unwrap();
    drop(old);
    let (store, parses) = open(&root);
    assert_eq!(parses.load(Ordering::Relaxed), 1);
    assert_eq!(store.query(&Query::default(), NOW, Sort::NewestFirst, None).unwrap().len(), 1);
}

#[test]
fn a_rebuild_indexes_a_key_once_and_reports_the_file_it_left_out() {
    let f = fixture("duplicates");
    f.deliver("k1", Some(3));
    fs::write(f.root.join("data/work/.Trash/cur/k1,U=9:2,S"), "subject copy\n").unwrap();
    let rebuilt = f.store.rebuild().unwrap();
    assert_eq!(rebuilt, Rebuilt { messages: 1, duplicates: vec![f.root.join("data/work/.Trash/cur/k1,U=9:2,S")] });
    assert_eq!(f.find("in:inbox"), ["k1"]);
    assert_eq!(f.find("in:trash"), [""; 0]);
}

#[test]
fn a_rebuild_reads_mail_another_tool_delivered() {
    let f = fixture("foreign");
    fs::write(f.root.join("data/work/new/1795000000.M5P9.elsewhere"), "subject from mutt\nhello").unwrap();
    assert_eq!(f.store.rebuild().unwrap().messages, 1);
    assert_eq!(f.find("is:unread mutt"), ["1795000000.M5P9.elsewhere"]);
    f.store.change(&id("1795000000.M5P9.elsewhere"), set(SEEN)).unwrap();
    assert_eq!(f.names("work/cur"), ["1795000000.M5P9.elsewhere:2,S"]);
}

#[test]
fn a_folder_made_through_the_store_has_its_role_in_both_places() {
    let f = fixture("folders");
    f.store.ensure_folder("work", "Lists/rust", Role::Other).unwrap();
    f.store.ensure_folder("work", "Lists/rust", Role::Junk).unwrap();
    let folders = f.store.folders().unwrap();
    let rust = folders.iter().find(|f| f.name == "Lists/rust").unwrap();
    assert_eq!(rust.role, Role::Junk);
    assert_eq!(fs::read_to_string(f.root.join("data/work/.Lists.rust/tuimex-role")).unwrap(), "junk");
}

fn head(f: &Fixture, key: &str, uid: u32) -> MessageId {
    let delivery = Delivery { key: key.to_string(), uid: Some(uid), origin: Origin::Server, flags: Flags::default() };
    f.store.deliver_head("work", "INBOX", &delivery, format!("subject {key}\n").as_bytes()).unwrap()
}

#[test]
fn a_header_block_is_marked_in_its_name_and_still_marked_after_a_rebuild() {
    let f = fixture("head");
    let id = head(&f, "k1", 7);
    assert_eq!(f.names("work/cur"), ["k1,U=7,H=1:2,"]);
    assert!(f.store.stored(&id).unwrap().1.headers_only);
    fs::remove_file(f.root.join("cache/index.sqlite")).unwrap();
    let (store, _) = open(&f.root);
    let (_, stored) = store.stored(&id).unwrap();
    assert!(stored.headers_only);
    assert_eq!(stored.uid, Some(7));
}

#[test]
fn filling_a_header_block_keeps_its_key_uid_and_flags_and_makes_its_body_searchable() {
    let f = fixture("fill");
    let id = head(&f, "k1", 7);
    f.store.apply(&id, &set(Flags { seen: true, ..Flags::default() })).unwrap();
    assert_eq!(f.find("harbour"), Vec::<String>::new());
    f.store.fill(&id, b"subject k1\nmeet at the harbour").unwrap();
    assert_eq!(f.names("work/cur"), ["k1,U=7:2,S"]);
    assert_eq!(f.names("work/tmp"), Vec::<String>::new());
    assert_eq!(f.store.read(&id).unwrap(), b"subject k1\nmeet at the harbour");
    assert!(!f.store.stored(&id).unwrap().1.headers_only);
    assert_eq!(f.find("harbour"), ["k1"]);
    assert_eq!(mode(&f.root.join("data/work/cur/k1,U=7:2,S")), 0o600);
}

#[test]
fn synced_lists_what_the_server_numbered_and_leaves_out_what_it_has_not() {
    let f = fixture("synced");
    f.deliver("local", None);
    f.deliver("b", Some(9));
    let a = head(&f, "a", 4);
    f.store.apply(&a, &set(Flags { flagged: true, ..Flags::default() })).unwrap();
    let synced = f.store.synced("work", "INBOX").unwrap();
    let seen: Vec<(&str, u32, bool)> = synced.iter().map(|s| (s.key.as_str(), s.uid, s.flags.flagged)).collect();
    assert_eq!(seen, [("a", 4, true), ("b", 9, false)]);
    assert_eq!(f.store.synced("work", "Archive").unwrap(), []);
}

#[test]
fn the_servers_namespace_is_kept_with_the_mail_and_outlives_the_index() {
    let f = fixture("namespace");
    assert_eq!(f.store.namespace("work").unwrap(), "", "nothing until a listing has said");
    f.store.set_namespace("work", "INBOX.").unwrap();
    assert_eq!(mode(&f.root.join("data/work/tuimex-namespace")), 0o600);
    fs::remove_file(f.root.join("cache/index.sqlite")).unwrap();
    let (store, _) = open(&f.root);
    assert_eq!(store.namespace("work").unwrap(), "INBOX.");
    store.set_namespace("work", "").unwrap();
    assert_eq!(store.namespace("work").unwrap(), "");
    assert_eq!(store.folders().unwrap().len(), 3, "and it is no folder");
}

#[test]
fn the_mail_is_taken_alone_only_while_no_store_is_open_and_a_store_does_not_open_while_it_is() {
    let f = fixture("lock");
    let data = f.root.join("data");
    assert_eq!(mode(&data.join("tuimex.lock")), 0o600);
    let (second, _) = open(&f.root);
    assert!(matches!(sole(&data), Err(Error::InUse(at)) if at == data), "two stores share it; nobody has it alone");
    drop((f.store, second));
    // The file is still there, as after a crash, and holds nobody out.
    assert!(data.join("tuimex.lock").exists());
    let alone = sole(&data).unwrap();
    let refused = Store::open(&data, &f.root.join("cache"), stand_in);
    assert!(matches!(refused, Err(Error::InUse(_))), "nothing opens the store while it is being moved");
    assert!(matches!(sole(&data), Err(Error::InUse(_))), "nor does a second mover");
    drop(alone);
    open(&f.root);
}

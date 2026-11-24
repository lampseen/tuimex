//! M2's exit, over the fixture mailbox: the index is rebuilt from the
//! Maildir to the same answers (ADR-013), everything the store makes is
//! owner-only (ADR-011), and every term of PLAN §6.1 finds what it should.

// A test that cannot set itself up must fail, which is what unwrap and
// expect do; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tuimex_core::{AccountId, Flags, MessageId, Query, Role, Sort};
use tuimex_fixtures::{FOLDERS, Mail, NOW, mailbox};
use tuimex_store::journal::Change;
use tuimex_store::maildir::Delivery;
use tuimex_store::{Parsed, Store};

/// A directory of this test's own under the build directory, with nothing
/// in it. Never the system temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("store-mailbox-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

/// Stands in for `tuimex-content`, which the store may not depend on
/// (ADR-007): it parses nothing, and knows each fixture message's fields
/// by recognising its bytes.
fn parse(raw: &[u8]) -> Parsed {
    let mail = mailbox().into_iter().find(|m| m.raw() == raw).expect("only fixture messages are stored");
    Parsed {
        date: mail.date,
        from: vec![mail.from],
        to: mail.to,
        cc: mail.cc,
        bcc: Vec::new(),
        reply_to: Vec::new(),
        subject: mail.subject.to_string(),
        message_id: Some(mail.message_id),
        references: mail.references,
        has_attachments: mail.has_attachments,
        has_invite: mail.has_invite,
        has_unsubscribe: mail.has_unsubscribe,
        attachments: Vec::new(),
        security: mail.security,
        text: mail.text.to_string(),
    }
}

fn open(root: &Path) -> Store {
    Store::open(&root.join("data"), &root.join("cache"), parse).unwrap()
}

/// A store holding the fixture mailbox, as a first sync would leave it.
fn fixture(name: &str) -> (Store, PathBuf) {
    let root = scratch(name);
    let store = open(&root);
    for account in ["personal", "work"] {
        for (folder, role, uid_validity) in FOLDERS {
            store.ensure_folder(account, folder, role).unwrap();
            store.set_uid_validity(account, folder, uid_validity).unwrap();
        }
    }
    for mail in mailbox() {
        let delivery = Delivery { key: mail.key.clone(), uid: mail.uid, origin: mail.origin, flags: mail.flags };
        store.deliver(mail.account, mail.folder, &delivery, &mail.raw()).unwrap();
    }
    (store, root)
}

fn id(mail: &Mail) -> MessageId {
    MessageId { account: AccountId(mail.account.to_string()), key: mail.key.clone() }
}

fn the(subject: &str, folder: &str) -> Mail {
    mailbox().into_iter().find(|m| m.subject == subject && m.folder == folder).unwrap()
}

/// The subjects `query` finds, sorted.
fn subjects(store: &Store, query: &str) -> Vec<String> {
    let found = store.query(&Query::parse(query), NOW, Sort::NewestFirst, None).unwrap();
    let mut subjects: Vec<String> = found.into_iter().map(|e| e.subject).collect();
    subjects.sort();
    subjects
}

fn finds(store: &Store, query: &str, expected: &[&str]) {
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(subjects(store, query), expected, "{query}");
}

/// What a session does to a mailbox after the first sync: changes made
/// here, changes from the server, a confirmation, a UID, a removal.
fn use_it(store: &Store) {
    let seen = Flags { seen: true, ..Flags::default() };
    let read = Change::Flags { set: seen, clear: Flags::default() };
    let first = store.change(&id(&the("Lunch on Thursday?", "INBOX")), read).unwrap().unwrap();
    store.change(&id(&the("Quarterly invoice", "INBOX")), Change::Move { to: "Archive".to_string() }).unwrap();
    store.change(&id(&the("You have won", "Trash")), Change::Flags { set: Flags::default(), clear: seen }).unwrap();
    store.confirm("personal", first.seq).unwrap();
    let flagged = Change::Flags { set: Flags { flagged: true, ..Flags::default() }, clear: Flags::default() };
    store.apply(&id(&the("Standup moved", "INBOX")), &flagged).unwrap();
    store.assign_uid(&id(&the("Notes for the trip", "Drafts")), 2).unwrap();
    store.remove(&id(&the("Welcome aboard", "Archive"))).unwrap();
    store.ensure_folder("work", "Lists/rust", Role::Other).unwrap();
    store.apply(&id(&the("Release keys rotated", "INBOX")), &Change::Move { to: "Lists/rust".to_string() }).unwrap();
    store.set_uid_validity("work", "Trash", 99).unwrap();
}

/// At least one query for every term in the grammar, and some together.
const QUERIES: [&str; 34] = [
    "",
    "in:inbox",
    "in:archive",
    "in:Lists/rust",
    "account:personal",
    "account:work",
    "is:unread",
    "is:read",
    "is:flagged",
    "is:answered",
    "is:draft",
    "is:signed",
    "is:encrypted",
    "is:agent-draft",
    "has:attachment",
    "has:invite",
    "has:unsubscribe",
    "from:grace",
    "to:linus",
    "cc:team",
    "subject:invoice",
    "after:7d",
    "before:7d",
    "after:2026-11-20",
    "before:2026-10-26",
    "larger:400",
    "larger:1k",
    "harbour",
    "payment",
    "\"boat trip\"",
    "in:inbox is:unread account:personal",
    "in:archive invoice",
    "is:bogus",
    "from:example.org before:7d",
];

#[test]
fn deleting_the_index_and_rebuilding_gives_the_same_answers() {
    let (store, root) = fixture("rebuild");
    use_it(&store);
    let answers = |store: &Store| -> Vec<_> {
        let sorts = [Sort::NewestFirst, Sort::OldestFirst];
        let run = |q: &&str, sort| store.query(&Query::parse(q), NOW, sort, None).unwrap();
        QUERIES.iter().flat_map(|q| sorts.map(|sort| run(q, sort))).collect()
    };
    let positions = |store: &Store| -> Vec<_> {
        let folders = store.folders().unwrap();
        folders
            .iter()
            .map(|f| (f.account.0.clone(), f.name.clone(), store.position(&f.account.0, &f.name).unwrap()))
            .collect()
    };
    let (before, folders, resume, journal) =
        (answers(&store), store.folders().unwrap(), positions(&store), store.pending("work").unwrap());
    assert_eq!(before[0].len(), mailbox().len() - 1, "the empty query finds every message but the one removed");
    assert!(before.iter().filter(|found| !found.is_empty()).count() > 60, "the queries are not all empty");
    drop(store);

    for file in fs::read_dir(root.join("cache")).unwrap().map(Result::unwrap) {
        fs::remove_file(file.path()).unwrap();
    }
    let store = open(&root);
    assert_eq!(answers(&store), before);
    assert_eq!(store.folders().unwrap(), folders, "folders, their roles and their counts");
    assert_eq!(positions(&store), resume, "where sync resumes from: no folder here had a modseq to lose");
    assert_eq!(store.pending("work").unwrap(), journal);
    assert_eq!(journal.len(), 1);

    assert_eq!(store.rebuild().unwrap().messages, mailbox().len() - 1);
    assert_eq!(answers(&store), before, "and rebuilding an index that is there changes nothing either");
}

#[test]
fn every_directory_is_0700_and_every_file_0600_after_a_full_session() {
    let (store, root) = fixture("modes");
    use_it(&store);
    store.rebuild().unwrap();
    drop(store);
    // Left open for the walk, so the files SQLite keeps beside a database
    // in use are there to be checked.
    let store = open(&root);
    use_it_again(&store);

    fn walk(dir: &Path, root: &Path, seen: &mut Vec<String>) {
        assert_eq!(fs::metadata(dir).unwrap().permissions().mode() & 0o777, 0o700, "{}", dir.display());
        for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
            let (path, kind) = (entry.path(), entry.file_type().unwrap());
            if kind.is_dir() {
                walk(&path, root, seen);
            } else {
                assert!(kind.is_file(), "{} is neither a file nor a directory", path.display());
                assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600, "{}", path.display());
                seen.push(path.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let mut seen = Vec::new();
    walk(&root, &root, &mut seen);

    // The walk is only worth its result if it met one of everything.
    for made in [
        "cache/index.sqlite",
        "cache/index.sqlite-wal",
        "cache/index.sqlite-shm",
        "data/tuimex.lock",
        "data/work/journal.jsonl",
        "data/personal/journal.jsonl",
        "data/work/tuimex-uidvalidity",
        "data/work/tuimex-role",
        "data/work/.Trash/tuimex-uidvalidity",
        "data/work/.Lists.rust/cur/",
        "data/personal/.Drafts/cur/",
        "data/personal/cur/",
    ] {
        assert!(seen.iter().any(|path| path.starts_with(made)), "the session made no {made}");
    }
    assert_eq!(seen.len(), 3 + 1 + 2 + 2 * 2 * FOLDERS.len() + (mailbox().len() - 1), "{seen:#?}");
    assert!(!seen.iter().any(|path| path.contains("/tmp/") || path.ends_with(".tmp")), "nothing is left half-made");
}

/// A second run's changes, over an index that was rebuilt and reopened.
fn use_it_again(store: &Store) {
    let answered = Change::Flags { set: Flags { answered: true, ..Flags::default() }, clear: Flags::default() };
    store.change(&id(&the("Photos from the lake", "INBOX")), answered).unwrap();
    store.change(&id(&the("Photos from the lake", "INBOX")), Change::Move { to: "Trash".to_string() }).unwrap();
    assert_eq!(subjects(store, "in:trash is:answered"), ["Photos from the lake"]);
}

const PERSONAL_INBOX: [&str; 6] = [
    "Issue 212: parsers",
    "Boat trip planning",
    "Photos from the lake",
    "Invitation: Book club",
    "Lunch on Thursday?",
    "Re: Boat trip planning",
];
const WORK_INBOX: [&str; 4] = ["Quarterly invoice", "Release keys rotated", "Offer details", "Standup moved"];
/// Dated in the week before the fixture's `NOW`.
const THIS_WEEK: [&str; 13] = [
    "Boat trip planning",
    "Photos from the lake",
    "Invitation: Book club",
    "Lunch on Thursday?",
    "Re: Boat trip planning",
    "Re: Boat trip planning",
    "Notes for the trip",
    "Quarterly invoice",
    "Release keys rotated",
    "Offer details",
    "Standup moved",
    "Re: Quarterly invoice",
    "Re: Standup moved",
];
const OLDER: [&str; 4] = ["Issue 212: parsers", "Your mooring receipt", "You have won", "Welcome aboard"];

#[test]
fn term_in_with_a_role_finds_the_folder_that_has_it_in_every_account() {
    let (store, _) = fixture("in-role");
    finds(&store, "in:inbox", &[PERSONAL_INBOX.as_slice(), &WORK_INBOX].concat());
    finds(&store, "in:INBOX", &[PERSONAL_INBOX.as_slice(), &WORK_INBOX].concat());
    finds(&store, "in:trash", &["You have won"]);
    finds(&store, "in:drafts", &["Notes for the trip", "Re: Quarterly invoice"]);
    finds(&store, "in:junk", &[]);

    // The role is the folder's, whatever the folder is called.
    store.ensure_folder("work", "Deleted Items", Role::Trash).unwrap();
    store.apply(&id(&the("Offer details", "INBOX")), &Change::Move { to: "Deleted Items".to_string() }).unwrap();
    finds(&store, "in:trash", &["You have won", "Offer details"]);
}

#[test]
fn term_in_with_a_folder_name_finds_that_folder() {
    let (store, _) = fixture("in-folder");
    store.ensure_folder("work", "Lists/rust", Role::Other).unwrap();
    store.apply(&id(&the("Release keys rotated", "INBOX")), &Change::Move { to: "Lists/rust".to_string() }).unwrap();
    finds(&store, "in:Lists/rust", &["Release keys rotated"]);
    finds(&store, "in:lists/RUST", &["Release keys rotated"]);
    finds(&store, "in:Lists", &[]);
    finds(&store, "in:Sent", &["Re: Boat trip planning", "Re: Standup moved"]);
}

#[test]
fn term_account() {
    let (store, _) = fixture("account");
    let work = [WORK_INBOX.as_slice(), &["Re: Quarterly invoice", "Re: Standup moved", "Welcome aboard"]].concat();
    finds(&store, "account:work", &work);
    assert_eq!(subjects(&store, "account:personal").len(), mailbox().len() - work.len());
    finds(&store, "account:nobody", &[]);
    finds(&store, "account:WORK", &[]);
}

#[test]
fn term_is_unread_and_is_read_split_the_mailbox() {
    let (store, _) = fixture("is-unread");
    let unread =
        ["Invitation: Book club", "Lunch on Thursday?", "Re: Boat trip planning", "Quarterly invoice", "Offer details"];
    finds(&store, "is:unread", &unread);
    let read = subjects(&store, "is:read");
    assert_eq!(read.len(), mailbox().len() - unread.len());
    assert!(read.contains(&"Standup moved".to_string()) && !read.contains(&"Offer details".to_string()));
}

#[test]
fn term_is_flagged() {
    finds(&fixture("is-flagged").0, "is:flagged", &["Photos from the lake"]);
}

#[test]
fn term_is_answered() {
    finds(&fixture("is-answered").0, "is:answered", &["Boat trip planning", "Standup moved"]);
}

#[test]
fn term_is_draft() {
    finds(&fixture("is-draft").0, "is:draft", &["Notes for the trip", "Re: Quarterly invoice"]);
}

#[test]
fn term_is_signed() {
    finds(&fixture("is-signed").0, "is:signed", &["Release keys rotated"]);
}

#[test]
fn term_is_encrypted() {
    finds(&fixture("is-encrypted").0, "is:encrypted", &["Offer details"]);
}

#[test]
fn term_is_agent_draft_finds_the_agents_draft_and_not_the_users() {
    finds(&fixture("is-agent-draft").0, "is:agent-draft", &["Re: Quarterly invoice"]);
}

#[test]
fn term_has_attachment() {
    finds(&fixture("has-attachment").0, "has:attachment", &["Photos from the lake", "Quarterly invoice"]);
}

#[test]
fn term_has_invite() {
    finds(&fixture("has-invite").0, "has:invite", &["Invitation: Book club"]);
}

#[test]
fn term_has_unsubscribe() {
    finds(&fixture("has-unsubscribe").0, "has:unsubscribe", &["Issue 212: parsers"]);
}

#[test]
fn term_from_looks_at_the_senders_name_and_address_only() {
    let (store, _) = fixture("from");
    finds(&store, "from:grace", &["Photos from the lake", "Lunch on Thursday?"]);
    finds(&store, "from:linus@example.net", &["Boat trip planning", "Re: Boat trip planning"]);
    finds(&store, "from:\"grace hopper\"", &["Photos from the lake", "Lunch on Thursday?"]);
    finds(&store, "from:emile", &["Standup moved"]);
    finds(&store, "from:photos", &[]);
}

#[test]
fn term_to_looks_at_the_recipients_only() {
    let (store, _) = fixture("to");
    finds(&store, "to:linus", &["Re: Boat trip planning"]);
    finds(&store, "to:grace", &["Notes for the trip"]);
    finds(&store, "to:billing@example.com", &["Re: Quarterly invoice"]);
    finds(&store, "to:team", &[]);
}

#[test]
fn term_cc_looks_at_the_copied_recipients_only() {
    let (store, _) = fixture("cc");
    finds(&store, "cc:team", &["Standup moved"]);
    finds(&store, "cc:\"platform team\"", &["Standup moved"]);
    finds(&store, "cc:ada", &[]);
}

#[test]
fn term_subject_looks_at_the_subject_only() {
    let (store, _) = fixture("subject");
    finds(&store, "subject:invoice", &["Quarterly invoice", "Re: Quarterly invoice"]);
    finds(&store, "subject:\"boat trip\"", &["Boat trip planning", "Re: Boat trip planning", "Re: Boat trip planning"]);
    finds(&store, "subject:payment", &[]);
}

#[test]
fn term_after_with_an_age_counts_back_from_now() {
    let (store, _) = fixture("after-age");
    finds(&store, "after:7d", &THIS_WEEK);
    // The oldest message is thirty days and most of an hour old.
    assert_eq!(subjects(&store, "after:31d").len(), mailbox().len());
    assert_eq!(subjects(&store, "after:30d").len(), mailbox().len() - 1);
    finds(&store, "after:0d", &[]);
}

#[test]
fn term_before_with_an_age_is_everything_after_does_not_find() {
    let (store, _) = fixture("before-age");
    finds(&store, "before:7d", &OLDER);
    finds(&store, "before:29d", &["Your mooring receipt", "Welcome aboard"]);
    finds(&store, "before:30d", &["Your mooring receipt"]);
    finds(&store, "before:31d", &[]);
    assert_eq!(subjects(&store, "before:0d").len(), mailbox().len());
}

#[test]
fn term_after_with_a_date_starts_with_that_day() {
    let (store, _) = fixture("after-date");
    let from_the_20th = [
        "Invitation: Book club",
        "Lunch on Thursday?",
        "Notes for the trip",
        "Quarterly invoice",
        "Release keys rotated",
        "Offer details",
        "Standup moved",
        "Re: Quarterly invoice",
        "Re: Standup moved",
    ];
    finds(&store, "after:2026-11-20", &from_the_20th);
    finds(&store, "after:2026-11-23", &["Lunch on Thursday?", "Notes for the trip", "Re: Quarterly invoice"]);
    finds(&store, "after:2026-11-24", &[]);
}

#[test]
fn term_before_with_a_date_stops_short_of_that_day() {
    let (store, _) = fixture("before-date");
    finds(&store, "before:2026-10-26", &["Your mooring receipt", "Welcome aboard"]);
    finds(&store, "before:2026-10-25", &["Your mooring receipt"]);
    finds(&store, "before:2026-10-24", &[]);
    finds(&store, "after:2026-10-25 before:2026-10-27", &["Welcome aboard", "Issue 212: parsers"]);
}

#[test]
fn term_larger_finds_messages_over_the_size_and_not_at_it() {
    let (store, _) = fixture("larger");
    let mut sizes: Vec<(usize, &str)> = mailbox().iter().map(|m| (m.raw().len(), m.subject)).collect();
    sizes.sort_unstable();
    let (largest, subject) = sizes[sizes.len() - 1];
    assert!(largest > sizes[sizes.len() - 2].0, "the fixture has one largest message");
    finds(&store, &format!("larger:{}", largest - 1), &[subject]);
    finds(&store, &format!("larger:{largest}"), &[]);
    assert_eq!(subjects(&store, "larger:0").len(), mailbox().len());
    finds(&store, "larger:1k", &[]);
    finds(&store, "larger:1MB", &[]);
}

#[test]
fn free_text_looks_at_subject_sender_and_body() {
    let (store, _) = fixture("free-text");
    finds(&store, "thursday", &["Lunch on Thursday?"]);
    finds(&store, "harbour", &["Re: Boat trip planning", "Your mooring receipt"]);
    finds(&store, "payment", &["Quarterly invoice", "Your mooring receipt", "Re: Quarterly invoice"]);
    finds(&store, "grace", &["Photos from the lake", "Lunch on Thursday?"]);
    finds(&store, "café", &["Lunch on Thursday?"]);
    finds(&store, "\"spare rope\"", &["Notes for the trip"]);
    finds(&store, "rope spare", &["Notes for the trip"]);
    finds(&store, "\"rope spare\"", &[]);
}

#[test]
fn a_term_the_grammar_does_not_know_is_free_text() {
    let (store, _) = fixture("unknown");
    finds(&store, "is:bogus", &[]);
    finds(&store, "thursday?", &["Lunch on Thursday?"]);
    finds(&store, "Issue:212", &["Issue 212: parsers"]);
}

#[test]
fn every_term_of_a_query_must_match() {
    let (store, _) = fixture("together");
    finds(
        &store,
        "",
        &[PERSONAL_INBOX.as_slice(), &WORK_INBOX, &THIS_WEEK[5..7], &THIS_WEEK[11..], &OLDER[1..]].concat(),
    );
    finds(
        &store,
        "in:inbox is:unread account:personal",
        &["Invitation: Book club", "Lunch on Thursday?", "Re: Boat trip planning"],
    );
    finds(&store, "in:inbox has:attachment is:unread invoice after:7d larger:100", &["Quarterly invoice"]);
    finds(&store, "in:inbox has:attachment is:unread invoice before:7d", &[]);
    finds(&store, "from:example.org before:7d", &OLDER[..2].iter().chain(&OLDER[3..]).copied().collect::<Vec<_>>());
}

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use tuimex_core::{AccountId, Envelope, Flags, MessageId, Origin, Query, Role, Security, Sort, Timestamp};

use super::{Index, Located, Position};
use crate::maildir::{Delivery, Maildir, Stored};
use crate::testing::{address, mode, parsed, scratch};
use crate::{Error, Parsed, files};

const NOW: Timestamp = Timestamp(1_795_435_200);

struct Fixture {
    index: Index,
    maildir: Maildir,
    root: PathBuf,
}

/// An index that has been through its first rebuild, beside a Maildir to
/// make message files in.
fn fixture(name: &str) -> Fixture {
    let root = scratch(&format!("index-{name}"));
    files::private_dir(&root).unwrap();
    let maildir = Maildir::new(&root.join("mail"));
    let index = Index::open(&root.join("index.sqlite")).unwrap();
    index.rebuild(|| Ok(())).unwrap();
    for (folder, role) in [("INBOX", Role::Inbox), ("Archive", Role::Archive)] {
        maildir.ensure_folder("work", folder, role).unwrap();
        index.put_folder("work", folder, role, None).unwrap();
    }
    Fixture { index, maildir, root }
}

impl Fixture {
    fn file(&self, key: &str, uid: Option<u32>) -> Stored {
        let delivery = Delivery { key: key.to_string(), uid, origin: Origin::Server, flags: Flags::default() };
        self.maildir.deliver("work", "INBOX", &delivery, b"x").unwrap()
    }

    fn add(&self, key: &str, parsed: &Parsed) -> Stored {
        let stored = self.file(key, None);
        self.index.insert("work", "INBOX", &stored, 1, parsed).unwrap();
        stored
    }

    fn find(&self, query: &str) -> Vec<String> {
        self.all(query).into_iter().map(|e| e.id.key).collect()
    }

    fn all(&self, query: &str) -> Vec<Envelope> {
        self.index.query(&Query::parse(query), NOW, Sort::NewestFirst, None).unwrap()
    }
}

fn id(key: &str) -> MessageId {
    MessageId { account: AccountId("work".to_string()), key: key.to_string() }
}

#[test]
fn the_bundled_sqlite_answers_an_fts5_query() {
    let f = fixture("fts5");
    f.add("k1", &parsed("Quarterly invoice", "payment is due in thirty days"));
    f.add("k2", &parsed("Lunch", "the café by the canal"));
    assert_eq!(f.find("payment"), ["k1"]);
    assert_eq!(f.find("canal"), ["k2"]);
    assert_eq!(f.find("absent"), [""; 0]);
}

#[test]
fn the_index_file_and_what_sqlite_puts_beside_it_are_owner_only() {
    let f = fixture("modes");
    f.add("k1", &parsed("s", "t"));
    let mut beside = Vec::new();
    for entry in fs::read_dir(&f.root).unwrap().map(Result::unwrap).filter(|e| e.path().is_file()) {
        assert_eq!(mode(&entry.path()), 0o600, "{:?}", entry.file_name());
        beside.push(entry.file_name().into_string().unwrap());
    }
    beside.sort();
    assert_eq!(beside, ["index.sqlite", "index.sqlite-shm", "index.sqlite-wal"]);
}

#[test]
fn an_index_is_not_current_until_a_rebuild_finishes() {
    let root = scratch("index-current");
    files::private_dir(&root).unwrap();
    let index = Index::open(&root.join("index.sqlite")).unwrap();
    assert!(!index.is_current().unwrap());
    let failed: crate::Result<()> = index.rebuild(|| Err(Error::NoEntry(0)));
    assert!(failed.is_err());
    assert!(!index.is_current().unwrap());
    index.rebuild(|| Ok(())).unwrap();
    assert!(index.is_current().unwrap());
    drop(index);
    assert!(Index::open(&root.join("index.sqlite")).unwrap().is_current().unwrap(), "and stays current when reopened");
}

#[test]
fn a_rebuild_that_fails_leaves_the_index_as_it_was() {
    let f = fixture("rebuild-fails");
    f.add("k1", &parsed("kept", "t"));
    let failed: crate::Result<()> = f.index.rebuild(|| {
        f.index.insert("work", "INBOX", &f.file("k2", None), 1, &parsed("lost", "t"))?;
        Err(Error::NoEntry(0))
    });
    assert!(failed.is_err());
    assert_eq!(f.find(""), ["k1"]);
    assert_eq!(f.find("kept"), ["k1"]);
    assert!(f.index.is_current().unwrap());
}

#[test]
fn a_file_that_is_not_a_database_is_replaced() {
    let root = scratch("index-not-a-database");
    files::private_dir(&root).unwrap();
    let path = root.join("index.sqlite");
    fs::write(&path, "this was never a database, and is longer than a header").unwrap();
    let index = Index::open(&path).unwrap();
    assert!(!index.is_current().unwrap());
    index.rebuild(|| Ok(())).unwrap();
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn a_link_where_the_index_should_be_is_refused() {
    let root = scratch("index-link");
    files::private_dir(&root).unwrap();
    fs::write(root.join("elsewhere"), "").unwrap();
    symlink(root.join("elsewhere"), root.join("index.sqlite")).unwrap();
    assert!(matches!(Index::open(&root.join("index.sqlite")), Err(Error::Io { .. })));
    assert_eq!(fs::read(root.join("elsewhere")).unwrap(), b"");
}

#[test]
fn an_envelope_comes_back_as_it_went_in() {
    let f = fixture("envelope");
    let delivery = Delivery {
        key: "k1".to_string(),
        uid: Some(9),
        origin: Origin::Agent,
        flags: Flags { seen: true, draft: true, ..Flags::default() },
    };
    let stored = f.maildir.deliver("work", "INBOX", &delivery, b"x").unwrap();
    let parsed = Parsed {
        date: Timestamp(-5),
        from: vec![address("Grace Hopper", "grace@example.net")],
        to: vec![address("", "ada@example.com"), address("Linus, \"the\" <Okafor>", "linus@example.net")],
        cc: vec![address("Platform Team", "team@example.org")],
        bcc: vec![address("", "quiet@example.net")],
        reply_to: vec![address("Desk", "desk@example.org")],
        subject: "Ünïcode, 'quotes' and % signs".to_string(),
        message_id: Some("<1@example.net>".to_string()),
        references: vec!["<a@example.net>".to_string(), "<b@example.net>".to_string()],
        has_attachments: true,
        has_invite: true,
        has_unsubscribe: true,
        attachments: Vec::new(),
        security: Security::Both,
        text: "body".to_string(),
    };
    f.index.insert("work", "INBOX", &stored, 4096, &parsed).unwrap();
    let got = f.all("").remove(0);
    assert_eq!(got.id, id("k1"));
    assert_eq!(got.folder.0, "INBOX");
    assert_eq!((got.date, got.size, got.flags, got.origin), (Timestamp(-5), 4096, delivery.flags, Origin::Agent));
    assert_eq!((&got.from, &got.to, &got.cc), (&parsed.from, &parsed.to, &parsed.cc));
    assert_eq!((&got.bcc, &got.reply_to), (&parsed.bcc, &parsed.reply_to));
    assert_eq!(
        (&got.subject, &got.message_id, &got.references),
        (&parsed.subject, &parsed.message_id, &parsed.references)
    );
    assert_eq!((got.has_attachments, got.security), (true, Security::Both));
    assert_eq!(f.find("has:invite has:unsubscribe"), ["k1"]);
}

#[test]
fn an_envelope_with_nothing_in_it_comes_back_empty() {
    let f = fixture("empty-envelope");
    let nothing = Parsed { from: Vec::new(), to: Vec::new(), subject: String::new(), ..parsed("", "") };
    f.add("k1", &nothing);
    let got = f.all("").remove(0);
    assert_eq!((got.from.len(), got.to.len(), got.cc.len(), got.references.len()), (0, 0, 0, 0));
    assert_eq!(
        (got.subject.as_str(), got.message_id, got.security, got.origin),
        ("", None, Security::None, Origin::Server)
    );
}

#[test]
fn the_separators_the_index_uses_cannot_be_smuggled_in_by_a_message() {
    let f = fixture("separators");
    let hostile = Parsed {
        from: vec![address("Mallory\u{1e}Second\u{1f}third@example.net", "mallory\u{1f}@example.net")],
        references: vec!["<a\u{1e}b@example.net>".to_string()],
        ..parsed("s", "t")
    };
    f.add("k1", &hostile);
    let got = f.all("").remove(0);
    assert_eq!(got.from, [address("MallorySecondthird@example.net", "mallory@example.net")], "still one sender");
    assert_eq!(got.references, ["<ab@example.net>"]);
}

#[test]
fn a_key_is_indexed_once_per_account() {
    let f = fixture("duplicate");
    let stored = f.add("k1", &parsed("first", "alpha"));
    let again = f.index.insert("work", "Archive", &stored, 1, &parsed("second", "beta"));
    assert!(matches!(again, Err(Error::Index(_))));
    assert_eq!(f.find("alpha"), ["k1"]);
    assert_eq!(f.find("beta"), [""; 0], "the refused message left nothing in the search table");
    f.index.insert("personal", "INBOX", &stored, 1, &parsed("second", "beta")).unwrap();
}

#[test]
fn locate_says_where_the_maildir_keeps_a_message() {
    let f = fixture("locate");
    let stored = f.add("k1", &parsed("s", "t"));
    let found = f.index.locate(&id("k1")).unwrap();
    assert_eq!(found, Some(Located { folder: "INBOX".to_string(), file: stored.file().to_string() }));
    assert_eq!(f.index.locate(&id("absent")).unwrap(), None);
}

#[test]
fn refile_follows_a_rename() {
    let f = fixture("refile");
    let stored = f.add("k1", &parsed("s", "t"));
    let flagged = f.maildir.set_flags(&stored, Flags { flagged: true, seen: true, ..Flags::default() }).unwrap();
    f.index.refile(&id("k1"), "INBOX", &flagged).unwrap();
    assert_eq!(f.find("is:flagged is:read in:inbox"), ["k1"]);

    let moved = f.maildir.move_to(&f.maildir.set_uid(&flagged, 4).unwrap(), "work", "Archive").unwrap();
    f.index.refile(&id("k1"), "Archive", &moved).unwrap();
    assert_eq!(f.find("in:inbox"), [""; 0]);
    assert_eq!(f.find("in:archive is:flagged"), ["k1"]);
    assert_eq!(f.index.locate(&id("k1")).unwrap().unwrap().file, "k1:2,FS");
}

#[test]
fn a_removed_message_is_gone_from_the_search_table_too() {
    let f = fixture("remove");
    f.add("k1", &parsed("s", "needle"));
    f.add("k2", &parsed("s", "needle"));
    f.index.remove(&id("k1")).unwrap();
    assert_eq!(f.find(""), ["k2"]);
    assert_eq!(f.find("needle"), ["k2"]);
    f.add("k3", &parsed("s", "thread"));
    assert_eq!(f.find("needle"), ["k2"], "a row number used again does not bring the old text back");
}

#[test]
fn a_position_is_the_uidvalidity_the_highest_uid_and_the_modseq() {
    let f = fixture("position");
    assert_eq!(f.index.position("work", "INBOX").unwrap(), Position::default());
    assert_eq!(f.index.position("work", "Nowhere").unwrap(), Position::default());
    f.index.put_folder("work", "INBOX", Role::Inbox, Some(77)).unwrap();
    f.index.set_modseq("work", "INBOX", 900).unwrap();
    for (key, uid) in [("k1", Some(3)), ("k2", Some(12)), ("k3", None)] {
        f.index.insert("work", "INBOX", &f.file(key, uid), 1, &parsed("s", "t")).unwrap();
    }
    let position = Position { uid_validity: Some(77), highest_uid: Some(12), highest_modseq: Some(900) };
    assert_eq!(f.index.position("work", "INBOX").unwrap(), position);
    assert_eq!(f.index.position("work", "Archive").unwrap(), Position::default(), "each folder has its own");
}

#[test]
fn a_modseq_is_forgotten_when_the_uidvalidity_changes_and_kept_when_it_does_not() {
    let f = fixture("modseq");
    f.index.put_folder("work", "INBOX", Role::Inbox, Some(77)).unwrap();
    f.index.set_modseq("work", "INBOX", 900).unwrap();
    f.index.put_folder("work", "INBOX", Role::Inbox, Some(77)).unwrap();
    assert_eq!(f.index.position("work", "INBOX").unwrap().highest_modseq, Some(900));
    f.index.put_folder("work", "INBOX", Role::Inbox, Some(78)).unwrap();
    let position = Position { uid_validity: Some(78), highest_uid: None, highest_modseq: None };
    assert_eq!(f.index.position("work", "INBOX").unwrap(), position);
}

#[test]
fn folders_come_with_their_role_and_counts() {
    let f = fixture("folders");
    f.add("k1", &parsed("s", "t"));
    let seen = f.add("k2", &parsed("s", "t"));
    let seen = f.maildir.set_flags(&seen, Flags { seen: true, ..Flags::default() }).unwrap();
    f.index.refile(&id("k2"), "INBOX", &seen).unwrap();
    let folders = f.index.folders().unwrap();
    let rows: Vec<_> =
        folders.iter().map(|f| (f.account.0.as_str(), f.name.as_str(), f.role, f.total, f.unseen)).collect();
    assert_eq!(rows, [("work", "Archive", Role::Archive, 0, 0), ("work", "INBOX", Role::Inbox, 2, 1)]);
    assert_eq!(folders[1].id.0, "INBOX");
}

#[test]
fn text_is_matched_by_whole_words_in_order_with_the_last_one_unfinished() {
    let f = fixture("words");
    f.add("k1", &parsed("Board minutes for November", "t"));
    f.add("k2", &parsed("Minutes of the board", "t"));
    assert_eq!(f.find("subject:\"board minutes\""), ["k1"]);
    assert_eq!(f.find("subject:\"board min\""), ["k1"]);
    assert_eq!(f.find("subject:nov"), ["k1"]);
    assert_eq!(f.find("subject:ovember"), [""; 0], "a word is matched from its start");
    assert_eq!(f.find("subject:\"boa minutes\""), [""; 0], "only the last word may be unfinished");
    assert_eq!(f.find("subject:board subject:minutes").len(), 2, "two terms need not be adjacent");
    assert_eq!(f.find("BOARD"), ["k1", "k2"]);
}

#[test]
fn text_ignores_case_and_accents() {
    let f = fixture("accents");
    f.add("k1", &Parsed { from: vec![address("Émile Durand", "emile@example.org")], ..parsed("Café ÜBER", "t") });
    for query in ["from:emile", "from:ÉMILE", "from:durand", "subject:cafe", "subject:über", "from:example.org"] {
        assert_eq!(f.find(query), ["k1"], "{query}");
    }
}

#[test]
fn nothing_typed_into_a_query_is_search_syntax() {
    let f = fixture("syntax");
    f.add("k1", &parsed("alpha beta", "gamma"));
    for query in [
        "\"alpha\" OR \"zzz\"",
        "subject:alpha\"",
        "alpha*",
        "alpha AND",
        "NEAR(alpha",
        "body:gamma",
        "alpha)",
        "-alpha",
        "^alpha",
        "{subject}:alpha",
        "alpha' OR 1=1 --",
    ] {
        f.index.query(&Query::parse(query), NOW, Sort::NewestFirst, None).unwrap_or_else(|e| panic!("{query}: {e}"));
    }
    assert_eq!(f.find("alpha OR"), [""; 0], "OR is a word to look for, and no message has it");
    assert_eq!(f.find("alpha NOT beta"), [""; 0]);
    assert_eq!(f.find("subject:gamma"), [""; 0], "a column is searched only when its term asks");
    assert_eq!(f.find("to:\"x\" OR subject:alpha"), [""; 0]);
}

#[test]
fn text_with_no_letter_or_digit_asks_for_nothing() {
    let f = fixture("punctuation");
    f.add("k1", &parsed("tom & jerry", "t"));
    for query in ["tom & jerry", "—", "subject:!!!", "from:@", "\"\" tom", "***"] {
        assert_eq!(f.find(query), ["k1"], "{query}");
    }
}

#[test]
fn results_are_sorted_by_date_then_account_and_key_and_can_be_limited() {
    let f = fixture("sort");
    for (key, date) in [("b", 20), ("a", 20), ("c", 30), ("d", 10)] {
        f.add(key, &Parsed { date: Timestamp(date), ..parsed("s", "t") });
    }
    f.index.put_folder("personal", "INBOX", Role::Inbox, None).unwrap();
    f.index
        .insert("personal", "INBOX", &f.file("z", None), 1, &Parsed { date: Timestamp(20), ..parsed("s", "t") })
        .unwrap();
    let keys = |sort, limit| -> Vec<String> {
        f.index.query(&Query::default(), NOW, sort, limit).unwrap().into_iter().map(|e| e.id.key).collect()
    };
    assert_eq!(keys(Sort::NewestFirst, None), ["c", "z", "a", "b", "d"]);
    assert_eq!(keys(Sort::OldestFirst, None), ["d", "z", "a", "b", "c"]);
    assert_eq!(keys(Sort::NewestFirst, Some(2)), ["c", "z"]);
    assert_eq!(keys(Sort::NewestFirst, Some(0)), [""; 0]);
}

#[test]
fn a_list_row_has_the_opening_the_files_and_what_the_message_offers() {
    let f = fixture("listed");
    let file = |index, name: &str, mime: &str, inline| tuimex_core::Attachment {
        index,
        name: name.to_string(),
        mime: mime.to_string(),
        size: 48_211,
        inline,
    };
    let files = vec![file(1, "plan, v2.pdf", "application/pdf", false), file(2, "", "image/png", true)];
    let text = "> what was asked\n\n  The answer is yes.\nMore follows.\n";
    let offers =
        Parsed { has_invite: true, has_unsubscribe: true, attachments: files.clone(), ..parsed("Re: plan", text) };
    f.add("k1", &offers);
    f.add("k2", &parsed("bare", ""));
    f.add("k3", &parsed("long", &"é".repeat(2000)));
    assert_eq!(f.index.listed(None).unwrap().len(), 3);
    let some = f.index.listed(Some(&[id("k1"), id("gone"), id("k2"), id("k3")])).unwrap();
    let [full, bare, long] = some.as_slice() else { panic!("a message that has gone is left out: {some:?}") };
    assert_eq!(full.envelope, f.all("subject:plan").remove(0));
    assert_eq!(full.snippet, "The answer is yes.", "the first line that is not quoted");
    assert_eq!(full.text.as_deref(), Some(text.trim()));
    assert_eq!((&full.attachments, full.invite, full.unsubscribe), (&files, true, true));
    assert_eq!((bare.snippet.as_str(), &bare.text, bare.invite, bare.unsubscribe), ("", &None, false, false));
    assert_eq!(long.text.as_ref().map(|t| t.chars().count()), Some(600), "cut between characters");
}

/// The interface's filter answers the same way (`tuimex-ui`, `mailbox`:
/// `text_is_matched_by_whole_words_in_order_the_last_as_a_prefix`).
#[test]
fn a_term_matches_whole_words_in_order_and_the_last_as_a_prefix() {
    let f = fixture("agrees");
    f.add("k1", &parsed("Re: Staging database migration window", "to be or to be sure"));
    let found =
        ["subject:stag", "subject:\"DATABASE migr\"", "\"staging database migration window\"", "\"to be sure\""];
    for query in found {
        assert_eq!(f.find(query), ["k1"], "{query}");
    }
    for query in ["subject:aging", "subject:\"database window\"", "subject:\"migr database\""] {
        assert_eq!(f.find(query), [""; 0], "{query}");
    }
    assert_eq!(f.find("..."), ["k1"], "nothing to look for is found anywhere");
}

/// Run by hand, optimised, for the number PLAN §9's measurement quotes:
/// `cargo test --release -p tuimex-store --lib fifty_thousand -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, which a loaded machine would fail"]
fn fifty_thousand_rows_are_read_for_the_list_at_start() {
    let f = fixture("fifty-thousand");
    let body = "The opening of a message, as long as most are. ".repeat(12);
    let template = f.file("template", None);
    f.index
        .rebuild(|| {
            f.index.put_folder("work", "INBOX", Role::Inbox, None)?;
            for n in 0..50_000 {
                let mut stored = template.clone();
                stored.key = format!("k{n}");
                let parsed = Parsed { date: Timestamp(i64::from(n)), ..parsed(&format!("Subject number {n}"), &body) };
                f.index.insert("work", "INBOX", &stored, 4096, &parsed)?;
            }
            Ok(())
        })
        .unwrap();
    let started = std::time::Instant::now();
    let all = f.index.listed(None).unwrap();
    println!("50,000 rows read from the index: {:?}", started.elapsed());
    assert_eq!(all.len(), 50_000);
    let ids: Vec<MessageId> = (0..100).map(|n| id(&format!("k{}", n * 400))).collect();
    let started = std::time::Instant::now();
    assert_eq!(f.index.listed(Some(&ids)).unwrap().len(), 100);
    println!("100 of them read again after a sync: {:?}", started.elapsed());
    assert!(started.elapsed().as_millis() < 100);
}

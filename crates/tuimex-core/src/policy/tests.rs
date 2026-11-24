use super::Request::{Accounts, Draft, Folders, List, Search, Show, Thread};
use super::*;

/// Every combination of the three keys, in the order the table's columns
/// are written: `read`, the folders, `draft`.
fn every_grant() -> Vec<Grants> {
    let mut all = Vec::new();
    for read in [false, true] {
        for folders in [&[][..], &["INBOX"], &["INBOX", "Sent"]] {
            for draft in [false, true] {
                all.push(Grants { read, folders: folders.iter().map(|f| f.to_string()).collect(), draft });
            }
        }
    }
    all
}

/// One row per request, one character per grant combination: `.` allowed,
/// `r` refused for want of `read`, `d` refused for want of `draft`.
///
/// ```text
///            read:   no     no      no          yes    yes     yes
///            folders: -     INBOX   INBOX,Sent  -      INBOX   INBOX,Sent
///            draft:  n y    n y     n y         n y    n y     n y
/// ```
const TABLE: [(Request, &str); 24] = [
    (Accounts, "r.r.r......."),
    (Folders { folder: None }, "rrrrrr......"),
    (Folders { folder: Some("INBOX") }, "rrrrrrrr...."),
    (Folders { folder: Some("Sent") }, "rrrrrrrrrr.."),
    (Folders { folder: Some("Trash") }, "rrrrrrrrrrrr"),
    (List { folder: None }, "rrrrrr......"),
    (List { folder: Some("INBOX") }, "rrrrrrrr...."),
    (List { folder: Some("Sent") }, "rrrrrrrrrr.."),
    (List { folder: Some("Trash") }, "rrrrrrrrrrrr"),
    (Search { folder: None }, "rrrrrr......"),
    (Search { folder: Some("INBOX") }, "rrrrrrrr...."),
    (Search { folder: Some("Sent") }, "rrrrrrrrrr.."),
    (Search { folder: Some("Trash") }, "rrrrrrrrrrrr"),
    (Show { folder: "INBOX" }, "rrrrrrrr...."),
    (Show { folder: "Sent" }, "rrrrrrrrrr.."),
    (Show { folder: "Trash" }, "rrrrrrrrrrrr"),
    (Thread { folder: "INBOX" }, "rrrrrrrr...."),
    (Thread { folder: "Sent" }, "rrrrrrrrrr.."),
    (Thread { folder: "Trash" }, "rrrrrrrrrrrr"),
    (Draft { reply_to: None }, "d.d.d.d.d.d."),
    (Draft { reply_to: Some("INBOX") }, "drdrdrdrd.d."),
    (Draft { reply_to: Some("Sent") }, "drdrdrdrdrd."),
    (Draft { reply_to: Some("Trash") }, "drdrdrdrdrdr"),
    (Draft { reply_to: Some("Drafts") }, "drdrdrdrdrdr"),
];

#[test]
fn every_request_against_every_grant() {
    let grants = every_grant();
    for (request, row) in TABLE {
        assert_eq!(row.len(), grants.len(), "{request:?} has a cell for every grant combination");
        for (grants, cell) in grants.iter().zip(row.chars()) {
            let expected = match cell {
                '.' => None,
                'r' => Some(Grant::Read),
                'd' => Some(Grant::Draft),
                other => panic!("{other} is not a cell"),
            };
            let got = allows(grants, &request).err().map(|denied| denied.grant);
            assert_eq!(got, expected, "{request:?} under {grants:?}");
        }
    }
}

/// Names a variant's place in the table. A new variant fails to compile
/// here until it is given one, and then fails the test below until the
/// table has its rows.
fn kind(request: &Request) -> usize {
    match request {
        Accounts => 0,
        Folders { .. } => 1,
        List { .. } => 2,
        Search { .. } => 3,
        Show { .. } => 4,
        Thread { .. } => 5,
        Draft { .. } => 6,
    }
}

#[test]
fn the_table_has_every_kind_of_request() {
    for wanted in 0..7 {
        assert!(TABLE.iter().any(|(request, _)| kind(request) == wanted), "no row for request kind {wanted}");
    }
}

#[test]
fn no_grants_at_all_allow_nothing() {
    for (request, _) in TABLE {
        assert!(allows(&Grants::default(), &request).is_err(), "{request:?}");
    }
}

#[test]
fn a_folder_is_matched_by_its_whole_name_and_only_inbox_ignores_case() {
    let grants = Grants { read: true, folders: vec!["inbox".into(), "Work/Bills".into()], draft: false };
    for (folder, allowed) in [
        ("INBOX", true),
        ("Inbox", true),
        ("Work/Bills", true),
        ("work/bills", false),
        ("Work", false),
        ("Work/Bills/2026", false),
        ("INBOX/Sub", false),
        ("", false),
    ] {
        assert_eq!(allows(&grants, &Show { folder }).is_ok(), allowed, "{folder:?}");
    }
}

#[test]
fn a_refusal_says_which_key_would_have_allowed_it() {
    let read = Grants { read: true, folders: vec!["INBOX".into()], draft: false };
    let denied = allows(&read, &Show { folder: "Trash" }).unwrap_err();
    assert_eq!((denied.grant.name(), denied.to_string().as_str()), ("read", "read does not cover the folder Trash"));
    let denied = allows(&read, &Draft { reply_to: None }).unwrap_err();
    assert_eq!((denied.grant.name(), denied.to_string().as_str()), ("draft", "draft is not granted"));
}

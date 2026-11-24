use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use tuimex_core::{Flags, Origin, Role};

use super::{Delivery, Maildir, Stored, folder_dir_name, folder_name, new_key, role_name, valid_key};
use crate::Error;
use crate::testing::{mode, scratch};

fn maildir(name: &str) -> (Maildir, std::path::PathBuf) {
    let root = scratch(&format!("maildir-{name}"));
    let maildir = Maildir::new(&root);
    maildir.ensure_folder("work", "INBOX", Role::Inbox).unwrap();
    maildir.ensure_folder("work", "Archive", Role::Archive).unwrap();
    (maildir, root)
}

fn delivery(key: &str, uid: Option<u32>) -> Delivery {
    Delivery { key: key.to_string(), uid, origin: Origin::Server, flags: Flags::default() }
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    names
}

#[test]
fn a_delivery_leaves_tmp_and_lands_in_cur_under_its_full_name() {
    let (maildir, root) = maildir("deliver");
    let flags = Flags { seen: true, flagged: true, ..Flags::default() };
    let stored = maildir
        .deliver(
            "work",
            "INBOX",
            &Delivery { flags, ..delivery("1795000000.M1P1.test", Some(7)) },
            b"Subject: x\r\n\r\nbody",
        )
        .unwrap();
    assert_eq!(names(&root.join("work/cur")), ["1795000000.M1P1.test,U=7:2,FS"]);
    assert_eq!(names(&root.join("work/tmp")), [""; 0]);
    assert_eq!(names(&root.join("work/new")), [""; 0]);
    assert_eq!(maildir.read(&stored).unwrap(), b"Subject: x\r\n\r\nbody");
    assert_eq!(mode(&stored.path()), 0o600);
}

#[test]
fn a_second_delivery_under_one_name_is_refused_and_leaves_nothing_in_tmp() {
    let (maildir, root) = maildir("deliver-twice");
    maildir.deliver("work", "INBOX", &delivery("k", Some(1)), b"first").unwrap();
    let again = maildir.deliver("work", "INBOX", &delivery("k", Some(1)), b"second");
    assert!(matches!(again, Err(Error::Io { ref source, .. }) if source.kind() == std::io::ErrorKind::AlreadyExists));
    assert_eq!(fs::read(root.join("work/cur/k,U=1:2,")).unwrap(), b"first");
    assert_eq!(names(&root.join("work/tmp")), [""; 0]);
}

#[test]
fn a_name_says_the_key_the_uid_who_wrote_it_and_the_flags() {
    let all = Flags { seen: true, flagged: true, answered: true, draft: true };
    let table = [
        ("k:2,", None, Origin::Server, Flags::default()),
        ("k,U=42:2,S", Some(42), Origin::Server, Flags { seen: true, ..Flags::default() }),
        ("k,U=3,O=a:2,D", Some(3), Origin::Agent, Flags { draft: true, ..Flags::default() }),
        ("k,O=l:2,DFRS", None, Origin::Local, all),
        ("k,U=4294967295:2,", Some(u32::MAX), Origin::Server, Flags::default()),
    ];
    for (name, uid, origin, flags) in table {
        let stored = Stored::from_name(name, Path::new("/f"), "cur").unwrap();
        assert_eq!((stored.key.as_str(), stored.uid, stored.origin, stored.flags), ("k", uid, origin, flags), "{name}");
        assert_eq!(stored.name(), name, "the name reads back");
    }
}

#[test]
fn a_name_another_tool_wrote_is_read_and_its_extras_survive_a_rename() {
    let (maildir, root) = maildir("foreign");
    fs::write(root.join("work/cur/1.host,S=100,W=120,U=9:2,PSa"), "x").unwrap();
    fs::write(root.join("work/new/2.host"), "y").unwrap();
    fs::write(root.join("work/cur/3.host,U=many:1,x"), "z").unwrap();
    let found = maildir.list("work", "INBOX").unwrap();
    assert_eq!(found.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["1.host", "2.host", "3.host"]);
    assert_eq!((found[0].uid, found[0].flags.seen), (Some(9), true));
    assert_eq!((found[1].uid, found[1].flags), (None, Flags::default()));
    assert_eq!(
        (found[2].uid, found[2].flags),
        (None, Flags::default()),
        "an unreadable UID and info are not guessed at"
    );

    assert_eq!(maildir.read(&found[0]).unwrap(), b"x", "it is read under the name it has, not one tuimex would give");
    maildir.set_flags(&found[0], Flags { flagged: true, ..Flags::default() }).unwrap();
    maildir.set_flags(&found[2], Flags { seen: true, ..Flags::default() }).unwrap();
    assert_eq!(names(&root.join("work/cur")), ["1.host,U=9,S=100,W=120:2,FPa", "3.host,U=many:2,S"]);
}

#[test]
fn a_flag_change_renames_the_file_and_keeps_the_key() {
    let (maildir, root) = maildir("flags");
    let stored = maildir.deliver("work", "INBOX", &delivery("k", Some(5)), b"x").unwrap();
    let seen = maildir.set_flags(&stored, Flags { seen: true, ..Flags::default() }).unwrap();
    assert_eq!(names(&root.join("work/cur")), ["k,U=5:2,S"]);
    assert_eq!((seen.key.as_str(), seen.uid), ("k", Some(5)));
    let same = maildir.set_flags(&seen, seen.flags).unwrap();
    assert_eq!(same, seen, "setting the flags a message has changes nothing");
}

#[test]
fn a_message_another_tool_left_in_new_is_read_there_and_renamed_into_cur() {
    let (maildir, root) = maildir("new-to-cur");
    fs::write(root.join("work/new/k"), "x").unwrap();
    let fresh = maildir.find("work", "INBOX", "k", None).unwrap().unwrap();
    assert_eq!(fresh.path(), root.join("work/new/k"));
    assert_eq!(maildir.read(&fresh).unwrap(), b"x");

    let seen = maildir.set_flags(&fresh, Flags { seen: true, ..Flags::default() }).unwrap();
    assert_eq!(seen.path(), root.join("work/cur/k:2,S"));
    assert_eq!(names(&root.join("work/new")), [""; 0]);
    assert_eq!(maildir.read(&seen).unwrap(), b"x");
}

#[test]
fn a_move_files_the_message_in_the_other_folder_without_its_uid() {
    let (maildir, root) = maildir("move");
    let stored = maildir.deliver("work", "INBOX", &delivery("k", Some(5)), b"x").unwrap();
    let moved = maildir.move_to(&stored, "work", "Archive").unwrap();
    assert_eq!(names(&root.join("work/cur")), [""; 0]);
    assert_eq!(names(&root.join("work/.Archive/cur")), ["k:2,"]);
    assert_eq!(moved.uid, None);
    assert_eq!(maildir.read(&moved).unwrap(), b"x");
}

#[test]
fn a_move_does_not_replace_a_message_already_there() {
    let (maildir, root) = maildir("move-clash");
    maildir.deliver("work", "Archive", &delivery("k", None), b"there first").unwrap();
    let stored = maildir.deliver("work", "INBOX", &delivery("k", Some(5)), b"x").unwrap();
    assert!(maildir.move_to(&stored, "work", "Archive").is_err());
    assert_eq!(fs::read(root.join("work/.Archive/cur/k:2,")).unwrap(), b"there first");
    assert!(stored.path().exists());
}

#[test]
fn a_uid_from_the_server_goes_into_the_name() {
    let (maildir, root) = maildir("uid");
    let draft = Delivery { origin: Origin::Agent, ..delivery("k", None) };
    let stored = maildir.deliver("work", "INBOX", &draft, b"x").unwrap();
    assert_eq!(maildir.set_uid(&stored, 12).unwrap().uid, Some(12));
    assert_eq!(names(&root.join("work/cur")), ["k,U=12,O=a:2,"]);
}

#[test]
fn remove_deletes_the_file() {
    let (maildir, root) = maildir("remove");
    let stored = maildir.deliver("work", "INBOX", &delivery("k", None), b"x").unwrap();
    maildir.remove(&stored).unwrap();
    assert_eq!(names(&root.join("work/cur")), [""; 0]);
    assert!(maildir.remove(&stored).is_err());
}

#[test]
fn list_reads_cur_and_new_and_nothing_that_is_not_a_message_file() {
    let (maildir, root) = maildir("list");
    maildir.deliver("work", "INBOX", &delivery("b", None), b"x").unwrap();
    fs::write(root.join("work/new/a"), "x").unwrap();
    fs::write(root.join("work/tmp/half-written"), "x").unwrap();
    fs::write(root.join("work/cur/.hidden"), "x").unwrap();
    fs::create_dir(root.join("work/cur/a-directory")).unwrap();
    fs::write(root.join("secret"), "not mail").unwrap();
    symlink(root.join("secret"), root.join("work/cur/linked:2,S")).unwrap();
    let keys: Vec<String> = maildir.list("work", "INBOX").unwrap().into_iter().map(|s| s.key).collect();
    assert_eq!(keys, ["a", "b"]);
}

#[test]
fn find_uses_the_remembered_name_and_looks_again_when_it_is_stale() {
    let (maildir, _) = maildir("find");
    let stored = maildir.deliver("work", "INBOX", &delivery("k", Some(1)), b"x").unwrap();
    assert_eq!(maildir.find("work", "INBOX", "k", Some(stored.file())).unwrap(), Some(stored.clone()));
    let seen = maildir.set_flags(&stored, Flags { seen: true, ..Flags::default() }).unwrap();
    assert_eq!(maildir.find("work", "INBOX", "k", Some(stored.file())).unwrap(), Some(seen.clone()));
    assert_eq!(maildir.find("work", "INBOX", "k", Some("other,U=1:2,")).unwrap(), Some(seen));
    assert_eq!(maildir.find("work", "INBOX", "absent", None).unwrap(), None);
}

#[test]
fn a_remembered_name_that_points_out_of_the_folder_is_not_followed() {
    let (maildir, root) = maildir("find-escape");
    fs::write(root.join("work/k,U=1:2,S"), "outside cur").unwrap();
    assert_eq!(maildir.find("work", "INBOX", "k", Some("../k,U=1:2,S")).unwrap(), None);
    assert_eq!(maildir.find("work", "INBOX", "k", Some("k,U=1:2,/../../k,U=1:2,S")).unwrap(), None);
}

#[test]
fn a_folder_name_becomes_one_directory_name_and_reads_back() {
    let table = [
        ("Sent", ".Sent"),
        ("Lists/rust", ".Lists.rust"),
        ("[Gmail]/All Mail", ".[Gmail].All Mail"),
        ("v1.2", ".v1%2E2"),
        ("50%", ".50%25"),
        ("..", ".%2E%2E"),
        ("a\0b\n", ".a%00b%0A"),
        ("Entwürfe", ".Entwürfe"),
        ("inbox/sub", ".inbox.sub"),
    ];
    for (folder, dir) in table {
        assert_eq!(folder_dir_name(folder).unwrap().as_deref(), Some(dir), "{folder:?}");
        assert_eq!(folder_name(dir).as_deref(), Some(folder), "{dir:?}");
    }
    for inbox in ["INBOX", "Inbox", "inbox"] {
        assert_eq!(folder_dir_name(inbox).unwrap(), None, "the INBOX is the account's own directory");
    }
}

#[test]
fn a_folder_name_with_an_empty_part_is_refused() {
    for folder in ["", "/", "/a", "a/", "a//b"] {
        assert!(matches!(folder_dir_name(folder), Err(Error::Name(_))), "{folder:?}");
    }
}

#[test]
fn a_directory_no_folder_name_maps_to_is_not_a_folder() {
    for dir in ["cur", ".", "..", ".a..b", ".a.", ".INBOX", ".a%2", ".a%ZZ", ".a%2e", ".a%41"] {
        assert_eq!(folder_name(dir), None, "{dir:?}");
    }
}

#[test]
fn no_folder_name_reaches_outside_the_account() {
    let (maildir, root) = maildir("escape");
    for folder in ["..", "../other", "a/../../b", "./x"] {
        maildir.ensure_folder("work", folder, Role::Other).unwrap();
    }
    assert_eq!(names(&root), ["work"]);
    let made = maildir.folders("work").unwrap();
    assert_eq!(made, ["INBOX", "..", "../other", "./x", "Archive", "a/../../b"]);
}

#[test]
fn an_account_name_is_one_plain_directory_name() {
    let (maildir, _) = maildir("account-names");
    for account in ["", ".", "..", ".hidden", "a/b", "/abs", "a\0b"] {
        assert!(matches!(maildir.ensure_folder(account, "INBOX", Role::Inbox), Err(Error::Name(_))), "{account:?}");
    }
}

#[test]
fn a_key_is_one_path_component_without_name_syntax() {
    for key in ["", ".", "..", ".x", "a/b", "a,b", "a:b", "a\0"] {
        assert!(!valid_key(key), "{key:?}");
        let (maildir, _) = maildir("bad-key");
        assert!(matches!(maildir.deliver("work", "INBOX", &delivery(key, None), b"x"), Err(Error::Name(_))), "{key:?}");
    }
    assert!(valid_key("1795000000.M1P1.host"));
}

#[test]
fn new_keys_are_valid_and_differ() {
    let (a, b) = (new_key(), new_key());
    assert!(valid_key(&a) && valid_key(&b));
    assert_ne!(a, b);
}

#[test]
fn accounts_and_folders_are_what_is_on_disk() {
    let (maildir, root) = maildir("listing");
    maildir.ensure_folder("personal", "Sent", Role::Sent).unwrap();
    fs::create_dir(root.join(".not-an-account")).unwrap();
    fs::write(root.join("a-file"), "").unwrap();
    assert_eq!(maildir.accounts().unwrap(), ["personal", "work"]);
    assert_eq!(maildir.folders("work").unwrap(), ["INBOX", "Archive"]);
    assert_eq!(maildir.folders("personal").unwrap(), ["Sent"], "an account has no INBOX until one is made");
    assert_eq!(Maildir::new(&root.join("absent")).accounts().unwrap(), [""; 0]);
}

#[test]
fn a_folder_keeps_its_uidvalidity_in_a_file() {
    let (maildir, root) = maildir("uidvalidity");
    assert_eq!(maildir.uid_validity("work", "Archive").unwrap(), None);
    maildir.set_uid_validity("work", "Archive", 1_700_000_001).unwrap();
    assert_eq!(fs::read_to_string(root.join("work/.Archive/tuimex-uidvalidity")).unwrap(), "1700000001");
    maildir.set_uid_validity("work", "Archive", 2).unwrap();
    assert_eq!(maildir.uid_validity("work", "Archive").unwrap(), Some(2));
    assert_eq!(maildir.uid_validity("work", "INBOX").unwrap(), None, "each folder has its own");
    assert_eq!(mode(&root.join("work/.Archive/tuimex-uidvalidity")), 0o600);
}

#[test]
fn a_uidvalidity_file_that_is_not_a_number_is_no_uidvalidity() {
    let (maildir, root) = maildir("uidvalidity-junk");
    fs::write(root.join("work/tuimex-uidvalidity"), "soon\n").unwrap();
    assert_eq!(maildir.uid_validity("work", "INBOX").unwrap(), None);
}

#[test]
fn a_folder_keeps_its_role_and_can_be_given_another() {
    let (maildir, root) = maildir("role");
    assert_eq!(maildir.role("work", "Archive").unwrap(), Role::Archive);
    assert_eq!(fs::read_to_string(root.join("work/.Archive/tuimex-role")).unwrap(), "archive");
    maildir.ensure_folder("work", "Lists", Role::Other).unwrap();
    assert_eq!(maildir.role("work", "Lists").unwrap(), Role::Other);
    assert!(!root.join("work/.Lists/tuimex-role").exists(), "no role is the absence of the file");
    maildir.ensure_folder("work", "Lists", Role::Junk).unwrap();
    assert_eq!(maildir.role("work", "Lists").unwrap(), Role::Junk);
}

#[test]
fn every_role_has_its_own_word() {
    let roles =
        [Role::Inbox, Role::Sent, Role::Drafts, Role::Archive, Role::Trash, Role::Junk, Role::Outbox, Role::Other];
    for role in roles {
        assert_eq!(super::role_named(role_name(role)), role);
    }
    assert_eq!(super::role_named("something else"), Role::Other);
}

#[test]
fn every_directory_made_is_owner_only() {
    let (_, root) = maildir("modes");
    for dir in ["", "work", "work/cur", "work/new", "work/tmp", "work/.Archive", "work/.Archive/cur"] {
        assert_eq!(mode(&root.join(dir)), 0o700, "{dir}");
    }
}

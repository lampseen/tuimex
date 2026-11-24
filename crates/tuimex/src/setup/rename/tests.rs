use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use tuimex_core::{AccountId, Flags, MessageId, Origin, Query, Role, Sort, Timestamp};
use tuimex_engine::{Keyring, Secret, oauth, pin};
use tuimex_store::Store;
use tuimex_store::journal::Change;
use tuimex_store::maildir::Delivery;

use super::{MAIL, MARK, Places, STEPS, carry_out, interrupted, unfinished};
use crate::config::Config;
use crate::setup::tests::{CLIENT_ID, CLIENT_SECRET, FakeNet, MemoryKeyring, PASSWORD, Run, scratch};
use crate::setup::write;

const REFRESH: &str = "1//synthetic-refresh-token-44ab";

/// A config a person has edited: comments, a name in each kind of quote,
/// grants, and sections that name accounts.
const CONFIG: &str = "# mine, and it stays\n[ui]\nglyphs = \"ascii\"\n\n[[account]]\nname = \"work\"   # the office one\n\
                      address = \"ada@example.org\"\nimap = { host = \"imap.example.org\", port = 993 }\n\
                      smtp = { host = \"smtp.example.org\", port = 587 }\nauth = \"password\"\n\n[account.agent]\n\
                      read = true\nfolders = [\"INBOX\"]\ndraft = false\n\n[[account]]\nname = 'home'\n\
                      address = \"ada@gmail.com\"\nimap = { host = \"imap.gmail.com\", port = 993 }\n\
                      smtp = { host = \"smtp.gmail.com\", port = 465 }\nauth = \"gmail-oauth\"\n\n[[section]]\n\
                      name = \"Work\"\nshort = \"Work\"\nquery = \"in:inbox account:work is:unread\"   # account:work\n\n\
                      [[section]]\nname = \"Shop\"\nshort = \"Shop\"\nquery = \"account:workshop subject:account:work-it\"\n";

fn id(account: &str) -> MessageId {
    MessageId { account: AccountId(account.to_string()), key: "k1".to_string() }
}

/// Two accounts with mail, sign-ins and a pinned certificate, and for
/// `work` a message archived here that its server has not heard of.
fn world(name: &str) -> (PathBuf, MemoryKeyring) {
    let root = scratch(name);
    fs::create_dir_all(root.join("config")).unwrap();
    fs::write(root.join("config/config.toml"), CONFIG).unwrap();
    pin::save(&root.join("config"), "work", &[7; 32]).unwrap();
    let keyring = MemoryKeyring::default();
    for (entry, secret) in [
        ("work", PASSWORD),
        (oauth::refresh_entry("home").as_str(), REFRESH),
        (oauth::CLIENT_ID, CLIENT_ID),
        (oauth::CLIENT_SECRET, CLIENT_SECRET),
    ] {
        keyring.set(entry, &Secret::new(secret.to_string())).unwrap();
    }
    let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
    for account in ["work", "home"] {
        store.ensure_folder(account, "INBOX", Role::Inbox).unwrap();
        store.ensure_folder(account, "Archive", Role::Archive).unwrap();
        store.set_uid_validity(account, "INBOX", 7).unwrap();
        let delivery =
            Delivery { key: "k1".to_string(), uid: Some(3), origin: Origin::Server, flags: Flags::default() };
        store
            .deliver(account, "INBOX", &delivery, b"Subject: kept\r\nMessage-ID: <k1@example.org>\r\n\r\nx\r\n")
            .unwrap();
    }
    store.change(&id("work"), Change::Move { to: "Archive".to_string() }).unwrap().unwrap();
    (root, keyring)
}

/// Everything a rename may touch, as it stands: the config's bytes, what
/// is in the data and config directories, and the keyring's entries.
fn state(root: &Path, keyring: &MemoryKeyring) -> (String, BTreeSet<String>, Vec<String>) {
    let listed = |dir: &'static str| -> Vec<String> {
        let names = fs::read_dir(root.join(dir)).unwrap().map(|e| e.unwrap().file_name().display().to_string());
        names.map(|name| format!("{dir}/{name}")).collect()
    };
    let config = fs::read_to_string(root.join("config/config.toml")).unwrap();
    (config, listed("data").into_iter().chain(listed("config")).collect(), keyring.names())
}

/// The state a finished rename of `work` to `office` leaves, checked from
/// every side that reads it.
fn renamed(root: &Path, keyring: &MemoryKeyring) {
    let (config, files, entries) = state(root, keyring);
    let expected =
        CONFIG.replace("name = \"work\"   # the office one", "name = \"office\"   # the office one").replace(
            "in:inbox account:work is:unread\"   # account:work",
            "in:inbox account:office is:unread\"   # account:work",
        );
    assert_eq!(config, expected, "the name and the one query that named it, and not a byte besides");
    let want = ["config/config.toml", "config/office.pem", "data/home", "data/office", "data/tuimex.lock"];
    assert_eq!(files, want.map(String::from).into_iter().collect());
    assert_eq!(entries, ["home/refresh-token", "oauth/google/client-id", "oauth/google/client-secret", "office"]);
    assert_eq!(keyring.get("office").as_deref(), Some(PASSWORD));
    assert_eq!(pin::load(&root.join("config"), "office").unwrap().unwrap(), [7; 32]);
    assert!(Config::parse(&config).unwrap().account("office").unwrap().grants().read, "its grants came with it");
    assert_eq!(unfinished(&root.join("data")).unwrap(), None);

    // The index was dropped and is read again from the mail; what was
    // waiting for the server still is, under the new name, by the same key.
    assert!(!root.join("cache/index.sqlite-wal").exists());
    let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
    let all = store.query(&Query::parse(""), Timestamp(i64::MAX / 2), Sort::NewestFirst, None).unwrap();
    let mut found: Vec<(String, String)> = all.into_iter().map(|e| (e.id.account.0, e.folder.0)).collect();
    found.sort();
    assert_eq!(found, [("home".to_string(), "INBOX".to_string()), ("office".to_string(), "Archive".to_string())]);
    let pending = store.pending("office").unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!((pending[0].key.as_str(), pending[0].folder.as_str(), pending[0].uid), ("k1", "INBOX", Some(3)));
    assert_eq!(pending[0].change, Change::Move { to: "Archive".to_string() });
    assert_eq!(store.position("office", "INBOX").unwrap().uid_validity, Some(7), "so nothing is fetched again");
}

fn run<'a>(root: &'a Path, keyring: &'a MemoryKeyring, net: &'a FakeNet) -> Run<'a> {
    Run { root, keyring, net, google: None, browser: false, claude: false }
}

#[test]
fn an_account_is_renamed_from_setup_s_menu_in_all_four_places_and_what_waited_for_the_server_still_does() {
    let (root, keyring) = world("rename");
    let net = FakeNet::trusted();
    // The menu's line, the first account, two names that will not do, the
    // name, and a no; then the same with a yes.
    let (result, script) = run(&root, &keyring, &net).go(&["6", "1", "home", "of/fice", ".office", "office", "n"]);
    result.unwrap();
    let shown = script.shown();
    let menu = "\nAccounts here: work, home.\n  1) Add another account\n  2) Sign in again to work\n  \
                3) Accept a new certificate for work\n  4) Sign in again to home\n  \
                5) Accept a new certificate for home\n  6) Rename an account\n  \
                7) Add a calendar to an account that has none\n  8) Quit";
    assert!(shown.contains(menu), "{shown}");
    assert_eq!(shown.matches("There is an account of that name already.").count(), 1, "{shown}");
    assert_eq!(shown.matches("Letters, digits, `-`, `_` and `.`; not starting with a dot.").count(), 2, "{shown}");
    let told = "\nThis renames work to office in four places: its mail on this computer, its saved sign-in in the \
                keyring, its server's certificate if one was accepted, and its name in the config.\nNo mail is \
                downloaded again, and nothing changes on the server.";
    assert!(shown.contains(told) && shown.ends_with("Nothing was changed."), "{shown}");
    assert_eq!(state(&root, &keyring).0, CONFIG, "a no is no");
    assert!(root.join("data/work").exists() && keyring.get("work").is_some());

    let (result, script) = run(&root, &keyring, &net).go(&["6", "1", "office", "y"]);
    result.unwrap();
    assert!(script.shown().ends_with("work is now office."), "{}", script.shown());
    for secret in [PASSWORD, REFRESH, CLIENT_SECRET] {
        assert!(!script.shown().contains(secret) && !shown.contains(secret));
    }
    renamed(&root, &keyring);
}

/// ADR-050: stopped after any step, the account is whole under the old
/// name or the mark says which rename to finish, and running it again
/// from setup ends in the same state as a rename that was never stopped.
#[test]
fn a_rename_stopped_after_any_step_is_all_old_or_is_finished_by_running_setup_again() {
    for done in 0..=STEPS.len() {
        let (root, keyring) = world(&format!("rename-stopped-{done}"));
        let dirs = (root.join("config"), root.join("data"));
        let (cache, config) = (root.join("cache"), root.join("config/config.toml"));
        let at =
            Places { data: &dirs.1, cache: &cache, config_dir: &dirs.0, config_path: &config, keyring: Some(&keyring) };
        fs::write(dirs.1.join(MARK), "work\noffice\n").unwrap();
        for (doing, step) in &STEPS[..done] {
            step(&at, "work", "office").unwrap_or_else(|e| panic!("{doing}: {e:#}"));
        }
        assert_eq!(unfinished(&dirs.1).unwrap(), Some(("work".to_string(), "office".to_string())), "after {done}");
        if done <= MAIL {
            // Nothing has moved: the old name still opens everything.
            assert_eq!(state(&root, &keyring).0, CONFIG);
            assert!(dirs.1.join("work/journal.jsonl").exists() && dirs.0.join("work.pem").exists());
            assert_eq!(keyring.get("work").as_deref(), Some(PASSWORD));
        } else {
            assert!(dirs.1.join("office/journal.jsonl").exists() && !dirs.1.join("work").exists(), "after {done}");
        }
        let net = FakeNet::trusted();
        let (result, script) = run(&root, &keyring, &net).go(&[]);
        result.unwrap_or_else(|e| panic!("after {done} steps: {e:#}\n{}", script.shown()));
        let shown = script.shown();
        assert!(shown.starts_with("The rename of work to office was begun and not finished. Finishing it now."));
        assert!(shown.ends_with("work is now office."), "{shown}");
        renamed(&root, &keyring);
    }
}

/// A keyring like the system's when it is locked or will not delete.
struct Sticky<'a>(&'a MemoryKeyring);

impl Keyring for Sticky<'_> {
    fn password(&self, entry: &str) -> Result<Option<Secret>, String> {
        self.0.password(entry)
    }
    fn set(&self, entry: &str, secret: &Secret) -> Result<(), String> {
        self.0.set(entry, secret)
    }
    fn remove(&self, _entry: &str) -> Result<(), String> {
        Err("the collection is locked".to_string())
    }
}

#[test]
fn a_keyring_that_refuses_the_write_stops_the_rename_with_nothing_moved_and_one_that_will_not_forget_is_said() {
    let (root, keyring) = world("rename-keyring");
    let before = state(&root, &keyring);
    let down =
        MemoryKeyring { kept: std::sync::Mutex::new(keyring.kept.lock().unwrap().clone()), down: Some("it is locked") };
    let net = FakeNet::trusted();
    let (result, _) = run(&root, &down, &net).go(&["6", "1", "office", "y"]);
    let error = format!("{:#}", result.unwrap_err());
    let said = "copying the sign-in to the new name in the keyring failed, and nothing was renamed: the keyring could \
                not be read: it is locked";
    assert_eq!(error, said);
    assert_eq!((state(&root, &keyring), unfinished(&root.join("data")).unwrap()), (before, None));

    // An account that signs in with a command has nothing in the keyring,
    // and is renamed on a machine with none.
    let by_command =
        CONFIG.replace("auth = \"password\"", "auth = \"command\"\npassword_cmd = [\"pass\", \"show\", \"work\"]");
    fs::write(root.join("config/config.toml"), &by_command).unwrap();
    run(&root, &down, &net).go(&["6", "1", "office", "y"]).0.unwrap();
    assert!(root.join("data/office").exists() && keyring.get("work").is_some() && keyring.get("office").is_none());

    // One that keeps the new entry and will not give up the old: the
    // rename is done, and the entry left behind is said.
    let (root, keyring) = world("rename-sticky");
    let dirs = (root.join("config"), root.join("data"));
    let (cache, config) = (root.join("cache"), root.join("config/config.toml"));
    let sticky = Sticky(&keyring);
    let at = Places { data: &dirs.1, cache: &cache, config_dir: &dirs.0, config_path: &config, keyring: Some(&sticky) };
    let notes = carry_out(&at, "work", "office").unwrap();
    let left = "The keyring would not remove what it kept under work (the collection is locked). Nothing uses it now.";
    assert_eq!(notes, [left]);
    assert_eq!((keyring.get("work").as_deref(), keyring.get("office").as_deref()), (Some(PASSWORD), Some(PASSWORD)));
    assert!(unfinished(&dirs.1).unwrap().is_none() && dirs.1.join("office").exists());
}

#[test]
fn a_rename_is_refused_while_another_tuimex_has_the_mail_open_and_where_the_new_name_is_taken_on_disk() {
    let (root, keyring) = world("rename-in-use");
    let before = state(&root, &keyring);
    let net = FakeNet::trusted();
    let open = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
    let (result, _) = run(&root, &keyring, &net).go(&["6", "1", "office", "y"]);
    let error = format!("{:#}", result.unwrap_err());
    assert!(error.contains("is in use by another tuimex: close the tuimex that is open"), "{error}");
    assert_eq!((state(&root, &keyring), unfinished(&root.join("data")).unwrap()), (before, None));
    drop(open);

    // Mail left by an account that is no longer in the config is not
    // moved onto.
    fs::create_dir_all(root.join("data/attic")).unwrap();
    let (_, script) = run(&root, &keyring, &net).go(&["6", "1", "attic"]);
    assert!(script.shown().contains("There is mail or a certificate under attic already"), "{}", script.shown());
    assert!(root.join("data/work").exists());
}

#[test]
fn the_config_is_given_the_new_name_only_where_that_changes_nothing_else_and_is_otherwise_left_to_its_owner() {
    let by_hand = "nothing was written; in the config, change `name = \"work\"` to `name = \"office\"` by hand";
    for config in [
        // One inline array, an escape in the name, the name twice, and none.
        "account = [{ name = \"work\", address = \"ada@example.org\" }]\n".to_string(),
        CONFIG.replace("name = \"work\"", "name = \"wor\\u006b\""),
        format!("{CONFIG}\n[[account]]\nname = \"work\"\naddress = \"bob@example.org\"\n"),
        CONFIG.replace("name = \"work\"", "name = \"other\""),
    ] {
        assert_eq!(write::renamed(&config, "work", "office").unwrap_err().to_string(), by_hand, "{config}");
    }
    // A name in single quotes, and a table header with a comment after it.
    let home = write::renamed(
        &CONFIG.replace("[[account]]\nname = 'home'", "[[account]] # two\nname = 'home'"),
        "home",
        "house",
    );
    assert!(home.unwrap().contains("[[account]] # two\nname = \"house\"\naddress = \"ada@gmail.com\""));
}

#[test]
fn what_is_said_of_an_unfinished_rename_names_both_and_what_finishes_it() {
    let said = "the rename of account work to office was begun and not finished, so its mail and its config do not \
                agree; run `tuimex setup`, which finishes it";
    assert_eq!(interrupted("work", "office"), said);
}

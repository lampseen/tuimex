use tuimex_core::{RemoteFolder, Role};

use super::{assign, namespace};

fn folder(name: &str, attributes: &[&str]) -> RemoteFolder {
    let delimiter = Some(if name.starts_with("INBOX.") { '.' } else { '/' });
    RemoteFolder { name: name.to_string(), delimiter, attributes: attributes.iter().map(ToString::to_string).collect() }
}

fn assigned(folders: &[RemoteFolder], configured: &[(Role, &str)], gmail: bool) -> Vec<(String, Role)> {
    let configured: Vec<(Role, String)> = configured.iter().map(|(r, n)| (*r, n.to_string())).collect();
    let mut all = assign(folders, &configured, gmail);
    all.retain(|(_, role)| *role != Role::Other);
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all
}

fn named(pairs: &[(&str, Role)]) -> Vec<(String, Role)> {
    pairs.iter().map(|(n, r)| (n.to_string(), *r)).collect()
}

#[test]
fn special_use_attributes_decide_whatever_the_folders_are_called() {
    let folders = [
        folder("inbox", &[]),
        folder("Kept", &["\\Archive"]),
        folder("Bin", &["\\Trash"]),
        folder("Out", &["\\Sent"]),
        folder("Unfinished", &["\\Drafts"]),
        folder("Rubbish", &["\\Junk"]),
        folder("Trash", &[]),
    ];
    let want = [
        ("Bin", Role::Trash),
        ("Kept", Role::Archive),
        ("Out", Role::Sent),
        ("Rubbish", Role::Junk),
        ("Unfinished", Role::Drafts),
        ("inbox", Role::Inbox),
    ];
    assert_eq!(assigned(&folders, &[], false), named(&want));
}

#[test]
fn without_special_use_each_usual_name_is_found_in_the_plans_order() {
    for (name, role) in [
        ("Archive", Role::Archive),
        ("Trash", Role::Trash),
        ("Deleted Items", Role::Trash),
        ("Deleted Messages", Role::Trash),
        ("Sent", Role::Sent),
        ("Sent Items", Role::Sent),
        ("Sent Messages", Role::Sent),
        ("Drafts", Role::Drafts),
        ("Junk", Role::Junk),
        ("Spam", Role::Junk),
    ] {
        assert_eq!(assigned(&[folder(name, &[])], &[], false), named(&[(name, role)]), "{name}");
    }
    let both = [folder("Deleted Items", &[]), folder("trash", &[]), folder("Spam", &[]), folder("Junk", &[])];
    assert_eq!(assigned(&both, &[], false), named(&[("Junk", Role::Junk), ("trash", Role::Trash)]));
}

#[test]
fn a_name_is_matched_below_the_servers_namespace() {
    let folders = [folder("INBOX", &[]), folder("INBOX.Sent", &[]), folder("INBOX.Trash", &[])];
    let want = [("INBOX", Role::Inbox), ("INBOX.Sent", Role::Sent), ("INBOX.Trash", Role::Trash)];
    assert_eq!(assigned(&folders, &[], false), named(&want));
}

#[test]
fn gmail_archives_to_all_mail_and_keeps_its_own_names() {
    let folders = [
        folder("INBOX", &[]),
        folder("[Gmail]", &["\\Noselect"]),
        folder("[Gmail]/All Mail", &["\\All"]),
        folder("[Gmail]/Trash", &["\\Trash"]),
        folder("[Gmail]/Sent Mail", &["\\Sent"]),
        folder("[Gmail]/Drafts", &["\\Drafts"]),
        folder("[Gmail]/Spam", &["\\Junk"]),
    ];
    let want = [
        ("INBOX", Role::Inbox),
        ("[Gmail]/All Mail", Role::Archive),
        ("[Gmail]/Drafts", Role::Drafts),
        ("[Gmail]/Sent Mail", Role::Sent),
        ("[Gmail]/Spam", Role::Junk),
        ("[Gmail]/Trash", Role::Trash),
    ];
    assert_eq!(assigned(&folders, &[], true), named(&want));
}

#[test]
fn all_is_not_an_archive_on_a_server_that_is_not_gmail() {
    assert_eq!(assigned(&[folder("Everything", &["\\All"])], &[], false), []);
}

#[test]
fn the_config_wins_over_an_attribute_and_a_name() {
    let folders = [folder("Archive", &["\\Archive"]), folder("Old", &[])];
    assert_eq!(assigned(&folders, &[(Role::Archive, "Old")], false), named(&[("Old", Role::Archive)]));
    // A configured folder the server does not have decides nothing.
    assert_eq!(assigned(&folders, &[(Role::Archive, "Gone")], false), named(&[("Archive", Role::Archive)]));
}

#[test]
fn a_role_nothing_fills_is_absent_and_a_folder_that_cannot_be_opened_is_left_out() {
    let folders = [folder("INBOX", &[]), folder("Projects", &[]), folder("Trash", &["\\Noselect"])];
    assert_eq!(assign(&folders, &[], false), named(&[("INBOX", Role::Inbox), ("Projects", Role::Other)]));
}

#[test]
fn the_inbox_is_never_given_another_role() {
    let folders = [folder("INBOX", &["\\Trash"])];
    assert_eq!(assigned(&folders, &[(Role::Archive, "INBOX")], false), named(&[("INBOX", Role::Inbox)]));
}

#[test]
fn the_namespace_is_the_inbox_and_its_delimiter_only_where_every_other_folder_is_under_it() {
    let listed = |names: &[&str], delimiter| -> Vec<RemoteFolder> {
        let folder = |name: &&str| RemoteFolder { name: name.to_string(), delimiter, attributes: Vec::new() };
        names.iter().map(folder).collect()
    };
    assert_eq!(namespace(&listed(&["INBOX", "INBOX.Sent", "INBOX.Drafts"], Some('.'))), "INBOX.");
    assert_eq!(
        namespace(&listed(&["INBOX", "INBOX/Receipts", "Sent"], Some('/'))),
        "",
        "a child of the Inbox is not one"
    );
    assert_eq!(namespace(&listed(&["INBOX", "Sent", "Trash"], Some('/'))), "");
    assert_eq!(namespace(&listed(&["INBOX"], Some('.'))), "", "an Inbox alone says nothing");
    assert_eq!(namespace(&listed(&["INBOX", "INBOX.Sent"], None)), "", "nor does a server with no delimiter");
    assert_eq!(namespace(&listed(&["INBOX", "INBOXES.Sent"], Some('.'))), "");
}

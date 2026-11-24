use std::collections::HashSet;

use super::{FOLDERS, NOW, mailbox};

/// CLAUDE.md: fixtures are synthetic. Every `@` in a message, whether in
/// an address, a Message-ID or a body, is followed by a reserved domain.
#[test]
fn every_address_is_under_a_reserved_example_domain() {
    for mail in mailbox() {
        let raw = String::from_utf8(mail.raw()).unwrap();
        for (at, _) in raw.match_indices('@') {
            let host: String = raw[at + 1..].chars().take_while(|c| c.is_alphanumeric() || ".-".contains(*c)).collect();
            let host = host.trim_end_matches('.');
            assert!(
                ["example.com", "example.org", "example.net"]
                    .iter()
                    .any(|d| host == *d || host.ends_with(&format!(".{d}"))),
                "{}: {host} is not a reserved domain",
                mail.subject
            );
        }
    }
}

#[test]
fn a_key_names_one_message_in_its_account() {
    let mut seen = HashSet::new();
    for mail in mailbox() {
        assert!(seen.insert((mail.account, mail.key.clone())), "{} is used twice in {}", mail.key, mail.account);
    }
}

#[test]
fn both_accounts_have_mail_in_folders_the_table_names() {
    let all = mailbox();
    for account in ["personal", "work"] {
        assert!(all.iter().any(|m| m.account == account && m.folder == "INBOX"));
    }
    for mail in &all {
        assert!(FOLDERS.iter().any(|(name, ..)| *name == mail.folder), "{} is not a fixture folder", mail.folder);
        assert!(mail.date < NOW, "{} is dated in the future", mail.subject);
    }
}

#[test]
fn the_date_header_is_the_date_field() {
    let all = mailbox();
    let first = all.iter().find(|m| m.subject == "Your mooring receipt").unwrap();
    assert!(String::from_utf8(first.raw()).unwrap().contains("Date: 24 Oct 2026 11:01:00 +0000\r\n"));
    assert_eq!(first.date.0, 1_792_839_660);
    let last = all.iter().find(|m| m.subject == "Lunch on Thursday?").unwrap();
    assert!(String::from_utf8(last.raw()).unwrap().contains("Date: 23 Nov 2026 11:05:00 +0000\r\n"));
    assert_eq!(last.date.0, 1_795_431_900);
}

#[test]
fn a_reply_carries_the_thread_it_answers() {
    let all = mailbox();
    let last = all.iter().find(|m| m.folder == "INBOX" && m.subject == "Re: Boat trip planning").unwrap();
    assert_eq!(last.references.len(), 2);
    let raw = String::from_utf8(last.raw()).unwrap();
    assert!(raw.contains(&format!("In-Reply-To: {}\r\n", last.references[1])));
}

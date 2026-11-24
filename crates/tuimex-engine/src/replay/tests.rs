use tuimex_core::{AccountId, Flags, MessageId, Origin, Role};
use tuimex_store::journal::Change;
use tuimex_store::maildir::Delivery;

use super::number;
use crate::tests::{ALL, world};

fn id(key: &str) -> MessageId {
    MessageId { account: AccountId("work".to_string()), key: key.to_string() }
}

#[test]
fn a_moved_message_takes_its_new_number_from_a_file_that_already_had_it_or_goes_when_there_is_none() {
    let w = world("number", ALL);
    w.store
        .run(|s| {
            s.ensure_folder("work", "INBOX", Role::Inbox)?;
            s.ensure_folder("work", "All", Role::Archive)?;
            let file = |key: &str, uid| Delivery {
                key: key.to_string(),
                uid,
                origin: Origin::Server,
                flags: Flags::default(),
            };
            // The message under both of a Gmail account's folders, and
            // two more in the Inbox.
            s.deliver("work", "All", &file("label", Some(5)), b"Subject: one\r\n\r\n")?;
            for key in ["moved", "unfound", "stayed"] {
                s.deliver("work", "INBOX", &file(key, Some(1)), b"Subject: one\r\n\r\n")?;
            }
            for key in ["moved", "unfound"] {
                s.apply(&id(key), &Change::Move { to: "All".to_string() })?;
            }
            number(s, &id("moved"), "All", Some(5))?;
            number(s, &id("unfound"), "All", None)?;
            number(s, &id("stayed"), "All", Some(6))?;
            assert_eq!(s.stored(&id("moved"))?.1.uid, Some(5), "the file `u` remembers is the one kept");
            assert!(s.stored(&id("label")).is_err() && s.stored(&id("unfound")).is_err());
            assert_eq!(s.stored(&id("stayed"))?.0, "INBOX", "a message that has moved on is left alone");
            Ok(())
        })
        .unwrap();
}

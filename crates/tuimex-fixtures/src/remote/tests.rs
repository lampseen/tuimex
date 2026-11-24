use std::thread;
use std::time::{Duration, Instant};

use tuimex_core::{
    Capabilities, Deadline, Flags, MoveOutcome, Remote, RemoteError, RemoteWrite, Timestamp, UidRange, Woken,
};

use super::FakeServer;

const ALL: Capabilities =
    Capabilities { uidplus: true, r#move: true, condstore: true, special_use: true, idle: true, gmail: false };
const SEEN: Flags = Flags { seen: true, flagged: false, answered: false, draft: false };

fn server(capabilities: Capabilities) -> FakeServer {
    let server = FakeServer::new("pw", capabilities);
    server.add_folder("INBOX", &[], 7);
    server.add_folder("Trash", &["\\Trash"], 8);
    for n in 1..=3 {
        let raw = format!("Subject: message {n}\r\n\r\nbody {n}\r\n");
        server.deliver("INBOX", raw.as_bytes(), Flags::default(), Timestamp(n));
    }
    server
}

fn soon() -> Deadline {
    Deadline(Instant::now() + Duration::from_secs(20))
}

#[test]
fn a_wrong_password_is_refused_and_the_right_one_is_not() {
    let server = server(ALL);
    assert_eq!(server.connect("guess").err(), Some(RemoteError::Auth));
    assert!(server.connect("pw").is_ok());
}

#[test]
fn an_envelope_is_the_header_block_with_the_whole_messages_size() {
    let mut remote = server(ALL).connect("pw").unwrap();
    assert_eq!(remote.select("INBOX").unwrap().uid_validity, 7);
    let fetched = remote.envelopes(UidRange { first: 2, last: Some(3) }).unwrap();
    assert_eq!(fetched.iter().map(|f| f.uid).collect::<Vec<_>>(), [2, 3]);
    assert_eq!(fetched[0].headers, b"Subject: message 2\r\n\r\n");
    assert_eq!(fetched[0].size as usize, remote.body(2).unwrap().len());
}

#[test]
fn flags_since_a_modseq_are_only_the_ones_changed_after_it() {
    let server = server(ALL);
    let mut remote = server.connect("pw").unwrap();
    let before = remote.select("INBOX").unwrap().highest_modseq.unwrap();
    server.set_flags("INBOX", 2, SEEN);
    let changed = remote.flags(Some(before)).unwrap();
    assert_eq!(changed.iter().map(|c| (c.uid, c.flags)).collect::<Vec<_>>(), [(2, SEEN)]);
    assert_eq!(remote.flags(None).unwrap().len(), 3);
}

#[test]
fn a_server_without_condstore_reports_no_modseq_and_refuses_to_be_asked_for_changes() {
    let mut remote = server(Capabilities { condstore: false, ..ALL }).connect("pw").unwrap();
    assert_eq!(remote.select("INBOX").unwrap().highest_modseq, None);
    assert!(matches!(remote.flags(Some(1)), Err(RemoteError::Refused(_))));
}

#[test]
fn a_server_without_special_use_lists_no_attributes() {
    let with = server(ALL).connect("pw").unwrap().folders().unwrap();
    assert_eq!(with[1].attributes, ["\\Trash"]);
    let without = server(Capabilities { special_use: false, ..ALL }).connect("pw").unwrap().folders().unwrap();
    assert!(without.iter().all(|f| f.attributes.is_empty()));
}

#[test]
fn the_connection_is_lost_after_the_scripted_number_of_commands_and_stays_lost() {
    let server = server(ALL);
    let mut remote = server.connect("pw").unwrap();
    server.drop_after(2);
    remote.select("INBOX").unwrap();
    remote.uids().unwrap();
    assert!(matches!(remote.uids(), Err(RemoteError::Connection(_))));
    assert!(matches!(remote.select("INBOX"), Err(RemoteError::Connection(_))));
    assert_eq!(server.take_log(), ["select INBOX", "uids"]);
}

#[test]
fn a_renumbered_folder_has_a_new_uidvalidity_and_uids_from_one() {
    let server = server(ALL);
    server.remove("INBOX", 1);
    server.renumber("INBOX", 70);
    let mut remote = server.connect("pw").unwrap();
    let selected = remote.select("INBOX").unwrap();
    assert_eq!((selected.uid_validity, selected.uid_next), (70, 3));
    assert_eq!(remote.uids().unwrap(), [1, 2]);
}

#[test]
fn a_move_with_move_takes_the_message_out_in_one_command() {
    let server = server(ALL);
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    server.take_log();
    assert_eq!(remote.move_to(&[2, 99], "Trash").unwrap(), MoveOutcome::Moved, "a UID nobody has is passed over");
    assert_eq!(server.take_log(), ["move [2, 99] Trash"]);
    assert_eq!(remote.uids().unwrap(), [1, 3]);
    remote.select("Trash").unwrap();
    assert_eq!(remote.uids().unwrap(), [1]);
}

#[test]
fn a_move_without_move_or_uidplus_copies_and_marks_and_is_never_an_expunge() {
    let server = server(Capabilities { r#move: false, uidplus: false, ..ALL });
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    server.take_log();
    assert_eq!(remote.move_to(&[2], "Trash").unwrap(), MoveOutcome::CopiedAndFlagged);
    assert_eq!(server.take_log(), ["copy [2] Trash", "store-deleted [2]"]);
    assert_eq!(remote.uids().unwrap(), [1, 3], "what is marked deleted is no longer listed");
    assert_eq!(remote.body(2).unwrap(), b"Subject: message 2\r\n\r\nbody 2\r\n", "and is still there to be purged");
    remote.select("Trash").unwrap();
    assert_eq!(remote.uids().unwrap(), [1]);
}

#[test]
fn a_move_without_move_and_with_uidplus_purges_the_messages_it_names() {
    let server = server(Capabilities { r#move: false, ..ALL });
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    server.take_log();
    assert_eq!(remote.move_to(&[2], "Trash").unwrap(), MoveOutcome::Moved);
    assert_eq!(server.take_log(), ["copy [2] Trash", "store-deleted [2]", "uid-expunge [2]"]);
    assert!(matches!(remote.body(2), Err(RemoteError::Refused(_))));
    assert_eq!(remote.uids().unwrap(), [1, 3]);
}

#[test]
fn a_message_is_found_by_its_message_id_in_the_selected_folder() {
    let server = server(ALL);
    let with_id = |id: &str| format!("Subject: found\r\nMESSAGE-ID: <{id}>\r\n\r\n<other@example.net>\r\n");
    server.deliver("INBOX", with_id("one@example.net").as_bytes(), Flags::default(), Timestamp(4));
    server.deliver("INBOX", with_id("other@example.net").as_bytes(), Flags::default(), Timestamp(5));
    server.deliver("INBOX", with_id("one@example.net").as_bytes(), Flags::default(), Timestamp(6));
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    assert_eq!(remote.find("one@example.net").unwrap(), [4, 6], "a body that quotes the id is no match");
    assert_eq!(remote.find("nobody@example.net").unwrap(), Vec::<u32>::new());
    remote.select("Trash").unwrap();
    assert_eq!(remote.find("one@example.net").unwrap(), Vec::<u32>::new());
}

#[test]
fn a_refused_move_moves_nothing() {
    let server = server(ALL);
    server.refuse_moves();
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    assert!(matches!(remote.move_to(&[2], "Trash"), Err(RemoteError::Refused(_))));
    assert_eq!(remote.uids().unwrap(), [1, 2, 3]);
}

#[test]
fn stored_flags_are_set_and_cleared_and_an_append_arrives() {
    let server = server(ALL);
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    remote.store_flags(&[1], Flags { seen: true, flagged: true, ..Flags::default() }, Flags::default()).unwrap();
    remote.store_flags(&[1], Flags::default(), Flags { flagged: true, ..Flags::default() }).unwrap();
    assert_eq!(server.flags("INBOX", 1), Some(SEEN));
    remote.store_flags(&[99], SEEN, Flags::default()).unwrap();
    remote.append("Trash", b"Subject: kept\r\n\r\n", SEEN).unwrap();
    assert_eq!(server.flags("Trash", 1), Some(SEEN));
    assert!(matches!(remote.append("Nowhere", b"Subject: lost\r\n\r\n", SEEN), Err(RemoteError::Refused(_))));
}

#[test]
fn idle_wakes_when_another_client_delivers_and_times_out_when_nobody_does() {
    let server = server(ALL);
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    assert_eq!(remote.idle(Deadline(Instant::now())).unwrap(), Woken::TimedOut);
    server.take_log();
    let other = server.clone();
    let idling = thread::spawn(move || remote.idle(soon()));
    while !server.take_log().contains(&"idle".to_string()) {
        thread::yield_now();
    }
    other.deliver("INBOX", b"Subject: new\r\n\r\n", Flags::default(), Timestamp(9));
    assert_eq!(idling.join().unwrap().unwrap(), Woken::Changed);
}

#[test]
fn a_change_made_before_the_idle_began_ends_it_at_once_and_only_once() {
    let server = server(ALL);
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    server.remove("INBOX", 1);
    assert_eq!(remote.idle(soon()).unwrap(), Woken::Changed);
    assert_eq!(remote.idle(Deadline(Instant::now())).unwrap(), Woken::TimedOut);
}

#[test]
fn an_interrupt_from_another_thread_ends_an_idle_and_the_connection() {
    let server = server(ALL);
    let mut remote = server.connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    let interrupt = remote.interrupt();
    let idling = thread::spawn(move || {
        let ended = remote.idle(soon());
        (ended, remote.uids())
    });
    while !server.take_log().contains(&"idle".to_string()) {
        thread::yield_now();
    }
    interrupt.fire();
    let (ended, after) = idling.join().unwrap();
    assert!(matches!(ended, Err(RemoteError::Connection(_))));
    assert!(matches!(after, Err(RemoteError::Connection(_))));
}

#[test]
fn a_server_without_idle_refuses_it() {
    let mut remote = server(Capabilities { idle: false, ..ALL }).connect("pw").unwrap();
    remote.select("INBOX").unwrap();
    assert!(matches!(remote.idle(soon()), Err(RemoteError::Refused(_))));
}

#[test]
fn refused_connections_run_out() {
    let server = server(ALL);
    server.refuse_connections(1);
    assert!(matches!(server.connect("pw").err(), Some(RemoteError::Connection(_))));
    assert!(server.connect("pw").is_ok());
}

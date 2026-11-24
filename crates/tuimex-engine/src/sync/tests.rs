use std::cell::RefCell;

use tuimex_core::{AccountId, Capabilities, Event, Flags, FolderId, MessageId, Origin, RemoteError, Role, Timestamp};
use tuimex_fixtures::NOW;
use tuimex_store::journal::Change;
use tuimex_store::maildir::Delivery;

use super::{BATCH, Pass, Policy, account, folder};
use crate::tests::{ALL, PASSWORD, World, message, world};
use crate::{Account, Error};

const NONE: Flags = Flags { seen: false, flagged: false, answered: false, draft: false };
const SEEN: Flags = Flags { seen: true, ..NONE };
const FLAGGED: Flags = Flags { flagged: true, ..NONE };
const WITHOUT_CONDSTORE: Capabilities = Capabilities { condstore: false, ..ALL };

fn set(flags: Flags) -> Change {
    Change::Flags { set: flags, clear: NONE }
}

impl World {
    fn pass_with(&self, policy: &Policy) -> Result<Pass, Error> {
        let mut remote = self.server.connect(PASSWORD)?;
        self.store.run(|s| s.ensure_folder("work", "INBOX", Role::Inbox))?;
        folder(&mut remote, &self.store, ("work", "INBOX"), policy, NOW, &|_, _| {})
    }

    /// One pass over the Inbox on a new connection.
    fn pass(&self) -> Pass {
        self.pass_with(&Policy::default()).unwrap()
    }

    /// A world whose Inbox holds messages 1 to `count`, synced once.
    fn synced(name: &str, capabilities: Capabilities, count: u32) -> World {
        let w = world(name, capabilities);
        (1..=count).for_each(|n| _ = w.deliver(n));
        w.pass();
        w.server.take_log();
        w
    }
}

#[test]
fn a_first_sync_files_every_message_whole_with_its_uid_and_flags() {
    let w = world("first", ALL);
    w.deliver(1);
    w.server.deliver("INBOX", &message(2), Flags { seen: true, answered: true, ..NONE }, NOW);
    let pass = w.pass();
    assert_eq!((pass.changed.len(), pass.arrived), (2, 0), "a first sync announces no new mail");
    let want = [
        (1, "message 1".to_string(), NONE, false),
        (2, "message 2".to_string(), Flags { seen: true, answered: true, ..NONE }, false),
    ];
    assert_eq!(w.here("INBOX"), want);
    assert_eq!(
        w.store
            .run(|s| s.read(&MessageId {
                account: AccountId("work".to_string()),
                key: s.synced("work", "INBOX")?[0].key.clone()
            }))
            .unwrap(),
        message(1)
    );
    let position = w.store.run(|s| s.position("work", "INBOX")).unwrap();
    assert_eq!((position.uid_validity, position.highest_uid), (Some(7), Some(2)));
    assert!(position.highest_modseq.is_some());
}

#[test]
fn a_second_sync_transfers_nothing() {
    let w = World::synced("second", ALL, 3);
    assert_eq!(w.pass(), Pass::default());
    assert_eq!(w.server.take_log(), ["select INBOX", "uids"]);
}

#[test]
fn a_second_sync_without_condstore_asks_for_flags_and_no_message() {
    let w = World::synced("second-plain", WITHOUT_CONDSTORE, 3);
    assert_eq!(w.pass(), Pass::default());
    assert_eq!(w.server.take_log(), ["select INBOX", "uids", "flags"]);
}

#[test]
fn a_uidvalidity_change_drops_the_folder_and_syncs_it_again_from_nothing() {
    let w = World::synced("uidvalidity", ALL, 3);
    let old: Vec<_> = (1..=3).map(|uid| w.id(uid)).collect();
    // A draft the server never numbered, a flag change and a move it has
    // not heard of: the first is not the server's to drop, and the other
    // two name UIDs that are about to mean something else.
    let draft = Delivery { key: "draft".to_string(), uid: None, origin: Origin::Local, flags: NONE };
    w.store.run(move |s| s.deliver("work", "INBOX", &draft, b"Subject: a local draft\r\n\r\n")).unwrap();
    w.store.run(|s| s.ensure_folder("work", "Archive", Role::Archive)).unwrap();
    let (flagged, moved) = (old[1].clone(), old[2].clone());
    w.store.run(move |s| s.change(&flagged, set(FLAGGED))).unwrap();
    w.store.run(move |s| s.change(&moved, Change::Move { to: "Archive".to_string() })).unwrap();

    w.server.remove("INBOX", 1);
    w.server.renumber("INBOX", 70);
    let pass = w.pass();

    let subjects: Vec<(u32, String)> = w.here("INBOX").into_iter().map(|(uid, subject, ..)| (uid, subject)).collect();
    let want = [(0, "a local draft"), (1, "message 2"), (2, "message 3")].map(|(uid, s)| (uid, s.to_string()));
    assert_eq!(subjects, want);
    assert_eq!(w.here("INBOX")[1].2, NONE, "the flag set under the old UIDs is gone with them");
    assert_eq!(w.here("Archive"), [], "the copy moved away under an old UID came back with the folder");
    assert_eq!(w.journal(), [], "no journal entry names a UID from before the change");
    assert_eq!(w.store.run(|s| s.position("work", "INBOX")).unwrap().uid_validity, Some(70));
    for id in &old {
        assert!(pass.changed.contains(id), "{id:?} was dropped and is reported");
        assert!(
            w.store
                .run({
                    let id = id.clone();
                    move |s| s.read(&id)
                })
                .is_err()
        );
    }
    assert_eq!(w.server.take_log(), ["select INBOX", "uids", "envelopes 1:2", "body 1", "body 2", "flags"]);
    assert_eq!(w.pass(), Pass::default(), "and the pass after it has nothing to do");
}

#[test]
fn a_connection_dropped_mid_batch_keeps_what_was_filed_and_the_next_pass_fetches_the_rest() {
    let w = world("dropped", ALL);
    let count = BATCH as u32 * 2 + 20;
    (1..=count).for_each(|n| _ = w.deliver(n));
    // select, uids, the first batch's envelopes and bodies, the second
    // batch's envelopes and half its bodies.
    w.server.drop_after(2 + 1 + BATCH + 1 + BATCH / 2);
    let failed = w.pass_with(&Policy::default()).unwrap_err();
    assert!(matches!(failed, Error::Remote(RemoteError::Connection(_))), "{failed:?}");
    assert!(failed.retries());
    assert_eq!(w.uids("INBOX"), (1..=BATCH as u32).collect::<Vec<_>>(), "the batch that was whole is kept");

    w.server.take_log();
    let pass = w.pass();
    assert_eq!(pass.changed.len(), BATCH + 20);
    assert_eq!(w.uids("INBOX"), (1..=count).collect::<Vec<_>>());
    let log = w.server.take_log();
    let second = format!("envelopes {}:{}", BATCH + 1, BATCH * 2);
    assert_eq!(log[..3], ["select INBOX".to_string(), "uids".to_string(), second]);
    assert!(!log.iter().any(|line| line == "body 1" || line == &format!("body {BATCH}")), "nothing is fetched twice");
}

#[test]
fn a_message_removed_elsewhere_is_removed_here_with_any_change_waiting_for_it() {
    for (name, capabilities) in [("removed", ALL), ("removed-plain", WITHOUT_CONDSTORE)] {
        let w = World::synced(name, capabilities, 3);
        let gone = w.id(2);
        w.store
            .run({
                let gone = gone.clone();
                move |s| s.change(&gone, set(FLAGGED))
            })
            .unwrap();
        w.server.remove("INBOX", 2);
        let pass = w.pass();
        assert_eq!(pass.changed, [gone]);
        assert_eq!(w.uids("INBOX"), [1, 3]);
        assert_eq!(w.journal(), []);
        assert!(!w.server.take_log().iter().any(|line| line.starts_with("envelopes")));
    }
}

#[test]
fn flags_changed_on_the_server_arrive_with_condstore_and_without() {
    for (name, capabilities, asked) in [("flags", ALL, "flags since"), ("flags-plain", WITHOUT_CONDSTORE, "flags")] {
        let w = World::synced(name, capabilities, 3);
        w.server.set_flags("INBOX", 2, Flags { seen: true, flagged: true, ..NONE });
        let pass = w.pass();
        assert_eq!(pass.changed, [w.id(2)]);
        assert_eq!(w.here("INBOX")[1].2, Flags { seen: true, flagged: true, ..NONE });
        assert!(w.server.take_log()[2].starts_with(asked));
        w.server.set_flags("INBOX", 2, NONE);
        w.pass();
        assert_eq!(w.here("INBOX")[1].2, NONE, "a flag cleared on the server is cleared here");
    }
}

#[test]
fn a_flag_changed_only_here_waits_in_the_journal_and_is_not_undone_by_a_sync() {
    for (name, capabilities) in [("mine", ALL), ("mine-plain", WITHOUT_CONDSTORE)] {
        let w = World::synced(name, capabilities, 2);
        let id = w.id(1);
        w.store.run(move |s| s.change(&id, set(FLAGGED))).unwrap();
        // Another message changing is what makes a CONDSTORE server
        // report anything at all.
        w.server.set_flags("INBOX", 2, SEEN);
        w.pass();
        assert_eq!(w.here("INBOX")[0].2, FLAGGED);
        assert_eq!(w.journal().len(), 1);
    }
}

#[test]
fn a_flag_changed_on_both_sides_takes_the_servers_value() {
    let w = World::synced("conflict", ALL, 1);
    let id = w.id(1);
    w.store.run(move |s| s.change(&id, set(FLAGGED))).unwrap();
    w.server.set_flags("INBOX", 1, Flags { seen: true, answered: true, ..NONE });
    assert_eq!(w.pass().changed, [w.id(1)]);
    assert_eq!(w.here("INBOX")[0].2, Flags { seen: true, answered: true, ..NONE });
    assert_eq!(w.journal(), [], "the change the server overrode is not replayed against it");
}

#[test]
fn seen_set_here_survives_a_conflict_and_stays_in_the_journal_for_the_server() {
    let w = World::synced("seen", ALL, 1);
    let id = w.id(1);
    w.store.run(move |s| s.change(&id, set(Flags { seen: true, answered: true, ..NONE }))).unwrap();
    w.server.set_flags("INBOX", 1, FLAGGED);
    w.pass();
    assert_eq!(w.here("INBOX")[0].2, Flags { seen: true, flagged: true, ..NONE });
    let journal = w.journal();
    assert_eq!(journal.len(), 1);
    assert_eq!((journal[0].uid, &journal[0].change), (Some(1), &set(SEEN)));
    w.pass();
    assert_eq!(w.here("INBOX")[0].2, Flags { seen: true, flagged: true, ..NONE }, "and the next pass leaves it be");
    assert_eq!(w.journal().len(), 1);
}

#[test]
fn seen_cleared_here_and_set_on_the_server_stays_set() {
    let w = world("unseen", ALL);
    w.server.deliver("INBOX", &message(1), SEEN, NOW);
    w.pass();
    let id = w.id(1);
    w.store.run(move |s| s.change(&id, Change::Flags { set: NONE, clear: SEEN })).unwrap();
    w.server.set_flags("INBOX", 1, Flags { seen: true, flagged: true, ..NONE });
    w.pass();
    assert_eq!(w.here("INBOX")[0].2, Flags { seen: true, flagged: true, ..NONE });
    assert_eq!(w.journal(), []);
}

#[test]
fn an_old_or_large_message_arrives_as_its_headers_and_a_recent_small_one_whole() {
    let w = world("bodies", ALL);
    let policy = Policy { bodies: 86_400, max_body: message(1).len() as u32, folders: Vec::new() };
    let large = [message(3), vec![b'x'; 10]].concat();
    w.server.deliver("INBOX", &message(1), NONE, Timestamp(NOW.0 - 86_400));
    w.server.deliver("INBOX", &message(2), NONE, Timestamp(NOW.0 - 86_401));
    w.server.deliver("INBOX", &large, NONE, NOW);
    w.pass_with(&policy).unwrap();
    let whole: Vec<(u32, bool)> =
        w.here("INBOX").into_iter().map(|(uid, _, _, headers_only)| (uid, !headers_only)).collect();
    assert_eq!(whole, [(1, true), (2, false), (3, false)]);
    assert_eq!(w.server.take_log(), ["select INBOX", "uids", "envelopes 1:3", "body 1", "flags"]);
    assert_eq!(w.here("INBOX")[1].1, "message 2", "a header block is still listed by what its headers say");
    w.pass_with(&policy).unwrap();
    assert_eq!(w.server.take_log(), ["select INBOX", "uids"], "a header block is not something still to fetch");
}

#[test]
fn a_message_moved_here_and_not_yet_on_the_server_is_not_fetched_again() {
    let w = World::synced("moved", ALL, 2);
    w.store.run(|s| s.ensure_folder("work", "Archive", Role::Archive)).unwrap();
    let id = w.id(2);
    w.store.run(move |s| s.change(&id, Change::Move { to: "Archive".to_string() })).unwrap();
    assert_eq!(w.pass(), Pass::default());
    assert_eq!(w.server.take_log(), ["select INBOX", "uids"]);
    assert_eq!((w.uids("INBOX"), w.here("Archive").len()), (vec![1], 1));
    assert_eq!(w.journal().len(), 1, "the move still waits for the server");
}

fn the_account(folders: &[&str]) -> Account {
    let policy = Policy { folders: folders.iter().map(ToString::to_string).collect(), ..Policy::default() };
    Account { policy, ..crate::tests::the_account() }
}

fn account_pass(w: &World, account_: &Account) -> Vec<Event> {
    let events = RefCell::new(Vec::new());
    let mut remote = w.server.connect(PASSWORD).unwrap();
    let tell = |report| match report {
        crate::Report::Event(event) => events.borrow_mut().push(event),
        crate::Report::Refused { reason, .. } => panic!("nothing was journaled, and the server refused: {reason}"),
    };
    account(&mut remote, &w.store, account_, NOW, &tell).unwrap();
    events.into_inner()
}

#[test]
fn an_account_pass_syncs_every_folder_with_a_role_and_those_the_config_names() {
    let w = world("account", ALL);
    w.deliver(1);
    w.server.deliver("Archive", &message(2), SEEN, NOW);
    w.server.deliver("Projects", &message(3), SEEN, NOW);
    let synced = |events: Vec<Event>| -> Vec<(String, usize)> {
        let folder = |e| match e {
            Event::Synced { account: AccountId(a), folder: FolderId(f), changed } if a == "work" => {
                Some((f, changed.len()))
            }
            _ => None,
        };
        events.into_iter().filter_map(folder).collect()
    };
    let named = |pairs: &[(&str, usize)]| pairs.iter().map(|(f, n)| (f.to_string(), *n)).collect::<Vec<_>>();
    assert_eq!(synced(account_pass(&w, &the_account(&[]))), named(&[("INBOX", 1), ("Archive", 1), ("Trash", 0)]));
    let roles = |w: &World| -> Vec<(String, Role)> {
        w.store.run(|s| s.folders()).unwrap().into_iter().map(|f| (f.name, f.role)).collect()
    };
    let want = [("Archive", Role::Archive), ("INBOX", Role::Inbox), ("Trash", Role::Trash)];
    assert_eq!(roles(&w), want.map(|(f, r)| (f.to_string(), r)));
    let again = account_pass(&w, &the_account(&["Projects"]));
    assert_eq!(synced(again), named(&[("INBOX", 0), ("Archive", 0), ("Trash", 0), ("Projects", 1)]));
    assert_eq!(w.here("Projects")[0].1, "message 3");
}

#[test]
fn unread_mail_new_to_the_inbox_is_announced_after_the_first_sync_and_not_by_it() {
    let w = world("newmail", ALL);
    w.deliver(1);
    let new_mail = |events: Vec<Event>| -> Vec<u32> {
        events
            .into_iter()
            .filter_map(|e| if let Event::NewMail { count, .. } = e { Some(count) } else { None })
            .collect()
    };
    assert_eq!(new_mail(account_pass(&w, &the_account(&[]))), Vec::<u32>::new());
    w.deliver(2);
    w.deliver(3);
    w.server.deliver("INBOX", &message(4), SEEN, NOW);
    w.server.deliver("Archive", &message(5), NONE, NOW);
    assert_eq!(new_mail(account_pass(&w, &the_account(&[]))), [2], "read mail and other folders ring nothing");
    assert_eq!(new_mail(account_pass(&w, &the_account(&[]))), Vec::<u32>::new());
}

#[test]
fn a_first_sync_says_how_far_it_has_got_after_each_batch_and_a_later_pass_says_nothing() {
    let w = world("progress", ALL);
    (1..=250).for_each(|n| _ = w.deliver(n));
    let said = RefCell::new(Vec::new());
    let pass = |w: &World| {
        let mut remote = w.server.connect(PASSWORD).unwrap();
        w.store.run(|s| s.ensure_folder("work", "INBOX", Role::Inbox)).unwrap();
        let progress = |done, total| said.borrow_mut().push((done, total));
        folder(&mut remote, &w.store, ("work", "INBOX"), &Policy::default(), NOW, &progress).unwrap();
    };
    pass(&w);
    assert_eq!(*said.borrow(), [(100, 250), (200, 250), (250, 250)]);
    w.deliver(251);
    pass(&w);
    assert_eq!(said.borrow().len(), 3, "new mail in a folder already here is not a first sync");
}

/// A first sync shows the Inbox long before it has reached the Archive.
/// `e` on a message that had just arrived said "no Archive folder" until
/// the Archive's own turn came.
#[test]
fn every_folder_is_known_here_by_the_time_the_first_is_filled_so_what_has_arrived_can_be_archived_at_once() {
    let w = world("folders-first", ALL);
    w.deliver(1);
    let known = RefCell::new(Vec::new());
    let mut remote = w.server.connect(PASSWORD).unwrap();
    let tell = |report| {
        if let crate::Report::Event(Event::Synced { folder: FolderId(folder), .. }) = report
            && folder == "INBOX"
        {
            // What the interface's reading of the store finds when the
            // Inbox's messages are handed to it.
            let folders = w.store.run(|s| s.folders()).unwrap();
            known.borrow_mut().extend(folders.into_iter().map(|f| f.role));
        }
    };
    account(&mut remote, &w.store, &the_account(&[]), NOW, &tell).unwrap();
    assert_eq!(known.into_inner(), [Role::Archive, Role::Inbox, Role::Trash]);
}

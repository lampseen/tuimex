//! Actions through a running engine: done here at once, replayed to the
//! server, undone (ADR-014, ADR-015).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use tuimex_core::{AccountId, Action, Ask, Capabilities, Event, Flags, FolderId, MessageId, Origin, Role};
use tuimex_fixtures::{FakeServer, NOW};
use tuimex_store::journal::Change;
use tuimex_store::maildir::Delivery;

use crate::tests::{Door, PASSWORD, PATIENCE, PostOffice, QUIET, Running, message, start, the_account};
use crate::{Account, Engine, Engines};

fn two(name: &str, capabilities: Capabilities) -> Running {
    let r = start(name, capabilities, PASSWORD, |w| (1..=2).for_each(|n| _ = w.deliver(n)));
    r.settle();
    r
}

impl Running {
    /// Cuts the engine off from its server: the connections it has are
    /// lost at their next command, and no new one is answered.
    fn offline(&self) {
        self.world.server.refuse_connections(usize::MAX);
        self.world.server.drop_after(0);
    }

    fn online(&self) {
        self.world.server.refuse_connections(0);
        self.engine.sync();
        self.wait_for(|e| matches!(e, Event::Synced { folder: FolderId(f), .. } if f == "Trash").then_some(()));
    }

    fn folder_of(&self, id: &MessageId) -> (String, Option<u32>) {
        let id = id.clone();
        self.world.store.run(move |s| s.stored(&id).map(|(folder, stored)| (folder, stored.uid))).unwrap()
    }
}

fn done(said: &str) -> Result<String, (String, String)> {
    Ok(said.to_string())
}

fn failed(reason: &str, hint: &str) -> Result<String, (String, String)> {
    Err((reason.to_string(), hint.to_string()))
}

#[test]
fn a_move_made_offline_replays_on_reconnect() {
    let r = two("offline", QUIET);
    let id = r.world.id(2);
    r.offline();
    assert_eq!(r.act(Action::Archive, std::slice::from_ref(&id)), done("Archived 1 message"));
    assert_eq!(r.folder_of(&id), ("Archive".to_string(), None), "it is done here at once");
    r.retry_in();
    assert_eq!(r.world.journal().len(), 1, "and waits in the journal while nobody answers");
    r.online();
    r.replayed();
    assert_eq!((r.world.there("INBOX"), r.world.there("Archive")), (vec![1], vec![1]));
    assert_eq!(r.folder_of(&id), ("Archive".to_string(), Some(1)), "under the number its new folder gave it");
    assert_eq!(r.world.uids("INBOX"), [1]);
    r.engine.stop();
}

#[test]
fn a_refused_move_is_undone_locally_and_reported() {
    let r = two("refused", QUIET);
    let id = r.world.id(2);
    r.world.server.refuse_moves();
    assert_eq!(r.act(Action::Trash, std::slice::from_ref(&id)), done("Trashed 1 message"));
    let refusal = failed("the move was refused", "1 message back in INBOX");
    assert_eq!(r.told(), (Some(r.serial.get()), refusal), "the refusal names the ask");
    assert_eq!(r.folder_of(&id), ("INBOX".to_string(), Some(2)), "with the number it had");
    let undo = r.act(Action::Undo, std::slice::from_ref(&id));
    assert_eq!(undo, failed("there is nothing to undo", ""), "what was refused is not there for u");
    assert_eq!((r.world.uids("INBOX"), r.world.here("Trash").len(), r.world.journal().len()), (vec![1, 2], 0, 0));
    r.world.server.take_log();
    r.online();
    assert!(!r.world.server.take_log().iter().any(|l| l.starts_with("envelopes") || l.starts_with("move")));
    r.engine.stop();
}

#[test]
fn a_server_without_uidplus_never_receives_expunge_and_is_said_to_purge_for_itself() {
    let r = two("no-uidplus", Capabilities { r#move: false, uidplus: false, ..QUIET });
    let id = r.world.id(2);
    assert_eq!(r.act(Action::Archive, std::slice::from_ref(&id)), done("Archived 1 message"));
    assert_eq!(r.said(), done("work cannot remove what it has copied; it will purge the originals itself"));
    r.replayed();
    r.online();
    let log = r.world.server.take_log();
    assert!(log.contains(&"copy [2] Archive".to_string()) && log.contains(&"store-deleted [2]".to_string()), "{log:?}");
    assert!(!log.iter().any(|line| line.contains("expunge")), "{log:?}");
    assert_eq!(r.world.uids("INBOX"), [1], "the original, marked deleted, does not come back");
    assert_eq!(r.folder_of(&id), ("Archive".to_string(), Some(1)));
    r.engine.stop();
}

#[test]
fn undo_before_the_server_has_heard_leaves_nothing_to_tell_it() {
    let r = two("undo-offline", QUIET);
    let id = r.world.id(2);
    r.offline();
    r.act(Action::Archive, std::slice::from_ref(&id)).unwrap();
    r.retry_in();
    let undo = || r.act(Action::Undo, std::slice::from_ref(&id));
    assert_eq!(undo(), done("Undone: 1 message back in INBOX"));
    assert_eq!((r.folder_of(&id), r.world.journal().len()), (("INBOX".to_string(), Some(2)), 0));
    r.world.server.take_log();
    r.online();
    let log = r.world.server.take_log();
    assert!(!log.iter().any(|l| l.starts_with("move") || l.starts_with("envelopes")), "{log:?}");
    assert_eq!(undo(), failed("there is nothing to undo", ""));
    r.engine.stop();
}

#[test]
fn undo_after_the_server_applied_a_move_moves_it_back_there_too() {
    for (name, action, folder) in [("undo-archive", Action::Archive, "Archive"), ("undo-trash", Action::Trash, "Trash")]
    {
        let r = two(name, QUIET);
        let id = r.world.id(2);
        r.act(action, std::slice::from_ref(&id)).unwrap();
        r.replayed();
        assert_eq!((r.world.there("INBOX"), r.world.there(folder)), (vec![1], vec![1]));
        assert_eq!(r.act(Action::Undo, std::slice::from_ref(&id)), done("Undone: 1 message back in INBOX"));
        r.replayed();
        assert_eq!((r.world.there("INBOX"), r.world.there(folder)), (vec![1, 3], vec![]));
        assert_eq!(r.folder_of(&id), ("INBOX".to_string(), Some(3)), "the same message, under its new number");
        r.online();
        assert_eq!((r.world.uids("INBOX"), r.world.here(folder).len()), (vec![1, 3], 0), "and nothing arrives twice");
        r.engine.stop();
    }
}

#[test]
fn flags_reach_the_server_and_undo_clears_them_there() {
    let r = two("act-flags", QUIET);
    let ids = [r.world.id(1), r.world.id(2)];
    assert_eq!(r.act(Action::Flag(true), &ids), done("Flagged 2 messages"));
    r.replayed();
    let flagged = Flags { flagged: true, ..Flags::default() };
    assert_eq!((r.world.server.flags("INBOX", 1), r.world.server.flags("INBOX", 2)), (Some(flagged), Some(flagged)));
    assert_eq!(r.act(Action::Seen(true), &ids[..1]), done("Marked 1 message read"));
    r.replayed();
    assert_eq!(r.world.server.flags("INBOX", 1), Some(Flags { seen: true, ..flagged }));
    assert_eq!(r.act(Action::Undo, &ids[..1]), done("Undone: 1 message as before"));
    r.replayed();
    assert_eq!(r.world.server.flags("INBOX", 1), Some(flagged));
    assert_eq!(r.world.here("INBOX")[0].2, flagged);
    assert_eq!(r.act(Action::Flag(true), &ids), done("Flagged 0 messages"), "what is already so is not done again");
    assert_eq!(r.act(Action::Undo, &ids), failed("there is nothing to undo", ""), "nor is it there for u");
    assert_eq!(r.act(Action::Undo, &ids), done("Undone: 2 messages as before"), "and the ask before it still is");
    r.engine.stop();
}

#[test]
fn a_message_moved_and_then_flagged_offline_is_found_again_on_the_server() {
    let r = two("chained", QUIET);
    let id = r.world.id(2);
    r.offline();
    r.act(Action::Archive, std::slice::from_ref(&id)).unwrap();
    r.act(Action::Flag(true), std::slice::from_ref(&id)).unwrap();
    assert_eq!(r.world.journal()[1].uid, None, "it has no number in the folder it was moved to");
    r.online();
    r.replayed();
    assert_eq!(r.world.server.flags("Archive", 1), Some(Flags { flagged: true, ..Flags::default() }));
    assert_eq!(r.folder_of(&id), ("Archive".to_string(), Some(1)));
    r.engine.stop();
}

#[test]
fn what_cannot_be_done_says_why_and_leaves_the_last_undo_alone() {
    let r = two("refusals", QUIET);
    let id = r.world.id(2);
    r.act(Action::Archive, std::slice::from_ref(&id)).unwrap();
    let nowhere = Action::Move(FolderId("Nowhere".to_string()));
    assert_eq!(r.act(nowhere, std::slice::from_ref(&id)), failed("no folder \"Nowhere\" in work", "Tab completes"));
    let compose = Action::Compose(tuimex_core::Start::New);
    assert!(r.act(compose, &[]).unwrap_err().0.contains("not something the engine does"));
    assert_eq!(r.act(Action::Undo, std::slice::from_ref(&id)), done("Undone: 1 message back in INBOX"));
    let projects = Action::Move(FolderId("Projects".to_string()));
    assert_eq!(r.act(projects, std::slice::from_ref(&id)), failed("no folder \"Projects\" in work", "Tab completes"));
    r.engine.stop();
}

#[test]
fn an_account_with_no_archive_says_how_to_name_one() {
    let world = crate::tests::world("no-archive", QUIET);
    let server = FakeServer::new(PASSWORD, QUIET);
    server.add_folder("INBOX", &[], 7);
    server.deliver("INBOX", &message(1), Flags::default(), NOW);
    let r = Running::over(crate::tests::World { server, ..world }, the_account(), PASSWORD);
    r.wait_for(|e| matches!(e, Event::Synced { folder: FolderId(f), .. } if f == "INBOX").then_some(()));
    let hint = "name one in the config: folders.archive = \"…\"";
    assert_eq!(r.act(Action::Archive, &[r.world.id(1)]), failed("work has no Archive folder", hint));
    r.engine.stop();
}

#[test]
fn a_draft_is_saved_and_a_later_version_takes_its_place() {
    let r = two("drafts", QUIET);
    let first = r.engine.save_draft(b"Subject: first thoughts\r\n\r\none\r\n".to_vec(), None, Origin::Local);
    let second = r.engine.save_draft(b"Subject: second thoughts\r\n\r\ntwo\r\n".to_vec(), Some(first), Origin::Agent);
    let drafts = r.world.here("Drafts");
    assert_eq!(drafts.len(), 1);
    assert_eq!(
        (drafts[0].1.as_str(), drafts[0].2),
        ("second thoughts", Flags { draft: true, seen: true, ..Flags::default() })
    );
    let kept = r.world.store.run(move |s| Ok((s.stored(&second)?.1.origin, s.read(&second)?))).unwrap();
    assert_eq!(kept, (Origin::Agent, b"Subject: second thoughts\r\n\r\ntwo\r\n".to_vec()));
    assert_eq!(r.world.journal(), [], "a draft written here is nothing the server is told of");
    r.engine.stop();
}

#[test]
fn engines_send_each_message_to_its_own_account_under_the_one_serial() {
    let work = crate::tests::world("router", QUIET);
    work.deliver(1);
    let home = FakeServer::new(PASSWORD, QUIET);
    home.add_folder("INBOX", &[], 3);
    home.add_folder("Archive", &["\\Archive"], 4);
    home.deliver("INBOX", &message(9), Flags::default(), NOW);
    let (sender, events) = mpsc::channel();
    let engine = |account: Account, server: &FakeServer| {
        let door = Door { server: server.clone(), password: PASSWORD, post: PostOffice::default() };
        Engine::start(account, Box::new(door), work.store.clone(), sender.clone())
    };
    let at_home = Account { name: "home".to_string(), ..the_account() };
    let engines = Engines::new(vec![engine(the_account(), &work.server), engine(at_home, &home)]);
    let in_inbox = |account: &'static str| {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let synced = work.store.run(move |s| s.synced(account, "INBOX")).unwrap();
            let folders = work.store.run(|s| s.folders()).unwrap();
            let whole = folders.iter().any(|f| f.account.0 == account && f.name == "Archive");
            if let Some(message) = synced.first().filter(|_| whole) {
                return MessageId { account: AccountId(account.to_string()), key: message.key.clone() };
            }
            assert!(Instant::now() < deadline, "{account} never synced");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let (ours, theirs) = (in_inbox("work"), in_inbox("home"));
    let said = |count: usize| -> Vec<String> {
        let mut said = Vec::new();
        while said.len() < count {
            match events.recv_timeout(PATIENCE).expect("the engines went quiet") {
                Event::ActionDone { summary } => said.push(summary),
                Event::ActionFailed { serial, reason, .. } => said.push(format!("E {serial} {reason}")),
                _ => {}
            }
        }
        said
    };
    let stranger = MessageId { account: AccountId("gone".to_string()), key: "k".to_string() };
    let both = vec![ours.clone(), theirs.clone()];
    engines.act(Ask { serial: 1, action: Action::Archive, on: vec![ours, theirs.clone(), stranger] });
    let mut first = said(3);
    first.sort();
    assert_eq!(first, ["Archived 1 message", "Archived 1 message", "E 1 1 message of no account in the config"]);
    engines.act(Ask { serial: 2, action: Action::Undo, on: both.clone() });
    assert_eq!(said(2), ["Undone: 1 message back in INBOX", "Undone: 1 message back in INBOX"]);
    engines.act(Ask { serial: 3, action: Action::Flag(true), on: vec![theirs.clone()] });
    engines.act(Ask { serial: 4, action: Action::Undo, on: vec![theirs] });
    engines.act(Ask { serial: 5, action: Action::Undo, on: both });
    // Each account answers for its share, in no order between them.
    let mut last = said(4);
    last.sort();
    let nothing = "E 5 there is nothing to undo";
    assert_eq!(last, [nothing, nothing, "Flagged 1 message", "Undone: 1 message as before"]);
    assert_eq!(engines.of("home").map(Engine::account), Some("home"));
    engines.stop();
}

#[test]
fn a_change_an_earlier_run_journaled_is_refused_as_a_notice_and_named_by_no_ask() {
    let r = two("refused-later", QUIET);
    let id = r.world.id(2);
    r.offline();
    r.act(Action::Trash, std::slice::from_ref(&id)).unwrap();
    r.retry_in();
    r.engine.stop();
    r.world.server.refuse_connections(0);
    r.world.server.refuse_moves();
    let again = Running::over(r.world, the_account(), PASSWORD);
    let refusal = failed("work refused a change made earlier: the move was refused", "1 message back in INBOX");
    assert_eq!(again.told(), (None, refusal));
    assert_eq!(again.folder_of(&id), ("INBOX".to_string(), Some(2)));
    again.engine.stop();
}

/// ADR-041, ADR-014: the Trash is emptied by name, and a server that
/// cannot purge by name is never sent a bare `EXPUNGE`.
#[test]
fn emptying_the_trash_deletes_by_name_and_only_what_was_in_it() {
    for (name, uidplus) in [("empty-trash", true), ("empty-trash-no-uidplus", false)] {
        let r = two(name, Capabilities { uidplus, ..QUIET });
        let ids = [r.world.id(1), r.world.id(2)];
        r.act(Action::Trash, &ids[..1]).unwrap();
        r.replayed();
        r.offline();
        r.act(Action::Trash, &ids[1..]).unwrap();
        r.retry_in();
        let empty = Action::EmptyTrash(AccountId("work".to_string()));
        let said = "Deleted 1 message from work's Trash; 1 message not yet on the server stay";
        r.engine.act(Ask { serial: 41, action: empty, on: Vec::new() });
        let counted = r.wait_for(|e| match e {
            Event::Emptied { serial, deleted, kept, .. } => Some((*serial, *deleted, *kept)),
            _ => None,
        });
        assert_eq!(counted, (41, 1, 1), "the interface is told the count it cannot make itself");
        assert_eq!(r.said(), done(said));
        assert_eq!(r.world.here("Trash").len(), 1, "gone here at once, but for the one the server has not heard of");
        r.world.server.take_log();
        r.world.server.refuse_connections(0);
        if !uidplus {
            r.engine.sync();
            assert_eq!(r.said(), done("work cannot purge by name; it will purge what was deleted itself"));
        }
        r.replayed();
        r.online();
        let log = r.world.server.take_log();
        assert!(log.contains(&"store-deleted [1]".to_string()), "{log:?}");
        assert_eq!(log.contains(&"uid-expunge [1]".to_string()), uidplus, "{log:?}");
        assert!(!log.iter().any(|line| line.starts_with("expunge")), "never a bare one: {log:?}");
        assert_eq!((r.world.uids("Trash"), r.world.here("Trash").len()), (vec![2], 1), "and it does not come back");
        assert_eq!((r.world.uids("INBOX").len(), r.world.journal().len()), (0, 0));
        r.engine.stop();
    }
}

#[test]
fn a_later_version_of_a_draft_the_server_keeps_deletes_the_servers_and_only_a_draft_is_discarded() {
    let r = two("draft-rewrite", QUIET);
    let kept = MessageId { account: AccountId("work".to_string()), key: "kept".to_string() };
    let (id, later) = (kept.clone(), kept.clone());
    r.world
        .store
        .run(move |s| {
            s.ensure_folder("work", "Drafts", tuimex_core::Role::Drafts)?;
            let flags = Flags { draft: true, ..Flags::default() };
            let delivery = Delivery { key: id.key.clone(), uid: Some(4), origin: Origin::Server, flags };
            s.deliver("work", "Drafts", &delivery, b"Subject: as the server has it\r\n\r\n")?;
            Ok(())
        })
        .unwrap();
    let saved = r.engine.save_draft(b"Subject: as it is now\r\n\r\n".to_vec(), Some(later), Origin::Local);
    assert_eq!(saved, kept, "it keeps its name");
    let (stored, journal) = (r.world.here("Drafts"), r.world.journal());
    assert_eq!((stored.len(), stored[0].1.as_str()), (1, "as it is now"));
    assert_eq!((journal.len(), journal[0].uid, &journal[0].change), (1, Some(4), &Change::Delete));
    let inbox = r.world.id(1);
    assert_eq!(r.act(Action::Discard, &[inbox]), failed("only a draft can be discarded", ""));
    assert_eq!(r.act(Action::Discard, &[kept]), done("Discarded 1 message"));
    assert_eq!((r.world.here("Drafts").len(), r.world.here("INBOX").len()), (0, 2));
    r.engine.stop();
}

/// PLAN §4.1: an Archive is made on first use, after asking. Asking is the
/// interface's; this is what follows a yes.
#[test]
fn an_archive_the_user_agreed_to_is_made_on_the_server_by_the_first_move_there_and_u_still_reaches_the_message() {
    let r = start("make-archive", QUIET, PASSWORD, |w| {
        w.server.remove_folder("Archive");
        (1..=2).for_each(|n| _ = w.deliver(n));
    });
    r.settle();
    let id = r.world.id(2);
    let hint = "name one in the config: folders.archive = \"…\"";
    assert_eq!(r.act(Action::Archive, std::slice::from_ref(&id)), failed("work has no Archive folder", hint));
    assert_eq!(r.act(Action::CreateArchive(AccountId("work".into())), &[]), done("work has an Archive folder"));
    assert!(!r.world.server.take_log().iter().any(|line| line.starts_with("create")), "nothing is made unused");
    assert_eq!(r.act(Action::Archive, std::slice::from_ref(&id)), done("Archived 1 message"));
    r.replayed();
    assert_eq!((r.world.there("INBOX"), r.world.there("Archive")), (vec![1], vec![1]));
    let numbered = r.folder_of(&id);
    assert_eq!(numbered, ("Archive".to_string(), Some(1)), "under the number the new folder gave it");
    // The folder's own pass finds it under the UIDVALIDITY it was
    // numbered with, and leaves it and its key alone.
    r.online();
    assert_eq!(r.folder_of(&id), numbered);
    assert_eq!(r.act(Action::Undo, std::slice::from_ref(&id)), done("Undone: 1 message back in INBOX"));
    r.replayed();
    assert_eq!((r.world.there("INBOX").len(), r.world.there("Archive").len()), (2, 0));
    r.engine.stop();
}

/// ADR-047: what follows a yes to a Trash is what follows one to an
/// Archive, and `:empty-trash` then has a Trash to empty.
#[test]
fn a_trash_the_user_agreed_to_is_made_on_the_server_by_the_first_move_there_and_u_still_reaches_the_message() {
    let r = start("make-trash", QUIET, PASSWORD, |w| {
        w.server.remove_folder("Trash");
        (1..=2).for_each(|n| _ = w.deliver(n));
    });
    // The two passes every start makes, which end here with the Archive.
    for _ in 0..2 {
        r.wait_for(|e| matches!(e, Event::Synced { folder: FolderId(f), .. } if f == "Archive").then_some(()));
    }
    r.world.server.take_log();
    let id = r.world.id(2);
    let hint = "name one in the config: folders.trash = \"…\"";
    assert_eq!(r.act(Action::Trash, std::slice::from_ref(&id)), failed("work has no Trash folder", hint));
    assert_eq!(r.act(Action::CreateTrash(AccountId("work".into())), &[]), done("work has a Trash folder"));
    assert!(!r.world.server.take_log().iter().any(|line| line.starts_with("create")), "nothing is made unused");
    assert_eq!(r.act(Action::Trash, std::slice::from_ref(&id)), done("Trashed 1 message"));
    r.replayed();
    assert_eq!((r.world.there("INBOX"), r.world.there("Trash")), (vec![1], vec![1]));
    assert_eq!(r.folder_of(&id), ("Trash".to_string(), Some(1)), "under the number the new folder gave it");
    assert_eq!(r.act(Action::Undo, std::slice::from_ref(&id)), done("Undone: 1 message back in INBOX"));
    r.replayed();
    assert_eq!((r.world.there("INBOX").len(), r.world.there("Trash").len()), (2, 0));
    assert_eq!(r.act(Action::Trash, std::slice::from_ref(&id)), done("Trashed 1 message"));
    r.replayed();
    let empty = Action::EmptyTrash(AccountId("work".into()));
    assert_eq!(r.act(empty, &[]), done("Deleted 1 message from work's Trash"));
    r.replayed();
    assert_eq!(r.world.there("Trash"), Vec::<u32>::new());
    r.engine.stop();
}

/// ADR-048: a server that keeps a person's folders under its Inbox
/// refuses an Archive at the top, on every move. The folder is named for
/// the server's namespace, which the listing recorded with the mail.
#[test]
fn an_archive_and_a_trash_are_made_under_the_servers_namespace_where_it_keeps_folders_under_the_inbox() {
    let r = start("namespace", QUIET, PASSWORD, |w| {
        for top in ["Archive", "Trash", "Projects"] {
            w.server.remove_folder(top);
        }
        w.server.keep_folders_under("INBOX.");
        w.server.add_folder("INBOX.Drafts", &[], 12);
        (1..=2).for_each(|n| _ = w.deliver(n));
    });
    let passes = |folder: &'static str| {
        for _ in 0..2 {
            r.engine.sync();
            r.wait_for(|e| matches!(e, Event::Synced { folder: FolderId(f), .. } if f == folder).then_some(()));
        }
    };
    passes("INBOX.Drafts");
    let (first, second) = (r.world.id(1), r.world.id(2));
    let work = || AccountId("work".into());
    for (create, then, folder) in [
        (Action::CreateArchive(work()), Action::Archive, "INBOX.Archive"),
        (Action::CreateTrash(work()), Action::Trash, "INBOX.Trash"),
    ] {
        let id = if folder == "INBOX.Archive" { &first } else { &second };
        r.act(create, &[]).unwrap();
        r.act(then, std::slice::from_ref(id)).unwrap();
        while !r.world.journal().is_empty() {
            std::thread::sleep(Duration::from_millis(5));
        }
        passes(folder);
        assert_eq!(r.world.there(folder), [1], "made where the server makes folders, and the message is in it");
        assert_eq!(r.folder_of(id), (folder.to_string(), Some(1)));
    }
    let log = r.world.server.take_log();
    assert!(log.contains(&"create INBOX.Archive".to_string()) && log.contains(&"create INBOX.Trash".to_string()));
    assert!(!log.iter().any(|line| line == "create Archive" || line == "create Trash"), "{log:?}");
    assert_eq!(r.act(Action::Undo, std::slice::from_ref(&second)), done("Undone: 1 message back in INBOX"));
    // After a restart with no server to ask, the prefix is still known.
    r.engine.stop();
    let prefix = r.world.store.run(|s| s.namespace("work")).unwrap();
    assert_eq!(prefix, "INBOX.");
    let roles = r.world.store.run(|s| s.folders()).unwrap();
    let role = |name: &str| roles.iter().find(|f| f.name == name).map(|f| f.role);
    assert_eq!((role("INBOX.Archive"), role("INBOX.Trash")), (Some(Role::Archive), Some(Role::Trash)));
}

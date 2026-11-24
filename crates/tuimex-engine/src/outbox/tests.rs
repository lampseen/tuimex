//! Sending through a running engine (PLAN §4.4): held, sent once, its
//! copy filed, and what happens when any step of that fails.

use std::time::Duration;

use tuimex_core::{Action, Ask, Capabilities, Event, Flags, MessageId, Origin, Remote, RemoteError, Role};
use tuimex_store::journal::{Change, Journal};
use tuimex_store::maildir::Delivery;

use crate::Account;
use crate::tests::{PASSWORD, QUIET, Running, World, start, the_account, world};

fn draft(to: &str) -> Vec<u8> {
    let head = format!("From: Ada <ada@example.org>\r\nTo: {to}\r\nBcc: quiet@example.net\r\nSubject: a draft");
    format!("{head}\r\nMessage-ID: <draft-1@example.org>\r\n\r\nhello\r\n").into_bytes()
}

fn with_sent(w: &World) {
    w.server.add_folder("Sent", &["\\Sent"], 11);
}

fn running(name: &str, capabilities: Capabilities, prepare: impl FnOnce(&World)) -> Running {
    let r = start(name, capabilities, PASSWORD, prepare);
    r.settle();
    r
}

impl Running {
    fn sent(&self) -> Result<MessageId, String> {
        self.wait_for(|e| match e {
            Event::Sent { id, .. } => Some(Ok(id.clone())),
            Event::SendFailed { reason, .. } => Some(Err(reason.clone())),
            _ => None,
        })
    }

    /// A draft, filed by the time this returns.
    fn draft(&self, to: &str) -> MessageId {
        let id = self.engine.save_draft(draft(to), None, Origin::Local);
        self.world.store.run(|_| Ok(())).unwrap();
        id
    }

    fn count(&self, folder: &'static str) -> usize {
        self.world.here(folder).len()
    }
}

#[test]
fn a_message_is_sent_as_written_without_its_bcc_and_its_copy_is_in_sent_once() {
    let r = running("send", QUIET, with_sent);
    let id = r.draft("Grace <grace@example.net>");
    assert_eq!(r.act(Action::Send, std::slice::from_ref(&id)), Ok("Sending to 2 recipients in 0s".to_string()));
    assert_eq!(r.sent(), Ok(id));
    let handed = r.post.handed();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].from, "ada@example.org");
    assert_eq!(handed[0].to, ["grace@example.net", "quiet@example.net"]);
    let written = String::from_utf8(draft("Grace <grace@example.net>")).unwrap();
    assert_eq!(handed[0].message, written.replace("Bcc: quiet@example.net\r\n", "").into_bytes());
    r.replayed();
    assert_eq!(r.world.there("Sent"), [1]);
    let sent = r.world.here("Sent");
    assert_eq!((sent.len(), sent[0].0, sent[0].2), (1, 1, Flags { seen: true, ..Flags::default() }));
    assert_eq!((r.count("Outbox"), r.count("Drafts"), r.post.handed().len()), (0, 0, 1));
    r.engine.stop();
}

#[test]
fn u_within_the_delay_returns_it_to_drafts_and_nothing_is_sent() {
    let w = world("unsend", QUIET);
    with_sent(&w);
    let r = Running::over(w, Account { send_delay: Duration::from_secs(3600), ..the_account() }, PASSWORD);
    r.settle();
    let id = r.draft("grace@example.net");
    assert_eq!(r.act(Action::Send, std::slice::from_ref(&id)), Ok("Sending to 2 recipients in 3600s".to_string()));
    assert_eq!((r.count("Outbox"), r.count("Drafts")), (1, 0));
    assert_eq!(r.act(Action::Undo, std::slice::from_ref(&id)), Ok("Not sent: back in Drafts".to_string()));
    assert_eq!((r.count("Outbox"), r.count("Drafts")), (0, 1));
    let nothing = Err(("there is nothing to undo".to_string(), String::new()));
    assert_eq!(r.act(Action::Undo, std::slice::from_ref(&id)), nothing);
    assert_eq!((r.post.handed().len(), r.world.journal().len()), (0, 0));
    r.engine.stop();
}

/// The interface reads the store straight after handing `u` over, and
/// shows what it finds: the Outbox, with the seconds stopped at nothing,
/// if the draft were put back by another thread in its own time.
#[test]
fn a_message_stopped_with_u_is_in_drafts_for_whoever_reads_the_store_next_without_waiting_to_be_told() {
    let w = world("unsend-at-once", QUIET);
    let r = Running::over(w, Account { send_delay: Duration::from_secs(3600), ..the_account() }, PASSWORD);
    r.settle();
    let id = r.draft("grace@example.net");
    r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    for _ in 0..50 {
        r.engine.act(Ask { serial: 1000, action: Action::Undo, on: vec![id.clone()] });
        let asked = id.clone();
        assert_eq!(r.world.store.run(move |s| Ok(s.stored(&asked)?.0)).unwrap(), "Drafts");
        assert_eq!(r.said(), Ok("Not sent: back in Drafts".to_string()));
        r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    }
    r.engine.stop();
}

#[test]
fn an_address_that_does_not_parse_or_is_not_the_accounts_stops_the_send_before_anything_is_queued() {
    let r = running("unsendable", QUIET, with_sent);
    let bad = r.draft("grace@example.net, nobody at all");
    let fix = |reason: &str| Err((reason.to_string(), "edit the draft".to_string()));
    assert_eq!(r.act(Action::Send, std::slice::from_ref(&bad)), fix("\"nobody at all\" is not an address"));
    let raw = String::from_utf8(draft("grace@example.net")).unwrap().replace("ada@example.org", "eve@example.com");
    let not_ours = r.engine.save_draft(raw.into_bytes(), None, Origin::Local);
    let good = r.draft("grace@example.net");
    let refused = r.act(Action::Send, &[good, not_ours]);
    assert_eq!(refused, fix("\"eve@example.com\" is not an address this account sends from"));
    assert_eq!((r.count("Outbox"), r.count("Drafts")), (0, 3), "one bad draft holds back the whole batch");
    let inbox = r.world.server.deliver("INBOX", &draft("grace@example.net"), Flags::default(), tuimex_fixtures::NOW);
    r.engine.sync();
    r.wait_for(|e| matches!(e, Event::NewMail { .. }).then_some(()));
    let not_a_draft = r.act(Action::Send, &[r.world.id(inbox)]);
    assert_eq!(not_a_draft, Err(("only a draft can be sent".to_string(), String::new())));
    assert_eq!(r.post.handed().len(), 0);
    r.engine.stop();
}

#[test]
fn a_refusal_leaves_it_in_the_outbox_with_the_reason_until_it_is_sent_again() {
    let r = running("send-refused", QUIET, with_sent);
    let id = r.draft("grace@example.net");
    let why = "refused by smtp.example.org: 552 message too large";
    r.post.fail_next(RemoteError::Refused(why.to_string()));
    r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    assert_eq!(r.sent(), Err(why.to_string()));
    assert!(r.events.recv_timeout(Duration::from_millis(300)).is_err(), "a refusal is not tried again by itself");
    assert_eq!((r.count("Outbox"), r.post.handed().len(), r.world.journal().len()), (1, 0, 0));
    assert_eq!(r.act(Action::Send, std::slice::from_ref(&id)), Ok("Sending to 2 recipients in 0s".to_string()));
    assert_eq!(r.sent(), Ok(id));
    assert_eq!(r.post.handed().len(), 1);
    r.engine.stop();
}

#[test]
fn a_lost_connection_is_tried_again_and_a_refused_sign_in_says_what_to_do() {
    let r = running("send-lost", QUIET, with_sent);
    let id = r.draft("grace@example.net");
    r.post.fail_next(RemoteError::Connection("nobody is listening".to_string()));
    r.post.fail_next(RemoteError::Auth);
    r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    assert_eq!(r.sent(), Err("work: connection lost: nobody is listening".to_string()));
    assert_eq!(r.sent(), Err("signing in to work failed; run `tuimex setup`".to_string()));
    assert_eq!((r.count("Outbox"), r.post.handed().len()), (1, 0));
    r.engine.stop();
}

#[test]
fn a_crash_between_smtp_acceptance_and_filing_does_not_send_twice() {
    let r = running("crash", QUIET, with_sent);
    let id = r.draft("grace@example.net");
    // The mail server is out of reach, so the copy cannot be filed.
    r.world.server.refuse_connections(usize::MAX);
    r.world.server.drop_after(0);
    r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    assert_eq!(r.sent(), Ok(id));
    r.retry_in();
    assert_eq!((r.post.handed().len(), r.world.journal().len(), r.count("Outbox")), (1, 1, 0));
    let Running { world, engine, .. } = r;
    engine.stop();

    world.server.refuse_connections(0);
    let again = Running::over(world, the_account(), PASSWORD);
    again.replayed();
    assert_eq!(again.world.there("Sent"), [1], "the copy is filed by the run that came after");
    assert_eq!((again.post.handed().len(), again.count("Outbox"), again.count("Sent")), (0, 0, 1));
    again.engine.stop();
}

#[test]
fn a_message_the_journal_says_was_accepted_is_filed_and_not_sent_even_from_the_outbox() {
    // What a run leaves that ended after writing the acceptance down and
    // before moving the file.
    let w = world("crash-in-outbox", QUIET);
    with_sent(&w);
    let delivery = Delivery { key: "accepted".to_string(), uid: None, origin: Origin::Local, flags: Flags::default() };
    w.store
        .run(move |s| {
            s.ensure_folder("work", "Outbox", Role::Outbox)?;
            s.deliver("work", "Outbox", &delivery, &draft("grace@example.net")).map(drop)
        })
        .unwrap();
    Journal::new(&w.root.join("data/work"))
        .record("accepted", "Outbox", None, Change::Move { to: "Sent".to_string() })
        .unwrap();
    let r = Running::over(w, the_account(), PASSWORD);
    r.replayed();
    assert_eq!(r.world.there("Sent"), [1]);
    assert_eq!((r.post.handed().len(), r.count("Outbox"), r.count("Sent")), (0, 0, 1));
    r.engine.stop();
}

#[test]
fn a_copy_already_on_the_server_is_not_appended_again() {
    let r = running("append-once", QUIET, with_sent);
    let id = r.draft("grace@example.net");
    // As after a pass that ended between the append and its record.
    r.world.server.deliver("Sent", &draft("grace@example.net"), Flags::default(), tuimex_fixtures::NOW);
    r.world.server.take_log();
    r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    r.sent().unwrap();
    r.replayed();
    assert_eq!(r.world.there("Sent"), [1]);
    r.engine.stop();
}

#[test]
fn gmail_files_what_it_sends_and_is_not_given_a_second_copy() {
    let r = running("gmail", Capabilities { gmail: true, ..QUIET }, with_sent);
    let id = r.draft("grace@example.net");
    r.world.server.take_log();
    r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    r.sent().unwrap();
    r.replayed();
    assert!(!r.world.server.take_log().iter().any(|line| line.starts_with("append")));
    assert_eq!((r.count("Outbox"), r.count("Sent"), r.post.handed().len()), (0, 0, 1));
    r.engine.stop();
}

#[test]
fn a_copy_the_server_will_not_keep_stays_here_and_the_message_is_never_sent_again() {
    let r = running("no-sent", QUIET, |_| ());
    let id = r.draft("grace@example.net");
    r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    let reason = "work would not keep a copy of what was sent: no folder Sent".to_string();
    assert_eq!(r.said(), Err((reason, "it is in Sent on this machine alone".to_string())));
    assert_eq!((r.count("Sent"), r.count("Outbox"), r.world.journal().len()), (1, 0, 0));
    r.replayed();
    assert_eq!((r.count("Sent"), r.post.handed().len()), (1, 1));
    r.engine.stop();
}

#[test]
fn a_draft_the_server_keeps_is_sent_as_a_copy_and_the_servers_goes_to_its_trash() {
    let r = running("server-draft", QUIET, |w| {
        with_sent(w);
        w.server.add_folder("Drafts", &["\\Drafts"], 12);
        w.server.deliver(
            "Drafts",
            &draft("grace@example.net"),
            Flags { draft: true, ..Flags::default() },
            tuimex_fixtures::NOW,
        );
    });
    let kept = r.world.store.run(|s| s.synced("work", "Drafts")).unwrap().remove(0).key;
    let kept = MessageId { account: tuimex_core::AccountId("work".to_string()), key: kept };
    r.act(Action::Send, std::slice::from_ref(&kept)).unwrap();
    let sent = r.sent().unwrap();
    assert_ne!(sent, kept);
    r.replayed();
    assert_eq!((r.world.there("Drafts"), r.world.there("Trash"), r.world.there("Sent")), (vec![], vec![1], vec![1]));
    assert_eq!((r.count("Drafts"), r.count("Outbox"), r.count("Sent"), r.post.handed().len()), (0, 0, 1, 1));
    r.engine.stop();
}

#[test]
fn a_reply_that_is_sent_marks_what_it_answers_answered_here_and_on_the_server_and_a_new_message_marks_nothing() {
    let r = running("answered", QUIET, |w| {
        with_sent(w);
        w.deliver(1);
        w.deliver(2);
    });
    let original = r.world.id(1);
    let reply = String::from_utf8(draft("grace@example.net")).unwrap();
    let reply = reply.replace("Subject: a draft", "Subject: Re: message 1\r\nIn-Reply-To: <1@example.net>");
    let id = r.engine.save_draft(reply.into_bytes(), None, Origin::Local);
    r.act(Action::Send, std::slice::from_ref(&id)).unwrap();
    let answered = r.wait_for(|e| match e {
        Event::Sent { answered, .. } => Some(answered.clone()),
        _ => None,
    });
    assert_eq!(answered, [original], "the event names what the row should now say");
    let flags = |uid: u32| r.world.here("INBOX").into_iter().find(|m| m.0 == uid).unwrap().2;
    assert!(flags(1).answered && !flags(2).answered, "here at once, and only what was answered");
    r.replayed();
    let mut other = r.world.server.connect(PASSWORD).unwrap();
    other.select("INBOX").unwrap();
    let there: Vec<(u32, bool)> = other.flags(None).unwrap().into_iter().map(|f| (f.uid, f.flags.answered)).collect();
    assert_eq!(there, [(1, true), (2, false)], "the server heard through the journal");
    assert!(r.world.journal().is_empty());

    let new = r.draft("grace@example.net");
    r.act(Action::Send, std::slice::from_ref(&new)).unwrap();
    let answered = r.wait_for(|e| match e {
        Event::Sent { answered, .. } => Some(answered.clone()),
        _ => None,
    });
    assert_eq!(answered, [], "a message that answers none marks none");
    assert!(!flags(2).answered);
    r.engine.stop();
}

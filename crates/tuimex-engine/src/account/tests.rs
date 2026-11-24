use std::thread;
use std::time::{Duration, Instant};

use tuimex_core::{Event, Flags, FolderId, RemoteError, Timestamp};
use tuimex_fixtures::NOW;

use crate::tests::{ALL, PASSWORD, PATIENCE, QUIET, Running, message, start};
use crate::{Error, SecretError};

#[test]
fn new_mail_arrives_through_idle() {
    let r = start("idle", ALL, PASSWORD, |w| _ = w.deliver(1));
    r.settle();
    r.world.deliver(2);
    let count = r.wait_for(|e| if let Event::NewMail { count, .. } = e { Some(*count) } else { None });
    assert_eq!(count, 1);
    assert_eq!(r.world.uids("INBOX"), [1, 2]);
    assert!(r.world.server.take_log().contains(&"idle".to_string()), "nothing but the idle asked for that pass");
    r.engine.stop();
}

#[test]
fn stop_ends_an_idle_that_would_otherwise_wait_half_an_hour() {
    let r = start("stop", ALL, PASSWORD, |_| ());
    while !r.world.server.take_log().contains(&"idle".to_string()) {
        thread::yield_now();
    }
    let begun = Instant::now();
    r.engine.stop();
    assert!(begun.elapsed() < PATIENCE);
}

#[test]
fn a_lost_connection_is_retried_after_one_two_then_four_seconds_and_a_sync_cuts_the_wait_short() {
    let r = start("backoff", QUIET, PASSWORD, |w| {
        w.deliver(1);
        w.server.refuse_connections(usize::MAX);
    });
    for seconds in [1, 2, 4] {
        let (msg, retry_in) = r.retry_in();
        assert_eq!(retry_in, Some(Duration::from_secs(seconds)));
        assert_eq!(msg, "work: connection lost: nobody is listening");
        r.engine.sync();
    }
    r.retry_in();
    r.world.server.refuse_connections(0);
    r.engine.sync();
    r.wait_for(|e| matches!(e, Event::Synced { folder: FolderId(f), .. } if f == "INBOX").then_some(()));
    assert_eq!(r.world.uids("INBOX"), [1]);
    r.engine.stop();
}

#[test]
fn a_success_starts_the_backoff_over() {
    let r = start("backoff-reset", QUIET, PASSWORD, |w| w.server.refuse_connections(usize::MAX));
    assert_eq!(r.retry_in().1, Some(Duration::from_secs(1)));
    r.engine.sync();
    assert_eq!(r.retry_in().1, Some(Duration::from_secs(2)));
    r.world.server.refuse_connections(0);
    r.engine.sync();
    r.wait_for(|e| matches!(e, Event::Synced { folder: FolderId(f), .. } if f == "Trash").then_some(()));
    // The connection that just worked is lost before its next command.
    r.world.server.drop_after(0);
    r.engine.sync();
    assert_eq!(
        r.retry_in(),
        ("work: connection lost: the connection was closed".to_string(), Some(Duration::from_secs(1)))
    );
    r.engine.stop();
}

#[test]
fn a_refused_sign_in_says_what_to_do_and_is_not_tried_again() {
    let r = start("auth", ALL, "not-the-password", |_| ());
    let (msg, retry_in) = r.retry_in();
    assert_eq!(msg, "signing in to work failed; run `tuimex setup`");
    assert_eq!(retry_in, None);
    assert!(!Error::Remote(RemoteError::Auth).retries());
    assert!(!Error::Secret(SecretError::Missing { account: "work".to_string() }).retries());
    // Neither the syncer nor the watcher knocks again by itself.
    assert!(r.events.recv_timeout(Duration::from_millis(300)).is_err());
    assert_eq!(r.world.server.take_log(), Vec::<String>::new());
    r.engine.stop();
}

/// A world whose Inbox holds one message too old to have brought its body.
fn with_a_header_block(name: &str) -> Running {
    let r = start(name, QUIET, PASSWORD, |w| {
        w.server.deliver("INBOX", &message(1), Flags::default(), Timestamp(NOW.0 - 366 * 86_400));
    });
    r.settle();
    assert!(r.world.here("INBOX")[0].3, "only the headers are here");
    r
}

fn body_event(e: &Event) -> Option<Result<String, String>> {
    match e {
        Event::BodyReady { body, .. } => Some(Ok(body.text.clone())),
        Event::BodyFailed { reason, .. } => Some(Err(reason.clone())),
        _ => None,
    }
}

#[test]
fn a_body_is_fetched_when_it_is_asked_for_and_kept() {
    let r = with_a_header_block("body");
    let id = r.world.id(1);
    r.engine.body(id.clone(), 80);
    assert_eq!(r.wait_for(body_event).unwrap().trim(), "the body of message 1");
    assert_eq!(r.world.server.take_log(), ["select INBOX", "body 1"]);
    assert!(!r.world.here("INBOX")[0].3, "the whole message is here now");
    r.engine.body(id, 80);
    assert_eq!(r.wait_for(body_event).unwrap().trim(), "the body of message 1");
    assert_eq!(r.world.server.take_log(), Vec::<String>::new(), "and is read from the Maildir the second time");
    r.engine.stop();
}

#[test]
fn a_body_is_not_fetched_by_a_uid_from_before_the_folder_was_renumbered() {
    let r = with_a_header_block("body-stale");
    r.world.server.renumber("INBOX", 70);
    r.engine.body(r.world.id(1), 80);
    let reason = r.wait_for(body_event).unwrap_err();
    assert_eq!(reason, "the folder changed on the server; sync, then open the message again");
    assert_eq!(r.world.server.take_log(), ["select INBOX"]);
    r.engine.stop();
}

#[test]
fn a_body_that_cannot_be_fetched_fails_with_the_reason_and_the_next_one_reconnects() {
    let r = with_a_header_block("body-lost");
    r.world.server.drop_after(1);
    r.engine.body(r.world.id(1), 80);
    assert_eq!(r.wait_for(body_event).unwrap_err(), "connection lost: the connection was closed");
    r.engine.body(r.world.id(1), 80);
    assert_eq!(r.wait_for(body_event).unwrap().trim(), "the body of message 1");
    r.engine.stop();
}

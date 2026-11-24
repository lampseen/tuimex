//! A Gmail account inside a running engine (ADR-019): its threads live
//! for hours and an access token for one, so every connection they make
//! asks for the token afresh. The mail server and Google are both
//! stand-ins; `tests/sign_in.rs` is the same chain over a real server.

// A test that cannot set itself up should stop with the reason.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use tuimex_core::{Capabilities, Event, Flags, RemoteError, RemoteWrite};
use tuimex_engine::oauth::{self, Client, Endpoints, SignIn, Tokens};
use tuimex_engine::sync::Policy;
use tuimex_engine::{Account, Connect, Engine, Error, Keyring, Letter, Secret, StoreHandle};
use tuimex_fixtures::{FakeGoogle, FakeServer, NOW};
use tuimex_store::Store;

const PATIENCE: Duration = Duration::from_secs(30);

/// A keyring that exists only in this test: the user's is never opened.
#[derive(Default)]
struct MemoryKeyring(Mutex<HashMap<String, String>>);

impl Keyring for MemoryKeyring {
    fn password(&self, entry: &str) -> Result<Option<Secret>, String> {
        Ok(self.0.lock().unwrap().get(entry).map(|kept| Secret::new(kept.clone())))
    }

    fn set(&self, entry: &str, secret: &Secret) -> Result<(), String> {
        self.0.lock().unwrap().insert(entry.to_string(), secret.expose().to_string());
        Ok(())
    }
}

/// Gmail as the engine reaches it: a token from the keyring's sign-in,
/// which the mail server takes if Google would vouch for it.
struct Gmail {
    server: FakeServer,
    google: FakeGoogle,
    tokens: Tokens,
    keyring: MemoryKeyring,
    /// Every token a connection was made with.
    used: std::sync::Arc<Mutex<Vec<String>>>,
}

impl Connect for Gmail {
    fn connect(&self) -> Result<Box<dyn RemoteWrite>, Error> {
        let token = self.tokens.access("gmail", &self.keyring)?;
        self.used.lock().unwrap().push(token.expose().to_string());
        if !self.google.accepts(token.expose()) {
            return Err(RemoteError::Auth.into());
        }
        Ok(Box::new(self.server.connect("the-mail-server-s-own")?))
    }

    fn send(&self, _letter: &Letter) -> Result<(), Error> {
        self.connect().map(drop)
    }
}

fn until(events: &Receiver<Event>, wanted: impl Fn(&Event) -> bool) -> Event {
    loop {
        let event = events.recv_timeout(PATIENCE).expect("the engine went quiet before the event came");
        if wanted(&event) {
            return event;
        }
    }
}

#[test]
fn a_token_that_ends_mid_session_is_replaced_unasked_and_a_revoked_sign_in_stops_naming_setup() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-oauth-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let google = FakeGoogle::start("synthetic-client.apps.example.com", "GOCSPX-synthetic").unwrap();
    let endpoints = Endpoints { authorize: google.authorize_url(), token: google.token_url() };
    let client =
        Client { id: "synthetic-client.apps.example.com".into(), secret: Secret::new("GOCSPX-synthetic".into()) };
    let begun = SignIn::begin(&endpoints, &client.id, "ada@example.com").unwrap();
    let code = begun.pasted(&google.authorize(begun.url()).unwrap()).unwrap();
    let (refresh, _) = begun.finish(&endpoints, &client, &code).unwrap();
    let keyring = MemoryKeyring::default();
    keyring.set(oauth::CLIENT_ID, &Secret::new(client.id.clone())).unwrap();
    keyring.set(oauth::CLIENT_SECRET, &client.secret).unwrap();
    keyring.set(&oauth::refresh_entry("gmail"), &refresh).unwrap();

    let capabilities =
        Capabilities { uidplus: true, r#move: true, condstore: true, special_use: true, idle: true, gmail: true };
    let server = FakeServer::new("the-mail-server-s-own", capabilities);
    server.add_folder("INBOX", &[], 7);
    server.deliver("INBOX", b"Subject: one\r\n\r\nfirst\r\n", Flags::default(), NOW);
    // A token that is about to end is never handed out twice, so each
    // connection the engine makes from here on asks Google for its own:
    // an hour passing, in the time a test has.
    google.lifetime(30);
    let used = std::sync::Arc::new(Mutex::new(Vec::new()));
    let gmail = Gmail {
        server: server.clone(),
        google: google.clone(),
        tokens: Tokens::new(endpoints),
        keyring,
        used: used.clone(),
    };

    let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
    let (sender, events) = mpsc::channel();
    let account = Account {
        name: "gmail".to_string(),
        roles: Vec::new(),
        policy: Policy::default(),
        identities: vec!["ada@example.com".to_string()],
        send_delay: Duration::ZERO,
    };
    let engine = Engine::start(account, Box::new(gmail), StoreHandle::start(store), sender);
    let synced = |event: &Event| matches!(event, Event::Synced { folder, changed, .. } if folder.0 == "INBOX" && !changed.is_empty());
    until(&events, synced);

    // The server ends the session, as Gmail does when the token it was
    // opened with runs out. The engine comes back by itself, with a new
    // token, and brings what arrived meanwhile.
    server.deliver("INBOX", b"Subject: two\r\n\r\nsecond\r\n", Flags::default(), NOW);
    server.drop_after(0);
    engine.sync();
    until(&events, synced);
    let tokens = used.lock().unwrap().clone();
    let mut distinct = tokens.clone();
    distinct.sort();
    distinct.dedup();
    assert!(
        distinct.len() >= 2 && distinct.len() == tokens.len(),
        "each connection had a token of its own: {tokens:?}"
    );
    assert!(tokens.iter().all(|token| google.accepts(token)));

    // Access removed in the Google account: the next connection cannot
    // be made, and that is said once, with what to run, and not retried.
    google.revoke();
    server.drop_after(0);
    engine.sync();
    let stopped = until(&events, |event| matches!(event, Event::SyncError { retry_in: None, .. }));
    let Event::SyncError { account, msg, .. } = stopped else { unreachable!() };
    assert_eq!(account.0, "gmail");
    assert_eq!(msg, "Google no longer accepts the sign-in kept for gmail; run `tuimex setup` to sign in again");
    engine.stop();
    for secret in google.tokens().iter().chain(&tokens).chain([&refresh.expose().to_string()]) {
        assert!(!msg.contains(secret.as_str()));
    }
}

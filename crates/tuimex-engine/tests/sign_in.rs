//! What `tuimex setup` asks of a real server before it saves anything,
//! and Gmail's way of signing in, as far as a server that is not Gmail
//! can show it. The server is the one the environment names
//! (`tests/spike.rs` lists the variables); `TUIMEX_SERVER_SMTPS_PORT` is
//! its SMTP port that is TLS from the first byte, where it takes mail,
//! and `TUIMEX_SERVER_SMTP_PORT` its SMTP port with no TLS at all.
//!
//! GreenMail offers XOAUTH2 on both protocols and takes exactly one token:
//! the account's own password. So this proves the exchange as tuimex
//! speaks it (the token with the command over IMAP, `AUTH XOAUTH2` over
//! SMTP, a wrong token refused by the server as a refused sign-in) and
//! the chain from a refresh token in a keyring to a signed-in connection.
//! It proves nothing about Google: not that Google issues a token for
//! these requests, nor that Gmail takes it.

// A test that cannot reach its server should stop with the reason.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tuimex_core::RemoteError;
use tuimex_engine::oauth::{self, Client, Endpoints, SignIn, Tokens};
use tuimex_engine::sync::{self, Policy};
use tuimex_engine::{
    Account, Connect, Credential, Error, ImapAccount, Keyring, Letter, Protocol, Secret, Server, StoreHandle,
    certificate, pin, sign_in, smtp_check,
};
use tuimex_fixtures::FakeGoogle;
use tuimex_store::Store;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-sign-in-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(dir.join("config")).unwrap();
    dir
}

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

/// The server's two ends and its certificate as it crosses the wire, or
/// None, with a line saying so, for a server that takes no mail to send.
fn servers(test: &str) -> Option<(common::Server, Server, Server, Vec<u8>)> {
    let named = common::server(&format!("real-server {test}"))?;
    let Ok(smtps) = env::var("TUIMEX_SERVER_SMTPS_PORT") else {
        println!("real-server {test}: NOT RUN, this server has no SMTP port named in TUIMEX_SERVER_SMTPS_PORT");
        return None;
    };
    let imap = Server { host: named.host.clone(), port: named.port, starttls: false, user: named.user.clone() };
    let smtp = Server { port: smtps.parse().expect("TUIMEX_SERVER_SMTPS_PORT is a port"), ..imap.clone() };
    let root = scratch(&format!("{test}-ca"));
    fs::copy(&named.ca, root.join("config/work.pem")).unwrap();
    let der = pin::load(&root.join("config"), "work").unwrap().unwrap();
    Some((named, imap, smtp, der))
}

#[test]
fn a_server_s_certificate_is_read_from_both_ports_without_signing_in() {
    let Some((_, imap, smtp, der)) = servers("certificate-shown") else { return };
    let reading = certificate(&imap, Protocol::Imap).unwrap();
    let sending = certificate(&smtp, Protocol::Smtp).unwrap();
    assert_eq!(reading.certificate, der, "what is shown is what the server presents");
    assert_eq!(sending.certificate, der, "one certificate serves both, so one pin does");
    assert!(!reading.trusted && !sending.trusted, "a certificate the server signed for itself is not vouched for");
    // A port that offers no TLS at all gives no certificate either way of
    // asking, and so nothing is ever sent to it (ADR-012).
    if let Ok(plain) = env::var("TUIMEX_SERVER_SMTP_PORT") {
        let plain = Server { port: plain.parse().expect("TUIMEX_SERVER_SMTP_PORT is a port"), ..smtp.clone() };
        for starttls in [false, true] {
            let refused = certificate(&Server { starttls, ..plain.clone() }, Protocol::Smtp);
            assert!(matches!(refused, Err(RemoteError::Connection(_))), "{refused:?}");
        }
    }
    println!("real-server: ran certificate-shown ({})", pin::fingerprint(&der));
}

#[test]
fn a_bearer_token_signs_in_over_imap_and_smtp_and_a_wrong_one_is_a_refused_sign_in() {
    let Some((named, imap, smtp, der)) = servers("xoauth2") else { return };
    let mut probe = named.client();
    if !probe.capabilities().unwrap().has_str("AUTH=XOAUTH2") {
        println!("real-server xoauth2: NOT RUN, this server does not offer AUTH=XOAUTH2");
        return;
    }
    let bearer = |token: &str| Credential::Bearer(Secret::new(token.to_string()));
    let pinned = || Some(der.clone());

    let mut signed_in = sign_in(&imap, "work", &bearer(&named.password), pinned()).unwrap();
    assert!(signed_in.folders().unwrap().iter().any(|folder| folder.name == "INBOX"));
    smtp_check(&smtp, "work", &bearer(&named.password), pinned()).unwrap();
    let refused = sign_in(&imap, "work", &bearer("ya29.synthetic-not-a-token"), pinned()).err();
    assert!(matches!(refused, Some(RemoteError::Auth)), "{refused:?}");
    let refused = smtp_check(&smtp, "work", &bearer("ya29.synthetic-not-a-token"), pinned()).err();
    assert!(matches!(refused, Some(Error::Remote(RemoteError::Auth))), "{refused:?}");
    // The password itself, both ways: what setup tests before it saves.
    smtp_check(&smtp, "work", &Credential::Password(Secret::new(named.password.clone())), pinned()).unwrap();
    let wrong = Credential::Password(Secret::new("not-the-password".to_string()));
    assert!(matches!(smtp_check(&smtp, "work", &wrong, pinned()), Err(Error::Remote(RemoteError::Auth))));
    // Unpinned, the server's own certificate ends the attempt first.
    assert!(matches!(smtp_check(&smtp, "work", &wrong, None), Err(Error::Remote(RemoteError::Connection(_)))));

    // From a sign-in at (a stand-in for) Google to mail on disk: the
    // refresh token in a keyring, an access token asked for when the
    // connection is made, XOAUTH2 to the server.
    let google = FakeGoogle::start("synthetic-client.apps.example.com", "GOCSPX-synthetic").unwrap();
    let endpoints = Endpoints { authorize: google.authorize_url(), token: google.token_url() };
    let client =
        Client { id: "synthetic-client.apps.example.com".into(), secret: Secret::new("GOCSPX-synthetic".into()) };
    let begun = SignIn::begin(&endpoints, &client.id, "spike@example.com").unwrap();
    let code = begun.pasted(&google.authorize(begun.url()).unwrap()).unwrap();
    let (refresh, _) = begun.finish(&endpoints, &client, &code).unwrap();
    let keyring = MemoryKeyring::default();
    keyring.set(oauth::CLIENT_ID, &Secret::new(client.id.clone())).unwrap();
    keyring.set(oauth::CLIENT_SECRET, &client.secret).unwrap();
    keyring.set(&oauth::refresh_entry("work"), &refresh).unwrap();

    let root = scratch("xoauth2");
    pin::save(&root.join("config"), "work", &der).unwrap();
    let store = Store::open(&root.join("data"), &root.join("cache"), tuimex_engine::parse).unwrap();
    let store = StoreHandle::start(store);
    let account = Account {
        name: "work".to_string(),
        roles: Vec::new(),
        policy: Policy::default(),
        identities: vec!["spike@example.com".to_string()],
        send_delay: Duration::ZERO,
    };
    let reach = ImapAccount {
        name: "work".to_string(),
        server: imap.clone(),
        smtp: Some(smtp.clone()),
        password_cmd: None,
        gmail: Some(Tokens::new(endpoints.clone())),
        keyring: Box::new(keyring),
        config_dir: root.join("config"),
    };
    // The token GreenMail takes is the account's password, so the stand-in
    // issues that one; one of its own making is refused like any other.
    google.access_tokens_are(Some(&named.password));
    sync::once(&reach, &store, &account, &|_| {}).unwrap();
    assert!(root.join("data/work/cur").exists(), "the Inbox was synced over XOAUTH2");
    // And out again: the engine's own way of sending, with the same token.
    let subject = format!("xoauth2-{}", std::process::id());
    let letter = Letter {
        from: "spike@example.com".to_string(),
        to: vec!["spike@example.com".to_string()],
        message: common::message(&subject),
    };
    reach.send(&letter).unwrap();
    let mut inbox = named.session();
    inbox.select("INBOX").unwrap();
    assert_eq!(inbox.uid_search(format!("HEADER Subject {subject}")).unwrap().len(), 1, "the message arrived");
    google.access_tokens_are(None);
    let reach = ImapAccount { gmail: Some(Tokens::new(endpoints.clone())), ..reach };
    let refused = sync::once(&reach, &store, &account, &|_| {}).unwrap_err();
    assert_eq!(refused.advice("work"), "signing in to work failed; run `tuimex setup`");
    assert!(matches!(reach.send(&letter), Err(Error::Remote(RemoteError::Auth))));
    google.revoke();
    let reach = ImapAccount { gmail: Some(Tokens::new(endpoints)), ..reach };
    let revoked = sync::once(&reach, &store, &account, &|_| {}).unwrap_err();
    assert_eq!(
        revoked.advice("work"),
        "Google no longer accepts the sign-in kept for work; run `tuimex setup` to sign in again"
    );
    println!(
        "real-server: ran xoauth2 (the token this server takes is the account's password; a wrong one is refused)"
    );
}

/// GreenMail starts TLS with the first byte on every port it has, so the
/// other way in, STARTTLS on a plain port, is proven on a server that
/// offers it: the certificate setup shows is read through it, a pinned
/// account signs in and syncs through it, and the same port asked for TLS
/// at once answers with no handshake, which is how setup tells the two
/// apart.
#[test]
fn a_port_that_begins_with_starttls_shows_its_certificate_and_is_signed_in_to() {
    let Some(named) = common::server("real-server starttls") else { return };
    let Ok(port) = env::var("TUIMEX_SERVER_STARTTLS_PORT") else {
        println!(
            "real-server starttls: NOT RUN, this server has no STARTTLS port named in TUIMEX_SERVER_STARTTLS_PORT"
        );
        return;
    };
    let port = port.parse().expect("TUIMEX_SERVER_STARTTLS_PORT is a port");
    let upgraded = Server { host: named.host.clone(), port, starttls: true, user: named.user.clone() };
    let root = scratch("starttls");
    fs::copy(&named.ca, root.join("config/work.pem")).unwrap();
    let der = pin::load(&root.join("config"), "work").unwrap().unwrap();
    let seen = certificate(&upgraded, Protocol::Imap).unwrap();
    assert_eq!(seen.certificate, der, "the certificate of the TLS port, reached by STARTTLS");
    let at_once = certificate(&Server { starttls: false, ..upgraded.clone() }, Protocol::Imap);
    assert!(matches!(at_once, Err(RemoteError::Connection(_))), "{at_once:?}");

    let reach = tuimex_engine::ImapAccount { server: upgraded, smtp: None, ..named.account(&root.join("config")) };
    let mut remote = reach.connect().unwrap();
    let inbox = remote.folders().unwrap().into_iter().any(|folder| folder.name.eq_ignore_ascii_case("INBOX"));
    assert!(inbox, "signed in, and the folders are listed");
    let selected = remote.select("INBOX").unwrap();
    println!(
        "real-server: ran starttls ({}; signed in and selected the Inbox under UIDVALIDITY {})",
        pin::fingerprint(&der),
        selected.uid_validity
    );
}

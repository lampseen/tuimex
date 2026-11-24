//! The ADR-023 spike: the pinned `imap` crate asked, against a real
//! server, for each of the six things ADR-014 and ADR-019 need of it,
//! before anything in `src/` is built on it.
//!
//! It runs only when a server is named:
//!
//! ```text
//! TUIMEX_SERVER_HOST      the name the certificate was made for, e.g. localhost
//! TUIMEX_SERVER_PORT      the implicit-TLS IMAP port
//! TUIMEX_SERVER_USER      a throwaway account, whose INBOX this writes to
//! TUIMEX_SERVER_PASSWORD
//! TUIMEX_SERVER_CA        a PEM file holding the server's certificate
//! TUIMEX_SERVER_REQUIRE   of condstore, move, uidplus: those the server
//!                         must offer; one it lacks fails the spike
//! ```
//!
//! Without `TUIMEX_SERVER_HOST` it says it was skipped and passes, so
//! `make real-server` reads the last line it prints as well as its exit
//! status. The connection is made the way ADR-012 needs and the crate's
//! own builder cannot do: this test's TLS, trusting one file, handed to
//! `imap::Client::new`.

// A spike that cannot do what it came for should stop with the reason,
// which is what unwrap does; clippy's test allowance does not reach helpers.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashSet;
use std::env;
use std::thread;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use common::{Tls, message};
use imap::extensions::idle::WaitOutcome;
use imap::{Authenticator, Session};

/// Appends a message to the INBOX and finds its UID by search, which works
/// on a server without UIDPLUS too.
fn append(session: &mut Session<Tls>, subject: &str) -> u32 {
    session.append("INBOX", &message(subject)).finish().unwrap();
    session.select("INBOX").unwrap();
    let found = session.uid_search(format!("HEADER Subject {subject}")).unwrap();
    found.into_iter().max().expect("the appended message is found by its subject")
}

fn uids(session: &mut Session<Tls>, folder: &str) -> HashSet<u32> {
    session.select(folder).unwrap();
    session.uid_search("ALL").unwrap()
}

/// The token has gone with the command; a challenge after it is the
/// server saying why it refused, and is answered with nothing.
struct Bearer;

impl Authenticator for Bearer {
    type Response = String;

    fn process(&self, _challenge: &[u8]) -> String {
        String::new()
    }
}

#[test]
fn the_pinned_imap_client_does_what_the_engine_will_ask_of_it() {
    let Some(server) = common::server("spike") else { return };
    let require = env::var("TUIMEX_SERVER_REQUIRE").unwrap_or_default();
    let mut ran = Vec::new();
    let mut session = server.session();
    let capabilities = session.capabilities().unwrap();
    // Whether to exercise an extension: yes if the server has it, and a
    // failure if it was required and is missing.
    let offered = |name: &str| {
        let has = capabilities.has_str(name);
        assert!(has || !require.split(',').any(|r| r.eq_ignore_ascii_case(name)), "the server lacks {name}");
        if !has {
            println!("spike: {name} NOT OFFERED by this server; not exercised");
        }
        has
    };

    // 1. UIDVALIDITY and UIDNEXT are reported on SELECT.
    let mailbox = session.select("INBOX").unwrap();
    assert!(mailbox.uid_validity.is_some() && mailbox.uid_next.is_some(), "{mailbox:?}");
    ran.push("uidvalidity");

    // 2. ENABLE CONDSTORE: SELECT then reports a modseq, and a fetch
    //    CHANGEDSINCE it returns only what changed after.
    if offered("CONDSTORE") {
        session.run_command_and_check_ok("ENABLE CONDSTORE").unwrap();
        let before = session.select("INBOX").unwrap().highest_mod_seq.expect("a modseq after ENABLE CONDSTORE");
        let old = append(&mut session, "spike-condstore-old");
        let between = session.select("INBOX").unwrap().highest_mod_seq.unwrap();
        assert!(between > before, "{between} > {before}");
        let new = append(&mut session, "spike-condstore-new");
        let changed = session.uid_fetch("1:*", format!("(UID FLAGS) (CHANGEDSINCE {between})")).unwrap();
        let changed: Vec<u32> = changed.iter().filter_map(|f| f.uid).collect();
        assert!(changed.contains(&new) && !changed.contains(&old), "{changed:?}");
        assert!(session.uid_fetch("1:*", "(UID FLAGS MODSEQ)").unwrap().iter().all(|f| f.mod_seq().is_some()));
        ran.push("condstore");
    }

    // 3. IDLE returns when another connection delivers.
    assert!(offered("IDLE") || capabilities.has_str("IDLE"), "IDLE is always required");
    session.select("INBOX").unwrap();
    let mut other = server.session();
    let delivering = thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        other.append("INBOX", &message("spike-idle")).finish().unwrap();
    });
    let woken = session.idle().timeout(Duration::from_secs(60)).keepalive(false).wait_while(|_| false).unwrap();
    delivering.join().unwrap();
    assert_eq!(woken, WaitOutcome::MailboxChanged);
    ran.push("idle");

    // 4. UID MOVE takes the message out of the source and into the target.
    if offered("MOVE") {
        let target = "tuimex-spike-target";
        // The folder is there already on a server this has run against.
        if let Err(e) = session.create(target) {
            println!("spike: creating {target}: {e}");
        }
        let before = uids(&mut session, target).len();
        let uid = append(&mut session, "spike-move");
        session.uid_mv(uid.to_string(), target).unwrap();
        assert!(!uids(&mut session, "INBOX").contains(&uid));
        assert_eq!(uids(&mut session, target).len(), before + 1);
        ran.push("move");
    }

    // 5. UID EXPUNGE removes the message named and leaves another that is
    //    also flagged \Deleted, which a bare EXPUNGE would take (ADR-014).
    if offered("UIDPLUS") {
        let (named, bystander) = (append(&mut session, "spike-expunge-a"), append(&mut session, "spike-expunge-b"));
        session.uid_store(format!("{named},{bystander}"), "+FLAGS (\\Deleted)").unwrap();
        session.uid_expunge(named.to_string()).unwrap();
        let left = uids(&mut session, "INBOX");
        assert!(!left.contains(&named) && left.contains(&bystander), "{left:?}");
        session.uid_store(bystander.to_string(), "-FLAGS (\\Deleted)").unwrap();
        ran.push("uidplus");
    }

    // 6. XOAUTH2: the client speaks the mechanism and a bad token is
    //    refused by the server rather than by the client. The token goes
    //    with the command (SASL-IR), the only form GreenMail takes:
    //    without it the server's `BAD` is for the command's shape, and
    //    says nothing about the token. A server that does not offer the
    //    mechanism proves only that tuimex would not try.
    if capabilities.has_str("AUTH=XOAUTH2") {
        assert!(capabilities.has_str("SASL-IR"), "a server offering XOAUTH2 without SASL-IR is not one this covers");
        let said = BASE64.encode("user=spike@example.com\x01auth=Bearer not-a-token\x01\x01");
        match server.client().authenticate(format!("XOAUTH2 {said}"), &Bearer) {
            Err((imap::Error::No(_), _)) => ran.push("xoauth2"),
            Err((other, _)) => panic!("XOAUTH2 failed before the server could refuse the token: {other}"),
            Ok(_) => panic!("the server accepted a token that is not one"),
        }
    } else {
        println!("spike: AUTH=XOAUTH2 NOT OFFERED by this server; the exchange itself is not exercised");
        ran.push("xoauth2-absent");
    }

    session.logout().unwrap();
    println!("spike: ran {}", ran.join(","));
}

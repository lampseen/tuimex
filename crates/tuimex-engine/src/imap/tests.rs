use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};

use tuimex_core::{MoveOutcome, Remote, RemoteError, RemoteWrite};

use super::{Credential, Imap, Server, quotable};
use crate::{Secret, pin};

const PASSWORD: &str = "synthetic-password-never-sent";

/// A server on this machine that greets, answers `STARTTLS` with
/// `answer`, and then records every byte it is sent until the client
/// goes. It never speaks TLS.
fn plain_server(answer: &'static str) -> (Server, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let received = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.write_all(b"* OK a server that will not encrypt\r\n").unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut asked = String::new();
        reader.read_line(&mut asked).unwrap();
        socket.write_all(answer.as_bytes()).unwrap();
        // Whatever comes next is answered in plain text, as a server
        // that never meant to encrypt would. The client hanging up is
        // how this ends, so a failed read or write is expected.
        let mut first = [0; 4096];
        let n = reader.read(&mut first).unwrap_or(0);
        let _ = socket.write_all(b"* BAD that was not a command\r\n");
        let mut rest = Vec::new();
        let _ = reader.read_to_end(&mut rest);
        [asked.as_bytes(), &first[..n], &rest].concat()
    });
    let server = Server { host: "localhost".to_string(), port, starttls: true, user: "ada@example.org".to_string() };
    (server, received)
}

fn connect(server: &Server) -> RemoteError {
    let secret = Credential::Password(Secret::new(PASSWORD.to_string()));
    Imap::connect(server, "work", &secret, Some(b"a pinned certificate".to_vec())).err().expect("no connection")
}

fn holds(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

#[test]
fn a_server_that_does_not_offer_starttls_is_left_before_anything_else_is_sent() {
    let (server, received) = plain_server("tuimex BAD unknown command\r\n");
    let error = connect(&server);
    assert!(matches!(&error, RemoteError::Connection(why) if why.contains("would not start TLS")), "{error:?}");
    assert_eq!(received.join().unwrap(), b"tuimex STARTTLS\r\n", "the request to encrypt is all it was sent");
}

#[test]
fn a_server_that_says_ok_and_carries_on_in_plain_text_is_never_sent_a_credential() {
    let (server, received) = plain_server("tuimex OK begin TLS\r\n");
    let error = connect(&server);
    assert!(matches!(error, RemoteError::Connection(_)), "{error:?}");
    let received = received.join().unwrap();
    assert!(received.starts_with(b"tuimex STARTTLS\r\n"));
    assert!(!holds(&received, "LOGIN") && !holds(&received, PASSWORD) && !holds(&received, "ada@example.org"));
}

#[test]
fn what_is_written_into_a_command_cannot_end_its_string_or_its_line() {
    assert_eq!(quotable("Sent Items").as_deref(), Some("Sent Items"));
    assert_eq!(quotable("a\"b\\c").as_deref(), Some("a\\\"b\\\\c"));
    for hostile in ["x\r\ntuimex LOGOUT", "x\n", "nul\0", "caf\u{e9}"] {
        assert_eq!(quotable(hostile), None, "{hostile:?}");
    }
}

/// ADR-014's path for a server without MOVE, against the real server the
/// environment names (`tests/spike.rs` lists the variables), which is
/// treated as if it had not offered MOVE, and then not UIDPLUS either.
/// Here and not under `tests/` because only this module can say what a
/// connection believes its server offers.
#[test]
fn without_move_a_real_server_is_sent_a_copy_and_a_mark_and_only_a_named_expunge() {
    let Ok(host) = std::env::var("TUIMEX_SERVER_HOST") else {
        println!("real-server no-move: SKIPPED, no server: set TUIMEX_SERVER_HOST and the rest");
        return;
    };
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let server = Server {
        host,
        port: var("TUIMEX_SERVER_PORT").parse().unwrap(),
        starttls: false,
        user: var("TUIMEX_SERVER_USER"),
    };
    let ca = std::path::PathBuf::from(var("TUIMEX_SERVER_CA"));
    let pinned = pin::load(ca.parent().unwrap(), ca.file_stem().unwrap().to_str().unwrap()).unwrap();
    let secret = Credential::Password(Secret::new(var("TUIMEX_SERVER_PASSWORD")));
    let mut remote = Imap::connect(&server, "work", &secret, pinned).unwrap();
    assert!(remote.capabilities.uidplus, "the servers this runs against offer UIDPLUS");
    remote.capabilities.r#move = false;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    // The folder is there already on a server this has run against.
    let made = remote.session.create("tuimex-no-move");
    let mut message = |n: u8| {
        let id = format!("no-move-{stamp}-{n}@example.com");
        let raw = format!("From: spike@example.com\r\nMessage-ID: <{id}>\r\nSubject: no-move {n}\r\n\r\nsynthetic\r\n");
        remote.append("INBOX", raw.as_bytes(), Default::default()).unwrap();
        remote.select("INBOX").unwrap();
        (id.clone(), remote.find(&id).unwrap()[0])
    };
    let (named, bystander, left) = (message(1), message(2), message(3));
    // Marked deleted by someone else: a bare EXPUNGE would take it too.
    remote.session.uid_store(bystander.1.to_string(), "+FLAGS.SILENT (\\Deleted)").unwrap();

    assert_eq!(remote.move_to(&[named.1], "tuimex-no-move").unwrap(), MoveOutcome::Moved);
    let all = |remote: &mut Imap| remote.session.uid_search("ALL").unwrap();
    assert!(!all(&mut remote).contains(&named.1) && all(&mut remote).contains(&bystander.1));

    remote.capabilities.uidplus = false;
    assert_eq!(remote.move_to(&[left.1], "tuimex-no-move").unwrap(), MoveOutcome::CopiedAndFlagged);
    assert!(all(&mut remote).contains(&left.1), "the original waits for the server to purge it");
    assert!(!remote.uids().unwrap().contains(&left.1), "and is not listed as mail");
    assert_eq!(remote.find(&left.0).unwrap(), Vec::<u32>::new());
    remote.select("tuimex-no-move").unwrap();
    assert_eq!((remote.find(&named.0).unwrap().len(), remote.find(&left.0).unwrap().len()), (1, 1));
    println!(
        "real-server: ran no-move (created the folder: {}; the bystander marked deleted is still there)",
        made.is_ok()
    );
}

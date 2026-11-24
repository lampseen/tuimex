//! Writing a calendar over CalDAV (ADR-062), through the real `Https` wire,
//! against a throwaway CalDAV server named by the environment:
//!
//! TUIMEX_CALDAV_SERVER    the server's root, e.g. http://127.0.0.1:16232
//! TUIMEX_CALDAV_USER      a throwaway user, under whose name each run
//!                         makes a calendar of its own
//! TUIMEX_CALDAV_PASSWORD
//!
//! `make real-server` starts a Radicale in a container and sets them.
//! Without `TUIMEX_CALDAV_SERVER` each test says it was skipped and passes;
//! with it, each prints `real-server: ran ...`, which the script looks for.

// A test that cannot reach its server should stop with the reason.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tuimex_core::Address;
use tuimex_core::calendar::{Event, Guest, Moment, Said};
use tuimex_engine::Secret;
use tuimex_engine::calendar::{
    Body, CalendarError, Fetch, Https, Sent, Wire, check_caldav, delete, fetch_caldav, put_changed, put_new,
};
use tuimex_store::calendar::Calendars;

const PATIENCE: Duration = Duration::from_secs(60);
const DAY: i64 = 86_400;

struct Server {
    root: String,
    user: String,
    password: Secret,
}

fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is not set, and TUIMEX_CALDAV_SERVER is"))
}

/// The server the environment names, or None with a line saying the test
/// did not run: a skip must never read as a pass.
fn server(test: &str) -> Option<Server> {
    let Ok(root) = env::var("TUIMEX_CALDAV_SERVER") else {
        println!("{test}: SKIPPED, no server: set TUIMEX_CALDAV_SERVER and the rest (see tests/caldav_server.rs)");
        return None;
    };
    let root = root.trim_end_matches('/').to_string();
    Some(Server { root, user: var("TUIMEX_CALDAV_USER"), password: Secret::new(var("TUIMEX_CALDAV_PASSWORD")) })
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-caldav-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

fn store(name: &str) -> Calendars {
    let root = scratch(name);
    Calendars::open(&root.join("data"), &root.join("cache")).unwrap()
}

fn now() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()).unwrap()
}

fn mei() -> Address {
    Address { name: Some("Mei Tanaka".into()), email: "mei@example.org".into() }
}

fn lunch(uid: &str, start: i64, summary: &str) -> Event {
    Event {
        uid: uid.to_string(),
        sequence: 0,
        instead_of: None,
        summary: summary.to_string(),
        start: Moment::Utc(start),
        end: Some(Moment::Utc(start + 3_600)),
        location: Some("Room 4".into()),
        description: None,
        organizer: Some(Address { name: Some("Robin Vale".into()), email: "robin@example.org".into() }),
        guests: vec![Guest { who: mei(), said: Said::Nothing }],
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

/// A request sent as the test, not as tuimex: to set up, or to play
/// another client changing the calendar.
fn raw(wire: &Https, method: &'static str, url: &str, body: Option<Body<'_>>, server: &Server) -> u16 {
    let sent = Sent { method, url, depth: None, body, only_if: None, sign_in: Some((&server.user, &server.password)) };
    wire.send(&sent).unwrap().status
}

/// What `store` holds of the calendar at `url`: address and text.
fn held(store: &Calendars, url: &str) -> Vec<(String, String)> {
    let all = store.held().unwrap().into_iter().filter(|h| h.calendar.as_deref() == Some(url));
    all.map(|h| (h.href, h.text)).collect()
}

/// The logical lines of calendar text, unfolded.
fn lines(text: &str) -> Vec<String> {
    let unfolded = text.replace("\r\n", "\n").replace("\n ", "").replace("\n\t", "");
    unfolded.lines().filter(|l| !l.is_empty()).map(str::to_string).collect()
}

fn is_mei(line: &str) -> bool {
    line.starts_with("ATTENDEE") && line.to_ascii_lowercase().ends_with("mailto:mei@example.org")
}

#[test]
fn an_event_is_written_changed_answered_and_deleted_on_a_caldav_server() {
    let Some(server) = server("real-server caldav-write") else { return };
    let wire = Https::new(PATIENCE);
    let sign_in = (server.user.as_str(), &server.password);
    let now = now();
    let today = now.div_euclid(DAY);
    let window = (today - 90, today + 730);
    let unique = format!("tuimex-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
    let url = format!("{}/{}/{unique}/", server.root, server.user);

    // The calendar is made here, by the test: tuimex writes into a
    // calendar the user already has and never makes one.
    assert_eq!(raw(&wire, "MKCALENDAR", &url, None, &server), 201, "the calendar was made");

    // `writer` is what tuimex holds; `reader` is a second tuimex, empty,
    // which can only learn what the server holds.
    let writer = store("writer");
    let reader = store("reader");
    let fetch = |store: &Calendars| fetch_caldav(&wire, store, "work", &url, sign_in, window, now);

    let empty = fetch(&writer).unwrap();
    assert_eq!((empty.changed, empty.removed), (0, 0));
    assert!(held(&writer, &url).is_empty());

    let start = (today + 2) * DAY + 12 * 3_600;
    let uid = format!("{unique}@example.org");
    let text = tuimex_content::event_text(&lunch(&uid, start, "Lunch"), &BTreeMap::new(), now);
    let href = put_new(&wire, &writer, "work", &url, sign_in, &text, "k1").unwrap();
    let after_new = writer.tags("work", &url).unwrap();
    assert_eq!(after_new.len(), 1);
    assert!(after_new[0].1.is_some(), "the server gave the event's tag: {after_new:?}");

    assert_eq!(fetch(&reader).unwrap().changed, 1);
    let read_back = held(&reader, &url);
    assert_eq!(read_back.len(), 1);
    assert_eq!(read_back[0].0, href, "the server lists the event where tuimex put it");
    let theirs = tuimex_content::calendar(&read_back[0].1).events;
    assert_eq!(theirs, tuimex_content::calendar(&text).events, "the server holds the event that was written");
    assert_eq!((theirs[0].summary.as_str(), &theirs[0].start), ("Lunch", &Moment::Utc(start)));

    assert_eq!(
        put_new(&wire, &writer, "work", &url, sign_in, &text, "k1"),
        Err(CalendarError::Exists),
        "a second event of that name is refused"
    );

    let moved = tuimex_content::event_text(&lunch(&uid, start + 3_600, "Lunch, moved"), &BTreeMap::new(), now);
    put_changed(&wire, &writer, "work", &url, sign_in, &href, &moved).unwrap();
    assert_eq!(fetch(&reader).unwrap().changed, 1);
    assert_eq!(tuimex_content::calendar(&held(&reader, &url)[0].1).events[0].summary, "Lunch, moved");

    // Another client changes it; tuimex still holds the tag from before.
    let elsewhere = tuimex_content::event_text(&lunch(&uid, start, "Lunch, changed elsewhere"), &BTreeMap::new(), now);
    let target = format!("{}{href}", server.root);
    let status = raw(&wire, "PUT", &target, Some(Body::calendar(&elsewhere)), &server);
    assert!((200..300).contains(&status), "the change behind tuimex's back: {status}");
    assert_eq!(
        put_changed(&wire, &writer, "work", &url, sign_in, &href, &text),
        Err(CalendarError::Changed),
        "a change over one not seen is refused"
    );
    assert_eq!(held(&writer, &url)[0].1, moved, "and nothing held is touched");

    // Read again, answer on what the server holds, and put it back.
    assert_eq!(fetch(&writer).unwrap().changed, 1);
    let server_had = held(&writer, &url)[0].1.clone();
    assert_eq!(tuimex_content::calendar(&server_had).events[0].summary, "Lunch, changed elsewhere");
    let answered = tuimex_content::with_answer(&server_had, &mei(), Said::Accepted).expect("Mei is invited");
    put_changed(&wire, &writer, "work", &url, sign_in, &href, &answered).unwrap();
    fetch(&reader).unwrap();
    let after = held(&reader, &url)[0].1.clone();
    let event = &tuimex_content::calendar(&after).events[0];
    let said: Vec<_> = event.guests.iter().map(|g| (g.who.email.as_str(), g.said)).collect();
    assert_eq!(said, [("mei@example.org", Said::Accepted)]);
    let kept: BTreeSet<String> = lines(&after).into_iter().collect();
    let lost: Vec<String> = lines(&server_had).into_iter().filter(|l| !is_mei(l) && !kept.contains(l)).collect();
    assert!(lost.is_empty(), "every line the server had is still there; lost {lost:?}\n{after}");
    let mei_line = lines(&after).into_iter().find(|l| is_mei(l)).expect("Mei's line");
    assert!(mei_line.contains("PARTSTAT=ACCEPTED"), "{mei_line}");

    delete(&wire, &writer, "work", &url, sign_in, &href).unwrap();
    assert!(held(&writer, &url).is_empty());
    let gone = fetch(&reader).unwrap();
    assert_eq!(gone, Fetch { unchanged: false, changed: 0, removed: 1 });
    assert!(held(&reader, &url).is_empty(), "a fetch no longer lists it");

    let wrong = Secret::new(format!("not-{}", server.password.expose()));
    let wrong_sign_in = (server.user.as_str(), &wrong);
    assert_eq!(
        answered_by_the_server(|| fetch_caldav(&wire, &reader, "work", &url, wrong_sign_in, window, now)),
        Err(CalendarError::SignIn),
        "a wrong password on a read"
    );
    assert_eq!(
        answered_by_the_server(|| put_new(&wire, &writer, "work", &url, wrong_sign_in, &text, "k2")),
        Err(CalendarError::SignIn),
        "and on a write"
    );
    assert_eq!(raw(&wire, "DELETE", &url, None, &server) / 100, 2, "the calendar is removed");
    println!(
        "real-server: ran caldav-write (made, read empty, put, read back, refused twice, changed, answered, deleted)"
    );
}

/// What the server answered, asking again while the connection was reset
/// under the answer. Radicale's server refuses a sign-in without reading
/// the request's body and then closes, and a close over unread bytes is a
/// reset that can reach the client before the 401 does. Only an answer is
/// evidence of what a wrong password is said as, so a reset is asked
/// again; one that never stops is the failure it looks like.
fn answered_by_the_server<T: PartialEq>(ask: impl Fn() -> Result<T, CalendarError>) -> Result<T, CalendarError> {
    let reset = Err(CalendarError::Unreachable("the connection failed".to_string()));
    let mut answer = ask();
    for _ in 0..9 {
        if answer != reset {
            break;
        }
        answer = ask();
    }
    answer
}

/// Reads one request: its head, and as much body as it says it has.
fn read_request(stream: &mut TcpStream) {
    let mut seen = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            return;
        }
        seen.extend_from_slice(&buf[..n]);
        if let Some(i) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&seen[..head_end]).to_ascii_lowercase();
    let length: usize =
        head.lines().find_map(|l| l.strip_prefix("content-length:")).map_or(0, |v| v.trim().parse().unwrap());
    while seen.len() < head_end + length {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            return;
        }
        seen.extend_from_slice(&buf[..n]);
    }
}

#[test]
fn an_address_that_redirects_is_not_followed() {
    let Some(server) = server("real-server caldav-redirect") else { return };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&asked);
    // The real calendar server is where it sends: following it would be
    // answered, so only a refusal here shows nothing was followed.
    let location = format!("{}/{}/", server.root, server.user);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            read_request(&mut stream);
            counted.fetch_add(1, Ordering::SeqCst);
            let answer = format!(
                "HTTP/1.1 301 Moved Permanently\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(answer.as_bytes()).unwrap();
        }
    });

    let wire = Https::new(PATIENCE);
    let sign_in = (server.user.as_str(), &server.password);
    let url = format!("http://127.0.0.1:{port}/calendars/robin/");
    assert_eq!(check_caldav(&wire, &url, sign_in), Err(CalendarError::Redirected));
    let store = store("redirect");
    assert_eq!(
        put_new(&wire, &store, "work", &url, sign_in, "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n", "k1"),
        Err(CalendarError::Redirected)
    );
    assert_eq!(asked.load(Ordering::SeqCst), 2, "one request each, and none sent on");
    assert!(store.held().unwrap().is_empty());
    println!("real-server: ran caldav-redirect (a read and a write, each refused, neither followed)");
}

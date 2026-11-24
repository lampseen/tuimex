use std::cell::RefCell;
use std::path::PathBuf;

use super::*;

const URL: &str = "https://dav.example.org/calendar/dav/robin%40example.org/events/";

fn store(name: &str) -> Calendars {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-calendar-{}-{name}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    Calendars::open(&root.join("data"), &root.join("cache")).unwrap()
}

/// A request as the script saw it: method, address, depth, body, and
/// whether it was signed in.
type Asked = (String, String, Option<u8>, String, bool);

/// Answers in the order given and keeps what it was asked.
struct Script {
    answers: RefCell<Vec<Got>>,
    asked: RefCell<Vec<Asked>>,
}

impl Script {
    fn new(answers: Vec<(u16, String)>) -> Script {
        let answers = answers.into_iter().rev().map(|(status, body)| Got { status, body }).collect();
        Script { answers: RefCell::new(answers), asked: RefCell::new(Vec::new()) }
    }

    fn methods(&self) -> Vec<String> {
        self.asked.borrow().iter().map(|(method, ..)| method.clone()).collect()
    }
}

impl Wire for Script {
    fn send(&self, sent: &Sent<'_>) -> Result<Got, CalendarError> {
        self.asked.borrow_mut().push((
            sent.method.to_string(),
            sent.url.to_string(),
            sent.depth,
            sent.body.unwrap_or_default().to_string(),
            sent.sign_in.is_some(),
        ));
        Ok(self.answers.borrow_mut().pop().expect("the script has an answer for every request"))
    }
}

fn about(tag: &str) -> (u16, String) {
    let body = format!(
        r#"<D:multistatus xmlns:D="DAV:" xmlns:CS="http://calendarserver.org/ns/"><D:response><D:href>/calendar/dav/robin%40example.org/events/</D:href>
        <D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:displayname>Work</D:displayname><CS:getctag>{tag}</CS:getctag></D:prop></D:propstat></D:response></D:multistatus>"#
    );
    (207, body)
}

fn listing(items: &[(&str, &str)]) -> (u16, String) {
    let mut body = String::from(r#"<D:multistatus xmlns:D="DAV:">"#);
    for (href, etag) in items {
        body += &format!(
            "<D:response><D:href>{href}</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:getetag>{etag}</D:getetag></D:prop></D:propstat></D:response>"
        );
    }
    (207, body + "</D:multistatus>")
}

fn events(items: &[(&str, &str, &str)]) -> (u16, String) {
    let mut body = String::from(r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">"#);
    for (href, etag, summary) in items {
        body += &format!(
            "<D:response><D:href>{href}</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:getetag>{etag}</D:getetag>\
             <C:calendar-data>BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:{summary}@example.org\nDTSTART:20261126T100000Z\nSUMMARY:{summary}\nEND:VEVENT\nEND:VCALENDAR\n</C:calendar-data>\
             </D:prop></D:propstat></D:response>"
        );
    }
    (207, body + "</D:multistatus>")
}

const A: &str = "/calendar/dav/robin%40example.org/events/a.ics";
const B: &str = "/calendar/dav/robin%40example.org/events/b.ics";
const C: &str = "/calendar/dav/robin%40example.org/events/c.ics";

fn fetch(script: &Script, store: &Calendars) -> Result<Fetch, CalendarError> {
    let password = Secret::new("app password".to_string());
    fetch_caldav(script, store, "work", URL, ("robin@example.org", &password), (20_700, 21_500), 1_000)
}

fn summaries(store: &Calendars) -> Vec<String> {
    store.held().unwrap().iter().map(|h| tuimex_content::calendar(&h.text).events[0].summary.clone()).collect()
}

#[test]
fn a_calendar_is_fetched_once_and_then_only_what_changed() {
    let store = store("caldav");
    let first = Script::new(vec![
        about("t1"),
        listing(&[(A, "\"1\""), (B, "\"1\"")]),
        events(&[(A, "\"1\"", "Standup"), (B, "\"1\"", "Review")]),
    ]);
    assert_eq!(fetch(&first, &store), Ok(Fetch { unchanged: false, changed: 2, removed: 0 }));
    assert_eq!(first.methods(), vec!["PROPFIND", "REPORT", "REPORT"]);
    let asked = first.asked.borrow();
    assert!(
        asked.iter().all(|(_, url, _, _, signed)| url == URL && *signed),
        "every request goes to the calendar's own address, signed in"
    );
    assert_eq!((asked[0].2, asked[1].2), (Some(0), Some(1)));
    assert!(
        asked[1].3.contains(r#"<C:time-range start="20260904T000000Z" end="20281112T000000Z"/>"#),
        "{}",
        asked[1].3
    );
    assert!(asked[2].3.contains(&format!("<D:href>{A}</D:href><D:href>{B}</D:href>")));
    assert_eq!(summaries(&store), vec!["Standup", "Review"]);
    assert_eq!(store.fetched().unwrap()[0].name.as_deref(), Some("Work"));

    // The same tag: one request, and nothing else is asked.
    let same = Script::new(vec![about("t1")]);
    assert_eq!(fetch(&same, &store), Ok(Fetch { unchanged: true, changed: 0, removed: 0 }));
    assert_eq!(same.methods(), vec!["PROPFIND"]);

    // A changed, B gone, C new: only A and C are fetched.
    let later = Script::new(vec![
        about("t2"),
        listing(&[(A, "\"2\""), (C, "\"1\"")]),
        events(&[(A, "\"2\"", "Standup moved"), (C, "\"1\"", "Lunch")]),
    ]);
    assert_eq!(fetch(&later, &store), Ok(Fetch { unchanged: false, changed: 2, removed: 1 }));
    assert!(later.asked.borrow()[2].3.contains(&format!("<D:href>{A}</D:href><D:href>{C}</D:href>")));
    assert_eq!(summaries(&store), vec!["Standup moved", "Lunch"]);
    assert_eq!(store.tag("work", URL).unwrap().as_deref(), Some("t2"));
}

#[test]
fn a_server_cannot_add_what_was_not_listed_or_send_the_fetch_elsewhere() {
    let store = store("caldav-hostile");
    let elsewhere = "https://evil.example.net/steal.ics";
    let script = Script::new(vec![
        about("t1"),
        listing(&[(A, "\"1\""), (elsewhere, "\"1\""), ("//evil.example.net/x.ics", "\"1\"")]),
        events(&[(A, "\"1\"", "Standup"), (B, "\"9\"", "Planted")]),
    ]);
    assert_eq!(fetch(&script, &store), Ok(Fetch { unchanged: false, changed: 1, removed: 0 }));
    assert!(!script.asked.borrow()[2].3.contains("evil.example.net"), "an address on another host is never asked for");
    assert_eq!(summaries(&store), vec!["Standup"], "an event nobody asked for is not kept");
}

#[test]
fn a_refusal_says_what_kind_and_changes_nothing() {
    let store = store("caldav-refused");
    let good = Script::new(vec![about("t1"), listing(&[(A, "\"1\"")]), events(&[(A, "\"1\"", "Standup")])]);
    fetch(&good, &store).unwrap();
    for (answers, want) in [
        (vec![(401, String::new())], CalendarError::SignIn),
        (vec![(403, String::new())], CalendarError::SignIn),
        (vec![(302, String::new())], CalendarError::Redirected),
        (vec![(500, String::new())], CalendarError::Status(500)),
        (
            vec![(207, "<html>Sign in</html>".to_string())],
            CalendarError::Answer("the calendar server's answer could not be read: it is not a multistatus".into()),
        ),
        (vec![about("t2"), (503, String::new())], CalendarError::Status(503)),
        (vec![about("t2"), listing(&[(A, "\"2\"")]), (500, String::new())], CalendarError::Status(500)),
    ] {
        assert_eq!(fetch(&Script::new(answers), &store), Err(want));
    }
    assert_eq!(summaries(&store), vec!["Standup"]);
    assert_eq!(
        store.tag("work", URL).unwrap().as_deref(),
        Some("t1"),
        "a fetch that failed part-way is tried whole next time"
    );
}

#[test]
fn only_an_address_over_tls_with_no_sign_in_name_is_asked() {
    let store = store("addresses");
    let password = Secret::new("p".to_string());
    for (url, want) in [
        ("http://dav.example.org/events/", CalendarError::NotTls),
        ("webcal://dav.example.org/events/", CalendarError::NotTls),
        ("file:///etc/passwd", CalendarError::NotTls),
        ("https://robin:secret@dav.example.org/events/", CalendarError::Credentials),
        ("http://127.0.0.1.example.net/events/", CalendarError::NotTls),
    ] {
        let script = Script::new(Vec::new());
        assert_eq!(
            fetch_caldav(&script, &store, "work", url, ("robin", &password), (0, 1), 0),
            Err(want.clone()),
            "{url}"
        );
        assert_eq!(fetch_feed(&script, &store, "work", &Secret::new(url.to_string()), 0), Err(want), "{url}");
        assert!(script.asked.borrow().is_empty(), "{url}: nothing was sent");
    }
    assert_eq!(origin("https://dav.example.org"), Ok("https://dav.example.org"));
    assert_eq!(origin("http://127.0.0.1:8443/x?y"), Ok("http://127.0.0.1:8443"));
    assert_eq!(within(URL, "/a.ics").as_deref(), Some("https://dav.example.org/a.ics"));
    assert_eq!(within(URL, "https://dav.example.org/a.ics").as_deref(), Some("https://dav.example.org/a.ics"));
    assert_eq!(within(URL, "https://dav.example.org.evil.example.net/a.ics"), None);
    assert_eq!(within(URL, "a.ics"), None);
}

#[test]
fn a_feed_is_kept_whole_and_its_address_is_in_no_error() {
    let store = store("feed");
    let address = Secret::new("https://calendar.example.net/api/calendar/v1/url/SECRET-TOKEN/calendar.ics".to_string());
    let feed = |summary: &str| {
        (
            200,
            format!(
                "\u{feff}BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x@example.net\r\nDTSTART:20261127T090000Z\r\nSUMMARY:{summary}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
            ),
        )
    };
    let first = Script::new(vec![feed("Dentist")]);
    assert_eq!(
        fetch_feed(&first, &store, "personal", &address, 500),
        Ok(Fetch { unchanged: false, changed: 1, removed: 0 })
    );
    let asked = first.asked.borrow();
    assert_eq!(
        (asked[0].0.as_str(), asked[0].2, asked[0].4),
        ("GET", None, false),
        "a feed is one plain request, with no sign-in beside its address"
    );
    assert_eq!(summaries(&store), vec!["Dentist"]);
    let held = &store.held().unwrap()[0];
    assert_eq!(
        (held.calendar.as_deref(), held.href.as_str()),
        (Some(FEED), ""),
        "the address is not what the feed is filed under"
    );

    assert_eq!(
        fetch_feed(&Script::new(vec![feed("Dentist")]), &store, "personal", &address, 600),
        Ok(Fetch { unchanged: true, changed: 0, removed: 0 })
    );
    assert_eq!(store.fetched().unwrap()[0].read_at, 600);
    assert_eq!(
        fetch_feed(&Script::new(vec![feed("Dentist, moved")]), &store, "personal", &address, 700).unwrap().changed,
        1
    );
    assert_eq!(summaries(&store), vec!["Dentist, moved"]);

    for (answer, want) in [
        ((200, "<html>Sign in to continue</html>".to_string()), CalendarError::NotACalendar),
        ((200, String::new()), CalendarError::NotACalendar),
        ((404, String::new()), CalendarError::Status(404)),
        ((301, String::new()), CalendarError::Redirected),
    ] {
        let error = fetch_feed(&Script::new(vec![answer]), &store, "personal", &address, 800).unwrap_err();
        assert_eq!(error, want);
        assert!(!error.to_string().contains("SECRET-TOKEN") && !format!("{error:?}").contains("SECRET-TOKEN"));
    }
    assert_eq!(summaries(&store), vec!["Dentist, moved"], "a failed fetch leaves what was held");
    assert_eq!(store.fetched().unwrap()[0].read_at, 700, "and it is not counted as read");
}

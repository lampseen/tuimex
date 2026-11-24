use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Instant;

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

/// What a request said beside its address and body: its precondition, as
/// `If-Match: <tag>` or `If-None-Match: <tag>`, and its body's type.
type Said = (Option<String>, Option<&'static str>);

/// Answers in the order given and keeps what it was asked.
struct Script {
    answers: RefCell<Vec<Got>>,
    asked: RefCell<Vec<Asked>>,
    said: RefCell<Vec<Said>>,
}

impl Script {
    fn new(answers: Vec<(u16, String)>) -> Script {
        Script::answering(answers.into_iter().map(|(status, body)| Got { status, body, etag: None }).collect())
    }

    fn answering(answers: Vec<Got>) -> Script {
        let answers = answers.into_iter().rev().collect();
        Script { answers: RefCell::new(answers), asked: RefCell::new(Vec::new()), said: RefCell::new(Vec::new()) }
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
            sent.body.map_or("", |body| body.text).to_string(),
            sent.sign_in.is_some(),
        ));
        let only_if = sent.only_if.map(|only| match only {
            Only::IfMatch(tag) => format!("If-Match: {tag}"),
            Only::IfNoneMatch(tag) => format!("If-None-Match: {tag}"),
        });
        self.said.borrow_mut().push((only_if, sent.body.map(|body| body.content_type)));
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

/// A keyring that exists only in the test: the user's is never opened.
#[derive(Default)]
struct MemoryKeyring(Mutex<HashMap<String, String>>);

impl MemoryKeyring {
    fn with(entries: &[(&str, &str)]) -> MemoryKeyring {
        let keyring = MemoryKeyring::default();
        for (entry, secret) in entries {
            keyring.0.lock().unwrap().insert(entry.to_string(), secret.to_string());
        }
        keyring
    }
}

impl crate::Keyring for MemoryKeyring {
    fn password(&self, entry: &str) -> Result<Option<Secret>, String> {
        Ok(self.0.lock().unwrap().get(entry).map(|kept| Secret::new(kept.clone())))
    }

    fn set(&self, entry: &str, secret: &Secret) -> Result<(), String> {
        self.0.lock().unwrap().insert(entry.to_string(), secret.expose().to_string());
        Ok(())
    }
}

/// Answers each address the same way every time, from any thread.
struct Served(Vec<(&'static str, u16, String)>);

impl Wire for Served {
    fn send(&self, sent: &Sent<'_>) -> Result<Got, CalendarError> {
        let (_, status, body) = self.0.iter().find(|(url, ..)| *url == sent.url).expect("an address it serves");
        Ok(Got { status: *status, body: body.clone(), etag: None })
    }
}

const FEED_ADDRESS: &str = "https://calendar.example.net/api/calendar/v1/url/SECRET-TOKEN/calendar.ics";
const DENTIST: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:d@example.net\r\nDTSTART:20261127T090000Z\r\n\
                       SUMMARY:Dentist\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

fn root(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-calendar-{}-{name}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    root
}

fn work_and_home() -> Vec<Configured> {
    vec![
        Configured {
            account: "work".into(),
            source: Source::CalDav { url: URL.into(), user: "robin@example.org".into(), password_cmd: None },
        },
        Configured { account: "home".into(), source: Source::Feed { address_cmd: None } },
    ]
}

/// Work's server refuses the sign-in; home's feed is a calendar.
fn served() -> Served {
    Served(vec![(URL, 401, String::new()), (FEED_ADDRESS, 200, DENTIST.to_string())])
}

#[test]
fn a_pass_over_two_accounts_reads_the_one_that_answers_when_the_other_fails() {
    let store = store("pass");
    let keyring = MemoryKeyring::with(&[("work", "app password"), ("home/calendar-feed", FEED_ADDRESS)]);
    let outcomes = pass(&served(), &store, &keyring, &work_and_home(), 1_000);
    let said: Vec<_> =
        outcomes.iter().map(|o| (o.account.as_str(), o.kind, o.host.as_deref(), o.read.clone(), o.read_at)).collect();
    assert_eq!(
        said,
        [
            ("work", "caldav", Some("dav.example.org"), Err(CalendarError::SignIn), None),
            (
                "home",
                "feed",
                Some("calendar.example.net"),
                Ok(Fetch { unchanged: false, changed: 1, removed: 0 }),
                Some(1_000)
            ),
        ]
    );
    assert_eq!(summaries(&store), ["Dentist"]);

    // With nothing to find a secret with, each says which and goes no
    // further; the feed's last reading is still said.
    let outcomes = pass(&served(), &store, &MemoryKeyring::default(), &work_and_home(), 2_000);
    let reasons: Vec<String> = outcomes.iter().map(|o| o.read.clone().unwrap_err().to_string()).collect();
    assert_eq!(
        reasons,
        [
            "there is no password for work in the keyring, and no password_cmd",
            "there is no address for home's calendar in the keyring, and no address_cmd; run `tuimex setup`",
        ]
    );
    assert_eq!((outcomes[1].host.as_deref(), outcomes[1].read_at), (None, Some(1_000)));
}

#[test]
fn a_feed_s_address_is_printed_by_its_command_and_a_failing_command_says_so_without_it() {
    let store = store("address-cmd");
    let by = |argv: &[&str]| Configured {
        account: "home".into(),
        source: Source::Feed { address_cmd: Some(argv.iter().map(ToString::to_string).collect()) },
    };
    let keyring = MemoryKeyring::default();
    let outcomes = pass(&served(), &store, &keyring, &[by(&["printf", "%s\\n", FEED_ADDRESS])], 1_000);
    assert_eq!(outcomes[0].read, Ok(Fetch { unchanged: false, changed: 1, removed: 0 }));
    let printed_then_failed = format!("echo {FEED_ADDRESS}; exit 3");
    for (argv, said) in [
        (vec!["sh", "-c", printed_then_failed.as_str()], "address_cmd for home's calendar failed: sh ended"),
        (vec!["printf", ""], "address_cmd for home's calendar printed no address"),
        (vec!["/nonexistent/tuimex-no-such-program"], "address_cmd for home's calendar could not run"),
    ] {
        let error = pass(&served(), &store, &keyring, &[by(&argv)], 2_000).remove(0).read.unwrap_err();
        assert!(error.to_string().starts_with(said), "{error}");
        assert!(!error.to_string().contains("SECRET-TOKEN") && !format!("{error:?}").contains("SECRET-TOKEN"));
    }
}

#[test]
fn caldav_is_asked_for_ninety_days_back_and_two_years_on() {
    let store = store("window");
    let keyring = MemoryKeyring::with(&[("work", "app password")]);
    let script = Script::new(vec![about("t1"), listing(&[(A, "\"1\"")]), events(&[(A, "\"1\"", "Standup")])]);
    let today = 20_000;
    pass(&script, &store, &keyring, &work_and_home()[..1], today * 86_400 + 3_600);
    let asked = &script.asked.borrow()[1].3;
    let range = format!(r#"<C:time-range start="{}" end="{}"/>"#, stamp(today - 90), stamp(today + 730));
    assert!(asked.contains(&range), "{asked}");
}

#[test]
fn a_check_asks_once_and_keeps_nothing() {
    let password = Secret::new("app password".to_string());
    let script = Script::new(vec![about("t1")]);
    assert_eq!(check_caldav(&script, URL, ("robin@example.org", &password)), Ok(Some("Work".to_string())));
    assert_eq!(script.methods(), ["PROPFIND"]);
    assert_eq!(script.asked.borrow()[0].2, Some(0));
    let refused = Script::new(vec![(403, String::new())]);
    assert_eq!(check_caldav(&refused, URL, ("robin@example.org", &password)), Err(CalendarError::SignIn));

    let address = Secret::new(FEED_ADDRESS.to_string());
    assert_eq!(check_feed(&served(), &address), Ok(()));
    let page = Script::new(vec![(200, "<!doctype html><title>Sign in</title>".to_string())]);
    assert_eq!(check_feed(&page, &address), Err(CalendarError::NotACalendar));
    assert_eq!(host(FEED_ADDRESS), Some("calendar.example.net"));
    assert_eq!(host("http://calendar.example.net/x"), None);
    assert_eq!(feed_entry("home"), "home/calendar-feed");
}

/// The events of one pass, in the order the calendars are configured.
fn next_pass(events: &Receiver<Event>) -> Vec<Event> {
    (0..2).map(|_| events.recv_timeout(Duration::from_secs(20)).expect("an event of the pass")).collect()
}

fn runner(root: &Path, every: Duration) -> Runner {
    Runner {
        calendars: work_and_home(),
        wire: Box::new(served()),
        keyring: Box::new(MemoryKeyring::with(&[("work", "app password"), ("home/calendar-feed", FEED_ADDRESS)])),
        data: root.join("data"),
        cache: root.join("cache"),
        every,
    }
}

fn ended(thread: &JoinHandle<()>) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !thread.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    thread.is_finished()
}

#[test]
fn the_thread_reads_at_start_and_when_nudged_says_each_and_stops_when_its_channel_closes() {
    let root = root("thread");
    let (nudge, nudges) = mpsc::channel();
    let (sender, events) = mpsc::channel();
    let running = start(runner(&root, Duration::from_secs(3_600)), nudges, sender);
    let refused = Event::CalendarFailed {
        account: AccountId("work".into()),
        reason: "the calendar's server refused the sign-in".into(),
        read_at: None,
    };
    let read = |changed| Event::CalendarRead { account: AccountId("home".into()), changed };
    assert_eq!(next_pass(&events), [refused.clone(), read(true)]);
    nudge.send(()).unwrap();
    assert_eq!(next_pass(&events), [refused, read(false)], "the same feed again is no change");
    drop(nudge);
    assert!(ended(&running), "the thread ends once nobody can nudge it");
    assert!(events.recv_timeout(Duration::from_millis(50)).is_err(), "and says nothing more");

    let kept = kept(&root.join("data"), &root.join("cache")).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!((kept[0].account.as_str(), kept[0].local, kept[0].name.as_deref()), ("home", false, None));
    assert!(kept[0].read_at.is_some());
    assert_eq!(kept[0].calendar.events[0].summary, "Dentist");
}

#[test]
fn the_thread_reads_again_unasked_and_stops_when_nobody_listens() {
    let root = root("thread-tick");
    let (nudge, nudges) = mpsc::channel();
    let (sender, events) = mpsc::channel();
    let running = start(runner(&root, Duration::from_millis(20)), nudges, sender);
    next_pass(&events);
    next_pass(&events);
    drop(events);
    assert!(ended(&running), "the thread ends once what it says reaches nobody");
    drop(nudge);
}

#[test]
fn what_is_kept_says_whose_it_is_whether_it_is_local_and_when_it_was_read() {
    let root = root("kept");
    let (data, cache) = (root.join("data"), root.join("cache"));
    let store = Calendars::open(&data, &cache).unwrap();
    let keyring = MemoryKeyring::with(&[("work", "app password")]);
    let script = Script::new(vec![about("t1"), listing(&[(A, "\"1\"")]), events(&[(A, "\"1\"", "Standup")])]);
    pass(&script, &store, &keyring, &work_and_home()[..1], 5_000);
    store.keep("home", DENTIST).unwrap();
    let kept = kept(&data, &cache).unwrap();
    let said: Vec<_> = kept
        .iter()
        .map(|k| (k.account.as_str(), k.local, k.name.as_deref(), k.read_at, k.calendar.events[0].summary.as_str()))
        .collect();
    assert_eq!(said, [("work", false, Some("Work"), Some(5_000), "Standup"), ("home", true, None, None, "Dentist")]);
}

const ROBIN: &str = "robin@example.org";
const LUNCH: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:lunch@example.org\r\nDTSTART:20261126T120000Z\r\n\
                     SUMMARY:Lunch\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
const LUNCH_MOVED: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:lunch@example.org\r\nDTSTART:20261126T130000Z\r\n\
                           SUMMARY:Lunch moved\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
const CALENDAR_TYPE: Option<&str> = Some("text/calendar; charset=utf-8");

fn got(status: u16, etag: Option<&str>) -> Got {
    Got { status, body: String::new(), etag: etag.map(str::to_string) }
}

/// What the store holds of the calendar at `URL`: address, tag and text.
fn holds(store: &Calendars) -> Vec<(String, Option<String>, String)> {
    let held = store.held().unwrap().into_iter().filter(|h| h.calendar.as_deref() == Some(URL));
    held.map(|h| (h.href, h.etag, h.text)).collect()
}

/// A store that holds A and B at tag "1", as a fetch left them.
fn store_holding_a_and_b(name: &str) -> Calendars {
    let store = store(name);
    let read = Script::new(vec![
        about("t1"),
        listing(&[(A, "\"1\""), (B, "\"1\"")]),
        events(&[(A, "\"1\"", "Standup"), (B, "\"1\"", "Review")]),
    ]);
    fetch(&read, &store).unwrap();
    store
}

fn put_new_as_robin(script: &Script, store: &Calendars, text: &str, name: &str) -> Result<String, CalendarError> {
    let password = Secret::new("app password".to_string());
    put_new(script, store, "work", URL, (ROBIN, &password), text, name)
}

fn put_changed_as_robin(script: &Script, store: &Calendars, href: &str, text: &str) -> Result<(), CalendarError> {
    let password = Secret::new("app password".to_string());
    put_changed(script, store, "work", URL, (ROBIN, &password), href, text)
}

fn delete_as_robin(script: &Script, store: &Calendars, href: &str) -> Result<(), CalendarError> {
    let password = Secret::new("app password".to_string());
    delete(script, store, "work", URL, (ROBIN, &password), href)
}

/// Method, address and body of the one request `script` was sent.
fn only_request(script: &Script) -> (String, String, String) {
    let asked = script.asked.borrow();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(asked[0].4, "a write is signed in");
    assert_eq!(asked[0].2, None, "a write has no depth");
    (asked[0].0.clone(), asked[0].1.clone(), asked[0].3.clone())
}

#[test]
fn a_read_carries_no_precondition_and_says_its_body_is_xml() {
    let store = store("read-headers");
    let script = Script::new(vec![about("t1"), listing(&[(A, "\"1\"")]), events(&[(A, "\"1\"", "Standup")])]);
    fetch(&script, &store).unwrap();
    let xml = Some("application/xml; charset=utf-8");
    assert_eq!(*script.said.borrow(), [(None, xml), (None, xml), (None, xml)]);
}

#[test]
fn a_new_event_is_put_only_where_nothing_is_and_kept_with_the_tag_the_server_gave() {
    let store = store("put-new");
    let script = Script::answering(vec![got(201, Some("\"e1\""))]);
    let href = put_new_as_robin(&script, &store, LUNCH, "k1.lunch").unwrap();
    assert_eq!(href, "/calendar/dav/robin%40example.org/events/k1.lunch.ics");
    assert_eq!(only_request(&script), ("PUT".into(), format!("{URL}k1.lunch.ics"), LUNCH.into()));
    assert_eq!(
        script.said.borrow()[0],
        (Some("If-None-Match: *".to_string()), CALENDAR_TYPE),
        "a new event never lands on one that is there"
    );
    assert_eq!(holds(&store), [(href.clone(), Some("\"e1\"".to_string()), LUNCH.to_string())]);

    // The same name again: the server says something is there.
    let again = Script::answering(vec![got(412, None)]);
    assert_eq!(put_new_as_robin(&again, &store, LUNCH_MOVED, "k1.lunch"), Err(CalendarError::Exists));
    assert_eq!(holds(&store), [(href, Some("\"e1\"".to_string()), LUNCH.to_string())], "and nothing is kept");
}

#[test]
fn a_new_event_s_name_is_tuimex_s_own_and_never_leaves_the_calendar() {
    let store = store("put-new-names");
    let long = "k".repeat(201);
    for name in [
        "",
        ".",
        "..",
        ".hidden",
        "../other/x",
        "a/b",
        "lunch@example.org",
        "a b",
        "%2e%2e",
        "a?b",
        "a#b",
        "é",
        long.as_str(),
    ] {
        let script = Script::new(Vec::new());
        assert_eq!(put_new_as_robin(&script, &store, LUNCH, name), Err(CalendarError::Name), "{name:?}");
        assert!(script.asked.borrow().is_empty(), "{name:?}: nothing was sent");
    }
    assert!(holds(&store).is_empty());
    for name in ["k1", "0123.abc", "a-b_c.d"] {
        assert!(valid_name(name), "{name}");
    }
}

#[test]
fn a_change_is_put_only_over_the_tag_last_read_and_a_412_leaves_the_store_as_it_was() {
    let store = store_holding_a_and_b("put-changed");
    let script = Script::answering(vec![got(204, Some("\"2\""))]);
    put_changed_as_robin(&script, &store, A, LUNCH_MOVED).unwrap();
    assert_eq!(only_request(&script), ("PUT".into(), format!("https://dav.example.org{A}"), LUNCH_MOVED.into()));
    assert_eq!(
        script.said.borrow()[0],
        (Some("If-Match: \"1\"".to_string()), CALENDAR_TYPE),
        "the change is made only over what was last read"
    );
    let after = holds(&store);
    assert_eq!(after[0], (A.to_string(), Some("\"2\"".to_string()), LUNCH_MOVED.to_string()));

    let changed = Script::answering(vec![got(412, None)]);
    let error = put_changed_as_robin(&changed, &store, A, LUNCH).unwrap_err();
    assert_eq!(error, CalendarError::Changed);
    assert_eq!(error.to_string(), "it changed at the provider; read the calendar again and repeat the change");
    assert_eq!(changed.said.borrow()[0].0.as_deref(), Some("If-Match: \"2\""), "the tag the last write returned");
    assert_eq!(holds(&store), after, "a refused change touches nothing held");
}

#[test]
fn a_write_with_no_tag_held_is_refused_before_anything_is_sent_until_a_fetch_reads_it_again() {
    let store = store("no-tag");
    let href = put_new_as_robin(&Script::answering(vec![got(201, None)]), &store, LUNCH, "k2").unwrap();
    assert_eq!(holds(&store), [(href.clone(), None, LUNCH.to_string())], "what was written is kept, with no tag");

    let script = Script::new(Vec::new());
    assert_eq!(put_changed_as_robin(&script, &store, &href, LUNCH_MOVED), Err(CalendarError::Unread));
    assert_eq!(delete_as_robin(&script, &store, &href), Err(CalendarError::Unread));
    assert_eq!(put_changed_as_robin(&script, &store, A, LUNCH_MOVED), Err(CalendarError::Unread), "nor one not held");
    assert!(script.asked.borrow().is_empty(), "nothing was sent");

    let read = Script::new(vec![
        about("t2"),
        listing(&[(href.as_str(), "\"3\"")]),
        events(&[(href.as_str(), "\"3\"", "Lunch")]),
    ]);
    fetch(&read, &store).unwrap();
    assert!(read.asked.borrow()[2].3.contains(&format!("<D:href>{href}</D:href>")), "the next fetch asks for it");
    let script = Script::answering(vec![got(204, Some("\"4\""))]);
    put_changed_as_robin(&script, &store, &href, LUNCH_MOVED).unwrap();
    assert_eq!(script.said.borrow()[0].0.as_deref(), Some("If-Match: \"3\""));
}

#[test]
fn an_address_off_the_calendar_is_refused_before_anything_is_sent() {
    let store = store_holding_a_and_b("elsewhere");
    for href in [
        "https://evil.example.net/calendar/dav/robin%40example.org/events/a.ics",
        "https://dav.example.org.evil.example.net/calendar/dav/robin%40example.org/events/a.ics",
        "//evil.example.net/calendar/dav/robin%40example.org/events/a.ics",
        "/calendar/dav/robin%40example.org/other/a.ics",
        "/calendar/dav/robin%40example.org/events/../other/a.ics",
        "/calendar/dav/robin%40example.org/events/%2e%2e/other/a.ics",
        "/calendar/dav/robin%40example.org/events/./a.ics",
        "/calendar/dav/robin%40example.org/events/",
        "/calendar/dav/robin%40example.org/events",
        "/calendar/dav/robin%40example.org/events/a.ics?x=1",
        "/calendar/dav/robin%40example.org/events//a.ics",
        "a.ics",
    ] {
        let script = Script::new(Vec::new());
        assert_eq!(put_changed_as_robin(&script, &store, href, LUNCH), Err(CalendarError::Elsewhere), "{href}");
        assert_eq!(delete_as_robin(&script, &store, href), Err(CalendarError::Elsewhere), "{href}");
        assert!(script.asked.borrow().is_empty(), "{href}: nothing was sent");
    }
    assert_eq!(
        under(URL, "/calendar/dav/robin@example.org/events/a.ics").as_deref(),
        Some("https://dav.example.org/calendar/dav/robin@example.org/events/a.ics"),
        "one address spelled two ways is still under the calendar"
    );
    assert_eq!(under(URL, A), Some(format!("https://dav.example.org{A}")));
    assert_eq!(holds(&store).len(), 2);
}

#[test]
fn a_delete_is_conditional_and_one_already_gone_is_done() {
    let store = store_holding_a_and_b("delete");
    let script = Script::answering(vec![got(204, None)]);
    delete_as_robin(&script, &store, A).unwrap();
    assert_eq!(only_request(&script), ("DELETE".into(), format!("https://dav.example.org{A}"), String::new()));
    assert_eq!(script.said.borrow()[0], (Some("If-Match: \"1\"".to_string()), None));
    assert_eq!(holds(&store).iter().map(|h| h.0.as_str()).collect::<Vec<_>>(), [B]);

    let changed = Script::answering(vec![got(412, None)]);
    assert_eq!(delete_as_robin(&changed, &store, B), Err(CalendarError::Changed));
    assert_eq!(holds(&store).len(), 1, "a refused delete keeps what was held");

    delete_as_robin(&Script::answering(vec![got(404, None)]), &store, B).unwrap();
    assert!(holds(&store).is_empty(), "one already gone at the provider is forgotten here");
}

#[test]
fn a_write_the_server_refuses_says_what_kind_and_changes_nothing() {
    let store = store_holding_a_and_b("write-refused");
    let before = holds(&store);
    for (status, want) in [
        (401, CalendarError::SignIn),
        (403, CalendarError::SignIn),
        (302, CalendarError::Redirected),
        (500, CalendarError::Status(500)),
    ] {
        let one = || Script::answering(vec![got(status, Some("\"9\""))]);
        assert_eq!(put_new_as_robin(&one(), &store, LUNCH, "k3"), Err(want.clone()), "{status}");
        assert_eq!(put_changed_as_robin(&one(), &store, A, LUNCH), Err(want.clone()), "{status}");
        assert_eq!(delete_as_robin(&one(), &store, A), Err(want), "{status}");
    }
    assert_eq!(holds(&store), before);

    let password = Secret::new("p".to_string());
    for (url, want) in [
        ("http://dav.example.org/events/", CalendarError::NotTls),
        ("https://robin:secret@dav.example.org/events/", CalendarError::Credentials),
    ] {
        let script = Script::new(Vec::new());
        let sign_in = (ROBIN, &password);
        assert_eq!(put_new(&script, &store, "work", url, sign_in, LUNCH, "k4"), Err(want.clone()), "{url}");
        assert_eq!(put_changed(&script, &store, "work", url, sign_in, A, LUNCH), Err(want.clone()), "{url}");
        assert_eq!(delete(&script, &store, "work", url, sign_in, A), Err(want), "{url}");
        assert!(script.asked.borrow().is_empty(), "{url}: nothing was sent");
    }
}

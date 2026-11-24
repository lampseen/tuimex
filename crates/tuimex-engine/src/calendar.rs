//! Fetching calendars (ADR-056, ADR-062): Google's over CalDAV with the
//! account's own password, any other from a private address that serves
//! iCalendar. What comes back is kept as text in the store's cache; only
//! `tuimex-content` reads it. A CalDAV calendar is also written, each
//! write conditional on the tag last read, so that a change made
//! elsewhere is never written over unseen.
//!
//! Every request goes to an address the user configured, over TLS, with no
//! redirect followed. Nothing a calendar names is fetched.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use tuimex_content::{DavItem, multistatus};
use tuimex_core::{AccountId, Event, Timestamp};
use tuimex_store::calendar::Calendars;

use crate::{Keyring, Secret, SecretError, resolve};

#[cfg(test)]
mod tests;

/// The name a feed is kept under: it has one address, and that address is
/// a secret, so it is never what the store files it by.
pub const FEED: &str = "feed";

/// The largest answer read. A calendar of ten thousand events is about a
/// tenth of this.
const LARGEST: u64 = 32 << 20;

/// How many events are asked for in one request.
const BATCH: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CalendarError {
    #[error("a calendar is fetched over TLS or not at all")]
    NotTls,
    #[error("a calendar's address cannot carry a sign-in name")]
    Credentials,
    #[error("the calendar's server could not be reached: {0}")]
    Unreachable(String),
    #[error("the calendar's server refused the sign-in")]
    SignIn,
    #[error("the calendar's server answered {0}")]
    Status(u16),
    #[error("what the address served is not a calendar")]
    NotACalendar,
    #[error("the calendar's server sent somewhere else, which is not followed")]
    Redirected,
    #[error("{0}")]
    Answer(String),
    #[error("it changed at the provider; read the calendar again and repeat the change")]
    Changed,
    #[error("an event of that name is already in the calendar")]
    Exists,
    #[error("an event's name holds letters, digits, `.`, `-` and `_` alone, and begins with no `.`")]
    Name,
    #[error("the event is not in this calendar, so it is not written")]
    Elsewhere,
    #[error("the event has not been read as the provider holds it; read the calendar again first")]
    Unread,
    #[error("the calendar could not be kept: {0}")]
    Store(String),
    #[error("the system gave no randomness to name the event with")]
    Random,
    /// Why the password or the feed's address could not be had, in words
    /// that hold neither.
    #[error("{0}")]
    Secret(String),
}

impl From<tuimex_store::Error> for CalendarError {
    fn from(error: tuimex_store::Error) -> CalendarError {
        CalendarError::Store(error.to_string())
    }
}

/// One request. `sign_in` is the user name and password sent with it.
pub struct Sent<'a> {
    pub method: &'static str,
    pub url: &'a str,
    pub depth: Option<u8>,
    pub body: Option<Body<'a>>,
    /// What the server must find at `url` for the request to be carried
    /// out: what keeps a write from landing on a change it has not seen.
    pub only_if: Option<Only<'a>>,
    pub sign_in: Option<(&'a str, &'a Secret)>,
}

/// What a request carries, and what kind of text it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Body<'a> {
    pub content_type: &'static str,
    pub text: &'a str,
}

impl<'a> Body<'a> {
    pub fn xml(text: &'a str) -> Body<'a> {
        Body { content_type: "application/xml; charset=utf-8", text }
    }

    pub fn calendar(text: &'a str) -> Body<'a> {
        Body { content_type: "text/calendar; charset=utf-8", text }
    }
}

/// A precondition of a write (RFC 7232).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Only<'a> {
    /// `If-Match`: only if what is there has this tag.
    IfMatch(&'a str),
    /// `If-None-Match`, which with `*` is: only if nothing is there.
    IfNoneMatch(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Got {
    pub status: u16,
    pub body: String,
    /// The `ETag` the server answered with, where it gave one.
    pub etag: Option<String>,
}

/// What carries a request. A trait so that a test answers from a script
/// and never opens a socket.
pub trait Wire {
    fn send(&self, sent: &Sent<'_>) -> Result<Got, CalendarError>;
}

/// `https://host`, or the loopback address a test server listens on, of a
/// URL that may be fetched.
fn origin(url: &str) -> Result<&str, CalendarError> {
    let scheme = if url.starts_with("https://") {
        "https://".len()
    } else if url.starts_with("http://127.0.0.1:") {
        "http://".len()
    } else {
        return Err(CalendarError::NotTls);
    };
    let end = url[scheme..].find(['/', '?', '#']).map_or(url.len(), |i| scheme + i);
    if url[scheme..end].contains('@') {
        return Err(CalendarError::Credentials);
    }
    Ok(&url[..end])
}

/// The URL of something a server listed, when it is on the server asked;
/// an address anywhere else is not followed (ADR-008).
fn within(base: &str, href: &str) -> Option<String> {
    let origin = origin(base).ok()?;
    if href.starts_with('/') && !href.starts_with("//") {
        return Some(format!("{origin}{href}"));
    }
    (href.starts_with(origin) && href[origin.len()..].starts_with('/')).then(|| href.to_string())
}

fn ok(got: Got) -> Result<Got, CalendarError> {
    match got.status {
        200..=299 => Ok(got),
        300..=399 => Err(CalendarError::Redirected),
        401 | 403 => Err(CalendarError::SignIn),
        status => Err(CalendarError::Status(status)),
    }
}

/// What a fetch found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fetch {
    /// The provider said nothing had changed, so nothing was asked for.
    pub unchanged: bool,
    pub changed: usize,
    pub removed: usize,
}

const PROPFIND: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:CS="http://calendarserver.org/ns/"><D:prop><D:displayname/><CS:getctag/><D:resourcetype/></D:prop></D:propfind>"#;

/// Asks for the tag of every event that touches the window, which is what
/// keeps a calendar of many years from being listed whole.
fn query(from: &str, to: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/></D:prop><C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VEVENT"><C:time-range start="{from}" end="{to}"/></C:comp-filter></C:comp-filter></C:filter></C:calendar-query>"#
    )
}

fn multiget(hrefs: &[String]) -> String {
    let mut body = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<C:calendar-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data/></D:prop>"#,
    );
    for href in hrefs {
        body += "<D:href>";
        for c in href.chars() {
            match c {
                '&' => body += "&amp;",
                '<' => body += "&lt;",
                '>' => body += "&gt;",
                c => body.push(c),
            }
        }
        body += "</D:href>";
    }
    body + "</C:calendar-multiget>"
}

/// `YYYYMMDDT000000Z` for a day number, as a time range is written.
fn stamp(day: i64) -> String {
    let (year, month, dom) = tuimex_core::calendar::civil_from_days(day);
    format!("{year:04}{month:02}{dom:02}T000000Z")
}

/// Brings what the store holds of one CalDAV calendar in step with the
/// server: asks whether anything changed, lists the tags of the events in
/// the window from `from_day` to `to_day`, fetches those that are new or
/// changed and forgets those that are gone. An event is written over only
/// with what the server sent for that same address.
pub fn fetch_caldav(
    wire: &dyn Wire,
    store: &Calendars,
    account: &str,
    url: &str,
    sign_in: (&str, &Secret),
    (from_day, to_day): (i64, i64),
    now: i64,
) -> Result<Fetch, CalendarError> {
    origin(url)?;
    let read = |body: &str| multistatus(body).map_err(|e| CalendarError::Answer(e.to_string()));
    let ask = |method, depth, body: &str| {
        let sent = Sent {
            method,
            url,
            depth: Some(depth),
            body: Some(Body::xml(body)),
            only_if: None,
            sign_in: Some(sign_in),
        };
        wire.send(&sent).and_then(ok)
    };

    let about = read(&ask("PROPFIND", 0, PROPFIND)?.body)?;
    let collection: DavItem = about.into_iter().next().unwrap_or_default();
    let (name, tag) = (collection.name.as_deref(), collection.ctag.as_deref());
    if tag.is_some() && tag == store.tag(account, url)?.as_deref() {
        store.read(account, url, name, tag, now)?;
        return Ok(Fetch { unchanged: true, ..Fetch::default() });
    }

    let listed = read(&ask("REPORT", 1, &query(&stamp(from_day), &stamp(to_day)))?.body)?;
    let held = store.tags(account, url)?;
    let mut wanted = Vec::new();
    for item in listed.iter().filter(|item| !item.gone && within(url, &item.href).is_some()) {
        let known = held.iter().find(|(href, _)| *href == item.href);
        if known.is_none_or(|(_, etag)| etag.is_none() || *etag != item.etag) {
            wanted.push(item.href.clone());
        }
    }
    let mut fetch = Fetch::default();
    for (href, _) in &held {
        if !listed.iter().any(|item| item.href == *href && !item.gone) {
            store.drop_one(account, url, href)?;
            fetch.removed += 1;
        }
    }
    for batch in wanted.chunks(BATCH) {
        for item in read(&ask("REPORT", 1, &multiget(batch))?.body)? {
            // Only what was asked for is kept: an answer cannot add an
            // event under an address nobody listed.
            if let (true, Some(text)) = (batch.contains(&item.href), &item.calendar) {
                store.put(account, url, &item.href, item.etag.as_deref(), text)?;
                fetch.changed += 1;
            }
        }
    }
    store.read(account, url, name, tag, now)?;
    Ok(fetch)
}

/// Fetches a feed whole and keeps it in place of the last one. `address`
/// is the secret itself; no error says more of it than that it was asked.
pub fn fetch_feed(
    wire: &dyn Wire,
    store: &Calendars,
    account: &str,
    address: &Secret,
    now: i64,
) -> Result<Fetch, CalendarError> {
    let body = get_feed(wire, address)?;
    let text = body.trim_start_matches('\u{feff}');
    let unchanged =
        store.held()?.iter().any(|h| h.account == account && h.calendar.as_deref() == Some(FEED) && h.text == text);
    if !unchanged {
        store.put(account, FEED, "", None, text)?;
    }
    store.read(account, FEED, None, None, now)?;
    Ok(Fetch { unchanged, changed: usize::from(!unchanged), removed: 0 })
}

/// A feed's text, once it is known to be a calendar.
fn get_feed(wire: &dyn Wire, address: &Secret) -> Result<String, CalendarError> {
    let url = address.expose();
    origin(url)?;
    let got = ok(wire.send(&Sent { method: "GET", url, depth: None, body: None, only_if: None, sign_in: None })?)?;
    // Some servers begin a file with a byte-order mark, which is no part
    // of the calendar.
    let text = got.body.trim_start_matches('\u{feff}');
    // A sign-in page or an error page answers 200 too.
    if !text.trim_start().get(..15).is_some_and(|start| start.eq_ignore_ascii_case("BEGIN:VCALENDAR")) {
        return Err(CalendarError::NotACalendar);
    }
    Ok(got.body)
}

/// Whether `address` serves a calendar: one request, and nothing kept.
pub fn check_feed(wire: &dyn Wire, address: &Secret) -> Result<(), CalendarError> {
    get_feed(wire, address).map(drop)
}

/// Whether the CalDAV calendar at `url` takes the sign-in: one `PROPFIND`
/// of the calendar alone, and nothing kept. The name it gives, if any.
pub fn check_caldav(wire: &dyn Wire, url: &str, sign_in: (&str, &Secret)) -> Result<Option<String>, CalendarError> {
    origin(url)?;
    let sent = Sent {
        method: "PROPFIND",
        url,
        depth: Some(0),
        body: Some(Body::xml(PROPFIND)),
        only_if: None,
        sign_in: Some(sign_in),
    };
    let got = ok(wire.send(&sent)?)?;
    let about = multistatus(&got.body).map_err(|e| CalendarError::Answer(e.to_string()))?;
    Ok(about.into_iter().next().and_then(|item| item.name))
}

/// `url` as the address of a collection, ending in `/`.
fn collection(url: &str) -> String {
    if url.ends_with('/') { url.to_string() } else { format!("{url}/") }
}

/// `text` with each `%XX` read as the byte it stands for, so that two
/// spellings of one address compare equal. Used to compare, never sent.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The URL of `href` when it names one thing inside the calendar at
/// `url`, on the calendar's own server: no other host, no parent, no
/// query, and not the calendar itself.
fn under(url: &str, href: &str) -> Option<String> {
    let full = within(url, href)?;
    let base = decoded(&collection(url));
    let rest = decoded(&full).strip_prefix(base.as_str())?.to_string();
    let safe = !rest.is_empty()
        && !rest.contains(['?', '#', '\\'])
        && rest.split('/').all(|segment| !segment.is_empty() && segment != "." && segment != "..");
    safe.then_some(full)
}

/// A name tuimex made for an event, which is all that is ever put in an
/// address: an event's UID is text from outside.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// The UID of an event made here, from the system's randomness. What is
/// before its `@` is also the name such an event is written under.
pub fn new_uid() -> Result<String, CalendarError> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| CalendarError::Random)?;
    Ok(tuimex_content::new_uid(random))
}

/// The tag the store holds for `href` of the calendar at `url`. A write is
/// conditional on it, so with none there is no write.
fn held_tag(store: &Calendars, account: &str, url: &str, href: &str) -> Result<String, CalendarError> {
    let held = store.tags(account, url)?;
    held.into_iter().find(|(h, _)| h == href).and_then(|(_, etag)| etag).ok_or(CalendarError::Unread)
}

fn write(
    wire: &dyn Wire,
    method: &'static str,
    url: &str,
    body: Option<Body<'_>>,
    only_if: Only<'_>,
    sign_in: (&str, &Secret),
) -> Result<Got, CalendarError> {
    wire.send(&Sent { method, url, depth: None, body, only_if: Some(only_if), sign_in: Some(sign_in) })
}

/// Writes a new event, `text`, into the CalDAV calendar at `url` as
/// `<url><name>.ics`, only if nothing is there yet, and keeps it in the
/// store under the address it returns. `name` is a key tuimex made.
pub fn put_new(
    wire: &dyn Wire,
    store: &Calendars,
    account: &str,
    url: &str,
    sign_in: (&str, &Secret),
    text: &str,
    name: &str,
) -> Result<String, CalendarError> {
    let origin = origin(url)?;
    if !valid_name(name) {
        return Err(CalendarError::Name);
    }
    let target = format!("{}{name}.ics", collection(url));
    let href = target[origin.len()..].to_string();
    if under(url, &href).is_none() {
        return Err(CalendarError::Elsewhere);
    }
    let got = write(wire, "PUT", &target, Some(Body::calendar(text)), Only::IfNoneMatch("*"), sign_in)?;
    if got.status == 412 {
        return Err(CalendarError::Exists);
    }
    let got = ok(got)?;
    store.put(account, url, &href, got.etag.as_deref(), text)?;
    Ok(href)
}

/// Writes `text` over the event at `href` of the calendar at `url`, only
/// if the provider still holds what tuimex last read of it.
pub fn put_changed(
    wire: &dyn Wire,
    store: &Calendars,
    account: &str,
    url: &str,
    sign_in: (&str, &Secret),
    href: &str,
    text: &str,
) -> Result<(), CalendarError> {
    origin(url)?;
    let target = under(url, href).ok_or(CalendarError::Elsewhere)?;
    let etag = held_tag(store, account, url, href)?;
    let got = write(wire, "PUT", &target, Some(Body::calendar(text)), Only::IfMatch(&etag), sign_in)?;
    if got.status == 412 {
        return Err(CalendarError::Changed);
    }
    let got = ok(got)?;
    store.put(account, url, href, got.etag.as_deref(), text)?;
    Ok(())
}

/// Removes the event at `href` from the calendar at `url`, only if the
/// provider still holds what tuimex last read of it. One already gone is
/// as good as removed.
pub fn delete(
    wire: &dyn Wire,
    store: &Calendars,
    account: &str,
    url: &str,
    sign_in: (&str, &Secret),
    href: &str,
) -> Result<(), CalendarError> {
    origin(url)?;
    let target = under(url, href).ok_or(CalendarError::Elsewhere)?;
    let etag = held_tag(store, account, url, href)?;
    let got = write(wire, "DELETE", &target, None, Only::IfMatch(&etag), sign_in)?;
    match got.status {
        412 => return Err(CalendarError::Changed),
        404 => {}
        _ => {
            ok(got)?;
        }
    }
    store.drop_one(account, url, href)?;
    Ok(())
}

/// The host of an address tuimex may fetch, which is all of a feed's
/// address that is ever shown (ADR-056).
pub fn host(url: &str) -> Option<&str> {
    origin(url).ok()?.split_once("://").map(|(_, host)| host)
}

/// The keyring entry of `account`'s feed address.
pub fn feed_entry(account: &str) -> String {
    format!("{account}/calendar-feed")
}

/// The address of `account`'s feed: what `address_cmd` prints, or else
/// the keyring's entry, found the way an account's password is.
pub fn feed_address(
    account: &str,
    address_cmd: Option<&[String]>,
    keyring: &dyn Keyring,
) -> Result<Secret, CalendarError> {
    let whose = format!("address_cmd for {account}'s calendar");
    resolve(&feed_entry(account), address_cmd, keyring).map_err(|error| {
        CalendarError::Secret(match error {
            SecretError::NoProgram { .. } => format!("{whose} names no program"),
            SecretError::Spawn { program, source, .. } => format!("{whose} could not run {program}: {source}"),
            SecretError::Failed { program, status, .. } => format!("{whose} failed: {program} ended with {status}"),
            SecretError::Empty { .. } => format!("{whose} printed no address"),
            SecretError::Keyring { reason, .. } => {
                format!("the keyring could not be read for {account}'s calendar: {reason}")
            }
            SecretError::Missing { .. } => format!(
                "there is no address for {account}'s calendar in the keyring, and no address_cmd; run `tuimex setup`"
            ),
        })
    })
}

/// How a configured calendar is reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Signed in to as the account's mail is: as `user`, with the password
    /// its `password_cmd` prints or the keyring holds.
    CalDav { url: String, user: String, password_cmd: Option<Vec<String>> },
    /// A private address, printed by `address_cmd` or kept in the keyring
    /// under `feed_entry`.
    Feed { address_cmd: Option<Vec<String>> },
}

/// One `[[calendar]]` of the config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configured {
    pub account: String,
    pub source: Source,
}

impl Configured {
    /// `caldav` or `feed`, as the config writes it.
    pub fn kind(&self) -> &'static str {
        match self.source {
            Source::CalDav { .. } => "caldav",
            Source::Feed { .. } => "feed",
        }
    }

    /// What the store files the calendar under: a CalDAV calendar's
    /// address, which is no secret, and never a feed's.
    fn key(&self) -> &str {
        match &self.source {
            Source::CalDav { url, .. } => url,
            Source::Feed { .. } => FEED,
        }
    }
}

/// What one calendar's part of a pass came to.
#[derive(Debug)]
pub struct Outcome {
    pub account: String,
    pub kind: &'static str,
    /// The host asked, where it is known: a feed's is known only once its
    /// address has been found.
    pub host: Option<String>,
    pub read: Result<Fetch, CalendarError>,
    /// When the calendar was last read whole, this pass included.
    pub read_at: Option<i64>,
}

/// The days either side of today that a CalDAV calendar is asked for.
const BEFORE: i64 = 90;
const AFTER: i64 = 730;
const DAY: i64 = 86_400;

fn fetch_one(
    wire: &dyn Wire,
    store: &Calendars,
    keyring: &dyn Keyring,
    calendar: &Configured,
    now: i64,
) -> (Option<String>, Result<Fetch, CalendarError>) {
    let account = calendar.account.as_str();
    match &calendar.source {
        Source::CalDav { url, user, password_cmd } => {
            let host = host(url).map(str::to_string);
            let password = match resolve(account, password_cmd.as_deref(), keyring) {
                Ok(password) => password,
                Err(error) => return (host, Err(CalendarError::Secret(error.to_string()))),
            };
            let today = now.div_euclid(DAY);
            (host, fetch_caldav(wire, store, account, url, (user, &password), (today - BEFORE, today + AFTER), now))
        }
        Source::Feed { address_cmd } => {
            let address = match feed_address(account, address_cmd.as_deref(), keyring) {
                Ok(address) => address,
                Err(error) => return (None, Err(error)),
            };
            (host(address.expose()).map(str::to_string), fetch_feed(wire, store, account, &address, now))
        }
    }
}

/// Fetches each calendar once, in order; one that fails does not stop
/// the rest.
pub fn pass(
    wire: &dyn Wire,
    store: &Calendars,
    keyring: &dyn Keyring,
    calendars: &[Configured],
    now: i64,
) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    for calendar in calendars {
        let (host, read) = fetch_one(wire, store, keyring, calendar, now);
        // A store that cannot say when it last read a calendar is said as
        // one never read: whether the store works is what `read` says.
        let fetched = store.fetched().unwrap_or_default();
        let read_at =
            fetched.iter().find(|f| f.account == calendar.account && f.calendar == calendar.key()).map(|f| f.read_at);
        outcomes.push(Outcome { account: calendar.account.clone(), kind: calendar.kind(), host, read, read_at });
    }
    outcomes
}

/// How often the calendar thread reads every calendar unasked.
pub const EVERY: Duration = Duration::from_secs(15 * 60);

/// What the calendar thread needs. It owns its own handle on the store,
/// for writing; anyone else reads through one of their own.
pub struct Runner {
    pub calendars: Vec<Configured>,
    pub wire: Box<dyn Wire + Send>,
    pub keyring: Box<dyn Keyring>,
    pub data: PathBuf,
    pub cache: PathBuf,
    /// `EVERY`, except in a test.
    pub every: Duration,
}

/// Starts the calendar thread beside the mail's (ADR-006). It reads every
/// calendar at once, then every `every` and whenever `nudges` delivers,
/// and says how each went on `events`. It ends when `nudges` has no
/// sender left, or nobody listens on `events`.
pub fn start(runner: Runner, nudges: Receiver<()>, events: Sender<Event>) -> JoinHandle<()> {
    thread::spawn(move || runner.run(&nudges, &events))
}

fn now() -> i64 {
    // A clock set before 1970 is read as 1970: the times it gives are
    // shown, and decide nothing.
    let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
}

impl Runner {
    fn run(&self, nudges: &Receiver<()>, events: &Sender<Event>) {
        let mut store = None;
        while self.once(&mut store, events) {
            match nudges.recv_timeout(self.every) {
                // Nudges that came while a pass ran are all answered by
                // the next one.
                Ok(()) => nudges.try_iter().for_each(drop),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// One pass, said on `events`; false once nobody listens.
    fn once(&self, store: &mut Option<Calendars>, events: &Sender<Event>) -> bool {
        if store.is_none() {
            match Calendars::open(&self.data, &self.cache) {
                Ok(opened) => *store = Some(opened),
                Err(error) => {
                    let reason = CalendarError::from(error).to_string();
                    return self.calendars.iter().all(|calendar| {
                        let account = AccountId(calendar.account.clone());
                        events.send(Event::CalendarFailed { account, reason: reason.clone(), read_at: None }).is_ok()
                    });
                }
            }
        }
        let Some(store) = store.as_ref() else { return true };
        for outcome in pass(self.wire.as_ref(), store, self.keyring.as_ref(), &self.calendars, now()) {
            let account = AccountId(outcome.account);
            let event = match outcome.read {
                Ok(fetch) => Event::CalendarRead { account, changed: fetch.changed + fetch.removed > 0 },
                Err(error) => Event::CalendarFailed {
                    account,
                    reason: error.to_string(),
                    read_at: outcome.read_at.map(Timestamp),
                },
            };
            if events.send(event).is_err() {
                return false;
            }
        }
        true
    }
}

/// Something held of a calendar, parsed, with what the views say beside
/// each of its events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub account: String,
    /// Only here, in a file of tuimex's own (ADR-055): drawn `local`.
    pub local: bool,
    /// The calendar's name, where its provider gave one.
    pub name: Option<String>,
    /// When its calendar was last read whole; None for a local event.
    pub read_at: Option<i64>,
    pub calendar: tuimex_core::calendar::Calendar,
}

/// Everything held, fetched and local, read through a handle of the
/// caller's own: SQLite lets it read while the calendar thread writes.
pub fn kept(data: &Path, cache: &Path) -> Result<Vec<Kept>, CalendarError> {
    let store = Calendars::open(data, cache)?;
    let fetched = store.fetched()?;
    let kept = store.held()?.into_iter().map(|held| {
        let about = held
            .calendar
            .as_ref()
            .and_then(|calendar| fetched.iter().find(|f| f.account == held.account && f.calendar == *calendar));
        Kept {
            local: held.calendar.is_none(),
            name: about.and_then(|f| f.name.clone()),
            read_at: about.map(|f| f.read_at),
            calendar: tuimex_content::calendar(&held.text),
            account: held.account,
        }
    });
    Ok(kept.collect())
}

/// The wire itself: `ureq` over TLS, told to follow no redirect and to
/// read no more than `LARGEST`.
pub struct Https {
    agent: ureq::Agent,
}

impl Https {
    pub fn new(patience: std::time::Duration) -> Https {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .allow_non_standard_methods(true)
            .timeout_global(Some(patience))
            .build();
        Https { agent: config.new_agent() }
    }
}

impl Wire for Https {
    fn send(&self, sent: &Sent<'_>) -> Result<Got, CalendarError> {
        use base64::Engine as _;
        origin(sent.url)?;
        // The transport's own words can hold the address, which for a
        // feed is the secret, so an error is reduced to its kind.
        let unreachable = |error: ureq::Error| {
            CalendarError::Unreachable(
                match error {
                    ureq::Error::Timeout(_) => "it did not answer in time",
                    ureq::Error::HostNotFound => "its host was not found",
                    ureq::Error::ConnectionFailed | ureq::Error::Io(_) => "the connection failed",
                    ureq::Error::BodyExceedsLimit(_) => "its answer was too large",
                    ureq::Error::Tls(_) | ureq::Error::Rustls(_) => "its certificate was not accepted",
                    _ => "the request failed",
                }
                .to_string(),
            )
        };
        let mut request = ureq::http::Request::builder().method(sent.method).uri(sent.url);
        if let Some(depth) = sent.depth {
            request = request.header("Depth", depth.to_string());
        }
        if let Some(body) = sent.body {
            request = request.header("Content-Type", body.content_type);
        }
        match sent.only_if {
            Some(Only::IfMatch(tag)) => request = request.header("If-Match", tag),
            Some(Only::IfNoneMatch(tag)) => request = request.header("If-None-Match", tag),
            None => {}
        }
        if let Some((user, password)) = sent.sign_in {
            let pair = zeroize::Zeroizing::new(format!("{user}:{}", password.expose()));
            let encoded = zeroize::Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(pair.as_bytes()));
            request = request.header("Authorization", format!("Basic {}", encoded.as_str()));
        }
        let request = request
            .body(sent.body.map_or("", |body| body.text))
            .map_err(|_| CalendarError::Unreachable("the address is not one".to_string()))?;
        let mut response = self.agent.run(request).map_err(unreachable)?;
        let status = response.status().as_u16();
        // A tag that is not text cannot be sent back, so it is taken as
        // none given: the next fetch asks for the event again.
        let etag = response.headers().get("ETag").and_then(|v| v.to_str().ok()).map(str::to_string);
        let body = response.body_mut().with_config().limit(LARGEST).read_to_string().map_err(unreachable)?;
        Ok(Got { status, body, etag })
    }
}

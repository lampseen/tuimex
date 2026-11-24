//! Fetching calendars (ADR-056, ADR-062): Google's over CalDAV with the
//! account's own password, any other from a private address that serves
//! iCalendar. What comes back is kept as text in the store's cache; only
//! `tuimex-content` reads it.
//!
//! Every request goes to an address the user configured, over TLS, with no
//! redirect followed. Nothing a calendar names is fetched.

use tuimex_content::{DavItem, multistatus};
use tuimex_store::calendar::Calendars;

use crate::Secret;

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
    #[error("the calendar could not be kept: {0}")]
    Store(String),
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
    pub body: Option<&'a str>,
    pub sign_in: Option<(&'a str, &'a Secret)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Got {
    pub status: u16,
    pub body: String,
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
        wire.send(&Sent { method, url, depth: Some(depth), body: Some(body), sign_in: Some(sign_in) }).and_then(ok)
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
    let url = address.expose();
    origin(url)?;
    let got = ok(wire.send(&Sent { method: "GET", url, depth: None, body: None, sign_in: None })?)?;
    // Some servers begin a file with a byte-order mark, which is no part
    // of the calendar.
    let text = got.body.trim_start_matches('\u{feff}');
    // A sign-in page or an error page answers 200 too.
    if !text.trim_start().get(..15).is_some_and(|start| start.eq_ignore_ascii_case("BEGIN:VCALENDAR")) {
        return Err(CalendarError::NotACalendar);
    }
    let unchanged =
        store.held()?.iter().any(|h| h.account == account && h.calendar.as_deref() == Some(FEED) && h.text == text);
    if !unchanged {
        store.put(account, FEED, "", None, text)?;
    }
    store.read(account, FEED, None, None, now)?;
    Ok(Fetch { unchanged, changed: usize::from(!unchanged), removed: 0 })
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
        if sent.body.is_some() {
            request = request.header("Content-Type", "application/xml; charset=utf-8");
        }
        if let Some((user, password)) = sent.sign_in {
            let pair = zeroize::Zeroizing::new(format!("{user}:{}", password.expose()));
            let encoded = zeroize::Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(pair.as_bytes()));
            request = request.header("Authorization", format!("Basic {}", encoded.as_str()));
        }
        let request = request
            .body(sent.body.unwrap_or_default())
            .map_err(|_| CalendarError::Unreachable("the address is not one".to_string()))?;
        let mut response = self.agent.run(request).map_err(unreachable)?;
        let status = response.status().as_u16();
        let body = response.body_mut().with_config().limit(LARGEST).read_to_string().map_err(unreachable)?;
        Ok(Got { status, body })
    }
}

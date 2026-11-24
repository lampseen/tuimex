//! Every calendar ask carried out against scratch stores: the events in
//! files under a scratch data directory, a provider that is a table in
//! memory behind the `Wire`, and the mail through a real engine over the
//! fixtures' server. Nothing here leaves the process.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tuimex_core::calendar::{Freq, Repeat, Rule, Said};
use tuimex_core::{AccountId, Answer, Capabilities, Event as Told, Flags, Identity, RemoteWrite, Timestamp};
use tuimex_engine::calendar::{Configured, Got, Only, Runner, Sent, Wire};
use tuimex_engine::sync::Policy;
use tuimex_engine::{Account, Connect, Engine, Error, Keyring, Letter, Secret};
use tuimex_fixtures::FakeServer;
use tuimex_store::Store;
use tuimex_store::calendar::Held;
use tuimex_ui::calendar::Calendar;

use super::*;
use crate::calendar::{About, Handed};

pub(crate) const ME: &str = "ada@example.org";
pub(crate) const URL: &str = "https://dav.example.org/cal/ada/";
const PASSWORD: &str = "synthetic-password";
const CAPABILITIES: Capabilities =
    Capabilities { uidplus: true, r#move: true, condstore: true, special_use: true, idle: true, gmail: false };
/// Long enough that only a hang reaches it.
const PATIENCE: Duration = Duration::from_secs(20);

struct Door(FakeServer);

impl Connect for Door {
    fn connect(&self) -> Result<Box<dyn RemoteWrite>, Error> {
        Ok(Box::new(self.0.connect(PASSWORD)?))
    }

    fn send(&self, _: &Letter) -> Result<(), Error> {
        Ok(())
    }
}

/// Holds the account's password and nothing else. No test opens the real
/// keyring.
struct Holds;

impl Keyring for Holds {
    fn password(&self, _: &str) -> Result<Option<Secret>, String> {
        Ok(Some(Secret::new(PASSWORD.into())))
    }

    fn set(&self, _: &str, _: &Secret) -> Result<(), String> {
        Err("this keyring keeps nothing".into())
    }
}

/// A provider in memory: what each address holds, at which version, and
/// every request it was sent. It keeps the preconditions a CalDAV server
/// keeps, so a write on a stale tag is refused as one would refuse it.
#[derive(Default)]
pub(crate) struct Dav {
    held: Mutex<BTreeMap<String, (u32, String)>>,
    asked: Mutex<Vec<String>>,
}

struct Line(Arc<Dav>);

fn tag(version: u32) -> String {
    format!("\"v{version}\"")
}

impl Wire for Line {
    fn send(&self, sent: &Sent<'_>) -> Result<Got, CalendarError> {
        let only = match sent.only_if {
            Some(Only::IfMatch(tag)) => format!(" If-Match: {tag}"),
            Some(Only::IfNoneMatch(tag)) => format!(" If-None-Match: {tag}"),
            None => String::new(),
        };
        self.0.asked.lock().unwrap().push(format!("{} {}{only}", sent.method, sent.url));
        let answer = |status: u16, etag: Option<String>| Ok(Got { status, body: String::new(), etag });
        if !sent.sign_in.is_some_and(|(user, password)| user == ME && password.expose() == PASSWORD) {
            return answer(401, None);
        }
        let mut held = self.0.held.lock().unwrap();
        let current = held.get(sent.url).map(|(version, _)| *version);
        let allowed = match sent.only_if {
            Some(Only::IfMatch(wanted)) => current.map(tag).as_deref() == Some(wanted),
            Some(Only::IfNoneMatch(_)) => current.is_none(),
            None => true,
        };
        match (sent.method, allowed) {
            ("DELETE", false) if current.is_none() => answer(404, None),
            (_, false) => answer(412, None),
            ("PUT", true) => {
                let version = current.map_or(1, |version| version + 1);
                held.insert(sent.url.to_string(), (version, sent.body.map_or("", |body| body.text).to_string()));
                answer(if current.is_some() { 204 } else { 201 }, Some(tag(version)))
            }
            ("DELETE", true) => {
                held.remove(sent.url);
                answer(204, None)
            }
            _ => answer(405, None),
        }
    }
}

impl Dav {
    /// The requests sent since this was last asked.
    pub fn asked(&self) -> Vec<String> {
        std::mem::take(&mut self.asked.lock().unwrap())
    }

    /// What every address holds, by address.
    pub fn texts(&self) -> Vec<(String, String)> {
        self.held.lock().unwrap().iter().map(|(url, (_, text))| (url.clone(), text.clone())).collect()
    }

    /// Somebody else changes what `name` holds, behind tuimex's back.
    fn touch(&self, name: &str) {
        let mut held = self.held.lock().unwrap();
        held.get_mut(&format!("{URL}{name}")).unwrap().0 += 1;
    }
}

/// Which calendar the account `work` has in the config.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    None,
    Feed,
    CalDav { write: bool },
}

/// One account, `work`, with its mail on a server in memory, its
/// calendars in scratch stores, and the reader that carries its asks out.
pub(crate) struct Rig {
    pub reader: Reader,
    pub engines: Engines,
    pub store: StoreHandle,
    arriving: Receiver<Handed>,
    pub events: Receiver<Told>,
    pub server: FakeServer,
    pub dav: Arc<Dav>,
    pub root: PathBuf,
    pub serial: u64,
}

pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("carry-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

/// A calendar part as a provider writes one: `ada` asked by `lars`, with
/// a line tuimex does not read.
pub(crate) fn invitation(uid: &str, sequence: u32, method: Option<&str>, extra: &str) -> String {
    let method = method.map_or(String::new(), |method| format!("METHOD:{method}\r\n"));
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Example//EN\r\n{method}BEGIN:VEVENT\r\nUID:{uid}\r\n\
         SEQUENCE:{sequence}\r\nDTSTAMP:20261120T090000Z\r\nDTSTART:20261126T100000Z\r\nDTEND:20261126T110000Z\r\n\
         SUMMARY:Planning\r\nX-PROVIDER-OWN:kept as written\r\nORGANIZER;CN=Lars Holm:mailto:lars@example.com\r\n\
         ATTENDEE;CN=Ada;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:ada@example.org\r\n\
         ATTENDEE;PARTSTAT=ACCEPTED:mailto:mei@example.net\r\n{extra}END:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

/// A message carrying `ics`, the `n`th of a test and so the `n`th by its
/// date.
pub(crate) fn mail(n: u32, from: &str, method: &str, ics: &str) -> Vec<u8> {
    format!(
        "From: {from}\r\nTo: {ME}\r\nSubject: Invitation {n}\r\nMessage-ID: <inv{n}@example.com>\r\n\
         Date: Mon, 23 Nov 2026 10:{n:02}:00 +0000\r\nMIME-Version: 1.0\r\n\
         Content-Type: text/calendar; method={method}; charset=utf-8\r\n\r\n{ics}"
    )
    .into_bytes()
}

pub(crate) fn id(local: bool, uid: &str) -> EventId {
    EventId { account: AccountId("work".into()), local, uid: uid.into(), occurrence: None }
}

fn answer(local: bool, uid: &str, answer: Answer) -> Action {
    Action::AnswerEvent(Box::new(EventAnswer { event: id(local, uid), answer, message: None }))
}

fn person(email: &str) -> Address {
    Address { name: None, email: email.into() }
}

/// An event as the form hands one over: no UID, an hour on 26 November.
fn made(summary: &str) -> Event {
    Event {
        uid: String::new(),
        sequence: 0,
        instead_of: None,
        summary: summary.into(),
        start: Moment::Utc(1_795_687_200),
        end: Some(Moment::Utc(1_795_690_800)),
        location: None,
        description: None,
        organizer: None,
        guests: Vec::new(),
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

fn save(local: bool, event: Event, replaces: Option<EventId>, invite: bool) -> Action {
    Action::SaveEvent(Box::new(Stored { account: AccountId("work".into()), local, event, replaces, invite }))
}

/// A reader for the account `work` over the stores under `root`, with the
/// provider its writes reach. Its own thread, were it started, would
/// find nothing to fetch from.
pub(crate) fn reader(root: &Path, kind: Kind, shown: mpsc::Sender<Handed>) -> (Reader, Arc<Dav>) {
    let source = match kind {
        Kind::None => None,
        Kind::Feed => Some(Source::Feed { address_cmd: None }),
        Kind::CalDav { .. } => Some(Source::CalDav { url: URL.into(), user: ME.into(), password_cmd: None }),
    };
    let calendars = source.map(|source| Configured { account: "work".into(), source }).into_iter().collect();
    let runner = Runner {
        calendars,
        wire: Box::new(Line(Arc::default())),
        keyring: Box::new(Holds),
        data: root.join("data"),
        cache: root.join("cache"),
        every: Duration::from_secs(3_600),
    };
    let identities = [ME, "ada@work.example.com"].map(|email| Identity {
        account: AccountId("work".into()),
        address: Address { name: Some("Ada".into()), email: email.into() },
    });
    let about = About { identities: identities.to_vec(), zone: Zone::fixed(3_600), zone_dirs: Vec::new() };
    let dav = Arc::<Dav>::default();
    let writable = if kind == (Kind::CalDav { write: true }) { vec!["work".to_string()] } else { Vec::new() };
    let reader = Reader::new(runner, about, shown).writing(writable, Box::new(Line(dav.clone())), Box::new(Holds));
    (reader, dav)
}

impl Rig {
    pub fn new(name: &str, kind: Kind) -> Rig {
        Rig::over(scratch(name), kind, &[ME])
    }

    /// Over `root`, which may hold what an earlier rig left. `sends_as`
    /// are the addresses the engine lets mail leave from.
    pub fn over(root: PathBuf, kind: Kind, sends_as: &[&str]) -> Rig {
        let (data, cache) = (root.join("data"), root.join("cache"));
        let server = FakeServer::new(PASSWORD, CAPABILITIES);
        server.add_folder("INBOX", &[], 7);
        let account = Account {
            name: "work".into(),
            roles: Vec::new(),
            policy: Policy::default(),
            identities: sends_as.iter().map(|address| (*address).to_string()).collect(),
            send_delay: Duration::from_secs(3_600),
        };
        let store = StoreHandle::start(Store::open(&data, &cache, tuimex_engine::parse).unwrap());
        let (told, events) = mpsc::channel();
        let engine = Engine::start(account, Box::new(Door(server.clone())), store.clone(), told);
        let (shown, arriving) = mpsc::channel();
        let (reader, dav) = reader(&root, kind, shown);
        Rig { reader, engines: Engines::new(vec![engine]), store, arriving, events, server, dav, root, serial: 0 }
    }

    pub fn calendars(&self) -> Calendars {
        Calendars::open(&self.root.join("data"), &self.root.join("cache")).unwrap()
    }

    /// Carries out an ask as the worker does, the calendar handed over
    /// after it.
    pub fn ask(&mut self, action: Action) -> Result<Option<MessageId>, Refusal> {
        self.serial += 1;
        let ask = Ask { serial: self.serial, action, on: Vec::new() };
        let post = Post { engines: &self.engines, store: &self.store };
        self.reader.carried = self.serial;
        let carried = self.reader.carry(&ask, &post);
        self.reader.refresh(&post).1.unwrap();
        carried
    }

    /// Everything handed over since this was last asked.
    pub fn handed(&self) -> Vec<Handed> {
        self.arriving.try_iter().collect()
    }

    /// The newest calendar handed over, and the serial it was read after.
    pub fn shown(&self) -> (Calendar, u64) {
        let mut handed = self.handed().into_iter().filter_map(|handed| match handed {
            Handed::Calendar { calendar, carried } => Some((*calendar, carried)),
            Handed::Invited(..) => None,
        });
        handed.next_back().expect("a calendar was handed over")
    }

    /// What `work`'s local calendar holds, as text.
    pub fn files(&self) -> Vec<Held> {
        self.calendars().held().unwrap().into_iter().filter(|held| held.calendar.is_none()).collect()
    }

    /// The folder a message is in, or `None` once it has gone.
    pub fn folder_of(&self, id: &MessageId) -> Option<String> {
        let id = id.clone();
        self.store.run(move |s| Ok(s.stored(&id)?.0)).ok()
    }

    /// The calendar part of a message waiting in the mail store.
    pub fn part_of(&self, id: &MessageId) -> (String, String) {
        let asked = id.clone();
        let raw = self.store.run(move |s| s.read(&asked)).unwrap();
        let head = String::from_utf8_lossy(&raw).into_owned();
        (head, content::calendar_part(&raw).unwrap().expect("the message carries a calendar").text)
    }

    /// Puts `text` at the provider under `name`, read by tuimex as it
    /// stands.
    pub fn provide(&self, name: &str, text: &str) {
        self.dav.held.lock().unwrap().insert(format!("{URL}{name}"), (1, text.to_string()));
        self.calendars().put("work", URL, &format!("/cal/ada/{name}"), Some(&tag(1)), text).unwrap();
    }

    /// Delivers `messages` to the Inbox and waits until the store holds
    /// `held` invitations.
    pub fn deliver(&self, messages: &[Vec<u8>], held: usize) {
        for raw in messages {
            self.server.deliver("INBOX", raw, Flags::default(), Timestamp(1_795_435_200));
        }
        let post = Post { engines: &self.engines, store: &self.store };
        let deadline = Instant::now() + PATIENCE;
        while post.invitations().len() != held {
            assert!(Instant::now() < deadline, "the invitations never arrived");
            self.engines.act(Ask { serial: 0, action: Action::Sync { all: true }, on: Vec::new() });
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// How the guest `ada` stands in the first event of `text`.
fn said(text: &str) -> Said {
    let parsed = content::calendar(text);
    parsed.events[0].guests.iter().find(|guest| guest.who.email == ME).map_or(Said::Nothing, |guest| guest.said)
}

#[test]
fn an_answer_to_an_event_of_a_feed_is_a_local_copy_and_a_reply_and_u_takes_both_back() {
    let mut rig = Rig::new("feed", Kind::Feed);
    let feed = invitation("planning@example.com", 0, None, "");
    rig.calendars().put("work", provider::FEED, "", None, &feed).unwrap();

    let mail = rig.ask(answer(false, "planning@example.com", Answer::Accept)).unwrap().expect("a reply is mailed");
    assert_eq!(rig.folder_of(&mail).as_deref(), Some("Outbox"), "it waits out the send delay");
    let (head, part) = rig.part_of(&mail);
    assert!(head.contains("text/calendar; method=REPLY"), "{head}");
    assert!(head.contains("lars@example.com") && !head.contains("mei@example.net"), "to the organizer alone: {head}");
    assert!(part.contains("PARTSTAT=ACCEPTED") && part.contains("mailto:ada@example.org"), "{part}");
    assert!(!part.contains("mei@example.net"), "the reply names the one who answers and nobody else");
    let files = rig.files();
    assert_eq!(files.len(), 1, "the feed is only read, so the answer is in a copy here (ADR-055)");
    assert_eq!(said(&files[0].text), Said::Accepted);
    assert!(rig.dav.asked().is_empty(), "nothing is sent to a provider that is only read");

    let (calendar, carried) = rig.shown();
    assert_eq!(carried, 1, "the calendar was read after the ask was carried out");
    let shelves: Vec<_> = calendar.shelves.iter().map(|shelf| (shelf.local, shelf.events.len())).collect();
    assert_eq!(shelves, [(false, 1)], "the copy and the feed's event are one event on screen");
    let me = calendar.shelves[0].events[0].guests.iter().find(|guest| guest.who.email == ME).unwrap();
    assert_eq!(me.said, Said::Accepted);

    assert_eq!(rig.ask(Action::UndoEvent(1)), Ok(Some(mail.clone())), "the row of its mail is read again");
    assert!(rig.files().is_empty(), "the copy is gone");
    assert_eq!(rig.folder_of(&mail), None, "the reply is out of the Outbox and out of Drafts");
    assert_eq!(rig.shown().0.shelves[0].events[0].guests[0].said, Said::Nothing);
    let again = rig.ask(Action::UndoEvent(1)).unwrap_err();
    assert_eq!(again.reason, "there is nothing of it left to take back");
}

#[test]
fn an_answer_in_a_written_calendar_changes_one_line_at_the_provider_and_u_puts_the_text_back() {
    let mut rig = Rig::new("caldav", Kind::CalDav { write: true });
    let text = invitation("planning@example.com", 0, None, "");
    rig.provide("abc.ics", &text);

    let mail = rig.ask(answer(false, "planning@example.com", Answer::Decline)).unwrap().unwrap();
    assert_eq!(rig.dav.asked(), [format!("PUT {URL}abc.ics If-Match: \"v1\"")]);
    let held = rig.dav.texts();
    assert!(held[0].1.contains("X-PROVIDER-OWN:kept as written"), "what the provider wrote is not lost");
    assert_eq!(said(&held[0].1), Said::Declined);
    assert_eq!(held[0].1.len(), text.len() + "DECLINED".len() - "NEEDS-ACTION".len(), "one value changed");
    assert!(rig.files().is_empty(), "nothing is kept locally of an event the provider holds the answer to");
    assert!(rig.part_of(&mail).1.contains("PARTSTAT=DECLINED"));

    assert_eq!(rig.ask(Action::UndoEvent(1)), Ok(Some(mail.clone())));
    assert_eq!(rig.dav.asked(), [format!("PUT {URL}abc.ics If-Match: \"v2\"")]);
    assert_eq!(rig.dav.texts()[0].1, text);
    assert_eq!(rig.folder_of(&mail), None);

    // Changed by another program since it was read: nothing is written
    // over it, and nothing is mailed about an answer that was not kept.
    rig.dav.touch("abc.ics");
    let refused = rig.ask(answer(false, "planning@example.com", Answer::Accept)).unwrap_err();
    assert_eq!(refused.reason, "the event changed at its provider since the calendar was read");
    assert_eq!(refused.hint, ":sync, then do it again");
    assert_eq!(rig.dav.texts()[0].1, text);
    let outbox = rig.store.run(|s| Ok(s.folders()?.iter().map(|f| f.total).sum::<u32>())).unwrap();
    assert_eq!(outbox, 0, "no message is anywhere");
}

#[test]
fn a_calendar_the_config_does_not_say_is_written_is_never_written() {
    let mut rig = Rig::new("unwritten", Kind::CalDav { write: false });
    rig.provide("abc.ics", &invitation("planning@example.com", 0, None, ""));

    rig.ask(answer(false, "planning@example.com", Answer::Tentative)).unwrap();
    assert_eq!(said(&rig.files()[0].text), Said::Tentative, "the answer is kept here, as for a feed");
    rig.ask(save(false, made("Dentist"), None, false)).unwrap();
    assert_eq!(rig.files().len(), 2, "an event made for the account is local too");

    let refused = rig.ask(Action::DeleteEvent(Box::new(id(false, "planning@example.com")))).unwrap_err();
    assert_eq!(refused.reason, "work's calendar is read here and never written, so the event stays as it is");
    assert_eq!(refused.hint, "delete it where the calendar is kept");
    let changed = save(false, made("Planning, moved"), Some(id(false, "planning@example.com")), false);
    assert_eq!(rig.ask(changed).unwrap_err().hint, "change it where the calendar is kept");
    assert!(rig.dav.asked().is_empty(), "not one request reached the provider");
    assert_eq!(rig.dav.texts().len(), 1);
}

#[test]
fn a_new_event_is_kept_under_a_uid_made_here_locally_or_at_the_provider_and_u_removes_it() {
    let mut rig = Rig::new("new", Kind::CalDav { write: true });
    rig.ask(save(true, made("Dentist"), None, false)).unwrap();
    let files = rig.files();
    let event = &content::calendar(&files[0].text).events[0];
    let (name, domain) = event.uid.split_once('@').unwrap();
    assert!(name.len() == 32 && name.chars().all(|c| c.is_ascii_hexdigit()) && domain == "tuimex", "{}", event.uid);
    assert_eq!((event.summary.as_str(), &event.start), ("Dentist", &Moment::Utc(1_795_687_200)));
    let (calendar, _) = rig.shown();
    assert!(calendar.shelves.iter().any(|shelf| shelf.local && shelf.events == [event.clone()]));
    assert_eq!(rig.ask(Action::UndoEvent(1)), Ok(None));
    assert!(rig.files().is_empty());

    rig.ask(save(false, made("Offsite"), None, false)).unwrap();
    let asked = rig.dav.asked();
    let put = asked[0].strip_prefix(&format!("PUT {URL}")).and_then(|rest| rest.strip_suffix(".ics If-None-Match: *"));
    let name = put.unwrap_or_else(|| panic!("a new event is put only where nothing is: {asked:?}"));
    assert!(name.len() == 32 && name.chars().all(|c| c.is_ascii_hexdigit()), "a name made here: {name}");
    let (url, text) = rig.dav.texts().remove(0);
    assert!(!url.contains(&content::calendar(&text).events[0].uid), "the UID is never the address");
    assert!(rig.files().is_empty());
    assert_eq!(rig.ask(Action::UndoEvent(3)), Ok(None));
    assert_eq!(rig.dav.asked(), [format!("DELETE {url} If-Match: \"v1\"")]);
    assert!(rig.dav.texts().is_empty());
}

#[test]
fn guests_are_mailed_an_invitation_when_the_reader_agreed_and_a_cancellation_when_the_event_is_deleted() {
    let mut rig = Rig::new("guests", Kind::None);
    let mut event = made("Offsite planning");
    event.organizer = Some(person(ME));
    event.guests =
        ["imani@example.com", "lars@example.com"].map(|g| Guest { who: person(g), said: Said::Nothing }).into();

    let invitation = rig.ask(save(true, event.clone(), None, true)).unwrap().expect("the guests are mailed");
    let (head, part) = rig.part_of(&invitation);
    assert!(head.contains("method=REQUEST") && head.contains("imani@example.com") && head.contains("lars@example.com"));
    assert!(part.contains("RSVP=TRUE") && part.contains("SUMMARY:Offsite planning"), "{part}");
    let uid = content::calendar(&rig.files()[0].text).events[0].uid.clone();
    assert!(part.contains(&format!("UID:{uid}")), "the guests are sent the event that was kept");

    let cancelled = rig.ask(Action::DeleteEvent(Box::new(id(true, &uid)))).unwrap().expect("the guests are told");
    let (head, part) = rig.part_of(&cancelled);
    assert!(head.contains("method=CANCEL") && part.contains("STATUS:CANCELLED"), "{head}{part}");
    assert!(rig.files().is_empty());
    assert_eq!(rig.ask(Action::UndoEvent(2)), Ok(Some(cancelled.clone())));
    assert_eq!((rig.files().len(), rig.folder_of(&cancelled)), (1, None), "the event is back and nobody is told");

    // Agreed to mail guests an event that has none: nothing is saved.
    let refused = rig.ask(save(true, made("Alone"), None, true)).unwrap_err();
    assert!(refused.reason.starts_with("the guests were not mailed, so nothing was saved:"), "{}", refused.reason);
    assert_eq!(rig.files().len(), 1);
}

#[test]
fn an_event_is_changed_in_place_and_moved_to_another_calendar_and_u_puts_each_back() {
    let mut rig = Rig::new("changed", Kind::CalDav { write: true });
    rig.ask(save(true, made("Dentist"), None, false)).unwrap();
    let was = rig.files().remove(0);
    let uid = content::calendar(&was.text).events[0].uid.clone();

    rig.ask(save(true, made("Dentist, moved"), Some(id(true, &uid)), false)).unwrap();
    let now = rig.files().remove(0);
    let event = &content::calendar(&now.text).events[0];
    assert_eq!((now.href.as_str(), event.uid.as_str(), event.summary.as_str()), (&*was.href, &*uid, "Dentist, moved"));
    assert_eq!(rig.ask(Action::UndoEvent(2)), Ok(None));
    assert_eq!(rig.files()[0].text, was.text, "the text it had is the text it has");

    rig.ask(save(false, made("Dentist"), Some(id(true, &uid)), false)).unwrap();
    assert!(rig.files().is_empty(), "it left the calendar it was in");
    assert_eq!(content::calendar(&rig.dav.texts()[0].1).events[0].uid, uid, "and is the same event where it went");
    assert_eq!(rig.ask(Action::UndoEvent(4)), Ok(None));
    assert_eq!((rig.dav.texts().len(), rig.files()[0].text.as_str()), (0, was.text.as_str()));
}

#[test]
fn one_day_of_a_repeating_event_is_stood_in_for_or_left_out_and_the_event_stays() {
    let mut rig = Rig::new("days", Kind::None);
    let rule =
        Rule { freq: Freq::Daily, interval: 1, count: None, until: None, by_day: Vec::new(), by_month_day: Vec::new() };
    rig.ask(save(true, Event { repeat: Some(Repeat::Rule(rule)), ..made("Stand-up") }, None, false)).unwrap();
    let uid = content::calendar(&rig.files()[0].text).events[0].uid.clone();
    let day = |n: i64| Moment::Utc(1_795_687_200 + n * 86_400);

    let moved = Event { instead_of: Some(day(1)), start: Moment::Utc(day(1).wall() + 3_600), ..made("Stand-up, late") };
    rig.ask(save(true, moved, Some(EventId { occurrence: Some(day(1)), ..id(true, &uid) }), false)).unwrap();
    let events = content::calendar(&rig.files()[0].text).events;
    assert_eq!(rig.files().len(), 1, "an event and what stands in for its days are one thing kept");
    let summaries: Vec<_> = events.iter().map(|e| (e.summary.as_str(), e.instead_of.clone())).collect();
    assert_eq!(summaries, [("Stand-up", None), ("Stand-up, late", Some(day(1)))]);

    // A day with nothing standing in for it is left out of the rule.
    rig.ask(Action::DeleteEvent(Box::new(EventId { occurrence: Some(day(2)), ..id(true, &uid) }))).unwrap();
    // The day that was stood in for goes, and is left out too.
    rig.ask(Action::DeleteEvent(Box::new(EventId { occurrence: Some(day(1)), ..id(true, &uid) }))).unwrap();
    let events = content::calendar(&rig.files()[0].text).events;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].excluded, [day(2), day(1)]);
    assert_eq!(rig.ask(Action::UndoEvent(4)), Ok(None));
    assert_eq!(content::calendar(&rig.files()[0].text).events.len(), 2, "the stand-in is back");

    rig.ask(Action::DeleteEvent(Box::new(id(true, &uid)))).unwrap();
    assert!(rig.files().is_empty(), "deleted whole, everything of its UID goes");
    assert_eq!(rig.ask(Action::UndoEvent(6)), Ok(None));
    assert_eq!(content::calendar(&rig.files()[0].text).events.len(), 2);
}

#[test]
fn a_time_proposed_and_an_invitation_ignored_are_the_reader_s_own_and_are_kept_across_runs() {
    let mut rig = Rig::new("own", Kind::Feed);
    let feed = invitation("planning@example.com", 0, None, "").replace("END:VEVENT", "END:VEVENT\r\nBEGIN:VEVENT\r\nUID:lunch@example.com\r\nDTSTART:20261127T120000Z\r\nSUMMARY:Lunch\r\nORGANIZER:mailto:lars@example.com\r\nATTENDEE:mailto:ada@example.org\r\nEND:VEVENT");
    rig.calendars().put("work", provider::FEED, "", None, &feed).unwrap();

    let start = Moment::Utc(1_795_773_600);
    let proposal = Answer::Propose { start: start.clone(), end: None };
    let counter = rig.ask(answer(false, "planning@example.com", proposal)).unwrap().unwrap();
    let (head, part) = rig.part_of(&counter);
    assert!(head.contains("method=COUNTER") && part.contains("DTSTART:20261127T100000Z"), "{head}{part}");
    assert_eq!(rig.ask(answer(false, "lunch@example.com", Answer::Ignore)), Ok(None), "ignoring mails nobody");
    let (calendar, _) = rig.shown();
    let proposed: Vec<_> = calendar.proposed.iter().map(|p| (p.account.0.as_str(), p.uid.as_str(), p.start)).collect();
    assert_eq!(proposed, [("work", "planning@example.com", 1_795_773_600 + 3_600)], "on the reader's own clock");
    assert_eq!(calendar.ignored, [(AccountId("work".into()), "lunch@example.com".to_string())]);
    assert!(rig.files().is_empty(), "the feed holds the event, so nothing is copied for a proposal");

    // Another run over the same directories begins with both.
    let (told, _events) = mpsc::channel();
    let mut next = Rig::over(rig.root.clone(), Kind::Feed, &[ME]);
    next.reader.start(told).unwrap();
    let (calendar, _) = next.shown();
    assert_eq!((calendar.proposed.len(), calendar.ignored.len()), (1, 1));
    let text = std::fs::read_to_string(rig.root.join("data/.calendars/answers.json")).unwrap();
    assert!(text.contains("lunch@example.com"), "kept beside the local events, not in the cache: {text}");

    assert_eq!(rig.ask(Action::UndoEvent(2)), Ok(None));
    assert_eq!(rig.ask(Action::UndoEvent(1)), Ok(Some(counter.clone())));
    let (calendar, _) = rig.shown();
    assert!(calendar.proposed.is_empty() && calendar.ignored.is_empty());
    assert_eq!(rig.folder_of(&counter), None);
    // An answer outright settles what was proposed.
    rig.ask(answer(false, "planning@example.com", Answer::Propose { start, end: None })).unwrap();
    rig.ask(answer(false, "planning@example.com", Answer::Accept)).unwrap();
    assert!(rig.shown().0.proposed.is_empty());
}

#[test]
fn what_cannot_be_done_is_refused_in_plain_words_and_changes_nothing() {
    let mut rig = Rig::new("refused", Kind::None);
    let refused = |rig: &mut Rig, action: Action| rig.ask(action).unwrap_err().reason;
    let unsaved = "the event is still being saved; do it again in a moment";
    assert_eq!(refused(&mut rig, Action::DeleteEvent(Box::new(id(true, "unsaved 4")))), unsaved);
    assert_eq!(refused(&mut rig, answer(true, "unsaved 4", Answer::Accept)), unsaved);
    assert_eq!(refused(&mut rig, save(true, made("Later"), Some(id(true, "unsaved 4")), false)), unsaved);

    let gone = "the event is no longer in work's calendar";
    assert_eq!(refused(&mut rig, Action::DeleteEvent(Box::new(id(true, "nowhere@example.com")))), gone);
    assert_eq!(refused(&mut rig, answer(true, "nowhere@example.com", Answer::Accept)), gone);
    assert_eq!(refused(&mut rig, save(true, made("Later"), Some(id(true, "nowhere@example.com")), false)), gone);
    assert_eq!(refused(&mut rig, Action::UndoEvent(99)), "there is nothing of it left to take back");

    // An event the account organizes has nobody to answer.
    let mut own = made("Mine");
    own.organizer = Some(person(ME));
    own.guests = vec![Guest { who: person("imani@example.com"), said: Said::Nothing }];
    rig.ask(save(true, own, None, false)).unwrap();
    let uid = content::calendar(&rig.files()[0].text).events[0].uid.clone();
    let before = rig.files();
    assert_eq!(refused(&mut rig, answer(true, &uid, Answer::Accept)), "you organize this event");
    // An account the config does not have has no address to answer from.
    let stranger = EventId { account: AccountId("elsewhere".into()), ..id(true, &uid) };
    let asked = EventAnswer { event: stranger, answer: Answer::Accept, message: None };
    assert_eq!(
        refused(&mut rig, Action::AnswerEvent(Box::new(asked))),
        "the event is no longer in elsewhere's calendar"
    );
    assert_eq!(rig.files(), before);
    assert_eq!(rig.store.run(|s| Ok(s.folders()?.iter().map(|f| f.total).sum::<u32>())).unwrap(), 0);
}

#[test]
fn mail_s_own_undo_of_the_reply_takes_the_calendar_entry_back_and_a_reply_that_left_stands() {
    let mut rig = Rig::new("mail-undo", Kind::Feed);
    rig.calendars().put("work", provider::FEED, "", None, &invitation("planning@example.com", 0, None, "")).unwrap();
    let mail = rig.ask(answer(false, "planning@example.com", Answer::Accept)).unwrap().unwrap();
    assert_eq!(rig.files().len(), 1);

    // `u` on the reply's row in the Outbox: the engine first, as the
    // worker has it, then the calendar.
    let post = Post { engines: &rig.engines, store: &rig.store };
    assert_eq!(rig.reader.unsent(std::slice::from_ref(&mail), &post), Ok(false), "still held: nothing to take back");
    rig.engines.act(Ask { serial: 2, action: Action::Undo, on: vec![mail.clone()] });
    assert_eq!(rig.reader.unsent(std::slice::from_ref(&mail), &post), Ok(true));
    assert!(rig.files().is_empty(), "the calendar no longer says accepted");
    assert_eq!(rig.folder_of(&mail).as_deref(), Some("Drafts"), "and the reply is where mail's undo leaves mail");
    assert_eq!(rig.reader.unsent(std::slice::from_ref(&mail), &post), Ok(false), "once");

    rig.serial = 2;
    let mail = rig.ask(answer(false, "planning@example.com", Answer::Decline)).unwrap().unwrap();
    rig.reader.sent(&mail);
    let stands = rig.ask(Action::UndoEvent(3)).unwrap_err();
    assert_eq!(stands.reason, "its mail has been sent, so it stands; answer again to change it");
    assert_eq!(said(&rig.files()[0].text), Said::Declined);
}

#[test]
fn an_answer_whose_mail_the_engine_refuses_leaves_no_calendar_entry() {
    // The engine lets mail leave from another address only, so the reply
    // from the invited one is refused once it is asked to send it.
    let mut rig = Rig::over(scratch("engine-refused"), Kind::Feed, &["someone.else@example.org"]);
    rig.calendars().put("work", provider::FEED, "", None, &invitation("planning@example.com", 0, None, "")).unwrap();
    rig.ask(answer(false, "planning@example.com", Answer::Accept)).unwrap();
    assert_eq!(rig.files().len(), 1);
    let deadline = Instant::now() + PATIENCE;
    let serial = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rig.events.recv_timeout(left).expect("the engine answers the send") {
            Told::ActionFailed { serial, .. } => break serial,
            Told::ActionDone { summary } => {
                panic!("the engine held mail from an address that is not its own: {summary}")
            }
            _ => {}
        }
    };
    assert_eq!(serial, 1, "the refusal names the calendar ask");
    let post = Post { engines: &rig.engines, store: &rig.store };
    assert_eq!(rig.reader.refused(serial, &post), Ok(true));
    assert!(rig.files().is_empty());
    assert_eq!(rig.store.run(|s| Ok(s.listed(None)?.len())).unwrap(), 0, "the reply nobody wrote is in no folder");
    assert_eq!(rig.reader.refused(serial, &post), Ok(false), "and an ask that is not the calendar's is not");
}

#[test]
fn the_local_events_are_exported_as_one_calendar_and_an_event_is_found_as_an_ask_names_it() {
    let mut rig = Rig::new("export", Kind::Feed);
    assert_eq!(rig.reader.exported(), Ok(None));
    rig.calendars().put("work", provider::FEED, "", None, &invitation("planning@example.com", 0, None, "")).unwrap();
    rig.ask(save(true, made("Dentist"), None, false)).unwrap();
    rig.ask(save(true, made("Library books due"), None, false)).unwrap();
    let text = rig.reader.exported().unwrap().expect("there are local events");
    assert_eq!(text.matches("BEGIN:VCALENDAR").count(), 1);
    let summaries: Vec<String> = content::calendar(&text).events.into_iter().map(|event| event.summary).collect();
    assert_eq!(summaries.len(), 2, "what a feed holds is not tuimex's to export: {summaries:?}");
    assert!(summaries.contains(&"Dentist".to_string()) && summaries.contains(&"Library books due".to_string()));

    assert_eq!(rig.reader.event(&id(false, "planning@example.com")).map(|e| e.summary.as_str()), Some("Planning"));
    assert_eq!(rig.reader.event(&id(true, "planning@example.com")), None, "in the calendar named, and no other");
}

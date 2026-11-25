use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use tuimex_core::Address;
use tuimex_core::calendar::{Calendar as Parsed, Guest, Said};
use tuimex_engine::calendar::{CalendarError, Configured, Got, Sent, Source, Wire};
use tuimex_engine::{Keyring, Secret};
use tuimex_store::calendar::Calendars;

use super::*;

const ME: &str = "robin.vale@example.org";
/// Long enough that only a hang reaches it.
const PATIENCE: Duration = Duration::from_secs(20);

/// A directory of this test's own under the build directory, never the
/// system temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("calendar-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An event at `wall` on the clock of `zone`.
fn event(uid: &str, wall: i64, zone: Option<&str>) -> Held {
    Held {
        uid: uid.into(),
        sequence: 0,
        instead_of: None,
        summary: uid.into(),
        start: Moment::Wall { wall, zone: zone.map(str::to_string) },
        end: None,
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

fn asked(mut event: Held, said: Said) -> Held {
    event.guests.push(Guest { who: Address { name: None, email: ME.into() }, said });
    event
}

fn kept(account: &str, local: bool, read_at: Option<i64>, events: Vec<Held>) -> Kept {
    let calendar = Parsed { method: None, events, zones: BTreeMap::new() };
    Kept { account: account.into(), local, name: None, read_at, calendar }
}

fn identity(account: &str, email: &str) -> Identity {
    Identity { account: AccountId(account.into()), address: Address { name: None, email: email.into() } }
}

/// A zone file of the oldest kind: one offset, kept always.
fn zone_file(offset: i32) -> Vec<u8> {
    let mut out = b"TZif\0".to_vec();
    out.extend([0; 15]);
    for count in [0u32, 0, 0, 0, 1, 4] {
        out.extend(count.to_be_bytes());
    }
    out.extend(offset.to_be_bytes());
    out.extend([0, 0]);
    out.extend(b"ZZZ\0");
    out
}

fn about(zone_dirs: Vec<PathBuf>) -> About {
    let identities = vec![identity("work", ME), identity("home", "robin@example.com")];
    About { identities, zone: Zone::fixed(3_600), zone_dirs }
}

fn uids(shelf: &Shelf) -> Vec<&str> {
    shelf.events.iter().map(|event| event.uid.as_str()).collect()
}

#[test]
fn the_calendar_is_a_shelf_for_each_thing_kept_with_its_zones_its_failures_and_who_the_accounts_are() {
    let zones = scratch("zones");
    std::fs::create_dir_all(zones.join("Scratch")).unwrap();
    std::fs::write(zones.join("Scratch/Half"), zone_file(19_800)).unwrap();
    // The system's file for a name the calendar defines itself.
    std::fs::write(zones.join("Scratch/Defined"), zone_file(-7_200)).unwrap();

    let mut feed = kept(
        "work",
        false,
        Some(1_000),
        vec![
            event("standup", 0, Some("Scratch/Defined")),
            event("review", 0, Some("Scratch/Half")),
            event("nowhere", 0, Some("Scratch/Missing")),
            event("climbs", 0, Some("../../etc/passwd")),
            asked(event("planning", 0, None), Said::Nothing),
        ],
    );
    feed.name = Some("Work".into());
    feed.calendar.zones.insert("Scratch/Defined".into(), Zone::fixed(3_600));
    let answered = kept("work", true, None, vec![asked(event("planning", 0, None), Said::Accepted)]);
    let made_here = kept("work", true, None, vec![event("dentist", 0, None)]);
    let home = kept("home", false, Some(500), vec![event("party", 0, None)]);
    let failed = [
        Failed {
            account: AccountId("home".into()),
            reason: "the connection failed".into(),
            read_at: Some(Timestamp(400)),
        },
        Failed { account: AccountId("side".into()), reason: "it did not answer in time".into(), read_at: None },
    ];
    let about = about(vec![zones]);

    let calendar = build(vec![feed, answered, made_here, home], &failed, &about, Timestamp(2_000));

    assert_eq!((calendar.now, &calendar.zone), (Timestamp(2_000), &Zone::fixed(3_600)));
    assert_eq!(calendar.identities, about.identities);
    let shelves: Vec<_> = calendar.shelves.iter().map(|s| (s.account.0.as_str(), s.local, uids(s))).collect();
    assert_eq!(
        shelves,
        [
            ("work", false, vec!["standup", "review", "nowhere", "climbs", "planning"]),
            ("work", true, vec!["dentist"]),
            ("home", false, vec!["party"]),
            ("side", false, vec![]),
        ],
        "the answer given here is not a second event, and the shelf that held only it is gone"
    );
    let work = &calendar.shelves[0];
    assert_eq!((work.name.as_deref(), work.read, &work.failed), (Some("Work"), Some(Timestamp(1_000)), &None));
    let planning = work.events.iter().find(|event| event.uid == "planning").unwrap();
    assert_eq!(planning.guests[0].said, Said::Accepted, "ADR-055: the answer given here, on the provider's event");

    // ADR-056: a calendar that fails keeps what it held, and says why and
    // when it was last read.
    let home = &calendar.shelves[2];
    assert_eq!((home.failed.as_deref(), home.read), (Some("the connection failed"), Some(Timestamp(400))));
    let side = &calendar.shelves[3];
    assert_eq!((side.failed.as_deref(), side.read), (Some("it did not answer in time"), None));
    let unread: Vec<_> = calendar.unread().into_iter().map(|(account, read)| (account.0.as_str(), read)).collect();
    assert_eq!(unread, [("home", Some(Timestamp(400))), ("side", None)]);

    // ADR-061: the calendar's own definition, then the system's, then none.
    let names: Vec<&str> = calendar.zones.keys().map(String::as_str).collect();
    assert_eq!(names, ["Scratch/Defined", "Scratch/Half"]);
    assert_eq!(calendar.zones["Scratch/Defined"], Zone::fixed(3_600), "what the calendar defined is not replaced");
    assert_eq!(calendar.zones["Scratch/Half"].offset_at_wall(0), Some(19_800));
}

#[test]
fn a_failure_is_on_every_shelf_of_the_account_s_own_and_never_on_its_local_one() {
    let kept = vec![
        kept("work", false, Some(100), vec![event("a", 0, None)]),
        kept("work", false, Some(100), vec![event("b", 0, None)]),
        kept("work", true, None, vec![event("c", 0, None)]),
    ];
    let failed = [Failed { account: AccountId("work".into()), reason: "refused".into(), read_at: None }];
    let calendar = build(kept, &failed, &about(Vec::new()), Timestamp(200));
    let said: Vec<_> = calendar.shelves.iter().map(|shelf| (shelf.failed.as_deref(), shelf.read)).collect();
    let read = Some(Timestamp(100));
    assert_eq!(said, [(Some("refused"), read), (Some("refused"), read), (None, None)], "the store's time stands");
    let about = about(Vec::new());
    let nothing =
        Calendar { zone: about.zone.clone(), identities: about.identities.clone(), ..Calendar::empty(Timestamp(200)) };
    assert_eq!(build(Vec::new(), &[], &about, Timestamp(200)), nothing, "nothing kept is no shelf");
}

const FEED: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:kept@example.org\r\nDTSTART:20261126T100000Z\r\n\
                    DTEND:20261126T110000Z\r\nSUMMARY:Kept on disk\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
const LOCAL: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:here@example.org\r\nDTSTART:20261127T100000Z\r\n\
                     SUMMARY:Made here\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

/// A provider nothing reaches: every request fails, and is counted.
struct Down(mpsc::Sender<()>);

impl Wire for Down {
    fn send(&self, _: &Sent<'_>) -> Result<Got, CalendarError> {
        // The test that listens has gone when it has seen enough.
        let _ = self.0.send(());
        Err(CalendarError::Unreachable("the connection failed".into()))
    }
}

/// A keyring that holds the feed's address and nothing else. No test
/// opens the real one.
struct Holds;

impl Keyring for Holds {
    fn password(&self, _: &str) -> Result<Option<Secret>, String> {
        Ok(Some(Secret::new("https://calendar.example.net/private/feed.ics".into())))
    }

    fn set(&self, _: &str, _: &Secret) -> Result<(), String> {
        Err("this keyring keeps nothing".into())
    }
}

/// The calendars a reader hands over, without what it hands over beside
/// them.
struct Arriving(Receiver<Handed>);

fn calendar_of(handed: Handed) -> Option<Calendar> {
    match handed {
        Handed::Calendar { calendar, .. } => Some(*calendar),
        Handed::Invited(..) => None,
    }
}

impl Arriving {
    fn recv_timeout(&self, wait: Duration) -> Result<Calendar, mpsc::RecvTimeoutError> {
        loop {
            if let Some(calendar) = calendar_of(self.0.recv_timeout(wait)?) {
                return Ok(calendar);
            }
        }
    }

    fn try_iter(&self) -> impl Iterator<Item = Calendar> + '_ {
        self.0.try_iter().filter_map(calendar_of)
    }

    fn try_recv(&self) -> Result<Calendar, mpsc::TryRecvError> {
        loop {
            if let Some(calendar) = calendar_of(self.0.try_recv()?) {
                return Ok(calendar);
            }
        }
    }
}

/// A reader over a scratch store that holds one fetched event and one
/// local one, what it sends, and how often its provider was asked.
fn reader(name: &str) -> (Reader, Arriving, Receiver<()>) {
    let root = scratch(name);
    let (data, cache) = (root.join("data"), root.join("cache"));
    let store = Calendars::open(&data, &cache).unwrap();
    store.put("work", tuimex_engine::calendar::FEED, "", None, FEED).unwrap();
    store.read("work", tuimex_engine::calendar::FEED, None, None, 1_000).unwrap();
    store.keep("work", LOCAL).unwrap();
    let ((shown, arriving), (asked, asks)) = (mpsc::channel(), mpsc::channel());
    let runner = Runner {
        calendars: vec![Configured { account: "work".into(), source: Source::Feed { address_cmd: None } }],
        wire: Box::new(Down(asked)),
        keyring: Box::new(Holds),
        data,
        cache,
        every: Duration::from_secs(3_600),
    };
    (Reader::new(runner, about(Vec::new()), shown), Arriving(arriving), asks)
}

fn summaries(calendar: &Calendar) -> Vec<(bool, &str)> {
    let events =
        calendar.shelves.iter().flat_map(|shelf| shelf.events.iter().map(|event| (shelf.local, &*event.summary)));
    events.collect()
}

#[test]
fn what_is_on_disk_is_handed_over_before_a_fetch_answers_and_a_failure_keeps_it() {
    let (mut reader, arriving, asks) = reader("start");
    let (events, heard) = mpsc::channel();
    reader.start(events).unwrap();

    let first = arriving.recv_timeout(PATIENCE).unwrap();
    assert_eq!(summaries(&first), [(false, "Kept on disk"), (true, "Made here")]);
    assert_eq!((first.shelves[0].read, &first.shelves[0].failed), (Some(Timestamp(1_000)), &None));

    let failed = heard.recv_timeout(PATIENCE).unwrap();
    let Event::CalendarFailed { account, reason, read_at } = &failed else { panic!("{failed:?}") };
    assert_eq!((account.0.as_str(), *read_at), ("work", Some(Timestamp(1_000))));
    reader.told(&failed).unwrap();
    let after = arriving.recv_timeout(PATIENCE).unwrap();
    assert_eq!(summaries(&after), summaries(&first), "the events held stay");
    assert_eq!(after.shelves[0].failed.as_ref(), Some(reason));
    assert_eq!(after.shelves[1].failed, None);
    assert!(!reason.contains("private"), "ADR-056: the address is not in {reason:?}");

    // `:sync` has the calendar thread ask again, without waiting for the
    // hour it would otherwise leave between passes.
    asks.recv_timeout(PATIENCE).unwrap();
    reader.nudge();
    asks.recv_timeout(PATIENCE).unwrap();
    assert!(matches!(heard.recv_timeout(PATIENCE).unwrap(), Event::CalendarFailed { .. }));

    // The calendar thread ends when the reader goes: nothing more is said.
    drop(reader);
    let ended = heard.recv_timeout(PATIENCE);
    assert!(matches!(ended, Err(mpsc::RecvTimeoutError::Disconnected)), "every sender of events has gone: {ended:?}");
}

#[test]
fn a_calendar_that_is_read_again_loses_its_failure_and_shows_what_the_store_now_holds() {
    let (mut reader, arriving, _asks) = reader("read");
    reader.read().unwrap();
    let work = AccountId("work".into());
    let failed = Event::CalendarFailed { account: work.clone(), reason: "refused".into(), read_at: None };
    reader.told(&failed).unwrap();
    let said = arriving.try_iter().last().unwrap();
    assert_eq!((said.shelves[0].failed.as_deref(), said.shelves[0].read), (Some("refused"), Some(Timestamp(1_000))));

    // The calendar thread writes through a handle of its own.
    let store = Calendars::open(&reader.data, &reader.cache).unwrap();
    store.put("work", tuimex_engine::calendar::FEED, "", None, &FEED.replace("Kept on disk", "Moved")).unwrap();
    store.read("work", tuimex_engine::calendar::FEED, None, None, 2_000).unwrap();
    reader.told(&Event::CalendarRead { account: work.clone(), changed: true }).unwrap();
    let read = arriving.try_iter().last().unwrap();
    assert_eq!(summaries(&read), [(false, "Moved"), (true, "Made here")]);
    assert_eq!((&read.shelves[0].failed, read.shelves[0].read), (&None, Some(Timestamp(2_000))));

    // An event that is not the calendar's hands nothing over.
    reader
        .told(&Event::Synced { account: work, folder: tuimex_core::FolderId("INBOX".into()), changed: Vec::new() })
        .unwrap();
    assert!(arriving.try_recv().is_err());
}

#[test]
fn a_store_that_cannot_be_read_says_so_and_a_failure_is_still_handed_over() {
    let (mut reader, arriving, _asks) = reader("unreadable");
    // Where the cache directory should be there is a file.
    reader.cache = reader.cache.join("calendar.sqlite").join("under-a-file");
    let said = reader.read().unwrap_err();
    assert!(said.starts_with("the calendar store: "), "{said}");
    assert!(arriving.try_recv().is_err());
    let failed = Event::CalendarFailed { account: AccountId("work".into()), reason: "refused".into(), read_at: None };
    assert!(reader.told(&failed).is_err());
    let shown = arriving.try_recv().unwrap();
    assert_eq!(shown.shelves.len(), 1);
    assert_eq!((shown.shelves[0].failed.as_deref(), uids(&shown.shelves[0])), (Some("refused"), vec![]));
}

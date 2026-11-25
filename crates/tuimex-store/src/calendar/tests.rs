use std::fs;

use super::*;
use crate::testing::{mode, scratch};

const EVENT: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:one@example.com\r\nDTSTART:20261126T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

fn open(name: &str) -> (Calendars, PathBuf) {
    let root = scratch(name);
    (Calendars::open(&root.join("data"), &root.join("cache")).unwrap(), root)
}

#[test]
fn what_was_fetched_is_kept_by_address_and_replaced_in_place() {
    let (calendars, _) = open("calendar-fetched");
    calendars.read("work", "/dav/events/", Some("Work"), Some("tag-1"), 100).unwrap();
    calendars.put("work", "/dav/events/", "/dav/events/a.ics", Some("\"1\""), EVENT).unwrap();
    calendars.put("work", "/dav/events/", "/dav/events/b.ics", None, "second").unwrap();
    calendars.put("personal", "feed", "", None, "a whole feed").unwrap();
    assert_eq!(
        calendars.tags("work", "/dav/events/").unwrap(),
        vec![("/dav/events/a.ics".to_string(), Some("\"1\"".to_string())), ("/dav/events/b.ics".to_string(), None)]
    );
    assert_eq!(calendars.tag("work", "/dav/events/").unwrap().as_deref(), Some("tag-1"));
    assert_eq!(calendars.tag("work", "/elsewhere/").unwrap(), None);

    calendars.put("work", "/dav/events/", "/dav/events/a.ics", Some("\"2\""), "changed").unwrap();
    calendars.drop_one("work", "/dav/events/", "/dav/events/b.ics").unwrap();
    calendars.drop_one("work", "/dav/events/", "/dav/events/never.ics").unwrap();
    calendars.read("work", "/dav/events/", Some("Work"), Some("tag-2"), 200).unwrap();
    let held = calendars.held().unwrap();
    assert_eq!(held.len(), 2);
    assert_eq!((held[0].account.as_str(), held[0].text.as_str()), ("personal", "a whole feed"));
    assert_eq!(
        (held[1].href.as_str(), held[1].etag.as_deref(), held[1].text.as_str()),
        ("/dav/events/a.ics", Some("\"2\""), "changed")
    );
    assert_eq!(
        calendars.fetched().unwrap(),
        vec![Fetched {
            account: "work".into(),
            calendar: "/dav/events/".into(),
            name: Some("Work".into()),
            tag: Some("tag-2".into()),
            read_at: 200
        }]
    );

    calendars.forget("work", "/dav/events/").unwrap();
    assert!(calendars.fetched().unwrap().is_empty());
    assert_eq!(calendars.held().unwrap().len(), 1, "another account's calendar is untouched");
}

#[test]
fn a_local_event_is_an_owner_only_file_named_by_a_key_made_here() {
    let (calendars, root) = open("calendar-local");
    let key = calendars.keep("work", EVENT).unwrap();
    let path = root.join("data/.calendars/work").join(format!("{key}.ics"));
    assert_eq!(fs::read_to_string(&path).unwrap(), EVENT);
    assert_eq!((mode(&path), mode(path.parent().unwrap())), (0o600, 0o700));
    assert!(!key.contains("example.com"), "the event's own id is not the file's name");
    assert_ne!(calendars.keep("work", EVENT).unwrap(), key, "two events never share a key");

    calendars.rewrite("work", &key, "changed").unwrap();
    assert_eq!(calendars.local("work", &key).unwrap(), "changed");
    let held = calendars.held().unwrap();
    assert_eq!(held.len(), 2);
    assert!(held.iter().all(|h| h.calendar.is_none() && h.account == "work"));

    calendars.remove("work", &key).unwrap();
    assert!(matches!(calendars.remove("work", &key), Err(Error::NoEvent { .. })));
    assert!(matches!(calendars.local("work", &key), Err(Error::NoEvent { .. })));
    assert!(matches!(calendars.rewrite("work", &key, "x"), Err(Error::NoEvent { .. })));
    assert_eq!(calendars.held().unwrap().len(), 1);
}

#[test]
fn a_name_from_outside_never_becomes_a_path() {
    let (calendars, root) = open("calendar-names");
    for account in ["", ".hidden", "../up", "a/b", "nul\0"] {
        assert!(matches!(calendars.keep(account, EVENT), Err(Error::Name(_))), "{account:?}");
    }
    let key = calendars.keep("work", EVENT).unwrap();
    for key in ["", ".", "..", "../../../etc/passwd", "a/b", "one@example.com", "a b"] {
        assert!(matches!(calendars.local("work", key), Err(Error::Name(_))), "{key:?}");
        assert!(matches!(calendars.rewrite("work", key, "x"), Err(Error::Name(_))), "{key:?}");
        assert!(matches!(calendars.remove("work", key), Err(Error::Name(_))), "{key:?}");
    }
    assert_eq!(calendars.local("work", &key).unwrap(), EVENT);
    // Files nobody here wrote are neither shown nor touched.
    let dir = root.join("data/.calendars/work");
    for stray in ["notes.txt", ".hidden.ics", "not utf8.ics"] {
        fs::write(dir.join(stray), [0xff, 0xfe]).unwrap();
    }
    assert_eq!(calendars.held().unwrap().len(), 1);
    assert!(dir.join("notes.txt").exists());
}

#[test]
fn the_cache_is_only_a_cache() {
    let root = scratch("calendar-cache");
    let (data, cache) = (root.join("data"), root.join("cache"));
    {
        let calendars = Calendars::open(&data, &cache).unwrap();
        calendars.put("work", "feed", "", None, "fetched").unwrap();
        calendars.keep("work", EVENT).unwrap();
        assert_eq!(mode(&cache.join("calendar.sqlite")), 0o600);
    }
    // Reopened, it is as it was.
    assert_eq!(Calendars::open(&data, &cache).unwrap().held().unwrap().len(), 2);
    // Deleted, or not a database at all, what was fetched is gone and the
    // local event is still there.
    fs::remove_dir_all(&cache).unwrap();
    assert_eq!(Calendars::open(&data, &cache).unwrap().held().unwrap().len(), 1);
    fs::write(cache.join("calendar.sqlite"), "not a database").unwrap();
    for suffix in ["-wal", "-shm"] {
        // What an unclean stop leaves beside a database goes with it.
        let _ = fs::remove_file(cache.join(format!("calendar.sqlite{suffix}")));
    }
    let again = Calendars::open(&data, &cache).unwrap();
    assert_eq!(again.held().unwrap().len(), 1);
    again.put("work", "feed", "", None, "fetched again").unwrap();
    assert_eq!(again.held().unwrap().len(), 2);
}

#[test]
fn local_calendars_are_not_an_account_s_mail() {
    let (calendars, root) = open("calendar-beside-mail");
    calendars.keep("work", EVENT).unwrap();
    let maildir = crate::maildir::Maildir::new(&root.join("data"));
    assert!(maildir.accounts().unwrap().is_empty(), "the directory of local calendars is not an account");
}

use std::path::Path;
use std::time::Duration;

use tuimex_core::Role;
use tuimex_engine::sync::Policy;

use tuimex_engine::calendar::{Configured, Source};
use tuimex_engine::{Keyring, Secret};

use super::{account, calendar_pass, calendars, policy, send_delay};
use crate::config::Config;
use crate::paths::Dirs;
use crate::setup::tests::{DENTIST, FEED, FakeWire, GOOGLE_DAV, MemoryKeyring, PASSWORD, caldav, scratch};

fn config(text: &str) -> Config {
    Config::parse(text).unwrap()
}

const DELAY: Duration = Duration::from_secs(10);
const WORK: &str = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n";

#[test]
fn the_sync_table_becomes_numbers_and_what_is_left_out_keeps_the_default() {
    assert_eq!(policy(None).unwrap(), Policy::default());
    let c = config("[sync]\nbodies = \"7d\"\nmax_body = \"2MB\"\nfolders = [\"Projects\"]\n");
    let want = Policy { bodies: 7 * 86_400, max_body: 2_000_000, folders: vec!["Projects".to_string()] };
    assert_eq!(policy(c.sync.as_ref()).unwrap(), want);
    assert_eq!(policy(config("[sync]\nmax_body = \"500kb\"\n").sync.as_ref()).unwrap().max_body, 500_000);
}

#[test]
fn an_amount_that_is_not_a_number_and_a_known_unit_is_an_error_naming_the_key() {
    for (text, key) in
        [("bodies = \"30\"", "bodies"), ("bodies = \"soon\"", "bodies"), ("max_body = \"10 furlongs\"", "max_body")]
    {
        let error = policy(config(&format!("[sync]\n{text}\n")).sync.as_ref()).unwrap_err().to_string();
        assert!(error.contains(&format!("sync.{key}")), "{error}");
    }
}

#[test]
fn tls_is_implicit_on_993_and_starttls_elsewhere_unless_the_config_says() {
    let starttls = |imap: &str| {
        let c = config(&format!("{WORK}imap = {imap}\n"));
        account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere"))
            .map(|(_, reach)| reach.server.starttls)
    };
    assert!(!starttls("{ host = \"imap.example.org\", port = 993 }").unwrap());
    assert!(starttls("{ host = \"imap.example.org\", port = 143 }").unwrap());
    assert!(!starttls("{ host = \"imap.example.org\", port = 1993, starttls = false }").unwrap());
    assert!(starttls("{ host = \"imap.example.org\", port = 993, starttls = true }").unwrap());
}

#[test]
fn the_account_signs_in_as_its_address_unless_a_user_is_named_and_carries_its_roles() {
    let c = config(&format!(
        "{WORK}imap = {{ host = \"imap.example.org\", port = 993 }}\npassword_cmd = [\"pass\", \"show\", \"work\"]\nfolders = {{ archive = \"Old\" }}\n"
    ));
    let (account_, reach) = account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap();
    assert_eq!((reach.server.user.as_str(), reach.server.host.as_str()), ("ada@example.org", "imap.example.org"));
    assert_eq!(reach.password_cmd.as_deref(), Some(&["pass".to_string(), "show".to_string(), "work".to_string()][..]));
    assert_eq!(account_.roles, [(Role::Archive, "Old".to_string())]);
    let c = config(&format!("{WORK}imap = {{ host = \"imap.example.org\", port = 993, user = \"ada\" }}\n"));
    assert_eq!(account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap().1.server.user, "ada");
}

#[test]
fn an_account_without_a_server_or_with_an_unknown_role_is_an_error_naming_it() {
    let no_server = account(&config(WORK).accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).err().unwrap();
    assert!(no_server.to_string().contains("account work has no `imap` server"));
    let c =
        config(&format!("{WORK}imap = {{ host = \"h.example.org\", port = 993 }}\nfolders = {{ inbox = \"X\" }}\n"));
    let bad_role = account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).err().unwrap();
    assert!(bad_role.to_string().contains("folders.inbox"));
}

#[test]
fn the_send_delay_is_ten_seconds_unless_the_config_says() {
    assert_eq!(send_delay(None).unwrap(), DELAY);
    assert_eq!(send_delay(config("[send]\n").send.as_ref()).unwrap(), DELAY);
    assert_eq!(send_delay(config("[send]\ndelay = \"0s\"\n").send.as_ref()).unwrap(), Duration::ZERO);
    assert_eq!(send_delay(config("[send]\ndelay = \"2m\"\n").send.as_ref()).unwrap(), Duration::from_secs(120));
    let error = send_delay(config("[send]\ndelay = \"soon\"\n").send.as_ref()).unwrap_err().to_string();
    assert!(error.contains("send.delay"), "{error}");
}

#[test]
fn the_account_sends_from_its_address_and_its_identities_through_its_smtp_server() {
    let c = config(&format!(
        "{WORK}identities = [\"Ada <ada@example.org>\", \"lovelace@example.net\"]\n\
         imap = {{ host = \"imap.example.org\", port = 993 }}\nsmtp = {{ host = \"smtp.example.org\", port = 465 }}\n"
    ));
    let (account_, reach) = account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap();
    assert_eq!(account_.identities, ["ada@example.org", "ada@example.org", "lovelace@example.net"]);
    assert_eq!(account_.send_delay, DELAY);
    let smtp = reach.smtp.unwrap();
    assert_eq!((smtp.host.as_str(), smtp.starttls, smtp.user.as_str()), ("smtp.example.org", false, "ada@example.org"));
    let c = config(&format!(
        "{WORK}imap = {{ host = \"imap.example.org\", port = 993 }}\nsmtp = {{ host = \"smtp.example.org\", port = 587, user = \"ada\" }}\n"
    ));
    let smtp = account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap().1.smtp.unwrap();
    assert_eq!((smtp.starttls, smtp.user.as_str()), (true, "ada"));
    let none = config(&format!("{WORK}imap = {{ host = \"imap.example.org\", port = 993 }}\n"));
    assert!(account(&none.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap().1.smtp.is_none());
}

/// Four accounts: Gmail read over CalDAV, a feed whose address is in the
/// keyring, a feed whose address is nowhere, and one with no calendar.
const CALENDARS: &str = r#"
[[account]]
name = "gmail"
address = "ada@gmail.com"
imap = { host = "imap.gmail.com", port = 993 }

[[account]]
name = "home"
address = "ada@example.com"
imap = { host = "127.0.0.1", port = 1143, user = "bridge-user" }

[[account]]
name = "spare"
address = "bob@example.com"
imap = { host = "imap.example.com", port = 993 }

[[account]]
name = "plain"
address = "eve@example.com"
imap = { host = "imap.example.com", port = 993 }

[[calendar]]
account = "gmail"
kind = "caldav"
url = "https://www.google.com/calendar/dav/ada@gmail.com/events/"

[[calendar]]
account = "home"
kind = "feed"

[[calendar]]
account = "spare"
kind = "feed"
address_cmd = ["pass", "show", "calendar/spare"]
"#;

#[test]
fn each_calendar_is_signed_in_to_as_its_account_s_mail_or_read_from_its_feed() {
    let c = config(CALENDARS);
    let all = calendars(&c, None).unwrap();
    assert_eq!(
        all,
        [
            Configured {
                account: "gmail".into(),
                source: Source::CalDav { url: GOOGLE_DAV.into(), user: "ada@gmail.com".into(), password_cmd: None },
            },
            Configured { account: "home".into(), source: Source::Feed { address_cmd: None } },
            Configured {
                account: "spare".into(),
                source: Source::Feed { address_cmd: Some(vec!["pass".into(), "show".into(), "calendar/spare".into()]) },
            },
        ]
    );
    assert_eq!(calendars(&c, Some("home")).unwrap(), all[1..2]);
    assert!(calendars(&c, Some("plain")).unwrap().is_empty());
}

fn dirs_under(root: &Path) -> Dirs {
    Dirs { config: root.join("config"), data: root.join("data"), cache: root.join("cache"), state: root.join("state") }
}

#[test]
fn sync_says_a_line_for_each_calendar_and_a_feed_s_host_alone() {
    let root = scratch("sync-calendars");
    let dirs = dirs_under(&root);
    // The spare feed's command is one that fails, so nothing is run here
    // that could print anything.
    let c = config(&CALENDARS.replace("[\"pass\", \"show\", \"calendar/spare\"]", "[\"false\"]"));
    let keyring = MemoryKeyring::default();
    keyring.set("gmail", &Secret::new(PASSWORD.into())).unwrap();
    keyring.set("home/calendar-feed", &Secret::new(FEED.into())).unwrap();
    let mut served = caldav(GOOGLE_DAV);
    served.push(("GET", FEED, "", 200, DENTIST.to_string()));
    let wire = FakeWire::new(served);

    let (lines, failed) = calendar_pass(&c, None, &wire, &keyring, &dirs, 1_800_000_000).unwrap();
    assert_eq!(
        lines,
        [
            "gmail/calendar (caldav, www.google.com): 1 changed",
            "home/calendar (feed, calendar.example.net): 1 changed",
            "E sync: spare/calendar (feed): address_cmd for spare's calendar failed: false ended with exit status: 1",
        ]
    );
    assert!(failed);
    let signed: Vec<Option<String>> = wire.asked.borrow().iter().map(|(.., user)| user.clone()).collect();
    assert_eq!(
        signed[..3],
        [Some("ada@gmail.com".to_string()), Some("ada@gmail.com".into()), Some("ada@gmail.com".into())]
    );

    // Again: Gmail's calendar has the same tag and is asked nothing more,
    // and the feed is what it was.
    let (lines, failed) = calendar_pass(&c, Some("gmail"), &wire, &keyring, &dirs, 1_800_000_100).unwrap();
    assert_eq!((lines, failed), (vec!["gmail/calendar (caldav, www.google.com): nothing new".to_string()], false));
    let (lines, _) = calendar_pass(&c, Some("home"), &wire, &keyring, &dirs, 1_800_000_100).unwrap();
    assert_eq!(lines, ["home/calendar (feed, calendar.example.net): nothing new"]);

    // A refused sign-in, and a feed that serves a web page.
    let refusing = FakeWire::new(vec![
        ("PROPFIND", GOOGLE_DAV, "", 401, String::new()),
        ("GET", FEED, "", 200, "<!doctype html><title>Sign in</title>".to_string()),
    ]);
    let (lines, failed) = calendar_pass(&c, None, &refusing, &keyring, &dirs, 1_800_000_200).unwrap();
    assert!(failed);
    assert_eq!(
        lines[..2],
        [
            "E sync: gmail/calendar (caldav, www.google.com): the calendar's server refused the sign-in",
            "E sync: home/calendar (feed, calendar.example.net): what the address served is not a calendar",
        ]
    );
    assert!(lines.iter().all(|line| !line.contains("SYNTHETIC-FEED-TOKEN") && !line.contains(PASSWORD)));

    // An account with no calendar: no line, and no calendar store made.
    let bare = scratch("sync-no-calendar");
    let (lines, failed) = calendar_pass(&c, Some("plain"), &wire, &keyring, &dirs_under(&bare), 0).unwrap();
    assert_eq!((lines.len(), failed), (0, false));
    assert!(!bare.join("cache").exists());
}

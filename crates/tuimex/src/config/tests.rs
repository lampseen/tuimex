use std::path::Path;

use tuimex_core::{AccountId, Grants};
use tuimex_ui::palette::Tok;

use super::{Config, path};

const TWO: &str = r#"
[[account]]
name = "work"
address = "ada@example.org"
[account.agent]
read = true

[[account]]
name = "personal"
address = "ada@example.com"
"#;

fn error(text: &str) -> String {
    format!("{:#}", Config::parse(text).unwrap_err())
}

/// The reference in PLAN §11, as it is printed there.
fn plan_reference() -> String {
    let plan = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/PLAN.md")).unwrap();
    let section = plan.split("## 11. Config reference").nth(1).expect("PLAN §11 is there");
    let block = section.split("```toml\n").nth(1).expect("PLAN §11 has a TOML block");
    block.split("```").next().unwrap().to_string()
}

#[test]
fn the_reference_in_the_plan_parses() {
    let config = Config::parse(&plan_reference()).unwrap();
    let account = config.account("personal").unwrap();
    assert_eq!(account.address, "you@example.com");
    assert_eq!(account.grants(), Grants { read: true, folders: vec!["INBOX".into()], draft: true });
}

/// The keys the reference shows commented out are part of the schema too.
#[test]
fn the_reference_in_the_plan_parses_with_every_optional_key_set() {
    let all: String =
        plan_reference().lines().map(|line| line.strip_prefix("# ").unwrap_or(line).to_string() + "\n").collect();
    assert!(all.contains("\npassword_cmd = "), "the commented keys were uncommented");
    Config::parse(&all).unwrap();
}

/// `docs/config.example.toml`, as it is and with every commented key set.
#[test]
fn the_example_config_parses_as_written_and_with_every_optional_key_set() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/config.example.toml");
    let example = std::fs::read_to_string(path).unwrap();
    assert!(Config::parse(&example).unwrap().agent.is_none(), "the agent is off until its table is uncommented");
    // A key is commented as `# key = …` or `# [table]`; prose is not TOML.
    let is_key =
        |line: &str| line.starts_with("# [") || line.split_once(" = ").is_some_and(|(key, _)| !key[2..].contains(' '));
    let all: String = example
        .lines()
        .map(|line| if line.starts_with("# ") && is_key(line) { &line[2..] } else { line })
        .map(|line| line.to_string() + "\n")
        .collect();
    let config = Config::parse(&all).unwrap();
    let agent = config.agent.expect("the [agent] table was uncommented");
    assert_eq!((agent.preset.as_deref(), agent.command), (Some("claude"), None), "no argv is the user's to write");
    assert_eq!(agent.timeout.as_deref(), Some("120s"));
    assert!(all.contains("\npassword_cmd = "));
    assert_eq!(super::colours(&config.accounts).unwrap().len(), 1, "the example's colour is one that is lent");
}

#[test]
fn an_account_without_an_agent_table_grants_nothing() {
    let config = Config::parse(TWO).unwrap();
    assert_eq!(config.account("personal").unwrap().grants(), Grants::default());
    assert!(config.account("nobody").is_none());
}

#[test]
fn read_covers_the_inbox_unless_folders_says_otherwise() {
    let config = Config::parse(TWO).unwrap();
    assert_eq!(
        config.account("work").unwrap().grants(),
        Grants { read: true, folders: vec!["INBOX".into()], draft: false }
    );
    let narrowed = TWO.replace("read = true", "read = true\nfolders = [\"Archive\"]\ndraft = true");
    let grants = Config::parse(&narrowed).unwrap().account("work").unwrap().grants();
    assert_eq!(grants, Grants { read: true, folders: vec!["Archive".into()], draft: true });
    let nothing = TWO.replace("read = true", "folders = [\"Archive\"]");
    assert!(!Config::parse(&nothing).unwrap().account("work").unwrap().grants().read);
}

#[test]
fn an_empty_file_is_a_config_with_no_accounts() {
    assert!(Config::parse("").unwrap().accounts.is_empty());
}

#[test]
fn an_unknown_key_is_an_error_naming_the_key_and_its_line() {
    for (text, key, line) in [
        ("colour = 1\n", "colour", 1),
        ("[[account]]\nname = \"a\"\naddress = \"a@example.com\"\npassword = \"x\"\n", "password", 4),
        ("[[account]]\nname = \"a\"\naddress = \"a@example.com\"\n[account.agent]\nsend = true\n", "send", 5),
        ("[[account]]\nname = \"a\"\naddress = \"a@example.com\"\nimap = { host = \"h\", tls = false }\n", "tls", 4),
        ("[sync]\nbodies = \"30d\"\n\nskip_tls_verify = true\n", "skip_tls_verify", 4),
        ("[ui]\ntheme = \"dark\"\n", "theme", 2),
        ("[notify]\nbell = true\nsound = \"x\"\n", "sound", 3),
        ("[[section]]\nname = \"n\"\nshort = \"s\"\nquery = \"q\"\ncolour = 1\n", "colour", 5),
    ] {
        assert_eq!(error(text), format!("line {line}: unknown key `{key}`"), "{text}");
    }
}

/// ADR-010: the config has no key for a secret, and one typed anyway is
/// not repeated by the error.
#[test]
fn an_error_never_quotes_a_value_from_the_file() {
    for text in [
        "[[account]]\nname = \"a\"\naddress = \"a@example.com\"\npassword = \"hunter2\"\n",
        "[[account]]\nname = \"a\"\naddress = \"a@example.com\"\n[account.agent]\nread = \"hunter2\"\n",
        "[[account]]\nname = \"a\"\naddress = \"a@example.com\"\npassword_cmd = \"hunter2\"\n",
        "[[account]]\nname = \"a\"\naddress = \"a@example.com\"\nimap = { host = \"h\", port = \"hunter2\" }\n",
        "[[account]]\nname = \"a\"\naddress = \"hunter2\n",
        "[[account]]\nname = \"a\"\naddress = \"a@example.com\"\nauth = hunter2\n",
        "hunter2\n",
    ] {
        let error = error(text);
        assert!(!error.contains("hunter2"), "{error}");
        assert!(error.starts_with("line "), "{error}");
    }
}

#[test]
fn a_wrong_type_and_a_missing_key_say_what_was_wanted() {
    let wrong = "[[account]]\nname = \"a\"\naddress = \"a@example.com\"\n[account.agent]\nread = \"yes\"\n";
    assert_eq!(error(wrong), "line 5: expected a boolean");
    assert_eq!(error("[[account]]\nname = \"a\"\n"), "line 1: missing key `address`");
}

#[test]
fn two_accounts_with_one_name_are_refused() {
    let twice = TWO.replace("personal", "work");
    assert_eq!(error(&twice), "two accounts are named work");
}

#[test]
fn a_file_that_is_not_there_is_an_error_and_not_an_empty_config() {
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("no-such-config.toml");
    let error = format!("{:#}", Config::load(&missing).unwrap_err());
    assert!(error.starts_with(&format!("reading the config {}", missing.display())), "{error}");
}

#[test]
fn the_path_is_the_flag_or_the_config_directory_and_never_the_current_one() {
    let dir = Path::new("/h/.config/tuimex");
    assert_eq!(path(None, dir), Path::new("/h/.config/tuimex/config.toml"));
    assert_eq!(path(Some(Path::new("/elsewhere/c.toml")), dir), Path::new("/elsewhere/c.toml"));
}

#[test]
fn ui_preview_is_one_of_four_words_and_auto_when_left_out() {
    use tuimex_ui::list::Preview;
    let of = |text: &str| super::preview(Config::parse(text).unwrap().ui.as_ref());
    assert_eq!(of("").unwrap(), Preview::Auto);
    assert_eq!(of("[ui]\nglyphs = \"ascii\"\n").unwrap(), Preview::Auto);
    for (word, preview) in
        [("auto", Preview::Auto), ("right", Preview::Right), ("below", Preview::Below), ("off", Preview::Off)]
    {
        assert_eq!(of(&format!("[ui]\npreview = \"{word}\"\n")).unwrap(), preview);
    }
    let refused = format!("{:#}", of("[ui]\npreview = \"beside\"\n").unwrap_err());
    assert!(refused.contains("ui.preview = \"beside\": it is one of auto, right, below, off"), "{refused}");
}

#[test]
fn an_account_s_color_is_one_of_the_four_accents_accounts_take() {
    let account = |color: &str| format!("[[account]]\nname = \"work\"\naddress = \"a@example.org\"\ncolor = {color}\n");
    let of = |color: &str| super::colours(&Config::parse(&account(color)).unwrap().accounts);
    for (word, token) in [("link", Tok::Link), ("head", Tok::Head), ("ok", Tok::Ok), ("warn", Tok::Warn)] {
        assert_eq!(of(&format!("\"{word}\"")).unwrap(), [(AccountId("work".into()), token)]);
    }
    for refused in ["agent", "acc", "err", "fg", "blue", "Link", ""] {
        let said = format!("{:#}", of(&format!("\"{refused}\"")).unwrap_err());
        assert!(
            said.contains("account work: color = ") && said.contains("it is one of link, head, ok, warn"),
            "{said}"
        );
    }
    assert!(Config::parse(&account("3")).is_err(), "a number is not a colour's name");
    let none = "[[account]]\nname = \"work\"\naddress = \"a@example.org\"\n";
    assert_eq!(super::colours(&Config::parse(none).unwrap().accounts).unwrap(), []);
}

const CALDAV: &str = "https://www.google.com/calendar/dav/ada@example.org/events/";

#[test]
fn a_calendar_is_caldav_with_an_address_or_a_feed_with_none() {
    let text = format!(
        "{TWO}\n[[calendar]]\naccount = \"work\"\nkind = \"caldav\"\nurl = \"{CALDAV}\"\n\n\
         [[calendar]]\naccount = \"personal\"\nkind = \"feed\"\naddress_cmd = [\"pass\", \"show\", \"cal\"]\n"
    );
    let config = Config::parse(&text).unwrap();
    let work = config.calendar("work").unwrap();
    let kept = (work.kind.as_deref(), work.url.as_deref(), work.address_cmd.as_ref());
    assert_eq!(kept, (Some("caldav"), Some(CALDAV), None));
    let personal = config.calendar("personal").unwrap();
    assert_eq!((personal.kind.as_deref(), personal.url.as_deref()), (Some("feed"), None));
    assert_eq!(personal.address_cmd.as_deref(), Some(&["pass".to_string(), "show".into(), "cal".into()][..]));
    let bare = Config::parse(&format!("{TWO}[[calendar]]\naccount = \"personal\"\nkind = \"feed\"\n")).unwrap();
    assert!(bare.calendar("personal").unwrap().address_cmd.is_none() && bare.calendar("work").is_none());
}

/// Each way a `[[calendar]]` can be wrong, named with the key it is
/// about; and a feed's address, given where it does not belong, is not
/// repeated.
#[test]
fn a_calendar_table_that_is_wrong_says_which_key_and_never_quotes_a_feed_s_address() {
    let secret = "https://calendar.example.net/api/calendar/v1/url/SECRET-TOKEN/calendar.ics";
    let table = |keys: &str| format!("{TWO}[[calendar]]\n{keys}\n");
    let work = "account = \"work\"\n";
    for (keys, said) in [
        (
            "account = \"nobody\"\nkind = \"feed\"".to_string(),
            "[[calendar]] account = \"nobody\": there is no [[account]] of that name".to_string(),
        ),
        (format!("{work}kind = \"ics\""), "[[calendar]] of work: `kind` is one of \"caldav\", \"feed\"".into()),
        (format!("{work}kind = \"caldav\""), "[[calendar]] of work: kind = \"caldav\" needs a `url`".into()),
        (
            format!("{work}kind = \"caldav\"\nurl = \"http://dav.example.org/cal/\""),
            "[[calendar]] of work: `url` is fetched over TLS only, so it begins https://".into(),
        ),
        (
            format!("{work}kind = \"caldav\"\nurl = \"https://ada:pw@dav.example.org/cal/\""),
            "[[calendar]] of work: `url` cannot carry a user name".into(),
        ),
        (
            format!("{work}kind = \"caldav\"\nurl = \"https:///cal/\""),
            "[[calendar]] of work: `url` names no host".into(),
        ),
        (
            format!("{work}kind = \"caldav\"\nurl = \"{CALDAV}\"\naddress_cmd = [\"pass\"]"),
            "[[calendar]] of work: `address_cmd` is a feed's".into(),
        ),
        (format!("{work}kind = \"feed\"\nurl = \"{secret}\""), "[[calendar]] of work: a feed has no `url`".into()),
        (format!("{work}kind = \"feed\"\naddress_cmd = []"), "[[calendar]] of work: `address_cmd` names no".into()),
        (format!("{work}kind = \"feed\"\naddress = \"{secret}\""), "line 14: unknown key `address`".into()),
        (format!("{work}kind = \"feed\"\naddress_cmd = \"{secret}\""), "line 14: expected a sequence".into()),
        (
            format!("{work}kind = \"feed\"\n\n[[calendar]]\n{work}kind = \"feed\""),
            "account work has two [[calendar]] tables, and an account has one calendar".into(),
        ),
    ] {
        let error = error(&table(&keys));
        assert!(error.starts_with(&said), "{keys}\n{error}");
        assert!(!error.contains("SECRET-TOKEN") && !error.contains("calendar.example.net"), "{error}");
    }
    let google = TWO.replace("address = \"ada@example.org\"", "address = \"ada@example.org\"\nauth = \"gmail-oauth\"");
    let error = error(&format!("{google}[[calendar]]\n{work}kind = \"caldav\"\nurl = \"{CALDAV}\"\n"));
    assert!(
        error.contains("work signs in with Google (auth = \"gmail-oauth\"); kind = \"feed\" is the way"),
        "{error}"
    );
}

/// ADR-062: a calendar is not written on a path nobody has tried, so the
/// key that says it is written is false until a table says otherwise.
#[test]
fn a_caldav_calendar_is_written_only_where_a_table_says_so_and_a_feed_never() {
    let caldav = format!("{TWO}[[calendar]]\naccount = \"work\"\nkind = \"caldav\"\nurl = \"{CALDAV}\"\n");
    let read = Config::parse(&caldav).unwrap();
    assert!(!read.writes("work"), "left out, it is read and never written");
    assert!(read.unwritten("work") && !read.unwritten("personal"), "and is one a write could be tried on");
    let said = Config::parse(&format!("{caldav}write = true\n")).unwrap();
    assert!(said.writes("work") && !said.writes("personal"));
    assert!(!said.unwritten("work"), "which one already written is not");

    // What setup appends when the write was proven after the table was
    // written: it only ever adds to the end of the file (ADR-043).
    let later_text = format!("{caldav}\n[[calendar]]\naccount = \"work\"\nwrite = true\n");
    let later = Config::parse(&later_text).unwrap();
    assert!(later.writes("work"));
    assert_eq!(later.calendar("work").unwrap().url.as_deref(), Some(CALDAV), "the calendar is the table with a kind");
    let with_servers = later_text.replace(
        "address = \"ada@example.org\"",
        "address = \"ada@example.org\"\nimap = { host = \"imap.example.org\", port = 993 }",
    );
    let read = crate::sync::calendars(&Config::parse(&with_servers).unwrap(), None).unwrap();
    assert_eq!(read.len(), 1, "and is one calendar to read, not two");
    let off = format!("{caldav}write = true\n\n[[calendar]]\naccount = \"work\"\nwrite = false\n");
    assert!(!Config::parse(&off).unwrap().writes("work"), "the last table to say so decides");

    let feed = format!("{TWO}[[calendar]]\naccount = \"work\"\nkind = \"feed\"\n");
    let only_read = "[[calendar]] of work: `write` is for kind = \"caldav\"; a feed is only ever read";
    let alone = "[[calendar]] of work: a table with no `kind` adds to the one above it, and holds `write` alone";
    for (text, said) in [
        (format!("{feed}write = true\n"), only_read),
        (format!("{feed}\n[[calendar]]\naccount = \"work\"\nwrite = true\n"), only_read),
        (
            format!("{TWO}[[calendar]]\naccount = \"work\"\nwrite = true\n"),
            "[[calendar]] of work: it has no `kind`, and no table above it to add to",
        ),
        (format!("{caldav}\n[[calendar]]\naccount = \"work\"\nurl = \"{CALDAV}\"\nwrite = true\n"), alone),
        (format!("{caldav}\n[[calendar]]\naccount = \"work\"\n"), alone),
    ] {
        assert_eq!(error(&text), said, "{text}");
    }

    // Plain HTTP is for a server on this machine, which is all the engine
    // sends it to; a host that only begins like one is not that.
    assert!(Config::parse(&caldav.replace(CALDAV, "http://127.0.0.1:5232/ada/calendar/")).is_ok());
    let elsewhere = error(&caldav.replace(CALDAV, "http://127.0.0.1.example.org/ada/calendar/"));
    assert!(elsewhere.starts_with("[[calendar]] of work: `url` is fetched over TLS only"), "{elsewhere}");
}

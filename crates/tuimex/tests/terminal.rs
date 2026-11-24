//! The binary on a real pseudo-terminal: what it does to the terminal, and
//! what it leaves on disk.

// A harness that cannot start should fail loudly, which is what unwrap
// does; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ENTER_SCREEN: &str = "\x1b[?1049h";
const LEAVE_SCREEN: &str = "\x1b[?1049l";
const BEGIN_FRAME: &str = "\x1b[?2026h";
const END_FRAME: &str = "\x1b[?2026l";

/// A home of this test's own under the build directory, so the run cannot
/// see or write the user's config, mail or log.
fn scratch_home(test: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    if home.exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    fs::create_dir_all(&home).unwrap();
    home
}

fn tuimex(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tuimex"));
    isolate(&mut cmd, home);
    cmd
}

/// Nothing of the caller's environment reaches the program but the path
/// the harness itself is found on, and the keyring is off (ADR-053). The locale is set because Python gives
/// a child with none a UTF-8 one, and a test that wants a terminal
/// promising nothing would not get it.
fn isolate(cmd: &mut Command, home: &Path) {
    cmd.env_clear().env("HOME", home).env("TERM", "xterm-256color").env("PATH", std::env::var_os("PATH").unwrap());
    cmd.env("LC_ALL", "C").env("TUIMEX_KEYRING", "none");
}

/// `q` is not a key: only the command leaves.
const QUIT: &str = ":q\r";

fn on_a_pty(home: &Path, keys: &str, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new("python3");
    isolate(&mut cmd, home);
    cmd.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/on_pty.py")).arg(keys).arg(env!("CARGO_BIN_EXE_tuimex"));
    let out = cmd.args(args).envs(env.iter().copied()).output().expect("python3 runs the pty harness");
    // The program's own stderr is the terminal; this one is the harness's.
    assert!(out.stderr.is_empty(), "the pty harness failed: {}", String::from_utf8_lossy(&out.stderr));
    out
}

/// What a terminal would show: the output with its control sequences and
/// control characters removed.
fn visible(out: &str) -> String {
    let mut text = String::new();
    let mut chars = out.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') {
                chars.by_ref().find(|c| ('\x40'..='\x7e').contains(c));
            }
        } else if !c.is_control() {
            text.push(c);
        }
    }
    text
}

fn modes_under(dir: &Path, out: &mut Vec<(PathBuf, u32)>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let meta = entry.metadata().unwrap();
        out.push((entry.path(), meta.permissions().mode() & 0o777));
        if meta.is_dir() {
            modes_under(&entry.path(), out);
        }
    }
}

#[test]
fn fixtures_opens_the_list_and_the_quit_command_leaves_the_terminal_as_it_was() {
    let home = scratch_home("list");
    let out = on_a_pty(&home, QUIT, &["--fixtures"], &[]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), ":q exits cleanly; the terminal showed {screen:?}");
    let entered = screen.find(ENTER_SCREEN).expect("the alternate screen is entered");
    let left = screen.rfind(LEAVE_SCREEN).expect("the alternate screen is left");
    assert!(entered < left);
    let shown = visible(&screen[entered..left]);
    for words in [
        "Inbox 9   Unread   Flagged 3   Drafts   Sent   Archive   Outbox 1",
        "Today",
        ">   work  Imani Castell",
        "Invitation: Q4 roadmap re~         inv   14:05",
        "Interview loop: backend c~          1f  21 Nov",
        "1 of 24 - 1 message",
        "sending in 6s - 1 not sent",
        "Enter open - e archive - d trash - r reply - ? help",
    ] {
        assert!(shown.contains(words), "the list at 80x24 shows {words:?}; the terminal showed {shown:?}");
    }
    assert_eq!(visible(&screen[left..]).trim(), "", "nothing is left on the terminal after :q");
}

/// The 80 by 24 screen as it stood at the end of each frame. Only what a
/// frame changed is written to the terminal, so the screen is kept as a
/// terminal keeps it: text goes where the cursor was last put, and every
/// other sequence is colour or mode.
fn frames(out: &str) -> Vec<String> {
    let mut grid = vec![vec![' '; 80]; 24];
    let (mut row, mut col) = (0, 0);
    let mut frames = Vec::new();
    let mut chars = out.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() != Some('[') {
                continue;
            }
            let sequence: String = chars.by_ref().take_while(|c| !('\x40'..='\x7e').contains(c)).collect();
            let rest = &out[out.len() - chars.as_str().len() - 1..];
            match rest.chars().next() {
                Some('H') => {
                    let mut at = sequence.split(';').map(|n| n.parse::<usize>().unwrap_or(1) - 1);
                    (row, col) = (at.next().unwrap_or(0), at.next().unwrap_or(0));
                }
                Some('J') => grid.iter_mut().for_each(|line| line.fill(' ')),
                Some('l') if sequence == "?2026" => {
                    frames.push(
                        grid.iter().map(|line| line.iter().collect::<String>().trim_end().to_string() + "\n").collect(),
                    );
                }
                _ => {}
            }
        } else if !c.is_control() && row < 24 && col < 80 {
            grid[row][col] = c;
            col += 1;
        }
    }
    frames
}

/// PLAN §10 M7: `tuimex --fixtures` is usable by hand. One short session,
/// a key to a frame: down, mark two, archive, undo, filter, `:trash`, help,
/// and out.
#[test]
fn a_session_of_keys_moves_marks_archives_undoes_finds_searches_trashes_and_shows_help() {
    let keys = "j, ,j, ,e,u,/,n,o,v,\r,\x1b,:,t,r,a,s,h,\r,?,\x1b,:q\r";
    let out = on_a_pty(&scratch_home("session"), keys, &["--fixtures"], &[("ON_PTY_STEPS", ",")]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), ":q exits cleanly after Esc closes help; the terminal showed {screen:?}");
    let frames = frames(&screen);
    let shows = |words: &str| frames.iter().position(|frame| frame.contains(words));
    let steps = [
        "1 of 24 - 1 message",
        "2 of 24 - thread of 5",
        "2 of 24 - 1 marked",
        "3 of 24 - 2 marked",
        "+ Archived 6 messages - u to undo",
        "+ Undone - 6 messages back in Inbox",
        "/nov",
        "3 here - Enter searches all mail",
        "Archive   Outbox 1   Search nov",
        "1 of 6 - 1 message",
        ":trash",
        "+ Trashed 1 message - u to undo",
        "140 keys and commands - lines 1-19 of 70",
    ];
    let mut last = 0;
    for words in steps {
        let at = shows(words).unwrap_or_else(|| panic!("no frame shows {words:?}; the frames were {frames:#?}"));
        assert!(at >= last, "{words:?} was shown before the step before it");
        last = at;
    }
    let after_archive = &frames[shows("+ Archived 6 messages").unwrap()];
    assert!(after_archive.contains("1 of 22 - 1 message"), "the two marked rows left the list: {after_archive:?}");
    let after_help = frames.last().unwrap();
    assert!(!after_help.contains("Help"), "nothing of the help is left after :q: {after_help:?}");
}

/// ADR-054: the calendar is a place beside mail in the real binary. One
/// session: `gc` opens it on the calendars `--fixtures` hands over, three
/// days on is the design's Thursday, and `gm` is back in mail where it was.
#[test]
fn a_session_in_the_calendar_moves_by_days_and_returns_to_mail() {
    let keys = "j,gc,l,l,l,gm,:q\r";
    let out = on_a_pty(&scratch_home("calendar"), keys, &["--fixtures"], &[("ON_PTY_STEPS", ",")]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), ":q leaves from the list; the terminal showed {screen:?}");
    let frames = frames(&screen);
    let steps: [&[&str]; 4] = [
        &["2 of 24 - thread of 5"],
        &["Month   Week   Day", "November 2026", "Mon 23 Nov - 2 events", "Halden contract call"],
        &["Thu 26 Nov - 4 events - 1 to answer", "Q4 roadm~   needs answer", "gm mail - ? help"],
        &["Inbox 9", "2 of 24 - thread of 5"],
    ];
    let mut from = 0;
    for words in steps {
        let shows = |frame: &String| words.iter().all(|w| frame.contains(w));
        let at = frames[from..].iter().position(shows);
        from += at.unwrap_or_else(|| panic!("no frame after {from} shows {words:?}: {frames:#?}")) + 1;
    }
}

/// ADR-054, ADR-056: over a real store the calendar kept on disk is on
/// screen behind `gc` whatever its provider says, and a provider that
/// cannot be reached is said on the status line with how old that is. The
/// feed's address is one nothing listens at, so the one fetch is refused
/// by this machine and nothing leaves it; every step waits for what it
/// names and for no length of time.
#[test]
fn a_calendar_kept_on_disk_is_shown_and_one_that_cannot_be_fetched_says_so() {
    let home = scratch_home("calendar-kept");
    let (data, cache) = (home.join(".local/share/tuimex"), home.join(".cache/tuimex"));
    let store = tuimex_store::Store::open(&data, &cache, tuimex_engine::parse).unwrap();
    store.ensure_folder("work", "INBOX", tuimex_core::Role::Inbox).unwrap();
    drop(store);
    // Every day at noon, so that it is on today whenever this runs.
    let feed = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:kept@example.org\r\n\
                DTSTART:20200101T120000Z\r\nDTEND:20200101T123000Z\r\nRRULE:FREQ=DAILY\r\nSUMMARY:Kept on disk\r\n\
                END:VEVENT\r\nEND:VCALENDAR\r\n";
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let calendars = tuimex_store::calendar::Calendars::open(&data, &cache).unwrap();
    calendars.put("work", tuimex_engine::calendar::FEED, "", None, feed).unwrap();
    calendars.read("work", tuimex_engine::calendar::FEED, None, None, i64::try_from(now).unwrap() - 2 * 3_600).unwrap();
    drop(calendars);
    // No notifier: a reminder for the event must reach no desktop.
    let config = "[notify]\nvia = \"none\"\n\n[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n\
                  auth = \"command\"\npassword_cmd = [\"true\"]\nimap = { host = \"127.0.0.1\", port = 1 }\n\
                  smtp = { host = \"127.0.0.1\", port = 1 }\n\n[[calendar]]\naccount = \"work\"\nkind = \"feed\"\n\
                  address_cmd = [\"echo\", \"https://127.0.0.1:1/private/feed.ics\"]\n";
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    fs::write(home.join(".config/tuimex/config.toml"), config).unwrap();
    let steps = [
        "wait Inbox",
        "keys gc",
        "wait Kept on disk",
        "wait work's calendar not read - last read 2h ago",
        // The events held stay beside the failure.
        "wait Kept on disk",
        // `:q` is back to mail from the calendar, and out from the list.
        "keys :q\\r",
        "wait Nothing here.",
        "keys :q\\r",
    ];
    let script = home.join("steps");
    fs::write(&script, steps.join("\n") + "\n").unwrap();
    // Wide enough that the mail account, which has no server either, does
    // not push the calendar's state off the status line.
    let env = [("ON_PTY_SCRIPT", script.to_str().unwrap()), ("ON_PTY_SIZE", "40x220"), ("ON_PTY_STEP_TIMEOUT", "20")];
    let out = on_a_pty(&home, "", &[], &env);
    let screen = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "every step was so, and :q left: {screen}");
    assert!(!screen.contains("private"), "ADR-056: nothing of the feed's address is on the screen");
}

/// A home with one account, `work`, whose servers nothing listens at, so
/// that nothing leaves this machine and a message sent waits in the
/// Outbox for as long as the test runs. No notifier: a reminder must
/// reach no desktop. Returns the home, and where its local events are.
fn calendar_home(test: &str) -> (PathBuf, PathBuf) {
    let home = scratch_home(test);
    let (data, cache) = (home.join(".local/share/tuimex"), home.join(".cache/tuimex"));
    let store = tuimex_store::Store::open(&data, &cache, tuimex_engine::parse).unwrap();
    store.ensure_folder("work", "INBOX", tuimex_core::Role::Inbox).unwrap();
    drop(store);
    let config = "[notify]\nvia = \"none\"\n\n[send]\ndelay = \"10m\"\n\n[[account]]\nname = \"work\"\n\
                  address = \"ada@example.org\"\nauth = \"command\"\npassword_cmd = [\"true\"]\n\
                  imap = { host = \"127.0.0.1\", port = 1 }\nsmtp = { host = \"127.0.0.1\", port = 1 }\n";
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    fs::write(home.join(".config/tuimex/config.toml"), config).unwrap();
    (home, data.join(".calendars/work"))
}

/// Runs tuimex in `home` through `steps`, each of which waits for what it
/// names and for no length of time, and returns what the terminal showed.
fn calendar_session(home: &Path, steps: &[String]) -> String {
    let script = home.join("steps");
    fs::write(&script, steps.join("\n") + "\n").unwrap();
    let env = [("ON_PTY_SCRIPT", script.to_str().unwrap()), ("ON_PTY_SIZE", "40x220"), ("ON_PTY_STEP_TIMEOUT", "20")];
    let out = on_a_pty(home, "", &[], &env);
    let screen = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(0), "every step was so, and :q left: {screen}");
    screen
}

/// ADR-057, by hand: an invitation that is in mail alone is on the
/// calendar; `i a` answers it; the row no longer waits, the event is a
/// file of the account's local calendar, and the reply is in the Outbox;
/// `u` takes the entry and the reply back.
#[test]
fn an_invitation_is_answered_in_the_calendar_its_reply_waits_in_the_outbox_and_u_takes_both_back() {
    let (home, events) = calendar_home("calendar-answer");
    // Two days from now, so that `I` reaches it whenever this runs.
    let day = (chrono::Utc::now() + chrono::Duration::days(2)).format("%Y%m%d").to_string();
    let invitation = format!(
        "From: Lars Holm <lars@example.com>\r\nTo: ada@example.org\r\nSubject: Invitation: Planning\r\n\
         Message-ID: <planning-1@example.com>\r\nDate: Mon, 23 Nov 2026 10:00:00 +0000\r\nMIME-Version: 1.0\r\n\
         Content-Type: text/calendar; method=REQUEST; charset=utf-8\r\n\r\n\
         BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:planning@example.com\r\n\
         SEQUENCE:0\r\nDTSTART:{day}T100000Z\r\nDTEND:{day}T110000Z\r\nSUMMARY:Planning\r\n\
         ORGANIZER;CN=Lars Holm:mailto:lars@example.com\r\n\
         ATTENDEE;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:ada@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let (data, cache) = (home.join(".local/share/tuimex"), home.join(".cache/tuimex"));
    let store = tuimex_store::Store::open(&data, &cache, tuimex_engine::parse).unwrap();
    let delivery = tuimex_store::maildir::Delivery {
        key: tuimex_store::maildir::new_key(),
        uid: Some(1),
        origin: tuimex_core::Origin::Server,
        flags: tuimex_core::Flags::default(),
    };
    store.deliver("work", "INBOX", &delivery, invitation.as_bytes()).unwrap();
    drop(store);
    let (kept, none) = (format!("{}/*.ics", events.display()), format!("test -z \"$(ls {})\"", events.display()));
    let steps = [
        "wait Invitation: Planning".to_string(),
        // The day view lists the days ahead, so the invitation is on it
        // once the calendar has been read; `I` then selects it.
        "keys gc3".to_string(),
        "wait needs answer".to_string(),
        format!("run test ! -e {}", events.display()),
        "keys Iia".to_string(),
        "wait Accepted Planning".to_string(),
        "wait in work, local".to_string(),
        "gone needs answer".to_string(),
        format!("until grep -qs PARTSTAT=ACCEPTED {kept}"),
        // The reply is mail: the Outbox is the seventh section.
        "keys gm".to_string(),
        "wait Archive   Outbox".to_string(),
        "keys llllll".to_string(),
        "wait Accepted: Planning".to_string(),
        "keys hhhhhh".to_string(),
        "keys gcu".to_string(),
        "wait Undone".to_string(),
        "wait needs answer".to_string(),
        format!("until {none}"),
        "keys gm".to_string(),
        "gone Outbox".to_string(),
        "keys :q\\r".to_string(),
    ];
    calendar_session(&home, &steps);
}

/// ADR-055, by hand: an event made with the form is a file here, is on
/// the calendar of a later run, and is deleted and brought back with `u`.
#[test]
fn an_event_made_with_the_form_is_there_after_a_restart_and_is_deleted_and_brought_back() {
    let (home, events) = calendar_home("calendar-form");
    let (kept, none) = (format!("{}/*.ics", events.display()), format!("test -z \"$(ls {})\"", events.display()));
    let made = [
        "wait Nothing here.".to_string(),
        "keys gcc".to_string(),
        "wait Title".to_string(),
        // The title, then `^s` to save.
        "keys Dentist".to_string(),
        "keys \\x13".to_string(),
        "wait Added Dentist to work, local".to_string(),
        format!("until grep -qs SUMMARY:Dentist {kept}"),
        "keys gm".to_string(),
        "wait Nothing here.".to_string(),
        "keys :q\\r".to_string(),
    ];
    calendar_session(&home, &made);
    let file = fs::read_dir(&events).unwrap().map(|entry| entry.unwrap().path()).next().expect("an event's file");
    assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "ADR-011: the owner's alone");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("@tuimex\r\n") && text.contains("SUMMARY:Dentist\r\n"), "{text}");

    let again = [
        "wait Nothing here.".to_string(),
        // Found on whichever day it was made for; `n` selects the match.
        "keys gc/Dentist\\r".to_string(),
        "wait 1 event matches".to_string(),
        "wait local".to_string(),
        "keys nd".to_string(),
        "wait Deleted Dentist".to_string(),
        format!("until {none}"),
        "keys u".to_string(),
        "wait Undone".to_string(),
        format!("until grep -qs SUMMARY:Dentist {kept}"),
        "keys gm".to_string(),
        "wait Nothing here.".to_string(),
        "keys :q\\r".to_string(),
    ];
    calendar_session(&home, &again);
}

/// PLAN §10 M8: the reader is usable by hand. One session: open a thread's
/// newest message, back to its first, scroll, list its files, return to
/// the list with the cursor kept; then open the bank's message, list its
/// links, choose the one whose text names another host, and decline.
#[test]
fn a_session_in_the_reader_scrolls_lists_files_and_confirms_a_deceptive_link() {
    let keys = "j,j,j,\r,b,b,j,a,\x1b,\x1b,j,j,j,\r,o,1,n,\x1b,:q\r";
    let out = on_a_pty(&scratch_home("reader"), keys, &["--fixtures"], &[("ON_PTY_STEPS", ",")]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), ":q leaves from the list; the terminal showed {screen:?}");
    let frames = frames(&screen);
    let steps: [&[&str]; 6] = [
        &["Contract draft v3 for the Halden account", "1 of 3 in the thread", "lines 2-12 of 30"],
        &["Files", " 1  contract-v3.pdf    482 KB   application/pdf", "Enter save - O open - Esc close"],
        &["4 of 24 - thread of 3", "Enter open - zo unfold"],
        &["Links", " 1  bank.example.com.device-check.example.net   https://bank.example.com/co~"],
        &["^ Text says bank.example.com;", "example.net. Open?  y yes - n no"],
        &["7 of 24 - 1 message"],
    ];
    // Each step is looked for after the one before it: the list is on
    // the screen both before a message is opened and after it is left.
    let mut seen = Vec::new();
    for words in steps {
        let from = seen.last().map_or(0, |at| at + 1);
        let shows = |frame: &String| words.iter().all(|w| frame.contains(w));
        let at = frames[from..].iter().position(shows);
        seen.push(from + at.unwrap_or_else(|| panic!("no frame after {from} shows {words:?}: {frames:#?}")));
    }
    let declined = &frames[seen[4] + 1];
    assert!(declined.contains("o links - Esc back") && !declined.contains("Text says"), "after n: {declined:?}");
    assert!(!screen.contains("--fixtures opens nothing"), "declining opened nothing, and asked for nothing");
}

/// The reader's rulings of design 8b1b21d, by hand: `a` over a link asks
/// the agent, and its advice joins the question; Tab after `:agent `
/// offers its three functions; `B` on a message with remote pictures asks
/// first. Each line waits for what the terminal shows, so nothing here
/// depends on how long a frame takes.
#[test]
fn a_session_asks_the_agent_about_a_link_completes_agent_and_asks_before_pictures() {
    let home = scratch_home("reader-rulings");
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    let agent = "cat > /dev/null; sleep 1; echo 'Against opening: the host is not the bank own'";
    let config = format!("[agent]\ncommand = [\"sh\", \"-c\", \"{agent}\"]\n");
    fs::write(home.join(".config/tuimex/config.toml"), config).unwrap();
    let hosts = "img.fernway.example.net, cdn.example.net";
    let script = [
        "wait Enter open",
        "keys jjjjjj",
        "wait 7 of 24",
        "keys \\r",
        "wait o links - Esc back",
        "keys o",
        "wait 1-9 open - j k Enter choose - A agent - Esc close",
        "keys A",
        "wait l   review this link",
        "wait nothing is sent until you pick",
        "keys l",
        "wait % Asking the agent - it reads the address and the message, and opens nothing",
        "wait device-check.example.net. Open?  y yes - n no",
        "wait % Agent advises against opening - the host is not the bank own",
        "wait Advice only. The agent did not open the link.",
        "keys n",
        "gone Agent advises",
        "wait o links - Esc back",
        "keys :q\\r",
        "wait 7 of 24",
        "keys :agent \\t",
        "wait reply [guidance]        write the body from the thread",
        "wait tidy                    clean up the body you wrote",
        "wait link <n>                ask about a link; the agent opens nothing",
        "wait :agent reply",
        "keys \\x1b",
        "gone write the body from the thread",
        "keys kk\\r",
        "wait Remote content not loaded - 9 images, 3 hosts",
        "keys B",
        &format!("wait Load 8 images from {hosts}?  y yes - n no"),
        "keys n",
        // `--fixtures` holds no page to write, and says so in place of
        // opening one: the copy was asked for, and nothing was loaded.
        "wait E :browser: this mailbox holds no pages",
        "keys :q\\r",
        "wait 5 of 24",
        "keys :q\\r",
    ];
    let file = home.join("script");
    fs::write(&file, script.join("\n")).unwrap();
    let env = [("ON_PTY_SCRIPT", file.to_str().unwrap()), ("ON_PTY_SIZE", "40x120")];
    let out = on_a_pty(&home, "", &["--fixtures"], &env);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
    assert!(!screen.contains("--fixtures opens nothing"), "nothing was handed to an opener");
}

/// ADR-031: the question `:agent link` asks is on the screen before the
/// agent has answered, and the advice joins it without a key being
/// pressed. A line of shell stands for the agent, keeps what it was handed
/// and takes a second to answer; no test starts a real one.
#[test]
fn the_agent_s_advice_arrives_under_a_question_already_asked() {
    let home = scratch_home("agent-link");
    let script = "cat > ../../../agent-saw; sleep 1; echo 'Against opening: the host is not the bank own'; echo more";
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    let config = format!("[agent]\ncommand = [\"sh\", \"-c\", \"{script}\"]\n");
    fs::write(home.join(".config/tuimex/config.toml"), config).unwrap();
    let keys = "jjjjjj\r,:agent link 1\r,n,\x1b,:q\r";
    let env = [("ON_PTY_STEPS", ","), ("ON_PTY_PAUSE", "2")];
    let out = on_a_pty(&home, keys, &["--fixtures"], &env);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
    let frames = frames(&screen);
    let question = "^ Text says bank.example.com;";
    let asking = frames.iter().position(|f| f.contains("% Asking the agent") && f.contains(question));
    let advice = "% Agent advises against opening - the host is not the bank own";
    let advised = frames.iter().position(|f| f.contains(advice) && f.contains(question));
    assert!(asking.is_some() && asking < advised, "asking at {asking:?}, advice at {advised:?}: {frames:#?}");
    assert!(frames[advised.unwrap()].contains("Advice only. The agent did not open the link."));
    assert!(frames[asking.unwrap()].contains("  agent saw "), "what left is said: {:?}", frames[asking.unwrap()]);
    assert!(!screen.contains("--fixtures opens nothing"), "n opened nothing");

    let saw = fs::read_to_string(home.join("agent-saw")).expect("the command was handed a prompt");
    let link = "The link opens: https://bank.example.com.device-check.example.net/confirm\nIt is shown as: https://bank.example.com/confirm\n";
    assert!(saw.contains(link), "the link's address and the text it was shown under: {saw}");
    assert!(saw.contains("found: its text names a different host from the one it opens"), "{saw}");
    assert!(saw.contains("confirm the device within 24 hours") && saw.contains("[person-1] Security Team"), "{saw}");
    assert!(!saw.contains("Example Bank") && !saw.contains("Robin"), "a name from the headers left the machine: {saw}");
}

/// Keys typed while tuimex starts are in the terminal's queue before it is
/// in raw mode, where Enter is a line feed. They are read in order once it
/// is up, and the Enter opens the message the six `j` moved to.
#[test]
fn keys_typed_before_the_first_frame_are_all_read_in_order_and_enter_is_enter() {
    let home = scratch_home("typed-early");
    let script = home.join("steps");
    fs::write(&script, "keys jjjjjj\\r\nwait Action needed: confirm your new device\nwait o links\nkeys :qa\\r\n")
        .unwrap();
    let env = [("ON_PTY_SCRIPT", script.to_str().unwrap()), ("ON_PTY_SIZE", "40x120"), ("ON_PTY_STEP_TIMEOUT", "10")];
    let out = on_a_pty(&home, "", &["--fixtures"], &env);
    assert_eq!(out.status.code(), Some(0), "the reader opened on the seventh row, and :qa left it and tuimex");
}

/// `^l` clears and draws the screen again, on a terminal that answers no
/// question about its cursor too.
#[test]
fn ctrl_l_redraws_without_asking_the_terminal_anything() {
    let out = on_a_pty(&scratch_home("redraw"), "j,\x0c,:q\r", &["--fixtures"], &[("ON_PTY_STEPS", ",")]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
    assert!(!screen.contains("\x1b[6n"), "the cursor's position is never asked for");
    let cleared = screen.rfind("\x1b[2J").expect("the screen is cleared");
    let after = frames(&screen[cleared..]);
    let whole = after.first().expect("a frame after the clear");
    for words in ["Inbox 9   Unread", ">   pers  Jonas Becker", "2 of 24 - thread of 5", "Enter open - zo unfold"] {
        assert!(whole.contains(words), "the frame after ^l is drawn whole: {words:?} is not in {whole:?}");
    }
}

/// Before there is a list to draw the screen says so: an index of an
/// older version, or none, is rebuilt from the Maildir before the first
/// row can be read, and the count is on the screen from the first moment.
#[test]
fn an_index_to_rebuild_is_counted_on_the_screen_before_the_list_is_drawn() {
    let home = scratch_home("loading");
    let (data, cache) = (home.join(".local/share/tuimex"), home.join(".cache/tuimex"));
    let store = tuimex_store::Store::open(&data, &cache, tuimex_engine::parse).unwrap();
    store.ensure_folder("work", "INBOX", tuimex_core::Role::Inbox).unwrap();
    for n in 1..=3 {
        let raw = format!(
            "From: Grace Hopper <grace@example.net>\r\nTo: ada@example.org\r\nSubject: Kept message {n}\r\nDate: Mon, \
             23 Nov 2026 10:0{n}:00 +0000\r\n\r\nThe body of message {n}.\r\n"
        );
        let (origin, flags) = (tuimex_core::Origin::Server, tuimex_core::Flags::default());
        let delivery = tuimex_store::maildir::Delivery { key: format!("k{n}"), uid: Some(n), origin, flags };
        store.deliver("work", "INBOX", &delivery, raw.as_bytes()).unwrap();
    }
    drop(store);
    fs::remove_dir_all(&cache).unwrap();
    // A server nothing listens at: the mail on disk is what is shown.
    let config = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\nauth = \"command\"\n\
                  password_cmd = [\"true\"]\nimap = { host = \"127.0.0.1\", port = 1 }\n\
                  smtp = { host = \"127.0.0.1\", port = 1 }\n";
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    fs::write(home.join(".config/tuimex/config.toml"), config).unwrap();
    let out = on_a_pty(&home, QUIT, &[], &[]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
    let shown = visible(&screen);
    let at = |words: &str| shown.find(words).unwrap_or_else(|| panic!("{words:?} is not in {shown:?}"));
    assert!(at("Opening mail") < at("Rebuilding the index - 0 of 3"));
    assert!(at("Rebuilding the index - 0 of 3") < at("Kept message 3"), "and then the list, from the index it made");
}

/// A sequence's first key waits half a second for its second and is then
/// dropped, so the `g` after the wait starts a sequence of its own.
#[test]
fn the_first_key_of_a_sequence_is_dropped_when_the_second_does_not_come() {
    let home = scratch_home("pending");
    let out = on_a_pty(&home, "G,g,g,:q\r", &["--fixtures"], &[("ON_PTY_STEPS", ","), ("ON_PTY_PAUSE", "0.8")]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
    let last = frames(&screen).into_iter().rev().find(|frame| frame.contains(" of 24 - ")).unwrap();
    assert!(last.contains("24 of 24 - 1 message"), "the cursor is still on the last row: {last:?}");
}

/// A terminal that has no Alt of its own sends Esc and then the letter.
/// The two written together are the chord, as a terminal writes them, and
/// focus the preview; an Esc that arrives alone is Esc, which goes back,
/// and the letter after it is only the letter.
#[test]
fn an_escape_and_a_letter_together_are_alt_and_apart_they_are_esc_then_the_letter() {
    let home = scratch_home("alt");
    // Each `keys` line is one write to the terminal, and each `wait` is
    // for what the frame after it shows.
    let steps = [
        "keys jjjzo",
        "wait 4 of 26 - 1 message of the thread",
        "keys \\x1bj",
        "wait lines 1-15 of 37",
        "wait j k scroll - Alt-k list - Enter open - ? help",
        "keys j",
        "wait lines 2-16 of 37",
        "keys \\x1b",
        "wait 4 of 26 - 1 message of the thread",
        "keys j",
        "wait 5 of 26 - 1 message of the thread",
        // `q` is no key: the cursor and the screen stay as they are.
        "keys q",
        "keys \\x1bl",
        "wait lines 1-",
        "keys \\x1bk",
        "wait 5 of 26 - 1 message of the thread",
        "keys :q\\r",
    ];
    let script = home.join("steps");
    fs::write(&script, steps.join("\n") + "\n").unwrap();
    let env = [("ON_PTY_SCRIPT", script.to_str().unwrap()), ("ON_PTY_SIZE", "40x120")];
    let out = on_a_pty(&home, "", &["--fixtures"], &env);
    assert_eq!(out.status.code(), Some(0), "every step was so, and :q left: {}", String::from_utf8_lossy(&out.stdout));
}

/// PLAN §10 M6: frames in synchronised output.
#[test]
fn every_frame_is_one_synchronised_update() {
    let out = on_a_pty(&scratch_home("frames"), QUIT, &["--fixtures"], &[]);
    let screen = String::from_utf8(out.stdout).unwrap();
    let begin = screen.find(BEGIN_FRAME).expect("a synchronised update begins");
    let end = screen.find(END_FRAME).expect("a synchronised update ends");
    let drawn = screen.find("Inbox").expect("the list is drawn");
    assert!(begin < drawn && drawn < end, "the frame is drawn inside the update");
    assert_eq!(screen.matches(BEGIN_FRAME).count(), screen.matches(END_FRAME).count());
}

/// PLAN §10 M6: resize is live. The preview needs thirty rows, so it is
/// absent at 80x24 and present once the terminal has grown.
#[test]
fn a_resize_redraws_the_list_at_the_new_size() {
    let out = on_a_pty(&scratch_home("resize"), QUIT, &["--fixtures"], &[("ON_PTY_RESIZE", "40x120")]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
    let first = screen.find(END_FRAME).unwrap() + END_FRAME.len();
    let (before, after) = (visible(&screen[..first]), visible(&screen[first..]));
    let preview = "From     Imani Castell <imani@example.org>";
    assert!(before.contains("Imani Castell") && !before.contains(preview), "80x24 has no preview: {before:?}");
    assert!(after.contains(preview), "120x40 shows the preview: {after:?}");
    assert!(after.contains("work       Imani Castell           Invitation: Q4 roadmap review, Thu 26 Nov 10:00                invite   14:05"));
}

/// The tiers are detected from the environment, and a flag overrides it.
#[test]
fn colours_and_glyphs_follow_the_environment_unless_a_flag_says_otherwise() {
    let rich = [("COLORTERM", "truecolor"), ("LC_ALL", "en_GB.UTF-8")];
    let run = |name: &str, args: &[&str], env: &[(&str, &str)]| {
        String::from_utf8(on_a_pty(&scratch_home(name), QUIT, args, env).stdout).unwrap()
    };
    // How crossterm writes the cursor in each tier: the accent as a colour
    // of its own, as an index into 256, and as magenta.
    let (rgb, indexed, named) = ("\x1b[38;2;245;170;77;", "\x1b[38;5;215;", "\x1b[38;5;5;");

    let detected = run("tier-rich", &["--fixtures"], &rich);
    assert!(detected.contains(&format!("{rgb}48;2;45;47;56m▌")), "truecolor and Unicode: {detected:?}");
    let plain = run("tier-plain", &["--fixtures"], &[]);
    assert!(plain.contains(&format!("{indexed}48;5;237m>")), "256 colours and ASCII: {plain:?}");
    let forced = run("tier-forced", &["--fixtures", "--colors", "16", "--glyphs", "ascii"], &rich);
    assert!(forced.contains(&format!("{named}49m>")), "the flags win, and the selected row has no fill: {forced:?}");
}

#[test]
fn the_log_goes_to_the_state_directory_and_never_to_the_terminal() {
    let home = scratch_home("log");
    let out = on_a_pty(&home, QUIT, &["--fixtures"], &[]);
    assert_eq!(out.status.code(), Some(0));
    let log = fs::read_to_string(home.join(".local/state/tuimex/tuimex.log")).unwrap();
    assert!(log.contains("start"), "the log records the start: {log:?}");
    let screen = String::from_utf8(out.stdout).unwrap();
    assert!(!screen.contains("start") && !screen.contains("INFO"), "a log line reached the terminal: {screen:?}");
}

/// ADR-011, over everything one run leaves behind.
#[test]
fn everything_a_run_creates_is_owner_only() {
    let home = scratch_home("modes");
    assert_eq!(on_a_pty(&home, QUIT, &["--fixtures"], &[]).status.code(), Some(0));
    let mut found = Vec::new();
    modes_under(&home, &mut found);
    assert!(found.iter().any(|(p, _)| p.ends_with("tuimex.log")), "the walk did not reach the log: {found:?}");
    for (path, mode) in found {
        let want = if path.is_dir() { 0o700 } else { 0o600 };
        assert_eq!(mode, want, "{} is {mode:o}", path.display());
    }
}

/// A Ghostty theme of invented colours, computed so that no theme's values
/// are written in the repository (ADR-003), and the colour it has for `n`.
fn invented_theme() -> (String, impl Fn(u8) -> (u8, u8, u8)) {
    let colour = |n: u8| {
        (n.wrapping_mul(41).wrapping_add(9), n.wrapping_mul(67).wrapping_add(3), n.wrapping_mul(89).wrapping_add(70))
    };
    let hex = |n: u8| {
        let (r, g, b) = colour(n);
        format!("#{r:02x}{g:02x}{b:02x}")
    };
    let mut theme = format!("background = {}\nforeground = {}\nselection-background = {}\n", hex(20), hex(21), hex(22));
    for n in 0..16 {
        theme += &format!("palette = {n}={}\n", hex(n));
    }
    (theme, colour)
}

/// ADR-026, end to end: a theme is imported into the config directory, and
/// the list is then drawn in its colours.
#[test]
fn an_imported_palette_is_owner_only_and_colours_the_list_once_the_config_names_it() {
    let home = scratch_home("palette");
    let (theme, colour) = invented_theme();
    fs::write(home.join("theme"), theme).unwrap();
    let out = tuimex(&home).args(["palette", "import"]).arg(home.join("theme")).output().unwrap();
    let said = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let config = home.join(".config/tuimex");
    let palette = config.join("palette.toml");
    assert!(said.contains(&format!("wrote {}", palette.display())), "{said}");
    assert!(said.contains(&format!("[ui]\npalette = \"{}\"", palette.display())), "{said}");
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&palette), mode(&config)), (0o600, 0o700));

    let again = tuimex(&home).args(["palette", "import"]).arg(home.join("theme")).output().unwrap();
    assert_eq!(again.status.code(), Some(1), "a palette already there is not written over");

    let truecolor = [("COLORTERM", "truecolor"), ("LC_ALL", "en_GB.UTF-8")];
    let cursor =
        |(r, g, b): (u8, u8, u8), (sr, sg, sb): (u8, u8, u8)| format!("\x1b[38;2;{r};{g};{b};48;2;{sr};{sg};{sb}m▌");
    let before = String::from_utf8(on_a_pty(&home, QUIT, &["--fixtures"], &truecolor).stdout).unwrap();
    assert!(!before.contains(&cursor(colour(5), colour(22))), "a palette no config names is not used");

    fs::write(config.join("config.toml"), "[ui]\npalette = \"~/.config/tuimex/palette.toml\"\n").unwrap();
    let after = String::from_utf8(on_a_pty(&home, QUIT, &["--fixtures"], &truecolor).stdout).unwrap();
    assert!(
        after.contains(&cursor(colour(5), colour(22))),
        "the cursor is the theme's magenta on its selection: {after:?}"
    );
}

#[test]
fn a_palette_the_config_names_and_cannot_be_read_stops_tuimex() {
    let home = scratch_home("palette-missing");
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    fs::write(home.join(".config/tuimex/config.toml"), "[ui]\npalette = \"absent.toml\"\n").unwrap();
    let out = tuimex(&home).arg("--fixtures").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let said = String::from_utf8(out.stderr).unwrap();
    assert!(said.contains(".config/tuimex/absent.toml"), "the path is read against the config directory: {said}");
    assert!(!String::from_utf8(out.stdout).unwrap().contains(ENTER_SCREEN));
}

/// The switch that makes the binary panic exists only in debug builds.
#[cfg(debug_assertions)]
#[test]
fn a_panic_restores_the_terminal_before_its_message_prints() {
    let home = scratch_home("panic");
    let out = on_a_pty(&home, "", &["--fixtures"], &[("TUIMEX_TEST_PANIC", "1")]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(101), "a panic's exit status; the terminal showed {screen:?}");
    let left = screen.find(LEAVE_SCREEN).expect("the alternate screen is left");
    let message = screen.find("TUIMEX_TEST_PANIC is set").expect("the panic message is shown");
    assert!(left < message, "the message printed into the alternate screen and was lost");
}

/// PLAN §11: never a demo in place of the user's mail.
#[test]
fn without_fixtures_it_says_why_it_stops_and_draws_nothing() {
    let home = scratch_home("no-fixtures");
    let out = tuimex(&home).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8(out.stderr).unwrap().contains("--fixtures"));
    assert!(!String::from_utf8(out.stdout).unwrap().contains(ENTER_SCREEN));
}

/// ADR-053: with the keyring turned off the binary asks no keyring service,
/// on Linux and on a Mac alike. An account whose password would be in one
/// is told the switch's own words, which no service says, and its server
/// is not tried.
#[test]
fn doctor_with_the_keyring_turned_off_asks_no_keyring_service_and_says_what_turned_it_off() {
    let home = scratch_home("keyring-off");
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    let config = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n\
                  imap = { host = \"127.0.0.1\", port = 1 }\nsmtp = { host = \"127.0.0.1\", port = 1 }\n";
    fs::write(home.join(".config/tuimex/config.toml"), config).unwrap();
    let out = tuimex(&home).arg("doctor").output().unwrap();
    let said = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{said}");
    let off = "no keyring service could be reached (TUIMEX_KEYRING is none, so none was tried)";
    assert!(said.contains(&format!("  WRONG  the keyring does not answer: {off}")), "{said}");
    assert!(!said.contains("IMAP 127.0.0.1:1"), "no server was tried with no password to offer:\n{said}");

    // A value that is not `none` is refused by name, never passed over.
    let out = tuimex(&home).arg("doctor").env("TUIMEX_KEYRING", "system").output().unwrap();
    let said = String::from_utf8(out.stdout).unwrap();
    assert!(said.contains("TUIMEX_KEYRING is \"system\", and the only value it takes is none"), "{said}");
}

/// `tuimex doctor --redact` through the binary: the report it prints
/// without the flag, line for line, with the account, its address, its
/// user name, its servers and the home directory each replaced, in the
/// server's refusal as in doctor's own words.
#[test]
fn doctor_redacted_prints_the_same_report_with_no_name_address_host_or_home_in_it() {
    let home = scratch_home("doctor-redact");
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    let config = "[[account]]\nname = \"bureau\"\naddress = \"lovelace@example.org\"\npassword_cmd = [\"printf\", \"x\"]\n\
                  imap = { host = \"127.0.0.1\", port = 1, user = \"lovelace.a\" }\nsmtp = { host = \"localhost\", port = 1 }\n";
    fs::write(home.join(".config/tuimex/config.toml"), config).unwrap();
    let doctor = |args: &[&str]| {
        let out = tuimex(&home).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "neither server is there to sign in to");
        String::from_utf8(out.stdout).unwrap()
    };
    let (plain, hidden) = (doctor(&["doctor"]), doctor(&["doctor", "--redact"]));
    let at_home = home.to_string_lossy();
    for held in ["bureau", "lovelace", "example.org", "127.0.0.1", "localhost", &*at_home] {
        assert!(plain.contains(held), "the report has no {held:?} to replace:\n{plain}");
        assert!(!hidden.contains(held), "{held:?} is in:\n{hidden}");
    }
    for line in ["config ~/.config/tuimex/config.toml", "account account-1 (address-1)", "  WRONG  IMAP host-1:1: "] {
        assert!(hidden.lines().any(|l| l.starts_with(line)), "wanted {line:?} in:\n{hidden}");
    }
    assert!(hidden.contains("  WRONG  SMTP host-2:1: ") && hidden.ends_with("things are wrong\n"), "{hidden}");
    assert_eq!(plain.lines().count(), hidden.lines().count(), "{plain}\n{hidden}");
}

/// PLAN §11: a config with nobody's mail in it is no reason to show a
/// demo either. And `--section` opens where it says, or says there is no
/// such place before the screen is taken.
#[test]
fn a_config_with_no_account_stops_it_and_a_section_that_is_not_there_does_too() {
    let home = scratch_home("no-account");
    fs::create_dir_all(home.join(".config/tuimex")).unwrap();
    fs::write(home.join(".config/tuimex/config.toml"), "[ui]\nglyphs = \"unicode\"\n").unwrap();
    let out = tuimex(&home).output().unwrap();
    let said = String::from_utf8(out.stderr).unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(said.contains("the config has no account") && said.contains("tuimex setup"), "{said}");
    assert!(!String::from_utf8(out.stdout).unwrap().contains(ENTER_SCREEN));

    let out = tuimex(&home).args(["--fixtures", "--section", "nowhere"]).output().unwrap();
    let said = String::from_utf8(out.stderr).unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(said.contains("--section nowhere: there is no section of that name"), "{said}");
    assert!(!String::from_utf8(out.stdout).unwrap().contains(ENTER_SCREEN));

    let out = on_a_pty(&home, QUIT, &["--fixtures", "--section", "flagged"], &[]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{screen:?}");
    assert!(visible(&screen).contains("1 of 3 "), "the three flagged rows: {screen:?}");
}

#[test]
fn every_flag_the_plan_names_for_m0_is_offered() {
    let out = tuimex(&scratch_home("help")).arg("--help").output().unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    for flag in ["--config", "--fixtures", "--glyphs", "--colors", "--section", "--version"] {
        assert!(help.contains(flag), "PLAN §10 names {flag}, but `tuimex --help` does not offer it");
    }
}

/// A stand-in for `$EDITOR`: a script that replaces the file it is given
/// with `text`, and notes beside itself that it ran on a terminal.
fn fake_editor(home: &Path, text: &str) -> String {
    let path = home.join("editor.sh");
    let script = format!(
        "#!/bin/sh\ntest -t 0 && test -t 1 && echo on-a-terminal > {}/editor-ran\nprintf '{text}' > \"$1\"\n",
        home.display()
    );
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path.display().to_string()
}

/// `keys` as steps of one key each, split on `|`: a key is then read
/// before the next is typed, however many there are.
fn one_by_one(keys: &str) -> String {
    keys.chars().map(String::from).collect::<Vec<_>>().join("|")
}

/// PLAN §10 M8: `Enter` on the body opens `$EDITOR` with the interface
/// suspended, and restores it with what the editor left.
#[test]
fn the_body_is_written_in_editor_and_the_interface_comes_back_around_it() {
    let home = scratch_home("editor");
    let editor = fake_editor(&home, "Written in the editor.\\n\\nSecond paragraph.\\n");
    let env = [("ON_PTY_STEPS", "|"), ("EDITOR", editor.as_str())];
    let out = on_a_pty(&home, &one_by_one("c\t\t\t\t\r\x1bn:q\r"), &["--fixtures"], &env);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
    assert_eq!(fs::read_to_string(home.join("editor-ran")).unwrap(), "on-a-terminal\n", "the editor had the terminal");
    let marks: Vec<(usize, &str)> =
        screen.match_indices(ENTER_SCREEN).chain(screen.match_indices(LEAVE_SCREEN)).collect();
    let mut marks: Vec<(usize, bool)> = marks.into_iter().map(|(at, mark)| (at, mark == ENTER_SCREEN)).collect();
    marks.sort();
    let order: Vec<bool> = marks.iter().map(|(_, entered)| *entered).collect();
    assert_eq!(order, [true, false, true, false], "the alternate screen is left for the editor and entered again");
    let after_editor = &screen[marks[2].0..];
    let frames = frames(after_editor);
    let back = frames.first().expect("a frame after the editor");
    assert!(
        back.contains("    Written in the editor.\n\n    Second paragraph.\n"),
        "the body is what the editor left: {back:?}"
    );
    assert!(
        back.contains("    To\n") && back.contains("Tab next field - Left Right identity - Enter edit"),
        "{back:?}"
    );
    assert!(
        frames.iter().any(|f| f.contains("Keep this draft?  y keep - n discard - Esc back")),
        "an edit is a change"
    );
    let cache = home.join(".cache/tuimex/compose");
    assert_eq!(fs::read_dir(&cache).unwrap().count(), 0, "nothing of the edit is left on disk");
    let mut found = Vec::new();
    modes_under(&home.join(".cache"), &mut found);
    assert!(found.iter().all(|(_, mode)| *mode == 0o700), "what was made for the editor is owner-only: {found:?}");
}

/// PLAN §10 M8, by hand: `c`, To and Subject, the body through the editor,
/// `:send` for the summary, `n` back, `^d` to save the draft.
#[test]
fn a_session_writes_a_message_reads_its_summary_and_saves_the_draft() {
    let home = scratch_home("compose");
    let editor = fake_editor(&home, "See you at one.\\n");
    let keys = "cada@example.com, Grace Hopper <grace@example.net>\t\t\tLunch on Thursday?\t\r:send\rn\x04\x1b4:q\r";
    let out =
        on_a_pty(&home, &one_by_one(keys), &["--fixtures"], &[("ON_PTY_STEPS", "|"), ("EDITOR", editor.as_str())]);
    let screen = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
    let frames = frames(&screen);
    let shows = |words: &str| frames.iter().position(|frame| frame.contains(words));
    let steps = [
        "    From     robin@example.com   robin.vale@example.org   desk@example.org",
        ">   To       ada@example.com\n             Grace Hopper <grace@example.net>",
        ">   Subject  Lunch on Thursday?",
        "    See you at one.",
        ":send",
        "    From     Robin Vale <robin.vale@example.org>\n    To       ada@example.com\n             Grace Hopper <grace@example.net>\n    Subject  Lunch on Thursday?\n    Signing  not signed - not encrypted",
        "Send to 2 recipients?  y send, held 10s - n back",
        "+ Draft saved",
        "draft saved ",
        ">   work  to ada@example.~    Lunch on Thursday?",
    ];
    let mut last = 0;
    for words in steps {
        let at = shows(words).unwrap_or_else(|| panic!("no frame shows {words:?}; the frames were {frames:#?}"));
        assert!(at >= last, "{words:?} was shown before the step before it");
        last = at;
    }
    let after_n = &frames[shows("+ Draft saved").unwrap()];
    assert!(
        after_n.contains("    Subject  Lunch on Thursday?") && after_n.contains("    See you at one."),
        "n went back to the draft: {after_n:?}"
    );
    assert!(!frames.iter().any(|f| f.contains("Sending to")), "n sent nothing");
}

/// ADR-031 on a terminal, with a line of shell standing for the agent: no
/// test starts a real one.
#[test]
fn agent_tidy_runs_the_configured_command_and_without_one_is_not_a_command() {
    // A pause between steps, because the agent's text arrives on its own
    // time and not as the frame after a key.
    let keys = "c|\t|\t|\t|\t|\r|:|agent tidy|\r|u|\x1b|n|:q\r";
    let run = |name: &str, config: Option<&str>| {
        let home = scratch_home(name);
        let editor = fake_editor(&home, "me and u at 1\\n");
        if let Some(config) = config {
            fs::create_dir_all(home.join(".config/tuimex")).unwrap();
            fs::write(home.join(".config/tuimex/config.toml"), config).unwrap();
        }
        let env = [("ON_PTY_STEPS", "|"), ("ON_PTY_PAUSE", "0.3"), ("EDITOR", editor.as_str())];
        let out = on_a_pty(&home, keys, &["--fixtures"], &env);
        let screen = String::from_utf8(out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(0), "the terminal showed {screen:?}");
        (frames(&screen), home)
    };
    let config = "[agent]\ncommand = [\"sh\", \"-c\", \"grep -q 'me and u at 1' && pwd > ../../../agent-ran && echo 'You and I, at one.'\"]\n";
    let (frames, home) = run("agent-tidy", Some(config));
    let shows = |words: &str| frames.iter().position(|frame| frame.contains(words));
    let steps = [
        "    me and u at 1",
        "% Tidying your text",
        "  % You and I, at one.",
        "+ Tidied 1 paragraph - u for your own text",
        "+ Undone - your own text is back",
    ];
    let mut last = 0;
    for words in steps {
        let at = shows(words).unwrap_or_else(|| panic!("no frame shows {words:?}; the frames were {frames:#?}"));
        assert!(at >= last, "{words:?} was shown before the step before it");
        last = at;
    }
    let working = &frames[shows("% Tidying your text").unwrap()];
    assert!(working.contains("agent saw the text as written") && working.contains("Esc cancel"), "{working:?}");
    let tidied = &frames[shows("+ Tidied 1 paragraph").unwrap()];
    assert!(tidied.contains("    % Tidied by an agent - "), "the body is marked as an agent's: {tidied:?}");
    let ran = fs::read_to_string(home.join("agent-ran")).expect("the command was given the body on standard input");
    assert_eq!(Path::new(ran.trim()), home.join(".cache/tuimex/agent"), "and ran in a directory of its own");

    let (frames, home) = run("agent-absent", None);
    assert!(
        frames.iter().any(|f| f.contains("E not a command: :agent - no [agent] table in the config")),
        "{frames:#?}"
    );
    assert!(!frames.iter().any(|f| f.contains("Tidying")) && !home.join(".cache/tuimex/agent").exists());
}

/// PLAN §6.7: a first launch with no config and a terminal asks setup's
/// questions. Here the person leaves at the first one (Ctrl-D), so nothing
/// is saved and the interface, which has no account to show, is not opened.
#[test]
fn a_first_launch_on_a_terminal_runs_setup_and_opens_nothing_when_setup_is_left() {
    let home = scratch_home("first-launch");
    let mut cmd = Command::new("python3");
    isolate(&mut cmd, &home);
    cmd.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/answer_on_pty.py"));
    let out = cmd.arg(r#"[["Email address:", "\u0004"]]"#).arg(env!("CARGO_BIN_EXE_tuimex")).output().unwrap();
    assert!(out.stderr.is_empty(), "the pty harness failed: {}", String::from_utf8_lossy(&out.stderr));
    let shown = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{shown}");
    assert!(shown.contains("tuimex setup adds a mail account."), "{shown}");
    assert!(shown.contains("E setup: the answers ended before the questions did; nothing was saved"), "{shown}");
    assert!(!shown.contains(ENTER_SCREEN) && !home.join(".config").exists(), "{shown}");
}

//! The renderer against the design, byte for byte (ADR-002, PLAN §8). Each
//! `list*` case gives the list screen the state `build()` in
//! `design/tui.js` gives that output; every other case types the keys of
//! its caption into the real `App`, and the reader's cases open their
//! message in it first. Both are painted through the real
//! renderer and compared with what `make golden` wrote from the design.

// A test that cannot read its inputs should fail loudly, which is what
// unwrap does; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod calendar;

use std::fs;
use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use tuimex_core::{AccountId, Action, Attachment, Check, Event, Listed, MessageId, Sending, Timestamp};
use tuimex_fixtures::design as fx;
use tuimex_ui::app::App;
use tuimex_ui::compose::quote::Opening;
use tuimex_ui::compose::summary::{self, Signing, Summary};
use tuimex_ui::glyphs::Glyphs;
use tuimex_ui::keymap::Key;
use tuimex_ui::layout;
use tuimex_ui::list::{self, AccountSync, Filter, List, Preview, Row, Sync, Tab};
use tuimex_ui::mailbox::Mailbox;
use tuimex_ui::paint::{Theme, paint};
use tuimex_ui::palette::{Palette, Tier, Tok};
use tuimex_ui::say;
use tuimex_ui::span::{Line, cells, markup};

/// The two builds `make golden` writes.
const DEFAULT: &str = ".";
const ASCII_16: &str = "ascii-16";

/// Every output of mail this test can draw; the calendar's are listed in
/// `calendar`. `no_output_goes_undrawn` fails when the design has one that
/// is in neither.
const DRAWN: [&str; 100] = [
    "list-three-accounts",
    "list-three-accounts-80-ascii",
    "account-cycle-0",
    "account-cycle-1",
    "account-cycle-2",
    "account-cycle-3",
    "list",
    "list-80-ascii",
    "list-thread",
    "list-filter",
    "list-filter-80-ascii",
    "list-empty",
    "list-first-sync",
    "list-failing",
    "list-outbox",
    "help",
    "move",
    "command-line-0",
    "command-line-1",
    "command-line-2",
    "command-line-3",
    "command-line-4",
    "command-line-5",
    "command-line-6",
    "command-line-7",
    "archive-undo-0",
    "archive-undo-1",
    "archive-undo-2",
    "trash-batch-0",
    "trash-batch-1",
    "move-refused-0",
    "move-refused-1",
    "move-refused-2",
    "send-sent-0",
    "send-sent-1",
    "send-undone-0",
    "send-undone-1",
    "compose",
    "send-summary",
    "compose-agent-working",
    "compose-agent-reply",
    "compose-agent-tidy",
    "agent-reply-0",
    "agent-reply-1",
    "agent-reply-2",
    "agent-failed-0",
    "agent-failed-1",
    "reader-plain",
    "reader-plain-80-ascii",
    "reader-html",
    "reader-attachments",
    "reader-invite",
    "reader-notices-0",
    "reader-notices-1",
    "reader-notices-2",
    "reader-notices-3",
    "reader-notices-4",
    "reader-notices-5",
    "links",
    "links-confirm",
    "links-agent-asking",
    "links-agent-verdict",
    "agent-link-0",
    "agent-link-1",
    "agent-link-2",
    "list-preview-focus",
    "list-wide",
    "list-wide-preview-focus",
    "list-account",
    "search",
    "search-empty",
    "help-commands",
    "missing-folder-0",
    "missing-folder-1",
    "empty-trash-0",
    "empty-trash-1",
    "browser-ask",
    "browser-ask-80-ascii",
    "browser-copy-0",
    "browser-copy-1",
    "browser-copy-2",
    "links-confirm-80-ascii",
    "agent-completion-0",
    "agent-completion-1",
    "list-wider",
    "search-waiting",
    "agent-menu-0",
    "agent-menu-1",
    "agent-menu-2",
    "agent-menu-3",
    "agent-menu-4",
    "list-drafts",
    "help-filtered",
    "list-browser-ask",
    "list-preview-scrolled",
    "list-preview-arriving",
    "list-wide-preview-scrolled",
    "list-wide-recipients",
    "loading",
    "loading-80-ascii",
];

fn golden_dir(build: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(build)
}

/// One line of `index.tsv`: what the generator wrote, and how.
struct Output {
    name: String,
    width: u16,
    height: u16,
    glyphs: Glyphs,
    tier: Tier,
}

fn index(build: &str) -> Vec<Output> {
    let text = fs::read_to_string(golden_dir(build).join("index.tsv")).expect("`make golden` writes index.tsv");
    let outputs: Vec<Output> = text
        .lines()
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            Output {
                name: f[0].to_string(),
                width: f[2].parse().unwrap(),
                height: f[3].parse().unwrap(),
                glyphs: match f[4] {
                    "unicode" => Glyphs::Unicode,
                    "ascii" => Glyphs::Ascii,
                    other => panic!("{}: glyphs {other}", f[0]),
                },
                tier: match f[5] {
                    "truecolor" => Tier::True,
                    "16" => Tier::C16,
                    other => panic!("{}: colours {other}", f[0]),
                },
            }
        })
        .collect();
    assert!(outputs.len() > 20, "{build}: index.tsv lists only {} outputs", outputs.len());
    outputs
}

/// A buffer in the format `scripts/gen-golden.mjs` writes: the characters,
/// and the same with each run of one style prefixed by `{fg [bg=x] [b] [u]}`.
/// A truecolor run is named by its token and a sixteen-colour run by the
/// colour asked for.
fn serialize(buf: &Buffer, tier: Tier) -> (String, String) {
    let name = |c: Color| -> String {
        match (tier, c) {
            (Tier::True, Color::Rgb(r, g, b)) => {
                Tok::ALL.iter().find(|t| t.truecolor() == (r, g, b)).expect("a palette colour").name().into()
            }
            (Tier::C16, Color::Reset) => "default".into(),
            (Tier::C16, Color::Black) => "black".into(),
            (Tier::C16, Color::DarkGray) => "brightblack".into(),
            (Tier::C16, Color::Magenta) => "magenta".into(),
            (Tier::C16, Color::Green) => "green".into(),
            (Tier::C16, Color::Yellow) => "yellow".into(),
            (Tier::C16, Color::Red) => "red".into(),
            (Tier::C16, Color::Cyan) => "cyan".into(),
            (Tier::C16, Color::LightBlue) => "brightblue".into(),
            (Tier::C16, Color::LightYellow) => "brightyellow".into(),
            (_, other) => panic!("{other:?} is not a colour the {tier:?} tier draws with"),
        }
    };
    let (mut plain, mut styled) = (String::new(), String::new());
    for y in buf.area.top()..buf.area.bottom() {
        let mut last = String::new();
        for x in buf.area.left()..buf.area.right() {
            let cell = &buf[(x, y)];
            let mut tag = name(cell.fg);
            if cell.bg != Color::Reset {
                tag += &format!(" bg={}", name(cell.bg));
            }
            if cell.modifier.contains(Modifier::BOLD) {
                tag += " b";
            }
            if cell.modifier.contains(Modifier::UNDERLINED) {
                tag += " u";
            }
            if tag != last {
                styled += &format!("{{{tag}}}");
                last = tag;
            }
            plain += cell.symbol();
            styled += cell.symbol();
        }
        plain.push('\n');
        styled.push('\n');
    }
    (plain, styled)
}

fn first_difference(want: &str, got: &str) -> String {
    for (i, (w, g)) in want.lines().zip(got.lines()).enumerate() {
        if w != g {
            return format!("row {i}\n want: {w}\n  got: {g}");
        }
    }
    format!("row counts differ: want {}, got {}", want.lines().count(), got.lines().count())
}

fn id(key: &str) -> MessageId {
    let all = fx::inbox();
    all.into_iter().find(|m| m.envelope.id.key == key).expect("a key of the fixture mailbox").envelope.id
}

fn at(rows: &[Row], key: &str) -> usize {
    rows.iter().position(|row| row.msg.envelope.id.key == key).expect("a row of that message")
}

fn tab(name: &str, count: usize) -> Tab {
    let tone = match name {
        "Flagged" => Tok::Warn,
        "Outbox" => Tok::Err,
        _ => Tok::Fg,
    };
    Tab { name: name.to_string(), count, tone }
}

/// The design's `COUNTS`: a tab has a number only when the number means
/// "this needs you". `outbox` is the count of failures, when the tab is
/// there at all.
fn tabs(inbox: Option<&[Row]>, flagged: &[Row], outbox: Option<&[Listed]>) -> Vec<Tab> {
    let mut tabs: Vec<Tab> = fx::SECTIONS
        .iter()
        .map(|(name, _)| match *name {
            "Inbox" => tab(name, inbox.map_or(0, |rows| rows.iter().filter(|r| r.unread).count())),
            "Flagged" => tab(name, flagged.iter().filter(|r| r.flagged).count()),
            _ => tab(name, 0),
        })
        .collect();
    if let Some(outbox) = outbox {
        tabs.push(tab("Outbox", outbox.iter().filter(|m| matches!(m.sending, Some(Sending::Failed { .. }))).count()));
    }
    tabs
}

/// The design's `ACCOUNTS`, in its order.
fn accounts() -> Vec<AccountId> {
    let mut all: Vec<AccountId> = Vec::new();
    for identity in fx::identities() {
        if !all.contains(&identity.account) {
            all.push(identity.account);
        }
    }
    all
}

/// The design's `SYNC`: one message held, one refused, both accounts well.
fn usual() -> Sync {
    Sync { held: Some(6), failed: 1, accounts: Vec::new() }
}

/// The lines of one `list*` output at the size the design gives it, or
/// `None` for a name this test has no state for.
fn draw(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    let (mail, held) = (fx::inbox(), fx::outbox());
    let inbox = list::rows(&mail, &[]);
    let all_tabs = tabs(Some(&inbox), &inbox, Some(&held));
    let marks: Vec<MessageId> = fx::MARKED.iter().map(|key| id(key)).collect();
    let accounts = accounts();
    let colours = list::account_colours(&layout::ACCOUNTS_BUILTIN, &accounts, &[]);
    let base = List {
        tabs: &all_tabs,
        section: "Inbox",
        rows: &inbox,
        search: None,
        selected: None,
        marks: &[],
        filter: None,
        preview: Preview::Auto,
        focus: false,
        pane_top: 0,
        check: None,
        body: None,
        arriving: false,
        stale: &[],
        accounts: &accounts,
        colours: &colours,
        words: (layout::WORDS_BUILTIN, layout::WORDS_BUILTIN),
        narrowed: None,
        grouped: true,
        empty: list::nothing_here(Some(fx::SYNCED)),
        left: Vec::new(),
        right: list::alerts(&usual()),
        command: None,
        panel: Vec::new(),
        top: 0,
        now: fx::NOW,
    };
    let on =
        |rows: &[Row], i: usize, marks: &[MessageId]| list::position(i + 1, rows.len(), list::target(&rows[i], marks));
    let lines = match name.strip_suffix("-80-ascii").unwrap_or(name) {
        "list" | "list-wide" | "list-wider" => {
            let i = at(&inbox, fx::HALDEN);
            list::screen(&List { selected: Some(i), marks: &marks, left: on(&inbox, i, &marks), ..base }, width, height)
        }
        "list-account" => {
            let work: Vec<Listed> = mail.iter().filter(|m| m.envelope.id.account.0 == "work").cloned().collect();
            let rows = list::rows(&work, &[]);
            let tabs = tabs(Some(&rows), &rows, Some(&held));
            let command = say::plain("Showing work only").key("gt for the next, :account for all").line(width);
            let narrowed = List { tabs: &tabs, rows: &rows, selected: Some(0), narrowed: accounts.get(1), ..base };
            list::screen(&List { left: on(&rows, 0, &[]), command: Some(command), ..narrowed }, width, height)
        }
        "list-three-accounts" => {
            let (mail, trio) = (fx::among_three(fx::inbox()), fx::TRIO.map(|name| AccountId(name.to_string())));
            let rows = list::rows(&mail, &[]);
            let i = at(&rows, "bank");
            let colours = list::account_colours(&layout::ACCOUNTS_BUILTIN, &trio, &[]);
            let three = List { rows: &rows, selected: Some(i), accounts: &trio, colours: &colours, ..base };
            let three = List { left: on(&rows, i, &[]), ..three };
            list::screen(&three, width, height)
        }
        "list-thread" => {
            let open = list::rows(&mail, &[id(fx::UNFOLDED.0)]);
            let i = at(&open, fx::UNFOLDED.1);
            list::screen(&List { rows: &open, selected: Some(i), left: on(&open, i, &[]), ..base }, width, height)
        }
        "list-filter" => {
            let matches: Vec<MessageId> = fx::FILTER.1.iter().map(|key| id(key)).collect();
            let filter = Filter { needle: fx::FILTER.0, matches: &matches };
            let left = markup(&format!("{{dim|{} here ·}} Enter {{dim|searches all mail}}", matches.len()));
            let command = Some(list::typed(&format!("/{}", fx::FILTER.0)));
            let selected = Some(at(&inbox, fx::FILTER.1[0]));
            let typing = List { selected, filter: Some(filter), preview: Preview::Off, left, command, ..base };
            list::screen(&typing, width, height)
        }
        "list-empty" => {
            let tabs = tabs(None, &inbox, Some(&held));
            list::screen(&List { tabs: &tabs, section: "Unread", rows: &[], ..base }, width, height)
        }
        "list-first-sync" => {
            let mail = fx::arrived();
            let rows = list::rows(&mail, &[]);
            let tabs = tabs(Some(&rows), &rows, None);
            let (account, done, total) = fx::ARRIVING;
            let sync = Sync {
                accounts: vec![(account.to_string(), AccountSync::FirstSync { done, total })],
                ..Sync::default()
            };
            let (left, right) = (on(&rows, 0, &[]), list::alerts(&sync));
            let arriving =
                List { tabs: &tabs, rows: &rows, selected: Some(0), preview: Preview::Off, left, right, ..base };
            list::screen(&arriving, width, height)
        }
        "list-failing" => {
            let (account, reason, retry, last) = fx::FAILING;
            let failing = AccountSync::Failing { reason: reason.to_string(), retry: Some(retry), last: Some(last) };
            let sync = Sync { accounts: vec![(account.to_string(), failing)], ..Sync::default() };
            let stale = AccountId(account.to_string());
            let (left, right) = (on(&inbox, 1, &[]), list::alerts(&sync));
            list::screen(
                &List { selected: Some(1), stale: std::slice::from_ref(&stale), left, right, ..base },
                width,
                height,
            )
        }
        // What the screen says before there is a list: mail is being
        // opened, and the index is part rebuilt.
        "loading" => {
            let mut lines = say::loading(Some((1240, 9240)));
            lines.resize(height, Vec::new());
            lines
        }
        "list-wide-recipients" => {
            let i = at(&inbox, "invite");
            list::screen(&List { selected: Some(i), left: on(&inbox, i, &[]), ..base }, width, height)
        }
        "list-preview-arriving" => {
            let i = at(&inbox, "statement");
            let waiting = List { selected: Some(i), arriving: true, left: on(&inbox, i, &[]), ..base };
            list::screen(&waiting, width, height)
        }
        "list-drafts" => {
            let drafts = fx::drafts();
            let rows = list::rows(&drafts, &[]);
            let saved = List { section: "Drafts", rows: &rows, selected: Some(0), preview: Preview::Off, ..base };
            list::screen(&List { left: on(&rows, 0, &[]), ..saved }, width, height)
        }
        "list-outbox" => {
            let rows = list::outbox(&held);
            let left = markup("{dim|2 of 2 ·} not sent");
            let held = List { section: "Outbox", rows: &rows, selected: Some(1), grouped: false, ..base };
            list::screen(&List { preview: Preview::Off, left, ..held }, width, height)
        }
        _ => return session(name, width, height),
    };
    Some(lines)
}

fn app(mail: Vec<Listed>) -> App {
    let mailbox = Mailbox {
        now: fx::NOW,
        sections: fx::sections(),
        folders: fx::folders(),
        mail,
        synced: Some(fx::SYNCED),
        me: fx::addresses(),
    };
    let mut app = App::new(Theme { palette: Palette::new(Tier::True), glyphs: Glyphs::Unicode }, mailbox);
    // As a config with an `[agent]` table sets it up: `:agent` exists.
    app.set_up(fx::identities(), true);
    app
}

/// An app over `mail` as three accounts hold it: the design's `trio`,
/// each with an address and the five folders.
fn trio(mail: Vec<Listed>) -> App {
    let folders = fx::TRIO.iter().flat_map(|account| fx::folders_of(account, [8, 0, 0, 0, 0])).collect();
    let mailbox = Mailbox {
        now: fx::NOW,
        sections: fx::sections(),
        folders,
        mail,
        synced: Some(fx::SYNCED),
        me: fx::addresses(),
    };
    let mut app = App::new(Theme { palette: Palette::new(Tier::True), glyphs: Glyphs::Unicode }, mailbox);
    let identity = |name: &&str| tuimex_core::Identity {
        account: AccountId(name.to_string()),
        address: tuimex_core::Address { name: None, email: format!("robin@{name}.example.net") },
    };
    app.set_up(fx::TRIO.iter().map(identity).collect(), true);
    app
}

/// The design's mailbox as `tuimex --fixtures` opens it: both inboxes, and
/// the outbox with one message held and one refused.
fn usual_mail() -> Vec<Listed> {
    fx::inbox().into_iter().chain(fx::outbox()).collect()
}

/// Types `keys`: a return is Enter, a tab Tab, `^x` the chord and `⌥x`
/// Alt with `x`.
fn press(app: &mut App, keys: &str) {
    let mut chars = keys.chars();
    while let Some(c) = chars.next() {
        app.key(match c {
            '\r' => Key::Enter,
            '\t' => Key::Tab,
            '^' => Key::Ctrl(chars.next().unwrap()),
            '⌥' => Key::Alt(chars.next().unwrap()),
            other => Key::Char(other),
        });
    }
}

/// The design's folders, less the Archive of one account.
fn without_archive(account: &str) -> Vec<tuimex_core::Folder> {
    fx::folders().into_iter().filter(|f| f.account.0 != account || f.role != tuimex_core::Role::Archive).collect()
}

/// A strip as the design's `strip` cuts one: the first `count` rows of the
/// list, less the rows a panel takes, then the panel, a blank row, the
/// status line and the command line. They are taken from the whole screen
/// the app draws, tall enough that nothing is cut; `p` has hidden the
/// preview, which a strip does not have.
fn strip(app: &mut App, count: usize) -> Vec<Line> {
    let lines = app.lines(120, 60);
    let text = |line: &Line| line.iter().map(|s| s.text.as_str()).collect::<String>();
    // A row has a space where a group's label has its second letter.
    let is_row = |line: &&Line| cells(line) > 0 && text(line).chars().nth(3) == Some(' ');
    let foot = lines.len() - 3;
    let panel_from = (2..foot).rev().find(|i| cells(&lines[*i]) == 0).expect("a blank row over the foot");
    let panel = if panel_from + 1 < foot { &lines[panel_from..foot] } else { &[] };
    let rows = lines[2..panel_from].iter().filter(is_row).take(count.saturating_sub(panel.len().saturating_sub(1)));
    rows.chain(panel).chain(&lines[foot..]).cloned().collect()
}

/// Hands what the app has asked for to `fake`, and the app the first
/// `told` things that come back.
fn answer(app: &mut App, fake: &mut fx::Fake, told: usize) {
    let events: Vec<Event> = app.asks().into_iter().flat_map(|ask| fake.ask(ask)).collect();
    events.into_iter().take(told).for_each(|event| app.event(event));
}

/// Opens the message `key` in the reader: Enter on its row, then `b` back
/// through its thread until it is the one open.
fn read(app: &mut App, fake: &mut fx::Fake, key: &str) {
    let mail = fx::inbox();
    let rows = list::rows(&mail, &[]);
    let row = rows.iter().position(|row| row.ids.iter().any(|id| id.key == key)).expect("a row of that message");
    press(app, &format!("{}\r", "j".repeat(row)));
    answer(app, fake, usize::MAX);
    for _ in 0..rows[row].size {
        if app.pictures().is_some_and(|(id, _)| id.key != key) {
            press(app, "b");
            answer(app, fake, usize::MAX);
        }
    }
    assert_eq!(app.pictures().map(|(id, _)| id.key.as_str()), Some(key), "the reader has it open");
}

/// The end of the reader as the design's `readerStrip` cuts it: the
/// `notices` rows under the header block, the last `panel` rows over the
/// foot, then the blank row, the status line and the command line. The
/// screen is tall enough that the body does not scroll, as a strip's does
/// not.
fn reader_strip(mut app: App, notices: usize, panel: usize) -> Vec<Line> {
    let lines = app.lines(120, 60);
    let blank = |line: &Line| line.iter().all(|s| s.text.trim().is_empty());
    let fields = (2..lines.len()).find(|i| blank(&lines[*i])).expect("a blank row under the fields");
    let foot = lines.len() - 3;
    lines[fields + 1..fields + 1 + notices].iter().chain(&lines[foot - panel..]).cloned().collect()
}

/// The reader's outputs: each opens its message and types what its
/// caption says. The strips are cut from the whole screen as the design
/// cuts them: a notice strip is the header block and two rows of the body,
/// and a link strip is the list of links down to the command line.
fn reading(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    let mut fake = fx::Fake::default();
    let opened = |key: &str, keys: &str, told: usize| {
        let (mut app, mut fake) = (app(usual_mail()), fx::Fake::default());
        read(&mut app, &mut fake, key);
        press(&mut app, keys);
        answer(&mut app, &mut fake, told);
        app
    };
    let notice = |mut app: App| app.lines(120, 40)[2..2 + height].to_vec();
    let link_strip = |mut app: App| app.lines(120, 40)[40 - height..].to_vec();
    let asking = "o:agent link 1\r";
    Some(match name.strip_suffix("-80-ascii").unwrap_or(name) {
        "reader-plain" => opened("halden-1", "", 0).lines(width, height),
        "reader-html" => opened("newsletter", "", 0).lines(width, height),
        "reader-attachments" => opened("photos", "a", 0).lines(width, height),
        "reader-invite" => opened("invite", "i", 0).lines(width, height),
        "links" => opened("newsletter", "o", 0).lines(width, height),
        "links-confirm" => opened("bank", "o1", 0).lines(width, height),
        "browser-ask" => opened("newsletter", "B", 0).lines(width, height),
        "browser-copy-0" => reader_strip(opened("newsletter", "B", 0), 1, 0),
        "browser-copy-1" => reader_strip(opened("newsletter", "By", 0), 1, 0),
        "browser-copy-2" => reader_strip(opened("newsletter", "Bn", 0), 1, 0),
        // The title and the two links, without the blank row over them.
        "agent-completion-1" => reader_strip(opened("bank", ":agent link \t", 0), 0, 3),
        "links-agent-asking" => opened("bank", asking, 1).lines(width, height),
        "links-agent-verdict" => opened("bank", asking, 2).lines(width, height),
        "agent-link-0" => link_strip(opened("bank", "o1", 0)),
        "agent-link-1" => link_strip(opened("bank", "o1A", 1)),
        "agent-link-2" => link_strip(opened("bank", "o1A", 2)),
        "agent-menu-1" => reader_strip(opened("halden-1", "A", 0), 1, 3),
        "agent-menu-3" => link_strip(opened("bank", "oA", 0)),
        "reader-notices-0" => notice(opened("halden-1", "", 0)),
        "reader-notices-1" => notice(opened("vault", "", 0)),
        "reader-notices-2" => {
            // The design gives this message a signature only here, made
            // with a key `gpg` does not have.
            let mut mail = usual_mail();
            let budget = mail.iter_mut().find(|m| m.envelope.id.key == "budget").unwrap();
            budget.text = Some("The numbers are in the sheet. Tell me if the travel line looks low.".to_string());
            let body = Box::new(fx::body(budget));
            let mut app = app(mail);
            read(&mut app, &mut fake, "budget");
            app.event(Event::BodyReady { id: id("budget"), body });
            app.event(Event::Checked { id: id("budget"), check: Check::UnknownKey { key: "9D2E41B7".to_string() } });
            notice(app)
        }
        "reader-notices-3" => notice(opened("payroll", "", 0)),
        "reader-notices-4" => notice(opened("halden-3", "", 0)),
        "reader-notices-5" => {
            // `DRAFTS[0]` as the design fills it in, alone in the drafts.
            let mut draft = fx::draft();
            let file = |index, name: &str, kb: u32| Attachment {
                index,
                name: name.to_string(),
                mime: "application/octet-stream".to_string(),
                size: kb * 1024,
                inline: false,
            };
            draft.attachments = vec![file(0, "contract-v3-signed.pdf", 512), file(1, "schedule-b.xlsx", 38)];
            draft.text =
                Some("Hi Priya,\n\n45 days is fine provided the first invoice goes out at signing.".to_string());
            // Enter on a draft is compose's; the reader comes to one from
            // the message before it in a section that holds both.
            draft.envelope.flags.flagged = true;
            let mut app = app(usual_mail().into_iter().chain([draft]).collect());
            press(&mut app, "3j\rK");
            assert_eq!(app.pictures().map(|(id, _)| id.key.as_str()), Some("draft-agent"));
            notice(app)
        }
        _ => return calendar::draw(name, width, height),
    })
}

/// The outputs drawn by typing into the app: the keys of each step's
/// caption in the design, after `p`.
fn session(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    let typed = |keys: &str, count: usize| {
        let mut app = app(usual_mail());
        press(&mut app, keys);
        strip(&mut app, count)
    };
    // The send flows start with nothing else in the outbox.
    let held = |keys: &str| {
        let mut app = app(fx::inbox());
        press(&mut app, "pjjj");
        app.send(fx::draft(), 10);
        press(&mut app, keys);
        strip(&mut app, 5)
    };
    Some(match name {
        "help" => {
            let mut app = app(usual_mail());
            press(&mut app, "?");
            app.lines(width, height)
        }
        "help-commands" => {
            let mut app = app(usual_mail());
            press(&mut app, "?");
            app.lines(width, height);
            press(&mut app, "^d^d^d");
            app.lines(width, height)
        }
        "help-filtered" => {
            let mut app = app(usual_mail());
            press(&mut app, "?agent");
            app.lines(width, height)
        }
        // The two marked rows, the cursor back on the Halden thread, and
        // the focus moved to the preview where it lies.
        "list-preview-focus" | "list-wide-preview-focus" => {
            let mut app = app(usual_mail());
            press(&mut app, "jj jjjjjjj kkkkkk");
            app.lines(width, height);
            press(&mut app, if name == "list-preview-focus" { "⌥j" } else { "⌥l" });
            app.lines(width, height)
        }
        // The long message, its preview focused and scrolled fourteen
        // lines, a frame after each so that it knows how far it can go.
        "list-preview-scrolled" | "list-wide-preview-scrolled" => {
            let mut app = app(usual_mail());
            press(&mut app, "jjjjj");
            app.lines(width, height);
            press(&mut app, if name == "list-preview-scrolled" { "⌥j" } else { "⌥l" });
            for _ in 0..14 {
                app.lines(width, height);
                press(&mut app, "j");
            }
            app.lines(width, height)
        }
        "account-cycle-0" | "account-cycle-1" | "account-cycle-2" | "account-cycle-3" => {
            let mut app = trio(fx::among_three(usual_mail()));
            let presses: usize = name[14..].parse().unwrap();
            press(&mut app, &format!("p{}", "gt".repeat(presses + 1)));
            strip(&mut app, 5)
        }
        "list-browser-ask" => {
            let (mut app, mut fake) = (app(usual_mail()), fx::Fake::default());
            press(&mut app, "jjjjB");
            answer(&mut app, &mut fake, usize::MAX);
            app.lines(width, height)
        }
        "agent-menu-0" => typed("pjA", 5),
        "agent-menu-4" => {
            let mut app = app(usual_mail());
            app.set_up(fx::identities(), false);
            press(&mut app, "pjjjA");
            strip(&mut app, 5)
        }
        "search" | "search-empty" | "search-waiting" => {
            let (mut app, mut fake) = (app(usual_mail().into_iter().chain(fx::filed()).collect()), fx::Fake::default());
            press(&mut app, if name == "search-empty" { "/zebra\r" } else { "/nov\r" });
            answer(&mut app, &mut fake, if name == "search-waiting" { 0 } else { usize::MAX });
            press(&mut app, if name == "search" { "jjj" } else { "" });
            // The cursor has rested on its row, whose message has no text.
            app.lines(width, height);
            app.rest();
            answer(&mut app, &mut fake, usize::MAX);
            app.lines(width, height)
        }
        "missing-folder-0" | "missing-folder-1" => {
            let mut app = app(usual_mail());
            app.refresh(fx::NOW, 0, Some(&[]), Vec::new(), without_archive("work"));
            press(&mut app, if name == "missing-folder-0" { "pjje" } else { "pjjey" });
            strip(&mut app, 5)
        }
        "empty-trash-0" | "empty-trash-1" => {
            let (mut app, mut fake) = (app(usual_mail()), fx::Fake::default());
            press(&mut app, "pj:empty-trash work\ry");
            let emptied = app.asks().into_iter().flat_map(|ask| fake.ask(ask));
            // In the second step three had not reached the server's Trash.
            let kept = if name == "empty-trash-0" { 0 } else { 3 };
            for event in emptied {
                app.event(match event {
                    Event::Emptied { serial, account, deleted, .. } => {
                        Event::Emptied { serial, account, deleted: deleted - kept, kept }
                    }
                    other => other,
                });
            }
            strip(&mut app, 5)
        }
        "move" => {
            let mut app = app(usual_mail());
            press(&mut app, "jjjM\t");
            app.lines(width, height)
        }
        "command-line-0" => typed("pj", 5),
        "command-line-1" => typed("pj:trash", 5),
        "command-line-2" => typed("pj:a\t", 5),
        "command-line-3" => typed("pj:empty-trash work\r", 5),
        "command-line-4" => typed("pje", 5),
        "command-line-5" => typed("pj:move Recipts\r", 5),
        "command-line-6" => typed("pj:arhcive\r", 5),
        "command-line-7" => {
            // What `:messages` lists happened earlier in the design's day,
            // so it is put in the log as it would have been said then.
            let ago = |minutes: i64| Timestamp(fx::NOW.0 - minutes * 60);
            let mut app = app(usual_mail());
            app.note(ago(100), say::ok("Flagged 1 message").key("u to undo"));
            app.note(ago(99), say::err("sync: work offline").then("retried 4 times"));
            app.note(ago(75), say::err("send: refused by smtp.example.org: 552 message too large"));
            app.note(ago(18), say::ok("Sent to 1 recipient"));
            app.note(ago(8), say::err(":move: no folder \"Recipts\" in personal").key("Tab completes"));
            app.note(ago(1), say::ok("Archived 1 message").key("u to undo"));
            press(&mut app, "pj:messages\r");
            strip(&mut app, 9)
        }
        "archive-undo-0" => typed("pjj", 5),
        "archive-undo-1" => typed("pjje", 5),
        "archive-undo-2" => typed("pjjeu", 5),
        "trash-batch-0" => typed("pjj jj jj ", 7),
        "trash-batch-1" => typed("pjj jj jj d", 7),
        "move-refused-0" => typed("pjjjMArchive", 5),
        "move-refused-1" | "move-refused-2" => {
            let mut app = app(usual_mail());
            let mut server = fx::Fake::refusing("Archive", "NO [OVERQUOTA] quota exceeded");
            press(&mut app, "pjjjMArchive\r");
            if name == "move-refused-2" {
                let told: Vec<Event> = app.asks().into_iter().flat_map(|ask| server.ask(ask)).collect();
                told.into_iter().for_each(|event| app.event(event));
            }
            strip(&mut app, 5)
        }
        "send-sent-0" | "send-undone-0" => held(""),
        "send-undone-1" => held("u"),
        "send-sent-1" => {
            // What sending does to the thread it answers is the engine's
            // to do, so the mail starts as the design patches it: the
            // newest Halden message read and answered, the first unflagged.
            let mut mail = fx::inbox();
            for m in &mut mail {
                match m.envelope.id.key.as_str() {
                    "halden-3" => (m.envelope.flags.seen, m.envelope.flags.answered) = (true, true),
                    "halden-1" => m.envelope.flags.flagged = false,
                    _ => {}
                }
            }
            let mut app = app(mail);
            press(&mut app, "pjjj");
            app.send(fx::draft(), 10);
            app.event(Event::Sent { id: fx::draft().envelope.id, answered: Vec::new() });
            strip(&mut app, 5)
        }
        _ => return writing(name, width, height),
    })
}

/// The end of compose as the design's `composeStrip` cuts it: the subject
/// and the blank row under it, the notice and the body, the `panel` rows
/// of a panel, a blank row, the status line and the command line.
fn compose_strip_over(app: &mut App, panel: usize) -> Vec<Line> {
    let lines = app.lines(120, 60);
    let text = |line: &Line| line.iter().map(|s| s.text.as_str()).collect::<String>();
    let subject = lines.iter().position(|line| text(line).trim_start().starts_with("Subject")).expect("a subject");
    let foot = lines.len() - 3 - panel;
    let last = (subject + 2..foot).rev().find(|i| !text(&lines[*i]).trim().is_empty()).unwrap_or(subject + 1);
    lines[subject..=last].iter().chain(&lines[foot..]).cloned().collect()
}

fn compose_strip(app: &mut App) -> Vec<Line> {
    compose_strip_over(app, 0)
}

/// The compose outputs. Each goes on writing a saved draft from the Drafts
/// section, with `r`, and types what its caption in the design says; what the agent command
/// would answer is told to the app as whoever runs it would tell it.
fn writing(name: &str, width: usize, height: usize) -> Option<Vec<Line>> {
    let minutes = |n: i64| Timestamp(fx::NOW.0 + n * 60);
    let open = |mail: Vec<Listed>, now: Timestamp, agent: bool, keys: &str| {
        let me = fx::addresses();
        let mailbox =
            Mailbox { now, sections: fx::sections(), folders: fx::folders(), mail, synced: Some(fx::SYNCED), me };
        let mut app = App::new(Theme { palette: Palette::new(Tier::True), glyphs: Glyphs::Unicode }, mailbox);
        app.set_up(fx::identities(), agent);
        press(&mut app, "4r");
        press(&mut app, keys);
        app
    };
    // The user's own reply, with the cursor moved from From to the body,
    // where `:` is the command line.
    let own = |body: &str, agent: bool, typed: &str| {
        let mail = fx::inbox().into_iter().chain([fx::own_reply(body)]).collect();
        open(mail, minutes(1), agent, &format!("\t\t\t\t\t{typed}"))
    };
    let answer = |app: &mut App, text: Result<&str, &str>| {
        let asked =
            app.asks().into_iter().find(|ask| matches!(ask.action, Action::Agent(_))).expect("the agent is asked");
        let text = text.map(str::to_string).map_err(str::to_string);
        app.event(Event::AgentText { serial: asked.serial, text });
    };
    let reply = ":agent reply agree to 45 days";
    Some(match name {
        "compose" => {
            let mail = usual_mail().into_iter().chain([fx::agent_draft()]).collect();
            let mut app = open(mail, minutes(-22), false, "");
            app.key(Key::Ctrl('d'));
            press(&mut app, "\t\t\t\t\t\t");
            app.lines(width, height)
        }
        "send-summary" => {
            // The design draws a message `gpg` will sign, which nothing
            // here can ask for until OpenPGP is built; the screen is given
            // that state as the list screens are given theirs.
            let (mail, identities) = (fx::inbox(), fx::identities());
            let opened = Opening { identities: &identities, key: String::new() }.from_draft(&fx::agent_draft());
            let draft = opened.draft(&identities).unwrap();
            let thread = tuimex_ui::compose::thread(&mail, &draft.id.account, opened.in_reply_to.as_deref());
            let signing = Signing::Signed { key: "51F09A2C".into(), no_key: vec!["billing@halden.example.com".into()] };
            let right = list::alerts(&usual());
            let agent = opened.agent.as_ref();
            summary::screen(
                &Summary { draft: &draft, agent, thread: &thread, signing: &signing, delay: 10, right: &right },
                width,
                height,
            )
        }
        "compose-agent-working" => own("", true, &format!("{reply}\r")).lines(width, height),
        "compose-agent-reply" => {
            let mut app = own("", true, &format!("{reply}\r"));
            answer(&mut app, Ok(fx::AGENT_WROTE));
            app.lines(width, height)
        }
        "compose-agent-tidy" => {
            let mut app = own(fx::YOURS, true, ":agent tidy\r");
            answer(&mut app, Ok(fx::AGENT_TIDIED));
            app.lines(width, height)
        }
        "agent-reply-0" => compose_strip(&mut own("", true, reply)),
        "agent-reply-1" => compose_strip(&mut own("", true, &format!("{reply}\r"))),
        "agent-reply-2" => {
            let mut app = own("", true, &format!("{reply}\r"));
            answer(&mut app, Ok(fx::AGENT_WROTE));
            compose_strip(&mut app)
        }
        "agent-failed-0" => {
            let mut app = own(fx::YOURS, true, ":agent tidy\r");
            answer(&mut app, Err("the agent command exited with 1"));
            compose_strip(&mut app)
        }
        "agent-failed-1" => compose_strip(&mut own(fx::YOURS, false, ":agent tidy\r")),
        "agent-menu-2" => compose_strip_over(&mut own(fx::YOURS, true, "A"), 4),
        // The panel is its blank row, its title and the three functions.
        "agent-completion-0" => compose_strip_over(&mut own(fx::YOURS, true, ":agent \t"), 5),
        _ => return reading(name, width, height),
    })
}

/// What is wrong with the output as it is drawn here, when anything is.
fn compare(build: &str, name: &str) -> Result<(), String> {
    let all = index(build);
    let output = all.iter().find(|o| o.name == name).unwrap_or_else(|| panic!("{build}/{name} is not in index.tsv"));
    let lines = draw(name, usize::from(output.width), usize::from(output.height))
        .ok_or_else(|| format!("{name}: the design has this output and no test draws it"))?;
    let mut buf = Buffer::empty(Rect::new(0, 0, output.width, output.height));
    if lines.len() != usize::from(output.height) {
        return Err(format!("{name}: {} lines for {} rows", lines.len(), output.height));
    }
    let theme = Theme { palette: Palette::new(output.tier), glyphs: output.glyphs };
    paint(&lines, &mut buf, Rect::new(0, 0, output.width, output.height), &theme);
    let (plain, styled) = serialize(&buf, output.tier);
    let read =
        |file: String| fs::read_to_string(golden_dir(build).join(&file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    let (want_plain, want_styled) = (read(format!("{name}.txt")), read(format!("{name}.ansi.txt")));
    if plain != want_plain {
        return Err(format!("{build}/{name}: text differs from the design\n{}", first_difference(&want_plain, &plain)));
    }
    if styled != want_styled {
        return Err(format!(
            "{build}/{name}: style differs from the design\n{}",
            first_difference(&want_styled, &styled)
        ));
    }
    Ok(())
}

/// Fails unless the output is the design's, byte for byte.
fn check(build: &str, name: &str) {
    if let Err(wrong) = compare(build, name) {
        panic!("{wrong}");
    }
}

/// The output as the default build draws it and, where the second build
/// has it too, in ASCII with sixteen colours at the same size.
fn both(name: &str) {
    check(DEFAULT, name);
    check(ASCII_16, name);
}

#[test]
fn list() {
    both("list");
}

#[test]
fn list_80_ascii() {
    check(DEFAULT, "list-80-ascii");
}

#[test]
fn list_preview_focus() {
    both("list-preview-focus");
}

#[test]
fn list_wide() {
    both("list-wide");
    both("list-wider");
    both("list-wide-preview-focus");
}

#[test]
fn list_account() {
    both("list-account");
}

#[test]
fn list_three_accounts() {
    both("list-three-accounts");
    check(DEFAULT, "list-three-accounts-80-ascii");
}

#[test]
fn account_cycle() {
    steps("account-cycle", 4);
}

#[test]
fn search() {
    both("search");
    both("search-empty");
    both("search-waiting");
}

#[test]
fn help_commands() {
    both("help-commands");
    both("help-filtered");
}

#[test]
fn missing_folder() {
    steps("missing-folder", 2);
}

#[test]
fn empty_trash() {
    steps("empty-trash", 2);
}

#[test]
fn list_thread() {
    both("list-thread");
}

#[test]
fn list_filter() {
    both("list-filter");
}

#[test]
fn list_filter_80_ascii() {
    check(DEFAULT, "list-filter-80-ascii");
}

#[test]
fn list_empty() {
    both("list-empty");
}

#[test]
fn list_first_sync() {
    both("list-first-sync");
}

#[test]
fn list_failing() {
    both("list-failing");
}

#[test]
fn list_preview_whole() {
    both("list-preview-scrolled");
    both("list-wide-preview-scrolled");
    both("list-preview-arriving");
    both("list-wide-recipients");
}

#[test]
fn loading() {
    both("loading");
    check(DEFAULT, "loading-80-ascii");
}

#[test]
fn list_drafts() {
    both("list-drafts");
}

#[test]
fn list_browser_ask() {
    both("list-browser-ask");
}

#[test]
fn list_outbox() {
    both("list-outbox");
}

#[test]
fn help() {
    both("help");
}

#[test]
fn move_prompt() {
    both("move");
}

fn steps(flow: &str, count: usize) {
    (0..count).for_each(|step| both(&format!("{flow}-{step}")));
}

#[test]
fn command_line() {
    steps("command-line", 8);
}

#[test]
fn archive_undo() {
    steps("archive-undo", 3);
}

#[test]
fn trash_batch() {
    steps("trash-batch", 2);
}

#[test]
fn move_refused() {
    steps("move-refused", 3);
}

#[test]
fn send_sent() {
    steps("send-sent", 2);
}

#[test]
fn send_undone() {
    steps("send-undone", 2);
}

#[test]
fn compose() {
    both("compose");
}

#[test]
fn send_summary() {
    both("send-summary");
}

#[test]
fn compose_agent() {
    ["working", "reply", "tidy"].iter().for_each(|step| both(&format!("compose-agent-{step}")));
}

#[test]
fn agent_reply() {
    steps("agent-reply", 3);
}

#[test]
fn agent_failed() {
    steps("agent-failed", 2);
}

#[test]
fn reader_plain() {
    both("reader-plain");
}

#[test]
fn reader_plain_80_ascii() {
    check(DEFAULT, "reader-plain-80-ascii");
}

#[test]
fn reader_html() {
    both("reader-html");
}

#[test]
fn reader_attachments() {
    both("reader-attachments");
}

#[test]
fn reader_invite() {
    both("reader-invite");
}

#[test]
fn reader_notices() {
    steps("reader-notices", 6);
}

#[test]
fn links() {
    both("links");
}

#[test]
fn links_confirm() {
    both("links-confirm");
}

#[test]
fn links_agent() {
    both("links-agent-asking");
    both("links-agent-verdict");
}

#[test]
fn agent_link() {
    steps("agent-link", 3);
}

#[test]
fn links_confirm_80_ascii() {
    check(DEFAULT, "links-confirm-80-ascii");
}

#[test]
fn browser_ask() {
    both("browser-ask");
    check(DEFAULT, "browser-ask-80-ascii");
}

#[test]
fn browser_copy() {
    steps("browser-copy", 3);
}

#[test]
fn agent_menu() {
    steps("agent-menu", 5);
}

#[test]
fn agent_completion() {
    steps("agent-completion", 2);
}

/// Every output of the design is drawn and compared. One the design gains
/// later is a failure here until it is drawn too, and every output that
/// differs is named, not only the first.
#[test]
fn no_output_goes_undrawn() {
    let (mut found, mut wrong) = (Vec::new(), Vec::new());
    let drawn: Vec<&str> = DRAWN.iter().copied().chain(calendar::drawn()).collect();
    for build in [DEFAULT, ASCII_16] {
        for output in index(build).iter() {
            let name = output.name.as_str();
            match compare(build, name) {
                Err(why) => wrong.push(why.lines().next().unwrap_or_default().to_string()),
                Ok(()) => assert!(drawn.contains(&name), "{build}/{name}: drawn, and not listed in DRAWN"),
            }
            found.push(name.to_string());
        }
    }
    assert!(wrong.is_empty(), "{} outputs are not the design's:\n{}", wrong.len(), wrong.join("\n"));
    for name in drawn {
        assert!(found.iter().any(|f| f == name), "{name} is drawn here and the design no longer has it");
    }
}

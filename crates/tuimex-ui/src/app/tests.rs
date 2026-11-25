use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tuimex_core::{AccountId, Action, Envelope, Event, Flags, Folder, FolderId, Listed, Role, Sending};
use tuimex_fixtures::design as fx;

use super::App;
use crate::glyphs::Glyphs;
use crate::keymap::{KEYMAP, Key, presses};
use crate::mailbox::Mailbox;
use crate::paint::Theme;
use crate::palette::{Palette, Tier};
use crate::span::Line;

pub(super) fn over(mail: Vec<Listed>) -> App {
    let mailbox = Mailbox {
        now: fx::NOW,
        sections: fx::sections(),
        folders: fx::folders(),
        mail,
        synced: Some(60),
        me: fx::addresses(),
    };
    let mut app = App::new(Theme { palette: Palette::new(Tier::True), glyphs: Glyphs::Unicode }, mailbox);
    app.set_up(fx::identities(), true);
    app.set_calendar(crate::calendar::tests::design());
    app
}

pub(super) fn app() -> App {
    over(fx::inbox().into_iter().chain(fx::outbox()).collect())
}

/// Types `keys`: a return is Enter, a tab Tab, an escape Esc, a delete
/// Backspace, `^x` is the chord and `⌥x` is Alt with `x`.
pub(super) fn press(app: &mut App, keys: &str) {
    let mut chars = keys.chars();
    while let Some(c) = chars.next() {
        app.key(match c {
            '\r' => Key::Enter,
            '\t' => Key::Tab,
            '\x1b' => Key::Esc,
            '\x7f' => Key::Backspace,
            '^' => Key::Ctrl(chars.next().unwrap()),
            '⌥' => Key::Alt(chars.next().unwrap()),
            other => Key::Char(other),
        });
    }
}

/// Hands what the app asked for to `fake`, and the app what comes back.
fn told(app: &mut App, fake: &mut fx::Fake) {
    let events: Vec<Event> = app.asks().into_iter().flat_map(|ask| fake.ask(ask)).collect();
    events.into_iter().for_each(|event| app.event(event));
}

/// The app with the message `key` open in the reader and its body there,
/// after `keys`. What opening asked for has been taken; what `keys` asked
/// for has not.
pub(super) fn reading(key: &str, keys: &str) -> App {
    let (mut app, mut fake) = (app(), fx::Fake::default());
    let mail = fx::inbox();
    let row = crate::list::rows(&mail, &[]).iter().position(|row| row.msg.envelope.id.key == key).unwrap();
    press(&mut app, &format!("{}\r", "j".repeat(row)));
    told(&mut app, &mut fake);
    press(&mut app, keys);
    app
}

pub(super) fn after(keys: &str) -> App {
    let mut app = app();
    press(&mut app, keys);
    app
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>().trim_end().to_string()).collect()
}

/// The `height` lines under the bar of places, which `bar` reads: a test of
/// a screen counts its rows from the screen's own first.
pub(super) fn under(app: &mut App, width: usize, height: usize) -> Vec<Line> {
    app.lines(width, height + 1).split_off(1)
}

pub(super) fn screen(app: &mut App, width: usize, height: usize) -> Vec<String> {
    texts(&under(app, width, height))
}

/// The bar of places over the screen, and what of it is drawn as the place
/// in view.
pub(super) fn bar(app: &mut App) -> (String, String) {
    let lines = app.lines(120, 41);
    let in_view = lines[0].iter().filter(|span| span.fg == crate::palette::Tok::Acc && span.bold);
    (texts(&lines[..1]).remove(0), in_view.map(|span| span.text.as_str()).collect())
}

/// The status line's left and the command line, at 120 by 40.
pub(super) fn foot(app: &mut App) -> (String, String) {
    let rows = screen(app, 120, 40);
    let left = rows[38].split("   ").next().unwrap().trim().to_string();
    (left, rows[39].trim().to_string())
}

fn said(keys: &str) -> String {
    foot(&mut after(keys)).1
}

fn at(keys: &str) -> String {
    foot(&mut after(keys)).0
}

#[test]
fn no_key_quits_and_the_two_commands_do() {
    for c in ('a'..='z').chain('A'..='Z') {
        assert!(!after(&c.to_string()).quit, "{c}");
    }
    assert!(!after("\x1b").quit, "Esc goes back, and from the list there is nowhere to go");
    assert!(after(":q\r").quit && after(":qa\r").quit);
    assert!(!after("\r:q\r").quit, ":q in the reader goes back to the list");
    assert!(after("\r:qa\r").quit);
}

#[test]
fn the_screen_is_the_list_with_the_cursor_on_its_first_row() {
    let rows = screen(&mut app(), 120, 40);
    assert_eq!(rows[0], "  Inbox 9   Unread   Flagged 3   Drafts   Sent   Archive   Outbox 1");
    assert_eq!(rows[2], "  Today");
    assert!(rows[3].starts_with("▌   work       Imani Castell           Invitation: Q4 roadmap review"), "{}", rows[3]);
    assert!(rows[19].starts_with("────"), "the divider over the preview: {}", rows[19]);
    assert_eq!(rows[20], "  Invitation: Q4 roadmap review, Thu 26 Nov 10:00", "the preview begins with its subject");
    assert!(rows[22].starts_with("    From     Imani Castell <imani@example.org>"), "the preview: {}", rows[22]);
    assert!(rows[38].starts_with("  1 of 24 · 1 message "), "{}", rows[38]);
    assert!(rows[38].ends_with("sending in 6s · 1 not sent"), "{}", rows[38]);
    assert_eq!(rows[39], "  Enter open · e archive · d trash · r reply · ? help");
}

/// A frame drawn over another, as after a resize or a change of mode,
/// leaves nothing of it.
#[test]
fn every_cell_is_written_at_any_size_in_every_mode() {
    let modes = ["", "?m", ":\t", ":messages\r", "/nov", ":empty-trash work\r", "7"];
    let reading = ["\r", "jjj\rG", "jjjjjjjj\ra", "\r:\t"];
    let writing = ["c", "cada@example.org, \t\t\t\t:send\r", "r^a/", "r\x1b", "r:agent reply\r", "f:", "jjj\rr"];
    for keys in modes.into_iter().chain(reading).chain(writing) {
        let mut app = after(keys);
        for (width, height) in (0..=130).step_by(13).flat_map(|w| (0..=45).step_by(3).map(move |h| (w, h))) {
            let area = Rect::new(0, 0, width, height);
            let mut buf = Buffer::filled(area, ratatui::buffer::Cell::new("¤"));
            app.render(area, &mut buf);
            let cells = (0..height).flat_map(|y| (0..width).map(move |x| (x, y)));
            assert!(cells.into_iter().all(|at| buf[at].symbol() != "¤"), "{keys:?} at {width}x{height}");
        }
    }
}

/// Everything a key could change, so that a key which changes none of it
/// did nothing. The screen is compared with its styles: the identity
/// chosen on From is told by weight alone.
fn state(app: &mut App) -> String {
    format!("{:?} {} {:?} {}", app.lines(120, 40), app.quit, app.asks.len(), app.redraw)
}

/// PLAN §10, M7: every key in the table does something. Each key of each
/// binding is pressed in a state where it has something to do, and has to
/// change what is drawn or asked.
#[test]
fn every_key_of_the_table_does_something() {
    // What is typed first, for the bindings that need more than a cursor
    // in the middle of the inbox.
    let before = |id: &str, key: &str| match (id, key) {
        ("global.back", _) => "jj ",
        // The first Halden message, unfolded from its thread, is longer
        // than the preview.
        ("preview.scroll", "k" | "^u" | "gg") => "jjjzo⌥jjj",
        ("preview.scroll" | "preview.focus-list", _) => "jjjzo⌥j",
        ("help.scroll", "^u") => "?\x04",
        ("help.scroll", _) => "?",
        ("global.undo", _) => "jje",
        ("list.match", _) => "/nov\r",
        ("list.unfold", _) => "j",
        ("list.fold", _) => "jzo",
        ("command.complete", _) => ":a",
        ("command.history", "Up") => ":sync\r:",
        ("command.history", _) => ":sync\r:^",
        ("command.erase" | "command.run", _) => ":sync all",
        ("command.cancel", _) => ":",
        ("confirm.yes" | "confirm.no" | "confirm.other", _) => ":empty-trash work\r",
        // A new message opens on To: five fields on is From, four the body.
        ("compose.identity" | "compose.identity-arrows", _) => "c\t\t\t\t\t",
        ("compose.editor", _) => "c\t\t\t\t",
        (id, _) if id.starts_with("compose.") => "c",
        // The first Halden message is longer than the screen and is one
        // of a thread; the bank's has links and the photographs files.
        ("reader.scroll", "k" | "^u") => "jjj\rbbjj",
        ("reader.scroll", "gg") => "jjj\rbbG",
        ("reader.thread", "n") => "jjj\rb",
        ("reader.list", "K") | ("reader.thread", _) => "jjj\r",
        ("reader.invite", _) => "\r",
        ("reader.links" | "reader.browser", _) => "jjjj\r",
        ("reader.attachments", _) => "jjjjjjjj\r",
        (id, "k") if id.starts_with("links.") => "jjjj\roj",
        (id, _) if id.starts_with("links.") => "jjjj\ro",
        ("attachments.choose", "k") => "jjjjjjjj\raj",
        (id, _) if id.starts_with("attachments.") => "jjjjjjjj\ra",
        (id, _) if id.starts_with("invite.") => "\ri",
        ("agent.tidy", _) => "c\t\t\t\tA",
        ("agent.link", _) => "jjjj\roA",
        ("agent.reply", _) => "jjA",
        // Today has two events, the first with a link; `I` finds the
        // invitation on Thursday, and a search for "call" two events.
        ("calendar.match", "N") => "gc/call\rn",
        ("calendar.match", _) => "gc/call\r",
        ("calendar.today", _) => "gcl",
        ("calendar.focus-grid", _) => "gc⌥l",
        ("calendar.view-number", "1") => "gc2",
        ("calendar.answer", _) => "gcI\r",
        ("calendar.links" | "calendar.edit" | "calendar.delete", _) => "gc\r",
        (id, _) if id.starts_with("calendar.") => "gc",
        // One field on from the title is Calendar, which is chosen from.
        ("event-form.choice" | "event-form.choice-arrows", _) => "gcc\t",
        (id, _) if id.starts_with("event-form.") => "gcc",
        // The roadmap review has fields under More; every event fits whole
        // at this size, so a key that scrolls says so.
        ("event.fold", "zo") => "gclll\rj\r",
        ("event.fold", _) => "gclll\rj\rzo",
        (id, _) if id.starts_with("event.") => "gc\r\r",
        ("calendars.choose", "k") => "gcCj",
        (id, _) if id.starts_with("calendars.") => "gcC",
        (id, _) if id.starts_with("reader.") => "jjj\rbb",
        _ => "jj",
    };
    let mut walked = Vec::new();
    for table in KEYMAP.iter() {
        walked.push(table.mode);
        for binding in table.keys {
            for name in binding.keys.split(' ').filter(|name| *name != "other") {
                let (mut app, mut fake) = (app(), fx::Fake::default());
                let mut first = before(binding.id, name).chars();
                while let Some(c) = first.next() {
                    match c {
                        '^' => app.key(Key::Up),
                        '\x04' => app.key(Key::Ctrl('d')),
                        '⌥' => app.key(Key::Alt(first.next().unwrap())),
                        c => press(&mut app, &c.to_string()),
                    }
                    // A message opened has its body before the next key.
                    told(&mut app, &mut fake);
                    state(&mut app);
                }
                let was = state(&mut app);
                let typed = match name {
                    "1-9" => vec![Key::Char('2')],
                    "any" => vec![Key::Char('x')],
                    name => presses(name),
                };
                typed.into_iter().for_each(|key| app.key(key));
                assert!(state(&mut app) != was, "{} ({name}) changed nothing", binding.id);
            }
        }
    }
    // Every table is walked: a table the design gains belongs here too.
    let later: Vec<&str> = KEYMAP.iter().map(|t| t.mode).filter(|mode| !walked.contains(mode)).collect();
    assert_eq!(later, [] as [&str; 0]);
}

#[test]
fn the_cursor_steps_over_group_labels_and_stops_at_the_ends() {
    assert_eq!(at("jjjjjjj"), "8 of 24 · 1 message");
    let rows = screen(&mut after("jjjjjjjj"), 120, 60);
    assert_eq!(rows[12], "  Yesterday");
    assert!(rows[13].starts_with("▌   personal   Greta Holm"), "{}", rows[13]);
    assert_eq!(at("Gj"), "24 of 24 · 1 message");
    assert_eq!(at("Gggk"), "1 of 24 · 1 message");
}

/// The design has no rule for a list longer than its room. The rows move
/// as little as keeps the cursor in sight, and a row that heads its group
/// brings the label.
#[test]
fn the_list_scrolls_to_keep_the_cursor_in_sight() {
    let mut app = after("G");
    let rows = screen(&mut app, 80, 12);
    assert!(rows[8].starts_with("▌   pers  Greta Holm"), "{rows:#?}");
    assert_eq!(rows[9], "", "the blank row over the status line");
    press(&mut app, "k");
    assert!(screen(&mut app, 80, 12)[7].starts_with("▌   work  Mei Tanaka"), "one up does not scroll");
    press(&mut app, "gg");
    let rows = screen(&mut app, 80, 12);
    assert_eq!((rows[2].as_str(), rows[3].starts_with("▌   work  Imani Castell")), ("  Today", true));
    press(&mut app, "jjjjjjjj");
    let rows = screen(&mut app, 80, 12);
    assert_eq!(rows[7], "  Yesterday", "{rows:#?}");
    assert!(rows[8].starts_with("▌   pers  Greta Holm"));
}

#[test]
fn half_a_page_is_half_the_rows_last_drawn() {
    let mut app = app();
    screen(&mut app, 120, 29);
    press(&mut app, "^d");
    assert_eq!(foot(&mut app).0, "13 of 24 · 1 message", "24 lines of rows without the preview");
    screen(&mut app, 120, 29);
    press(&mut app, "^u^u");
    assert_eq!(foot(&mut app).0, "1 of 24 · 1 message");
}

#[test]
fn a_sequence_waits_for_its_second_key_and_a_timeout_drops_it() {
    let mut app = after("jjg");
    assert!(app.waiting());
    press(&mut app, "g");
    assert!(!app.waiting());
    assert_eq!(foot(&mut app).0, "1 of 24 · 1 message");
    press(&mut app, "jjg");
    app.timeout();
    assert!(!app.waiting());
    press(&mut app, "g");
    assert_eq!(foot(&mut app).0, "3 of 24 · 1 message", "the second g starts a sequence of its own");
    assert_eq!(at("jjgj"), "3 of 24 · 1 message", "a sequence that is none drops both keys");
}

#[test]
fn sections_are_reached_by_h_l_tab_and_number_and_go_round() {
    let open = |keys: &str| screen(&mut after(keys), 120, 40)[38].trim().to_string();
    assert!(open("l").starts_with("1 of 9 "), "Unread: {}", open("l"));
    assert!(open("\t\t").starts_with("1 of 3 · 1 message"), "Flagged");
    assert!(open("h").starts_with("1 of 2 · waiting to send"), "back from the first is the last, the outbox");
    assert_eq!(open("hl"), open(""));
    assert!(open("3").starts_with("1 of 3 "));
    assert_eq!(screen(&mut after("4"), 120, 40)[3], "    Nothing here.");
    assert_eq!(said("9"), "E section 9: there are 7");
    let mut app = after("j jl");
    press(&mut app, "h");
    assert_eq!(foot(&mut app).0, "1 of 24 · 1 message", "leaving a section clears its marks and its cursor");
    let mut back = after("l");
    back.key(Key::BackTab);
    assert_eq!(foot(&mut back).0, "1 of 24 · 1 message");
}

#[test]
fn space_marks_and_unmarks_and_v_marks_the_range_from_the_last_mark() {
    assert_eq!(at("jj "), "3 of 24 · 1 marked");
    assert_eq!(at("jj  "), "3 of 24 · 1 message");
    assert_eq!(at("j jjjV"), "5 of 24 · 4 marked");
    assert_eq!(at("jjjj kkV"), "3 of 24 · 3 marked", "upwards too");
    assert_eq!(at("V"), "1 of 24 · 1 marked", "with nothing marked it marks the row");
    assert_eq!(at("j jjjV\x1b"), "5 of 24 · 1 message", "Esc clears them");
}

/// PLAN §6.3: a thread row targets the whole thread unless unfolded.
#[test]
fn a_thread_unfolds_and_folds_and_an_action_takes_what_the_status_line_says() {
    assert_eq!(at("j"), "2 of 24 · thread of 5");
    assert_eq!(at("jzo"), "2 of 28 · 1 message of the thread");
    let mut app = after("jzojj");
    assert!(screen(&mut app, 120, 40)[6].contains("Great. Then I will bring the food"), "a reply shows its opening");
    press(&mut app, "e");
    assert_eq!(foot(&mut app), ("4 of 27 · 1 message of the thread".into(), "✓ Archived 1 message · u to undo".into()));
    press(&mut app, "zc");
    assert_eq!(foot(&mut app).0, "2 of 24 · thread of 4", "folded, the cursor is on the thread");
    assert_eq!(said("je"), "✓ Archived 5 messages · u to undo");
    assert_eq!(at("zo"), "1 of 24 · 1 message", "a row that is no thread stays as it is");
}

#[test]
fn marked_rows_are_taken_wherever_the_cursor_is_and_a_marked_thread_is_taken_whole() {
    let mut app = after("j jj G");
    assert_eq!(foot(&mut app).0, "24 of 24 · 2 marked");
    press(&mut app, "d");
    assert_eq!(foot(&mut app), ("22 of 22 · 1 message".into(), "✓ Trashed 8 messages · u to undo".into()));
    let asked = app.asks();
    assert_eq!((asked.len(), &asked[0].action, asked[0].on.len()), (1, &Action::Trash, 8));
    press(&mut app, "u");
    assert_eq!(foot(&mut app), ("24 of 24 · 1 message".into(), "✓ Undone · 8 messages back in Inbox".into()));
}

#[test]
fn slash_finds_among_the_rows_here_as_it_is_typed_in_the_query_grammar() {
    let mut app = after("jj/nov");
    let here = |n: usize| format!("{n} here · Enter searches all mail");
    assert_eq!(foot(&mut app), (here(3), "/nov".into()));
    let rows = screen(&mut app, 120, 40);
    assert!(rows[10].starts_with("▌   work       Mei Tanaka"), "the cursor goes to the next match: {}", rows[10]);
    assert!(rows.iter().all(|row| !row.starts_with("────")), "the rows have the screen while they are looked through");
    assert_eq!(at("/is:unread from:greta"), here(1));
    assert_eq!(at("/roof"), here(1), "a folded thread matches when a message of it does");
    assert_eq!(at("/zzz"), here(0));
    assert_eq!(at("jj/nov\x1b"), "8 of 24 · 1 message", "Esc drops the filter and leaves the cursor on what it found");
    assert_eq!(said("n"), "E n: no filter · / to find");
    assert_eq!(
        foot(&mut after("/\r")).1,
        "Enter open · e archive · d trash · r reply · ? help",
        "nothing typed asks nothing"
    );
    assert_eq!(foot(&mut after("/nov library^w")).1, "/nov");
    assert_eq!(foot(&mut after("/nov^ux\x7f")).1, "/");
}

#[test]
fn the_command_line_completes_edits_and_remembers() {
    assert_eq!(said(":a\t\t"), ":account");
    let mut back = after(":a\t");
    back.key(Key::BackTab);
    assert_eq!(foot(&mut back).1, ":agent");
    assert_eq!(said(":mes\t"), ":messages", "one choice is filled in");
    assert_eq!(said(":move Ar\t"), ":move Archive");
    assert_eq!(said(":section f\t"), ":section Flagged");
    assert_eq!(said(":empty-trash \t\t"), ":empty-trash work");
    assert_eq!(said(":move Old mail^w^w"), ":move");
    assert_eq!(said(":move Sent^u"), ":");
    assert_eq!(said(":ab\x7f"), ":a");
    assert_eq!(said(":\x7f"), "Enter open · e archive · d trash · r reply · ? help", "Backspace on nothing leaves");
    assert_eq!(said(":trash\x1b"), "Enter open · e archive · d trash · r reply · ? help");

    let mut app = after(":sync\r:section Flagged\r:");
    app.key(Key::Up);
    assert_eq!(foot(&mut app).1, ":section Flagged");
    app.key(Key::Up);
    app.key(Key::Up);
    assert_eq!(foot(&mut app).1, ":sync", "the oldest is the end");
    app.key(Key::Down);
    app.key(Key::Down);
    assert_eq!(foot(&mut app).1, ":");
    press(&mut app, "sy");
    app.key(Key::Up);
    assert_eq!(foot(&mut app).1, ":sync", "only what starts with what was typed");
}

#[test]
fn a_long_menu_shows_the_choices_around_the_selected_one() {
    let mut app = after(":\t");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(
        (rows[25].as_str(), rows[26].trim_end()),
        ("  Commands", "▌   archive                 archive what is selected")
    );
    assert_eq!(rows[39], "  :archive");
    (0..12).for_each(|_| app.key(Key::Tab));
    let rows = screen(&mut app, 120, 40);
    assert!(rows[36].starts_with("▌   forward"), "{rows:#?}");
    assert!(rows[26].starts_with("    flag"));
}

#[test]
fn an_unknown_command_offers_the_nearest_and_tab_takes_it() {
    let mut app = after("jj:arhcive\r");
    assert_eq!(foot(&mut app).1, "E not a command: :arhcive · did you mean :archive? Tab to fix");
    press(&mut app, "\t");
    assert_eq!(foot(&mut app).1, ":archive");
    press(&mut app, "\r");
    assert_eq!(foot(&mut app).1, "✓ Archived 1 message · u to undo");
    assert_eq!(said(":xyzzy\r"), "E not a command: :xyzzy");
    assert_eq!(said(":arhcive\rl"), said("l"), "any other key drops the offer");
    assert_eq!(at(":arhcive\rj\t"), "1 of 9 · 1 message", "and Tab is the next section again");
}

#[test]
fn a_move_names_a_folder_of_the_message_s_account() {
    assert_eq!(said("j:move archive\r"), "✓ Moved 5 messages to Archive · u to undo");
    let mut app = after("j:move Recipts\r");
    assert_eq!(foot(&mut app).1, "E :move: no folder \"Recipts\" in personal · Tab completes");
    assert!(app.asks().is_empty(), "nothing moved");
    press(&mut app, "\t");
    let rows = screen(&mut app, 120, 40);
    assert_eq!((rows[32].as_str(), rows[39].as_str()), ("  Folders in personal", "  :move Sent"));
    assert_eq!(said(":move\r"), "E :move: name a folder · Tab completes");
    assert_eq!(said("jjjM"), ":move", "M opens the command line");
    assert_eq!(said("j:archive now\r"), "E :archive: a target cannot be named · it takes the selected or marked rows");
}

#[test]
fn flag_and_read_toggle_and_u_reverses_the_last_action_first() {
    let mut app = after("jjs");
    assert_eq!(foot(&mut app).1, "✓ Flagged 1 message · u to undo");
    assert!(screen(&mut app, 120, 40)[0].contains("Flagged 4"));
    press(&mut app, "m");
    assert_eq!(foot(&mut app).1, "✓ Marked 1 message read · u to undo");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Inbox 8 "));
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · 1 message unread again");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · 1 message unflagged");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "E undo: nothing to undo");
    let asked: Vec<(u64, Action)> = app.asks().into_iter().map(|ask| (ask.serial, ask.action)).collect();
    let want = [Action::Flag(true), Action::Seen(true), Action::Undo, Action::Undo];
    assert_eq!(asked, (1..).zip(want).collect::<Vec<_>>());

    assert_eq!(said("jjjs"), "✓ Flagged 3 messages · u to undo", "a thread not flagged throughout is flagged");
    assert_eq!(said("jjjss"), "✓ Unflagged 3 messages · u to undo");
    assert_eq!(said("jjjm"), "✓ Marked 3 messages read · u to undo");
    assert_eq!(said("jjjmm"), "✓ Marked 3 messages unread · u to undo");
    assert_eq!(said(":unread\r"), "✓ Marked 1 message unread · u to undo");
    assert_eq!(said("jj:read\r:flag\r"), "✓ Flagged 1 message · u to undo");
    assert_eq!(said("4e"), "E :archive: nothing is selected");
}

/// ADR-014: a change the server refuses is undone here and reported.
#[test]
fn a_refusal_puts_back_only_what_the_refused_action_changed() {
    let mut app = after("jjsm");
    let flag = app.asks().remove(0);
    app.event(Event::ActionFailed {
        serial: flag.serial,
        reason: "NO flags are read-only".into(),
        hint: String::new(),
    });
    assert_eq!(foot(&mut app).1, "E :flag: work refused it · NO flags are read-only · 1 message unflagged");
    let top = screen(&mut app, 120, 40)[0].clone();
    assert!(top.starts_with("  Inbox 8   Unread   Flagged 3 "), "the read mark made since stays: {top}");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · 1 message unread again", "and it is what u reverses");
    app.event(Event::ActionFailed { serial: 99, reason: "NO".into(), hint: String::new() });
    assert_eq!(foot(&mut app).1, "✓ Undone · 1 message unread again", "a refusal of nothing known says nothing");
}

/// ADR-040: what answers no ask of this run is said, and puts nothing back.
#[test]
fn a_notice_is_said_as_the_engine_worded_it() {
    let mut app = after("s");
    let notice = |error, text: &str, hint: &str| Event::Notice {
        account: tuimex_core::AccountId("work".into()),
        error,
        text: text.into(),
        hint: hint.into(),
    };
    app.event(notice(true, "work refused a change made earlier: NO", "1 message back in INBOX"));
    assert_eq!(foot(&mut app).1, "E work refused a change made earlier: NO · 1 message back in INBOX");
    app.event(notice(false, "work will purge the originals itself", ""));
    assert_eq!(foot(&mut app).1, "work will purge the originals itself");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · 1 message unflagged", "the flag set before them is still what u reverses");
}

#[test]
fn a_confirm_takes_y_or_enter_and_any_other_key_cancels_and_is_dropped() {
    let mut app = after("jj:empty-trash\r");
    assert_eq!(foot(&mut app).1, "Delete 212 messages in work's Trash for good?  y yes · n no", "the row's account");
    press(&mut app, "e");
    assert_eq!(
        foot(&mut app),
        ("3 of 24 · 1 message".into(), "Enter open · e archive · d trash · r reply · ? help".into())
    );
    assert!(app.asks().is_empty(), "the e that cancelled archived nothing");

    for yes in ["y", "\r"] {
        let (mut app, mut fake) = (after("d:empty-trash work\r"), fx::Fake::default());
        app.asks();
        press(&mut app, yes);
        assert_eq!(foot(&mut app).1, "Emptying work's Trash", "no count before the store has counted");
        told(&mut app, &mut fake);
        assert_eq!(fake.asked[0].action, Action::EmptyTrash(AccountId("work".into())));
        assert_eq!(foot(&mut app).1, "✓ Deleted 212 messages from work's Trash");
        assert_eq!(app.mailbox.folder(&AccountId("work".into()), Role::Trash).map(|f| f.total), Some(0));
        assert!(app.mailbox.mail.iter().all(|m| m.envelope.folder != FolderId("Trash".into())));
        press(&mut app, "u");
        assert_eq!(foot(&mut app).1, "E undo: nothing to undo", "what was deleted for good cannot be brought back");
    }
    assert_eq!(said(":empty-trash nobody\r"), "E :empty-trash: no account \"nobody\" · Tab completes");
}

#[test]
fn messages_lists_what_was_said_newest_first_and_esc_closes_it() {
    let mut app = after("e:nope\rjs:messages\r");
    let rows = screen(&mut app, 120, 40);
    let panel: Vec<&str> = rows[33..38].iter().map(|row| row.trim()).collect();
    let want =
        ["Messages", "14:20  ✓ Flagged 1 message", "14:20  E not a command: :nope", "14:20  ✓ Archived 1 message", ""];
    assert_eq!(panel, want, "without the keys to press next");
    assert_eq!(rows[39], "  j k scroll · Esc back");
    press(&mut app, "jjjjk");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[34], "  Messages");
    assert!(rows[35].contains("E not a command"), "j and k scroll, and stop at the last entry: {}", rows[35]);
    press(&mut app, "\x1b");
    assert!(!app.quit);
    assert_eq!(foot(&mut app).1, "Enter open · e archive · d trash · r reply · ? help");
}

#[test]
fn messages_keeps_the_last_two_hundred() {
    let mut app = app();
    (0..150).for_each(|_| press(&mut app, "s"));
    press(&mut app, ":nope\r");
    (0..60).for_each(|_| press(&mut app, "s"));
    assert_eq!(app.log.len(), 200);
}

#[test]
fn the_outbox_is_a_tab_while_it_holds_mail_and_u_takes_a_held_message_back() {
    let mut app = after("7");
    assert_eq!(foot(&mut app), ("1 of 2 · waiting to send".into(), "u back to Drafts · ? help".into()));
    press(&mut app, "u");
    assert_eq!(foot(&mut app), ("1 of 1 · not sent".into(), "✓ Not sent · back in Drafts".into()));
    assert_eq!(
        app.asks().pop().map(|ask| (ask.action, ask.on)),
        Some((Action::Undo, vec![fx::id("personal", "out-rack")]))
    );
    press(&mut app, "d");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[0], "  Inbox 9   Unread   Flagged 3   Drafts   Sent   Archive", "the tab goes with the last of it");
    assert_eq!(rows[39], "  ✓ Trashed 1 message · u to undo");
    assert_eq!(rows[38], "", "the last section is open, and nothing is left to say on the right");
    press(&mut app, "4");
    assert!(screen(&mut app, 120, 40)[3].contains("Re: Bike rack: still want it?"), "the message is in Drafts");
}

#[test]
fn a_send_is_held_then_sent_or_refused() {
    let mut app = app();
    app.send(fx::draft(), 10);
    assert_eq!(foot(&mut app).1, "Sending to 3 recipients in 10s · u to stop");
    assert_eq!(app.asks().pop().map(|ask| ask.action), Some(Action::Send));
    let id = fx::draft().envelope.id;
    app.event(Event::SendFailed { id: id.clone(), reason: "550 no such user".into() });
    assert!(screen(&mut app, 120, 40)[0].ends_with("Outbox 2"));
    press(&mut app, ":messages\r");
    assert!(screen(&mut app, 120, 40)[35].ends_with("E send: 550 no such user"));
    app.event(Event::Sent { id, answered: Vec::new() });
    press(&mut app, "\x1b5");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[3].contains("Re: Contract draft v3"), "it is in Sent: {}", rows[3]);
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "E undo: nothing to undo", "what has gone cannot be taken back");
}

#[test]
fn help_shows_only_what_matches_what_is_typed_under_its_headings_and_esc_closes_it() {
    let mut app = after("?");
    let rows = screen(&mut app, 120, 40);
    assert_eq!((rows[0].as_str(), rows[39].as_str()), ("  Help", "  type to filter · ^d ^u scroll · Esc back"));
    assert_eq!(rows[38].split("   ").next(), Some("  144 keys and commands · lines 1-35 of 70"));
    // Scrolled to the end, then filtered to less than a page: the page
    // is shown from its top, and says no lines.
    press(&mut app, "^d^d^dagent");
    let rows = screen(&mut app, 120, 40);
    let shown: Vec<&str> = rows[2..14].iter().map(|row| row.trim()).collect();
    let agent =
        ["Agent (after A)", "r                        write a reply", "t                        tidy, in compose"];
    assert_eq!(shown[..2], ["Everywhere", "A                        what the agent can do"]);
    assert_eq!(shown[3..6], agent, "a section whose heading matches is shown whole");
    assert_eq!(shown[8..10], ["Commands", ":agent reply|tidy|link   reply, tidy or link: hand text to the agent"]);
    assert!(rows[14..37].iter().all(|row| row.is_empty()), "what does not match is not drawn: {rows:#?}");
    assert_eq!((rows[38].split("   ").next(), rows[39].as_str()), (Some("  5 of 144 shown"), "  /agent"));
    press(&mut app, "\x7f\x7f\x7f\x7f\x7fQq\x7f");
    assert!(!app.quit, "q is typed, not obeyed");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(
        (rows[38].split("   ").next(), rows[39].as_str()),
        (Some("  2 of 144 shown"), "  /Q"),
        "whatever the case"
    );
    press(&mut app, "zzz");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[2..37].iter().all(|row| row.is_empty()) && rows[38].starts_with("  0 of 144 shown"), "{rows:#?}");
    press(&mut app, "\x1b");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Inbox 9"));
}

#[test]
fn sections_and_accounts_are_named_on_the_command_line() {
    assert!(at(":section flagged\r").starts_with("1 of 3 "));
    assert!(at(":section outbox\r").starts_with("1 of 2 "));
    assert_eq!(said(":section Spam\r"), "E :section: no section \"Spam\" · Tab completes");
    let mut app = after(":account personal\r");
    assert_eq!(
        foot(&mut app),
        (
            "personal only · 1 of 11 · thread of 5".into(),
            "Showing personal only · gt for the next, :account for all".into()
        )
    );
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Inbox 4   Unread   Flagged   Drafts"), "every tab is narrowed");
    press(&mut app, ":account\r");
    assert_eq!(foot(&mut app), ("1 of 24 · 1 message".into(), "Showing every account".into()));
    assert_eq!(said(":account nobody\r"), "E :account: no account \"nobody\" · Tab completes");

    let mut app = after("l:sync\r");
    assert_eq!(foot(&mut app).1, "Syncing Unread");
    press(&mut app, ":sync all\r");
    assert_eq!(foot(&mut app).1, "Syncing every account");
    let asked: Vec<Action> = app.asks().into_iter().map(|ask| ask.action).collect();
    assert_eq!(asked, [Action::Sync { all: false }, Action::Sync { all: true }]);
    assert!(after(":q\r").quit && after(":qa\r").quit);
}

#[test]
fn p_shows_and_hides_the_preview_and_ctrl_l_asks_for_a_redraw() {
    let mut app = app();
    assert!(screen(&mut app, 120, 40)[22].starts_with("    From"));
    press(&mut app, "p");
    assert!(screen(&mut app, 120, 40).iter().all(|row| !row.starts_with("    From")));
    press(&mut app, "p");
    assert!(screen(&mut app, 120, 40)[22].starts_with("    From"), "and p brings it back where it was");
    press(&mut app, "^l");
    assert!(app.take_redraw() && !app.take_redraw());
}

#[test]
fn enter_opens_the_row_s_message_marks_it_read_and_esc_returns_with_the_cursor_kept() {
    let mut app = after("jjjj\r");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[0], "  This week: winter routes, a gear swap and one trail closure");
    assert!(rows[2].starts_with("    From     Fernway Weekly <news@fernway.example.net>"), "{}", rows[2]);
    let asked: Vec<Action> = app.asks().into_iter().map(|ask| ask.action).collect();
    assert_eq!(asked, [Action::Read, Action::Seen(true)]);
    for back in ["\x1b", ":q\r"] {
        let mut app = after(&format!("jjjj\r{back}"));
        assert!(!app.quit);
        assert!(at("jjjj").starts_with("5 of 24"));
        assert!(foot(&mut app).0.starts_with("5 of 24"), "{}", foot(&mut app).0);
        assert_eq!(screen(&mut app, 120, 40)[0], "  Inbox 8   Unread   Flagged 3   Drafts   Sent   Archive   Outbox 1");
    }
}

#[test]
fn the_reader_scrolls_and_says_which_lines_show() {
    let mut app = reading("halden-3", "bb");
    assert_eq!(foot(&mut app).0, "lines 1-27 of 30");
    for (keys, lines) in
        [("j", "2-28"), ("jjjjjj", "4-30"), ("G", "4-30"), ("Gk", "3-29"), ("Ggg", "1-27"), ("^d^u", "1-27")]
    {
        press(&mut app, "gg");
        foot(&mut app);
        press(&mut app, keys);
        assert_eq!(foot(&mut app).0, format!("lines {lines} of 30"), "{keys}");
    }
    assert_eq!(foot(&mut reading("bank", "jjj")).0, "", "a body that fits does not scroll");
}

#[test]
fn n_and_b_walk_the_thread_and_capital_j_and_k_the_list() {
    let place = |app: &mut App| screen(app, 120, 40)[0].split("   ").last().unwrap().trim().to_string();
    let mut app = reading("halden-3", "");
    assert_eq!(place(&mut app), "3 of 3 in the thread");
    press(&mut app, "b");
    assert_eq!(place(&mut app), "2 of 3 in the thread");
    press(&mut app, "bb");
    assert_eq!(place(&mut app), "1 of 3 in the thread");
    assert_eq!(foot(&mut app).1, "E no earlier message in the thread");
    press(&mut app, "nnn");
    assert_eq!(foot(&mut app).1, "E no later message in the thread");

    let mut app = reading("halden-3", "J");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  This week: winter routes"));
    press(&mut app, "KK");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  [ops] Nightly backup"));
    press(&mut app, "\x1b");
    assert!(foot(&mut app).0.starts_with("3 of 24"), "the list's cursor went with it");
}

#[test]
fn an_action_in_the_reader_takes_the_message_open_and_not_its_thread() {
    let mut app = reading("halden-3", "be");
    assert_eq!(foot(&mut app).1, "✓ Archived 1 message · u to undo");
    assert!(foot(&mut app).0.contains("thread of 2"), "back in the list: {}", foot(&mut app).0);
    let archived = app.asks().into_iter().find(|ask| ask.action == Action::Archive).unwrap();
    assert_eq!(archived.on.iter().map(|id| id.key.as_str()).collect::<Vec<_>>(), ["halden-2"]);

    let mut app = reading("bank", "s");
    assert_eq!(foot(&mut app).1, "✓ Flagged 1 message · u to undo");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Action needed"), "a flag leaves the reader open");
    press(&mut app, "u");
    assert_eq!(foot(&mut app).1, "✓ Undone · 1 message unflagged");
}

/// ADR-018: the real host is on the screen before anything opens, a link
/// that deceives is asked about, and one that is neither web nor mail is
/// shown and refused.
#[test]
fn a_link_opens_by_number_after_assessment() {
    let mut app = reading("newsletter", "o");
    let rows = screen(&mut app, 120, 40);
    assert!(
        rows.iter().any(|r| r.contains(" 5  click.opentrack.example.net   view it in a browser   tracking service"))
    );
    assert_eq!(foot(&mut app).0, "opens https://fernway.example.net/routes/winter");
    press(&mut app, "j");
    assert_eq!(foot(&mut app).0, "opens https://shop.example.net/swap/tables");
    press(&mut app, "\r");
    assert_eq!(foot(&mut app).1, "Opening shop.example.net");
    assert_eq!(app.asks().pop().unwrap().action, Action::OpenLink(2));

    for answer in ["n", "x", "\x1b"] {
        let mut app = reading("bank", "o1");
        assert!(foot(&mut app).1.starts_with("▲ Text says bank.example.com; opens bank.example.com.device-check"));
        press(&mut app, answer);
        assert!(app.asks().is_empty(), "{answer} opened it");
        assert!(foot(&mut app).1.starts_with("r reply"), "the list of links closed: {}", foot(&mut app).1);
    }
    let mut app = reading("bank", ":link 1\ry");
    assert_eq!(foot(&mut app).1, "Opening bank.example.com.device-check.example.net");
    assert_eq!(app.asks().pop().unwrap().action, Action::OpenLink(1));
    assert_eq!(foot(&mut reading("bank", ":link 7\r")).1, "E :link: this message has no link 7 · o lists them");
    assert_eq!(foot(&mut reading("bank", ":link\r")).1, "E :link: give a number · :link <n>");
    assert_eq!(said(":link 1\r"), "E :link: no message is open · Enter opens one");
}

#[test]
fn files_are_saved_at_once_and_opened_after_a_question_naming_the_type() {
    let mut app = reading("photos", "ajj\r");
    assert_eq!(foot(&mut app).1, "Saving group-shot.heic");
    assert_eq!(app.asks().pop().unwrap().action, Action::SaveAttachment(2, "".into()));
    let mut app = reading("photos", ":save 4 ~/maps/saddle.gpx\r");
    assert_eq!(app.asks().pop().unwrap().action, Action::SaveAttachment(3, "~/maps/saddle.gpx".into()));

    let mut app = reading("photos", "aO");
    assert_eq!(foot(&mut app).1, "Open ridge-01.jpg (image/jpeg) in the system's program for it?  y yes · n no");
    assert!(app.asks().is_empty(), "nothing is opened before the answer");
    press(&mut app, "y");
    assert_eq!(app.asks().pop().unwrap().action, Action::OpenAttachment(0));
    assert!(reading("photos", ":open 2\rn").asks().is_empty());
    assert_eq!(foot(&mut reading("photos", ":open 9\r")).1, "E :open: this message has no file 9 · a lists them");
    assert_eq!(foot(&mut reading("bank", "a")).1, "E a: this message has no files");
}

/// ADR-009: the sealed copy is one key away, and loading anything remote
/// is asked first, each time, naming every host.
#[test]
fn the_browser_copy_asks_each_time_before_it_may_load_pictures() {
    let hosts = "img.fernway.example.net, cdn.example.net";
    for (answer, remote, loads) in [("y", true, "8 images load from 2 hosts"), ("n", false, "nothing remote loads")] {
        let mut app = reading("newsletter", "B");
        assert_eq!(foot(&mut app).1, format!("Load 8 images from {hosts}?  y yes · n no"));
        assert!(app.asks().is_empty(), "nothing is opened before the answer");
        press(&mut app, answer);
        assert_eq!(app.asks().pop().unwrap().action, Action::Browser { remote });
        assert_eq!(foot(&mut app).1, format!("Opening the sealed copy in your browser · {loads}"));
        press(&mut app, ":browser\r");
        assert!(foot(&mut app).1.starts_with("Load 8 images"), "{answer} was remembered");
    }
    for leaves in ["\x1b", "x"] {
        let mut app = reading("newsletter", &format!("B{leaves}"));
        assert!(app.asks().is_empty(), "{leaves:?} opened it");
        assert!(foot(&mut app).1.starts_with("r reply"), "{}", foot(&mut app).1);
    }
    let mut app = reading("newsletter", "B");
    let narrow = screen(&mut app, 74, 24).pop().unwrap();
    assert_eq!(narrow.trim(), "Load 8 images from img.fernway.example.net and 1 more?  y yes · n no");
    assert_eq!(screen(&mut app, 60, 24).pop().unwrap().trim(), "Load 8 images from 2 hosts?  y yes · n no");

    let mut app = reading("bank", "B");
    assert_eq!(app.asks().pop().unwrap().action, Action::Browser { remote: false });
    assert_eq!(foot(&mut app).1, "Opening the sealed copy in your browser · nothing remote loads");
}

#[test]
fn unsubscribe_is_never_automatic() {
    let mut app = reading("newsletter", ":unsubscribe\r");
    assert_eq!(foot(&mut app).1, "Unsubscribe at fernway.example.net?  y yes · n no");
    assert!(app.asks().is_empty());
    press(&mut app, "y");
    assert_eq!(app.asks().pop().unwrap().action, Action::Unsubscribe);
    assert_eq!(foot(&mut reading("bank", ":unsubscribe\r")).1, "E :unsubscribe: this message names no way to");
}

/// ADR-031: the question is there before the agent answers, and what the
/// agent says decides nothing.
#[test]
fn agent_link_asks_at_once_and_shows_what_comes_back_as_advice() {
    let mut app = reading("bank", ":agent link 1\r");
    let ask = app.asks().pop().unwrap();
    assert_eq!(ask.action, Action::AgentLink(1));
    assert!(foot(&mut app).1.starts_with("▲ Text says bank.example.com"));
    let shows = |app: &mut App, text: &str| screen(app, 120, 40).iter().any(|row| row.contains(text));
    assert!(shows(&mut app, "◇ Asking the agent"));
    app.event(Event::AgentAdvice { serial: ask.serial + 1, advice: "opening".into(), why: "not this ask's".into() });
    assert!(!shows(&mut app, "Agent advises"));
    app.event(Event::AgentAdvice { serial: ask.serial, advice: "opening".into(), why: "it looks fine".into() });
    assert!(shows(&mut app, "◇ Agent advises opening · it looks fine"));
    assert!(app.asks().is_empty(), "advice opens nothing");
    press(&mut app, "n");
    assert!(app.asks().is_empty() && !shows(&mut app, "Agent advises"));

    let mut app = reading("bank", ":agent link 2\r");
    let serial = app.asks().pop().unwrap().serial;
    assert_eq!(foot(&mut app).1, "Open examplebank.example.net?  y yes · n no");
    app.event(Event::ActionFailed { serial, reason: "the agent command exited with 1".into(), hint: String::new() });
    assert!(shows(&mut app, "E :agent link: the agent command exited with 1 · nothing changed"));
    press(&mut app, "y");
    assert_eq!(app.asks().pop().unwrap().action, Action::OpenLink(2));
}

/// The design's rulings on links: `A` then `l` asks about the selected
/// one, the question over a deceptive one offers `A`, and once asked the
/// question keeps its two answers.
#[test]
fn capital_a_asks_the_agent_about_a_link_from_the_list_and_from_its_question() {
    let shows = |app: &mut App, text: &str| screen(app, 120, 40).iter().any(|row| row.contains(text));
    let mut app = reading("bank", "o");
    assert_eq!(foot(&mut app).1, "1-9 open · j k Enter choose · A agent · Esc close");
    press(&mut app, "jA");
    assert_eq!(foot(&mut app), ("nothing is sent until you pick".into(), "Esc cancel".into()));
    assert!(shows(&mut app, "l   review this link") && app.asks().is_empty(), "the menu sends nothing");
    press(&mut app, "l");
    assert_eq!(app.asks().pop().unwrap().action, Action::AgentLink(2));
    assert_eq!(foot(&mut app).1, "Open examplebank.example.net?  y yes · n no");
    assert!(shows(&mut app, "◇ Asking the agent"));

    let mut app = reading("bank", "o1");
    assert!(foot(&mut app).1.ends_with("Open?  y yes · n no · A ask the agent"), "{}", foot(&mut app).1);
    let narrow = screen(&mut app, 80, 24).pop().unwrap();
    assert!(narrow.trim_end().ends_with("Open?  y yes · n no · A ask the agent"), "{narrow}");
    press(&mut app, "A");
    let asked: Vec<Action> = app.asks().into_iter().map(|ask| ask.action).collect();
    assert_eq!(asked, [Action::AgentLink(1)], "the agent is asked, and nothing opens");
    assert!(foot(&mut app).1.ends_with("Open?  y yes · n no"), "{}", foot(&mut app).1);
    assert!(shows(&mut app, "◇ Asking the agent"));
    // Asked once: a second `A` is any other key, which leaves the question.
    press(&mut app, "A");
    assert!(app.asks().is_empty() && foot(&mut app).1.starts_with("r reply"), "{}", foot(&mut app).1);

    let mut app = reading("bank", "o1Ay");
    let asked: Vec<Action> = app.asks().into_iter().map(|ask| ask.action).collect();
    assert_eq!(asked, [Action::AgentLink(1), Action::OpenLink(1)], "y answers without waiting for the agent");
}

/// The menu offers what the agent can do where it was opened, sends
/// nothing itself, and any key that is not one of its picks closes it.
#[test]
fn capital_a_opens_the_agent_s_menu_and_only_a_pick_sends() {
    let shows = |app: &mut App, text: &str| screen(app, 120, 40).iter().any(|row| row.contains(text));
    let mut app = after("jjjA");
    assert!(shows(&mut app, "r   write a reply") && !shows(&mut app, "tidy what I wrote"));
    assert_eq!(foot(&mut app), ("nothing is sent until you pick".into(), "Esc cancel".into()));
    press(&mut app, "t");
    assert!(app.asks().is_empty() && foot(&mut app).0.starts_with("4 of 24"), "t is not offered on a row, and closes");
    press(&mut app, "A\x1b");
    assert!(app.asks().is_empty() && foot(&mut app).0.starts_with("4 of 24"), "Esc cancels");
    press(&mut app, "Ar");
    let asked: Vec<Action> = app.asks().into_iter().map(|ask| ask.action).collect();
    assert!(matches!(asked[..], [Action::Agent(tuimex_core::AgentAsk::Reply { .. })]), "{asked:?}");
    assert!(shows(&mut app, "◇ Writing a reply"), "compose is open on the reply, and the agent is writing it");

    let mut app = reading("bank", "A");
    assert!(shows(&mut app, "r   write a reply"), "the reader offers the same one choice");
    let mut app = after("c\t\t\t\tA");
    assert!(shows(&mut app, "r   write a reply") && shows(&mut app, "t   tidy what I wrote"));
    let mut app = after("cA");
    assert!(!shows(&mut app, "write a reply") && shows(&mut app, "To       A"), "in To it is a letter");
}

/// `B` in the list is the reader's `B`, of the row's message: the body is
/// fetched first when it is not here, and the same question is asked
/// before the copy may load pictures.
#[test]
fn capital_b_on_a_row_fetches_the_message_and_asks_the_reader_s_question() {
    let (mut app, mut fake) = (after("jjjjB"), fx::Fake::default());
    assert_eq!(foot(&mut app).1, "Fetching this message · the browser copy opens when it is here");
    let asked = app.asks();
    assert_eq!((asked.len(), &asked[0].action, asked[0].on[0].key.as_str()), (1, &Action::Read, "newsletter"));
    asked.into_iter().flat_map(|ask| fake.ask(ask)).for_each(|event| app.event(event));
    let hosts = "img.fernway.example.net, cdn.example.net";
    assert_eq!(foot(&mut app).1, format!("Load 8 images from {hosts}?  y yes · n no"));
    assert!(foot(&mut app).0.starts_with("5 of 24"), "the list is still what is shown");
    press(&mut app, "y");
    let asked = app.asks();
    assert_eq!((&asked[0].action, asked[0].on[0].key.as_str()), (&Action::Browser { remote: true }, "newsletter"));
    assert_eq!(foot(&mut app).1, "Opening the sealed copy in your browser · 8 images load from 2 hosts");
    // Whoever makes the copy makes it of what they last read, so the
    // message is read again, with nothing said of a text that is here;
    // `n` declines the pictures and not the copy.
    press(&mut app, "B");
    assert!(foot(&mut app).1.starts_with("Enter open"), "{}", foot(&mut app).1);
    told(&mut app, &mut fake);
    press(&mut app, "n");
    assert_eq!(app.asks().pop().unwrap().action, Action::Browser { remote: false });

    // On a thread it is the newest message, the one the preview shows,
    // and from the focused preview too.
    let mut app = after("jjj");
    screen(&mut app, 120, 40);
    press(&mut app, "⌥jB");
    assert_eq!(app.asks().pop().unwrap().on[0].key, fx::HALDEN);

    // A body that arrives after the cursor has moved on opens nothing.
    let mut app = after("jjjjB");
    let late: Vec<Event> = app.asks().into_iter().flat_map(|ask| fake.ask(ask)).collect();
    press(&mut app, "j");
    late.into_iter().for_each(|event| app.event(event));
    assert!(app.asks().is_empty() && foot(&mut app).1.starts_with("Enter open"), "{}", foot(&mut app).1);
}

/// The preview is the whole message: resting on a row asks for its body
/// once, a row with no opening says the body is on its way, and when it
/// is here the preview scrolls to its end.
#[test]
fn the_preview_reads_the_message_the_cursor_rests_on_and_shows_all_of_it() {
    let (mut app, mut fake) = (after("jjjjj"), fx::Fake::default());
    let long = "Staging database migration window";
    assert_eq!(screen(&mut app, 120, 40)[20], format!("  {long}"));
    assert!(app.asks().is_empty() && app.restless(), "stepping onto a row asks for nothing");
    app.rest();
    let asked = app.asks();
    assert_eq!((asked.len(), &asked[0].action, asked[0].on[0].key.as_str()), (1, &Action::Read, "migration"));
    app.rest();
    assert!(app.asks().is_empty() && !app.restless(), "and it is asked for once");
    asked.into_iter().flat_map(|ask| fake.ask(ask)).for_each(|event| app.event(event));
    press(&mut app, "⌥jG");
    let rows = screen(&mut app, 120, 40);
    assert!(foot(&mut app).0.starts_with("lines ") && foot(&mut app).0.ends_with("-55 of 55"), "{}", foot(&mut app).0);
    assert_eq!((rows[35].trim(), rows[36].trim()), ("Thanks,", "Lars"), "the end of the message: {rows:#?}");

    // A message with no opening here says its body is being fetched,
    // until it comes or cannot.
    let fetching = |app: &mut App| screen(app, 120, 40).iter().any(|row| row.trim() == "Fetching this message.");
    let mut app = after("G");
    let statement = app.rows().iter().position(|row| row.msg.envelope.id.key == "statement").unwrap();
    press(&mut app, &"k".repeat(23 - statement));
    assert!(fetching(&mut app) && app.restless());
    app.rest();
    let id = app.asks().pop().unwrap().on.pop().unwrap();
    assert!(fetching(&mut app), "while it is on its way");
    app.event(Event::BodyFailed { id, reason: "offline".into() });
    assert!(!fetching(&mut app) && !app.restless(), "one that cannot be read is not asked for again");

    // With no preview in sight there is nothing to read for.
    let mut app = after("pj");
    screen(&mut app, 120, 40);
    assert!(!app.restless());
    // Focusing the preview asks at once.
    let mut app = after("jj");
    screen(&mut app, 120, 40);
    press(&mut app, "⌥j");
    assert_eq!(app.asks().pop().unwrap().action, Action::Read);
}

/// Enter on a draft is compose, on the draft as it was saved: every
/// field, the files, and the whole body, of which a row holds the opening.
#[test]
fn enter_on_a_draft_opens_it_in_compose_as_it_was_saved_and_not_in_the_reader() {
    let mut drafts = fx::drafts();
    let whole = drafts[0].text.clone().unwrap();
    drafts[0].text = Some(whole[..40].to_string());
    drafts[0].envelope.bcc = vec![tuimex_core::Address { name: None, email: "audit@example.org".into() }];
    let mut app = over(fx::inbox().into_iter().chain(drafts.clone()).collect());
    press(&mut app, "4");
    assert_eq!(foot(&mut app).1, "Enter edit · d trash · ? help");
    press(&mut app, "\r");
    let asked = app.asks();
    assert_eq!((asked.len(), &asked[0].action, &asked[0].on[0]), (1, &Action::Read, &drafts[0].envelope.id));
    assert!(app.reading.id().is_none() && app.compose.is_none(), "nothing opens until the body is here");
    let body = Box::new(tuimex_core::Body { text: whole.clone(), ..fx::body(&drafts[0]) });
    app.event(Event::BodyReady { id: drafts[0].envelope.id.clone(), body });
    let rows = screen(&mut app, 120, 40);
    let shows = |text: &str| rows.iter().any(|row| row.trim() == text);
    assert!(app.reading.id().is_none(), "the reader is not what opens");
    assert!(
        shows("To       Priya Raman <priya@example.org>") && shows("Dmitri Volkov <dmitri@example.org>"),
        "{rows:#?}"
    );
    assert!(shows("Cc       billing@halden.example.com") && shows("Bcc      audit@example.org"), "{rows:#?}");
    assert!(shows("Files    contract-v3-signed.pdf       512 KB") && shows("schedule-b.xlsx               38 KB"));
    assert!(shows("◇ Written by an agent · 23 Nov 13:52") && shows("Thanks,") && shows("Robin"), "{rows:#?}");
    let compose = app.compose.as_ref().unwrap();
    assert_eq!((compose.body.as_str(), compose.files.len()), (whole.as_str(), 2), "the body is whole");
    // Read once, it opens at once the next time.
    press(&mut app, "\x1b\r");
    assert!(app.compose.is_some() && app.asks().is_empty());

    // The offline mailbox answers for its drafts as for its inbox.
    let (mut app, mut fake) = (over(fx::inbox().into_iter().chain(fx::drafts()).collect()), fx::Fake::default());
    press(&mut app, "4\r");
    told(&mut app, &mut fake);
    assert!(app.compose.is_some(), "the draft's body was read, and compose opened on it");
}

/// ADR-031: with no `[agent]` table the key says what is missing, and
/// nothing is handed on.
#[test]
fn without_an_agent_capital_a_says_what_is_missing_and_nothing_leaves() {
    let mut app = reading("bank", "");
    app.set_up(fx::identities(), false);
    press(&mut app, "o");
    assert_eq!(foot(&mut app).1, "1-9 open · j k Enter choose · A agent · Esc close");
    press(&mut app, "A");
    assert_eq!(foot(&mut app).1, "E A: no agent is set up · add an [agent] table to the config");
    assert!(app.asks().is_empty());
    press(&mut app, "o1");
    assert!(foot(&mut app).1.ends_with("Open?  y yes · n no"), "{}", foot(&mut app).1);
    press(&mut app, "A");
    assert!(app.asks().is_empty());
    press(&mut app, ":agent \t");
    assert_eq!(foot(&mut app).1, ":agent", "nothing completes a command that is not there");
}

#[test]
fn tab_after_agent_offers_its_functions_and_after_link_the_links_of_the_message_open() {
    let has = |app: &mut App, text: &str| screen(app, 120, 40).iter().any(|row| row.trim_end().ends_with(text));
    let mut app = after(":agent \t");
    assert_eq!(foot(&mut app).1, ":agent reply");
    assert!(has(&mut app, "  :agent"));
    assert!(has(&mut app, "▌   reply [guidance]        write the body from the thread"));
    assert!(has(&mut app, "    tidy                    clean up the body you wrote"));
    assert!(has(&mut app, "    link <n>                ask about a link; the agent opens nothing"));
    press(&mut app, "\t\t");
    assert_eq!(foot(&mut app).1, ":agent link");
    press(&mut app, " \t");
    assert_eq!(foot(&mut app).1, ":agent link", "no message is open, so there is no link to offer");

    let mut app = reading("bank", ":agent link \t");
    assert_eq!(foot(&mut app).1, ":agent link 1");
    assert!(has(&mut app, "  :agent link"));
    assert!(has(&mut app, "▌   1   bank.example.com.device-check.example.net"));
    assert!(has(&mut app, "    2   examplebank.example.net"));
    press(&mut app, "\t\r");
    assert_eq!(app.asks().pop().unwrap().action, Action::AgentLink(2));
}

#[test]
fn what_was_done_outside_is_said_and_what_failed_is_an_error() {
    let mut app = reading("photos", "a\r");
    let serial = app.asks().pop().unwrap().serial;
    app.event(Event::ActionDone { summary: "Saved ridge-01.jpg to Downloads".into() });
    assert_eq!(foot(&mut app).1, "✓ Saved ridge-01.jpg to Downloads");
    app.event(Event::ActionFailed {
        serial,
        reason: ":save: ridge-01.jpg is there already".into(),
        hint: ":save 1 <path>".into(),
    });
    assert_eq!(foot(&mut app).1, "E :save: ridge-01.jpg is there already · :save 1 <path>");
}

/// PLAN §6.5: a signature nobody checked is said to be unchecked, and
/// never drawn as good.
#[test]
fn a_signed_message_without_a_verdict_claims_none() {
    let mut app = after("jjj\rbb");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[8], "    ▲ Signature not checked · tuimex does not verify one yet");
    assert!(!rows.iter().any(|row| row.contains("Signed by")));
}

/// A message with more links than a third of the screen: the list shows
/// the ones around the cursor, and the status line the one it is on.
#[test]
fn a_long_list_of_links_shows_the_ones_around_the_cursor() {
    let mut app = reading("bank", "");
    let mut body = fx::body(&fx::inbox().into_iter().find(|m| m.envelope.id.key == "bank").unwrap());
    let link =
        |n: usize| tuimex_core::Link { n, url: format!("https://www.example.com/{n}"), shown: format!("page {n}") };
    body.links = (1..=20).map(link).collect();
    app.event(Event::BodyReady { id: fx::id("personal", "bank"), body: Box::new(body) });
    press(&mut app, "o");
    press(&mut app, &"j".repeat(15));
    let rows = screen(&mut app, 120, 24);
    assert_eq!(foot(&mut app).0, "opens https://www.example.com/16");
    assert!(rows.iter().any(|r| r.starts_with("▌   16  www.example.com   page 16")), "{rows:#?}");
    assert!(!rows.iter().any(|r| r.contains("page 8")) && rows.iter().any(|r| r.contains("page 9")), "{rows:#?}");
}

#[test]
fn q_on_the_command_line_leaves_the_reader_before_it_leaves_tuimex() {
    let mut app = reading("bank", ":q\r");
    assert!(!app.quit && foot(&mut app).0.starts_with("7 of 24"));
    press(&mut app, ":q\r");
    assert!(app.quit);
}

/// A terminal that draws pictures draws them wherever `pictures` says, so
/// it says nothing once help, a draft or the list is what the frame shows.
#[test]
fn a_picture_is_reported_only_while_the_reader_s_own_frame_shows_its_placeholder() {
    let mut app = reading("photos", "");
    let shade = |app: &mut App| screen(app, 120, 40).iter().any(|row| row.contains("░░░░"));
    assert!(shade(&mut app));
    let (id, placed) = app.pictures().expect("a message is open");
    assert_eq!((id.key.as_str(), placed.len()), ("photos", 1));
    // The row it gives is counted from the top of the screen, which the
    // bar of places is on.
    let shaded = |line: &Line| line.iter().any(|span| span.text.contains("░░░░"));
    let first = app.lines(120, 40).iter().position(shaded);
    assert_eq!(app.pictures().map(|(_, placed)| placed[0].row), first);
    for (covered_by, back) in [("?", "\x1b"), ("r", "\x1b"), ("a", "\x1b")] {
        press(&mut app, covered_by);
        let drawn = shade(&mut app);
        let placed = app.pictures().map_or(0, |(_, placed)| placed.len());
        assert_eq!(placed, usize::from(drawn), "after {covered_by}, a picture is reported where one is drawn");
        press(&mut app, back);
        assert!(shade(&mut app) && app.pictures().is_some_and(|(_, placed)| placed.len() == 1), "back from it");
    }
    press(&mut app, "\x1b");
    assert!(app.pictures().is_none(), "the preview's placeholder is its own, and no picture is drawn over the list");
}

/// A mailbox the size of a long-kept one: `count` messages in two accounts,
/// a fifth of them in the Inbox, every fourth a reply to the one before.
fn large(count: u32) -> Vec<Listed> {
    let template = fx::inbox().remove(0);
    (0..count)
        .map(|n| {
            let mut m = template.clone();
            let account = if n % 2 == 0 { "personal" } else { "work" };
            m.envelope.id = fx::id(account, &format!("m{n}"));
            m.envelope.folder = tuimex_core::FolderId(if n % 5 == 0 { "Inbox" } else { "Archive" }.to_string());
            m.envelope.date = tuimex_core::Timestamp(fx::NOW.0 - i64::from(n) * 600);
            m.envelope.subject = format!("Subject number {n} of the long mailbox");
            m.envelope.message_id = Some(format!("<m{n}@example.com>"));
            m.envelope.references = if n % 4 == 1 { vec![format!("<m{}@example.com>", n - 1)] } else { Vec::new() };
            m.envelope.flags.seen = n % 7 != 0;
            m.envelope.flags.flagged = n % 97 == 0;
            m.snippet = format!("The opening of message {n}, as the index keeps it.");
            m
        })
        .collect()
}

/// Run by hand, optimised, for the numbers PLAN §9 quotes:
/// `cargo test --release -p tuimex-ui --lib fifty_thousand -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, which a loaded machine would fail"]
fn fifty_thousand_messages_are_drawn_and_moved_through_within_a_frame() {
    let started = std::time::Instant::now();
    let mut app = over(large(50_000));
    app.lines(120, 40);
    println!("first frame: {:?}", started.elapsed());
    fn time(app: &mut App, what: &str, keys: &str) -> std::time::Duration {
        let started = std::time::Instant::now();
        for _ in 0..20 {
            press(app, keys);
            app.lines(120, 40);
        }
        let each = started.elapsed() / 20;
        println!("{what}, and a frame: {each:?}");
        each
    }
    let inbox = [time(&mut app, "j", "j"), time(&mut app, "to the next section and back", "lh")];
    press(&mut app, ":section Archive\r");
    assert!(foot(&mut app).0.starts_with("1 of 32500 "), "{:?}", foot(&mut app));
    let archive = [
        time(&mut app, "j", "j"),
        time(&mut app, "flag", "s"),
        time(&mut app, "trash", "d"),
        time(&mut app, "undo", "u"),
        time(&mut app, "archive in a section of 10,000", ":section Inbox\re"),
    ];
    press(&mut app, ":section Archive\r/subject:number 4");
    let typed = time(&mut app, "two keys of a filter", "2\x7f") / 2;
    // Moving is within a frame at sixty a second. What changes which
    // messages a section of tens of thousands holds threads it again, and
    // is within three.
    assert!(inbox.iter().all(|each| each.as_millis() < 16), "{inbox:?}");
    assert!(archive.iter().chain([&typed]).all(|each| each.as_millis() < 50), "{archive:?} {typed:?}");
}

/// Run by hand, optimised, for the numbers PLAN §9 quotes:
/// `cargo test --release -p tuimex-ui --lib a_filter_is_typed_over -- --ignored --nocapture`.
/// Each key is timed with the frame that follows it. Every subject here
/// holds `number`, so each of its letters is asked of the whole section,
/// and the `4` after it of every message through all its text: the most a
/// key can cost.
#[test]
#[ignore = "a measurement, which a loaded machine would fail"]
fn a_filter_is_typed_over_fifty_and_a_hundred_and_fifty_thousand_messages_a_key_at_a_time() {
    for (count, within) in [(50_000, 16), (150_000, 50)] {
        let mut slowest = std::time::Duration::ZERO;
        let mut app = over(large(count));
        press(&mut app, ":section Archive\r");
        app.lines(120, 40);
        println!("{count} messages, {}", foot(&mut app).0);
        for (what, keys) in [
            ("/", "/"),
            ("the first letter", "n"),
            ("each letter after it", "umber 41"),
            ("each letter erased", "\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7f"),
            ("each letter of a term", "\x7fsubject:number"),
        ] {
            let (mut all, mut worst) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
            for key in keys.chars() {
                let started = std::time::Instant::now();
                press(&mut app, &key.to_string());
                app.lines(120, 40);
                all += started.elapsed();
                worst = worst.max(started.elapsed());
            }
            println!("  {what}: {:?} a key with its frame, {worst:?} the slowest", all / keys.chars().count() as u32);
            slowest = slowest.max(worst);
        }
        // Within a frame at sixty a second in a section of 32,500, and
        // within three in one of 97,500.
        assert!(slowest.as_millis() < within, "{slowest:?} over {count}");
    }
}

/// A frame is made from the rows near the cursor and from threads kept
/// since the last change, and an action finds its row by number. Through
/// two thousand keys over a mailbox of threads, each frame is the one made
/// from every row with nothing kept, and each numbered row is that row.
#[test]
fn a_frame_from_the_rows_near_the_cursor_and_what_is_kept_is_the_frame_from_them_all() {
    let mut app = over(large(1500));
    let keys =
        ["j", "j", "j", "k", "^d", "^u", "G", "gg", "s", "m", "e", "d", "u", "zo", "zc", "l", "h", " ", "V", "\x1b"];
    // First what chance seldom reaches: a thread unfolded far down, and
    // rows above it leaving.
    press(&mut app, ":section Archive\r");
    let thread = app.rows().iter().skip(200).position(|row| row.fold.is_some()).unwrap() + 200;
    let down = "j".repeat(thread);
    let mut scripted =
        vec![down.as_str(), "zo", "j", "s", "k", "k", "k", "e", "d", "u", "gg", "e", "G", "k", "d"].into_iter();
    let mut seed = 7u32;
    for step in 0..2000 {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let key = scripted.next().unwrap_or(keys[(seed >> 16) as usize % keys.len()]);
        press(&mut app, key);
        match (seed >> 8) % 4 {
            0 => answered(&mut app),
            // A message arrives, and at times another is gone from the store.
            gone @ (1 | 2) => {
                let mut arrived = app.mailbox.mail[(seed >> 4) as usize % app.mailbox.mail.len()].clone();
                arrived.envelope.id = fx::id("work", &format!("arrived{step}"));
                arrived.envelope.message_id = Some(format!("<arrived{step}@example.com>"));
                let leaves = app.mailbox.mail[(seed >> 12) as usize % app.mailbox.mail.len()].envelope.id.clone();
                let asked: Vec<_> =
                    [arrived.envelope.id.clone()].into_iter().chain((gone == 2).then_some(leaves)).collect();
                let folders = app.mailbox.folders.clone();
                app.asks();
                app.refresh(fx::NOW, u64::MAX, Some(&asked), vec![arrived], folders);
            }
            _ => {}
        }
        let (width, height) = (120, [40, 24, 12][step % 3]);
        let top = app.top;
        let near = app.lines_under(width, height);
        let (near_top, kept) = (std::mem::replace(&mut app.top, top), app.threads.take());
        let right = app.alerts();
        let all = app.list_lines(width, height, usize::MAX, right);
        assert_eq!((near, near_top), (all, app.top), "step {step}, after {key:?}");
        app.threads.replace(kept);

        let rows = app.rows();
        assert_eq!(app.count(), rows.len(), "step {step}, after {key:?}");
        let far = (seed >> 3) as usize % rows.len().max(1);
        for at in (app.selected.saturating_sub(3)..app.selected + 4).chain([far]) {
            let Some(row) = rows.get(at) else { continue };
            let numbered = app.row(at).expect("a row of that number");
            let id = &row.msg.envelope.id;
            assert_eq!((&numbered.msg.envelope.id, &numbered.ids, numbered.fold), (id, &row.ids, row.fold));
            let takes = rows.iter().position(|r| r.ids.contains(&id));
            assert_eq!(app.row_of(id), takes, "step {step}, after {key:?}");
        }
    }
}

/// A filter being typed is asked of what the text before the key matched,
/// and its frame and its cursor come from the threads holding a match.
/// Through filters typed, erased and typed over, with threads unfolded and
/// mail arriving between keys, each frame is the one made from every row
/// with every message asked afresh, and the cursor is on the first row
/// after it that matches.
#[test]
fn a_filter_typed_a_key_at_a_time_shows_and_finds_what_asking_every_row_afresh_does() {
    let mut app = over(large(1500));
    let filters = [
        "number 4",
        "subject:number 12",
        "is:unread 7",
        "of the long",
        "message 10",
        "from:nobody",
        "\"number 1\" mailbox",
        "is:flagged opening",
        "zz",
        " 3  of",
        "to:grace is:read number 9",
    ];
    let between = ["j", "^d", "G", "gg", "zo", "zo", "zc", "l", "h", "k"];
    press(&mut app, ":section Archive\r");
    // Threads left unfolded, whose rows match one by one.
    for skip in [2, 40, 300] {
        let thread = app.rows().iter().skip(skip).position(|row| row.fold.is_some()).unwrap() + skip;
        press(&mut app, &format!("gg{}zo", "j".repeat(thread)));
    }
    let (mut seed, mut step) = (11u32, 0);
    let mut next = move |below: usize| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (seed >> 16) as usize % below
    };
    for round in 0..40 {
        for _ in 0..next(6) {
            press(&mut app, between[next(between.len())]);
        }
        let erased = "\x7f".repeat(next(8));
        let typed = format!("/{}{erased}{}", filters[next(filters.len())], filters[next(filters.len())]);
        for key in typed.chars() {
            if next(9) == 0 {
                let mut arrived = app.mailbox.mail[next(app.mailbox.mail.len())].clone();
                arrived.envelope.id = fx::id("work", &format!("arrived{step}"));
                arrived.envelope.message_id = Some(format!("<arrived{step}@example.com>"));
                let (asked, folders) = ([arrived.envelope.id.clone()], app.mailbox.folders.clone());
                app.refresh(fx::NOW, u64::MAX, Some(&asked), vec![arrived], folders);
            }
            let from = app.selected;
            press(&mut app, &key.to_string());
            step += 1;
            let said = format!("round {round}, {:?} after {key:?}", app.filter);
            let rows = app.rows();
            let hit = app.matching(&rows).unwrap();
            let sought =
                (0..rows.len()).map(|k| (from + k) % rows.len()).find(|at| hit.contains(&rows[*at].msg.envelope.id));
            assert_eq!(app.selected, sought.unwrap_or(from), "{said}");
            drop(rows);

            let (width, height) = (120, [40, 24, 12][step % 3]);
            let top = app.top;
            let near = app.lines_under(width, height);
            let near_top = std::mem::replace(&mut app.top, top);
            let right = app.alerts();
            let all = app.list_lines(width, height, usize::MAX, right);
            assert_eq!((near, near_top), (all, app.top), "{said}");
        }
        press(&mut app, "\x1b");
    }
}

/// What the store answers an action with: the messages it named, as they
/// are now shown.
fn answered(app: &mut App) {
    for ask in app.asks() {
        let rows = app.mailbox.mail.iter().filter(|m| ask.on.contains(&m.envelope.id)).cloned().collect();
        let folders = app.mailbox.folders.clone();
        app.refresh(fx::NOW, ask.serial, Some(&ask.on), rows, folders);
    }
}

/// Run by hand, optimised, for the numbers PLAN §9 quotes:
/// `cargo test --release -p tuimex-ui --lib held_key -- --ignored --nocapture`.
/// A held key repeats every 33 ms, and what is typed while one key is
/// handled is handled before the next frame, so the key and the store's
/// answer to it are timed apart from the frame.
#[test]
#[ignore = "a measurement, which a loaded machine would fail"]
fn a_held_key_over_a_hundred_and_fifty_thousand_messages_is_handled_well_within_its_repeat() {
    let mut app = over(large(150_000));
    app.lines(120, 40);
    fn time(app: &mut App, what: &str, keys: &str) -> std::time::Duration {
        let (mut handled, mut drawn) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        for _ in 0..30 {
            let started = std::time::Instant::now();
            press(app, keys);
            answered(app);
            handled += started.elapsed();
            let started = std::time::Instant::now();
            app.lines(120, 40);
            drawn += started.elapsed();
        }
        println!("{what}: {:?} a key with the store's answer, {:?} a frame", handled / 30, drawn / 30);
        handled / 30 + drawn / 30
    }
    let mut each = vec![time(&mut app, "j in the Inbox of 30,000", "j"), time(&mut app, "flag there", "s")];
    each.push(time(&mut app, "archive there", "e"));
    each.push(time(&mut app, "trash there", "d"));
    press(&mut app, ":section Flagged\r");
    each.push(time(&mut app, "unflag in Flagged", "s"));
    press(&mut app, ":section Archive\r");
    each.push(time(&mut app, "j in the Archive of 120,000", "j"));
    each.push(time(&mut app, "flag there", "s"));
    each.push(time(&mut app, "read there", "m"));
    each.push(time(&mut app, "trash there", "d"));
    // Mail arriving is not a key, and is timed for what it holds a key up.
    let started = std::time::Instant::now();
    for n in 0..10 {
        let mut arrived = app.mailbox.mail[0].clone();
        arrived.envelope.id = fx::id("work", &format!("arrived{n}"));
        (arrived.envelope.message_id, arrived.envelope.references) =
            (Some(format!("<arrived{n}@example.com>")), Vec::new());
        let folders = app.mailbox.folders.clone();
        app.refresh(fx::NOW, 0, Some(std::slice::from_ref(&arrived.envelope.id.clone())), vec![arrived], folders);
        app.lines(120, 40);
    }
    println!("a message arriving in the Inbox, and a frame: {:?}", started.elapsed() / 10);
    assert!(each.iter().all(|each| each.as_millis() < 16), "{each:?}");
}

/// What a sync brought is taken in under the cursor, which stays on its
/// message: a row arriving above it, the row above it leaving, its own
/// flags changing.
#[test]
fn a_refresh_takes_what_the_store_holds_and_keeps_the_cursor_on_its_message() {
    let mut app = after("jj");
    let before = screen(&mut app, 120, 40);
    assert!(foot(&mut app).0.starts_with("3 of 24 "), "{:?}", foot(&mut app));
    let on = fx::inbox().into_iter().find(|m| m.envelope.id.key == "backup").unwrap();
    assert!(before.iter().any(|line| line.starts_with('▌') && line.contains(&on.envelope.subject)), "{before:#?}");

    let mut arrived = fx::inbox().remove(0);
    arrived.envelope.id = fx::id("work", "arrived");
    (arrived.envelope.date, arrived.envelope.subject) = (fx::NOW, "Just arrived".to_string());
    (arrived.envelope.message_id, arrived.envelope.flags.seen) = (Some("<arrived@example.com>".into()), false);
    app.refresh(fx::NOW, 0, Some(&[arrived.envelope.id.clone()]), vec![arrived], fx::folders());
    let now = screen(&mut app, 120, 40);
    assert!(now[0].starts_with("  Inbox 10 "), "{}", now[0]);
    assert!(now.iter().any(|line| line.contains("Just arrived")));
    assert!(foot(&mut app).0.starts_with("4 of 25 "), "{:?}", foot(&mut app));

    app.refresh(fx::NOW, 0, Some(&[fx::id("work", "arrived"), fx::id("work", "invite")]), Vec::new(), fx::folders());
    assert!(foot(&mut app).0.starts_with("2 of 23 "), "what the store no longer has is gone: {:?}", foot(&mut app));
    let unread = Listed { envelope: Envelope { flags: Flags::default(), ..on.envelope.clone() }, ..on.clone() };
    app.refresh(fx::NOW, 0, Some(std::slice::from_ref(&on.envelope.id)), vec![unread], fx::folders());
    let now = screen(&mut app, 120, 40);
    assert!(now.iter().any(|line| line.starts_with('▌') && line.contains(&on.envelope.subject)), "{now:#?}");

    let everything: Vec<Listed> = fx::inbox().into_iter().filter(|m| m.envelope.id.account.0 == "work").collect();
    let rows = crate::list::rows(&everything, &[]).len();
    app.refresh(fx::NOW, 0, None, everything, fx::folders());
    let said = foot(&mut app).0;
    assert!(said.contains(&format!(" of {rows} ")), "with nothing named, the mail is what was handed over: {said}");
}

/// The store does not hold why a message was not sent, or how long it has
/// left; a refresh leaves both as they are known here.
#[test]
fn a_refresh_leaves_what_is_known_of_a_message_being_sent() {
    let mut app = app();
    let failed = fx::outbox().into_iter().find(|m| matches!(m.sending, Some(Sending::Failed { .. }))).unwrap();
    let mut folders = fx::folders();
    let outbox = FolderId("Outbox".to_string());
    folders.push(Folder {
        id: outbox.clone(),
        account: failed.envelope.id.account.clone(),
        name: "Outbox".into(),
        role: Role::Outbox,
        unseen: 0,
        total: 1,
    });
    let stored = Listed { sending: None, envelope: Envelope { folder: outbox, ..failed.envelope.clone() }, ..failed };
    let id = stored.envelope.id.clone();
    app.refresh(fx::NOW, 0, Some(std::slice::from_ref(&id)), vec![stored], folders);
    let top = screen(&mut app, 120, 40)[0].clone();
    assert!(top.trim_end().ends_with("Outbox 1"), "{top}");
}

/// The design's `list-failing` and `list-first-sync`, from the events
/// that cause them.
#[test]
fn an_account_out_of_step_with_its_server_says_so_until_it_syncs() {
    /// The whole status line.
    fn status(app: &mut App) -> String {
        let lines = screen(app, 120, 40);
        lines[lines.len() - 2].trim_end().to_string()
    }
    let mut app = over(fx::inbox());
    let work = AccountId("work".to_string());
    let failed = |msg: &str, retry_in| Event::SyncError { account: work.clone(), msg: msg.into(), retry_in };
    app.event(failed("work: offline", Some(std::time::Duration::from_secs(16))));
    let lines = screen(&mut app, 120, 40);
    assert!(status(&mut app).ends_with("work offline · retry in 16s"), "{:?}", status(&mut app));
    let stale = lines.iter().filter(|line| line.contains("stale")).count();
    assert_eq!(stale, 7, "the rows of work's in sight, as the design's list-failing has them, and none of personal's");
    press(&mut app, ":messages\r");
    assert!(screen(&mut app, 120, 40).iter().any(|line| line.contains("E sync: work: offline")));
    press(&mut app, "\x1b");

    let synced = Event::Synced { account: work.clone(), folder: FolderId("INBOX".into()), changed: Vec::new() };
    app.event(synced.clone());
    assert!(!screen(&mut app, 120, 40).iter().any(|line| line.contains("stale") || line.contains("offline")));
    app.event(failed("signing in to work failed; run `tuimex setup`", None));
    let said = "work signing in to work failed; run `tuimex setup` · last synced 14:20";
    assert!(status(&mut app).ends_with(said), "{:?}", status(&mut app));

    app.event(synced.clone());
    let first = |done| Event::FirstSync { account: work.clone(), folder: FolderId("INBOX".into()), done, total: 3812 };
    app.event(first(1240));
    assert!(status(&mut app).ends_with("work first sync · 1,240 of 3,812"), "{:?}", status(&mut app));
    app.event(first(3812));
    assert!(!status(&mut app).contains("first sync"), "{:?}", status(&mut app));
}

#[test]
fn a_section_is_opened_by_name_or_short_name_and_one_that_is_not_there_is_not() {
    let mut app = app();
    assert!(app.section("flagged") && screen(&mut app, 120, 40).iter().any(|line| line.contains("1 of 3 ")));
    assert!(!app.section("nowhere"));
}

/// A refresh read before an ask was carried out knows nothing of it; one
/// read after it is the store's word on the same message.
#[test]
fn a_refresh_read_before_an_ask_leaves_what_the_ask_changed_as_it_is_shown() {
    let mut app = after("e");
    let ask = app.asks().remove(0);
    let stale: Vec<Listed> = fx::inbox().into_iter().filter(|m| ask.on.contains(&m.envelope.id)).collect();
    assert!(foot(&mut app).0.starts_with("1 of 23 "), "{:?}", foot(&mut app));
    app.refresh(fx::NOW, ask.serial - 1, Some(&ask.on), stale.clone(), fx::folders());
    assert!(foot(&mut app).0.starts_with("1 of 23 "), "it stays archived: {:?}", foot(&mut app));
    app.refresh(fx::NOW, ask.serial, Some(&ask.on), stale, fx::folders());
    assert!(foot(&mut app).0.contains(" of 24 "), "the store has the last word: {:?}", foot(&mut app));
}

/// The loop moves the clock with `tick`, and draws only when it says so.
#[test]
fn a_held_message_a_retry_and_the_age_of_a_sync_count_with_the_clock_and_a_second_that_changes_nothing_draws_nothing() {
    use tuimex_core::Timestamp;
    let at = |seconds: i64| Timestamp(fx::NOW.0 + seconds);
    let status = |app: &mut App| screen(app, 120, 40)[38].clone();
    let mut app = app();
    assert!(!app.tick(fx::NOW), "no time has passed");
    assert!(app.tick(at(1)));
    assert!(status(&mut app).ends_with("sending in 5s · 1 not sent"), "{}", status(&mut app));
    assert!(!app.tick(at(1)), "the same second again");
    assert!(app.tick(at(4)));
    assert!(status(&mut app).ends_with("sending in 2s · 1 not sent"), "{}", status(&mut app));
    let work = AccountId("work".into());
    let retry_in = Some(std::time::Duration::from_secs(8));
    app.event(Event::SyncError { account: work.clone(), msg: "work: no route to the server".into(), retry_in });
    assert!(app.tick(at(30)));
    let late = status(&mut app);
    assert!(late.contains("sending in 0s") && late.contains("retry in 0s"), "neither counts below nothing: {late}");

    let mut idle = over(Vec::new());
    idle.event(Event::Synced { account: work, folder: FolderId("INBOX".into()), changed: Vec::new() });
    let age = |app: &mut App| screen(app, 120, 40).into_iter().find(|row| row.contains("Synced")).unwrap();
    assert!(idle.tick(at(59)));
    assert_eq!(age(&mut idle).trim(), "Synced 59s ago.");
    assert!(idle.tick(at(61)));
    assert_eq!(age(&mut idle).trim(), "Synced 1m ago.");
    assert!(!idle.tick(at(62)), "nothing on the screen names this second");
    assert!(idle.tick(at(120)), "the minute turned");
}

/// The store hands over no `sender_was`: it is worked out from the thread
/// when the message is opened, and a refresh from the store keeps it.
#[test]
fn a_sender_whose_name_first_wrote_to_the_thread_from_another_address_is_noticed_in_real_mail() {
    let stored = || fx::inbox().into_iter().map(|m| Listed { sender_was: None, ..m }).collect::<Vec<_>>();
    let (mail, mut app) = (stored(), over(stored()));
    let row = crate::list::rows(&mail, &[]).iter().position(|row| row.msg.envelope.id.key == "halden-3").unwrap();
    press(&mut app, &format!("{}\r", "j".repeat(row)));
    let notice = "Sender differs from the thread · earlier messages came from priya@example.org";
    let noticed = |app: &mut App| screen(app, 120, 40).iter().any(|line| line.contains(notice));
    assert!(noticed(&mut app), "{:#?}", screen(&mut app, 120, 40));
    let again: Vec<Listed> = stored().into_iter().filter(|m| m.envelope.id.key == "halden-3").collect();
    app.refresh(fx::NOW, 0, Some(&[fx::id("work", "halden-3")]), again, fx::folders());
    assert!(noticed(&mut app), "the store's row says nothing of it, and it is still said");
    // The message the name first wrote is not the odd one out, and nor
    // is anyone else's.
    press(&mut app, "b");
    assert!(!noticed(&mut app));
    press(&mut app, "b");
    assert!(!noticed(&mut app));
}

/// PLAN §4.1: an account whose server has no Archive is asked before one
/// is made, and what was pressed is then done.
#[test]
fn archiving_in_an_account_with_no_archive_asks_to_make_one_and_then_archives() {
    let without = |folders: Vec<Folder>| -> Vec<Folder> {
        folders.into_iter().filter(|f| !(f.account.0 == "work" && f.role == Role::Archive)).collect()
    };
    let mut app = app();
    app.refresh(fx::NOW, 0, Some(&[]), Vec::new(), without(fx::folders()));
    let on = |app: &mut App| screen(app, 120, 40).into_iter().find(|row| row.starts_with('▌')).unwrap();
    let first = on(&mut app);
    assert!(first.contains("Invitation: Q4 roadmap review"), "a message of the work account: {first}");
    press(&mut app, "e");
    let asked = screen(&mut app, 120, 40).pop().unwrap();
    assert_eq!(asked, "  work has no Archive folder. Create one on its server?  y yes · n no");
    press(&mut app, "n");
    assert!(app.asks().is_empty() && on(&mut app) == first, "no is no: nothing is made and nothing moved");
    press(&mut app, "ey");
    let asks = app.asks();
    assert_eq!(asks[0].action, Action::CreateArchive(AccountId("work".into())));
    assert_eq!((&asks[1].action, asks[1].on.len()), (&Action::Archive, 1));
    assert!(on(&mut app) != first, "the row has left the Inbox");
    assert_eq!(foot(&mut app).1, "✓ Archived 1 message · u to undo");
    press(&mut app, "u");
    assert_eq!(on(&mut app), first, "and u brings it back as from any Archive");
}

/// Three accounts, two of them alike in name, over the design's mailbox.
fn three() -> App {
    three_drawn_with(Palette::new(Tier::True))
}

fn three_drawn_with(palette: Palette) -> App {
    let mail = fx::among_three(fx::inbox().into_iter().chain(fx::filed()).collect());
    let folders = fx::TRIO.iter().flat_map(|account| fx::folders_of(account, [8, 1, 0, 1, 0])).collect();
    let mailbox =
        Mailbox { now: fx::NOW, sections: fx::sections(), folders, mail, synced: Some(60), me: fx::addresses() };
    let mut app = App::new(Theme { palette, glyphs: Glyphs::Unicode }, mailbox);
    let identity = |name: &&str| tuimex_core::Identity {
        account: AccountId(name.to_string()),
        address: tuimex_core::Address { name: None, email: format!("robin@{name}.example.net") },
    };
    app.set_up(fx::TRIO.iter().map(identity).collect(), true);
    app
}

/// The text and the colour of the account column of each row in sight.
fn account_column(app: &mut App, width: usize) -> Vec<(String, crate::palette::Tok)> {
    account_column_of(app, width, &crate::layout::ACCOUNTS_BUILTIN)
}

/// As `account_column`, where an account may wear any of `colours`.
fn account_column_of(
    app: &mut App,
    width: usize,
    colours: &[crate::palette::Tok],
) -> Vec<(String, crate::palette::Tok)> {
    let lines = under(app, width, 40);
    let named = |line: &&Line| line.len() > 5 && fx::TRIO.iter().any(|name| name.contains(line[4].text.as_str()));
    let is_row = |line: &&Line| named(line) && line[4].text.len() >= 4 && colours.contains(&line[4].fg);
    lines[2..38].iter().filter(is_row).map(|line| (line[4].text.clone(), line[4].fg)).collect()
}

#[test]
fn three_accounts_have_three_names_in_three_colours_at_every_width_of_the_column() {
    use crate::palette::Tok;
    let colours = [("home", Tok::Link), ("work", Tok::Head), ("proton", Tok::Ok)];
    for (width, names) in [(120, ["gmail-home", "gmail-work", "proton"]), (82, ["home", "work", "prot"])] {
        let column = account_column(&mut three(), width);
        assert!(column.len() >= 10, "{width}: {column:?}");
        for (i, name) in names.iter().enumerate() {
            let shown: Vec<&(String, Tok)> = column.iter().filter(|(text, _)| text == name).collect();
            assert!(!shown.is_empty(), "{width}: no row of {name} in {column:?}");
            assert!(shown.iter().all(|(_, colour)| *colour == colours[i].1), "{width}: {name} in one colour");
        }
        assert!(column.iter().all(|(text, _)| names.contains(&text.as_str())), "{width}: {column:?}");
    }
}

#[test]
fn gt_steps_through_all_three_accounts_and_back_to_all_and_capital_t_steps_the_other_way() {
    let mut app = three();
    let tabs = |app: &mut App| screen(app, 120, 40)[0].clone();
    assert_eq!(tabs(&mut app), "  Inbox 9   Unread   Flagged 3   Drafts   Sent   Archive");
    let steps = [
        ("gmail-home only · 1 of 11 · thread of 5", "  Inbox 4   Unread   Flagged   Drafts", "gmail-home"),
        ("gmail-work only · 1 of 7 · 1 message", "  Inbox 4   Unread   Flagged 2   Drafts", "gmail-work"),
        ("proton only · 1 of 6 · 1 message", "  Inbox 1   Unread   Flagged 1   Drafts", "proton"),
    ];
    for (left, counts, account) in steps {
        press(&mut app, "gt");
        let said = format!("Showing {account} only · gt for the next, :account for all");
        assert_eq!(foot(&mut app), (left.to_string(), said));
        assert!(tabs(&mut app).starts_with(counts), "{account}: {}", tabs(&mut app));
        assert!(account_column(&mut app, 120).is_empty(), "{account}: one account needs no column");
    }
    press(&mut app, "gt");
    assert_eq!(foot(&mut app), ("1 of 24 · 1 message".to_string(), "Showing every account".to_string()));
    assert!(tabs(&mut app).starts_with("  Inbox 9   Unread   Flagged 3"));
    for (left, _, _) in steps.iter().rev() {
        press(&mut app, "gT");
        assert_eq!(foot(&mut app).0, *left, "gT is the account before");
    }
    press(&mut app, "gT");
    assert_eq!(foot(&mut app).1, "Showing every account");
}

#[test]
fn the_account_command_completes_all_three_accounts() {
    let mut app = three();
    press(&mut app, ":account \t");
    let rows = screen(&mut app, 120, 40);
    let panel: Vec<&str> = rows[33..37].iter().map(|row| row.trim()).collect();
    assert_eq!(panel, ["Accounts", "▌   gmail-home", "gmail-work", "proton"]);
    press(&mut app, "\t\t\r");
    assert!(foot(&mut app).0.starts_with("proton only · 1 of 6"), "{}", foot(&mut app).0);
}

#[test]
fn a_search_spans_all_three_accounts_and_each_row_says_its_own() {
    let (mut app, mut fake) = (three(), fx::Fake::default());
    press(&mut app, "/nov\r");
    assert_eq!(screen(&mut app, 120, 40)[3], "    Searching all mail.", "until the answer comes");
    // The design's answer names its two accounts; here the same messages
    // are the three accounts'.
    for event in app.asks().into_iter().flat_map(|ask| fake.ask(ask)) {
        let Event::Found { serial, mut hits, searched } = event else { continue };
        for hit in &mut hits {
            let here = app.mailbox.mail.iter().find(|m| m.envelope.id.key == hit.id.key).unwrap();
            hit.id = here.envelope.id.clone();
        }
        app.event(Event::Found { serial, hits, searched });
    }
    let rows = screen(&mut app, 120, 40);
    assert!(rows[0].ends_with("Archive   Search nov"), "{}", rows[0]);
    let found: Vec<&String> = rows[2..14].iter().filter(|row| row.contains("Nov")).collect();
    assert_eq!(found.len(), 6, "{rows:#?}");
    for account in ["gmail-home ", "gmail-work ", "proton "] {
        assert!(found.iter().any(|row| row[4..].contains(account)), "a row of {account} in {found:#?}");
    }
    assert!(found[4].contains("gmail-home   to Greta Holm") && found[4].contains("Sent   9 Nov"), "{}", found[4]);
    assert!(found[3].contains("Notes from the vendor call  next review is in No"), "{}", found[3]);
}

#[test]
fn the_preview_and_the_status_line_are_of_the_selected_row_s_account() {
    let mut app = three();
    // The seventh row is proton's, between rows of the other two.
    press(&mut app, "jjjjjjj");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[10].starts_with("▌   proton       Mei Tanaka"), "{}", rows[10]);
    assert_eq!(rows[20], "  Payroll corrections for November", "the preview is that row's message");
    press(&mut app, "gtgtgt");
    assert_eq!(foot(&mut app).0, "proton only · 1 of 6 · 1 message");
    let status = &under(&mut app, 120, 40)[38];
    assert_eq!((status[1].text.as_str(), status[1].fg), ("proton", crate::palette::Tok::Ok), "in the account's colour");
    assert_eq!(screen(&mut app, 120, 40)[20], "  Payroll corrections for November", "and its first row is previewed");
}

/// `three`, drawn with an invented palette file's colours.
fn three_loaded(changed: &[(crate::palette::Tok, crate::palette::Rgb)]) -> App {
    three_drawn_with(Palette { tier: Tier::True, custom: Some(crate::palette::tests::invented(changed)) })
}

#[test]
fn three_accounts_with_a_loaded_palette_take_the_order_its_colours_give() {
    use crate::palette::Tok;
    let names = |app: &mut App| {
        let mut seen: Vec<(String, Tok)> = account_column_of(app, 120, &Tok::ALL);
        seen.sort_by(|a, b| a.0.cmp(&b.0));
        seen.dedup();
        seen
    };
    // `TUI.accountOrder` gives this palette green, blue, pink.
    let mut app = three_loaded(&crate::palette::tests::GREEN_FIRST);
    let took = [("gmail-home".to_string(), Tok::Ok), ("gmail-work".into(), Tok::Link), ("proton".into(), Tok::Head)];
    assert_eq!(names(&mut app), took);
    press(&mut app, "gt");
    let status = &under(&mut app, 120, 40)[38];
    assert_eq!((status[1].text.as_str(), status[1].fg), ("gmail-home", Tok::Ok), "and the status line follows");
}

#[test]
fn a_color_in_the_config_fixes_one_account_and_the_rest_keep_their_turn() {
    use crate::palette::Tok;
    let mut app = three();
    app.set_colours(vec![(AccountId("gmail-work".into()), Tok::Warn)]);
    let mut seen = account_column_of(&mut app, 120, &Tok::ALL);
    seen.sort_by(|a, b| a.0.cmp(&b.0));
    seen.dedup();
    let took = [("gmail-home".to_string(), Tok::Link), ("gmail-work".into(), Tok::Warn), ("proton".into(), Tok::Ok)];
    assert_eq!(seen, took, "the third account is still third in turn");
}

/// The design's fallback, which a palette with no accent to spare gets:
/// numbers and the bright words in `fg`, nouns and separators in `dim`.
#[test]
fn words_with_no_colour_that_would_show_are_told_apart_by_brightness() {
    use crate::palette::Tok;
    use crate::palette::tests::{CYAN_AS_DIM, GREEN_AS_TEXT};
    let mut app = three_loaded(&[CYAN_AS_DIM, GREEN_AS_TEXT]);
    press(&mut app, "p");
    let lines = under(&mut app, 140, 40);
    let words = |subject: &str| -> Vec<(String, Tok)> {
        let line = lines.iter().find(|line| line.iter().any(|s| s.text.contains(subject))).unwrap();
        let from = line.iter().position(|s| s.text.contains(subject)).unwrap();
        let drawn = line[from + 1..line.len() - 5].iter().filter(|s| !s.text.trim().is_empty());
        drawn.map(|s| (s.text.clone(), s.fg)).collect()
    };
    let (fg, dim) = (Tok::Fg, Tok::Dim);
    assert_eq!(
        words("Contract draft v3"),
        [("3".into(), fg), (" messages".into(), dim), (" · ".into(), dim), ("2".into(), fg), (" files".into(), dim)]
    );
    assert_eq!(words("Invitation: Q4"), [("invite".to_string(), fg)]);
    assert_eq!(words("Staging database"), [("replied".to_string(), fg)]);

    let mut cyan = three_loaded(&[]);
    press(&mut cyan, "p");
    let line = &under(&mut cyan, 140, 40)[3];
    assert!(line.iter().any(|s| s.text == "invite" && s.fg == Tok::Agent), "with an accent to spare they wear it");
}

#[test]
fn a_fifth_account_takes_the_first_one_s_colour() {
    use crate::palette::Tok;
    let accounts: Vec<AccountId> = ["a", "b", "c", "d", "e"].iter().map(|n| AccountId(n.to_string())).collect();
    let colours = crate::list::account_colours(&crate::layout::ACCOUNTS_BUILTIN, &accounts, &[]);
    assert_eq!(colours, [Tok::Link, Tok::Head, Tok::Ok, Tok::Warn, Tok::Link]);
}

/// The design's `search`: every message found is a row, wherever it is
/// filed, and Esc goes back to the row the search was asked from.
#[test]
fn enter_on_a_filter_searches_all_mail_and_esc_returns_with_the_cursor_kept() {
    let mail = fx::inbox().into_iter().chain(fx::outbox()).chain(fx::filed()).collect();
    let (mut app, mut fake) = (over(mail), fx::Fake::default());
    press(&mut app, "jjjj/nov\r");
    let asked = app.asks();
    assert_eq!(asked.last().map(|ask| &ask.action), Some(&Action::Search("nov".into())));
    asked.into_iter().flat_map(|ask| fake.ask(ask)).for_each(|event| app.event(event));
    assert_eq!(
        foot(&mut app),
        ("1 of 6 · 1 message".into(), "Enter open · e archive · r reply · Esc back · ? help".into())
    );
    press(&mut app, "N");
    assert_eq!(foot(&mut app).0, "6 of 6 · 1 message", "N goes round the results");
    press(&mut app, "nnnn");
    assert_eq!(foot(&mut app).0, "4 of 6 · 1 message");

    // An action is carried out on the message found, which stays a row.
    press(&mut app, "e");
    assert_eq!(foot(&mut app).1, "✓ Archived 1 message · u to undo");
    let rows = screen(&mut app, 120, 40);
    let vendor = rows.iter().find(|row| row.contains("Notes from the vendor call")).unwrap();
    assert!(vendor.contains("Archive  14 Nov"), "it is where it was moved to: {vendor}");
    assert_eq!(foot(&mut app).0, "4 of 6 · 1 message");

    // Enter opens it, Esc comes back to the results, and Esc again to the
    // row the search was asked from.
    press(&mut app, "\r");
    assert_eq!(screen(&mut app, 120, 40)[0], "  Notes from the vendor call");
    press(&mut app, "\x1b");
    assert!(screen(&mut app, 120, 40)[0].ends_with("Search nov"));
    press(&mut app, "\x1b");
    assert_eq!(screen(&mut app, 120, 40)[0], "  Inbox 9   Unread   Flagged 3   Drafts   Sent   Archive   Outbox 1");
    assert!(foot(&mut app).0.starts_with("5 of 23"), "{}", foot(&mut app).0);
}

#[test]
fn an_answer_to_a_search_no_longer_shown_is_dropped() {
    let (mut app, mut fake) = (app(), fx::Fake::default());
    press(&mut app, "/nov\r");
    let late: Vec<Event> = app.asks().into_iter().flat_map(|ask| fake.ask(ask)).collect();
    press(&mut app, "\x1b");
    late.into_iter().for_each(|event| app.event(event));
    assert!(screen(&mut app, 120, 40)[0].ends_with("Outbox 1"), "the section is still what is shown");
    assert_eq!(foot(&mut app).0, "1 of 24 · 1 message");
}

#[test]
fn alt_j_focuses_the_preview_which_then_scrolls_while_actions_still_take_the_row() {
    let mut app = after("jjjzo");
    // Where the preview lies is known once a frame has shown it.
    screen(&mut app, 120, 40);
    press(&mut app, "⌥j");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[20], "▌ Contract draft v3 for the Halden account", "the cursor is on the preview's subject");
    assert_eq!(foot(&mut app), ("lines 1-15 of 37".into(), "j k scroll · Alt-k list · Enter open · ? help".into()));
    for (keys, lines) in
        [("j", "2-16"), ("^d", "9-23"), ("G", "23-37"), ("k", "22-36"), ("gg", "1-15"), ("^d^u", "1-15")]
    {
        press(&mut app, keys);
        assert_eq!(foot(&mut app).0, format!("lines {lines} of 37"), "{keys}");
    }
    assert!(screen(&mut app, 120, 40)[6].starts_with("▌   work       Priya Raman"), "the row's cursor has not moved");

    // As in a second pane: what acts on a message acts on the selected
    // one, whichever pane has the focus.
    press(&mut app, "s");
    assert_eq!(foot(&mut app).1, "✓ Unflagged 1 message · u to undo");
    press(&mut app, "⌥k");
    assert_eq!(foot(&mut app).0, "4 of 26 · 1 message of the thread", "Alt-k is back in the list");
    press(&mut app, "⌥l\x1b");
    assert_eq!(
        foot(&mut app).0,
        "4 of 26 · 1 message of the thread",
        "Alt-l focuses too, and Esc leaves the preview first"
    );
    press(&mut app, "⌥jj");
    press(&mut app, "⌥hj");
    assert_eq!(foot(&mut app).0, "5 of 26 · 1 message of the thread", "and j moves the cursor again");
    assert_eq!(rows[19].chars().next(), Some('─'));
}

#[test]
fn the_preview_is_beside_the_list_in_a_wide_terminal_and_capital_p_and_the_command_move_it() {
    let mut app = app();
    let beside = |app: &mut App, width: usize| screen(app, width, 45)[2].contains('│');
    let below = |app: &mut App, width: usize| screen(app, width, 45).iter().any(|row| row.starts_with("────"));
    assert!(beside(&mut app, 160) && !below(&mut app, 160), "from 160 columns it is beside");
    assert!(below(&mut app, 159) && !beside(&mut app, 159));
    assert!(screen(&mut app, 180, 45)[2].ends_with("│  Invitation: Q4 roadmap review, Thu 26 Nov 10:00"));

    screen(&mut app, 180, 45);
    press(&mut app, "P");
    assert!(below(&mut app, 180), "P puts a preview that is beside below");
    press(&mut app, "P");
    assert!(beside(&mut app, 180) && beside(&mut app, 120), "and one that is below beside, at any width that has room");
    assert!(below(&mut app, 92), "a list with no room for its sender keeps the preview below");

    for (word, at_180, at_120) in
        [("below", "below", "below"), ("right", "beside", "beside"), ("auto", "beside", "below")]
    {
        press(&mut app, &format!(":preview {word}\r"));
        for (width, want) in [(180, at_180), (120, at_120)] {
            assert_eq!(beside(&mut app, width), want == "beside", ":preview {word} at {width}");
            assert_eq!(below(&mut app, width), want == "below", ":preview {word} at {width}");
        }
    }
    press(&mut app, ":preview off\r");
    assert!(!beside(&mut app, 180) && !below(&mut app, 180));
    press(&mut app, "p");
    assert!(beside(&mut app, 180), "p brings back what :preview off hid");
    assert_eq!(said(":preview sideways\r"), "E :preview: it takes auto, right, below or off · Tab completes");
    assert!(
        !below(&mut app, 180) || screen(&mut app, 180, 29).iter().all(|row| !row.contains('│')),
        "too short for one"
    );
    assert!(screen(&mut app, 180, 29).iter().all(|row| !row.contains('│') && !row.starts_with("────")));
}

#[test]
fn the_focus_leaves_a_preview_that_is_no_longer_drawn() {
    let mut app = app();
    screen(&mut app, 120, 40);
    press(&mut app, "⌥j");
    assert!(foot(&mut app).0.starts_with("lines "));
    assert!(screen(&mut app, 120, 24)[22].starts_with("  1 of 24"), "24 rows have no preview");
    assert!(foot(&mut app).0.starts_with("1 of 24"), "and the focus did not wait for it to come back");
    let mut app = after("p");
    screen(&mut app, 120, 40);
    press(&mut app, "⌥j");
    assert!(foot(&mut app).0.starts_with("1 of 24"), "a hidden preview takes no focus");
}

#[test]
fn a_sender_with_no_display_name_is_shown_as_the_address_once_and_not_as_a_b_a_b() {
    let mut mail = fx::inbox();
    let first = crate::list::rows(&mail, &[])[0].msg.envelope.id.clone();
    let m = mail.iter_mut().find(|m| m.envelope.id == first).unwrap();
    let address = m.envelope.from[0].email.clone();
    m.envelope.from[0].name = None;
    let mut app = over(mail);
    for keys in ["", "\r"] {
        press(&mut app, keys);
        let lines = screen(&mut app, 120, 40);
        let from = lines.iter().find(|line| line.trim_start().starts_with("From")).unwrap();
        assert_eq!(from.trim(), format!("From     {address}"), "in the preview, then in the reader");
    }
}

#[test]
fn a_long_saved_path_loses_its_middle_and_never_the_file_name_at_the_screen_edge() {
    let path = "~/Documents/clients/2026/northern-region/quarterly/received/Q3 figures (final).xlsx";
    let mut app = reading("halden-3", "");
    app.event(Event::Saved { path: path.into() });
    let said = |app: &mut App, width| screen(app, width, 40).pop().unwrap();
    assert_eq!(said(&mut app, 120), format!("  ✓ Saved {path}"), "whole where there is room");
    assert_eq!(said(&mut app, 60), "  ✓ Saved ~/Documents/clients/2026/…/Q3 figures (final).xlsx");
    assert_eq!(said(&mut app, 60).chars().count(), 60);
    assert_eq!(said(&mut app, 30), "  ✓ Saved …/Q3 figures (final).xlsx", "the name is given up last");
    press(&mut app, ":messages\r");
    assert!(screen(&mut app, 120, 40).iter().any(|line| line.contains(path)), "and :messages keeps it whole");
}

#[test]
fn an_empty_section_says_no_age_of_a_sync_until_a_sync_of_this_run_has_happened() {
    let mailbox = Mailbox { synced: None, mail: Vec::new(), ..app().mailbox };
    let mut app = App::new(Theme { palette: Palette::new(Tier::True), glyphs: Glyphs::Unicode }, mailbox);
    let shown = |app: &mut App| screen(app, 120, 40).into_iter().filter(|row| !row.is_empty()).nth(1).unwrap();
    assert_eq!(shown(&mut app).trim(), "Nothing here.");
    assert!(app.tick(tuimex_core::Timestamp(fx::NOW.0 + 90)), "the minute turned");
    assert!(!screen(&mut app, 120, 40).iter().any(|row| row.contains("Synced")), "time passing is not a sync");
    let (account, folder) = (AccountId("work".into()), FolderId("INBOX".into()));
    app.event(Event::Synced { account, folder, changed: Vec::new() });
    assert!(screen(&mut app, 120, 40).iter().any(|row| row.trim() == "Synced 0s ago."));
}

#[test]
fn the_row_of_a_message_says_replied_once_the_reply_to_it_has_been_sent() {
    let mut app = app();
    // The third row has no other word to say about itself.
    press(&mut app, "jj");
    let first = app.rows()[2].msg.envelope.id.clone();
    let row = |app: &mut App| screen(app, 120, 40).into_iter().find(|row| row.starts_with('▌')).unwrap();
    assert!(!row(&mut app).contains("replied"));
    app.send(fx::draft(), 10);
    app.event(Event::Sent { id: fx::draft().envelope.id, answered: vec![first] });
    assert!(row(&mut app).contains("replied"), "{}", row(&mut app));
}

#[test]
fn emptying_the_trash_says_how_many_were_deleted_and_what_was_kept_and_why() {
    let mut app = after(":empty-trash work\ry");
    let ask = app.asks().pop().unwrap();
    let account = AccountId("work".into());
    app.event(Event::Emptied { serial: ask.serial, account, deleted: 2, kept: 1 });
    let said = "✓ Deleted 2 messages from work's Trash · 1 kept, not yet on the server";
    assert_eq!(foot(&mut app).1, said);
    press(&mut app, ":messages\r");
    assert!(screen(&mut app, 120, 40).iter().any(|line| line.ends_with(&said[4..])), "and :messages keeps the reason");
}

/// ADR-047: a missing Trash is asked for as a missing Archive is, where
/// `d` once said "no Trash folder" and moved nothing.
#[test]
fn trashing_in_an_account_with_no_trash_asks_to_make_one_and_then_trashes() {
    let without = |folders: Vec<Folder>| -> Vec<Folder> {
        folders.into_iter().filter(|f| !(f.account.0 == "work" && f.role == Role::Trash)).collect()
    };
    let mut app = app();
    app.refresh(fx::NOW, 0, Some(&[]), Vec::new(), without(fx::folders()));
    let on = |app: &mut App| screen(app, 120, 40).into_iter().find(|row| row.starts_with('▌')).unwrap();
    let first = on(&mut app);
    press(&mut app, "d");
    let asked = screen(&mut app, 120, 40).pop().unwrap();
    assert_eq!(asked, "  work has no Trash folder. Create one on its server?  y yes · n no");
    press(&mut app, "n");
    assert!(app.asks().is_empty() && on(&mut app) == first, "no is no: nothing is made and nothing moved");
    press(&mut app, "dy");
    let asks = app.asks();
    assert_eq!(asks[0].action, Action::CreateTrash(AccountId("work".into())));
    assert_eq!((&asks[1].action, asks[1].on.len()), (&Action::Trash, 1));
    assert!(on(&mut app) != first, "the row has left the Inbox");
    assert_eq!(foot(&mut app).1, "✓ Trashed 1 message · u to undo");
    press(&mut app, "u");
    assert_eq!(on(&mut app), first, "and u brings it back as from any Trash");
}

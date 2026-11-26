use tuimex_fixtures::design as fx;

use crate::app::App;
use crate::app::tests::{self as keyed, bar, foot, press, screen};
use crate::keymap::Key;
use crate::list::Preview;
use crate::spot::{Mouse, Target};

const SIZE: (usize, usize) = (120, 41);

/// The design's mailbox with the mouse on.
fn app() -> App {
    after("")
}

fn after(keys: &str) -> App {
    let mut app = keyed::after(keys);
    app.set_mouse(true);
    app
}

fn texts(app: &mut App, (width, height): (usize, usize)) -> Vec<String> {
    app.lines(width, height).iter().map(|line| line.iter().map(|span| span.text.as_str()).collect()).collect()
}

/// The first cell of `text` where a screen of `size` shows it.
fn cell(app: &mut App, size: (usize, usize), text: &str) -> (usize, usize) {
    let rows = texts(app, size);
    let found = rows.iter().enumerate().find_map(|(row, line)| Some((line.find(text)?, row, line)));
    let (byte, row, line) = found.unwrap_or_else(|| panic!("{text:?} is not on the screen: {rows:#?}"));
    (line[..byte].chars().count(), row)
}

/// Presses the pointer on `text`, as a screen of `size` shows it.
fn click_at(app: &mut App, size: (usize, usize), text: &str) {
    let (col, row) = cell(app, size, text);
    app.mouse(Mouse::Click { col, row });
}

fn click(app: &mut App, text: &str) {
    click_at(app, SIZE, text);
}

fn wheel(app: &mut App, size: (usize, usize), text: &str, down: bool) {
    let (col, row) = cell(app, size, text);
    app.mouse(Mouse::Wheel { col, row, down });
}

/// The row the cursor is on.
fn cursor(app: &mut App) -> String {
    let rows = screen(app, 120, 40);
    rows.into_iter().find(|row| row.starts_with('▌')).unwrap_or_default()
}

#[test]
fn a_click_on_a_row_moves_the_cursor_there_and_a_second_click_opens_it() {
    let mut app = app();
    click(&mut app, "Lars Lindqvist");
    assert!(cursor(&mut app).contains("Staging database migration window"), "{}", cursor(&mut app));
    assert_eq!(foot(&mut app).0, "6 of 24 · 1 message", "the list is still the screen");
    assert!(app.asks().is_empty(), "moving the cursor asks for nothing");
    click(&mut app, "Staging database migration window");
    assert_eq!(screen(&mut app, 120, 40)[0].trim_end(), screen(&mut after("jjjjj\r"), 120, 40)[0].trim_end());
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Staging database migration window"), "the reader is open");
}

#[test]
fn a_click_anywhere_in_a_row_is_on_that_row_and_past_its_last_cell_is_on_none() {
    let mut app = app();
    for text in ["personal   Greta Holm", "Photos from Saturday", "4 files   20:44"] {
        press(&mut app, "gg");
        click(&mut app, text);
        assert!(cursor(&mut app).contains("Greta Holm"), "{text}");
    }
    press(&mut app, "gg");
    let (_, row) = cell(&mut app, SIZE, "Greta Holm");
    app.mouse(Mouse::Click { col: 119, row });
    assert_eq!(foot(&mut app).0, "9 of 24 · 1 message", "the last cell of the row is the row's");
    app.mouse(Mouse::Click { col: 120, row });
    assert_eq!(foot(&mut app).0, "9 of 24 · 1 message", "past it is nothing, and the row is not opened");
}

#[test]
fn a_click_on_a_date_label_a_blank_line_or_the_rule_does_nothing() {
    let mut app = after("jj");
    let was = texts(&mut app, SIZE);
    click(&mut app, "Yesterday");
    click(&mut app, "Past 7 days");
    click(&mut app, "────");
    let (_, label) = cell(&mut app, SIZE, "Yesterday");
    app.mouse(Mouse::Click { col: 10, row: label - 1 });
    app.mouse(Mouse::Click { col: 500, row: 500 });
    assert_eq!(texts(&mut app, SIZE), was);
    assert!(app.asks().is_empty());
}

#[test]
fn a_click_on_the_hint_line_or_the_status_line_does_nothing() {
    let mut app = app();
    let was = texts(&mut app, SIZE);
    for text in ["e archive", "d trash", "Enter open", "? help", "1 of 24", "1 not sent"] {
        click(&mut app, text);
        click(&mut app, text);
    }
    assert_eq!(texts(&mut app, SIZE), was);
    assert!(app.asks().is_empty(), "nothing was archived, trashed or opened");
}

#[test]
fn a_click_on_a_tab_opens_that_section_and_on_the_open_one_does_nothing() {
    let mut app = after("jj");
    click(&mut app, "Flagged");
    assert_eq!(screen(&mut app, 120, 40), screen(&mut after("3"), 120, 40));
    // The count is the tab's too.
    click(&mut app, "9   Unread");
    assert_eq!(screen(&mut app, 120, 40), screen(&mut after("1"), 120, 40));
    press(&mut app, "jj");
    let was = texts(&mut app, SIZE);
    click(&mut app, "Inbox");
    assert_eq!(texts(&mut app, SIZE), was, "the cursor stays where it was");
    // Between two tabs is neither.
    let (col, row) = cell(&mut app, SIZE, "Unread");
    app.mouse(Mouse::Click { col: col - 2, row });
    assert_eq!(texts(&mut app, SIZE), was);
}

#[test]
fn a_click_on_a_place_goes_there_as_its_function_key_does() {
    let mut app = app();
    click(&mut app, "Calendar");
    assert_eq!(bar(&mut app).1, "F2 Calendar");
    let mut keyed = self::app();
    keyed.key(Key::F(2));
    assert_eq!(texts(&mut app, SIZE), texts(&mut keyed, SIZE));
    click(&mut app, "F1");
    assert_eq!(bar(&mut app).1, "F1 Mail");
    // From the reader too, which F2 leaves as well.
    let mut reading = after("\r");
    click(&mut reading, "F2 Calendar");
    assert_eq!(bar(&mut reading).1, "F2 Calendar");
}

#[test]
fn the_bar_is_not_pressed_where_its_keys_are_not_read() {
    for keys in ["c", "?"] {
        let mut app = after(keys);
        let was = texts(&mut app, SIZE);
        click(&mut app, "Calendar");
        assert_eq!(texts(&mut app, SIZE), was, "{keys}");
    }
}

#[test]
fn the_wheel_moves_the_cursor_over_the_list_and_scrolls_the_preview_over_it() {
    let mut app = app();
    wheel(&mut app, SIZE, "Today", true);
    assert_eq!(foot(&mut app).0, "4 of 24 · thread of 3");
    wheel(&mut app, SIZE, "Mei Tanaka", false);
    wheel(&mut app, SIZE, "Mei Tanaka", false);
    assert_eq!(foot(&mut app).0, "1 of 24 · 1 message", "it stops at the first row");
    // Over the preview its lines scroll and the cursor stays.
    // The first Halden message, unfolded from its thread, is longer than
    // the preview.
    let mut app = after("jjjzo");
    let (_, rule) = cell(&mut app, SIZE, "────");
    app.mouse(Mouse::Wheel { col: 30, row: rule + 4, down: true });
    assert_eq!(foot(&mut app).0, "4 of 26 · 1 message of the thread", "the cursor did not move");
    press(&mut app, "⌥j");
    assert!(foot(&mut app).0.starts_with("lines 4-"), "{}", foot(&mut app).0);
    app.mouse(Mouse::Wheel { col: 30, row: rule + 4, down: false });
    assert!(foot(&mut app).0.starts_with("lines 1-"), "{}", foot(&mut app).0);
}

#[test]
fn a_click_on_the_preview_takes_the_focus_there_and_on_a_row_gives_it_back() {
    let mut app = app();
    app.set_preview(Preview::Right);
    let wide = (170, 31);
    click_at(&mut app, wide, "The pre-read is the roadmap");
    let mut keyed = self::app();
    keyed.set_preview(Preview::Right);
    // The focus goes to a preview that has been drawn.
    texts(&mut keyed, wide);
    press(&mut keyed, "⌥l");
    assert_eq!(texts(&mut app, wide), texts(&mut keyed, wide));
    // The row the cursor is on is not opened by the press that comes back
    // to the list.
    click_at(&mut app, wide, "Imani Castell           Invitation");
    press(&mut keyed, "⌥h");
    assert_eq!(texts(&mut app, wide), texts(&mut keyed, wide));
    click_at(&mut app, wide, "Jonas Becker");
    assert_eq!(texts(&mut app, wide)[5].chars().next(), Some('▌'));
}

#[test]
fn the_pointer_is_not_read_while_a_question_is_open_or_a_line_is_typed() {
    for keys in [":", "/nov", ":messages\r", ":empty-trash work\r", "A"] {
        let mut app = after(keys);
        let was = texts(&mut app, SIZE);
        click(&mut app, "Lars Lindqvist");
        click(&mut app, "Flagged");
        click(&mut app, "Calendar");
        wheel(&mut app, SIZE, "Lars Lindqvist", true);
        assert_eq!(texts(&mut app, SIZE), was, "{keys}");
    }
}

#[test]
fn the_mouse_command_turns_the_pointer_on_and_off_and_alone_to_the_other() {
    let mut app = keyed::app();
    assert!(!app.mouse_on(), "off until it is asked for");
    click(&mut app, "Lars Lindqvist");
    assert_eq!(foot(&mut app).0, "1 of 24 · 1 message", "and a report nobody asked for is not read");
    press(&mut app, ":mouse\r");
    assert_eq!((app.mouse_on(), foot(&mut app).1.as_str()), (true, "Mouse on · hold Shift to select text"));
    press(&mut app, ":mouse\r");
    assert_eq!((app.mouse_on(), foot(&mut app).1.as_str()), (false, "Mouse off"));
    press(&mut app, ":mouse off\r");
    assert!(!app.mouse_on(), "off twice is off");
    press(&mut app, ":mouse on\r:mouse on\r");
    assert!(app.mouse_on());
    press(&mut app, ":mouse maybe\r");
    assert!(app.mouse_on(), "a word it does not take changes nothing");
    assert_eq!(foot(&mut app).1, "E :mouse: it takes on or off, or nothing · Tab completes");
    // In the reader and the calendar too.
    let mut reading = keyed::after("\r:mouse\r");
    assert!(reading.mouse_on() && foot(&mut reading).1.starts_with("Mouse on"));
    app.set_mouse(false);
    assert!(!app.mouse_on());
}

#[test]
fn a_press_ends_a_sequence_that_was_begun() {
    let mut app = after("jjg");
    assert!(app.waiting());
    click(&mut app, "Lars Lindqvist");
    assert!(!app.waiting());
    press(&mut app, "g");
    assert_eq!(foot(&mut app).0, "6 of 24 · 1 message", "the g before the press was not the first of gg");
}

/// A spot is where its thing was drawn: every row the list shows is under
/// one, and a tab's cells spell its name.
#[test]
fn every_spot_lies_on_the_text_it_names_at_any_size() {
    let names: Vec<String> = fx::sections().into_iter().map(|section| section.name).collect();
    for keys in ["", "G", "jjjjjjjjjjjj", "3", "⌥j", "zo"] {
        for (width, height) in (0..=180).step_by(12).flat_map(|w| (0..=45).step_by(3).map(move |h| (w, h))) {
            let mut app = after(keys);
            let rows = texts(&mut app, (width, height));
            let said = |spot: &crate::spot::Spot| -> String {
                rows[spot.row].chars().skip(spot.cols.start).take(spot.cols.len()).collect()
            };
            for spot in &app.spots {
                let at = format!("{keys:?} at {width}x{height}: {spot:?}");
                assert!(spot.row < rows.len() && spot.cols.end <= width && !spot.cols.is_empty(), "{at}");
                match spot.target {
                    // The last thing a narrow row has room for is cut short.
                    _ if spot.cols.end == width && said(spot).ends_with('…') => {}
                    Target::Tab(tab) => {
                        let name = names.get(tab).map_or("Outbox", String::as_str);
                        assert!(said(spot).starts_with(name), "{at}: {:?}", said(spot));
                    }
                    Target::Place(_) => assert!(said(spot).starts_with('F'), "{at}"),
                    Target::Row(_) => assert!(said(spot).chars().nth(4).is_some_and(|c| c != ' '), "{at}"),
                    Target::Pane => {}
                }
            }
            // Every line a row was drawn on is one a press finds a row on.
            let is_row = |line: &str| {
                let mut cells = line.chars();
                let margin: String = cells.by_ref().take(4).collect();
                ["    ", "▌   "].contains(&margin.as_str()) && cells.next().is_some_and(char::is_alphabetic)
            };
            let listed = rows.iter().enumerate().skip(3).take_while(|(_, row)| !row.starts_with('─'));
            for (row, line) in listed.filter(|(row, line)| is_row(line) && row + 3 < height && width >= 40) {
                let on = crate::spot::at(&app.spots, 4, row);
                assert!(matches!(on, Some(Target::Row(_))), "{keys:?} {width}x{height}: {line}");
            }
        }
    }
}

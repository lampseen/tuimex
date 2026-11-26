use tuimex_core::calendar::civil_from_days;

use crate::app::App;
use crate::app::mouse::tests::{SIZE, after, app as fresh, cell, click_at, texts, wheel};
use crate::app::tests::{foot, press, screen};
use crate::calendar::form::FIELDS;
use crate::calendar::parts::clock;
use crate::spot::{Mouse, Spot, Target};

fn click(app: &mut App, text: &str) {
    click_at(app, SIZE, text);
}

/// The screen a press leaves, against the one `keys` leave.
fn same(app: &mut App, keys: &str) {
    assert_eq!(screen(app, 120, 40), screen(&mut after(keys), 120, 40), "as {keys:?} leaves it");
}

/// The cell of the day of the month `dom` in the month's grid, which is
/// left of the bar, on the first row of days that has it.
fn grid_cell(app: &mut App, dom: i64) -> (usize, usize) {
    let number = format!("{dom:>2}");
    let rows = texts(app, SIZE);
    for (row, line) in rows.iter().enumerate().skip(1) {
        let grid: Vec<char> = line.chars().take_while(|c| *c != '│').collect();
        if grid.iter().any(|c| c.is_alphabetic()) {
            continue;
        }
        let at = (1..grid.len().saturating_sub(1)).find(|&i| {
            let here: String = grid[i..i + 2].iter().collect();
            here == number && [' ', '▌'].contains(&grid[i - 1]) && !grid.get(i + 2).is_some_and(char::is_ascii_digit)
        });
        if let Some(col) = at {
            return (col, row);
        }
    }
    panic!("{dom} is not in the grid: {rows:#?}")
}

fn press_day(app: &mut App, dom: i64) {
    let (col, row) = grid_cell(app, dom);
    app.mouse(Mouse::Click { col, row });
}

#[test]
fn a_click_on_a_view_opens_it() {
    let mut app = after("gc");
    click(&mut app, "Week");
    same(&mut app, "gc2");
    click(&mut app, "Week");
    same(&mut app, "gc2");
    click(&mut app, "Day");
    same(&mut app, "gc3");
    click(&mut app, "Month");
    same(&mut app, "gc1");
    assert!(app.asks().is_empty());
}

#[test]
fn a_click_on_a_day_moves_the_cursor_there() {
    let mut app = after("gc");
    press_day(&mut app, 26);
    same(&mut app, "gclll");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events · 1 to answer");
    // On the day the cursor has it is Enter, which opens the day.
    press_day(&mut app, 26);
    same(&mut app, "gclll\r");
    // From the list the press gives the grid the focus back.
    let mut app = after("gc⌥l");
    press_day(&mut app, 25);
    same(&mut app, "gc⌥l⌥hll");
    // The weekdays, the month's name and a blank cell are not days.
    let mut app = after("gc");
    let was = texts(&mut app, SIZE);
    for text in [" Mo  Tu", " November 2026"] {
        let (col, row) = cell(&mut app, SIZE, text);
        app.mouse(Mouse::Click { col: col + 2, row });
    }
    let (col, row) = grid_cell(&mut app, 1);
    app.mouse(Mouse::Click { col: col - 5, row });
    assert_eq!(texts(&mut app, SIZE), was);
}

#[test]
fn a_click_on_an_event_picks_it_and_a_second_opens_it() {
    // In the month's list, which takes the focus from the grid.
    let mut app = after("gc");
    click(&mut app, "Halden contract call");
    same(&mut app, "gc⌥lj");
    click(&mut app, "Halden contract call");
    same(&mut app, "gc⌥lj\r");
    // In the day's list.
    let mut app = after("gc3");
    click(&mut app, "Standup");
    same(&mut app, "gc3j");
    click(&mut app, "Halden contract call");
    same(&mut app, "gc3jj");
    click(&mut app, "Halden contract call");
    same(&mut app, "gc3jj\r");
    // A day's heading and the blank line before it are not events.
    let mut app = after("gc3j");
    let was = texts(&mut app, SIZE);
    let (col, row) = cell(&mut app, SIZE, "Tomorrow");
    app.mouse(Mouse::Click { col, row });
    app.mouse(Mouse::Click { col, row: row - 1 });
    assert_eq!(texts(&mut app, SIZE), was);
    assert!(app.asks().is_empty());
}

#[test]
fn a_click_on_a_week_s_day_moves_there() {
    let mut app = after("gc2");
    click(&mut app, "Thu 26");
    same(&mut app, "gc2lll");
    click(&mut app, "Thu 26");
    same(&mut app, "gc2lll\r");
    // An event's block picks it, and on the one picked opens it; the head
    // of its day is then not the day's to open.
    let mut app = after("gc2");
    click(&mut app, "Q4 roadmap");
    same(&mut app, "gc2llljj");
    click(&mut app, "Thu 26");
    same(&mut app, "gc2llljj");
    click(&mut app, "Fri 27");
    same(&mut app, "gc2llljjl");
    click(&mut app, "Q4 roadmap");
    click(&mut app, "Q4 roadmap");
    same(&mut app, "gc2llljj\r");
    assert!(app.asks().is_empty());
}

#[test]
fn the_wheel_moves_through_the_agenda_and_by_weeks_over_the_month() {
    let mut app = after("gc⌥l");
    wheel(&mut app, SIZE, "Standup", true);
    same(&mut app, "gc⌥lj");
    wheel(&mut app, SIZE, "Standup", false);
    same(&mut app, "gc⌥l");
    // Over the grid, which has not the focus, it is nothing.
    let (col, row) = grid_cell(&mut app, 26);
    app.mouse(Mouse::Wheel { col, row, down: true });
    same(&mut app, "gc⌥l");

    let mut app = after("gc");
    let (col, row) = grid_cell(&mut app, 26);
    app.mouse(Mouse::Wheel { col, row, down: true });
    same(&mut app, "gcj");
    let (col, row) = grid_cell(&mut app, 30);
    app.mouse(Mouse::Wheel { col, row: row + 2, down: false });
    same(&mut app, "gcjk");
    wheel(&mut app, SIZE, "Standup", true);
    same(&mut app, "gc");

    // In the week and the day as j and k, and on an event's screen as j.
    for keys in ["gc2", "gc3", "gclll\rj\r"] {
        let mut app = after(keys);
        app.mouse(Mouse::Wheel { col: 60, row: 20, down: true });
        let notch = if keys.ends_with('\r') { "jjj" } else { "j" };
        same(&mut app, &format!("{keys}{notch}"));
    }
}

#[test]
fn a_click_on_a_form_field_moves_the_focus_there_and_changes_nothing() {
    let mut app = after("gccLunch");
    click(&mut app, "Where");
    same(&mut app, "gccLunch\t\t\t\t\t\t");
    click(&mut app, "Where");
    same(&mut app, "gccLunch\t\t\t\t\t\t");
    click(&mut app, "Calendar personal");
    same(&mut app, "gccLunch\t\t\t\t\t\t\t\t\t\t");
    // Nothing was typed, and with nothing typed Esc closes at once.
    let mut app = after("gcc");
    click(&mut app, "Starts");
    press(&mut app, "\x1b");
    same(&mut app, "gcc\t\t\x1b");
    assert!(app.asks().is_empty());
    // Past the fields is the notes, which a press does not open.
    let mut app = after("gcc");
    let was = texts(&mut app, SIZE);
    click(&mut app, "No notes.");
    assert_eq!(texts(&mut app, SIZE), was);
}

/// A date or a search being typed, the question an invitation asks, and
/// the form's own question each take no press and are not dismissed by one.
#[test]
fn the_pointer_is_not_read_while_a_date_is_typed_or_a_question_asked() {
    for (keys, on) in [("gcgd", "Week"), ("gc/", "Week"), ("gcI\ri", "Account review"), ("gccLunch\x1b", "Where")] {
        let mut app = after(keys);
        let was = texts(&mut app, SIZE);
        click(&mut app, on);
        wheel(&mut app, SIZE, on, true);
        assert_eq!(texts(&mut app, SIZE), was, "{keys:?}");
    }
}

/// The design's calendar with a second link on the Q4 roadmap review.
fn two_links(keys: &str) -> App {
    let mut app = fresh();
    let mut calendar = crate::calendar::tests::design();
    let roadmap =
        calendar.shelves.iter_mut().flat_map(|shelf| &mut shelf.events).find(|e| e.summary == "Q4 roadmap review");
    roadmap.into_iter().for_each(|event| event.links.push("https://notes.example.org/q4-roadmap".to_string()));
    app.set_calendar(calendar);
    press(&mut app, keys);
    app
}

#[test]
fn a_click_on_an_event_s_link_selects_it_and_a_second_is_assessed_as_enter_is() {
    let mut app = two_links("gclll\rjo");
    click(&mut app, "notes.example.org");
    assert_eq!(screen(&mut app, 120, 40), screen(&mut two_links("gclll\rjoj"), 120, 40));
    assert!(app.asks().is_empty(), "choosing a link opens nothing");
    click(&mut app, "notes.example.org");
    let mut keyed = two_links("gclll\rjoj\r");
    assert_eq!(screen(&mut app, 120, 40), screen(&mut keyed, 120, 40));
    assert_eq!(app.asks(), keyed.asks());
    // A link whose text names another host is asked about first.
    let mut app = after("gcI\rIo");
    click(&mut app, "1  bank.example.com.device-check");
    same(&mut app, "gcI\rIo\r");
    assert!(app.asks().is_empty(), "nothing is opened before the question is answered");
}

#[test]
fn a_click_on_a_calendar_selects_it_and_a_second_shows_or_hides_it() {
    let mut app = after("gclllC");
    click(&mut app, "work       its own");
    same(&mut app, "gclllCjj");
    click(&mut app, "work       its own");
    same(&mut app, "gclllCjj ");
    // While the panel is open the view under it takes no press.
    click(&mut app, "Week");
    same(&mut app, "gclllCjj ");
}

/// Each state, and what kind of thing an event is drawn as in it.
const STATES: [(&str, bool); 14] = [
    ("gc", false),
    ("gc⌥l", false),
    ("gclll⌥ljj", false),
    ("gc2", true),
    ("gc2llljj", true),
    ("gc2]", true),
    ("gc3", false),
    ("gc3j", false),
    ("gc/call\r", false),
    ("gclll\rj\r", false),
    ("gclll\rjo", false),
    ("gcC", false),
    ("gcc", false),
    ("gcc\t\t", false),
];

/// A spot is where its thing was drawn: a view's spot spells its name, a
/// day's its number, an event's its title or the edge of its block, a
/// field's its label.
#[test]
fn every_calendar_spot_lies_on_what_it_names_at_any_size() {
    for (keys, blocks) in STATES {
        for (width, height) in (0..=180).step_by(12).flat_map(|w| (0..=45).step_by(3).map(move |h| (w, h))) {
            let mut app = after(keys);
            let rows = texts(&mut app, (width, height));
            let said = |spot: &Spot| -> String {
                rows[spot.row].chars().skip(spot.cols.start).take(spot.cols.len()).collect()
            };
            for spot in &app.spots {
                let at = format!("{keys:?} at {width}x{height}: {spot:?}");
                assert!(spot.row < rows.len() && spot.cols.end <= width && !spot.cols.is_empty(), "{at}");
                let said = said(spot);
                let cut = said.ends_with('…');
                let fine = match spot.target {
                    Target::CalView(view) => cut || view.name().starts_with(said.as_str()),
                    Target::Day(day) => cut || said.contains(&civil_from_days(day).2.to_string()),
                    Target::Occasion(_) if blocks => said.starts_with(['┃', '▌']),
                    Target::Occasion(n) => {
                        let picked = &app.cal.shown[n];
                        let entry = app.cal.model.find(picked, None).expect("an event drawn is in the calendar");
                        // A narrow row keeps the hour and gives up the title.
                        let title: String = entry.event.summary.chars().take(4).collect();
                        let hour = if entry.all_day { "all day".to_string() } else { clock(entry.start) };
                        said.contains('…') || said.contains(&title) || said.contains(&hour)
                    }
                    Target::CalEntry(_) => cut || ["personal", "work", "example"].iter().any(|n| said.contains(n)),
                    Target::FormField(at) => cut || said.contains(FIELDS[at].label()),
                    Target::CalGrid | Target::CalList | Target::Place(_) | Target::Answer(_) => true,
                    // What only mail draws.
                    _ => false,
                };
                assert!(fine, "{at}: {said:?}");
            }
        }
    }
}

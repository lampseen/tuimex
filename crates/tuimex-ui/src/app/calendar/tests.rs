use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tuimex_core::{Action, Event, Timestamp};
use tuimex_fixtures::design as fx;

use crate::app::App;
use crate::app::tests::{after, app as fresh, foot, press, screen};
use crate::calendar::Calendar;
use crate::calendar::tests::design as calendar;

/// The left of the status line after `keys`. Today is Monday 23 November.
fn at(keys: &str) -> String {
    foot(&mut after(keys)).0
}

fn said(keys: &str) -> String {
    foot(&mut after(keys)).1
}

const MONTH: &str = "Enter open · J K next event · t today · c new event · gm mail · ? help";

#[test]
fn gc_opens_the_calendar_on_today_and_gm_returns_with_each_side_where_it_was() {
    let mut app = after("jjjgc");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[0].starts_with("  Month   Week   Day") && rows[0].ends_with("November 2026"), "{}", rows[0]);
    assert_eq!(foot(&mut app), ("Mon 23 Nov · 2 events".to_string(), MONTH.to_string()));
    press(&mut app, "lll");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events · 1 to answer");
    press(&mut app, "gm");
    assert_eq!(foot(&mut app).0, "4 of 24 · thread of 3", "mail's cursor is where it was");
    press(&mut app, "jgc");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events · 1 to answer", "and the calendar's");
    press(&mut app, "gm");
    assert_eq!(foot(&mut app).0, "5 of 24 · 1 message");
    assert!(app.asks().is_empty(), "moving about asks for nothing");
}

#[test]
fn the_calendar_is_opened_from_the_reader_and_the_message_is_still_open_on_the_way_back() {
    let mut app = after("\rgc");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Month"));
    press(&mut app, "gm");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Invitation: Q4 roadmap review"));
}

/// ADR-054: h, l, Tab and the digits are days and views here, and sections
/// only in mail.
#[test]
fn the_calendar_s_keys_are_read_before_the_ones_of_everywhere() {
    let mut app = after("gcl");
    assert_eq!(foot(&mut app).0, "Tue 24 Nov · 2 events", "l is a day");
    for (keys, view) in [("2", "week"), ("\t", "day"), ("\t", ""), ("3", "day"), ("1", "")] {
        press(&mut app, keys);
        let row = screen(&mut app, 120, 40)[2].clone();
        match view {
            "" => assert!(row.contains("November 2026"), "{keys:?} is the month: {row}"),
            "week" => assert!(row.trim_start().starts_with("Mon 23"), "{keys:?} is the week, a column to a day: {row}"),
            _ => assert_eq!(row, "  Tomorrow · Tue 24 November", "{keys:?} is the day, from the cursor's on"),
        }
    }
    app.key(crate::keymap::Key::BackTab);
    assert_eq!(screen(&mut app, 120, 40)[2], "  Tomorrow · Tue 24 November", "S-Tab steps back, round the end");
    press(&mut app, "gm");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Inbox 9"), "and no section was changed");
}

#[test]
fn a_day_a_week_and_a_page_each_move_the_cursor_and_t_brings_it_back() {
    assert_eq!(at("gch"), "Sun 22 Nov · 0 events");
    assert_eq!(at("gcj"), "Mon 30 Nov · 1 event");
    assert_eq!(at("gck"), "Mon 16 Nov · 1 event");
    assert_eq!(at("gc]"), "Wed 23 Dec · 1 event");
    assert_eq!(at("gc[["), "Wed 23 Sep · 0 events");
    assert_eq!(at("gc]]jjlt"), "Mon 23 Nov · 2 events");
    let mut app = after("gc:goto 2027-01-31\r]");
    assert_eq!(foot(&mut app).0, "Sun 28 Feb · 0 events", "a page on from the 31st is the month's last day");
    assert!(screen(&mut app, 120, 40)[0].ends_with("February 2027"));
    // The week's page is seven days and the day's is one.
    assert_eq!(at("gc2]"), "Mon 30 Nov · 1 event");
    assert_eq!(at("gc3["), "Sun 22 Nov · 0 events");
}

#[test]
fn in_the_month_the_next_event_is_the_next_day_that_has_one() {
    assert_eq!(at("gc:goto 2026-11-21\rJ"), "Mon 23 Nov · 2 events");
    assert_eq!(at("gc:goto 2026-11-21\rK"), "Fri 20 Nov · 2 events");
    assert_eq!(at("gc:goto 2026-11-21\rJK"), "Fri 20 Nov · 2 events");
}

#[test]
fn enter_goes_deeper_and_esc_comes_back() {
    let mut app = after("gclll\r");
    assert_eq!(screen(&mut app, 120, 40)[2], "  Thu 26 November");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 1 of 4 · 1 to answer", "the first event of the day is selected");
    press(&mut app, "j");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 2 of 4 · 1 to answer");
    press(&mut app, "\r");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[0], format!("  Q4 roadmap review{}2 of 4 on Thu 26 Nov", " ".repeat(79)));
    assert_eq!(rows[2], "    When     Thu 26 Nov 2026, 10:00 to 11:30 · in 3 days");
    press(&mut app, "J");
    let vendor = format!("  Vendor call{}3 of 4 on Thu 26 Nov", " ".repeat(85));
    assert_eq!(screen(&mut app, 120, 40)[0], vendor, "J steps to the next event without going back");
    press(&mut app, "l");
    assert_eq!(screen(&mut app, 120, 40)[0], vendor, "a key the event's table does not name is not the calendar's");
    press(&mut app, "\x1b");
    assert_eq!(screen(&mut app, 120, 40)[2], "  Thu 26 November");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 3 of 4 · 1 to answer");
    press(&mut app, "\x1b");
    assert!(screen(&mut app, 120, 40)[2].contains("November 2026"), "Esc undoes one Enter");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events · 1 to answer", "the month selects a day and no event");
    press(&mut app, "\x1b\x1b");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Month"), "Esc does not leave the calendar; gm does");
}

#[test]
fn events_are_stepped_through_across_days_in_the_order_they_are_drawn() {
    let mut app = after("gclll\rjjj");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 of 4 · 1 to answer");
    let fri = ["1 of 4", "2 of 4", "3 of 4", "4 of 4"].map(|at| format!("Fri 27 Nov · {at} · 1 to answer"));
    for want in &fri {
        press(&mut app, "j");
        assert_eq!(foot(&mut app).0, *want);
    }
    press(&mut app, "J");
    assert_eq!(foot(&mut app).0, "Sat 28 Nov · 1 of 1", "the cabin weekend again, on its second day");
    press(&mut app, "KK");
    assert_eq!(foot(&mut app).0, fri[2]);
    press(&mut app, "h");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 1 of 4 · 1 to answer", "a day moved to has its first event selected");
}

#[test]
fn gd_says_what_the_words_mean_and_goes_there() {
    let mut app = after("gclllgd");
    assert_eq!(foot(&mut app), (String::new(), "Go to:".to_string()));
    press(&mut app, "5 de");
    assert_eq!(foot(&mut app).0, "", "words that name no day yet say nothing");
    press(&mut app, "c");
    assert_eq!(foot(&mut app), ("goes to Sat 5 Dec 2026".to_string(), "Go to: 5 dec".to_string()));
    press(&mut app, "\r");
    assert_eq!(foot(&mut app), ("Sat 5 Dec · 1 event".to_string(), MONTH.to_string()));
    assert!(screen(&mut app, 120, 40)[0].ends_with("December 2026"), "the view stays; only the date moves");
    assert_eq!(at("gclllgd+2w\r"), "Thu 10 Dec · 2 events", "a step is taken from the cursor");
    assert_eq!(at("gclllgdfri\r"), "Fri 27 Nov · 4 events · 1 to answer", "a weekday from today");
    assert_eq!(at("gcgd5 dec\x1b"), "Mon 23 Nov · 2 events", "Esc goes nowhere");
    assert_eq!(said("gcgdsoon\r"), "E gd: not a date: \"soon\" · 2026-12-05, 5 dec, fri, +2w");
    assert_eq!(said("gc:goto\r"), "E :goto: name a date · 2026-12-05, 5 dec, fri, +2w");
    assert_eq!(at(":goto 5 dec\r"), "Sat 5 Dec · 1 event", "from mail it opens the calendar there");
}

#[test]
fn a_find_is_typed_and_enter_searches_every_day() {
    let mut app = after("gc/Call");
    assert_eq!(foot(&mut app), (String::new(), "/Call".to_string()));
    let rows = screen(&mut app, 120, 40);
    assert!(rows[3].contains("Standup") && rows[4].contains("Halden contract call"), "the month still lists the day");
    press(&mut app, "\r");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[0].starts_with("  Month   Week   Day   Search Call"), "{}", rows[0]);
    assert_eq!(foot(&mut app).0, "3 events match \"Call\"");
    // The list begins at the selected match's day, so the selection is
    // the row under the first label.
    let picked = |app: &mut App| screen(app, 120, 40)[2..4].join("|");
    let shows = |day: &str, title: &str, rows: String| assert!(rows.contains(day) && rows.contains(title), "{rows}");
    shows("Mon 23 November", "Halden contract call", picked(&mut app));
    press(&mut app, "n");
    shows("Thu 26 November", "Vendor call", picked(&mut app));
    press(&mut app, "n");
    shows("Mon 7 December", "On call", picked(&mut app));
    press(&mut app, "NN");
    shows("Mon 23 November", "Halden contract call", picked(&mut app));
    press(&mut app, "N");
    shows("Mon 23 November", "Halden contract call", picked(&mut app));
    press(&mut app, "\x1b");
    assert_eq!(foot(&mut app).0, "Mon 23 Nov · 2 events", "Esc is back to the view, where it was");
    assert!(screen(&mut app, 120, 40)[0].ends_with("November 2026"));
    assert_eq!(said("gcn"), "E n: no filter · / to find");
    assert_eq!(said("gcN"), "E N: no filter · / to find");
    assert_eq!(at("gcl/\r"), "Tue 24 Nov · 2 events", "nothing typed is nothing searched");
    assert_eq!(at("gcl/call\x1b"), "Tue 24 Nov · 2 events");
    let mut again = after("gcl/call\rn/");
    assert!(!screen(&mut again, 120, 40)[0].contains("Search"), "one thing is looked for at a time");
    press(&mut again, "\x1b");
    assert_eq!(foot(&mut again).0, "Tue 24 Nov · 2 events", "and the view is where the search began");
    assert_eq!(at("gcl/call\rn/gear\r\x1b"), "Tue 24 Nov · 2 events");
}

#[test]
fn capital_i_goes_round_the_invitations_that_wait_for_an_answer() {
    let mut app = after("gcI");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events · 1 to answer");
    press(&mut app, "I");
    assert_eq!(foot(&mut app).0, "Fri 27 Nov · 4 events · 1 to answer");
    press(&mut app, "I");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events · 1 to answer", "after the last, the first");
    press(&mut app, "\r");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 2 of 4 · 1 to answer", "Enter is on the invitation it found");
    press(&mut app, "I");
    assert_eq!(foot(&mut app).0, "Fri 27 Nov · 3 of 4 · 1 to answer");
    let mut empty = fresh();
    empty.set_calendar(Calendar::empty(fx::NOW));
    press(&mut empty, "gcI");
    assert_eq!(foot(&mut empty).1, "E I: no invitation waits for an answer");
}

#[test]
fn gt_shows_one_account_s_events_and_leaves_mail_as_it_was() {
    let mut app = after("gcgt");
    let only = "gt for the next, :account for all";
    assert_eq!(
        foot(&mut app),
        ("personal only · Mon 23 Nov · 0 events".to_string(), format!("Showing personal only · {only}"))
    );
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[3].split('│').nth(1).map(str::trim), Some("Nothing on this day."));
    assert!(
        rows[6].contains("11:00-11:45  Dentist") && !rows[6].contains("personal"),
        "no account column: {}",
        rows[6]
    );
    press(&mut app, "gt");
    assert_eq!(
        foot(&mut app),
        ("work only · Mon 23 Nov · 2 events".to_string(), format!("Showing work only · {only}"))
    );
    press(&mut app, "gt");
    assert_eq!(foot(&mut app), ("Mon 23 Nov · 2 events".to_string(), "Showing every account".to_string()));
    press(&mut app, "gT");
    assert_eq!(foot(&mut app).0, "work only · Mon 23 Nov · 2 events");
    press(&mut app, ":account personal\r");
    assert_eq!(foot(&mut app).0, "personal only · Mon 23 Nov · 0 events");
    press(&mut app, "gm");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Inbox 9"), "mail still shows every account");
    press(&mut app, "gc:account\r");
    assert_eq!(foot(&mut app).0, "Mon 23 Nov · 2 events");
    assert_eq!(said("gc:account nobody\r"), "E :account: no account \"nobody\" · Tab completes");
}

#[test]
fn an_event_selected_in_an_account_narrowed_away_is_no_longer_selected() {
    let mut app = after("gc\rj");
    assert_eq!(foot(&mut app).0, "Mon 23 Nov · 2 of 2");
    press(&mut app, "gt");
    assert_eq!(foot(&mut app).0, "personal only · Mon 23 Nov · 0 events");
}

#[test]
fn the_calendars_panel_hides_a_calendar_from_every_view() {
    let mut app = after("gclllC");
    let rows = screen(&mut app, 120, 40);
    let panel: Vec<&str> = rows[32..37].iter().map(|row| row.trim()).collect();
    let want = [
        "Calendars",
        "▌   personal   its own   5 events    read 2m ago",
        "personal   local     2 events    only in tuimex",
        "work       its own   7 events    read 20s ago",
        "work       local     1 event     only in tuimex",
    ];
    assert_eq!(panel, want);
    assert_eq!(foot(&mut app), (String::new(), "j k choose · Space show or hide · Esc close".to_string()));
    assert!(rows[9].contains("Library books due"));
    press(&mut app, "jjjjkk ");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[34].trim(), "▌   personal   local     2 events    only in tuimex   hidden");
    assert!(rows[9].contains("Cabin weekend"), "Friday's list begins with what is left: {}", rows[9]);
    app.set_calendar(calendar());
    press(&mut app, "\x1b");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[9].contains("Cabin weekend"), "what is hidden stays hidden when the calendars are read again");
    assert_eq!(foot(&mut app).1, MONTH);
    press(&mut app, "Cj ");
    assert!(screen(&mut app, 120, 40)[9].contains("Library books due"), "Space again shows it");
}

#[test]
fn gn_turns_reminders_off_and_on_and_off_is_what_the_status_line_says() {
    let mut app = after("gcgn");
    assert!(!app.reminders());
    assert_eq!(foot(&mut app).1, "Reminders off · gn turns them on");
    assert!(screen(&mut app, 120, 40)[38].ends_with("sending in 6s · 1 not sent · reminders off"));
    press(&mut app, "gn");
    assert!(app.reminders());
    assert_eq!(foot(&mut app).1, "Reminders on");
    assert!(screen(&mut app, 120, 40)[38].ends_with("sending in 6s · 1 not sent"), "on is not said");
    assert_eq!(said("gc:notify off\r"), "Reminders off · gn turns them on");
    assert_eq!(said("gc:notify off\r:notify on\r"), "Reminders on");
    assert_eq!(said(":notify\r"), "E :notify: it takes on or off · Tab completes");
    let mut app = fresh();
    app.set_reminders(false);
    press(&mut app, "gc");
    assert!(screen(&mut app, 120, 40)[38].ends_with("reminders off"), "as it was kept from the run before");
}

/// ADR-054 and ADR-018: `o` lists an event's links with the host each
/// really opens, the meeting's first.
#[test]
fn o_lists_the_selected_event_s_links_with_the_host_each_opens() {
    let mut app = after("gclll\rjo");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[35..37], ["  Links", "▌    1  meet.example.org   the meeting"]);
    let hint = "1-9 open · j k Enter choose · A agent · Esc close";
    assert_eq!(foot(&mut app), ("opens https://meet.example.org/r/q4-roadmap".to_string(), hint.to_string()));
    press(&mut app, "5");
    assert_eq!(foot(&mut app).1, "E o: this event has no link 5 · o lists them");
    assert!(app.asks().is_empty(), "nothing is opened");
    press(&mut app, "o\x1b");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 2 of 4 · 1 to answer", "Esc closes the list and no more");

    let mut lying = after("gcI\rI");
    assert_eq!(foot(&mut lying).0, "Fri 27 Nov · 3 of 4 · 1 to answer");
    press(&mut lying, "o");
    let rows = screen(&mut lying, 120, 40);
    let link = "▌    1  bank.example.com.device-check.example.net   bank.example.com   ▲ text says bank.example.com";
    assert_eq!(rows[36], link);
    assert_eq!(foot(&mut lying).0, "opens https://bank.example.com.device-check.example.net/meet");

    assert_eq!(said("gco"), "E o: no event is selected", "the month selects a day");
    assert_eq!(said("gclll\rjjjo"), "E o: this event has no link");
}

#[test]
fn i_asks_the_five_answers_of_an_event_somebody_asked_the_reader_to() {
    let question = "Answer?  a accept · t tentative · d decline · p propose a time · x ignore · Esc cancel";
    let mut app = after("gcI\ri");
    assert_eq!(foot(&mut app).1, question);
    press(&mut app, "q");
    assert!(app.asks().is_empty());
    assert_eq!(foot(&mut app).1.split(" · ").next(), Some("Enter open"), "any other key leaves the question");
    assert_eq!(said("gci"), "E i: no event is selected");
    assert_eq!(said("gcl\rji"), "E i: nobody asked for an answer to this event", "the dentist");
}

#[test]
fn a_key_with_nothing_to_act_on_says_so() {
    assert_eq!(said("gcd"), "E d: no event is selected");
    assert_eq!(said("gcE"), "E E: no event is selected");
    assert_eq!(said("gcu"), "E undo: nothing to undo");
    assert_eq!(said("gcA"), "E A: nothing here for the agent");
}

/// ADR-055: `:export` is an ask carrying the path, answered as a saved
/// attachment is.
#[test]
fn export_asks_for_the_local_events_to_be_written_and_says_where_they_went() {
    let mut app = after("gc:export ~/cal.ics\r");
    assert_eq!(foot(&mut app).1, "Writing local events as .ics");
    let asks = app.asks();
    assert_eq!(asks[0].action, Action::Export("~/cal.ics".into()));
    app.event(Event::Saved { path: "~/cal.ics".to_string() });
    assert_eq!(foot(&mut app).1, "✓ Saved ~/cal.ics");
    let reason = ":export: tuimex holds no event of its own".to_string();
    app.event(Event::ActionFailed { serial: asks[0].serial, reason, hint: String::new() });
    assert_eq!(foot(&mut app).1, "E :export: tuimex holds no event of its own");
    let mut app = after(":export\r");
    assert_eq!(app.asks()[0].action, Action::Export("".into()), "from mail too, to wherever downloads go");
}

#[test]
fn the_calendar_has_its_commands_and_refuses_the_ones_that_take_rows_of_mail() {
    let mut app = after(":calendar week\r");
    assert!(screen(&mut app, 120, 40)[2].trim_start().starts_with("Mon 23"), "the week, a column to a day");
    press(&mut app, ":archive\r");
    assert_eq!(foot(&mut app).1, "E :archive: the calendar is open · gm returns to mail");
    assert!(app.asks().is_empty());
    press(&mut app, ":mail\r");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Inbox 9"));
    press(&mut app, ":calendar\r");
    assert!(screen(&mut app, 120, 40)[2].trim_start().starts_with("Mon 23"), "alone, where it was");
    assert_eq!(said(":calendar year\r"), "E :calendar: it takes month, week or day · Tab completes");
    press(&mut app, ":calendar month\r\r\r");
    assert_eq!(screen(&mut app, 120, 40)[0], format!("  Standup{}1 of 2 on Mon 23 Nov", " ".repeat(89)));
    press(&mut app, ":q\r");
    assert_eq!(screen(&mut app, 120, 40)[2], "  Today · Mon 23 November", ":q is back, out of the event");
    press(&mut app, ":q\r");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Inbox 9") && !app.quit, "and then out of the calendar");
    assert!(after("gc:qa\r").quit);
    assert_eq!(said("gc:zzzz\r"), "E not a command: :zzzz");
    let mut app = after("gc:calendar \t");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[33..37].iter().map(|row| row.trim()).collect::<Vec<_>>(), ["Views", "▌   month", "week", "day"]);
}

#[test]
fn help_in_the_calendar_leads_with_the_calendar_s_keys() {
    let mut app = after("gc?");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[2].contains("Calendar (after gc)") && !rows[2].contains("List"), "{}", rows[2]);
    press(&mut app, "\x1b?list");
    assert!(screen(&mut app, 120, 40)[2].contains("List"), "typing filters every table");
    press(&mut app, "\x1b\x1b");
    assert!(screen(&mut app, 120, 40)[0].starts_with("  Month"), "Esc closes help, in the calendar");
}

/// The day changes while tuimex is open.
#[test]
fn today_is_the_clock_s() {
    let mut app = after("gc");
    app.tick(Timestamp(fx::NOW.0 + 86_400));
    press(&mut app, "lt");
    assert_eq!(foot(&mut app).0, "Tue 24 Nov · 2 events");
    let rows = screen(&mut app, 120, 40);
    assert!(rows[2].ends_with("Today · Tue 24 November"), "{}", rows[2]);
}

/// A frame drawn over another, as after a resize, leaves nothing of it,
/// at sizes with room for no month and for no list.
#[test]
fn every_cell_is_written_at_any_size_wherever_the_calendar_is() {
    let states = ["gc", "gclll\rj", "gc2", "gcC", "gclll\rjo", "gcI\ri", "gc/call", "gc/call\r", "gcgd5 dec", "gcc"];
    let changing = ["gclll\rd", "gclll\rjipfri", "gcI\rIo\r", "gcI\rIoA", "gclll\rjo\r", "gclll\rjjd"];
    for keys in states.into_iter().chain(["gc\r\r", "gc?", "gc:\t", "gcgt"]).chain(changing) {
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

/// What the views read is one account's identity: an invitation that came
/// to one account is not answered by the other's address (ADR-057).
#[test]
fn an_invitation_waits_only_for_the_account_it_came_to() {
    let mut moved = calendar();
    let roadmap = moved.shelves[2].events.remove(4);
    assert_eq!(roadmap.summary, "Q4 roadmap review");
    moved.shelves[0].events.push(roadmap);
    let mut app: App = fresh();
    app.set_calendar(moved);
    press(&mut app, "gclll");
    assert_eq!(foot(&mut app).0, "Thu 26 Nov · 4 events", "in personal's calendar, work's address is another guest");
}

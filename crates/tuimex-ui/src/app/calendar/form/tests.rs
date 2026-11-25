use tuimex_core::calendar::Moment;
use tuimex_core::{AccountId, Action, Event};
use tuimex_fixtures::design as fx;

use crate::app::App;
use crate::app::tests::{after, foot, press, screen};
use crate::calendar::form::{Field, Form};
use crate::keymap::Key;

/// The hint on a field that is typed into, and on one that is chosen from.
const HINT: &str = "Tab next field · Enter notes · ^s save · Esc close";
const CHOOSING: &str = "Tab next field · h l choose · Enter notes · ^s save · Esc close";

fn form(app: &App) -> &Form {
    app.cal.form.form.as_ref().expect("the form is open")
}

/// The calendar's own screen is back, with its tabs.
fn closed(app: &mut App) -> bool {
    !app.form_is_open() && screen(app, 120, 40)[0].starts_with("  Month   Week   Day")
}

#[test]
fn c_opens_a_new_event_at_nine_on_the_cursor_s_day_in_the_first_account() {
    let mut app = after("gclllc");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[0], "  New event");
    assert_eq!(rows[2], "▌   Title");
    assert_eq!(rows[3], "    Calendar personal local   work local");
    assert_eq!(rows[4], "    Starts   Thu 26 Nov 2026 09:00");
    assert_eq!(rows[5], "    Ends     Thu 26 Nov 2026 10:00");
    assert_eq!(rows[9], "    Guests");
    assert_eq!(rows[10], "    Reminder 10 minutes before");
    assert_eq!(rows[12], "    No notes.");
    assert_eq!(foot(&mut app), ("not saved".to_string(), HINT.to_string()));
}

#[test]
fn today_it_opens_at_the_next_whole_hour_and_in_the_week_at_the_selected_event_s() {
    // It is 14:20 on Monday the 23rd.
    assert_eq!(screen(&mut after("gcc"), 120, 40)[4], "    Starts   Mon 23 Nov 2026 15:00");
    // On Thursday the third event is the vendor call at eleven.
    assert_eq!(screen(&mut after("gclll2JJJc"), 120, 40)[4], "    Starts   Thu 26 Nov 2026 11:00");
    assert_eq!(screen(&mut after("gclll3JJJc"), 120, 40)[4], "    Starts   Thu 26 Nov 2026 09:00", "only the week");
}

#[test]
fn the_form_opens_in_the_account_the_calendar_is_narrowed_to_and_event_takes_a_title() {
    assert_eq!(form(&after("gclllgtgtc")).calendar, 1, "work local");
    let mut app = after(":event Offsite planning\r");
    assert_eq!(screen(&mut app, 120, 40)[2], "▌   Title    Offsite planning");
    press(&mut app, "\x1b");
    assert!(foot(&mut app).1.starts_with("Keep the changes?"), "what was typed after :event was typed");
}

#[test]
fn a_calendar_tuimex_can_write_is_offered_by_the_account_s_name_and_chosen_first() {
    let mut app = after("");
    app.set_writable_calendars(vec![AccountId("work".to_string())]);
    press(&mut app, "gclllgtgtc");
    assert_eq!(screen(&mut app, 120, 40)[3], "    Calendar personal local   work   work local");
    assert_eq!(form(&app).calendar, 1);
}

#[test]
fn tab_moves_through_the_fields_and_left_and_right_choose() {
    let mut app = after("gclllc\t");
    assert_eq!(form(&app).focus, Field::Calendar);
    app.key(Key::Right);
    assert_eq!(screen(&mut app, 120, 40)[3], "▌   Calendar personal local   work local");
    assert_eq!(form(&app).calendar, 1);
    app.key(Key::Left);
    app.key(Key::Left);
    assert_eq!(form(&app).calendar, 1, "round the end");
    press(&mut app, "\t\t\t");
    app.key(Key::Right);
    assert_eq!(screen(&mut app, 120, 40)[6], "▌   All day  yes");
    app.key(Key::Tab);
    app.key(Key::Right);
    assert_eq!(screen(&mut app, 120, 40)[7], "▌   Repeats  daily");
    app.key(Key::BackTab);
    assert_eq!(form(&app).focus, Field::AllDay);
    app.key(Key::BackTab);
    app.key(Key::BackTab);
    app.key(Key::BackTab);
    app.key(Key::BackTab);
    app.key(Key::BackTab);
    assert_eq!(form(&app).focus, Field::Reminder, "S-Tab from the first is the last");
    app.key(Key::Left);
    assert_eq!(screen(&mut app, 120, 40)[10], "▌   Reminder 5 minutes before");
    press(&mut app, "\t");
    app.key(Key::Right);
    assert_eq!(foot(&mut app).1, "E Right: Title has nothing to choose from");
}

/// `h` and `l` are Left and Right wherever a field is chosen from, and the
/// hint names them there. In a field that is typed into they are letters.
#[test]
fn h_and_l_choose_on_a_field_that_is_chosen_from_and_are_letters_where_text_is_typed() {
    let mut app = after("gclllchl\t");
    assert_eq!(screen(&mut app, 120, 40)[2], "    Title    hl", "typed into the title");
    assert_eq!(foot(&mut app).1, CHOOSING, "on Calendar the hint names them");
    press(&mut app, "l");
    assert_eq!(form(&app).calendar, 1);
    assert_eq!(screen(&mut app, 120, 40)[3], "▌   Calendar personal local   work local");
    press(&mut app, "hh");
    assert_eq!(form(&app).calendar, 1, "round the end, as Left goes");
    // Starts is typed into: the letters are words of a date that names none.
    press(&mut app, "\thl");
    assert_eq!(form(&app).starts_words.as_deref(), Some("hl"));
    assert_eq!(foot(&mut app).1, HINT, "and the hint names no key that would type");
    press(&mut app, "^u\t\tl");
    assert_eq!(screen(&mut app, 120, 40)[6], "▌   All day  yes");
    press(&mut app, "\tlll");
    assert_eq!(screen(&mut app, 120, 40)[7], "▌   Repeats  every weekday");
    press(&mut app, "h");
    assert_eq!(screen(&mut app, 120, 40)[7], "▌   Repeats  weekly");
    press(&mut app, "\thl\t\th");
    assert_eq!(form(&app).place, "hl", "Where is typed into");
    assert_eq!(screen(&mut app, 120, 40)[10], "▌   Reminder 5 minutes before");
    // At a question a key of the form's is no answer, on either kind of
    // field: it leaves the question and nothing else.
    press(&mut app, "\x1b");
    assert!(foot(&mut app).1.starts_with("Keep the changes?"));
    press(&mut app, "l");
    assert_eq!(foot(&mut app).1, CHOOSING);
    assert_eq!(screen(&mut app, 120, 40)[10], "▌   Reminder 5 minutes before", "and chose nothing");
}

#[test]
fn a_field_is_edited_with_the_command_line_s_keys() {
    let mut app = after("gclllcOffsite planning^w");
    assert_eq!(form(&app).title, "Offsite ");
    press(&mut app, "x\x7f\x7f");
    assert_eq!(form(&app).title, "Offsite");
    press(&mut app, "^u");
    assert_eq!(form(&app).title, "");
    press(&mut app, "\t\t\t\t\t\tRoom 4");
    assert_eq!(screen(&mut app, 120, 40)[8], "▌   Where    Room 4");
}

#[test]
fn the_status_line_says_what_a_date_was_taken_to_mean_and_words_that_name_none_are_refused() {
    let mut app = after("gclllc\t\t");
    assert_eq!(foot(&mut app).0, "starts Thu 26 Nov 2026 09:00");
    press(&mut app, "fri 14:00");
    assert_eq!(screen(&mut app, 120, 40)[4], "▌   Starts   fri 14:00");
    assert_eq!(foot(&mut app).0, "starts Fri 27 Nov 2026 14:00");
    press(&mut app, "\t");
    assert_eq!(screen(&mut app, 120, 40)[5], "▌   Ends     Fri 27 Nov 2026 15:00", "the end moved with the start");
    assert_eq!(foot(&mut app).0, "ends Fri 27 Nov 2026 15:00");
    press(&mut app, "teatime^s");
    assert_eq!(
        foot(&mut app),
        (String::new(), "E ^s: Ends: not a date: \"teatime\" · 2026-12-05, 5 dec, fri, +2w".into())
    );
    assert!(app.form_is_open(), "and the form stays");
}

#[test]
fn each_refusal_on_save_says_why() {
    assert_eq!(foot(&mut after("gclllc\t\t\t08:00^s")).1, "E ^s: it ends before it starts");
    let mut app = after("gclllc\t\t\t\t\t\t\timani@example.org, lars^s");
    assert_eq!(foot(&mut app).1, "E ^s: \"lars\" is not an address");
    assert!(app.form_is_open());
}

#[test]
fn with_no_guests_s_saves_at_once() {
    let mut app = after("gclllcDentist^s");
    assert!(closed(&mut app));
    assert_eq!(foot(&mut app).1, "✓ Added Dentist to personal, local · u to undo");
}

#[test]
fn guests_are_asked_about_first_and_only_y_saves() {
    let guests = "gclllc\t\t\t\t\t\t\tImani Castell <imani@example.org>, lars@example.org";
    let mut app = after(&format!("{guests}^s"));
    let asked = "Save, and mail the invitation to 2 guests?  y save and send, held 10s · n back";
    assert_eq!(foot(&mut app), ("not saved".to_string(), asked.to_string()));
    assert_eq!(
        screen(&mut app, 120, 40)[9],
        "▌   Guests   Imani Castell <imani@example.org>",
        "no caret at a question"
    );
    press(&mut app, "n");
    assert_eq!(foot(&mut app).1, HINT);
    press(&mut app, "^s\x1b");
    assert_eq!(foot(&mut app).1, HINT, "Esc leaves the question and not the form");
    press(&mut app, "^sy");
    assert!(closed(&mut app));
}

#[test]
fn esc_closes_at_once_with_nothing_typed_and_otherwise_asks_to_keep_the_changes() {
    assert!(closed(&mut after("gclllc\x1b")));
    let mut app = after("gclllcDentist\x1b");
    assert_eq!(foot(&mut app).1, "Keep the changes?  y save · n discard · Esc back");
    press(&mut app, "\x1b");
    assert_eq!(foot(&mut app).1, HINT);
    press(&mut app, "\x1bn");
    assert!(closed(&mut app));
    assert_eq!(foot(&mut app).1, "✓ Changes discarded");
    let mut app = after("gclllcDentist\x1by");
    assert!(closed(&mut app));
    assert!(foot(&mut app).1.starts_with("✓ Added Dentist"), "y saves");
    // A change of choice is a change too.
    let mut app = after("gclllc\t");
    app.key(Key::Right);
    press(&mut app, "\x1b");
    assert!(foot(&mut app).1.starts_with("Keep the changes?"));
}

#[test]
fn enter_hands_the_notes_to_the_editor_and_takes_back_what_it_left() {
    let mut app = after("gclllc\t\t\t\r");
    let asked: Vec<Action> = app.asks().into_iter().map(|ask| ask.action).collect();
    assert_eq!(asked, [Action::Edit(String::new())]);
    app.event(Event::Edited { text: Ok("Bring the slides.\r\n\r\nAnd the cake.\r\n".into()) });
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[12..15], ["    Bring the slides.", "", "    And the cake."]);
    assert!(form(&app).dirty);
    press(&mut app, "\r");
    let asked: Vec<Action> = app.asks().into_iter().map(|ask| ask.action).collect();
    assert_eq!(asked, [Action::Edit("Bring the slides.\n\nAnd the cake.\n".into())], "the notes as they are");
    app.event(Event::Edited { text: Err("vi ended with exit status: 1".into()) });
    assert_eq!(foot(&mut app).1, "E editor: vi ended with exit status: 1 · the notes are as they were");
    assert_eq!(form(&app).notes, "Bring the slides.\n\nAnd the cake.\n");
}

#[test]
fn e_opens_the_selected_event_and_saving_it_unchanged_changes_nothing() {
    // The second event on Thursday is the roadmap review, which work's
    // own calendar holds.
    let mut app = after("gclll\rjE");
    let rows = screen(&mut app, 120, 40);
    assert_eq!(rows[0], "  Q4 roadmap review");
    assert_eq!(rows[3], "    Calendar personal local   work local   work");
    assert_eq!(rows[4], "    Starts   Thu 26 Nov 2026 10:00");
    assert_eq!(rows[8], "    Where    Room Fjord");
    assert_eq!(rows[9], "    Guests   Robin Vale <robin.vale@example.org>");
    assert_eq!(rows[13], "    Reminder 10 minutes before");
    assert!(rows[15].starts_with("    The pre-read is the roadmap document"), "{}", rows[15]);
    let was = app.picked_entry().map(|entry| entry.event.clone()).expect("still selected");
    let saved = form(&app).saved(&app.cal.model.zone, &app.identities).expect("it saves");
    assert_eq!(saved.event, was);
    press(&mut app, "\x1b");
    assert!(closed(&mut app), "nothing was typed, so nothing is asked");
}

#[test]
fn e_with_nothing_selected_says_so() {
    assert_eq!(foot(&mut after("gcE")).1, "E E: no event is selected");
}

#[test]
fn an_all_day_event_is_saved_as_whole_days() {
    let mut app = after("gclllcCabin\t\t\tsat\t");
    app.key(Key::Right);
    assert_eq!(screen(&mut app, 120, 40)[5], "    Ends     Sat 28 Nov 2026");
    let saved = form(&app).saved(&app.cal.model.zone, &app.identities).expect("it saves");
    assert_eq!((saved.event.start, saved.event.end), (Moment::Day(fx::THU), Some(Moment::Day(fx::THU + 3))));
}

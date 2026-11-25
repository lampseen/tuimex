use std::path::PathBuf;

use tuimex_core::{Action, AgentAsk, Ask, Attached, Draft, Event, Listed, Origin, Sending};
use tuimex_fixtures::design as fx;

use crate::app::App;
use crate::glyphs::Glyphs;
use crate::keymap::Key;
use crate::mailbox::Mailbox;
use crate::paint::Theme;
use crate::palette::{Palette, Tier};

fn app_with(mail: Vec<Listed>, agent: bool) -> App {
    let me = fx::addresses();
    let mailbox =
        Mailbox { now: fx::NOW, sections: fx::sections(), folders: fx::folders(), mail, synced: Some(60), me };
    let mut app = App::new(Theme { palette: Palette::new(Tier::True), glyphs: Glyphs::Unicode }, mailbox);
    app.set_up(fx::identities(), agent);
    app
}

fn app() -> App {
    app_with(fx::inbox(), true)
}

/// Types `keys`: a return is Enter, a tab Tab, an escape Esc, a delete
/// Backspace, `^x` the chord, and `←` and `→` the arrows.
fn press(app: &mut App, keys: &str) {
    let mut chars = keys.chars();
    while let Some(c) = chars.next() {
        app.key(match c {
            '\r' => Key::Enter,
            '\t' => Key::Tab,
            '\x1b' => Key::Esc,
            '\x7f' => Key::Backspace,
            '^' => Key::Ctrl(chars.next().unwrap()),
            '←' => Key::Left,
            '→' => Key::Right,
            other => Key::Char(other),
        });
    }
}

fn after(keys: &str) -> App {
    let mut app = app();
    press(&mut app, keys);
    app
}

fn screen(app: &mut App) -> Vec<String> {
    let lines = crate::app::tests::under(app, 120, 40);
    lines.iter().map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>().trim_end().to_string()).collect()
}

/// The command line.
fn said(app: &mut App) -> String {
    screen(app)[39].trim().to_string()
}

/// The row of the field the cursor is on, or nothing when it is on the
/// body.
fn focused(app: &mut App) -> String {
    screen(app).into_iter().find(|row| row.starts_with('▌')).unwrap_or_default()
}

/// What was handed on, by the kind of each ask.
fn asked(asks: &[Ask]) -> Vec<String> {
    asks.iter().map(|ask| format!("{:?}", ask.action).split(['(', ' ']).next().unwrap().to_string()).collect()
}

fn written(asks: &[Ask]) -> Draft {
    asks.iter()
        .rev()
        .find_map(|ask| if let Action::Written(draft) = &ask.action { Some((**draft).clone()) } else { None })
        .unwrap()
}

/// Answers the agent's ask as whoever runs the command would.
fn answer(app: &mut App, text: Result<&str, &str>) -> AgentAsk {
    let asks = app.asks();
    let ask = asks.iter().find(|ask| matches!(ask.action, Action::Agent(_))).expect("the agent is asked");
    app.event(Event::AgentText { serial: ask.serial, text: text.map(str::to_string).map_err(str::to_string) });
    let Action::Agent(asked) = &ask.action else { unreachable!() };
    asked.clone()
}

#[test]
fn tab_and_shift_tab_walk_the_fields_and_enter_moves_on_until_the_body() {
    let mut app = after("c");
    assert_eq!(focused(&mut app), "▌   To");
    let mut walked = Vec::new();
    for _ in 0..6 {
        press(&mut app, "\t");
        walked.push(focused(&mut app).chars().skip(4).take(7).collect::<String>().trim().to_string());
    }
    assert_eq!(walked, ["Cc", "Bcc", "Subject", "", "From", "To"], "the body is the stop with no cursor");
    app.key(Key::BackTab);
    assert!(focused(&mut app).starts_with("▌   From     robin@example.com   robin.vale@example.org"));
    press(&mut app, "\r\r\r\r\r");
    assert_eq!(focused(&mut app), "", "Enter moves on, field by field, to the body");
    assert!(app.asks().is_empty());
    press(&mut app, "\r");
    assert_eq!(asked(&app.asks()), ["Edit"], "and there it opens the editor");
    assert_eq!(said(&mut app), "Tab next field · Enter edit · A agent · ^s send · Esc close");
    app.key(Key::Tab);
    let choosing = "Tab next field · h l identity · Enter edit · A agent · ^s send · Esc close";
    assert_eq!(said(&mut app), choosing, "the keys that choose are named on From, where they are not letters");
}

/// "Send from several accounts": the identity is chosen on From, and the
/// draft is filed and sent under that identity's account.
#[test]
fn left_and_right_choose_the_identity_and_the_account_follows_it() {
    let chosen = |keys: &str| {
        let mut app = after(&format!("cada@example.com\t\t\t\t\t{keys}^d"));
        let draft = written(&app.asks());
        (draft.id.account.0, draft.from.email)
    };
    assert_eq!(chosen(""), ("work".into(), "robin.vale@example.org".into()), "the account of the row under the cursor");
    assert_eq!(chosen("←"), ("personal".into(), "robin@example.com".into()));
    assert_eq!(chosen("→"), ("work".into(), "desk@example.org".into()));
    assert_eq!(chosen("→→"), ("personal".into(), "robin@example.com".into()), "and round");
    // Off From the arrows choose nothing.
    assert_eq!(chosen("\t←"), ("work".into(), "robin.vale@example.org".into()));
    // `h` and `l` are the arrows on From, which takes no typed text.
    assert_eq!(chosen("h"), ("personal".into(), "robin@example.com".into()));
    assert_eq!(chosen("l"), ("work".into(), "desk@example.org".into()));
    assert_eq!(chosen("ll"), ("personal".into(), "robin@example.com".into()));
    // In a field that is typed into they are letters, and choose nothing.
    let mut app = after("chl@example.com\t\thl@example.net\t\t\t^d");
    let draft = written(&app.asks());
    assert_eq!((draft.to[0].email.as_str(), draft.from.email.as_str()), ("hl@example.com", "robin.vale@example.org"));
    assert_eq!(draft.bcc[0].email, "hl@example.net");

    // A draft saved under one account and then moved to another leaves the
    // first: the old copy is discarded, by its own id.
    let mut app = after("cada@example.com^d\t\t\t\t\t←^d");
    let asks = app.asks();
    assert_eq!(asked(&asks), ["Written", "SaveDraft", "Discard", "Written", "SaveDraft"]);
    assert_eq!((asks[1].on[0].account.0.as_str(), asks[2].on[0].account.0.as_str()), ("work", "work"));
    assert_eq!(asks[4].on[0].account.0, "personal");
    let drafts: Vec<&Listed> = app.mailbox.mail.iter().filter(|m| m.envelope.flags.draft).collect();
    assert_eq!(drafts.iter().map(|m| m.envelope.id.account.0.as_str()).collect::<Vec<_>>(), ["personal"]);
}

#[test]
fn what_is_typed_goes_into_the_field_and_a_colon_there_is_a_colon() {
    let mut app = after("cAda <ada@example.com>, \tx\x7f\t\tRe: q? u: ok");
    let rows = screen(&mut app);
    assert_eq!(rows[0], "  Re: q? u: ok");
    assert_eq!(rows[3..6], ["    To       Ada <ada@example.com>", "    Cc", "    Bcc"]);
    assert_eq!(rows[6], "▌   Subject  Re: q? u: ok", "letters that are keys elsewhere are text in a field");
    press(&mut app, " two words^w");
    assert_eq!(focused(&mut app), "▌   Subject  Re: q? u: ok two");
    press(&mut app, "^u");
    assert_eq!(focused(&mut app), "▌   Subject");
    press(&mut app, "\t:");
    assert_eq!(said(&mut app), ":", "on the body a colon is the command line");
}

/// ADR-015: checked before the summary, and named.
#[test]
fn an_address_that_does_not_parse_stops_the_summary_and_is_named() {
    for (to, cc, error) in [
        ("ada@example.com, grace@", "", "E :send: \"grace@\" is not an address"),
        ("ada@example.com", "Bob <bob at example.org>", "E :send: \"Bob <bob at example.org>\" is not an address"),
        ("", "", "E :send: there is nobody to send to · fill To"),
        (" , ", "", "E :send: there is nobody to send to · fill To"),
    ] {
        for send in ["^s", "\t\t\t:send\r"] {
            let mut app = after(&format!("c{to}\t{cc}{send}"));
            assert_eq!(said(&mut app), error, "{to:?} {cc:?} {send:?}");
            assert!(screen(&mut app).iter().any(|row| row.starts_with("    Bcc")), "the fields are still open");
            assert!(app.asks().is_empty(), "nothing is handed on");
        }
    }
    assert_eq!(said(&mut after("cgrace@^d")), "E save: \"grace@\" is not an address");
    assert_eq!(said(&mut after(":send\r")), "E :send: no draft is open · c writes one");
}

#[test]
fn only_y_sends_and_the_draft_is_handed_on_before_the_send() {
    let typed = "r\t\t\tbilling@halden.example.com\t\t\t";
    let summary = |key: &str| {
        let mut app = after(&format!("jjj{typed}:send\r"));
        let rows = screen(&mut app);
        assert_eq!(rows[39], "  Send to 2 recipients?  y send, held 10s · n back");
        assert_eq!(rows[3], "    To       Priya Raman <priya@example.net>");
        assert_eq!(rows[4], "    Cc       billing@halden.example.com                    ▲ not in this thread");
        assert_eq!(rows[6], "    Signing  not signed · not encrypted");
        press(&mut app, key);
        app
    };
    for other in ["n", "\r", "\x1b", "Y", " ", "x"] {
        let mut app = summary(other);
        assert!(app.asks().is_empty(), "{other:?} sends nothing");
        assert!(
            screen(&mut app)[5].starts_with("    Bcc"),
            "{other:?} goes back to the fields, and is not typed there"
        );
    }
    let mut app = summary("y");
    let asks = app.asks();
    assert_eq!(asked(&asks), ["Written", "Send"]);
    let draft = written(&asks);
    assert_eq!(asks[1].on, std::slice::from_ref(&draft.id), "the send names the draft just handed over");
    assert_eq!(
        (draft.to[0].email.as_str(), draft.cc[0].email.as_str()),
        ("priya@example.net", "billing@halden.example.com")
    );
    assert_eq!(draft.in_reply_to.as_deref(), Some("<halden-3@design.example.net>"));
    assert_eq!(draft.references.len(), 3);
    assert_eq!(draft.origin, Origin::Local);
    assert!(draft.body.starts_with("On 23 Nov 11:32, Priya Raman wrote:\n> Hi Robin,"));
    assert_eq!(said(&mut app), "Sending to 2 recipients in 10s · u to stop", "back on the list, held");
    let held = app.mailbox.mail.iter().find(|m| m.envelope.id == draft.id).unwrap();
    assert_eq!(held.sending, Some(Sending::Held { seconds: 10 }));
    app.event(Event::Sent { id: draft.id, answered: Vec::new() });
    assert_eq!(said(&mut app), "✓ Sent to 2 recipients");
}

#[test]
fn ctrl_d_saves_the_draft_into_drafts_and_hands_it_on() {
    let mut app = after("cada@example.com\t\t\tLunch?\t^d");
    assert_eq!(said(&mut app), "✓ Draft saved");
    assert!(screen(&mut app)[38].starts_with("  draft saved 14:20"));
    let asks = app.asks();
    assert_eq!(asked(&asks), ["Written", "SaveDraft"]);
    let draft = written(&asks);
    assert_eq!(
        (draft.subject.as_str(), draft.to[0].email.as_str(), draft.in_reply_to),
        ("Lunch?", "ada@example.com", None)
    );
    assert_eq!(asks[1].on, std::slice::from_ref(&draft.id));
    // It is a row of Drafts at once, and opens again from there.
    press(&mut app, "\x1b4");
    let rows = screen(&mut app);
    assert!(rows[3].contains("to ada@example.com") && rows[3].contains("Lunch?"), "{}", rows[3]);
    for again in ["r", "R", "f", ":reply\r"] {
        press(&mut app, again);
        assert_eq!(
            focused(&mut app),
            "▌   From     robin@example.com   robin.vale@example.org   desk@example.org",
            "{again}"
        );
        assert_eq!(screen(&mut app)[0..4][3], "    To       ada@example.com", "{again} goes on writing it, as it was");
        press(&mut app, "\x1b");
    }
    press(&mut app, "r");
    assert_eq!(screen(&mut app)[0], "  Lunch?");
    // Saved again, it is the same draft and not a second one.
    press(&mut app, "\t\t\t\t!^d");
    assert_eq!(written(&app.asks()).id, draft.id);
    assert_eq!(app.mailbox.mail.iter().filter(|m| m.envelope.flags.draft).count(), 1);
}

#[test]
fn esc_asks_keep_or_discard_only_when_something_changed() {
    let mut untouched = after("r\x1b");
    assert!(screen(&mut untouched)[0].starts_with("  Inbox"), "a reply nobody touched just closes");
    assert!(untouched.asks().is_empty());

    let ask = "Keep this draft?  y keep · n discard · Esc back";
    let mut back = after("cada@example.com\x1b");
    assert_eq!(said(&mut back), ask);
    press(&mut back, "\x1b");
    assert_eq!(focused(&mut back), "▌   To       ada@example.com", "Esc again, or any other key, is back in the draft");
    press(&mut back, "\x1bx");
    assert_eq!(focused(&mut back), "▌   To       ada@example.com", "and the key is not typed");

    let mut kept = after("cada@example.com\x1by");
    assert_eq!(asked(&kept.asks()), ["Written", "SaveDraft"]);
    assert_eq!(said(&mut kept), "✓ Draft saved");
    assert!(screen(&mut kept)[0].starts_with("  Inbox"));

    let mut dropped = after("cada@example.com\x1bn");
    assert!(dropped.asks().is_empty(), "a draft never saved is nobody's to delete");
    assert_eq!(said(&mut dropped), "✓ Draft discarded");
    let mut saved = after("cada@example.com^dx\x1bn");
    let asks = saved.asks();
    assert_eq!(asked(&asks), ["Written", "SaveDraft", "Discard"]);
    assert_eq!(asks[2].on, asks[1].on);
    assert!(!saved.mailbox.mail.iter().any(|m| m.envelope.flags.draft), "the row leaves Drafts");

    let mut wrong = after("cada@\x1by");
    assert_eq!(said(&mut wrong), "E save: \"ada@\" is not an address");
    assert_eq!(focused(&mut wrong), "▌   To       ada@", "a draft that cannot be kept stays open");
}

#[test]
fn enter_on_the_body_asks_for_the_editor_and_what_it_left_is_the_body() {
    let mut app = after("jjjr\r");
    let asks = app.asks();
    let [Ask { action: Action::Edit(body), .. }] = &asks[..] else { panic!("{asks:?}") };
    assert!(
        body.starts_with("On 23 Nov 11:32, Priya Raman wrote:\n> Hi Robin,"),
        "the editor is given the quoted message"
    );
    app.event(Event::Edited { text: Ok("Thanks, both fine.\r\n\r\nRobin\n".into()) });
    assert_eq!(screen(&mut app)[8..11], ["    Thanks, both fine.", "", "    Robin"]);
    press(&mut app, "\x1b");
    assert_eq!(said(&mut app), "Keep this draft?  y keep · n discard · Esc back", "an edited body is a change");
    press(&mut app, "x");
    app.event(Event::Edited { text: Err("vi ended with exit status: 1".into()) });
    assert_eq!(said(&mut app), "E editor: vi ended with exit status: 1 · the body is as it was");
    assert_eq!(screen(&mut app)[8], "    Thanks, both fine.");
}

#[test]
fn ctrl_a_attaches_a_file_with_the_path_completed() {
    let mut app = after("c^a~/do");
    assert_eq!(said(&mut app), "attach ~/do");
    press(&mut app, "\t");
    let asks = app.asks();
    assert_eq!(asks[0].action, Action::Paths("~/do".into()));
    app.event(Event::Paths { typed: "~/do".into(), found: vec!["~/docs/".into()] });
    assert_eq!(said(&mut app), "attach ~/docs/", "one choice is taken");
    press(&mut app, "re\t");
    app.event(Event::Paths { typed: "stale".into(), found: vec!["~/elsewhere".into()] });
    assert_eq!(said(&mut app), "attach ~/docs/re", "an answer to something no longer typed is dropped");
    let found = ["~/docs/receipt.png", "~/docs/report one.pdf", "~/docs/reports/"].map(String::from).to_vec();
    app.event(Event::Paths { typed: "~/docs/re".into(), found });
    let rows = screen(&mut app);
    assert_eq!(rows[33..37], ["  Files", "▌   ~/docs/receipt.png", "    ~/docs/report one.pdf", "    ~/docs/reports/"]);
    press(&mut app, "\t");
    assert_eq!(said(&mut app), "attach ~/docs/report one.pdf", "Tab moves through the choices");
    app.key(Key::BackTab);
    app.key(Key::BackTab);
    assert_eq!(said(&mut app), "attach ~/docs/reports/");
    press(&mut app, "\t\t\r");
    assert_eq!(app.asks().last().unwrap().action, Action::Attach("~/docs/report one.pdf".into()));
    let file = Attached {
        name: "report one.pdf".into(),
        size: 48 * 1024,
        path: Some(PathBuf::from("/home/robin/docs/report one.pdf")),
    };
    app.event(Event::Attached { file: Ok(file.clone()) });
    assert_eq!(said(&mut app), "✓ Attached report one.pdf · 48 KB");
    assert_eq!(screen(&mut app)[7], "    Files    report one.pdf                48 KB");
    press(&mut app, "ada@example.com^d");
    assert_eq!(written(&app.asks()).files, [file], "the path is handed on with the draft");
    app.event(Event::Attached { file: Err("~/docs: not a file".into()) });
    assert_eq!(said(&mut app), "E attach: ~/docs: not a file");
    press(&mut app, "^ax\x1b");
    assert_eq!(focused(&mut app), "▌   To       ada@example.com", "Esc leaves the prompt and not the draft");
}

/// The reply to the Halden thread, with the cursor on the body.
fn replying(typed: &str) -> App {
    after(&format!("jjjr{typed}"))
}

#[test]
fn agent_reply_sends_the_thread_redacted_and_restores_what_returns() {
    let mut app = replying(":agent reply tell Priya 45 days is fine\r");
    let rows = screen(&mut app);
    assert_eq!(rows[8], "    ◇ Writing a reply · from the 3 messages of the thread");
    assert!(rows[38].ends_with("agent saw placeholders for 3 people, 4 addresses"), "{}", rows[38]);
    assert_eq!(rows[39], "  Esc cancel");
    press(&mut app, "x\t\r:^s^d");
    assert_eq!(screen(&mut app)[39], "  Esc cancel", "no other key does anything while it works");

    let returned = "Hi [person-1a],\n\n45 days is fine. [person-3] ([address-3]) agrees, says [person-2].\n\n[person-9] [address-1";
    let AgentAsk::Reply { thread, guidance } = answer(&mut app, Ok(returned)) else { panic!("a reply was asked for") };
    assert_eq!(guidance, "tell [person-1a] 45 days is fine", "the guidance is redacted with the thread");
    let head = "The reply will be sent by [person-2] <[address-1]>.\n\nFrom: [person-1] <[address-2]>\n\
                To: [person-2] <[address-1]>\nCc: [person-3] <[address-3]>\n";
    assert!(thread.starts_with(head), "{thread}");
    for left in ["Priya", "Raman", "Dmitri", "Volkov", "Robin", "Vale", "@example"] {
        assert!(!thread.contains(left) && !guidance.contains(left), "{left} left the machine");
    }
    assert!(
        thread.contains("Hi [person-2a],") && thread.contains("Halden asked for 45 days instead of 30"),
        "{thread}"
    );

    let rows = screen(&mut app);
    assert_eq!(rows[8], "    ◇ Written by an agent · 23 Nov 14:20");
    assert_eq!(rows[10], "    Hi Priya,");
    assert_eq!(rows[12], "    45 days is fine. Dmitri Volkov (dmitri@example.org) agrees, says Robin Vale.");
    assert_eq!(rows[14], "    [person-9] [address-1", "what the agent invented is left as text");
    assert_eq!(rows[39], "  ✓ Reply written · u to undo");
    assert!(rows[38].ends_with("agent saw placeholders for 3 people, 4 addresses"));
    assert_eq!(
        rows[3..6],
        ["    To       Priya Raman <priya@example.net>", "    Cc", "    Bcc"],
        "recipients are only ever the user's"
    );
}

#[test]
fn agent_tidy_marks_what_it_changed_and_u_restores_the_user_s_text() {
    let mut app = replying("");
    app.event(Event::Edited { text: Ok(fx::YOURS.into()) });
    press(&mut app, ":agent tidy\r");
    assert_eq!(screen(&mut app)[8], "    ◇ Tidying your text");
    let AgentAsk::Tidy { body } = answer(&mut app, Ok(&fx::AGENT_TIDIED.replace("Priya", "[person-1a]"))) else {
        panic!()
    };
    assert!(
        body.starts_with("Hi [person-1a],\n\n45 days is ok i think") && body.contains("me and [person-2a] are"),
        "{body}"
    );
    let rows = screen(&mut app);
    assert_eq!(rows[8], "    ◇ Tidied by an agent · 23 Nov 14:20");
    let tidied = [
        "    Hi Priya,",
        "",
        "  ◇ 45 days is fine, as long as the first invoice goes out at signing.",
        "",
        "  ◇ Dmitri and I are the right contacts for the support clause.",
        "",
        "    Thanks,",
        "    Robin",
    ];
    assert_eq!(rows[10..18], tidied);
    assert_eq!(rows[39], "  ✓ Tidied 2 paragraphs · u for your own text");
    assert!(rows[38].ends_with("agent saw placeholders for 3 people"));

    press(&mut app, "u");
    let rows = screen(&mut app);
    assert_eq!(rows[39], "  ✓ Undone · your own text is back");
    assert_eq!(
        rows[8..11],
        ["    Hi Priya,", "", "    45 days is ok i think so long as the 1st invoice goes at signing"],
        "no notice, no mark"
    );
    press(&mut app, "u");
    assert_eq!(said(&mut app), "E undo: nothing to undo");
    press(&mut app, "\t^d");
    assert_eq!(written(&app.asks()).origin, Origin::Local, "the user's own text is the user's again");
}

#[test]
fn a_failing_agent_changes_nothing_and_says_what_left() {
    let mut app = replying("");
    app.event(Event::Edited { text: Ok(fx::YOURS.into()) });
    press(&mut app, ":agent tidy\r");
    answer(&mut app, Err("the agent command exited with 1"));
    let rows = screen(&mut app);
    assert_eq!(rows[39], "  E :agent tidy: the agent command exited with 1 · nothing changed");
    assert_eq!(rows[8], "    Hi Priya,");
    assert!(
        rows[38].ends_with("agent saw placeholders for 3 people"),
        "the text did leave, and the line still says so"
    );
    press(&mut app, "u");
    assert_eq!(said(&mut app), "E undo: nothing to undo");
    for (typed, error) in [
        (":agent reply\r", "E :agent reply: this draft answers no message"),
        (":agent tidy\r", "E :agent tidy: there is no text to tidy"),
        (":agent\r", "E :agent: it takes reply, tidy or link"),
        (":agent sing\r", "E :agent: it takes reply, tidy or link"),
    ] {
        let mut app = after(&format!("c\t\t\t\t{typed}"));
        assert_eq!(said(&mut app), error);
        assert!(app.asks().is_empty(), "{typed}: nothing leaves");
    }
    assert_eq!(said(&mut after(":agent tidy\r")), "E :agent tidy: no draft is open · r replies");
}

#[test]
fn esc_cancels_the_agent_and_a_late_answer_changes_nothing() {
    let mut app = replying(":agent reply\r");
    let asks = app.asks();
    let serial = asks.iter().find(|ask| matches!(ask.action, Action::Agent(_))).unwrap().serial;
    press(&mut app, "\x1b");
    assert_eq!(asked(&app.asks()), ["AgentCancel"]);
    assert_eq!(said(&mut app), "Agent stopped · nothing changed");
    let before = screen(&mut app)[8..12].to_vec();
    assert_eq!(before[0], "    On 23 Nov 11:32, Priya Raman wrote:", "the body is as it was");
    app.event(Event::AgentText { serial, text: Ok("too late".into()) });
    app.event(Event::AgentText { serial: serial + 40, text: Ok("not asked for".into()) });
    assert_eq!(screen(&mut app)[8..12], before);
    press(&mut app, "\x1b");
    assert!(screen(&mut app)[0].starts_with("  Inbox"), "and the draft is still untouched");
}

/// ADR-016, ADR-027: marked through to the summary and the saved draft.
#[test]
fn an_agent_s_text_marks_the_draft_through_the_summary_and_the_saved_draft() {
    let mut app = replying(":agent reply\r");
    answer(&mut app, Ok("Fine by me.\u{1b}[2J\u{7}"));
    assert_eq!(screen(&mut app)[10], "    Fine by me.[2J", "a control character the command printed is not kept");
    press(&mut app, "^s");
    let rows = screen(&mut app);
    assert_eq!(rows[6..8], ["", "    ◇ Written by an agent · 23 Nov 14:20"]);
    press(&mut app, "n^d");
    let draft = written(&app.asks());
    assert_eq!((draft.origin, draft.body.as_str()), (Origin::Agent, "Fine by me.[2J"));
    let row = app.mailbox.mail.iter().find(|m| m.envelope.id == draft.id).unwrap();
    assert_eq!(row.envelope.origin, Origin::Agent, "its row in Drafts says agent draft");
    press(&mut app, "\x1b4r");
    assert_eq!(screen(&mut app)[8], "    ◇ Written by an agent · 23 Nov 14:20", "and it opens marked");
}

/// ADR-031: with no `[agent]` table the command does not exist.
#[test]
fn without_an_agent_table_the_command_does_not_exist_and_nothing_leaves() {
    for keys in ["jjjr:agent reply\r", "jjjr:agent tidy\r", ":agent link 1\r", ":agent\r"] {
        let mut app = app_with(fx::inbox(), false);
        press(&mut app, keys);
        assert_eq!(said(&mut app), "E not a command: :agent · no [agent] table in the config", "{keys}");
        assert!(app.asks().is_empty(), "{keys}");
    }
}

#[test]
fn commands_on_the_list_wait_until_the_draft_is_closed() {
    for command in ["archive", "trash", "move Archive", "compose", "reply", "sync", "q", "qa", "empty-trash work"] {
        let mut app = replying(&format!(":{command}\r"));
        let name = command.split(' ').next().unwrap();
        assert_eq!(said(&mut app), format!("E :{name}: a draft is open · Esc closes it"));
        assert!(app.asks().is_empty() && !app.quit, "{command}");
    }
    let mut app = replying("?");
    assert_eq!(screen(&mut app)[0], "  Help", "help opens over the draft");
    press(&mut app, "\x1b:messages\r");
    assert!(screen(&mut app).iter().any(|row| row == "  Messages"));
}

#[test]
fn compose_opens_from_the_keys_and_the_commands_alike() {
    for (keys, subject, to) in [
        ("jjjr", "Re: Contract draft v3 for the Halden account", "Priya Raman <priya@example.net>"),
        ("jjj:reply\r", "Re: Contract draft v3 for the Halden account", "Priya Raman <priya@example.net>"),
        ("jR", "Re: Cabin weekend: who is driving?", "Jonas Becker <jonas@example.com>"),
        ("j:replyall\r", "Re: Cabin weekend: who is driving?", "Jonas Becker <jonas@example.com>"),
        ("jf", "[jonas@example.com: Re: Cabin weekend: who is driving?]", ""),
        ("j:forward\r", "[jonas@example.com: Re: Cabin weekend: who is driving?]", ""),
        ("c", "New message", ""),
        (":compose\r", "New message", ""),
        (":compose ada@example.com\r", "New message", "ada@example.com"),
        (":compose mailto:ada@example.com?subject=Hello\r", "Hello", "ada@example.com"),
    ] {
        let rows = screen(&mut after(keys));
        assert_eq!(rows[0].trim(), subject, "{keys}");
        assert_eq!(rows[3].chars().skip(6).collect::<String>().trim(), to, "{keys}");
    }
    let mut empty = app_with(Vec::new(), true);
    press(&mut empty, "r");
    assert_eq!(said(&mut empty), "E :reply: nothing is selected");
    let mut nobody = app();
    nobody.set_up(Vec::new(), true);
    press(&mut nobody, "c");
    assert_eq!(said(&mut nobody), "E :compose: the config has no account to send from");
    assert_eq!(said(&mut after(":compose tel:5550100\r")), "E :compose: not a mailto link");
}

/// PLAN §6.3, ADR-008: an address in a message is written to only by the
/// user, after reading every field.
#[test]
fn a_mailto_link_and_a_mailto_unsubscribe_open_compose_and_send_nothing() {
    let mut app = after("jjjjjj\r");
    let read = app.asks().pop().unwrap();
    let bank = fx::inbox().into_iter().find(|m| m.envelope.id == read.on[0]).unwrap();
    let link = |n, url: &str, shown: &str| tuimex_core::Link { n, url: url.into(), shown: shown.into() };
    let body = tuimex_core::Body {
        links: vec![link(1, "mailto:help@example.net?subject=Help%20me&bcc=eve@example.org", "Contact us")],
        unsubscribe: Some(tuimex_core::Unsubscribe::Mailto("mailto:leave@example.net?subject=unsubscribe".into())),
        ..fx::body(&bank)
    };
    app.event(Event::BodyReady { id: read.on[0].clone(), body: Box::new(body) });
    for (keys, to, subject) in [
        (":link 1\r", "help@example.net", "Help me"),
        ("o1", "help@example.net", "Help me"),
        (":unsubscribe\r", "leave@example.net", "unsubscribe"),
    ] {
        press(&mut app, keys);
        let rows = screen(&mut app);
        assert_eq!(
            (rows[0].as_str(), rows[3].as_str()),
            (format!("  {subject}").as_str(), format!("▌   To       {to}").as_str())
        );
        assert_eq!(app.compose.as_ref().map(|c| c.identity), Some(0), "from the account the message came to");
        assert_eq!(rows[5], "    Bcc", "a link names no blind copy");
        assert!(app.asks().is_empty(), "{keys}: nothing is sent, opened or asked for");
        press(&mut app, "\x1bn");
        assert_eq!(screen(&mut app)[0], "  Action needed: confirm your new device", "closing it is back in the reader");
    }
}

/// From the reader, a reply is to the message open, quoting the text the
/// reader shows.
#[test]
fn a_reply_from_the_reader_quotes_what_the_reader_shows() {
    let mut app = after("jjjjjj\r");
    let read = app.asks().pop().unwrap();
    let bank = fx::inbox().into_iter().find(|m| m.envelope.id == read.on[0]).unwrap();
    let body = tuimex_core::Body { text: "As the reader shows it [1]".into(), ..fx::body(&bank) };
    app.event(Event::BodyReady { id: read.on[0].clone(), body: Box::new(body) });
    for (keys, subject) in [
        ("r", "Re: Action needed: confirm your new device"),
        ("R", "Re: Action needed: confirm your new device"),
        ("f", "[security@examplebank.example.net: Action needed: confirm your new device]"),
    ] {
        press(&mut app, keys);
        let rows = screen(&mut app);
        assert_eq!(rows[0].trim(), subject);
        assert!(rows.iter().any(|row| row.trim_start_matches(['>', ' ']) == "As the reader shows it [1]"), "{rows:#?}");
        press(&mut app, "\x1b");
        assert_eq!(
            screen(&mut app)[0],
            "  Action needed: confirm your new device",
            "an untouched one closes to the reader"
        );
    }
    // `:agent link` is the reader's, and waits while a draft is open.
    press(&mut app, "r:agent link 1\r");
    assert_eq!(said(&mut app), "E :agent link: a draft is open · Esc closes it");
    press(&mut app, "\x1b:agent link 1\r");
    assert_eq!(asked(&app.asks()), ["AgentLink"]);
}

/// The list's hint for a refused message: Enter, edit and resend.
#[test]
fn a_refused_message_opens_again_from_the_outbox() {
    let mut app = app_with(fx::inbox().into_iter().chain(fx::outbox()).collect(), true);
    press(&mut app, "7j\r");
    let rows = screen(&mut app);
    assert_eq!(
        (rows[0].as_str(), rows[3].as_str()),
        ("  Offsite rooms", "    To       Imani Castell <imani@example.org>")
    );
    assert_eq!(rows[7], "    Files    rooms.xlsx                    31 MB");
    press(&mut app, "\t\t\t\t\t:send\ry");
    let asks = app.asks();
    assert_eq!(asked(&asks), ["Written", "Send"]);
    assert_eq!(asks[1].on[0].key, "out-headcount", "the same message goes again");
}

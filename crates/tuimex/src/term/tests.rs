use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};
use tuimex_core::{Action, Ask};
use tuimex_ui::app::App;
use tuimex_ui::keymap::Key;

use super::{Backend, Input, convert, event_loop};

/// A key held down: what a keyboard repeats, each with when it was sent,
/// and when the loop read each.
pub(crate) struct Typist {
    pub keys: std::sync::mpsc::Receiver<(char, Instant)>,
    pub next: Option<(char, Instant)>,
    pub read: Vec<(Instant, Instant)>,
}

impl Input for Typist {
    fn poll(&mut self, wait: Duration) -> std::io::Result<bool> {
        if self.next.is_none() {
            self.next = self.keys.recv_timeout(wait).ok();
        }
        Ok(self.next.is_some())
    }

    fn read(&mut self) -> std::io::Result<Event> {
        let waiting = self.next.take().map_or_else(|| self.keys.recv(), Ok);
        let (key, sent) = waiting.map_err(|_| std::io::Error::other("no key is coming"))?;
        self.read.push((sent, Instant::now()));
        Ok(Event::Key(KeyEvent::new(code(key), KeyModifiers::NONE)))
    }
}

/// The key a typed character is: a return is Enter.
fn code(key: char) -> KeyCode {
    if key == '\r' { KeyCode::Enter } else { KeyCode::Char(key) }
}

/// A terminal that keeps nothing but when each frame ended.
pub(crate) struct Glass(pub std::rc::Rc<std::cell::RefCell<Vec<Instant>>>);

impl std::io::Write for Glass {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.windows(8).any(|window| window == b"\x1b[?2026l") {
            self.0.borrow_mut().push(Instant::now());
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The loop's side of a calendar nobody sends, with nothing to remind of.
pub(crate) fn no_calendar() -> crate::diary::Diary {
    crate::diary::Diary::new(std::sync::mpsc::channel().1, None)
}

/// A backend that keeps what it was asked, and has nothing to tell.
#[derive(Default)]
struct Asked(Vec<Ask>);

impl Backend for Asked {
    fn ask(&mut self, ask: Ask) {
        self.0.push(ask);
    }

    fn events(&mut self) -> Vec<tuimex_core::Event> {
        Vec::new()
    }
}

/// Runs the loop over the design's mailbox with `bursts` typed: the keys
/// of one are all waiting together, and the next come once the loop has
/// nothing left to read. Returns the frames drawn, what was asked for and
/// the status line at the end.
fn typed(bursts: &[&str]) -> (usize, Vec<Ask>, String) {
    let (send, keys) = std::sync::mpsc::sync_channel(0);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            // Taken by a rendezvous only when the loop waits for a key,
            // which it does once it has drawn what came before.
            bursts.iter().for_each(|burst| send.send(burst.to_string()).unwrap());
        });
        let frames = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let viewport = Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 120, 40));
        let glass = CrosstermBackend::new(Glass(frames.clone()));
        let mut terminal = Terminal::with_options(glass, TerminalOptions { viewport }).unwrap();
        let mut app = App::new(crate::theme::choose(None, None, |_| None), crate::fixture_mailbox());
        let (mut bursts, mut asked) = (Bursts { keys, waiting: Default::default() }, Asked::default());
        event_loop(&mut terminal, &mut bursts, &mut app, &mut asked, &mut no_calendar()).unwrap();
        let lines = app.lines(120, 40);
        let status: String = lines[38].iter().map(|span| span.text.as_str()).collect();
        (frames.borrow().len(), asked.0, status.trim().to_string())
    })
}

/// Keys that arrive a burst at a time.
struct Bursts {
    keys: std::sync::mpsc::Receiver<String>,
    waiting: std::collections::VecDeque<char>,
}

impl Input for Bursts {
    fn poll(&mut self, wait: Duration) -> std::io::Result<bool> {
        if self.waiting.is_empty() && !wait.is_zero() {
            let burst = self.keys.recv().map_err(|_| std::io::Error::other("no key is coming"))?;
            self.waiting.extend(burst.chars());
        }
        Ok(!self.waiting.is_empty())
    }

    fn read(&mut self) -> std::io::Result<Event> {
        if self.waiting.is_empty() {
            self.poll(Duration::MAX)?;
        }
        let key = self.waiting.pop_front().ok_or_else(|| std::io::Error::other("no key is coming"))?;
        Ok(Event::Key(KeyEvent::new(code(key), KeyModifiers::NONE)))
    }
}

/// A held key repeats faster than frames are drawn. Each key had a frame
/// of its own, so the list went on moving, and messages went on being
/// unflagged, after the key was let go.
#[test]
fn keys_that_were_waiting_together_are_all_handled_in_order_before_one_frame() {
    let (frames, asked, status) = typed(&["jjjjjjjjj", ":q\r"]);
    assert_eq!(frames, 2, "the first frame, and one for the nine keys");
    assert!(asked.is_empty() && status.starts_with("10 of 24 "), "every key moved the cursor: {status}");

    let (frames, asked, status) = typed(&["jsjsjsk", ":q\r"]);
    assert_eq!(frames, 2);
    let flagged: Vec<(u64, &Action, &str)> =
        asked.iter().map(|ask| (ask.serial, &ask.action, ask.on[0].key.as_str())).collect();
    let rows: Vec<&str> = flagged.iter().map(|(_, _, key)| *key).collect();
    assert_eq!(flagged.len(), 3, "each `s` is an ask of its own, which `u` takes back one at a time: {flagged:?}");
    assert!(
        flagged.windows(2).all(|pair| pair[0].0 < pair[1].0 && pair[0].2 != pair[1].2),
        "in the order typed: {rows:?}"
    );
    assert!(status.starts_with("3 of 24 "), "{status}");

    let (frames, _, status) = typed(&["j", "j", "j", ":q\r"]);
    assert_eq!(
        (frames, status.starts_with("4 of 24 ")),
        (4, true),
        "a key typed alone has its frame at once: {status}"
    );
}

/// The preview reads the message the cursor rests on. A key held down
/// repeats faster than the rest, so the rows it passes are not read, and
/// the row it stops on is read once.
#[test]
fn the_preview_s_message_is_read_once_the_cursor_rests_and_not_while_a_key_is_held() {
    const HELD: usize = 20;
    let (send, keys) = std::sync::mpsc::channel();
    let asked = std::thread::scope(|scope| {
        scope.spawn(move || {
            let key = |c: char| send.send((c, Instant::now())).unwrap();
            for _ in 0..HELD {
                key('j');
                std::thread::sleep(Duration::from_millis(10));
            }
            // Let go: longer than the rest, then the quit command.
            std::thread::sleep(Duration::from_millis(600));
            ":q\r".chars().for_each(key);
        });
        let viewport = Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 120, 40));
        let glass = CrosstermBackend::new(Glass(Default::default()));
        let mut terminal = Terminal::with_options(glass, TerminalOptions { viewport }).unwrap();
        let mut app = App::new(crate::theme::choose(None, None, |_| None), crate::fixture_mailbox());
        let (mut typist, mut asked) = (Typist { keys, next: None, read: Vec::new() }, Asked::default());
        event_loop(&mut terminal, &mut typist, &mut app, &mut asked, &mut no_calendar()).unwrap();
        asked.0
    });
    let mail = tuimex_fixtures::design::inbox();
    let rested = &tuimex_ui::list::rows(&mail, &[])[HELD].msg.envelope.id;
    let read: Vec<&str> =
        asked.iter().filter(|ask| ask.action == Action::Read).map(|ask| ask.on[0].key.as_str()).collect();
    assert_eq!(read, [rested.key.as_str()], "one message is read, the one the cursor stopped on");
}

#[test]
fn a_plain_or_shifted_letter_is_that_letter() {
    assert_eq!(convert(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)), Some(Key::Char('q')));
    assert_eq!(convert(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)), Some(Key::Char('G')));
}

#[test]
fn a_chord_is_not_its_letter() {
    assert_eq!(convert(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)), Some(Key::Ctrl('d')));
    assert_eq!(convert(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT)), Some(Key::Alt('j')));
    assert_eq!(convert(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT | KeyModifiers::CONTROL)), None);
    assert_eq!(convert(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)), None);
}

#[test]
fn enter_typed_before_raw_mode_arrives_as_a_line_feed_and_is_still_enter() {
    assert_eq!(convert(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)), Some(Key::Enter));
}

#[test]
fn the_keys_the_table_names_by_word_arrive_as_themselves() {
    let plain = |code| convert(KeyEvent::new(code, KeyModifiers::NONE));
    assert_eq!(plain(KeyCode::Enter), Some(Key::Enter));
    assert_eq!(plain(KeyCode::Esc), Some(Key::Esc));
    assert_eq!(plain(KeyCode::Tab), Some(Key::Tab));
    assert_eq!(plain(KeyCode::Backspace), Some(Key::Backspace));
    assert_eq!(plain(KeyCode::Up), Some(Key::Up));
    assert_eq!(convert(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)), Some(Key::BackTab));
    assert_eq!(plain(KeyCode::F(1)), None);
}

/// A terminal that keeps every byte written to it.
struct Tape(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

impl std::io::Write for Tape {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A backend with a clock that stands at one second, and always more to
/// come, so the loop looks at the clock without a key.
struct Clock(i64);

impl Backend for Clock {
    fn ask(&mut self, _: Ask) {}

    fn events(&mut self) -> Vec<tuimex_core::Event> {
        Vec::new()
    }

    fn working(&self) -> bool {
        true
    }

    fn now(&self) -> Option<tuimex_core::Timestamp> {
        Some(tuimex_core::Timestamp(self.0))
    }
}

/// Nobody types until the loop has waited `quiet` times; then `:q`.
struct Patient {
    quiet: usize,
    keys: std::collections::VecDeque<char>,
}

impl Input for Patient {
    fn poll(&mut self, wait: Duration) -> std::io::Result<bool> {
        if !wait.is_zero() && self.quiet > 0 {
            self.quiet -= 1;
            return Ok(false);
        }
        Ok(self.quiet == 0 && !self.keys.is_empty())
    }

    fn read(&mut self) -> std::io::Result<Event> {
        let key = self.keys.pop_front().ok_or_else(|| std::io::Error::other("no key is coming"))?;
        Ok(Event::Key(KeyEvent::new(code(key), KeyModifiers::NONE)))
    }
}

/// ADR-058: a reminder rings the terminal's bell. The bell is one byte
/// on the same stream as the frames, so it is written between two of
/// them and never inside one, once for what fell due.
#[test]
fn the_bell_is_written_once_after_the_frame_that_shows_what_fell_due() {
    use tuimex_core::calendar::{Event as Held, Moment};
    use tuimex_ui::calendar::{Calendar, Shelf};

    const NOON: i64 = 1_795_435_200;
    let state = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("term-{}-bell", std::process::id()));
    let settings = crate::notify::Settings { events: true, lead: 600, via: crate::notify::Via::None };
    let reminders = crate::notify::Reminders::new(settings, None, state);
    let (shown, arriving) = std::sync::mpsc::channel();
    let mut diary = crate::diary::Diary::new(arriving, Some(reminders));
    let event = Held {
        uid: "standup@example.com".into(),
        sequence: 0,
        instead_of: None,
        summary: "Stand-up".into(),
        start: Moment::Utc(NOON + 900),
        end: None,
        location: None,
        description: None,
        organizer: None,
        guests: Vec::new(),
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    };
    let shelf = Shelf {
        account: tuimex_core::AccountId("work".into()),
        local: true,
        name: None,
        read: None,
        failed: None,
        events: vec![event],
    };
    let calendar = Calendar { shelves: vec![shelf], ..Calendar::empty(tuimex_core::Timestamp(NOON)) };
    shown.send(crate::calendar::Handed::Calendar { calendar: Box::new(calendar), carried: 0 }).unwrap();

    let tape = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let viewport = Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 120, 40));
    let mut terminal =
        Terminal::with_options(CrosstermBackend::new(Tape(tape.clone())), TerminalOptions { viewport }).unwrap();
    let mut app = App::new(crate::theme::choose(None, None, |_| None), crate::fixture_mailbox());
    // The reminder fell due a minute ago, for an event nine minutes off.
    let mut clock = Clock(NOON + 360);
    let mut input = Patient { quiet: 4, keys: ":q\r".chars().collect() };
    event_loop(&mut terminal, &mut input, &mut app, &mut clock, &mut diary).unwrap();

    let written = tape.borrow();
    let bells: Vec<usize> = (0..written.len()).filter(|&at| written[at] == 0x07).collect();
    assert_eq!(bells.len(), 1, "one reminder, one bell, however often the clock is looked at after");
    let before = &written[..bells[0]];
    let last = |marker: &[u8]| before.windows(marker.len()).rposition(|window| window == marker);
    let (begun, ended) = (last(b"\x1b[?2026h"), last(b"\x1b[?2026l"));
    assert!(begun.is_some() && begun < ended, "the bell follows a whole frame: {begun:?} {ended:?}");
    let frame = String::from_utf8_lossy(&before[begun.unwrap()..]).to_string();
    assert!(frame.contains("Stand-up"), "and that frame shows what the bell is for");
}

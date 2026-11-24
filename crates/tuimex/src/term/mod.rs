//! Terminal lifecycle and the event loop.
//!
//! `ratatui::try_init` installs a panic hook that leaves raw mode and the
//! alternate screen before the panic message prints, so a crash never
//! strands the user in a broken terminal. `restore` runs on every normal
//! exit path, errors included.

use std::io::Write;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen,
    disable_raw_mode, enable_raw_mode,
};
use crossterm::{execute, queue};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tuimex_core::{Action, Ask, Body, Event as Told, Folder, Listed, MessageId, Timestamp};
use tuimex_ui::app::App;
use tuimex_ui::keymap::{Key, TIMEOUT_MS};
use tuimex_ui::paint::{Theme, paint};
use tuimex_ui::say;

use crate::diary::Diary;
use crate::pictures::Painter;

/// Whatever carries out what the UI asks for. The UI has shown each ask's
/// effect before this hears of it (ADR-015), so all it owes back is what
/// then happened: a refusal, a message sent.
pub trait Backend {
    fn ask(&mut self, ask: Ask);
    /// What has happened since this was last called. It never waits. The
    /// loop takes these and then `refreshes`, again and again until both
    /// are empty, so what happened after rows were read is left for the
    /// call that follows them.
    fn events(&mut self) -> Vec<Told>;
    /// Whether something is still to come that no key will bring, so the
    /// loop has to ask again without one.
    fn working(&self) -> bool {
        false
    }
    /// The bytes of an attached picture of that message, by its place
    /// among the attachments, when there are bytes behind it.
    fn picture(&mut self, _id: &MessageId, _index: usize) -> Option<Vec<u8>> {
        None
    }
    /// What changed in the store since this was last called. It never
    /// waits.
    fn refreshes(&mut self) -> Vec<Refresh> {
        Vec::new()
    }
    /// The time, for a backend that keeps one. Without one the interface
    /// stays at the time its mailbox was made with, which is how
    /// `--fixtures` draws the design's fixed afternoon every time.
    fn now(&self) -> Option<Timestamp> {
        None
    }
    /// What the agent may be shown of a message it is asked about
    /// (ADR-031): the names in its headers, and its body as the reader
    /// shows it (ADR-017).
    fn shown(&mut self, _id: &MessageId) -> Option<(Vec<String>, Body)> {
        None
    }
}

/// What the store now holds of some messages, for `App::refresh`: `rows`
/// are those of `asked` that are still there, and with none asked for
/// every message there is.
#[derive(Debug)]
pub struct Refresh {
    pub now: Timestamp,
    /// The serial of the last `Ask` carried out before the store was read.
    pub after: u64,
    pub asked: Option<Vec<MessageId>>,
    pub rows: Vec<Listed>,
    pub folders: Vec<Folder>,
}

/// How long the loop waits for a key before asking a backend that is
/// working what has happened.
const TICK: Duration = Duration::from_millis(100);
/// How long the cursor stays on a row before its message is read for the
/// preview: longer than a keyboard's repeat, shorter than a glance.
const REST: Duration = Duration::from_millis(150);

/// Where what is typed comes from: the terminal, or a test's own keys.
pub(crate) trait Input {
    /// Whether an event is there to be read, waiting up to `wait` for one.
    fn poll(&mut self, wait: Duration) -> std::io::Result<bool>;
    fn read(&mut self) -> std::io::Result<Event>;
}

struct Tty;

impl Input for Tty {
    fn poll(&mut self, wait: Duration) -> std::io::Result<bool> {
        event::poll(wait)
    }

    fn read(&mut self) -> std::io::Result<Event> {
        event::read()
    }
}

/// A terminal that is written to through `W`.
type Screen<W> = Terminal<CrosstermBackend<W>>;

/// The terminal from before there is a list to draw: it is taken when
/// there is first something to say, and given back when the loop ends or
/// this is dropped without one.
pub struct Opening {
    theme: Theme,
    terminal: Option<ratatui::DefaultTerminal>,
}

impl Opening {
    pub fn new(theme: Theme) -> Opening {
        Opening { theme, terminal: None }
    }

    fn terminal(&mut self) -> Result<&mut ratatui::DefaultTerminal> {
        if self.terminal.is_none() {
            self.terminal = Some(ratatui::try_init()?);
        }
        // Filled just above when it was empty.
        self.terminal.as_mut().context("the terminal was not taken")
    }

    /// Draws the loading screen: mail is being opened, and with `rebuilt`
    /// how far the index's rebuild has come.
    pub fn say(&mut self, rebuilt: Option<(usize, usize)>) -> Result<()> {
        let theme = self.theme;
        self.terminal()?.draw(|f| {
            let area = f.area();
            paint(&say::loading(rebuilt), f.buffer_mut(), area, &theme);
        })?;
        Ok(())
    }

    pub fn run(mut self, app: &mut App, backend: &mut dyn Backend, diary: &mut Diary) -> Result<()> {
        event_loop(self.terminal()?, &mut Tty, app, backend, diary)
    }
}

impl Drop for Opening {
    fn drop(&mut self) {
        if self.terminal.is_some() {
            ratatui::restore();
        }
    }
}

pub(crate) fn event_loop<W: Write>(
    terminal: &mut Screen<W>,
    input: &mut dyn Input,
    app: &mut App,
    backend: &mut dyn Backend,
    diary: &mut Diary,
) -> Result<()> {
    // crossterm starts listening for a change of size when it is first
    // asked for an event. Asked here, before anything is drawn, a resize
    // during the first frame is an event waiting after it and not a signal
    // nobody was listening for.
    input.poll(Duration::ZERO)?;
    diary.begin(app);
    let mut painter = Painter::default();
    // Debug builds only: lets the panic path be exercised from a test
    // without shipping a crash switch in release binaries.
    #[cfg(debug_assertions)]
    if std::env::var_os("TUIMEX_TEST_PANIC").is_some() {
        frame(terminal, app, backend, &mut painter)?;
        panic!("TUIMEX_TEST_PANIC is set");
    }
    let mut changed = true;
    while !app.quit {
        // Drawn after whatever events were waiting, so a resize needs
        // nothing of its own: it is an event like any other, and ratatui
        // reads the size on each draw. A tick on which nothing happened
        // draws nothing.
        if app.take_redraw() {
            repaint(terminal)?;
            changed = true;
        }
        if changed {
            frame(terminal, app, backend, &mut painter)?;
        }
        // The first key of a sequence waits only so long for the next.
        if app.waiting() && !input.poll(Duration::from_millis(TIMEOUT_MS))? {
            app.timeout();
            continue;
        }
        // A backend that is working is asked again without a key: it has
        // something to tell that no key will bring, such as the agent's
        // text.
        // A row the cursor has come to rest on is read for the preview,
        // once no key has come for `REST`: a key held down steps over rows
        // faster than that, and none of them is read.
        let resting = app.restless();
        changed = if resting { input.poll(REST)? } else { !backend.working() || input.poll(TICK)? };
        if resting && !changed {
            app.rest();
            app.asks().into_iter().for_each(|ask| backend.ask(ask));
        }
        // Every event that is waiting is taken, in the order it came,
        // before the one frame that shows them all: a key held down
        // repeats faster than a terminal takes frames, and a frame for each
        // would go on playing them out after the key was let go.
        let mut waiting = changed;
        while waiting {
            if let Event::Key(k) = input.read()?
                && k.kind != KeyEventKind::Release
                && let Some(key) = convert(k)
            {
                app.key(key);
                diary.switched(app);
            }
            for ask in app.asks() {
                // `$EDITOR` has the terminal while it runs, as it was
                // before tuimex drew on it.
                let edits = matches!(ask.action, Action::Edit(_));
                if edits {
                    disable_raw_mode()?;
                    execute!(std::io::stdout(), LeaveAlternateScreen)?;
                }
                backend.ask(ask);
                if edits {
                    enable_raw_mode()?;
                    execute!(std::io::stdout(), EnterAlternateScreen)?;
                    repaint(terminal)?;
                }
            }
            waiting = !app.quit && input.poll(Duration::ZERO)?;
        }
        // What is told, then the rows read after it: the store has the
        // last word on a message. A backend that has more to tell after
        // those rows keeps it for the next turn here, so nothing told is
        // taken ahead of rows that were read before it.
        loop {
            let (told, rows) = (backend.events(), backend.refreshes());
            if told.is_empty() && rows.is_empty() {
                break;
            }
            changed = true;
            told.into_iter().for_each(|told| app.event(told));
            for Refresh { now, after, asked, rows, folders } in rows {
                app.refresh(now, after, asked.as_deref(), rows, folders);
            }
        }
        // The loop wakes for a working backend ten times a second, and a
        // countdown or an age is drawn again only in the second it changes.
        // A calendar is sent whole and by itself, on no key and no ask,
        // so it is looked for on every turn, clock or none.
        changed |= diary.hand(app);
        if let Some(now) = backend.now() {
            changed |= app.tick(now);
            changed |= diary.tick(app, now);
        }
    }
    Ok(())
}

/// Clears the screen and has the next frame drawn whole instead of as a
/// difference from the last. `Terminal::clear` would ask the terminal
/// where its cursor is first, and fail on one that does not answer.
fn repaint<W: Write>(terminal: &mut Screen<W>) -> Result<()> {
    execute!(terminal.backend_mut(), Clear(ClearType::All))?;
    terminal.swap_buffers();
    Ok(())
}

/// One frame, inside a synchronised update (terminal mode 2026): the
/// terminal shows it whole or not yet, never half drawn. A terminal that
/// does not know the mode ignores both ends.
fn frame<W: Write>(
    terminal: &mut Screen<W>,
    app: &mut App,
    backend: &mut dyn Backend,
    painter: &mut Painter,
) -> Result<()> {
    queue!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
    terminal.draw(|f| {
        app.render(f.area(), f.buffer_mut());
        if let Some((id, placed)) = app.pictures() {
            painter.paint(id, placed, |index| backend.picture(id, index), f.buffer_mut());
        }
    })?;
    queue!(terminal.backend_mut(), EndSynchronizedUpdate)?;
    Ok(terminal.backend_mut().flush()?)
}

/// A chord is not the letter it is built on: `^d` and `Alt-j` reach the UI
/// as themselves, and `Alt-q`, which nothing binds, never reaches it as `q`.
/// A terminal that sends Alt as an Esc before the letter is read the same
/// way when the two arrive together; an Esc alone is Esc.
fn convert(k: KeyEvent) -> Option<Key> {
    let plain = (k.modifiers - KeyModifiers::SHIFT).is_empty();
    match k.code {
        KeyCode::Char(c) if plain => Some(Key::Char(c)),
        // Enter typed before the terminal is in raw mode reaches it as a
        // line feed, which is what `^j` sends: what was typed while tuimex
        // started is read as it was meant.
        KeyCode::Char('j') if k.modifiers == KeyModifiers::CONTROL => Some(Key::Enter),
        KeyCode::Char(c) if k.modifiers == KeyModifiers::CONTROL => Some(Key::Ctrl(c)),
        KeyCode::Char(c) if k.modifiers == KeyModifiers::ALT => Some(Key::Alt(c)),
        _ if !plain => None,
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Esc),
        KeyCode::Tab => Some(Key::Tab),
        KeyCode::BackTab => Some(Key::BackTab),
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Up => Some(Key::Up),
        KeyCode::Down => Some(Key::Down),
        KeyCode::Left => Some(Key::Left),
        KeyCode::Right => Some(Key::Right),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod tests;

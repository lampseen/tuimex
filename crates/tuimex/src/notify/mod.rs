//! An event's reminder, while tuimex is open (ADR-058): when one falls
//! due, how it reaches the desktop or the terminal, and the switch that
//! turns reminders off and on across runs.
//!
//! The bell and the status line are the interface's. What is here raises
//! the notification and hands the interface what fell due.

#![expect(dead_code, reason = "joined to the interface and the event loop by its own step")]

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Result, anyhow};
use tuimex_core::calendar::{Event, Zone};
use tuimex_core::reminder::{self, Held, Reminder};
use tuimex_store::files;

use crate::config;

/// The longest title a notification is given, in characters. A title is
/// a line on a pop-up, and what is past this is cut and marked `…`.
pub const LONGEST: usize = 120;

/// The file under the state directory that remembers the switch: `on` or
/// `off` and a newline.
pub const SWITCH_FILE: &str = "reminders";

/// The fixed words that are the desktop notification's other half. The
/// event's text is never put where the service reads markup.
const BODY: &str = "Reminder from tuimex";

/// How a reminder is raised beyond the bell and the status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// The system's own program: `notify-send`, or `osascript` on macOS.
    Desktop,
    /// The terminal's OSC 9, for a terminal that shows it.
    Osc9,
    None,
}

/// `[notify]` as reminders take it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Whether a first run starts with reminders on.
    pub events: bool,
    /// Seconds before its start an event with no alarm of its own is
    /// reminded.
    pub lead: i64,
    pub via: Via,
}

/// What `[notify]` says, or ten minutes by the desktop, on, for what it
/// leaves out. A word it does not take is an error naming the key.
pub fn settings(notify: Option<&config::Notify>) -> Result<Settings> {
    let events = notify.and_then(|n| n.events).unwrap_or(true);
    let lead = match notify.and_then(|n| n.lead.as_deref()) {
        None => 10 * 60,
        Some(text) => {
            let seconds = crate::sync::amount("notify.lead", text, &[("s", 1), ("m", 60), ("h", 3_600)])?;
            i64::try_from(seconds).unwrap_or(i64::MAX).min(reminder::FARTHEST)
        }
    };
    let via = match notify.and_then(|n| n.via.as_deref()) {
        None | Some("desktop") => Via::Desktop,
        Some("osc9") => Via::Osc9,
        Some("none") => Via::None,
        Some(other) => return Err(anyhow!("notify.via = {other:?}: it is one of desktop, osc9, none")),
    };
    Ok(Settings { events, lead, via })
}

/// Whatever shows a reminder's title outside the interface. A test's
/// records it; no test raises a real one.
pub trait Notifier {
    /// Raises a notification for an event titled `title`, which is the
    /// calendar's text, uncleaned. The error is a sentence for the user
    /// that names the program.
    fn notify(&mut self, title: &str) -> Result<(), String>;
}

/// The title as it may be shown: no control character (ESC and BEL, which
/// would end an OSC early, among them), no character that turns the
/// direction of text, each run of space one space, at most `LONGEST`
/// characters.
pub fn clean(title: &str) -> String {
    let bidi =
        |c: char| matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
    let kept: String = title.chars().filter(|&c| !bidi(c)).map(|c| if c.is_control() { ' ' } else { c }).collect();
    let words: Vec<&str> = kept.split_whitespace().collect();
    let joined = if words.is_empty() { "An event".to_string() } else { words.join(" ") };
    if joined.chars().count() <= LONGEST {
        return joined;
    }
    let mut cut: String = joined.chars().take(LONGEST - 1).collect();
    cut.push('…');
    cut
}

/// Which system program a desktop notification is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Linux,
    Mac,
}

impl System {
    pub fn this() -> System {
        if cfg!(target_os = "macos") { System::Mac } else { System::Linux }
    }

    /// The whole argument list, program first, for an already cleaned
    /// title. The title is one argument after `--`, so one beginning with
    /// `-` is not read as an option. `notify-send` is given it as the
    /// summary, which the notification specification makes plain text;
    /// only `BODY` is where markup is read. `osascript` is given a fixed
    /// script that reads it from its arguments, so it is never script.
    pub fn program(self) -> &'static str {
        match self {
            System::Linux => "notify-send",
            System::Mac => "osascript",
        }
    }

    pub fn argv(self, title: &str) -> Vec<String> {
        let fixed: &[&str] = match self {
            System::Linux => &["--app-name=tuimex", "--"],
            System::Mac => &[
                "-e",
                "on run argv",
                "-e",
                "display notification (item 1 of argv) with title \"tuimex\" subtitle \"Reminder\"",
                "-e",
                "end run",
                "--",
            ],
        };
        let mut argv = vec![self.program().to_string()];
        argv.extend(fixed.iter().map(|s| (*s).to_string()));
        argv.push(title.to_string());
        if self == System::Linux {
            argv.push(BODY.to_string());
        }
        argv
    }
}

/// The system's own program. No test makes one that names it: a test
/// must never raise a real notification.
pub struct Desktop {
    system: System,
    /// The program started, which is `system`'s own except in a test.
    program: String,
}

impl Desktop {
    pub fn new() -> Desktop {
        let system = System::this();
        Desktop { system, program: system.program().to_string() }
    }
}

impl Notifier for Desktop {
    fn notify(&mut self, title: &str) -> Result<(), String> {
        let argv = self.system.argv(&clean(title));
        let mut command = Command::new(&self.program);
        command.args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let mut child = command.spawn().map_err(|e| {
            format!("{} could not be started ({e}), so reminders are shown only on the status line", self.program)
        })?;
        // Waited for on a thread of its own, so the event loop is never
        // held up and the ended program is reaped rather than left a
        // zombie. Its exit status says nothing the user can act on.
        let reaped = std::thread::Builder::new().name("notify".into()).spawn(move || {
            let _ = child.wait();
        });
        // With no thread to wait on it, the program is reaped when
        // tuimex exits; the notification itself was still raised.
        if let Err(e) = reaped {
            tracing::warn!(error = %e, "a notification's program is left to be reaped at exit");
        }
        Ok(())
    }
}

/// OSC 9, written to the terminal, which shows the text as it likes.
pub struct Osc9<W: Write> {
    out: W,
}

impl<W: Write> Osc9<W> {
    pub fn new(out: W) -> Osc9<W> {
        Osc9 { out }
    }
}

impl<W: Write> Notifier for Osc9<W> {
    fn notify(&mut self, title: &str) -> Result<(), String> {
        let written = write!(self.out, "\x1b]9;{}\x07", clean(title)).and_then(|()| self.out.flush());
        written.map_err(|e| format!("the terminal did not take the reminder ({e}); it is on the status line"))
    }
}

/// The notifier `via` names; `terminal` is called only for OSC 9.
pub fn notifier(via: Via, terminal: impl FnOnce() -> Box<dyn Write>) -> Option<Box<dyn Notifier>> {
    match via {
        Via::Desktop => Some(Box::new(Desktop::new())),
        Via::Osc9 => Some(Box::new(Osc9::new(terminal()))),
        Via::None => None,
    }
}

/// Whether reminders are on, as the switch was last left, or `default`
/// (the config's `notify.events`) when it never was. A file that cannot be
/// read, or holds anything but `on` or `off`, is no choice at all.
pub fn switched(state: &Path, default: bool) -> bool {
    match files::read_own(&state.join(SWITCH_FILE), 16).as_deref().map(|b| b.trim_ascii()) {
        Ok(b"on") => true,
        Ok(b"off") => false,
        _ => default,
    }
}

/// Remembers the switch for the next run. The config is not written
/// (ADR-043): this file is.
pub fn switch(state: &Path, on: bool) -> io::Result<()> {
    files::private_dir(state)?;
    files::replace(&state.join(SWITCH_FILE), if on { b"on\n" } else { b"off\n" })
}

/// What one look at the clock gives the interface.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// The reminders that fell due since the last look: the interface
    /// rings the bell and puts them on the status line.
    pub due: Vec<Reminder>,
    /// That the notifier could not raise one, said once a run.
    pub failure: Option<String>,
}

/// Reminders, owned by the event loop and given the time on each turn.
pub struct Reminders {
    settings: Settings,
    notifier: Option<Box<dyn Notifier>>,
    state: PathBuf,
    on: bool,
    /// The last instant looked at. Before the first look, none: the first
    /// raises what fell due in the last `reminder::LATE` seconds for an
    /// event yet to begin.
    checked: Option<i64>,
    told: bool,
}

impl Reminders {
    pub fn new(settings: Settings, notifier: Option<Box<dyn Notifier>>, state: PathBuf) -> Reminders {
        let on = switched(&state, settings.events);
        Reminders { settings, notifier, state, on, checked: None, told: false }
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    /// `gn` and `:notify on|off`. The choice holds for this run whatever
    /// happens; the error says it will not outlast it.
    pub fn set(&mut self, on: bool) -> Result<(), String> {
        self.on = on;
        switch(&self.state, on).map_err(|e| format!("reminders are {} until tuimex closes: {e}", word(on)))
    }

    /// Looks at the clock: `now` in seconds since the epoch, and the events
    /// held with the zones they name and the reader's. Notifies of each
    /// reminder that fell due since the last look, and returns them. While
    /// off, the clock still moves on, so turning on raises no backlog.
    pub fn tick(
        &mut self,
        now: i64,
        events: &[Event],
        zones: &BTreeMap<String, Zone>,
        local: &Zone,
        declined: &dyn Fn(&Event) -> bool,
    ) -> Tick {
        let checked = self.checked.unwrap_or(i64::MIN);
        // A clock set back is not looked at again until it passes the
        // last look, so nothing is raised twice.
        self.checked = Some(checked.max(now));
        if !self.on {
            return Tick::default();
        }
        let held = Held { events, zones, local, lead: self.settings.lead };
        let due = reminder::due(&held, declined, checked, now);
        let mut failure = None;
        for raised in &due {
            let Some(notifier) = self.notifier.as_mut() else { break };
            if let Err(why) = notifier.notify(&raised.summary)
                && !self.told
            {
                self.told = true;
                failure = Some(why);
            }
        }
        Tick { due, failure }
    }
}

fn word(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

#[cfg(test)]
mod tests;

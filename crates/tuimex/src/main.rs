//! tuimex: a terminal mail client that treats mail as hostile input.

mod agent;
mod cli;
mod config;
mod desk;
mod doctor;
mod live;
mod message;
mod net;
mod outside;
mod palette;
mod paths;
mod pictures;
mod setup;
mod sync;
mod term;
mod theme;

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use tuimex_core::{AccountId, Action, Ask, Body, Draft, Event, MessageId, Timestamp};
use tuimex_fixtures::design;
use tuimex_store::files;
use tuimex_ui::app::App;
use tuimex_ui::calendar::{Calendar, Shelf};
use tuimex_ui::mailbox::Mailbox;

fn main() -> Result<ExitCode> {
    let args = cli::Args::parse();
    match &args.command {
        // Its answer is the one JSON object on stdout, failures included.
        Some(cli::Run::Agent(command)) => {
            return Ok(agent::run(command, args.config.as_deref(), tuimex_engine::parse));
        }
        Some(cli::Run::Sync { account }) => return sync::run(args.config.as_deref(), account.as_deref()),
        Some(cli::Run::Setup) => {
            let done = setup::command(args.config.as_deref())?;
            return Ok(if done { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
        Some(cli::Run::Doctor { redact }) => return doctor::command(args.config.as_deref(), *redact),
        // Handled below, once the directories are known.
        Some(cli::Run::Palette { .. }) | None => {}
    }
    let dirs = paths::Dirs::from_env()?;
    if let Some(cli::Run::Palette { action: cli::Palette::Import { file } }) = &args.command {
        let imported = palette::import(file, &dirs.config)?;
        println!("wrote {}", imported.path.display());
        if !imported.built_in.is_empty() {
            println!("the theme has no colour for: {}; each keeps the built-in one", imported.built_in.join(", "));
        }
        println!("to draw with it, in config.toml:\n\n[ui]\npalette = \"{}\"", imported.path.display());
        return Ok(ExitCode::SUCCESS);
    }
    init_log(&dirs)?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        fixtures = args.fixtures,
        config = ?args.config,
        glyphs = ?args.glyphs,
        colors = ?args.colors,
        section = ?args.section,
        "start"
    );
    // A variable that is unset, or is not text, advertises nothing.
    let mut theme = theme::choose(args.colors, args.glyphs, |key| std::env::var(key).ok());
    let home = directories::BaseDirs::new().context("no home directory to read ~/ against")?;
    let at = config::path(args.config.as_deref(), &dirs.config);
    if !args.fixtures && !at.exists() {
        // Never a demo in place of the user's mail (PLAN §11). A first
        // launch asks setup's questions and then opens what they set up;
        // with nobody to ask, it says what to run and stops.
        if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
            bail!(
                "there is no config at {}; `tuimex setup` writes one, on a terminal, and `tuimex --fixtures` runs offline",
                at.display()
            );
        }
        if !setup::command(args.config.as_deref())? {
            return Ok(ExitCode::FAILURE);
        }
    }
    let mut config = config(args.config.as_deref(), &dirs)?;
    let ui = config.ui.take();
    let (preview, colours) = (config::preview(ui.as_ref())?, config::colours(&config.accounts)?);
    if let Some(named) = ui.and_then(|ui| ui.palette) {
        theme.palette.custom = Some(palette::load(&palette::resolve(&named, home.home_dir(), &dirs.config))?);
    }
    // `paths.downloads`, or else the system's download directory.
    let downloads = match config.paths.take().and_then(|paths| paths.downloads) {
        Some(named) => palette::resolve(&named, home.home_dir(), &dirs.config),
        None => directories::UserDirs::new()
            .and_then(|user| user.download_dir().map(Path::to_path_buf))
            .unwrap_or_else(|| home.home_dir().join("Downloads")),
    };
    let agent = desk::agent(config.agent.as_ref(), &dirs.cache)?;
    // Whatever names a section that is not there is told on the terminal
    // as it was, and not behind the screen.
    let open = |app: &mut App| match &args.section {
        Some(name) if !app.section(name) => bail!("--section {name}: there is no section of that name"),
        _ => Ok(()),
    };
    if args.fixtures {
        let outside = outside::Outside::new(Box::new(Nowhere), downloads, &dirs.cache);
        let mut app = App::new(theme, fixture_mailbox());
        app.set_up(design::identities(), agent.is_some());
        app.set_calendar(fixture_calendar());
        app.set_preview(preview);
        open(&mut app)?;
        term::Opening::new(theme).run(&mut app, &mut desk::Desk::new(Offline::new(outside), agent, &dirs.cache))?;
        return Ok(ExitCode::SUCCESS);
    }
    tuimex_ui::when::set_zone(zone);
    if let Some((old, new)) = setup::rename::unfinished(&dirs.data)? {
        bail!(setup::rename::interrupted(&old, &new));
    }
    // The screen is taken once the config has been found good and before
    // the store is opened, so that a wait for the index is said on it.
    let mut opening = term::Opening::new(theme);
    let (mailbox, live, delay) = live::start(&config, &dirs, downloads, &mut |rebuilt| opening.say(rebuilt))?;
    let mut app = App::new(theme, mailbox);
    app.set_up(live::identities(&config), agent.is_some());
    app.set_delay(delay);
    app.set_colours(colours);
    app.set_preview(preview);
    let mut desk = desk::Desk::new(live, agent, &dirs.cache);
    let ran = match open(&mut app) {
        Ok(()) => opening.run(&mut app, &mut desk),
        Err(error) => {
            drop(opening);
            Err(error)
        }
    };
    // After the terminal is the user's again, so that a wait is explained
    // on it and not behind the alternate screen.
    desk.into_inner().stop(|accounts| {
        eprintln!("tuimex: waiting for {} to finish sending a message, a minute at most", accounts.join(" and "));
    });
    ran?;
    Ok(ExitCode::SUCCESS)
}

/// Seconds east of UTC on this machine at `at`, as `TZ` or the system
/// says. An instant its zone cannot place is read as UTC.
fn zone(at: i64) -> i32 {
    use chrono::{Offset, TimeZone};
    chrono::Local.timestamp_opt(at, 0).single().map_or(0, |local| local.offset().fix().local_minus_utc())
}

/// The config file. One that is not there is no config; one that is there
/// and cannot be read stops tuimex.
fn config(flag: Option<&Path>, dirs: &paths::Dirs) -> Result<config::Config> {
    let path = config::path(flag, &dirs.config);
    if flag.is_none() && !path.exists() {
        return Ok(config::Config::default());
    }
    config::Config::load(&path)
}

/// The design's mailbox: both inboxes, what is in the outbox, and the two
/// filed messages its search finds.
fn fixture_mailbox() -> Mailbox {
    let mut mail = design::inbox();
    mail.extend(design::outbox());
    mail.extend(design::filed());
    mail.extend(design::drafts());
    Mailbox {
        now: design::NOW,
        sections: design::sections(),
        folders: design::folders(),
        mail,
        synced: Some(design::SYNCED),
        me: design::addresses(),
    }
}

/// The design's calendars, each read as long ago as the design says, on
/// the design's fixed afternoon and a clock that is UTC, as its mail is.
fn fixture_calendar() -> Calendar {
    let shelf = |kept: design::Kept| Shelf {
        account: AccountId(kept.account.to_string()),
        local: kept.local,
        name: None,
        read: kept.read.map(|ago| Timestamp(design::NOW.0 - i64::from(ago))),
        failed: None,
        events: kept.events,
    };
    let shelves = design::calendars().into_iter().map(shelf).collect();
    Calendar { shelves, identities: design::identities(), ..Calendar::empty(design::NOW) }
}

/// What `--fixtures` opens a link or a file with: nothing, and it says
/// what it was handed.
struct Nowhere;

impl outside::Opener for Nowhere {
    fn open(&mut self, target: &str) -> std::io::Result<()> {
        Err(std::io::Error::other(format!("--fixtures opens nothing; it was handed {target}")))
    }
}

/// How long `--fixtures` holds a confirmed message: the default of
/// `send.delay` (ADR-015).
const HELD: Duration = Duration::from_secs(10);

/// `--fixtures` has no server: what the UI asks for is agreed to and goes
/// nowhere, and what it reads is the design's. A message sent is held,
/// built as it would be for a server, and then said to have gone.
struct Offline {
    fake: design::Fake,
    outside: outside::Outside,
    bodies: HashMap<MessageId, Body>,
    events: Vec<Event>,
    /// What compose last handed over of each draft.
    written: Vec<Draft>,
    /// The messages waiting to go, and when each does.
    held: Vec<(MessageId, Instant)>,
}

impl Offline {
    fn new(outside: outside::Outside) -> Self {
        Offline {
            fake: design::Fake::default(),
            outside,
            bodies: HashMap::new(),
            events: Vec::new(),
            written: Vec::new(),
            held: Vec::new(),
        }
    }

    /// The message as a server would be given it, with its files read from
    /// where they are. A file the draft already carried has no bytes here:
    /// nothing is stored in `--fixtures`.
    fn bytes(draft: &Draft) -> Result<Vec<u8>, String> {
        let mut files = Vec::new();
        for file in &draft.files {
            if let Some(path) = &file.path {
                let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
                files.push(message::File { name: file.name.clone(), bytes });
            }
        }
        let now = chrono::Utc::now().timestamp();
        message::build(draft, files, now, &tuimex_store::maildir::new_key(), message::Bcc::Withheld)
    }
}

impl term::Backend for Offline {
    fn ask(&mut self, ask: Ask) {
        match &ask.action {
            Action::Written(draft) => {
                self.written.retain(|d| d.id != draft.id);
                self.written.push((**draft).clone());
            }
            Action::Send => self.held.extend(ask.on.iter().map(|id| (id.clone(), Instant::now() + HELD))),
            Action::Undo | Action::Discard => self.held.retain(|(id, _)| !ask.on.contains(id)),
            _ => {}
        }
        if let Action::AgentLink(_) = ask.action {
            // Advice comes from the command `[agent]` names or from nowhere:
            // the fixtures' own is for drawing the design, and is never
            // shown to a person as an agent's.
            let reason = "no [agent] table in the config".to_string();
            return self.events.push(Event::ActionFailed { serial: ask.serial, reason, hint: String::new() });
        }
        for event in self.fake.ask(ask.clone()) {
            match event {
                Event::BodyReady { id, body } => {
                    self.bodies.insert(id.clone(), (*body).clone());
                    self.events.push(Event::BodyReady { id, body });
                }
                event => self.events.push(event),
            }
        }
        let body = ask.on.first().and_then(|id| self.bodies.get(id));
        self.events.extend(body.and_then(|body| self.outside.carry_out(&ask, body, None)));
    }

    fn events(&mut self) -> Vec<Event> {
        let now = Instant::now();
        let (due, held) = std::mem::take(&mut self.held).into_iter().partition(|(_, at)| *at <= now);
        self.held = held;
        for (id, _) in due {
            let built = self.written.iter().find(|d| d.id == id).map_or(Ok(Vec::new()), Offline::bytes);
            self.events.push(match built {
                Ok(_) => Event::Sent { id, answered: Vec::new() },
                Err(reason) => Event::SendFailed { id, reason },
            });
        }
        std::mem::take(&mut self.events)
    }

    fn working(&self) -> bool {
        !self.held.is_empty()
    }

    fn shown(&mut self, id: &MessageId) -> Option<(Vec<String>, Body)> {
        let mail = design::inbox();
        let e = &mail.iter().find(|m| m.envelope.id == *id)?.envelope;
        let names = e.from.iter().chain(&e.to).chain(&e.cc).filter_map(|a| a.name.clone()).collect();
        Some((names, self.bodies.get(id)?.clone()))
    }
}

/// Logs go to a file and never to stdout or stderr, which belong to the TUI.
fn init_log(dirs: &paths::Dirs) -> Result<()> {
    files::private_dir(&dirs.state).with_context(|| format!("creating {}", dirs.state.display()))?;
    let path = dirs.state.join("tuimex.log");
    let file = files::append(&path).with_context(|| format!("opening {}", path.display()))?;
    tracing_subscriber::fmt().with_writer(Mutex::new(file)).with_ansi(false).init();
    Ok(())
}

//! What compose needs of this machine and cannot do from inside the
//! interface: the body through `$EDITOR`, the files a path could name, a
//! file's size, and the agent command (ADR-031). Everything else the
//! interface asks for is the mail backend's and is passed on.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use tuimex_core::{Action, AgentAsk, Ask, Attached, Body, Event, Links, MessageId, assess, redact};
use tuimex_engine::agent::{self, Agent, AgentError, Cancel};
use tuimex_store::files;
use tuimex_store::maildir::new_key;

use crate::config;
use crate::term::{Backend, Refresh};

#[cfg(test)]
mod tests;

/// How long the agent command has when `[agent]` does not say.
const TIMEOUT: Duration = Duration::from_secs(120);
/// More choices than this are not a menu anyone reads.
const MOST_PATHS: usize = 200;

pub struct Desk<B> {
    inner: B,
    agent: Option<Agent>,
    /// The cache directory, which holds a body while it is being edited
    /// and the empty directory the agent command runs in (ADR-011).
    cache: PathBuf,
    home: Option<PathBuf>,
    told: Vec<Event>,
    /// What each thread running the agent command has to tell when it
    /// ends: nothing, for one that was cancelled.
    done: (Sender<Option<Event>>, Receiver<Option<Event>>),
    /// How many of those threads have not ended.
    jobs: usize,
    /// Stops the command `:agent reply` or `:agent tidy` started.
    running: Option<Cancel>,
}

/// The `[agent]` table as the engine takes it.
pub fn agent(table: Option<&config::AgentCommand>, cache: &Path) -> anyhow::Result<Option<Agent>> {
    let Some(table) = table else { return Ok(None) };
    let command = command(table)?;
    let timeout = match table.timeout.as_deref() {
        None => TIMEOUT,
        Some(text) => {
            let (digits, unit) = text.split_at(text.trim_end_matches(char::is_alphabetic).len());
            let seconds = match unit {
                "s" => 1,
                "m" => 60,
                _ => anyhow::bail!("agent.timeout = {text:?}: the unit is one of s, m"),
            };
            let count: u64 =
                digits.parse().map_err(|_| anyhow::anyhow!("agent.timeout = {text:?} does not start with a number"))?;
            Duration::from_secs(count.saturating_mul(seconds))
        }
    };
    Ok(Some(Agent { command, timeout, dir: cache.join("agent") }))
}

/// The program and arguments the table stands for: the built-in agent's,
/// which the user never writes, or the command they gave.
pub fn command(table: &config::AgentCommand) -> anyhow::Result<Vec<String>> {
    match (&table.preset, &table.command) {
        (Some(_), Some(_)) => anyhow::bail!("[agent] has both `use` and `command`; it takes one"),
        (None, None) => anyhow::bail!("[agent] needs `use = \"claude\"`, or a `command`"),
        (Some(name), None) => {
            agent::preset(name).ok_or_else(|| anyhow::anyhow!("agent.use = {name:?}: the built-in agent is \"claude\""))
        }
        (None, Some(command)) if command.is_empty() => anyhow::bail!("agent.command names no program"),
        (None, Some(command)) => Ok(command.clone()),
    }
}

impl<B> Desk<B> {
    pub fn new(inner: B, agent: Option<Agent>, cache: &Path) -> Desk<B> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Desk {
            inner,
            agent,
            cache: cache.to_path_buf(),
            home,
            told: Vec::new(),
            done: mpsc::channel(),
            jobs: 0,
            running: None,
        }
    }

    /// `~/` as the home directory; any other path as written.
    /// The mail backend, when the loop has ended and it is to be stopped.
    pub fn into_inner(self) -> B {
        self.inner
    }

    fn expand(&self, path: &str) -> PathBuf {
        match (path.strip_prefix("~/"), &self.home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(path),
        }
    }

    /// The paths `typed` could go on to name, written as it was begun: a
    /// directory ends in a slash, and a file whose name starts with a dot
    /// is offered only once the dot is typed.
    fn paths(&self, typed: &str) -> Vec<String> {
        let (dir, start) = typed.rsplit_once('/').map_or(("", typed), |(dir, start)| (dir, start));
        let shown = if typed.contains('/') { format!("{dir}/") } else { String::new() };
        let within = if typed.contains('/') { self.expand(&shown) } else { PathBuf::from(".") };
        // A directory that cannot be listed completes to nothing.
        let Ok(entries) = std::fs::read_dir(within) else { return Vec::new() };
        let mut found: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let offered = name.starts_with(start) && (start.starts_with('.') || !name.starts_with('.'));
                let slash = if entry.path().is_dir() { "/" } else { "" };
                offered.then(|| format!("{shown}{name}{slash}"))
            })
            .collect();
        found.sort();
        found.truncate(MOST_PATHS);
        found
    }

    fn attach(&self, typed: &str) -> Result<Attached, String> {
        let path = self.expand(typed.trim());
        let meta = std::fs::metadata(&path).map_err(|e| format!("{typed}: {e}"))?;
        let name = path.file_name().and_then(|n| n.to_str()).filter(|_| meta.is_file());
        let name = name.ok_or_else(|| format!("{typed}: not a file"))?.to_string();
        let size = u32::try_from(meta.len()).map_err(|_| format!("{typed}: too large to send"))?;
        Ok(Attached { name, size, path: Some(path) })
    }

    /// The body through `editor`, in a file of its own directory under
    /// the cache, which is removed with whatever the editor left beside
    /// it. The caller has given the terminal back.
    fn edit(&self, body: &str, editor: &str) -> Result<String, String> {
        let dir = self.cache.join("compose").join(new_key());
        let path = dir.join("body.txt");
        let through = || -> Result<String, String> {
            files::private_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let mut file = files::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            file.write_all(body.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
            drop(file);
            // No shell: the words after the program are its arguments.
            let mut words = editor.split_whitespace();
            let program = words.next().unwrap_or("vi");
            let status = Command::new(program).args(words).arg(&path).status();
            let status = status.map_err(|e| format!("{program} could not be started: {e}"))?;
            if !status.success() {
                return Err(format!("{program} ended with {status}"));
            }
            std::fs::read_to_string(&path).map_err(|e| format!("reading what {program} left: {e}"))
        };
        let result = through();
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            // The text is back either way; what stays is a private file.
            tracing::warn!(dir = %dir.display(), %error, "an edited body was not removed");
        }
        result
    }

    /// Runs the agent command on a thread of its own, so the interface
    /// stays answerable to Esc while it works (ADR-006). `job` is given the
    /// command and what stops it, and says what the interface is told.
    fn start(
        &mut self,
        function: &'static str,
        serial: u64,
        job: impl FnOnce(&Agent, &Cancel) -> Option<Event> + Send + 'static,
    ) -> Option<Cancel> {
        let agent = self.agent.clone()?;
        if let Err(e) = files::private_dir(&agent.dir) {
            let (reason, hint) = (format!("{}: {e}", agent.dir.display()), String::new());
            self.told.push(Event::ActionFailed { serial, reason, hint });
            return None;
        }
        let (cancel, done) = (Cancel::default(), self.done.0.clone());
        let stop = cancel.clone();
        self.jobs += 1;
        tracing::info!(function, serial, "the agent command is asked");
        std::thread::spawn(move || {
            let told = job(&agent, &stop);
            tracing::info!(function, serial, "the agent command is done");
            // Nobody is listening once tuimex is on its way out.
            let _ = done.send(told);
        });
        Some(cancel)
    }

    /// `:agent reply` and `:agent tidy`.
    fn write(&mut self, serial: u64, asked: AgentAsk) {
        if self.agent.is_none() {
            return self.told.push(Event::AgentText { serial, text: Err("no [agent] table in the config".into()) });
        }
        let function = if matches!(asked, AgentAsk::Tidy { .. }) { "tidy" } else { "reply" };
        self.running = self.start(function, serial, move |agent, cancel| {
            let result = match &asked {
                AgentAsk::Reply { thread, guidance } => agent::reply(agent, thread, guidance, cancel),
                AgentAsk::Tidy { body } => agent::tidy(agent, body, cancel),
            };
            let cancelled = matches!(result, Err(AgentError::Cancelled));
            (!cancelled).then(|| Event::AgentText { serial, text: result.map_err(|e| e.to_string()) })
        });
    }

    /// `:agent link <n>`: the link, what the assessment found (ADR-018)
    /// and the message with `names` and its addresses redacted go to the
    /// agent, and its one line comes back as advice. Nothing here opens
    /// the link, and the advice decides nothing.
    fn advise(&mut self, serial: u64, n: usize, names: &[String], body: &Body) {
        let failed = |reason: &str| Event::ActionFailed { serial, reason: reason.to_string(), hint: String::new() };
        let Some(link) = body.links.iter().find(|link| link.n == n) else {
            return self.told.push(failed("the message has no such link"));
        };
        let assessed = assess(&link.url, &link.shown);
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let redacted = redact(&body.text, &names, Links::Kept);
        let shown = link.shown.clone();
        self.told.push(Event::AgentAsked { serial, replaced: redacted.replaced() });
        // Not stopped by Esc: the question it joins is answered without it.
        self.start("link", serial, move |agent, cancel| {
            let said = agent::advise_link(agent, &assessed.url, &shown, &assessed.warnings, &redacted.text, cancel);
            let advice = said.map_err(|e| e.to_string()).and_then(|text| {
                let line = redacted.restore(text.lines().next().unwrap_or_default());
                let line: String = line.chars().filter(|c| !c.is_control()).collect();
                let (advice, why) = line.split_once(':').ok_or("the agent's answer was not advice")?;
                let advice = advice.trim().to_lowercase();
                let known = ["for opening", "against opening"].contains(&advice.as_str());
                known
                    .then(|| (advice, why.trim().to_string()))
                    .ok_or_else(|| "the agent's answer was not advice".to_string())
            });
            Some(match advice {
                Ok((advice, why)) => Event::AgentAdvice { serial, advice, why },
                Err(reason) => Event::ActionFailed { serial, reason, hint: String::new() },
            })
        });
    }
}

impl<B: Backend> Backend for Desk<B> {
    fn ask(&mut self, ask: Ask) {
        match ask.action {
            Action::Edit(body) => {
                // As mutt chooses: `$VISUAL`, then `$EDITOR`, then vi.
                let set = |key: &&str| std::env::var(key).ok().filter(|v| !v.trim().is_empty());
                let editor = ["VISUAL", "EDITOR"].iter().find_map(set).unwrap_or_else(|| "vi".to_string());
                self.told.push(Event::Edited { text: self.edit(&body, &editor) });
            }
            Action::Paths(typed) => self.told.push(Event::Paths { found: self.paths(&typed), typed }),
            Action::Attach(path) => self.told.push(Event::Attached { file: self.attach(&path) }),
            Action::Agent(asked) => self.write(ask.serial, asked),
            Action::AgentCancel => self.running.take().iter().for_each(Cancel::cancel),
            // Without an `[agent]` table the interface has no `:agent`,
            // and whatever asks regardless is the mail backend's to refuse.
            Action::AgentLink(n) if self.agent.is_some() => {
                match ask.on.first().and_then(|id: &MessageId| self.inner.shown(id)) {
                    Some((names, body)) => self.advise(ask.serial, n, &names, &body),
                    None => {
                        let reason = "the message has not been read".to_string();
                        self.told.push(Event::ActionFailed { serial: ask.serial, reason, hint: String::new() });
                    }
                }
            }
            _ => self.inner.ask(ask),
        }
    }

    fn events(&mut self) -> Vec<Event> {
        let mut told = std::mem::take(&mut self.told);
        for ended in self.done.1.try_iter() {
            self.jobs -= 1;
            told.extend(ended);
        }
        told.extend(self.inner.events());
        told
    }

    fn working(&self) -> bool {
        self.jobs > 0 || self.inner.working()
    }

    fn picture(&mut self, id: &MessageId, index: usize) -> Option<Vec<u8>> {
        self.inner.picture(id, index)
    }

    fn refreshes(&mut self) -> Vec<Refresh> {
        self.inner.refreshes()
    }

    fn shown(&mut self, id: &MessageId) -> Option<(Vec<String>, Body)> {
        self.inner.shown(id)
    }

    fn now(&self) -> Option<tuimex_core::Timestamp> {
        self.inner.now()
    }
}

//! `tuimex doctor` (PLAN §6.7): each account end to end, then what tuimex
//! leans on outside itself, then the modes of its own files. It prints
//! what it found and changes nothing; a line marked WRONG is one that
//! stops something working, and makes the exit status non-zero.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use ratatui_image::picker::{Picker, ProtocolType};
use tuimex_core::Role;
use tuimex_engine::calendar;
use tuimex_engine::oauth::{Endpoints, Tokens};
use tuimex_engine::{Keyring, pin};
use tuimex_store::calendar::{Calendars, Fetched};

use crate::config::{self, Config};
use crate::net::{self, Auth, Net};
use crate::paths::Dirs;
use crate::setup::NO_KEYRING;

/// How many files with the wrong mode are named before the rest are counted.
const NAMED: usize = 8;

pub struct Doctor<'a> {
    pub keyring: &'a dyn Keyring,
    pub net: &'a dyn Net,
    pub dirs: &'a Dirs,
    pub config_path: &'a Path,
    pub endpoints: Endpoints,
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// What the terminal said of its image protocol, or why it was not asked.
    pub pictures: String,
    /// Whether what is printed is fit to post in public (`--redact`).
    pub redact: bool,
}

/// Where `program` is, as a shell would find it: a path as it stands, a
/// bare name on `path`. Only a file that may be executed counts.
pub fn on_path(program: &str, path: &OsStr) -> Option<PathBuf> {
    let runs = |file: &PathBuf| fs::metadata(file).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if program.contains('/') {
        return Some(PathBuf::from(program)).filter(runs);
    }
    std::env::split_paths(path).map(|dir| dir.join(program)).find(runs)
}

struct Report<'a> {
    say: &'a mut dyn FnMut(String),
    wrong: usize,
}

impl Report<'_> {
    fn head(&mut self, text: &str) {
        (self.say)(format!("\n{text}"));
    }

    /// A finding under its mark, with any further lines of it set under
    /// its first.
    fn line(&mut self, mark: &str, text: &str) {
        (self.say)(format!("  {mark:<7}{}", text.replace('\n', "\n         ")));
    }

    fn ok(&mut self, text: &str) {
        self.line("ok", text);
    }

    /// Worth knowing, and nothing is broken by it.
    fn note(&mut self, text: &str) {
        self.line("note", text);
    }

    fn wrong(&mut self, text: &str) {
        self.wrong += 1;
        self.line("WRONG", text);
    }
}

/// The values `--redact` replaces, longest first, each beside what stands
/// for it.
pub struct Known(Vec<(String, String)>);

impl Known {
    /// Accounts are numbered in the config's order, then any in `named`
    /// that the config does not hold. An address also gives up its two
    /// halves, which a server's own words can hold apart.
    pub fn new(accounts: &[config::Account], named: &[String], home: Option<&str>) -> Self {
        fn bare(identity: &str) -> &str {
            identity.rsplit_once('<').map_or(identity, |(_, rest)| rest.trim_end_matches('>'))
        }
        let servers = || accounts.iter().flat_map(|account| [&account.imap, &account.smtp]).flatten();
        let names = accounts.iter().map(|account| account.name.as_str()).chain(named.iter().map(String::as_str));
        let addresses: Vec<&str> = (accounts.iter())
            .flat_map(|a| std::iter::once(a.address.as_str()).chain(a.identities.iter().flatten().map(|i| bare(i))))
            .chain(servers().filter_map(|server| server.user.as_deref()))
            .collect();
        let halves: Vec<(&str, &str)> = addresses.iter().filter_map(|address| address.rsplit_once('@')).collect();
        let kinds: [(&str, Vec<&str>); 5] = [
            ("account", names.collect()),
            ("address", addresses.clone()),
            ("host", servers().map(|server| server.host.as_str()).collect()),
            ("user", halves.iter().map(|(user, _)| *user).collect()),
            ("domain", halves.iter().map(|(_, domain)| *domain).collect()),
        ];
        let mut known: Vec<(String, String)> = Vec::new();
        for (kind, values) in kinds {
            let mut n = 0;
            for value in values {
                if !value.is_empty() && !known.iter().any(|(held, _)| held.eq_ignore_ascii_case(value)) {
                    n += 1;
                    known.push((value.to_string(), format!("{kind}-{n}")));
                }
            }
        }
        let home = home.map(|home| home.trim_end_matches('/')).filter(|home| !home.is_empty());
        known.extend(home.map(|home| (home.to_string(), "~".to_string())));
        known.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
        Known(known)
    }

    /// With the hosts of calendars too, numbered after the servers'.
    fn with_hosts(mut self, hosts: &[String]) -> Self {
        let mut n = self.0.iter().filter(|(_, stands)| stands.starts_with("host-")).count();
        for host in hosts {
            if !self.0.iter().any(|(held, _)| held.eq_ignore_ascii_case(host)) {
                n += 1;
                self.0.push((host.clone(), format!("host-{n}")));
            }
        }
        self.0.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
        self
    }
}

/// The length of a fingerprint as `pin::fingerprint` writes one.
const PRINT: usize = 32 * 3 - 1;

/// `report` with each known value, in any case, replaced by what stands
/// for it, and each certificate fingerprint by its number in order of
/// appearance: a pinned certificate can be one machine's alone. A value is
/// replaced only where it stands whole, with no letter, digit, `-` or `_`
/// against it, so an account named `work` leaves `network` and
/// `work-old` alone; and the text is read once, so nothing put in is
/// itself replaced.
pub fn redact(report: &str, known: &Known) -> String {
    let wordy = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    let (mut out, mut prints, mut at) = (String::with_capacity(report.len()), Vec::<&str>::new(), 0);
    while let Some(c) = report[at..].chars().next() {
        let (before, rest) = report.split_at(at);
        let whole = |len: usize| {
            !before.chars().next_back().is_some_and(wordy) && !rest[len..].chars().next().is_some_and(wordy)
        };
        let head = |len: usize| rest.get(..len).filter(|_| whole(len));
        let hex = |(i, b): (usize, u8)| if i % 3 == 2 { b == b':' } else { b.is_ascii_hexdigit() };
        let value = |(value, _): &&(String, String)| head(value.len()).is_some_and(|h| h.eq_ignore_ascii_case(value));
        if let Some(print) = head(PRINT).filter(|print| print.bytes().enumerate().all(hex)) {
            let seen = prints.iter().position(|p| p.eq_ignore_ascii_case(print));
            let n = seen.unwrap_or_else(|| {
                prints.push(print);
                prints.len() - 1
            });
            out.push_str(&format!("fingerprint-{}", n + 1));
            at += PRINT;
        } else if let Some((value, stands)) = known.0.iter().find(value) {
            out.push_str(stands);
            at += value.len();
        } else {
            out.push(c);
            at += c.len_utf8();
        }
    }
    out
}

impl Doctor<'_> {
    /// Prints through `say` and returns whether nothing was wrong. Redacted,
    /// the report is said whole at the end: a fingerprint's number is its
    /// place in all of it.
    pub fn run(&self, say: &mut dyn FnMut(String)) -> bool {
        if !self.redact {
            return self.report(say).0;
        }
        let mut lines = Vec::new();
        let (fine, known) = self.report(&mut |line| lines.push(line));
        say(redact(&lines.join("\n"), &known));
        fine
    }

    /// Whether nothing was wrong, and the values the report may hold.
    fn report(&self, say: &mut dyn FnMut(String)) -> (bool, Known) {
        let mut report = Report { say, wrong: 0 };
        report.head(&format!("config {}", self.config_path.display()));
        let config = match Config::load(self.config_path) {
            Ok(config) => Some(config),
            Err(error) => {
                report.wrong(&format!("{error:#}"));
                None
            }
        };
        // A config that is TOML and is refused can be refused for a name
        // in it, which the report then holds. One that cannot be read, or
        // is not TOML, is refused in words that quote nothing of it, so
        // finding no name there is the right answer and not an error lost.
        let loose = config.is_none().then(|| fs::read_to_string(self.config_path).ok()?.parse::<toml::Table>().ok());
        let tables = loose.as_ref().and_then(|table| table.as_ref()?.get("account")?.as_array());
        let name = |table: &toml::Value| Some(table.get("name")?.as_str()?.to_string());
        let mut named: Vec<String> = tables.into_iter().flatten().filter_map(name).collect();
        match crate::setup::rename::unfinished(&self.dirs.data) {
            Ok(Some((old, new))) => {
                report.wrong(&crate::setup::rename::interrupted(&old, &new));
                named.extend([old, new]);
            }
            Ok(None) => {}
            Err(error) => report.wrong(&format!("{error:#}")),
        }
        let accounts = config.as_ref().map_or(&[][..], |config| &config.accounts);
        match (config.is_some(), accounts.len()) {
            (false, _) => {}
            (true, 0) => report.wrong("it has no account; `tuimex setup` adds one"),
            (true, n) => report.ok(&format!("it reads, and has {n} account{}", if n == 1 { "" } else { "s" })),
        }
        let mut keyring_needed = false;
        let read = if config.as_ref().is_none_or(|config| config.calendars.is_empty()) {
            Ok(Vec::new())
        } else {
            self.calendars_read()
        };
        let mut hosts = Vec::new();
        for account in accounts {
            report.head(&format!("account {} ({})", account.name, account.address));
            keyring_needed |= self.account(&mut report, account);
            if let Some(calendar) = config.as_ref().and_then(|config| config.calendar(&account.name)) {
                keyring_needed |= self.calendar(&mut report, account, calendar, &read, &mut hosts);
            }
        }
        report.head("this machine");
        self.keyring(&mut report, keyring_needed);
        self.programs(&mut report, config.as_ref().and_then(|config| config.agent.as_ref()));
        report.head("files");
        self.modes(&mut report);
        let wrong = report.wrong;
        say(match wrong {
            0 => "\nnothing is wrong".to_string(),
            n => format!("\n{n} thing{} wrong", if n == 1 { " is" } else { "s are" }),
        });
        (wrong == 0, Known::new(accounts, &named, (self.env)("HOME").as_deref()).with_hosts(&hosts))
    }

    /// When each calendar was last read, from the store `tuimex sync` and
    /// the calendar thread keep. None there is none read yet; it is not
    /// made here.
    fn calendars_read(&self) -> Result<Vec<Fetched>, String> {
        if !self.dirs.cache.join("calendar.sqlite").exists() {
            return Ok(Vec::new());
        }
        let store = Calendars::open(&self.dirs.data, &self.dirs.cache).map_err(|e| e.to_string())?;
        store.fetched().map_err(|e| e.to_string())
    }

    /// The account's calendar: its kind and host, whether what signs it
    /// in or names it can be found, and when it was last read. Its server
    /// is not asked: a feed is read whole or not at all, and `tuimex
    /// sync` does that. Returns whether its secret is the keyring's.
    fn calendar(
        &self,
        report: &mut Report,
        account: &config::Account,
        calendar: &config::Calendar,
        read: &Result<Vec<Fetched>, String>,
        hosts: &mut Vec<String>,
    ) -> bool {
        let name = &account.name;
        let (key, needs_keyring) = match &calendar.url {
            Some(url) => {
                let host = tuimex_engine::calendar::host(url).unwrap_or(url);
                hosts.push(host.to_string());
                match tuimex_engine::resolve(name, account.password_cmd.as_deref(), self.keyring) {
                    Ok(_) => report.ok(&format!("its calendar is read over CalDAV from {host}, with its password")),
                    Err(error) => report.wrong(&format!("its calendar, over CalDAV from {host}: {error}")),
                }
                (url.as_str(), account.password_cmd.is_none())
            }
            None => {
                let by_command = calendar.address_cmd.is_some();
                match calendar::feed_address(name, calendar.address_cmd.as_deref(), self.keyring) {
                    Ok(address) => match calendar::host(address.expose()) {
                        Some(host) => {
                            hosts.push(host.to_string());
                            let from = if by_command { "its `address_cmd` printed" } else { "the keyring holds" };
                            report.ok(&format!("its calendar is a feed from {host}, whose address {from}"));
                        }
                        None => report.wrong(
                            "its calendar is a feed whose address is not one tuimex fetches: it begins https:// \
                             and names no user; `tuimex setup` asks for it again",
                        ),
                    },
                    Err(error) => report.wrong(&format!("its calendar is a feed: {error}")),
                }
                (calendar::FEED, !by_command)
            }
        };
        match read {
            Err(error) => report.wrong(&format!("the calendar store could not be read: {error}")),
            Ok(read) => match read.iter().find(|f| f.account == *name && f.calendar == key) {
                Some(f) => {
                    let at = chrono::DateTime::from_timestamp(f.read_at, 0).unwrap_or_default();
                    report.ok(&format!("its calendar was last read {}", at.format("%Y-%m-%d %H:%M UTC")));
                }
                None => report.note("its calendar has not been read yet; `tuimex sync` reads it"),
            },
        }
        needs_keyring
    }

    /// One account, as far as it gets. Returns whether it signs in with
    /// something the keyring holds.
    fn account(&self, report: &mut Report, account: &config::Account) -> bool {
        let name = &account.name;
        let read = net::auth(account).and_then(|kind| Ok((kind, net::servers(account)?, net::roles(account)?)));
        let (kind, (imap, smtp), configured) = match read {
            Ok(read) => read,
            Err(error) => {
                report.wrong(&error.to_string());
                return false;
            }
        };
        let needs_keyring = kind != Auth::Command;
        let pinned = match pin::load(&self.dirs.config, name) {
            Ok(pinned) => pinned,
            Err(error) => {
                report.wrong(&error.to_string());
                return needs_keyring;
            }
        };
        match &pinned {
            Some(pinned) => {
                report.ok(&format!("its server's certificate is pinned: SHA-256 {}", pin::fingerprint(pinned)))
            }
            None => report.ok("its servers are checked against this system's certificate roots"),
        }
        let tokens = (kind == Auth::Gmail).then(|| Tokens::new(self.endpoints.clone()));
        let credential =
            match tuimex_engine::credential(name, account.password_cmd.as_deref(), tokens.as_ref(), self.keyring) {
                Ok(credential) => credential,
                Err(error) => {
                    report.wrong(&error.advice(name));
                    return needs_keyring;
                }
            };
        report.ok(match kind {
            Auth::Password => "its password is in the keyring",
            Auth::Command => "its `password_cmd` printed a password",
            Auth::Gmail => "Google gave an access token for the sign-in kept in the keyring",
        });
        // A refusal over a certificate has one remedy, which is setup's.
        let told = |at: String, error: tuimex_engine::Error| {
            let said = error.advice(name);
            let hint = if said.contains("certificate") && !said.contains("tuimex setup") {
                format!(
                    "\n`tuimex setup`, at \"Accept a new certificate for {name}\", shows it and asks whether to trust it"
                )
            } else {
                String::new()
            };
            format!("{at}: {said}{hint}")
        };
        let at = format!("IMAP {}:{}", imap.host, imap.port);
        match self.net.folders(&imap, name, &credential, pinned.clone()) {
            Ok((folders, gmail)) => {
                let plural = if folders.len() == 1 { "" } else { "s" };
                report.ok(&format!("{at}: signed in, {} folder{plural}", folders.len()));
                let assigned = tuimex_engine::roles(&folders, &configured, gmail);
                let has = |role: Role| assigned.iter().any(|(_, r)| *r == role);
                let none: Vec<&str> = net::ROLES.iter().filter(|(role, _)| !has(*role)).map(|(_, key)| *key).collect();
                if !none.is_empty() {
                    let fix = format!("set `folders` for {name} in the config");
                    report.note(&format!("no folder has the role of: {}; {fix}", none.join(", ")));
                }
            }
            Err(error) => report.wrong(&told(at, error.into())),
        }
        match smtp {
            None => report.wrong("it has no `smtp` server in the config, so nothing can be sent from it"),
            Some(smtp) => {
                let at = format!("SMTP {}:{}", smtp.host, smtp.port);
                match self.net.smtp(&smtp, name, &credential, pinned) {
                    Ok(()) => report.ok(&format!("{at}: signed in, and nothing was sent")),
                    Err(error) => report.wrong(&told(at, error)),
                }
            }
        }
        needs_keyring
    }

    /// Asks the keyring for an entry that is not there: an answer of
    /// "nothing" shows the service is up without reading or writing a
    /// secret.
    fn keyring(&self, report: &mut Report, needed: bool) {
        match self.keyring.password("tuimex-doctor") {
            Ok(_) => report.ok("the keyring answers"),
            Err(reason) => {
                let text = format!("the keyring does not answer: {reason}\n{NO_KEYRING}");
                if needed { report.wrong(&text) } else { report.note(&text) }
            }
        }
    }

    fn programs(&self, report: &mut Report, agent: Option<&config::AgentCommand>) {
        let path = (self.env)("PATH").unwrap_or_default();
        let found = |program: &str| on_path(program, OsStr::new(&path));
        match found("gpg") {
            Some(gpg) => report.ok(&format!("gpg is {}", gpg.display())),
            None => report.note("no `gpg` on the path: signed and encrypted mail cannot be checked, read or made"),
        }
        // As compose chooses: `$VISUAL`, then `$EDITOR`, then vi.
        let set = ["VISUAL", "EDITOR"].iter().find_map(|key| (self.env)(key).filter(|v| !v.is_empty()));
        let editor = set.clone().unwrap_or_else(|| "vi".to_string());
        let program = editor.split_whitespace().next().unwrap_or_default();
        let from = if set.is_some() { "from the environment" } else { "as neither $VISUAL nor $EDITOR is set" };
        match found(program) {
            Some(at) => report.ok(&format!("a message is written in {} ({from})", at.display())),
            None => {
                report.wrong(&format!("the editor `{program}` ({from}) is not on the path; compose cannot open it"))
            }
        }
        report.note(&self.pictures);
        let Some(table) = agent else {
            return report.note("no [agent] table in the config, so there is no `:agent`");
        };
        // Found, never run: running it would send text to its provider.
        match (crate::desk::command(table), &table.preset) {
            (Err(error), _) => report.wrong(&error.to_string()),
            (Ok(command), Some(name)) => match found(&command[0]) {
                Some(at) => {
                    report.ok(&format!("`:agent` uses the built-in {name}, found at {}; it was not run", at.display()))
                }
                None => report.wrong(&format!("[agent] use = \"{name}\", and no `{}` is on the path", command[0])),
            },
            (Ok(command), None) => match found(&command[0]) {
                Some(at) => {
                    report.ok(&format!("`:agent` runs {}; it was not run", at.display()));
                    report.note(
                        "that is a command of your own: tuimex hands it text and cannot vouch for what it then does, \
                         opening a link included (use = \"claude\" is the one tuimex can)",
                    );
                }
                None => report.wrong(&format!("[agent] command: `{}` is not a program on the path", command[0])),
            },
        }
    }

    /// Every directory tuimex owns is 0700 and every file in one 0600
    /// (ADR-011). A link is not followed: what it points at is not here.
    fn modes(&self, report: &mut Report) {
        let dirs = [&self.dirs.config, &self.dirs.data, &self.dirs.cache, &self.dirs.state];
        let (mut seen, mut open, mut unread) = (0usize, Vec::new(), Vec::new());
        let mut pending: Vec<PathBuf> = dirs.iter().filter(|dir| dir.exists()).map(|dir| dir.to_path_buf()).collect();
        while let Some(path) = pending.pop() {
            let Ok(meta) = fs::symlink_metadata(&path) else {
                unread.push(path);
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            seen += 1;
            let (mode, wanted) = (meta.permissions().mode() & 0o777, if meta.is_dir() { 0o700 } else { 0o600 });
            if mode != wanted {
                open.push(format!("{} is {mode:03o}, not {wanted:03o}", path.display()));
            }
            if meta.is_dir() {
                match fs::read_dir(&path) {
                    Ok(entries) => pending.extend(entries.flatten().map(|entry| entry.path())),
                    Err(_) => unread.push(path),
                }
            }
        }
        for path in &unread {
            report.wrong(&format!("{} could not be read", path.display()));
        }
        for line in open.iter().take(NAMED) {
            report.wrong(line);
        }
        if open.len() > NAMED {
            report.wrong(&format!("and {} more", open.len() - NAMED));
        }
        if !open.is_empty() {
            let all: Vec<String> = dirs.iter().map(|dir| dir.display().to_string()).collect();
            report.note(&format!("to mend: chmod -R u=rwX,go= {}", all.join(" ")));
        } else if unread.is_empty() {
            report.ok(&format!("{seen} files and directories under tuimex's four are readable by you alone"));
        }
    }
}

/// What the terminal answers when asked for its image protocol.
fn pictures() -> String {
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return "pictures: not asked, as this is not a terminal".to_string();
    }
    // A terminal that does not answer has no protocol to draw with.
    let protocol = Picker::from_query_stdio().ok().map(|picker| picker.protocol_type());
    match protocol {
        Some(ProtocolType::Kitty) => "pictures: this terminal draws them (the kitty protocol)".to_string(),
        Some(ProtocolType::Sixel) => "pictures: this terminal draws them (sixel)".to_string(),
        Some(ProtocolType::Iterm2) => "pictures: this terminal draws them (the iTerm2 protocol)".to_string(),
        Some(ProtocolType::Halfblocks) | None => {
            "pictures: this terminal has no image protocol; an attached picture is named, and opens outside".to_string()
        }
    }
}

/// `tuimex doctor`, against the network and the keyring.
pub fn command(config: Option<&Path>, redact: bool) -> Result<ExitCode> {
    let dirs = Dirs::from_env()?;
    let path = config::path(config, &dirs.config);
    let doctor = Doctor {
        keyring: &tuimex_engine::SystemKeyring,
        net: &net::Live,
        dirs: &dirs,
        config_path: &path,
        endpoints: Endpoints::google(),
        env: &|key| std::env::var(key).ok(),
        pictures: pictures(),
        redact,
    };
    let fine = doctor.run(&mut |line| println!("{line}"));
    Ok(if fine { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

#[cfg(test)]
mod tests;

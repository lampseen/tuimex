//! What the reader asks for outside the mailbox: a link opened, a file
//! saved or handed to the system's program for its type, the sealed copy
//! written and shown in a browser. The UI has no handle on any of it
//! (ADR-001); each arrives here as an `Ask` the user made for that one
//! thing (ADR-008).
//!
//! Nothing the UI sent is trusted to name what is opened: the link, the
//! file's name and the page are each worked out again from the message.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tuimex_content as content;
use tuimex_core::{Action, Ask, Body, Event, Picture, Unsubscribe, assess};
use tuimex_store::files;

use crate::pictures;

/// ADR-011: a browser copy or an opened file is removed after a day.
const KEPT: Duration = Duration::from_secs(24 * 60 * 60);
/// The widest the reader's text is asked for. The reader wraps it to the
/// screen itself, so a paragraph arrives as one line.
pub(crate) const UNWRAPPED: usize = 10_000;

/// Whatever shows the user a web address or a file: the system's opener,
/// or in a test a list of what it would have been given.
pub trait Opener {
    fn open(&mut self, target: &str) -> std::io::Result<()>;
}

/// The program the system has for the target's type. No test makes one:
/// a test must never start a browser or a viewer.
pub struct System;

impl Opener for System {
    fn open(&mut self, target: &str) -> std::io::Result<()> {
        open::that_detached(target)
    }
}

/// The message as the reader shows it, with the size of each picture it
/// can draw.
pub fn read(raw: &[u8]) -> Result<Body, content::Error> {
    let mut body = content::body(raw, UNWRAPPED)?;
    // The pictures the message shows in its text, which are the ones it
    // attached as part of it. A part that cannot be read is no picture.
    let sized = |index: usize| content::attachment(raw, index).ok().and_then(|file| pictures::size(&file.data));
    let inline = body.attachments.iter().filter(|a| a.inline);
    body.pictures = inline
        .filter_map(|a| sized(a.index).map(|(width, height)| Picture { index: a.index, width, height }))
        .collect();
    Ok(body)
}

pub struct Outside {
    opener: Box<dyn Opener>,
    /// Where `:save` puts a file that is given no path.
    downloads: PathBuf,
    /// The cache directory: what is written to be shown goes under it.
    cache: PathBuf,
    /// What this run wrote to be shown, which goes when it ends.
    shown: Vec<PathBuf>,
}

fn failed(what: impl std::fmt::Display, why: impl std::fmt::Display) -> String {
    format!("{what}: {why}")
}

impl Outside {
    /// Removes what an earlier run left to be shown more than a day ago.
    pub fn new(opener: Box<dyn Opener>, downloads: PathBuf, cache: &Path) -> Outside {
        let outside = Outside { opener, downloads, cache: cache.join("shown"), shown: Vec::new() };
        let old = |path: &PathBuf| {
            let age = path.symlink_metadata().and_then(|m| m.modified()).map(|at| SystemTime::now().duration_since(at));
            // A date in the future is no age at all, and the file stays.
            matches!(age, Ok(Ok(age)) if age > KEPT)
        };
        // A cache directory that is not there yet holds nothing to remove,
        // and an entry that cannot be read is left for the next run.
        let left = std::fs::read_dir(&outside.cache).into_iter().flatten().flatten().map(|entry| entry.path());
        left.filter(old).for_each(|path| remove(&path));
        outside
    }

    /// Carries out `ask` on the message whose reading gave `body`, and
    /// says what the UI is still owed: where a file went, or why nothing
    /// happened. `raw` is the message, when there are bytes behind it.
    pub fn carry_out(&mut self, ask: &Ask, body: &Body, raw: Option<&[u8]>) -> Option<Event> {
        let done = match &ask.action {
            Action::OpenLink(n) => self.link(body, *n).map(|()| None),
            Action::Unsubscribe => self.unsubscribe(body).map(|()| None),
            Action::SaveAttachment(index, to) => self.save(raw, *index, to).map(|path| Some(Event::Saved { path })),
            Action::OpenAttachment(index) => self.open_file(raw, *index).map(|()| None),
            Action::Browser { remote } => self.browser(body, raw, *remote).map(|()| None),
            _ => return None,
        };
        match done {
            Ok(told) => told,
            Err(reason) => Some(Event::ActionFailed { serial: ask.serial, reason, hint: String::new() }),
        }
    }

    fn open(&mut self, what: &str, target: &str) -> Result<(), String> {
        self.opener.open(target).map_err(|e| failed(what, e))
    }

    /// ADR-018, whatever the UI did before asking: only a web address
    /// opens, with its tracking parameters gone.
    fn web(&mut self, what: &str, url: &str, shown: &str) -> Result<(), String> {
        let found = assess(url, shown);
        if found.blocked || !(found.url.starts_with("http://") || found.url.starts_with("https://")) {
            return Err(failed(what, "only web links are opened"));
        }
        self.open(what, &found.url)
    }

    fn link(&mut self, body: &Body, n: usize) -> Result<(), String> {
        let what = format!(":link {n}");
        let link = body.links.iter().find(|link| link.n == n).ok_or_else(|| failed(&what, "no such link"))?;
        self.web(&what, &link.url, &link.shown)
    }

    fn unsubscribe(&mut self, body: &Body) -> Result<(), String> {
        match &body.unsubscribe {
            Some(Unsubscribe::Https(url)) => self.web(":unsubscribe", url, ""),
            _ => Err(failed(":unsubscribe", "this message names no page for it")),
        }
    }

    /// A file of this run's own under the cache directory, in a directory
    /// of its own so that it keeps the name its type is told by.
    fn show(&mut self, what: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = self.cache.join(format!("{since}-{}-{}", std::process::id(), self.shown.len()));
        let path = dir.join(name);
        let written = files::private_dir(&dir).and_then(|()| files::create(&path)).and_then(|mut f| f.write_all(bytes));
        self.shown.push(dir);
        written.map_err(|e| failed(what, e))?;
        let target = path.to_str().ok_or_else(|| failed(what, "its path is not text"))?.to_string();
        self.open(what, &target)
    }

    fn attachment(what: &str, raw: Option<&[u8]>, index: usize) -> Result<content::Payload, String> {
        let raw = raw.ok_or_else(|| failed(what, "this mailbox holds no files"))?;
        content::attachment(raw, index).map_err(|e| failed(what, e))
    }

    /// `:save <n> [path]`. With no path the file goes to the download
    /// directory under its own name, made safe (ADR-007), and beside a
    /// file already of that name. A path is the user's own choice, read
    /// against the download directory, and is never written over. Returns
    /// where the file is.
    fn save(&mut self, raw: Option<&[u8]>, index: usize, to: &Path) -> Result<String, String> {
        let what = ":save";
        let file = Self::attachment(what, raw, index)?;
        let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
        let typed = match (to.strip_prefix("~"), &home) {
            (Ok(rest), Some(home)) => home.join(rest),
            _ => self.downloads.join(to),
        };
        let named = to.as_os_str().is_empty() || typed.is_dir();
        let (dir, name) = if named {
            (typed, file.name.clone())
        } else {
            let name =
                typed.file_name().and_then(|n| n.to_str()).ok_or_else(|| failed(what, "that is not a file's path"))?;
            (typed.parent().unwrap_or(Path::new("/")).to_path_buf(), name.to_string())
        };
        if !dir.exists() {
            files::private_dir(&dir).map_err(|e| failed(what, e))?;
        }
        let (stem, extension) = name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()).unwrap_or((&name, ""));
        let numbered =
            |n: u32| if extension.is_empty() { format!("{stem}-{n}") } else { format!("{stem}-{n}.{extension}") };
        let tries = if named { 100 } else { 1 };
        for n in 1..=tries {
            let path = dir.join(if n == 1 { name.clone() } else { numbered(n) });
            match files::create(&path) {
                Ok(mut made) => {
                    made.write_all(&file.data).map_err(|e| failed(what, e))?;
                    return Ok(homely(&path, home.as_deref()));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(failed(what, e)),
            }
        }
        Err(failed(what, format_args!("{name} is there already")))
    }

    fn open_file(&mut self, raw: Option<&[u8]>, index: usize) -> Result<(), String> {
        let file = Self::attachment(":open", raw, index)?;
        self.show(":open", &file.name, &file.data)
    }

    /// ADR-009: the sanitised page, sealed unless the user agreed to the
    /// hosts the message itself named.
    fn browser(&mut self, body: &Body, raw: Option<&[u8]>, remote: bool) -> Result<(), String> {
        let what = ":browser";
        let raw = raw.ok_or_else(|| failed(what, "this mailbox holds no pages"))?;
        let hosts = if remote { body.image_hosts.as_slice() } else { &[] };
        let page = content::browser_copy(raw, hosts).map_err(|e| failed(what, e))?;
        let page = page.ok_or_else(|| failed(what, "this message has no HTML to show"))?;
        self.show(what, "message.html", page.as_bytes())
    }
}

/// A path as a person writes it: what is under `home` begins with `~`.
fn homely(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

fn remove(path: &Path) {
    let gone = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    // What cannot be removed now is tried again by the next run, once it
    // is a day old; the log says it was left.
    if let Err(e) = gone {
        tracing::warn!(path = %path.display(), error = %e, "left behind");
    }
}

/// ADR-011: what was written to be shown goes when tuimex does.
impl Drop for Outside {
    fn drop(&mut self) {
        self.shown.iter().for_each(|path| remove(path));
    }
}

#[cfg(test)]
mod tests;

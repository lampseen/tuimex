//! Renaming an account (ADR-050). The name is where four things are kept:
//! the mail directory, the keyring entries, the pinned certificate and
//! the config's `name`. They are moved in an order in which stopping
//! anywhere leaves either everything under the old name or a rename that
//! the same steps, run again, finish; and a file in the data directory
//! says which rename that is until the last step is done.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use tuimex_engine::{Keyring, calendar, oauth};
use tuimex_store::files;

use super::write;
use crate::config::Config;

/// In the data directory while a rename is under way: the old name and
/// the new, a line each.
const MARK: &str = "renaming";

/// Where an account's four things are.
pub struct Places<'a> {
    pub data: &'a Path,
    pub cache: &'a Path,
    pub config_dir: &'a Path,
    pub config_path: &'a Path,
    /// None for an account that signs in with a command: nothing of its
    /// is in the keyring, and a machine with no keyring can rename it.
    pub keyring: Option<&'a dyn Keyring>,
}

/// The rename that was begun and not finished: the old name and the new.
pub fn unfinished(data: &Path) -> Result<Option<(String, String)>> {
    let path = data.join(MARK);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let mut names = text.lines().map(str::to_string);
    Ok(names.next().zip(names.next()))
}

/// What is said wherever tuimex starts while a rename is unfinished.
pub fn interrupted(old: &str, new: &str) -> String {
    format!(
        "the rename of account {old} to {new} was begun and not finished, so its mail and its config do not agree; \
         run `tuimex setup`, which finishes it"
    )
}

/// The keyring entries that are an account's own, under a name. The
/// client every Gmail account shares is nobody's and stays.
fn entries(name: &str) -> [String; 3] {
    [name.to_string(), oauth::refresh_entry(name), calendar::feed_entry(name)]
}

/// Moves `from` to `to`, or finds it already moved. Both there is a
/// state no step here makes, and nothing is done to it.
fn moved(from: &Path, to: &Path) -> Result<()> {
    match (from.symlink_metadata().is_ok(), to.symlink_metadata().is_ok()) {
        (false, _) => Ok(()),
        (true, true) => bail!("{} and {} are both there; one of them has to go first", from.display(), to.display()),
        (true, false) => files::rename(from, to).with_context(|| {
            format!("moving {} to {}; a move across filesystems is not made by copying", from.display(), to.display())
        }),
    }
}

type Step = fn(&Places, &str, &str) -> Result<Option<String>>;

/// The steps, in the order that makes each safe to stop after. Until
/// `mail` has run everything is under the old name; from then on the
/// steps left finish the rename when run again. Each finds its own work
/// done and passes over it.
const STEPS: [(&str, Step); 6] = [
    // First, and by copying: a keyring that refuses stops the rename
    // while the old name still opens everything.
    ("copying the sign-in to the new name in the keyring", |at, old, new| {
        for (was, now) in entries(old).iter().zip(entries(new)) {
            let Some(keyring) = at.keyring else { break };
            if let Some(secret) =
                keyring.password(was).map_err(|why| anyhow!("the keyring could not be read: {why}"))?
            {
                keyring.set(&now, &secret).map_err(|why| anyhow!("the keyring would not keep it: {why}"))?;
            }
        }
        Ok(None)
    }),
    // Before the mail moves: the index names every message by its
    // account, and one that outlived the move would name files that are
    // gone. Without it the next start reads the Maildir again (ADR-013).
    // What was fetched of calendars is filed by account too, and is
    // fetched again (ADR-055).
    ("dropping the index, which is rebuilt from the mail", |at, _, _| {
        let calendars = ["calendar.sqlite", "calendar.sqlite-wal", "calendar.sqlite-shm"];
        for file in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"].into_iter().chain(calendars) {
            match fs::remove_file(at.cache.join(file)) {
                Err(e) if e.kind() != ErrorKind::NotFound => return Err(e).context(format!("removing {file}")),
                _ => {}
            }
        }
        Ok(None)
    }),
    // The account's own events go with its mail (ADR-055).
    ("moving the mail", |at, old, new| {
        moved(&at.data.join(old), &at.data.join(new))?;
        let events = at.data.join(".calendars");
        moved(&events.join(old), &events.join(new)).map(|()| None)
    }),
    ("moving the pinned certificate", |at, old, new| {
        moved(&at.config_dir.join(format!("{old}.pem")), &at.config_dir.join(format!("{new}.pem"))).map(|()| None)
    }),
    ("changing the name in the config", |at, old, new| {
        let existing =
            fs::read_to_string(at.config_path).with_context(|| format!("reading {}", at.config_path.display()))?;
        let config = Config::parse(&existing)?;
        if config.account(old).is_some() {
            write::save(at.config_path, &write::renamed(&existing, old, new)?, at.config_dir)?;
        } else if config.account(new).is_none() {
            bail!("the config has neither {old} nor {new}");
        }
        Ok(None)
    }),
    // Last: until the config says the new name, the old entries are
    // what signs the account in. One that will not go is left, and said.
    ("removing the old name from the keyring", |at, old, _| {
        let Some(keyring) = at.keyring else { return Ok(None) };
        let refused = entries(old).iter().find_map(|entry| keyring.remove(entry).err());
        Ok(refused
            .map(|why| format!("The keyring would not remove what it kept under {old} ({why}). Nothing uses it now.")))
    }),
];

/// The step whose completion is the rename having happened.
const MAIL: usize = 2;

/// Why `new` cannot be the name of `old`, before anything is touched.
pub fn refused(at: &Places, config: &Config, existing: &str, old: &str, new: &str) -> Option<String> {
    let taken = |path: PathBuf| path.symlink_metadata().is_ok();
    if !super::valid_name(new) {
        Some("Letters, digits, `-`, `_` and `.`; not starting with a dot.".to_string())
    } else if new == old || config.account(new).is_some() {
        Some("There is an account of that name already.".to_string())
    } else if taken(at.data.join(new)) || taken(at.config_dir.join(format!("{new}.pem"))) {
        Some(format!("There is mail or a certificate under {new} already, from an account that is not in the config."))
    } else {
        write::renamed(existing, old, new).err().map(|error| error.to_string())
    }
}

/// Renames `old` to `new`, or finishes a rename that was begun: with the
/// mail directory held alone, the mark written, each step, the mark
/// removed. Returns what is left to tell. An error before the mail has
/// moved takes the mark away again, since everything is under the old
/// name; after it, the mark stays for the next run to finish from.
pub fn carry_out(at: &Places, old: &str, new: &str) -> Result<Vec<String>> {
    let _alone = tuimex_store::sole(at.data).map_err(|error| {
        anyhow!("{error}: close the tuimex that is open, or wait for `tuimex sync` to end, and rename then")
    })?;
    let mark = at.data.join(MARK);
    files::replace(&mark, format!("{old}\n{new}\n").as_bytes())
        .with_context(|| format!("writing {}", mark.display()))?;
    let mut notes = Vec::new();
    for (n, (doing, step)) in STEPS.iter().enumerate() {
        match step(at, old, new) {
            Ok(note) => notes.extend(note),
            Err(error) if n <= MAIL && !at.data.join(new).exists() => {
                fs::remove_file(&mark).with_context(|| format!("removing {}", mark.display()))?;
                return Err(error.context(format!("{doing} failed, and nothing was renamed")));
            }
            Err(error) => return Err(error.context(format!("{doing} failed; {}", interrupted(old, new)))),
        }
    }
    fs::remove_file(&mark).with_context(|| format!("removing {}", mark.display()))?;
    Ok(notes)
}

#[cfg(test)]
mod tests;

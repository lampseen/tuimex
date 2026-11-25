//! The only place in the workspace that creates a file or a directory
//! (ADR-011). Everything made here is readable by its owner alone, and a
//! file is made by a call that fails if the path is already taken, so
//! nothing planted at that path beforehand is ever written through.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

/// Makes `path` and any missing parents, each owner-only. A directory that
/// was already there is narrowed to the same mode.
pub fn private_dir(path: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(DIR_MODE).create(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(DIR_MODE))
}

/// A new owner-only file, opened for writing. Fails if `path` exists.
pub fn create(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).mode(FILE_MODE).open(path)
}

/// A file that grows across runs, such as a log: made as `create` makes it
/// the first time, and afterwards reopened to append.
pub fn append(path: &Path) -> io::Result<File> {
    match OpenOptions::new().append(true).create_new(true).mode(FILE_MODE).open(path) {
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            // A link here was not made by `create`, and appending through
            // it would write wherever it points.
            if !fs::symlink_metadata(path)?.file_type().is_file() {
                return Err(io::Error::new(ErrorKind::InvalidInput, "not a regular file"));
            }
            let file = OpenOptions::new().append(true).open(path)?;
            file.set_permissions(fs::Permissions::from_mode(FILE_MODE))?;
            Ok(file)
        }
        other => other,
    }
}

/// Gives the file at `from` the name `to` instead, failing if `to` is
/// taken. A link then an unlink, because a rename would silently replace
/// whatever was at `to`; this is how a Maildir delivery leaves `tmp`.
pub fn place(from: &Path, to: &Path) -> io::Result<()> {
    fs::hard_link(from, to)?;
    fs::remove_file(from)
}

/// Gives the file or directory at `from` the name `to` in one step, which
/// has happened or has not: nothing is copied, so it fails across
/// filesystems, and it fails if `to` is taken.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    if to.symlink_metadata().is_ok() {
        return Err(io::Error::new(ErrorKind::AlreadyExists, "the new name is taken"));
    }
    fs::rename(from, to)
}

/// Puts `bytes` at `path` whole: a reader, or a crash, sees the old content
/// or the new and never part of either. The one case where an existing path
/// is written over, and only by a rename from a file `create` made beside it.
pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut name = path.file_name().ok_or(ErrorKind::InvalidInput)?.to_os_string();
    name.push(".tmp");
    replace_through(&path.with_file_name(name), path, bytes)
}

/// As `replace`, with the new content written at `tmp` first: for a
/// directory whose every file means something, where the file `replace`
/// makes beside `path` would be read as one more. `tmp` must be on the
/// same filesystem as `path`.
pub fn replace_through(tmp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    match fs::remove_file(tmp) {
        // Nothing was left behind by a run that died mid-replace.
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        other => other?,
    }
    let mut file = create(tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(tmp, path)
}

/// At most `most` bytes of the regular file at `path`, which this module
/// made: a link or anything else planted there is refused rather than
/// read, so the file cannot be pointed at another, or at a device that
/// never ends.
pub fn read_own(path: &Path, most: u64) -> io::Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::new(ErrorKind::InvalidInput, "not a regular file"));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(most).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests;

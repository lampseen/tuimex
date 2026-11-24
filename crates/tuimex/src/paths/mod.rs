//! The four directories tuimex reads and writes, and no others (ADR-011).
//! XDG on Linux and on macOS, so the paths the documents name are the real
//! ones on both. The current directory is never one of them.

use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug, PartialEq)]
pub struct Dirs {
    /// What the user said: `config.toml`, pinned certificates.
    pub config: PathBuf,
    /// Mail.
    pub data: PathBuf,
    /// What tuimex worked out, and can work out again.
    pub cache: PathBuf,
    /// Logs.
    pub state: PathBuf,
}

impl Dirs {
    /// Under test this is a scratch root whatever the environment says, so
    /// no unit test can reach the user's mail, config or logs.
    pub fn from_env() -> Result<Self> {
        #[cfg(test)]
        let (var, home) = (|_: &str| None, scratch_root());
        #[cfg(not(test))]
        let (var, home) = (
            |k: &str| std::env::var_os(k).map(PathBuf::from),
            directories::BaseDirs::new()
                .ok_or_else(|| anyhow::anyhow!("no home directory; tuimex cannot place its files"))?
                .home_dir()
                .to_path_buf(),
        );
        Ok(Self::resolve(var, &home))
    }

    fn resolve(var: impl Fn(&str) -> Option<PathBuf>, home: &Path) -> Self {
        // The XDG spec says a relative value is invalid and must be ignored;
        // joining it to the current directory is what ADR-011 rules out.
        let xdg = |key: &str, fallback: &str| {
            var(key).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(fallback)).join("tuimex")
        };
        Dirs {
            config: xdg("XDG_CONFIG_HOME", ".config"),
            data: xdg("XDG_DATA_HOME", ".local/share"),
            cache: xdg("XDG_CACHE_HOME", ".cache"),
            state: xdg("XDG_STATE_HOME", ".local/state"),
        }
    }
}

/// One per test process, under the build directory: the system temporary
/// directory is shared, and ADR-011 keeps tuimex out of it.
#[cfg(test)]
fn scratch_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("tuimex-{}", std::process::id()))
}

#[cfg(test)]
mod tests;

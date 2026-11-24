//! What the unit tests of every module share.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tuimex_core::{Address, Security, Timestamp};

use crate::Parsed;

/// A path of this test's own under the build directory, with nothing at
/// it. Never the system temporary directory, and never anything the user
/// owns.
pub fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("store-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

pub fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

pub fn address(name: &str, email: &str) -> Address {
    Address { name: Some(name.to_string()).filter(|n| !n.is_empty()), email: email.to_string() }
}

/// A parse result with nothing in it but what the test is about.
pub fn parsed(subject: &str, text: &str) -> Parsed {
    Parsed {
        date: Timestamp(1_795_000_000),
        from: vec![address("Grace Hopper", "grace@example.net")],
        to: vec![address("Ada Lovelace", "ada@example.com")],
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        subject: subject.to_string(),
        message_id: None,
        references: Vec::new(),
        has_attachments: false,
        has_invite: false,
        has_unsubscribe: false,
        attachments: Vec::new(),
        security: Security::None,
        text: text.to_string(),
    }
}

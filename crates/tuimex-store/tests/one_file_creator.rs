//! ADR-011: one module creates files, so that one module is where their
//! mode and the create-new rule have to be right.

// A scan that cannot read its input must fail rather than pass on less,
// which is what unwrap does; clippy's test allowance does not reach helpers.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

/// Every way std offers to bring a file or directory into being, or to
/// give an existing one a new path.
const CREATORS: [&str; 8] =
    ["File::create", "fs::write", "OpenOptions", "create_dir", "DirBuilder", "fs::copy", "hard_link", "fs::rename"];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") && entry.file_name() != "tests.rs" {
            out.push(path);
        }
    }
}

#[test]
fn only_the_files_module_creates_a_file() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let the_module = Path::new("tuimex-store/src/files/mod.rs");
    let mut files = Vec::new();
    for c in fs::read_dir(&crates).unwrap().map(Result::unwrap) {
        sources(&c.path().join("src"), &mut files);
    }
    assert!(files.iter().any(|f| f.ends_with(the_module)), "the scan did not reach {}", the_module.display());
    let mut found = Vec::new();
    for file in files.iter().filter(|f| !f.ends_with(the_module)) {
        let text = fs::read_to_string(file).unwrap();
        for (n, line) in text.lines().enumerate() {
            if let Some(c) = CREATORS.iter().find(|c| line.contains(**c)) {
                found.push(format!("{}:{}: {c}", file.strip_prefix(&crates).unwrap().display(), n + 1));
            }
        }
    }
    assert!(found.is_empty(), "files are created only in tuimex_store::files (ADR-011):\n{}", found.join("\n"));
}

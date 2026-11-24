//! ADR-003: the only colour values in this repository are the built-in
//! palette's. The check is inverted: it fails on any `#rrggbb` that is not
//! in the palette, so it names nothing it forbids and catches any palette
//! that strays in.

// A test that cannot read its inputs should fail loudly, which is what
// unwrap does; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use tuimex_ui::palette::{Tok, hex};

/// Every file of the repository. `design/` is the palette's source; the
/// build directory and dot-directories other than `.github` hold other
/// checkouts and tool state, not what ships.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let (path, name) = (entry.path(), entry.file_name());
        let name = name.to_string_lossy();
        if name == "target" || name == "design" || (name.starts_with('.') && name != ".github") {
            continue;
        }
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Exactly six hex digits after a `#`, at least one of them a letter, so
/// an issue number is not taken for a colour.
fn colours(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (i, _) in text.match_indices('#') {
        let digits: String = text[i + 1..].chars().take_while(char::is_ascii_hexdigit).collect();
        if digits.len() == 6 && digits.chars().any(|c| c.is_ascii_alphabetic()) {
            found.push(format!("#{}", digits.to_ascii_lowercase()));
        }
    }
    found
}

#[test]
fn only_the_built_in_palette_is_shipped() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let allowed: BTreeSet<String> = Tok::ALL.iter().map(|t| hex(t.truecolor())).collect();
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() > 100, "walked only {} files from {}", files.len(), root.display());
    let mut stray = Vec::new();
    for file in files {
        // A file that is not text cannot hold a colour a reader would see.
        let Ok(text) = fs::read_to_string(&file) else { continue };
        for colour in colours(&text) {
            if !allowed.contains(&colour) {
                stray.push(format!("{}: {colour}", file.strip_prefix(&root).unwrap_or(&file).display()));
            }
        }
    }
    assert!(stray.is_empty(), "colour values outside the built-in palette (ADR-003):\n{}", stray.join("\n"));
}

#[test]
fn the_scanner_sees_a_colour_and_not_an_issue_number() {
    let sample = format!("accent = \"{}\" and #4821 and #123456 and #abcdefa", hex((0x7c, 0xc4, 0xdc)).to_uppercase());
    assert_eq!(colours(&sample), vec![hex((0x7c, 0xc4, 0xdc))]);
}

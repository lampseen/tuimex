//! ADR-007: `tuimex-content` cannot reach a file, a socket or a process.
//! Its manifest keeps out every crate that could, and this scan keeps out
//! the standard library's own doors.

// A test that cannot read its inputs should fail loudly, which is what
// unwrap does; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// The modules of `std` that do I/O, or reach the machine around the
/// process.
const DOORS: [&str; 5] = ["fs", "net", "process", "os", "env"];

/// A source with its whitespace removed, except where it separates two
/// words, so a line break hides nothing and `fs as f` is still `fs`.
fn dense(source: &str) -> String {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut gap = false;
    for c in source.chars() {
        if c.is_whitespace() {
            gap = true;
            continue;
        }
        if gap && word(c) && out.ends_with(word) {
            out.push(' ');
        }
        gap = false;
        out.push(c);
    }
    out
}

/// Every module of `std` a source names, through a path or a `use` group.
fn std_modules(source: &str) -> Vec<String> {
    let dense = dense(source);
    let mut found = Vec::new();
    for (at, _) in dense.match_indices("std::") {
        let rest = &dense[at + 5..];
        let names = match rest.strip_prefix('{') {
            Some(group) => group.split('}').next().unwrap(),
            None => rest.split([',', ';', '}']).next().unwrap(),
        };
        found.extend(
            names.split(',').map(|name| name.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect()),
        );
    }
    found
}

#[test]
fn the_scan_sees_through_every_spelling() {
    for (source, door) in [
        ("use std::fs;", "fs"),
        ("let f = std::fs::read(p);", "fs"),
        ("use std::{collections::HashMap, net::TcpStream};", "net"),
        ("use std::{\n    borrow::Cow,\n    process,\n};", "process"),
        ("use std :: os :: unix;", "os"),
        ("::std::env::var(\"HOME\")", "env"),
        ("use std::fs as f;", "fs"),
        ("use std::{io, fs as f};", "fs"),
    ] {
        assert!(std_modules(source).iter().any(|m| m == door), "{source}");
    }
    assert_eq!(
        std_modules("use std::collections::HashMap; use std::{rc::Rc, cell::Cell};"),
        ["collections", "rc", "cell"]
    );
}

#[test]
fn the_content_crate_does_no_io() {
    let mut files = Vec::new();
    sources(&crate_dir().join("src"), &mut files);
    assert!(files.len() >= 8, "the walk found only {} source files", files.len());
    let mut bad = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file).unwrap();
        let name = file.strip_prefix(crate_dir()).unwrap().display().to_string();
        for module in std_modules(&source) {
            if DOORS.contains(&module.as_str()) {
                bad.push(format!("{name}: std::{module}"));
            }
        }
        // Renaming `std` would walk every path past the check above.
        for alias in ["use std as", "extern crate std", "include_bytes!", "include_str!"] {
            if dense(&source).contains(alias) {
                bad.push(format!("{name}: {alias}"));
            }
        }
    }
    assert!(bad.is_empty(), "tuimex-content reaches for I/O:\n{}", bad.join("\n"));
}

/// The crates that parse, and the two that carry data and errors. A new
/// name here is a new way out of the crate, and needs the argument that it
/// is not one. `roxmltree` builds a read-only tree from a string it is
/// given and its one dependency searches bytes (ADR-062).
const ALLOWED: [&str; 7] = ["ammonia", "base64", "html2text", "mail-parser", "roxmltree", "thiserror", "tuimex-core"];

#[test]
fn the_content_crate_depends_on_parsers_alone() {
    let manifest = fs::read_to_string(crate_dir().join("Cargo.toml")).unwrap();
    let mut table = String::new();
    let mut seen = 0;
    for line in manifest.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        if let Some(header) = line.strip_prefix('[') {
            table = header.trim_end_matches(']').to_string();
            assert!(
                matches!(table.as_str(), "package" | "lints" | "dependencies"),
                "[{table}] is a table this test does not read; a dependency could hide in it"
            );
        } else if table == "dependencies" {
            let name = line.split(['.', '=', ' ']).next().unwrap();
            assert!(ALLOWED.contains(&name), "{name} is not a crate tuimex-content may depend on");
            seen += 1;
        }
    }
    assert!(seen > 0, "no dependency was read from the manifest");
}

use std::path::Path;

use super::{Found, KEYMAP, Key, TIMEOUT_MS, find, hint, markdown};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn quoted<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split(&format!("{key}: '")).nth(1)?.split('\'').next()
}

/// ADR-002: the keys are the design's, entry for entry and in its order.
#[test]
fn the_table_is_the_design_s_keymap() {
    let js = std::fs::read_to_string(root().join("design/tui.js")).unwrap();
    let table = js.split("const KEYMAP = {").nth(1).unwrap().split("\n};").next().unwrap();
    assert!(table.contains(&format!("timeoutMs: {TIMEOUT_MS},")));
    let mut designed = Vec::new();
    for line in table.lines() {
        if let Some(mode) = quoted(line, "mode") {
            designed.push(format!("{mode}: {}", quoted(line, "title").unwrap()));
        } else if let Some(id) = quoted(line, "id") {
            let (keys, does) = (quoted(line, "keys").unwrap(), quoted(line, "does").unwrap());
            designed.push(format!("{id} | {keys} | {does} | {}", quoted(line, "hint").unwrap_or("")));
        }
    }
    let mut ours = Vec::new();
    for table in KEYMAP {
        ours.push(format!("{}: {}", table.mode, table.title));
        ours.extend(table.keys.iter().map(|b| format!("{} | {} | {} | {}", b.id, b.keys, b.does, b.hint)));
    }
    assert!(designed.len() > 60, "read only {} lines of the design's keymap", designed.len());
    assert_eq!(ours, designed);
}

/// `make keymap` runs this with TUIMEX_BLESS set, which writes the file
/// before comparing.
#[test]
fn docs_keymap_md_is_generated_from_the_table() {
    let path = root().join("docs/keymap.md");
    if std::env::var_os("TUIMEX_BLESS").is_some() {
        std::fs::write(&path, markdown()).unwrap();
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(on_disk == markdown(), "docs/keymap.md is stale; run `make keymap`");
}

fn bound(modes: &[&str], typed: &[Key]) -> Option<(&'static str, usize)> {
    match find(modes, typed) {
        Found::Bound(binding, i) => Some((binding.id, i)),
        _ => None,
    }
}

#[test]
fn a_press_is_found_by_mode_and_says_which_of_the_binding_s_keys_it_was() {
    let list = ["list", "global"];
    assert_eq!(bound(&list, &[Key::Char('j')]), Some(("list.move", 0)));
    assert_eq!(bound(&list, &[Key::Char('k')]), Some(("list.move", 1)));
    assert_eq!(bound(&list, &[Key::Ctrl('u')]), Some(("list.page", 1)));
    assert_eq!(bound(&list, &[Key::Char(' ')]), Some(("list.mark", 0)));
    assert_eq!(bound(&list, &[Key::BackTab]), Some(("global.section-prev", 1)));
    assert_eq!(bound(&list, &[Key::Char('7')]), Some(("global.section", 6)));
    assert_eq!(bound(&["command"], &[Key::Ctrl('u')]), Some(("command.erase", 1)));
    assert_eq!(bound(&["command"], &[Key::Char('j')]), None, "a mode binds only its own table");
    assert_eq!(bound(&list, &[Key::Char('0')]), None);
    assert_eq!(bound(&list, &[Key::Alt('l')]), Some(("list.focus-preview", 1)));
    assert_eq!(bound(&["preview"], &[Key::Alt('k')]), Some(("preview.focus-list", 0)));
    assert_eq!(bound(&list, &[Key::Char('q')]), None, "q is not a key");
}

#[test]
fn a_sequence_is_a_prefix_until_its_last_key() {
    let list = ["list", "global"];
    assert!(matches!(find(&list, &[Key::Char('g')]), Found::Prefix));
    assert!(matches!(find(&list, &[Key::Char('z')]), Found::Prefix));
    assert_eq!(bound(&list, &[Key::Char('g'), Key::Char('g')]), Some(("list.ends", 0)));
    assert_eq!(bound(&list, &[Key::Char('z'), Key::Char('c')]), Some(("list.fold", 0)));
    assert!(matches!(find(&list, &[Key::Char('g'), Key::Char('j')]), Found::Nothing));
    assert!(matches!(find(&list, &[Key::Char('1')]), Found::Bound(..)), "1-9 is a range, not the sequence 1 - 9");
}

/// The confirm's last row is every key the table does not name. Read as
/// keys it would bind the letters of "any other".
#[test]
fn any_other_is_not_a_key() {
    for c in "anyother".chars().filter(|c| !"yn".contains(*c)) {
        assert!(matches!(find(&["confirm"], &[Key::Char(c)]), Found::Nothing), "{c}");
    }
    assert_eq!(bound(&["confirm"], &[Key::Char('y')]), Some(("confirm.yes", 0)));
    assert_eq!(bound(&["confirm"], &[Key::Esc]), Some(("confirm.no", 1)));
}

#[test]
fn a_hint_is_the_binding_s_keys_and_its_word() {
    assert_eq!(hint("list.archive"), ("e", "archive"));
    assert_eq!(hint("global.back"), ("Esc", "back"));
}

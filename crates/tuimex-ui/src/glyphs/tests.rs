use std::path::Path;

use super::{Glyphs, detect};

#[test]
fn the_ascii_tier_is_the_design_s_table() {
    let js = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/tui.js")).unwrap();
    let table = js.split("const GLYPHS = {").nth(1).unwrap().split("\n};").next().unwrap();
    let quoted =
        |line: &str, key: &str| -> char { line.split(&format!("{key}: '")).nth(1).unwrap().chars().next().unwrap() };
    let rows: Vec<&str> = table.lines().filter(|l| l.contains("unicode:")).collect();
    assert!(rows.len() >= 13, "read only {} glyphs from the design", rows.len());
    for row in rows {
        let (unicode, ascii) = (quoted(row, "unicode"), quoted(row, "ascii"));
        assert_eq!(Glyphs::Ascii.map(unicode), ascii, "{row}");
        assert_eq!(Glyphs::Unicode.map(unicode), unicode);
    }
    assert_eq!(Glyphs::Ascii.map('a'), 'a');
}

#[test]
fn the_tier_comes_from_the_locale() {
    assert_eq!(detect([None, None, Some("en_GB.UTF-8")], Some("xterm")), Glyphs::Unicode);
    assert_eq!(detect([Some("C"), None, Some("en_GB.UTF-8")], Some("xterm")), Glyphs::Ascii);
    assert_eq!(detect([Some(""), Some("de_DE.utf8"), None], None), Glyphs::Unicode);
    assert_eq!(detect([None, None, None], Some("xterm")), Glyphs::Ascii);
    assert_eq!(detect([None, None, Some("en_GB.UTF-8")], Some("linux")), Glyphs::Ascii);
}

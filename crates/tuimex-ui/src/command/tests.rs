use std::path::Path;

use super::{COMMANDS, Command, choice, nearest, split, starting};

#[test]
fn a_line_is_its_command_and_the_rest() {
    assert_eq!(split("move  Old mail "), ("move", "Old mail"));
    assert_eq!(split(" trash"), ("trash", ""));
    assert_eq!(split(""), ("", ""));
}

/// PLAN §6.3: Jaro-Winkler, 0.8 or more.
#[test]
fn a_near_miss_is_offered_its_command_and_a_stranger_is_not() {
    assert_eq!(nearest("arhcive"), Some("archive"));
    assert_eq!(nearest("mesages"), Some("messages"));
    assert_eq!(nearest("emptytrash"), Some("empty-trash"));
    assert_eq!(nearest("xyzzy"), None);
}

#[test]
fn completion_is_by_prefix_whatever_the_case_in_the_table_s_order() {
    let commands = || COMMANDS.iter().map(choice);
    let names = |prefix| starting(commands(), prefix).into_iter().map(|(name, _)| name).collect::<Vec<_>>();
    assert_eq!(names("a"), ["archive", "account", "agent"], "the design's `:a Tab`");
    assert_eq!(names("RE"), ["read", "reply", "replyall"]);
    assert_eq!(names("").len(), COMMANDS.len());
}

/// ADR-002: the commands are the design's, entry for entry and in its
/// order, with what Tab offers after `:agent`.
#[test]
fn the_table_is_the_design_s_commands() {
    let js = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/tui.js")).unwrap();
    let table = js.split("const COMMANDS = [").nth(1).unwrap().split("\n];").next().unwrap();
    let quoted = |line: &str, key: &str| -> Option<String> {
        let rest = line.split(&format!("{key}: '")).nth(1)?;
        // The design escapes the one apostrophe a line holds.
        let end = (0..rest.len()).find(|i| rest[*i..].starts_with('\'') && !rest[..*i].ends_with('\\'))?;
        Some(rest[..end].replace("\\'", "'"))
    };
    let designed: Vec<String> = table
        .lines()
        .filter_map(|line| {
            let indent = line.len() - line.trim_start().len();
            Some(format!("{indent} {} | {} | {}", quoted(line, "name")?, quoted(line, "args")?, quoted(line, "does")?))
        })
        .collect();
    fn flat(commands: &[Command], indent: usize, out: &mut Vec<String>) {
        for c in commands {
            out.push(format!("{indent} {} | {} | {}", c.name, c.args, c.does));
            flat(c.choices, indent + 4, out);
        }
    }
    let mut ours = Vec::new();
    flat(COMMANDS, 2, &mut ours);
    assert!(designed.len() > 25, "read only {} lines of the design's commands", designed.len());
    assert_eq!(ours, designed);
}

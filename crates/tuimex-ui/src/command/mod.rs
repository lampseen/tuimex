//! The commands: the design's `COMMANDS` table, which completion and the
//! help are drawn from, and the nearest one to a name that is not a command.

use crate::palette::Tok;
use crate::span::{Line, run};

/// One `:` command, or one thing a command takes: its name, what follows
/// it, and the line completion and the help say of it.
#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub name: &'static str,
    pub args: &'static str,
    pub does: &'static str,
    /// What Tab offers after the name.
    pub choices: &'static [Command],
}

const fn c(name: &'static str, args: &'static str, does: &'static str) -> Command {
    Command { name, args, does, choices: &[] }
}

/// The design's `COMMANDS`, in the order completion offers them.
pub const COMMANDS: &[Command] = &[
    c("archive", "", "archive what is selected"),
    c("trash", "", "move what is selected to Trash"),
    c("flag", "", "flag or unflag what is selected"),
    c("read", "", "mark what is selected read"),
    c("unread", "", "mark what is selected unread"),
    c("move", "<folder>", "move what is selected to a folder"),
    c("section", "<name>", "switch to a section by name"),
    c("account", "[name]", "show one account only; alone, every account"),
    Command {
        name: "agent",
        args: "reply|tidy|link",
        does: "reply, tidy or link: hand text to the agent",
        choices: &[
            c("reply", "[guidance]", "write the body from the thread"),
            c("tidy", "", "clean up the body you wrote"),
            c("link", "<n>", "ask about a link; the agent opens nothing"),
        ],
    },
    c("compose", "[address]", "write a new message"),
    c("reply", "", "reply to the sender"),
    c("replyall", "", "reply to everyone"),
    c("forward", "", "forward the message"),
    c("send", "", "the send summary, then confirm"),
    c("browser", "", "open the sealed browser copy; asks before images"),
    c("save", "<n> [path]", "save an attachment to downloads"),
    c("open", "<n>", "open an attachment, after asking"),
    c("link", "<n>", "open a link, after assessment"),
    c("invite", "accept|tentative|decline|propose|ignore", "answer an invitation"),
    c("calendar", "[month|week|day]", "the calendar; alone, where it was"),
    c("mail", "", "back to mail, where it was"),
    c("goto", "<date>", "go to a date: 5 dec, fri, +2w"),
    c("event", "[title]", "a new event on the selected day"),
    c("export", "[path]", "write local events as .ics"),
    c("notify", "on|off", "reminders on or off"),
    c("unsubscribe", "", "prepare an unsubscribe for review"),
    c("preview", "auto|right|below|off", "where the preview sits"),
    c("mouse", "[on|off]", "the mouse on or off; alone, the other"),
    c("sync", "[all]", "sync this section, or all"),
    c("empty-trash", "[account]", "delete an account's Trash for good"),
    c("messages", "", "the last results and errors"),
    c("q", "", "quit from the list; back elsewhere"),
    c("qa", "", "quit"),
];

/// A command as completion draws it: its name and what it takes, then the
/// line about it.
pub fn choice(command: &Command) -> (&'static str, Line) {
    let takes = format!("{} {}", command.name, command.args);
    (command.name, vec![run(format!("{takes:<24}"), Tok::Fg), run(command.does, Tok::Dim)])
}

/// A choice that is only its name.
pub fn plain(name: &str) -> (&str, Line) {
    (name, vec![run(name, Tok::Fg)])
}

/// How alike a mistyped name must be to a command for it to be offered
/// (PLAN §6.3).
const NEAR: f64 = 0.8;

/// A line as its command and the rest.
pub fn split(line: &str) -> (&str, &str) {
    let line = line.trim();
    line.split_once(' ').map_or((line, ""), |(name, rest)| (name, rest.trim()))
}

/// The choices whose names start with `prefix`, whatever its case.
pub fn starting<'a>(choices: impl IntoIterator<Item = (&'a str, Line)>, prefix: &str) -> Vec<(String, Line)> {
    let prefix = prefix.to_lowercase();
    let starts = |name: &str| name.to_lowercase().starts_with(&prefix);
    choices.into_iter().filter(|(name, _)| starts(name)).map(|(name, note)| (name.to_string(), note)).collect()
}

pub fn nearest(typed: &str) -> Option<&'static str> {
    let scored = COMMANDS.iter().map(|c| (strsim::jaro_winkler(typed, c.name), c.name));
    scored.filter(|(score, _)| *score >= NEAR).max_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, name)| name)
}

#[cfg(test)]
mod tests;

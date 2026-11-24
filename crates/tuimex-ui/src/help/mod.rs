//! The help screen: the design's `helpScreen`. With nothing typed it is
//! every table of the keymap in three columns and then every command, as
//! one page that scrolls. Typing filters it to what matches, in one
//! column, under the headings those entries belong to.

use crate::command::{COMMANDS, Command};
use crate::glyphs::DOT;
use crate::keymap::{self, KEYMAP, Table};
use crate::layout::INDENT;
use crate::list::{heading, hint, status_row, typed};
use crate::palette::Tok;
use crate::span::{Line, cells, fit, gap, highlight, run};

/// The keys of the place help was asked from, in three columns: mail's,
/// and the calendar's. Filtering reaches every table from either.
const MAIL: [&[&str]; 3] = [
    &["global", "command", "confirm", "compose", "help"],
    &["list"],
    &["reader", "preview", "links", "attachments", "agent", "invite"],
];
const CALENDAR: [&[&str]; 3] = [
    &["global", "command", "confirm", "help"],
    &["calendar"],
    &["event", "invite", "calendars", "event-form", "links"],
];

fn has(needle: &str, texts: &[&str]) -> bool {
    texts.iter().any(|text| text.to_lowercase().contains(needle))
}

fn typed_as(command: &Command) -> String {
    format!(":{} {}", command.name, command.args)
}

fn title(text: &str) -> Line {
    vec![run(text, Tok::Fg).bold()]
}

fn entry(left: &str, does: &str, width: usize) -> Line {
    vec![run(format!("{left:<width$}"), Tok::Fg), run(does, Tok::Dim)]
}

fn column(modes: &[&str]) -> Vec<Line> {
    let tables: Vec<&Table> = modes.iter().filter_map(|mode| KEYMAP.iter().find(|t| t.mode == *mode)).collect();
    let key_width = tables.iter().flat_map(|t| t.keys).map(|b| b.keys.chars().count()).max().unwrap_or(0) + 2;
    let mut out = Vec::new();
    for table in tables {
        out.push(title(table.title));
        out.extend(table.keys.iter().map(|binding| entry(binding.keys, binding.does, key_width)));
        out.push(Line::new());
    }
    out
}

/// The whole page: the keys in three columns, then the commands.
fn page(calendar: bool) -> Vec<Line> {
    let places = if calendar { CALENDAR } else { MAIL };
    let columns: Vec<Vec<Line>> = places.iter().map(|modes| column(modes)).collect();
    let widths: Vec<usize> = columns.iter().map(|c| c.iter().map(|l| cells(l)).max().unwrap_or(0) + 2).collect();
    let depth = columns.iter().map(Vec::len).max().unwrap_or(0);
    let name_width = COMMANDS.iter().map(|c| typed_as(c).chars().count()).max().unwrap_or(0) + 3;
    let mut body: Vec<Line> = Vec::new();
    for y in 0..depth {
        let cell = |(column, width): (&Vec<Line>, &usize)| fit(column.get(y).map_or(&[][..], |l| l), *width);
        body.push(columns.iter().zip(&widths).flat_map(cell).collect());
    }
    body.push(title("Commands"));
    body.extend(COMMANDS.iter().map(|c| entry(&typed_as(c), c.does, name_width)));
    body
}

/// What `needle`, in lower case, matches, and how many entries that is:
/// each section that has a match under its heading, and a section whose
/// heading matches whole.
fn matching(needle: &str) -> (Vec<Line>, usize) {
    let keys = KEYMAP.iter().map(|t| (t.title, t.keys.iter().map(|b| (b.keys.to_string(), b.does)).collect()));
    let commands = ("Commands", COMMANDS.iter().map(|c| (typed_as(c), c.does)).collect());
    let mut sections: Vec<(&str, Vec<(String, &str)>)> = keys.chain([commands]).collect();
    for (heading, rows) in &mut sections {
        if !has(needle, &[heading]) {
            rows.retain(|(left, does)| has(needle, &[left, does]));
        }
    }
    sections.retain(|(_, rows)| !rows.is_empty());
    let rows = || sections.iter().flat_map(|(_, rows)| rows);
    let width = rows().map(|(left, _)| left.chars().count()).max().unwrap_or(0) + 3;
    let mut body = Vec::new();
    for (heading, rows) in &sections {
        body.push(highlight(heading, Some(needle), run("", Tok::Fg).bold()));
        body.extend(rows.iter().map(|(left, does)| {
            let mut line = fit(&highlight(left, Some(needle), run("", Tok::Fg)), width);
            line.extend(highlight(does, Some(needle), run("", Tok::Dim)));
            line
        }));
        body.push(Line::new());
    }
    (body, rows().count())
}

/// What `screen` drew beside the lines: how far it is scrolled, and how
/// many lines of the page show.
pub struct Drawn {
    pub lines: Vec<Line>,
    pub top: usize,
    pub room: usize,
}

/// `top` is how many lines of the page are scrolled off, `calendar` whether
/// help was asked for there, and `right` the right of the status line, as
/// on every screen.
pub fn screen(needle: &str, top: usize, calendar: bool, right: &Line, width: usize, height: usize) -> Drawn {
    let all = KEYMAP.iter().map(|t| t.keys.len()).sum::<usize>() + COMMANDS.len();
    let (body, shown) = if needle.is_empty() { (page(calendar), all) } else { matching(&needle.to_lowercase()) };
    let room = height.saturating_sub(5);
    let top = top.min(body.len().saturating_sub(room));
    let last = body.len().min(top + room);
    let indent = |line: &Line| -> Line {
        let blank = cells(line) == 0;
        if blank { Line::new() } else { fit(&[vec![gap(INDENT)], line.clone()].concat(), width) }
    };
    let mut lines = vec![heading(title("Help")), Line::new()];
    lines.extend(body[top..last].iter().map(indent));
    lines.resize(2 + room, Line::new());

    // Where the page is scrolled to is said only when it scrolls.
    let scroll = match body.len() > room {
        true => format!(" {DOT} lines {}-{last} of {}", top + 1, body.len()),
        false => String::new(),
    };
    let (left, command) = match needle.is_empty() {
        true => {
            let keys = [("type", "to filter"), keymap::hint("help.scroll"), keymap::hint("global.back")];
            (format!("{shown} keys and commands{scroll}"), hint(&keys))
        }
        false => (format!("{shown} of {all} shown{scroll}"), typed(&format!("/{needle}"))),
    };
    lines.extend([Line::new(), status_row(width, &[run(left, Tok::Dim)], right), command]);
    Drawn { lines, top, room }
}

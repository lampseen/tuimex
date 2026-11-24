//! The keys, as one table: the design's `KEYMAP`. The dispatcher looks a
//! press up here, and the help screen, every hint line and `docs/keymap.md`
//! are drawn from it, so a key cannot be bound and undocumented.

/// A key press, whatever terminal library read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Alt(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
}

/// How long the first key of `gg` or `zo` waits for the second.
pub const TIMEOUT_MS: u64 = 500;

pub struct Binding {
    /// The name a `[keys]` entry remaps.
    pub id: &'static str,
    /// The keys as the help writes them, separated by spaces.
    pub keys: &'static str,
    pub does: &'static str,
    /// The word a hint line puts after the key; empty when no hint uses it.
    pub hint: &'static str,
}

pub struct Table {
    pub mode: &'static str,
    pub title: &'static str,
    pub keys: &'static [Binding],
}

const fn b(id: &'static str, keys: &'static str, does: &'static str, hint: &'static str) -> Binding {
    Binding { id, keys, does, hint }
}

pub const KEYMAP: &[Table] = &[
    Table {
        mode: "global",
        title: "Everywhere",
        keys: &[
            b("global.command", ":", "command line", ""),
            b("global.help", "?", "help; type to filter", "help"),
            b("global.back", "Esc", "back; clear find, marks", "back"),
            b("global.section-prev", "h S-Tab", "previous section", ""),
            b("global.section-next", "l Tab", "next section", ""),
            b("global.section", "1-9", "section by position", ""),
            b("global.undo", "u", "undo the last action", "undo"),
            b("global.agent", "A", "what the agent can do", "agent"),
            b("global.redraw", "^l", "redraw", ""),
        ],
    },
    Table {
        mode: "list",
        title: "List",
        keys: &[
            b("list.move", "j k", "down, up", ""),
            b("list.ends", "gg G", "first, last row", ""),
            b("list.page", "^d ^u", "half a page down, up", ""),
            b("list.filter", "/", "find; Enter: all mail", "find"),
            b("list.match", "n N", "next, previous match", ""),
            b("list.mark", "Space", "mark the row", "mark"),
            b("list.mark-range", "V", "mark a range", ""),
            b("list.open", "Enter", "open; a draft, to edit", "open"),
            b("list.preview", "p", "show or hide preview", ""),
            b("list.preview-place", "P", "preview beside, below", ""),
            b("list.browser", "B", "sealed browser copy", "browser copy"),
            b("list.focus-preview", "Alt-j Alt-l", "focus the preview", ""),
            b("list.account", "gt gT", "next, previous account", ""),
            b("list.archive", "e", "archive", "archive"),
            b("list.trash", "d", "move to Trash", "trash"),
            b("list.flag", "s", "flag or unflag", "flag"),
            b("list.read", "m", "mark read or unread", "read"),
            b("list.move-to", "M", "move to a folder", "move"),
            b("list.compose", "c", "write a new message", "write"),
            b("list.reply", "r", "reply", "reply"),
            b("list.reply-all", "R", "reply to everyone", "reply all"),
            b("list.forward", "f", "forward", "forward"),
            b("list.unfold", "zo", "unfold a thread", "unfold"),
            b("list.fold", "zc", "fold a thread", "fold"),
        ],
    },
    Table {
        mode: "preview",
        title: "Preview, focused",
        keys: &[
            b("preview.scroll", "j k ^d ^u gg G", "scroll", "scroll"),
            b("preview.focus-list", "Alt-k Alt-h", "back to the list", "list"),
        ],
    },
    Table {
        mode: "reader",
        title: "Reader",
        keys: &[
            b("reader.scroll", "j k ^d ^u gg G", "scroll", ""),
            b("reader.thread", "n b", "thread: next, previous", "next, previous"),
            b("reader.list", "J K", "list: next, previous", ""),
            b("reader.links", "o", "links", "links"),
            b("reader.attachments", "a", "attachments", "files"),
            b("reader.invite", "i", "answer an invitation", "answer"),
            b("reader.browser", "B", "sealed browser copy", "browser copy"),
            b("reader.actions", "e d s m M r R f", "as in the list", ""),
        ],
    },
    Table {
        mode: "links",
        title: "Links (after o)",
        keys: &[
            b("links.number", "1-9", "open that link", "open"),
            b("links.choose", "j k Enter", "choose, then open", "choose"),
            b("links.close", "Esc", "close", "close"),
        ],
    },
    Table {
        mode: "attachments",
        title: "Attachments (after a)",
        keys: &[
            b("attachments.choose", "j k", "choose", "choose"),
            b("attachments.save", "Enter", "save to downloads", "save"),
            b("attachments.open", "O", "open, after asking", "open"),
            b("attachments.close", "Esc", "close", "close"),
        ],
    },
    Table {
        mode: "agent",
        title: "Agent (after A)",
        keys: &[
            b("agent.reply", "r", "write a reply", "write a reply"),
            b("agent.tidy", "t", "tidy, in compose", "tidy what I wrote"),
            b("agent.link", "l", "review a link, in links", "review this link"),
        ],
    },
    Table {
        mode: "invite",
        title: "Invitation (after i)",
        keys: &[
            b("invite.accept", "a", "accept", "accept"),
            b("invite.tentative", "t", "tentative", "tentative"),
            b("invite.decline", "d", "decline", "decline"),
        ],
    },
    Table {
        mode: "compose",
        title: "Compose",
        keys: &[
            b("compose.field", "Tab S-Tab", "next, previous field", ""),
            b("compose.identity", "Left Right", "identity, on From", "identity"),
            b("compose.editor", "Enter", "edit body in $EDITOR", "edit"),
            b("compose.attach", "^a", "attach a file", "attach"),
            b("compose.summary", "^s", "send summary", "send"),
            b("compose.save", "^d", "save the draft", "save draft"),
            b("compose.close", "Esc", "close; keep or discard", "close"),
        ],
    },
    Table {
        mode: "command",
        title: "Command line",
        keys: &[
            b("command.complete", "Tab S-Tab", "complete, cycle", ""),
            b("command.history", "Up Down", "history, by prefix", ""),
            b("command.erase", "^w ^u", "delete word, line", ""),
            b("command.run", "Enter", "run", ""),
            b("command.cancel", "Esc", "cancel", ""),
        ],
    },
    Table {
        mode: "confirm",
        title: "Confirm",
        keys: &[
            b("confirm.yes", "y Enter", "yes", "yes"),
            b("confirm.no", "n Esc", "no", "no"),
            b("confirm.other", "any other", "cancels; key dropped", ""),
        ],
    },
    Table { mode: "help", title: "Help (after ?)", keys: &[b("help.scroll", "^d ^u", "scroll", "scroll")] },
];

/// The confirm's last row describes every key the table does not name, so
/// it is not a key to look up.
const ANY_OTHER: &str = "confirm.other";

/// The presses one name of the table stands for: `gg` is two and `^d` is
/// one. `1-9` is a range, which `find` matches itself.
pub(crate) fn presses(name: &str) -> Vec<Key> {
    match name {
        "Space" => vec![Key::Char(' ')],
        "Enter" => vec![Key::Enter],
        "Esc" => vec![Key::Esc],
        "Tab" => vec![Key::Tab],
        "S-Tab" => vec![Key::BackTab],
        "Up" => vec![Key::Up],
        "Down" => vec![Key::Down],
        "Left" => vec![Key::Left],
        "Right" => vec![Key::Right],
        "1-9" => Vec::new(),
        _ => match (name.strip_prefix('^'), name.strip_prefix("Alt-")) {
            (Some(letter), _) => letter.chars().map(Key::Ctrl).collect(),
            (_, Some(letter)) => letter.chars().map(Key::Alt).collect(),
            _ => name.chars().map(Key::Char).collect(),
        },
    }
}

pub enum Found {
    /// The binding, and which of its keys was pressed, counted from 0. For
    /// `1-9` that is the digit less one.
    Bound(&'static Binding, usize),
    /// The start of a longer sequence.
    Prefix,
    Nothing,
}

/// What `typed` means in the first of `modes` that binds it.
pub fn find(modes: &[&str], typed: &[Key]) -> Found {
    let mut prefix = false;
    let tables = modes.iter().filter_map(|mode| KEYMAP.iter().find(|t| t.mode == *mode));
    for binding in tables.flat_map(|t| t.keys).filter(|b| b.id != ANY_OTHER) {
        for (i, name) in binding.keys.split(' ').enumerate() {
            if let ("1-9", [Key::Char(digit @ '1'..='9')]) = (name, typed) {
                return Found::Bound(binding, *digit as usize - '1' as usize);
            }
            let want = presses(name);
            if want == typed {
                return Found::Bound(binding, i);
            }
            prefix |= want.starts_with(typed);
        }
    }
    if prefix { Found::Prefix } else { Found::Nothing }
}

/// A binding's keys and its hint word, for a hint line.
pub fn hint(id: &str) -> (&'static str, &'static str) {
    let all = KEYMAP.iter().flat_map(|t| t.keys);
    all.filter(|b| b.id == id).map(|b| (b.keys, b.hint)).next().unwrap_or(("", ""))
}

/// `docs/keymap.md`. A test compares the file with this, and writes it when
/// `TUIMEX_BLESS` is set.
pub fn markdown() -> String {
    let mut out = String::from(
        "# Keys\n\nGenerated from the table in `crates/tuimex-ui/src/keymap/mod.rs`, which is the design's\n\
         `KEYMAP`; `make keymap` writes this file, so edit the table. The `?` screen is drawn from the same\n\
         table. A sequence such as `gg` waits 500 ms for its second key.\n",
    );
    for table in KEYMAP {
        out += &format!("\n## {}\n\n| key | does |\n|---|---|\n", table.title);
        for binding in table.keys {
            out += &format!("| `{}` | {} |\n", binding.keys, binding.does);
        }
    }
    out
}

#[cfg(test)]
mod tests;

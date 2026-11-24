use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(name = "tuimex", version, about, disable_help_subcommand = true)]
pub struct Args {
    /// Config file to read instead of $XDG_CONFIG_HOME/tuimex/config.toml.
    #[arg(long, value_name = "PATH", global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Run>,

    /// Run offline on fixture mail: no account, no network, no mail on disk.
    #[arg(long)]
    pub fixtures: bool,

    /// Glyph tier. Swaps characters only; widths never change between tiers.
    #[arg(long, value_enum)]
    pub glyphs: Option<Glyphs>,

    /// Colour tier. Detected from the terminal when omitted.
    #[arg(long, value_enum)]
    pub colors: Option<Colors>,

    /// Section to open on, by name or short name.
    #[arg(long, value_name = "NAME")]
    pub section: Option<String>,
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum Run {
    #[command(flatten)]
    Agent(Command),
    /// Bring every account's mail up to date with its server, once. For a
    /// person or a timer: it is the one subcommand that connects.
    Sync {
        /// Only this account.
        #[arg(long)]
        account: Option<String>,
    },
    /// Add a mail account, or sign in again to one: questions on the
    /// terminal, which work over SSH. Nothing is saved until the account's
    /// servers have taken the sign-in.
    Setup,
    /// Check every account end to end, and what tuimex needs of this
    /// machine. Changes nothing; exits non-zero when something is wrong.
    Doctor {
        /// Replace account names, addresses, hosts, certificate
        /// fingerprints and the home directory with placeholders, for a
        /// report that will be posted in public.
        #[arg(long)]
        redact: bool,
    },
    /// The colours tuimex draws with.
    Palette {
        #[command(subcommand)]
        action: Palette,
    },
}

// Run by a person, so the agent policy and the audit log do not cover it.
#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum Palette {
    /// Write a palette from a Ghostty or Alacritty theme file into the
    /// config directory, for `ui.palette` to name.
    Import { file: PathBuf },
}

// The agent command line (PLAN §7): each prints one JSON object and exits,
// reading the local store. There is none that sends, moves, deletes or
// flags, and `draft` has no flag that attaches a file (ADR-016).
//
// Serialised, a command is its line in the audit log: its name and the
// arguments it was given. Not a doc comment, which clap would print as
// the description of `tuimex` itself.
#[derive(Subcommand, Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "subcommand", content = "arguments", rename_all = "lowercase")]
pub enum Command {
    /// The accounts that grant an agent anything, and what each grants.
    Accounts,
    /// The folders an agent may read, with their counts.
    Folders {
        /// Only this account's.
        #[arg(long)]
        account: Option<String>,
    },
    /// Messages in the folders an agent may read, newest first.
    List {
        /// Only this account's.
        #[arg(long)]
        account: Option<String>,
        /// Only this folder's, by name.
        #[arg(long)]
        folder: Option<String>,
        /// In the grammar the sections use, such as `is:unread from:ada`.
        #[arg(long)]
        query: Option<String>,
        /// At most this many; 50 when not given.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// One message: its text as the reader shows it, its links and the
    /// names of its attachments.
    Show {
        id: String,
        /// Needed only when two accounts hold a message with this id.
        #[arg(long)]
        account: Option<String>,
    },
    /// The conversation a message belongs to, oldest first.
    Thread {
        id: String,
        /// Needed only when two accounts hold a message with this id.
        #[arg(long)]
        account: Option<String>,
    },
    /// Messages matching a query, in every folder an agent may read.
    Search {
        /// In the grammar the sections use, such as `is:unread from:ada`.
        query: String,
        /// At most this many; 50 when not given.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Write a draft from the text on standard input. Nothing is sent: a
    /// person sends it from tuimex, where it is marked as an agent's.
    Draft {
        /// The account the draft is from.
        #[arg(long)]
        account: String,
        /// A recipient's bare address; repeat the flag or separate with commas.
        #[arg(long, required = true, value_delimiter = ',')]
        to: Vec<String>,
        /// As `--to`.
        #[arg(long, value_delimiter = ',')]
        cc: Vec<String>,
        #[arg(long)]
        subject: String,
        /// The id of the message this answers, in the same account.
        #[arg(long, value_name = "ID")]
        reply_to: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum Glyphs {
    Unicode,
    Ascii,
    Nerd,
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum Colors {
    #[value(name = "16")]
    C16,
    #[value(name = "256")]
    C256,
    #[value(name = "true")]
    True,
}

#[cfg(test)]
mod tests;

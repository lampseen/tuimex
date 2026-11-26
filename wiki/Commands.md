# Commands

## On the command line

| | |
|---|---|
| `tuimex` | open the mail; with no config, on a terminal, `tuimex setup` first |
| `tuimex setup` | add an account, sign in again to one, accept a server's new certificate, or rename an account |
| `tuimex doctor [--redact]` | check every account end to end, and this machine. Changes nothing; exits non-zero when something is wrong. `--redact` prints the same report with account names, addresses, hosts, certificate fingerprints and the home directory replaced by placeholders, for posting in public |
| `tuimex sync [--account A]` | bring every account, or one, up to date with its server once, and exit |
| `tuimex palette import <file>` | write a palette from a Ghostty or Alacritty theme file, for `ui.palette` to name |
| `tuimex --version` | the version |

Options to `tuimex` itself:

| | |
|---|---|
| `--fixtures` | the design's synthetic mailbox: no account, no network, no mail on disk |
| `--section <name>` | open on a section, by its name or short name |
| `--config <path>` | read this config file. Every subcommand takes it too |
| `--glyphs unicode\|ascii\|nerd` | which characters to draw with. `nerd` draws the Unicode set |
| `--colors 16\|256\|true` | how many colours. Detected from the terminal when left out |

### For an agent or a script

Each prints one JSON object and exits, reads only mail already on this
machine, and is refused until an account grants it. [The agent](The-agent)
says what each grant covers.

| | |
|---|---|
| `tuimex accounts` | the accounts that grant an agent anything, and what each grants |
| `tuimex folders [--account A]` | the folders an agent may read, with their counts |
| `tuimex list [--account A] [--folder F] [--query Q] [--limit N]` | messages in the folders an agent may read, newest first; 50 unless `--limit` says otherwise |
| `tuimex search <query> [--limit N]` | messages matching a query, in every folder an agent may read |
| `tuimex show <id> [--account A]` | one message: its text as the reader shows it, its links, the names of its files |
| `tuimex thread <id> [--account A]` | the conversation a message belongs to, oldest first |
| `tuimex draft --account A --to ADDR --subject S [--cc ADDR] [--reply-to <id>]` | file a draft from the text on standard input. Nothing is sent |

`--account` on `show` and `thread` is needed only when two accounts hold a
message with the same id. `--to` and `--cc` take bare addresses, repeated
or separated by commas.

## Inside tuimex

`:` opens the command line. `Tab` and `S-Tab` complete commands, folder,
section and account names; `Up` and `Down` walk the history, filtered by
what you have typed; `^w` and `^u` delete a word and the line.

| | |
|---|---|
| `:archive` | archive what is selected |
| `:trash` | move what is selected to Trash |
| `:flag` | flag or unflag what is selected |
| `:read` `:unread` | mark what is selected read, or unread |
| `:move <folder>` | move what is selected to a folder |
| `:section <name>` | switch to a section by name |
| `:account [name]` | show one account only; alone, every account |
| `:compose [address]` | write a new message |
| `:reply` `:replyall` `:forward` | reply to the sender, reply to everyone, forward |
| `:send` | the send summary, then confirm |
| `:browser` | open the sealed browser copy; asks before images |
| `:save <n> [path]` | save a file to downloads, or where the path says |
| `:open <n>` | open a file in the system's program for it, after asking |
| `:link <n>` | open a link, after the check |
| `:unsubscribe` | prepare an unsubscribe for review; never automatic |
| `:agent reply [guidance]` | have the agent write the reply from the conversation |
| `:agent tidy` | have the agent clean up the text you wrote |
| `:agent link <n>` | ask the agent about a link; the agent opens nothing |
| `:preview auto\|right\|below\|off` | where the preview sits |
| `:mouse [on\|off]` | the [mouse](Keys#the-mouse) on or off for this run; alone, the other of the two |
| `:sync [all]` | sync now |
| `:empty-trash [account]` | delete an account's Trash for good, after a question with the count. `u` does not reach it |
| `:invite accept\|tentative\|decline\|propose\|ignore` | answer the invitation of the message open in the reader, or of the event selected in the calendar. `propose` then asks for the time |
| `:calendar [month\|week\|day]` | open the calendar; alone, where it was |
| `:mail` | back to mail, where it was |
| `:goto <date>` | go to a date in the calendar: `2026-12-05`, `5 dec`, `fri`, `+2w` |
| `:notify on\|off` | reminders on or off; the choice is remembered |
| `:event [title]` | the form for a new event on the selected day |
| `:export [path]` | write every local event to one `.ics` file, in the download directory or where the path says. Never over a file that is there |
| `:messages` | the last results and errors |
| `:q` | quit from the list; back elsewhere |
| `:qa` | quit from anywhere |

**What a command acts on.** The selected row, or every marked row when
there are marks. A folded thread is taken whole. A command does not take a
row's number: move the cursor or mark. In the calendar the rows of mail are
out of sight, so a command that takes them is refused there and says so.

**What it says back.** A result is in the past tense with the next key
after a dot, `✓ Trashed 3 messages · u to undo`. An error starts with `E` and names
the command, `E :move: no folder "Recipts" in work · Tab completes`. A
mistyped command offers the nearest one: `E not a command: :archvie · did
you mean :archive? Tab to fix`.

`:agent` exists only with an `[agent]` table in the config.

---

[Keys](Keys) · [Calendar](Calendar) · [The agent](The-agent) ·
[Sections and search](Sections-and-search)

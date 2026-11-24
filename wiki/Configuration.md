# Configuration

`tuimex setup` writes `~/.config/tuimex/config.toml` (or under
`$XDG_CONFIG_HOME`, on Linux and macOS alike), and editing it by hand works
as well. `tuimex --config <file>` reads another file for one run. A config
in the current directory is never read.
[`docs/config.example.toml`](https://github.com/lampseen/tuimex/blob/main/docs/config.example.toml)
shows every key, and a test parses it.

Three rules hold for the whole file:

- **A key tuimex does not know is an error**, naming the key and its line:
  ``line 2: unknown key `colour` ``. The error never repeats a value from the
  file. A file that does not parse stops tuimex; it never falls back to
  defaults.
- **No key holds a password.** A password, or a calendar's private
  address, is in the system keyring, or printed by a command you name.
- **Setup only adds to the file.** It keeps what you wrote, comments
  included, to the byte.

## `[[account]]`, once for each account

| key | holds |
|---|---|
| `name` | the short name the list shows, and `account:` matches. Two accounts may not share one. Rename with `tuimex setup`, which moves the mail, the keyring entries and the account's `[[calendar]]` with it |
| `address` | the mailbox's address |
| `identities` | other addresses you send as from this account: `["You <you@example.com>", "alias@example.com"]`. Compose offers each on From |
| `imap` | `{ host = "imap.example.com", port = 993 }`. Also `user`, where the sign-in name is not the address, and `starttls` |
| `smtp` | the same keys, for sending. Without it nothing can be sent from the account |
| `auth` | `"password"` (the keyring), `"command"`, or `"gmail-oauth"`. Left out, it is `command` when there is a `password_cmd` and `password` otherwise |
| `password_cmd` | with `auth = "command"`: the program and its arguments, `["pass", "show", "mail/work"]`. No shell reads it |
| `folders` | a role no folder plainly fills: `{ archive = "Old mail" }` |
| `color` | `link`, `head`, `ok` or `warn`: this account's colour in the list, in place of the one it would take in turn |

`starttls` says how TLS begins, never whether: `false` is TLS from the
first byte, `true` is STARTTLS on a plain connection that is abandoned if
the server will not upgrade. Left out, ports 993 and 465 are the first and
any other port the second. There is no setting that turns certificate
checking off.

### `[account.agent]`, under an account

| key | holds |
|---|---|
| `read` | `true` lets the `tuimex` subcommands read this account for an outside agent |
| `folders` | the folders `read` covers; `["INBOX"]` when left out |
| `draft` | `true` lets an agent file drafts |

Absent, the account grants nothing. See [The agent](The-agent).

## `[[calendar]]`, at most once for each account

An account's calendar, which `gc` shows beside every other account's
([Calendar](Calendar)), `tuimex sync` reads and `tuimex doctor` names. It
is a table of its own, after the accounts, because setup only adds to
the end of the file. An account with no table still has a calendar:
its local one, which tuimex alone holds.

| key | holds |
|---|---|
| `account` | the `name` of an `[[account]]`. One that is not there is an error naming it |
| `kind` | `"caldav"` or `"feed"` |
| `url` | `caldav` only, and required for it: the calendar's address, `https://` with no user name in it. For Gmail, `https://www.google.com/calendar/dav/<address>/events/`. `http://127.0.0.1:<port>/` is taken too, for a server on this machine |
| `write` | `caldav` only: `true` has tuimex write events at the provider. Left out, it is `false` |
| `address_cmd` | `feed` only: a program and its arguments that print the feed's address, `["pass", "show", "calendar/home"]`. No shell reads it, and the first line printed is the address |

```toml
[[calendar]]
account = "work"
kind = "caldav"
url = "https://www.google.com/calendar/dav/you@example.com/events/"
```

**CalDAV** signs in as the account's mail does: with its IMAP user name
and the same password, from the keyring or the account's `password_cmd`.
There is no second password. An account with `auth = "gmail-oauth"`
cannot use it; a feed is the way for that account.

**`write`** is false until you or `tuimex setup` say otherwise, because
writing to a provider is a path that has to be tried before it is
trusted, and writing to Google's calendar has never been tried by
anyone here ([What is proven](What-is-proven)). While it is false the
calendar is read and never written: an answer, or an event you make for
that account, is kept in the account's local calendar, exactly as for a
feed. `tuimex setup` offers to find out: it adds one event named
"tuimex write test" a year from today, removes it again, and sets the
key when both worked. For a calendar that was already in the file it
does that by adding a second, short table, since it only ever adds to
the end of the file:

```toml
[[calendar]]
account = "work"
write = true
```

A table with no `kind` adds to the account's table above it and holds
`write` alone. By hand, `write = true` in the calendar's own table says
the same.

**A feed** is a calendar's private address, which serves it in iCalendar
format. Anyone holding the address can read the calendar, so it is never
in the config: a `url` under a feed is an error, and the error does not
repeat it. The address is in the keyring under `<account>/calendar-feed`,
where `tuimex setup` puts it, or printed by `address_cmd`. A feed is
read-only.

`tuimex sync` asks a CalDAV calendar for the events from 90 days ago to
two years ahead. Nothing a calendar names, such as a link in an event's
notes, is fetched.

## `[sync]`

| key | default | holds |
|---|---|---|
| `bodies` | `"30d"` | messages newer than this have their text downloaded ahead of time. Older ones arrive when opened |
| `max_body` | `"10MB"` | a message larger than this arrives when opened, not before. `KB`, `MB` or `GB` |
| `folders` | `[]` | folders to sync besides every folder with a role |

## `[send]`

| key | default | holds |
|---|---|---|
| `delay` | `"10s"` | how long a message waits after `y`, in which `u` still stops it. `s` or `m` |

## `[ui]`

| key | default | holds |
|---|---|---|
| `preview` | `"auto"` | `auto`, `right`, `below` or `off`. `auto` is beside the list from 160 columns, else below |
| `palette` | | a palette file to draw with; see below |

`glyphs` and `colors` are accepted here and not read yet: use the
`--glyphs` and `--colors` options, or let tuimex detect them. See
[Where it runs](Where-it-runs).

## `[paths]`

| key | holds |
|---|---|
| `downloads` | where a file is saved. The system's download directory when left out |

## `[[section]]`, once for each tab

`name`, `short`, `query`, and optionally `threaded`. See
[Sections and search](Sections-and-search).

## `[agent]`

| key | holds |
|---|---|
| `use` | `"claude"`: the Claude CLI on your `PATH`, with arguments tuimex supplies |
| `command` | in place of `use`: any other program, as its words. No shell reads them |
| `timeout` | how long it may run: `"90s"`, `"2m"`. 120 seconds when left out |

Absent, there is no `:agent`. See [The agent](The-agent).

## `[notify]`

An event's reminder, while tuimex is open. Reminders are not yet joined
to the interface, so for now these keys are read and checked and nothing
is raised.

| key | default | holds |
|---|---|---|
| `events` | `true` | whether reminders start on. Once they are switched off or on, that choice is kept in the file `reminders` under the state directory (`~/.local/state/tuimex`), and it wins over this key |
| `lead` | `"10m"` | how long before its start an event with no alarm of its own is reminded. `s`, `m` or `h` |
| `via` | `"desktop"` | `desktop`: `notify-send`, or `osascript` on macOS. `osc9`: the terminal's own notification, for a terminal that shows one. `none`: neither |

A word `via` does not take, or a `lead` in another unit, is an error
naming the key. Only the event's title is sent, never its guests, place,
link or notes, with its control characters gone and cut to 120
characters. A title can still show on a locked screen.

## Accepted, and not read yet

`[notify]`'s `bell` and `osc9`, `[pgp]` (`sign`, `key`) and `[keys]` parse,
so a file that has them is not refused, and nothing reads them: new-mail
notifications, OpenPGP and remapping keys are not built.

## Your own palette

One palette ships. Any other is a file you supply, of up to twelve
colours, one per token, each as `name = "#rrggbb"`:

| token | colours |
|---|---|
| `bg` `sel` | the background, the selection |
| `fg` `dim` `faint` | text, quieter text, the quietest |
| `acc` | the accent |
| `ok` `warn` `err` | the mark of a result, of a notice, of an error |
| `agent` | the mark of whatever an agent wrote or said |
| `link` `head` | two more accents; with `ok` and `warn`, the colours accounts take in the list |

A token left out keeps the built-in colour. Name the file in the config:

```toml
[ui]
palette = "palette.toml"
```

A path that does not begin `~/` is read against the config directory. The
file's colours are used where the terminal shows truecolor; in 256 and 16
colours tuimex draws with the built-in tiers.

### From your terminal's theme

```sh
tuimex palette import ~/.config/ghostty/themes/mine
```

reads a Ghostty or Alacritty theme file and writes
`~/.config/tuimex/palette.toml` from it, then prints the two lines to add
to the config. It says which tokens the theme had no colour for; each of
those keeps the built-in one. A palette already there is left alone, since
you may have edited it: move it aside to import another.

This is also the way to use a theme whose licence forbids copying it into
other software: the import reads the
copy you hold and writes to your own config, and no colour passes through
anything tuimex distributes.

With a palette loaded, tuimex works out which of its accents to give the
accounts so that they stay furthest apart, for ordinary sight and for the
two common kinds of colour blindness. `color` under an account overrides
that for one account.

---

[Setup and accounts](Setup-and-accounts) ·
[Sections and search](Sections-and-search) · [Where it runs](Where-it-runs)

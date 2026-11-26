# Where it runs

Supported here means that a run proved it, not that it should work. Where
nothing has, this page says so.

## Systems

**Linux** is where tuimex is written and used, and where its whole test
suite has passed. The CI job `check` runs that suite on every change, and
the job `release-build` builds the release binary on the oldest runner
image and starts it.

**macOS** is a claim the CI job `macos` makes, and nothing else does: it
runs the whole suite on a Mac and builds the release binary there. The
suite has passed on `macos-latest` in CI as of 2026-11-24, and a green
`macos` on a commit is the evidence for that commit. Nobody has used
tuimex interactively on a Mac; a report is welcome. The paths are the same
as on Linux (`~/.config/tuimex` and the rest), not `~/Library`.

**Windows** is not supported and nothing is built for it. Mail directory
names, the keyring and the paths would each need a second branch.

Release archives are built for x86_64 and arm64 on both systems: the
release workflow built, attested and published all four for the first
release on 2026-11-24. The Linux x86_64 one has been downloaded, checked
and started by hand. Neither Mac archive has been unpacked or run by
anyone, and the Linux arm64 one has not been run on an arm64 machine. See
[Installing](Installing).

## Terminals

Any terminal that understands the usual escape codes. tuimex degrades
honestly: sixteen colours and plain ASCII carry the same information as
truecolor and Unicode, in the same columns.

| | how tuimex decides | to override |
|---|---|---|
| **colours** | truecolor when `COLORTERM` is `truecolor` or `24bit`; else 256 when `TERM` contains `256color`; else sixteen | `--colors true`, `256` or `16` |
| **glyphs** | Unicode when the locale (`LC_ALL`, `LC_CTYPE` or `LANG`) is UTF-8; ASCII otherwise, and on the Linux console | `--glyphs unicode` or `ascii` |
| **pictures** | drawn where the terminal has the kitty, sixel or iTerm2 image protocol; elsewhere a placeholder names the picture | |

Over SSH, `COLORTERM` is often not passed on, so a truecolor terminal may
get 256 colours; `--colors true` says otherwise.

Each frame is drawn inside one synchronised update, which stops flicker on
terminals that support it and is ignored by the rest. Resizing redraws at
the new size. A crash puts the terminal back before its message prints.

`Alt-j` and its neighbours are read as Alt with the letter, or as `Esc`
and the letter arriving together, whichever the terminal sends.

A reminder of an event rings the terminal's bell and, by `notify.via`,
runs `notify-send` on Linux or `osascript` on macOS, or writes an OSC 9
sequence for a terminal that shows one. No test runs either program, and
neither has been seen to raise a notification by anyone here.

The mouse is read only once `:mouse` or `ui.mouse = true` asks for it, and
so far in mail alone, not in the calendar ([Keys](Keys#the-mouse)). Presses and the wheel are all
that is asked of the terminal, and that is given back on the way out,
around `$EDITOR` and in a crash. Nobody has tried it inside tmux or on
macOS.

Not built: `NO_COLOR`, and notifications of new mail.

The screens are compared with the design cell for cell in two renderings:
truecolor with Unicode, and sixteen colours with ASCII. The 256-colour tier
is not compared screen by screen.

## Mail providers

tuimex speaks IMAP and SMTP, so any provider that offers both should work,
and "should" is the honest word for most of them.

| | what has been run |
|---|---|
| **an IMAP and SMTP server in a container** | `make real-server`, which the CI job `real-server` runs on every change: GreenMail and Dovecot, with sync, new mail arriving by itself, archive, trash, undo, sending, a pinned certificate, and the interface driven on a terminal |
| **Gmail**, app password or browser sign-in | two accounts were set up and synced once, by hand, on 2026-11-23. Which way each signed in was not recorded, and sending was not. The browser sign-in is tested against a stand-in for Google |
| **Proton Bridge** | one account was set up and synced once, by hand, on 2026-11-23. Certificate pinning is tested against a container's certificate |
| **Fastmail, iCloud** | setup knows their servers. Nobody has run them |
| **anything else** | hosts and ports typed by hand in setup. How TLS begins on a port is found by trying |
| **Microsoft 365** sign-in, **JMAP** | not built |

## Calendars

| | what has been run |
|---|---|
| **a CalDAV server in a container** | `make real-server`: Radicale, with an event written, changed, answered and deleted through the engine. A further leg answers an invitation in the interface on a terminal and reads the event back from Radicale and the reply from the mail server; it passed on 2026-11-24 and runs in the CI job `real-server` |
| **Google**, over CalDAV with an app password | two accounts' calendars, one of them on a Google-hosted domain, were set up and read by hand on 2026-11-24, and their owner said the calendar looked right; it was not compared event by event. The same day setup's write check passed on both, and an event made in tuimex was seen in Google Calendar and, deleted there, went from tuimex at the next `:sync`. Changing an event, answering an invitation and deleting from tuimex have not been tried there |
| **Proton** and any other feed | read from the private address. One Proton calendar was set up and read by hand on 2026-11-24, with the same caveat. No feed from Google has been read |

The by-hand script for real providers is
[`docs/testing.md`](https://github.com/lampseen/tuimex/blob/main/docs/testing.md),
with its record of runs at the end. If you run it, a row for that table is
a welcome pull request.

## A keyring

Passwords are kept in the Secret Service on Linux and the login keychain on
macOS. The tests use a keyring in memory and never the system's, so the
system keyring is exercised only by the by-hand runs above. Without one,
use a command that prints the password.

---

[What is proven](What-is-proven) · [Installing](Installing) ·
[Troubleshooting](Troubleshooting)

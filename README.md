# tuimex

A terminal mail client for clearing an inbox across several accounts, where
nothing a message contains can act for you or report on you.

```sh
tuimex
```

```
  Inbox 9   Unread   Flagged 3   Drafts   Sent   Archive   Outbox 1

  Today
    work  Imani Castell       Invitation: Q4 roadmap re~         inv   14:05
    pers  Jonas Becker      > Cabin weekend: who is dri~          5m   13:40
  + work  Backup Robot        [ops] Nightly backup finished with 2 ~   12:58
>   work  Priya Raman       > Contract draft v3 for the~       3m 2f   11:32 !
    pers  Fernway Weekly      This week: winter routes, a gear swap~   10:15
    work  Lars Lindqvist      Staging database migratio~          re   09:48
    pers  Example Bank        Action needed: confirm your new device   09:02
    work  Mei Tanaka          Payroll corrections for November         08:30 !

  Yesterday
    pers  Greta Holm          Photos from Saturday                4f   20:44
  + pers  Parcel Desk         Your parcel is out for delivery          16:20
    work  Olu Adeyemi         New keys for the shared vault            11:05

  Past 7 days
    work  Imani Castell       Budget numbers for the of~          re  21 Nov
    pers  Tomas Ferreira      Bike rack: still want it?               21 Nov
    work  Hiring Desk         Interview loop: backend c~          1f  21 Nov

  4 of 24 - 2 marked                                sending in 6s - 1 not sent
  e archive - d trash - M move - Esc clear marks - ? help
```

tuimex is short for *tui mail exchange*. The screen above is the list at
80 columns, in the plain-ASCII, sixteen-colour rendering every terminal
can show; a test compares it with what tuimex draws.

## What it does

tuimex puts the mail of every account in one list and lets you clear it
from the keyboard. Each message is dealt with in one key: `e` archives,
`d` trashes, `r` replies, `s` flags. Nothing asks you to confirm, because
`u` takes the last action back, on the server too. Mark rows and the next
key takes all of them.

Mail is read as text. A message cannot load a picture, run a script, open
a link or tell anyone you read it: tuimex has no code that fetches what a
message names. A link shows the host it really opens, and one that
deceives is asked about first.
[Your first session](https://github.com/lampseen/tuimex/wiki/Your-first-session).

## Install

Every [release](https://github.com/lampseen/tuimex/releases) carries an
archive for Linux and for macOS, each for x86_64 and arm64, named
`tuimex_<os>_<arch>.tar.gz` (`tuimex_linux_amd64.tar.gz`,
`tuimex_darwin_arm64.tar.gz`) and attested by the workflow that built it.
Download the one for your system from
`https://github.com/lampseen/tuimex/releases/latest/download/`, check it
with `gh attestation verify <archive> --repo lampseen/tuimex`, and put
`tuimex` somewhere on your `PATH`. The binary is not signed with an Apple
Developer ID, so macOS may warn the first time;
[Installing](https://github.com/lampseen/tuimex/wiki/Installing) says what
to do.

From a clone, with Rust installed, `make install-binary` builds it into
`~/.local/bin/tuimex`. A Homebrew tap is to follow.
[Installing](https://github.com/lampseen/tuimex/wiki/Installing).

## Setting it up

With no settings yet, `tuimex` starts `tuimex setup` first. It asks for an
address, who provides the mailbox, and how to sign in, and saves nothing
until the account's servers have taken the sign-in. Run it again to add
another account. A password goes to the system keyring and never to a
file. Gmail signs in with an app password, or through a browser with a
client you register yourself; Proton is reached through Proton Bridge,
whose certificate you accept once.

`tuimex doctor` checks every account end to end and says what to fix.
[Setup and accounts](https://github.com/lampseen/tuimex/wiki/Setup-and-accounts) ·
[configuration](https://github.com/lampseen/tuimex/wiki/Configuration)

## Reading and triage

The tabs are sections, each a saved query: `in:inbox is:unread`,
`has:attachment from:ada after:7d`. `/` finds among the rows in front of
you, and Enter then searches all mail, in every account. Conversations
fold into one row. The preview shows the whole message beside or below
the list.

`Enter` opens a message. `o` lists its links with where each really goes,
`a` its files, and `B` opens the message as its author styled it, in your
browser, from a sealed copy that can run nothing and load nothing unless
you say yes to its pictures.
[Reading mail](https://github.com/lampseen/tuimex/wiki/Reading-mail) ·
[links and the browser copy](https://github.com/lampseen/tuimex/wiki/Links-and-the-browser-copy) ·
[attachments](https://github.com/lampseen/tuimex/wiki/Attachments) ·
[sections and search](https://github.com/lampseen/tuimex/wiki/Sections-and-search) ·
[every key](https://github.com/lampseen/tuimex/wiki/Keys) · [all commands](https://github.com/lampseen/tuimex/wiki/Commands)

## Writing and sending

`c` writes, `r` replies, `f` forwards, in your own `$EDITOR`. Choose who
it is from on the From line, and it goes out through that account. Before
anything leaves, one screen lists every recipient and marks any whose
domain is nowhere in the conversation; only `y` sends, and for ten seconds
after it `u` still stops the message.
[Writing and sending](https://github.com/lampseen/tuimex/wiki/Writing-and-sending).

## The agent

Two things, both off until you turn them on. Inside tuimex, `A` hands a
conversation to a command you configured, with names, addresses and
numbers replaced by placeholders, to write a reply, tidy your text or
advise on a link; the agent never opens the link. From outside, an agent
of yours can call `tuimex list`, `show`, `search` and `draft` for the
folders you granted. It sees exactly the text you see, and there is no
subcommand that sends, moves, deletes or flags.
[The agent](https://github.com/lampseen/tuimex/wiki/The-agent).

## What it touches

```
~/.config/tuimex/        your settings, a pinned certificate, a palette
~/.local/share/tuimex/   your mail, one Maildir for each account
~/.cache/tuimex/         the index and sealed copies, which can be rebuilt
~/.local/state/tuimex/   its log, and the record of every agent call
```

Nothing else, and every file in them is readable by you alone. Passwords
and tokens are in the system keyring, or come from a command you name;
none reaches a file or the log. tuimex connects to your mail servers over
TLS that is always verified, and fetches nothing a message points at: no
remote image, no tracking pixel, no stylesheet, no link preview. These
rules are a policy the tests hold, and the page says where they stop.
[Security and privacy](https://github.com/lampseen/tuimex/wiki/Security-and-privacy).

## Where it runs

Linux and macOS, in a terminal, down to sixteen colours and plain ASCII.
`make real-server` runs the tests against two mail servers in containers,
and the CI job of that name runs it on every change. Real Gmail and Proton
accounts have been set up and synced once by hand, and no more than that
is recorded. macOS is what its CI job says and nothing more: nobody has
used tuimex on a Mac by hand.
[Where it runs](https://github.com/lampseen/tuimex/wiki/Where-it-runs) ·
[what is proven, and by which test](https://github.com/lampseen/tuimex/wiki/What-is-proven) ·
[troubleshooting](https://github.com/lampseen/tuimex/wiki/Troubleshooting)

## Scope

v1 leaves out, on purpose: Windows, Microsoft 365 sign-in and JMAP,
composing HTML, remote content inside tuimex, search on the server,
scripting and plugins, S/MIME, and a settings screen. OpenPGP and
answering invitations are planned and not built.

## Contributing

The aims are in [`docs/north-star.md`](docs/north-star.md), the reasoning
behind each decision in [`docs/decisions.md`](docs/decisions.md), and the
plan in [`docs/PLAN.md`](docs/PLAN.md). The rules the code is written to
are in [`CLAUDE.md`](CLAUDE.md), and [`CONTRIBUTING.md`](CONTRIBUTING.md)
says where everything else is. A suspected security hole goes through
[`SECURITY.md`](SECURITY.md).

## Authorship

tuimex was built in the shape of [bothy](https://github.com/lampseen/bothy)
and [pullpen](https://github.com/lampseen/pullpen): decisions recorded and
numbered, the code held to budgets that fail the build, and what the
documentation claims listed beside the test that proves it.

The decisions are the maintainer's; Claude planned and implemented within
them. The design (`design/`) was drafted by Claude and accepted by the
maintainer, and the tests compare the screens against it cell for cell.

## Credits and licence

MIT, see [`LICENSE`](LICENSE). The palette is the design's own.

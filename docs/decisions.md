# Decisions

Why tuimex is the way it is. A decision recorded here is settled; proposing
it again means answering the record by number, not starting the argument over.
A record that is overturned stays, with a later record saying what replaced it.

Records ADR-007 to ADR-018 each answer something found in the two mail
clients reviewed before this project began. The review is
[`history/2026-11-23-prior-art.md`](history/2026-11-23-prior-art.md), as
ADR-052 allows it to be published.

---

## ADR-001 — The core is Rust, in a workspace whose crates are the boundaries

tuimex follows pullpen's ADR-001 for pullpen's reasons: the screen is drawn
cell by cell against a design specified cell by cell, startup has to be
instant, and it ships as one binary.

The workspace is not a filing system. Each rule that must hold is a crate
that cannot break it: `tuimex-ui` depends on `tuimex-core` alone, so it
cannot open a socket or a file; `tuimex-content` depends on nothing that does
I/O, so parsing a message cannot reach the network (ADR-007). A rule held by
the dependency graph does not need a reviewer to notice it.

**What it costs.** Seven crates for one binary, and types that cross a boundary
have to live in `tuimex-core`. Compile times are longer than Go's.

## ADR-002 — The design artifact is the visual spec, and the goldens are its proof

As pullpen's ADR-002. `design/` is the design exactly as Bryan delivers it.
`design/tui.js` renders every screen from fixture data without a browser,
`scripts/gen-golden.mjs` writes `tests/golden/` from it, and the Rust
renderer is compared with those files byte for byte. A change to the look
starts in `tui.js`.

The design does not exist yet, and the plan does not wait for it. Milestones
M1 to M5 (content, store, engine, agent command line) draw nothing and are
built first. M6 is the first that needs `design/tui.js`, and it does not
start without it: no screen is invented in Rust and reconciled later.
[The brief](history/2026-11-23-design-brief.md) lists what the artifact has to show.

**What it costs.** Five milestones of work with no screen to look at, proven
only by tests and the command line.

## ADR-003 — One palette ships; every other one is a file you supply

As pullpen's ADR-003, in the same shape: the design's tokens in truecolor,
256-colour and 16-colour tiers are the only palette in the repository,
`ui.palette` points at a TOML file for any other, and a test fails on any
colour value in a shipped file that is not the built-in palette's.

## ADR-004 — The budgets, and why they are failing checks

`make check` fails when any of these is crossed:

- the stripped release binary is larger than 16 MiB;
- the code is longer than 14,000 lines;
- comments are more than 30% of the code;
- the prose is more than 75% of the code, once the code passes 2,000 lines.

The numbers live in the Makefile; a test fails if a document quotes them
differently. Code is the non-test Rust under `crates/*/src`, which a filename
filter can find because unit tests live in `tests.rs`.

A mail client grows by accretion: one more header, one more provider quirk.
The cap is what makes each addition argue for itself. It is set below the
estimate for everything v1 names, so the last milestones have to pay for
their lines by finding some to remove, or by a record that says why not.

The binary figure is an estimate made before anything was built. M0 measures
the scaffold with every dependency in ADR-005 linked, and a record replaces
the figure with the measurement plus headroom.

**What it costs.** Records whose only content is a number going up, as
pullpen has five of.

*2026-11-23: the code figure is ADR-042's now.*

*2026-11-24: and the binary figure is ADR-049's.*

*2026-11-24: the code figure is ADR-059's.*

*2026-11-24: and then ADR-063's; the binary figure is ADR-064's.*

## ADR-005 — The dependency ceiling is a list

The crates tuimex may depend on are the list in PLAN §1. Adding one takes a
record that says what it replaces and what it costs in binary size.

Mail pulls in dependencies faster than most software, because every format
has a crate. Each one is code that parses hostile input on the user's
machine. The list is short on purpose and each entry has a job no other
entry does.

## ADR-006 — No async runtime; threads and channels

Every I/O library tuimex needs has a blocking form: the IMAP client, the
SMTP client, SQLite, the token request. The engine runs one thread per
server connection and one for the store, and they talk to the UI over
channels. The UI thread reads terminal events and channel messages and never
waits on anything else.

This is the opposite of a client that runs an async runtime and then calls
`block_on` from the UI thread, which the review saw. A runtime does not keep
an interface responsive; a rule does. Here the rule is the crate graph
(ADR-001): the UI has no handle on anything that blocks.

**What it costs.** A connection per folder being watched means a thread per
folder being watched; PLAN §4.3 caps how many. Cancelling a blocked read
means closing its socket from another thread.

## ADR-007 — Message bytes are parsed in one crate, which can do no I/O

`tuimex-content` is the only crate that parses a message: MIME, headers,
HTML, calendar parts, signatures' structure. What it returns is plain data
(text, a list of links, a list of attachments by name and size). It has no
dependency that opens a file or a socket, and a test scans its source for
`std::fs`, `std::net` and `std::process` and fails on any.

Both reviewed clients parsed mail wherever it was convenient, which puts
code that sanitises HTML a few calls from code that opens a URL. Keeping
the parser where it cannot act means a parsing bug can produce wrong text
and nothing worse.

**What it costs.** Anything that needs both a parsed message and the disk
(saving an attachment, verifying a signature with `gpg`) is two steps in two
crates.

## ADR-008 — tuimex fetches nothing a message names

No image, stylesheet, font, preview or tracking pixel is requested because a
message referred to it. There is no setting that turns this on and no
per-sender exception in v1.

A blocker that matches strings is walked through by `src=" https://…"`,
`HTTPS://`, `@import` and a CSS escape. A client that blocks by default and
does it properly still has a fetch, and a fetch that does not check where
it is going will request `http://localhost` or a LAN address for whoever
wrote the message. Having no fetch removes both failures and the code that
would need defending.

Pictures attached to the message, including ones its HTML shows inline, are
drawn through the terminal's image protocol where it has one. They are bytes
already on disk.

**What it costs.** Mail that keeps its pictures on a server shows alt text.
The browser copy (ADR-009) can load them, after a prompt that names every
host it would contact.

## ADR-009 — HTML is read as text; the browser copy is sanitised and sealed

The reader shows HTML mail as text: block structure, emphasis, lists, quotes,
tables as rows, links as numbered references. Elements a browser would not
display (`display:none`, `visibility:hidden`, zero size, `hidden`) are
dropped, so the text is what the author meant a reader to see (ADR-017).

`:browser` writes a copy to a private directory (ADR-011) and opens it. The
copy has been through the sanitiser (no script, no event handler, no form,
no frame, no `meta refresh`, no `base`), and begins with a
`Content-Security-Policy` of `default-src 'none'; img-src data:; style-src
'unsafe-inline'`. Attached images are inlined as `data:` URLs.
`:browser remote` asks first, naming the hosts, and widens `img-src` to
those hosts over https alone.

*2026-11-23, after use on real mail: the question is asked each time.* Bryan
chose, of four options, that `B` and `:browser` ask whenever the message has
pictures on a server (`Load 9 images from <hosts>?`), and that there is no
`:browser remote`. A yes widens `img-src` exactly as above; a no opens the
copy sealed; a message with no such picture opens at once. Nothing is
remembered per sender or per message. Where the row is too narrow for every
host the question names the first and counts the rest. Inside tuimex nothing
is fetched either way (ADR-008).

*2026-11-24: a yes to pictures loads no tracker.* The question counted
every `<img>` on another host, so on a message whose one remote image was
a 1×1 pixel it asked to "load 1 image" from the tracking service, and a
yes reported the reading. An image is now left out of the question, the
count and the copy when it is one the reader's notice already names as a
tracker: on a known tracking service's host, a pixel or less across, or
hidden by an attribute, a style or the element it is in. `img-src` is
widened host by host, so a host that serves both a picture and a pixel
is allowed and the pixel's address is taken out of the copy, as every
tracker's is whichever host it is on. A message whose only remote images
are trackers is not asked about and opens sealed. Nothing new is said:
the reader's notice names the tracker, the line after a yes counts what
loads, and the copy's own line counts what did not. Held by
`a_yes_to_the_pictures_loads_no_tracker_and_a_message_with_only_trackers_is_not_asked`
and the corpus cases named `tracker-…`.

A browser engine used to draw mail brings a browser's attack surface to
every message selected. A client that writes a message's HTML to a file as
it came and opens it runs the sender's script in the reader's real browser
from a `file://` origin. A test holds a corpus of hostile messages and
asserts what the copy may contain.

**What it costs.** Layout is lost in the reader. A layout engine in
terminal cells is thousands of lines that would need their own goldens; it
is ruled out (ADR-022) rather than deferred.

## ADR-010 — A secret reaches no file tuimex writes

A password or refresh token is kept in the system keyring, or produced each
time by a `password_cmd` the user configures. The config file has no field
that holds one. In memory a secret lives in a type that zeroes itself and
prints as `[redacted]`.

A test gives tuimex a known secret, exercises setup, sync, send, the agent
command line and a failure of each, and then searches every file under the
scratch root and everything printed for that string. CI runs it as a named
job that fails if the test did not run, as pullpen's token job does.

The failure this answers is a password in plain text in a config file
created with the default mode.

**What it costs.** A machine with no keyring service (a bare server, some
containers) needs `password_cmd`, and setup has to say so clearly.

## ADR-011 — Files are owner-only, created new, in one of four places

Every file tuimex creates is mode 0600 in a directory that is 0700, opened
with a call that fails if the path exists. They live in exactly these places:
the config directory, the data directory (mail), the cache directory (the
index, browser copies, opened attachments), and the state directory (logs).
The system temporary directory is never used. Browser copies and opened
attachments are removed after a day and at exit.

The config is read from `$XDG_CONFIG_HOME/tuimex/config.toml` or the path
given to `--config`. The current directory is never searched: a client
that reads a config from there first lets a cloned repository carry one
that points it at another server or another database.

One module in `tuimex-store` creates files, and a test fails on any other
call to `File::create`, `fs::write` or `OpenOptions` in the workspace.

## ADR-012 — TLS is always verified; a self-signed server is pinned, not waved through

Every connection is TLS (implicit, or STARTTLS that fails closed), and the
certificate is always checked. There is no `skip_tls_verify`.

A server with its own certificate, Proton Bridge being the common one, is
handled by `tuimex setup`: it shows the certificate's SHA-256 fingerprint,
and on acceptance saves the certificate beside the config as a trust root
for that account alone. A changed certificate is then an error that names
both fingerprints.

**What it costs.** Bridge users answer one more question at setup, and again
whenever Bridge regenerates its certificate.

## ADR-013 — The Maildir is the mail; the index is a cache

Each account's mail is a Maildir++ tree under the data directory: plain
files that mutt, notmuch and a backup tool can read. SQLite holds the index:
envelopes, threads, flags, the search index, sync positions. Deleting the
index loses nothing; it is rebuilt from the Maildir, and a test does exactly
that and compares.

For that to hold, everything needed to resume sync is in the Maildir: the
UID rides in the file name (`,U=<uid>`, as mbsync does), and each folder
keeps its UIDVALIDITY in a small file.

Mail kept only in a database is opaque to every other tool. A Maildir
alone has to be indexed again by every process that opens it, which an
agent command line reading the store many times a minute cannot afford.

**What it costs.** Two representations to keep in step, and file names that
change when a flag does.

## ADR-014 — What sync may never do

- A folder whose UIDVALIDITY changed is synced again from nothing. Its old
  UIDs are never used again.
- Messages are removed from a server with `UID EXPUNGE` naming them, or
  `UID MOVE`. A bare `EXPUNGE` is never sent: it removes every message
  anyone marked deleted, not the ones tuimex meant. A server without
  UIDPLUS or MOVE gets the message copied and flagged `\Deleted`, and the
  status line says the server will purge it.
- A change made here is written to the account's journal before it is
  applied locally, and leaves the journal when the server confirms it. On
  reconnect the journal is replayed. A change the server refuses is undone
  locally and reported.
- A flag changed here and on the server since the last sync resolves to the
  server's value, except that `\Seen` set on either side stays set.

The review saw the absence of each: a UIDVALIDITY never read from the
server, a bare `EXPUNGE` for both delete and move, and a flag change that
never reached the server at all.

## ADR-015 — Undo instead of confirm; sending is held and summarised

Archive, trash, move, flag and mark take effect at once and `u` reverses the
last of them, including after the server has applied it. Delete means move to
Trash; emptying Trash is a command that confirms.

Sending cannot be undone, so it is the one thing confirmed: a summary shows
every recipient, the sending identity, each attachment with its size, and
whether the message will be signed or encrypted. After confirmation the
message waits in the outbox for `send.delay` (ten seconds by default), during
which `u` returns it to drafts.

Addresses are checked before the summary. One that does not parse is an
error naming it; dropping it and sending to the rest is the failure this
answers.

## ADR-016 — An agent may read what it is granted and write drafts; nothing else

The command line that scripts and agents use is off until an account grants
it. There are two grants, `read` (with the folders it covers) and `draft`.
There is no grant for sending, moving, deleting or flagging, and no code
path from the command line to any of them.

A draft made this way is marked as such. It cannot carry an attachment, the
reader shows who wrote it, and it leaves only through the send summary
(ADR-015), in front of a person. Every call is appended to an audit log with
its arguments and outcome.

The policy is one pure function in `tuimex-core`, `allows(grants, request)`,
with a table test that lists every request against every grant. The
failure it answers is a permission check that returns `true`.

**What it costs.** An agent cannot archive what it has dealt with. That is a
later record, once the read and draft paths have been used for a while.
North star says what the policy is not: a sandbox.

## ADR-017 — What the agent reads is what the reader shows

The text the command line returns for a message is the output of the same
function the reader draws, including the removal of hidden elements
(ADR-009). There is no separate "body for machines". A test asserts the two
are the same call.

Everything that came from a message is returned under a key named
`untrusted`, apart from what tuimex itself assigned (ids, dates as parsed,
flags, folder). The raw message is not available.

Hidden text is how a mail speaks to an agent without its owner seeing. If
the two always see the same words, a person reading the message can see
what the agent was told.

## ADR-018 — Only web and mail links open, and the destination is always shown

`http`, `https` and `mailto` are the only schemes tuimex will open; anything
else is shown and refused. Before a link opens, the status line shows its
real host. A link is confirmed first when its text names a different host
from its target, when it carries credentials before an `@`, when its host is
a bare IP address, or when it mixes scripts.

Assessment is a pure function in `tuimex-core`. Known click-tracking
parameters are removed before opening.

## ADR-019 — Gmail signs in with OAuth2, through a client ID the user supplies

Gmail is reached with XOAUTH2: a browser sign-in to a loopback redirect,
with PKCE, and the refresh token kept in the keyring (ADR-010). `tuimex
setup` walks through creating the client in Google's console and asks for
its ID and secret, which are stored in the keyring too.

Google classes the mail scope as restricted and requires a paid annual
security assessment of any client that ships with one. Projects of this size
ask the user for their own, and so does this one. An app password remains
the other route.

**What it costs.** About ten minutes in Google's console before first use,
and a consent screen that says the app is unverified.

*2026-11-23, as built in M9.* One client serves every Gmail account: its
ID and secret are kept once, under `oauth/google/…`, and each account's
refresh token under its own name. The token goes with the `AUTHENTICATE`
command where the server offers SASL-IR (ADR-023's note). The token
requests are checked against the roots `ureq` carries, Mozilla's, not the
system's as IMAP and SMTP are (ADR-029): that is what the one feature
PLAN §1 allows it brings. Google's pages, read that day, add two costs
this record did not know: while the user's app is in "Testing" a sign-in
lasts seven days, and a client's secret is shown only when it is made.
Setup says both. Proven against a stand-in for Google and against
GreenMail; against Google itself only by `docs/testing.md`, which nobody
has run.

## ADR-020 — OpenPGP is the `gpg` binary

Signing, encrypting, verifying and decrypting run `gpg` with the message on
standard input. tuimex never reads a private key, has no keyring of its own
and parses only gpg's `--status-fd` output. With no `gpg` on the path, a
signed message says so and an encrypted one cannot be opened.

S/MIME is not in v1.

## ADR-021 — An invitation is read and answered, not kept

A `text/calendar` part is shown as a card: what, when in the local zone,
where, who. Accept, tentative and decline compose a reply carrying the
matching `REPLY`. Nothing is stored and no calendar is written to; a URL in
an invitation is a link like any other (ADR-018).

The parser is a small one in `tuimex-content` covering the properties the
card shows. Recurrence is displayed as its rule in words where simple and as
"repeats" otherwise.

*2026-11-24: ADR-054 replaces "Nothing is stored and no calendar is written
to", and ADR-057 adds two answers to the three. The card and the rule for a
URL stand.*

## ADR-022 — What v1 leaves out

Each of these is absent on purpose, and adding one takes a record:

- Windows. Maildir names, the keyring and paths each need a second branch.
  CI proves Linux and macOS.
- Microsoft 365 sign-in and JMAP. The `Remote` trait is where they would go.
- Composing HTML. Mail is written as plain text in `$EDITOR`.
- A layout engine for HTML, and any browser engine (ADR-009).
- Remote content inside tuimex (ADR-008).
- Scripting, plugins and hooks.
- Search on the server. Search covers what has been synced.
- S/MIME (ADR-020), and a calendar store (ADR-021).
- A settings screen. `tuimex setup` asks plain questions; everything else is
  the config file.

*2026-11-24: a calendar store is no longer left out (ADR-054).*

## ADR-023 — The IMAP client is the `imap` crate, pinned, behind a trait

The engine speaks IMAP through `imap` at an exact pinned version, wrapped by
a `Remote` trait that the rest of the engine uses.

The crate's 3.0 line is still an alpha, which would normally rule it out.
It is chosen because it carries everything ADR-014 needs (UIDVALIDITY,
`ENABLE CONDSTORE`, IDLE, `UID MOVE`, `UID EXPUNGE`, XOAUTH2), and the
evidence is the spike below, against real servers. Its `AutoTls` mode was
read and fails closed. It is blocking, which suits ADR-006.

M3 begins with a spike that exercises those six things against the CI
server before anything is built on them. If one fails, a record replaces
this one before M3 continues.

**What it costs.** A pinned alpha takes no fixes without a deliberate bump,
and the trait is a layer that exists partly so the crate can be replaced.

*2026-11-23: the spike ran and this record stands.* It is
`crates/tuimex-engine/tests/spike.rs`, run by `make real-server` against
two servers in containers, because neither offers everything:

| Server | What it offers | The spike's last line |
|---|---|---|
| GreenMail 2.1.15 | UIDPLUS, MOVE, IDLE, `AUTH=XOAUTH2`; no CONDSTORE, no SPECIAL-USE | `spike: ran uidvalidity,idle,move,uidplus,xoauth2` |
| Dovecot 2.3, as its image is configured | CONDSTORE, UIDPLUS, MOVE, IDLE; no XOAUTH2, no SPECIAL-USE | `spike: ran uidvalidity,condstore,idle,move,uidplus,xoauth2-absent` |

So CONDSTORE is proven on Dovecot alone, and XOAUTH2 on GreenMail alone,
where "proven" means the crate carried the exchange and the server refused
a token that was not one; a real token is M9's. Neither server offers
SPECIAL-USE, so the folder roles that come from it are proven against
`FakeRemote` alone. What the spike found about how the crate connects is ADR-029.

*2026-11-23, with M9: the XOAUTH2 row proved less than it said, and now
proves it.* The spike sent `AUTHENTICATE XOAUTH2` and gave the token when
challenged, as the crate's `authenticate` does. GreenMail takes the token
only on the command line itself (SASL-IR) and answered `BAD Missing
argument`, which the spike read as a refused token. The engine now sends
the token with the command wherever a server offers SASL-IR, which Gmail
also does, and the spike asserts a `NO`. GreenMail accepts exactly one
token, the account's own password, over IMAP and SMTP, so
`tests/sign_in.rs` proves the exchange and a refused token, and nothing
about Google.

## ADR-024 — An agent drafts the design; it is the spec once Bryan accepts it

ADR-002 had Bryan deliver `design/tui.js` and told the agent not to write
it. On 2026-11-23 he asked for a draft instead. An agent writes
`design/tui.js` from [the brief](history/2026-11-23-design-brief.md) on a branch, and
it becomes the visual spec when Bryan merges it, not before. Until then no
golden is generated from it and M6 does not start.

That is the only part of ADR-002 this replaces. The artifact is still the
spec, the goldens are still its proof, and a change to the look still starts
in `tui.js`.

The draft is tuimex's own design for triage. Reusing pullpen's visual system
was offered and turned down; what the two share is the artifact's shape and
the command line's conventions (PLAN §6.2).

Bryan accepted the second draft on 2026-11-23, as drawn, with the choices
it lists.

**What it costs.** An agent's draft drifts toward what is easy to render and
toward the sibling it was shown, and it arrives looking finished, which
makes it easier to accept than to question. The answer is that Bryan reviews
the screens rather than the diff, and the draft lists every place it chose
something the brief left open.

## ADR-025 — The binary budget stays 16 MiB, now against a measurement

ADR-004's figure was an estimate. Measured at M0, on x86-64 Linux with
rustc 1.100.0 and the release profile (stripped, thin LTO, one codegen
unit):

| Build | Bytes | MiB |
|---|---|---|
| The scaffold as committed | 1,080,416 | 1.03 |
| The scaffold with every dependency in PLAN §1 linked | 12,605,256 | 12.02 |

A dependency nobody calls is not linked, so the second figure did not come
from adding lines to the manifests. A copy of the scaffold, kept outside the
repository, was given one function per crate that does that crate's job on
data read at run time, so nothing could be folded away: parse a message,
sanitise its HTML and render it as text, build a message, open a SQLite
database and query an FTS5 table, sign in over IMAP then fetch, move,
expunge and idle, send raw bytes over SMTP, post a form with `ureq`, add a
certificate to a `rustls` root store, write and read a keyring entry, decode
an image and draw it through `ratatui-image`, and so on down the list. The
copy was built and not run: running it would have written to a real keyring.
Each crate that carries panic messages was then found by its source path
among the binary's strings. `serde`, `directories`, `thiserror`, `zeroize`,
`sha2` and `getrandom` leave no such trace and are linked only on the
evidence that the code calling them compiled into the same binary. `insta`
is used by tests alone and is never in the binary.

Building the same copy with one dependency taken out shows where the weight
is:

| Left out | MiB saved |
|---|---|
| `keyring` (the Secret Service client and its D-Bus library) | 2.37 |
| `rusqlite` with bundled SQLite | 2.07 |
| `image` and `ratatui-image` | 1.00 |
| `imap` (which brings `rustls` 0.22 beside the 0.23 that `lettre` and `ureq` use) | 0.95 |

The budget stays 16 MiB. That is the measurement plus 3.98 MiB, and the
headroom has to hold everything tuimex itself becomes.

**What it costs.** One call per crate is a floor: a crate used fully links
more of itself than a crate called once. How much 14,000 lines of tuimex
weigh is not known either. If 3.98 MiB turns out to be too little, the record
that raises the figure has the second table to argue with first.

## ADR-026 — A palette is imported from a terminal theme the user already has

`tuimex palette import <file>` reads a terminal theme in Ghostty's or
Alacritty's format and writes a palette file to the config directory, which
`ui.palette` then names (ADR-003). The mapping from a theme's colours to the
design's tokens is one table, and the design states it.

Some palettes cannot ship. Bryan wanted Dracula PRO's Nosferatu, whose
licence forbids copying it into other software, and ADR-003 already keeps
every palette but one out of the repository. Importing reads the copy the
user is licensed to hold and writes it to their own config; no value passes
through anything tuimex distributes.

It is built with the palette loader in M6. The file is created through the
store's file module like every other (ADR-011).

**What it costs.** Two theme formats to parse, and a token with no
counterpart in a sixteen-colour theme (`faint`) has to be derived or left to
the built-in value.

## ADR-027 — A folder's role and a draft's author ride in the Maildir too

Beside each folder's UIDVALIDITY file sits a `tuimex-role` file naming its
role, and a draft's file name carries who wrote it (`,O=a` for an agent,
`,O=l` for the user), next to the UID.

ADR-013 says deleting the index loses nothing, and lists the UID and
UIDVALIDITY as what the Maildir must therefore hold. Two more facts turned
out to exist nowhere else: which folder is the Archive, which `in:archive`
needs, and that an agent wrote a draft, which `is:agent-draft`, the reader's
notice and the send summary all need (ADR-016). An index rebuilt without
them would answer those queries differently, and the rebuild test compares
exactly that.

The CONDSTORE modseq is deliberately not kept this way. A rebuild loses it
and pays one full flag fetch per folder.

**What it costs.** Two more conventions another Maildir tool does not know.
A tool that renames a draft and drops `,O=a` turns an agent's draft into an
ordinary one.

## ADR-028 — A message whose body has not been fetched is its header block, marked in its name

PLAN §4.3 downloads an envelope for every message and a body only for
recent, small or opened ones, and ADR-013 says everything sync needs is in
the Maildir. The plan did not say what a message without its body is on
disk. It is a file holding the header block the server sent, with `,H=1`
among the fields of its name, beside `,U=`. When the body arrives the file's
content is replaced whole and the field is dropped; the key, the UID and the
flags stay.

The mark is in the name for the reason the UID is: an index that knew it
alone would lose it in a rebuild, and every message would then look complete
or need fetching again. A header block with nothing after it cannot be told
from a message with an empty body by its content.

The index reads such a file as it reads any other, so the message is listed,
threaded and found by its headers before its body is here.

**What it costs.** Another mail reader pointed at the Maildir shows these
messages with empty bodies. `larger:` compares against the size of the file,
which for a header block is not the message's size. A crash between
replacing the content and renaming leaves a whole message still marked, and
it is fetched once more when opened.

## ADR-029 — The engine makes the TLS connection and hands it to the IMAP client

The engine opens the socket, does the TLS handshake with the workspace's
`rustls`, and gives the stream to `imap::Client::new`. The crate's own
`rustls-tls` feature is off, and `rustls-native-certs` is named directly for
the system's trust roots. STARTTLS is sent by the engine, which reads the
answer and nothing after it before the handshake.

M0 assumed the crate would connect by itself. Its builder cannot do two
things the records need: it takes no trust store, so a pinned certificate
cannot be checked (ADR-012), and it never gives up its socket, so a blocked
IDLE cannot be ended by closing it from another thread (ADR-006). The spike
connected the other way and passed (ADR-023), so that is the way built on.

A pinned account accepts exactly the certificate the user accepted, whoever
signed it, with the handshake's signature still checked; any other account
is checked against the system's roots. There is still no way to turn
checking off.

Measured on x86-64 Linux, release profile:

| | |
|---|---|
| Leave `Cargo.lock` | `rustls` 0.22.4, `rustls-webpki` 0.102.8, `rustls-connector` 0.19.2, `rustls-native-certs` 0.7.3, `rustls-pemfile` 2.2.0, `openssl-probe` 0.1.6, `core-foundation` 0.9.4, `security-framework` 2.11.1 |
| Enters | `rustls-native-certs` 0.8.5, the version `lettre` asks for, so SMTP adds no second copy |
| The binary before `tuimex sync` existed (no IMAP, TLS or keyring linked) | 5,737,000 bytes |
| The binary with `tuimex sync` (IMAP, `rustls` 0.23, the roots, the keyring) | 10,344,104 bytes |

The binary was never built with the crate's 0.22 path, so there is no figure
for what leaving it saved here. ADR-025's row for `imap` (0.95 MiB,
counting `rustls` 0.22) describes a build that no longer exists and is
stale; its row for `keyring` stands.

**What it costs.** About forty lines of connection code that the crate
would otherwise own, including the STARTTLS upgrade, and a verifier of our
own for pinned accounts. `keyring` on Linux brings `zbus` and with it
`async-io` and `async-executor`: an executor inside a dependency, on a
thread it owns, which ADR-006 did not foresee and this record does not
settle.

## ADR-030 — The policy is asked once for each folder a call touches, and a message is named by account and key

ADR-016 made the policy one function over one account's grants. Five of
the seven subcommands span accounts or folders, so this is how they ask:

- `allows` answers for one account and at most one folder. `folders`,
  `list`, `search` and `thread` ask again for every folder, row and member
  they would return, and leave out what is refused. A call is refused as a
  whole only when nothing in its scope is allowed, so a query can narrow
  what a grant covers and never widen it.
- An account that grants nothing is not named by `accounts`, and asking for
  it by name is refused in the words an account that does not exist gets.
- `draft --reply-to` needs `read` over the folder of the message answered:
  the draft carries that message's ids.
- A folder is granted by its name, matched whole; `INBOX` in any case. A
  role is not a name: `folders = ["sent"]` covers no folder called `Sent`.
- A key is unique within an account and not across them, so `show` and
  `thread` take `--account`, and without it an id two readable accounts
  share is an error that says so. PLAN §7 first gave them the id alone.
- `list` and `search` return fifty rows unless `--limit` says otherwise,
  counted after the policy has removed what it removes.

**What it costs.** `thread` reads every envelope of the account to find one
conversation. A refusal and "no such message" differ, so an agent that may
read an account can learn that an id exists in a folder it may not read.

## ADR-031 — `:agent` hands text to a command and takes text back

`:agent <function>` runs the command configured under `[agent]`, writes a
prompt to its standard input and reads text from its standard output. v1
has three functions:

- `:agent reply [guidance]`, in compose: the thread as the reader shows it
  (ADR-017) goes in, and what comes back becomes the body.
- `:agent tidy`, in compose: the body as written goes in, and a cleaned
  version comes back. The original is one `u` away.
- `:agent link <n>`: the link's address, the text it was shown under, the
  assessment's warnings (ADR-018) and the message go in; the verdict comes
  back as advice beside the confirm. It neither opens nor blocks the link.

The agent can only produce text. Recipients, identity and attachments are
never taken from its output, so a message that tells the agent to send
something elsewhere can at worst put those words in a body the user reads
before the send summary (ADR-015). A body an agent wrote or changed is
marked as an agent's (ADR-016).

An agent never opens a link. The command is run with its tools off, and
nothing tuimex does fetches on its behalf (ADR-008): a fetch tells the
sender the mail was read, which is the report the north star rules out.

Before text leaves, addresses, phone numbers, long digit runs and the names
in the message's headers are replaced by placeholders (`[person-1]`,
`[address-2]`), and put back in what returns. Link addresses are withheld
from `reply` and `tidy`.

This is the one place message text leaves the machine: a "local" agent is a
local program that sends what it is given to its provider. It happens only
when `:agent` is typed, never on opening or selecting a message, and with
no `[agent]` table the command does not exist. ADR-022 rules out hooks;
this is a single command with a fixed contract, not an event anything can
subscribe to.

**What it costs.** Redaction by pattern misses what has no pattern: a name
in running text, a street address, an account number written in words. The
status line says what was replaced, so the gap is visible, not hidden. The
process is started by the engine, as `gpg` is (ADR-020).

*2026-11-23: the Claude CLI is built in.* `[agent]` takes `use = "claude"`
in place of a `command`, and tuimex supplies the arguments: answer once,
no built-in tool, no MCP server, no skill or slash command, nothing saved,
anything that would ask refused. "An agent never opens a link" then rests
on a constant and the test that pins it
(`the_built_in_claude_command_has_its_tools_off_and_is_exactly_this`), not
on a line a user could mistype or trim. `command` stays for any other
program and cannot be combined with `use`; for one, `tuimex doctor` says
that tuimex cannot vouch for what it does with a link. `tuimex setup` asks
one question and writes the table.

*2026-11-24: `A` opens a menu.* The same three functions are offered on
a key, where each applies: `r` on a row, in the reader and in compose,
`t` in compose, `l` over a link. Opening the menu sends nothing; the
pick is the asking, as typing the command is, and with no `[agent]`
table the key says so and offers nothing.

## ADR-034 — A message that was moved is found again by its Message-ID

After a move or an append the engine asks the target folder which UID holds
the message's `Message-ID` (`UID SEARCH UNDELETED HEADER Message-ID`), and
gives the file here that UID. A change made to a message before its move
has been replayed is written to the journal without a UID, and is resolved
the same way when its turn comes. `RemoteWrite` has `find` for this, and
`move_to` and `append` report no UIDs.

PLAN §4 had the server report the new UIDs. UIDPLUS does send them, in
`COPYUID` and `APPENDUID`, but the pinned `imap` crate (ADR-023) returns
`()` from `uid_mv` and `uid_copy` and drops the tagged line that carries
`COPYUID`; a server without UIDPLUS sends nothing at all. Without the new
UID the folder's next pass fetches the moved message as a new one beside
the copy already here, and `u` after the server has applied a move
(ADR-015) has nothing to name it by. Asking works on every server and is
one mechanism in place of two.

Two things follow. A message marked `\Deleted` is treated as removed:
`uids` lists `UNDELETED`, so the original a server without MOVE or UIDPLUS
is left holding (ADR-014) does not come back as mail. And a message with
no `Message-ID`, or one that cannot be written in a command, cannot be
found: its copy here is removed and the folder's next pass brings the
server's, under a new key, so `u` no longer reaches it.

Proven by `make real-server` on both servers (`archive-trash-undo`, and
`no-move` for the copy path), and against `FakeRemote` by the tests in
`crates/tuimex-engine/src/act/tests.rs`. Gmail, where the found UID may be
one a file here already has, is covered by a unit test of the rule and not
by a server.

**What it costs.** A `SELECT` and a search for each folder messages were
moved to, and one search for each message. Two messages with one
`Message-ID` in a folder are told apart only by taking the newest.

## ADR-035 — SMTP is `lettre`'s connection; a pinned certificate is compared after its handshake

Mail is sent through `lettre::transport::smtp::client::SmtpConnection`,
with the bytes from the Outbox, less the `Bcc` header, which
`tuimex-content` removes (ADR-007) because the recipients must not be shown
it. For an account without a pin `lettre` checks the certificate against
the system's roots, loaded by the same `rustls-native-certs` as IMAP
(ADR-029). For a pinned account `lettre` is told to accept any certificate,
and the engine compares the one presented with the pinned one, byte for
byte, before the password or a message is sent; a difference is the error
of ADR-012 with both fingerprints.

ADR-029 gives IMAP a verifier of the engine's own. `lettre` takes none: its
TLS parameters are built from a root store and two switches, and its
`rustls` configuration is private. Adding the pinned certificate as a root
was the first plan and does not survive a self-signed certificate marked as
a certificate authority, which is what Proton Bridge presents: `webpki`
refuses a CA used as a server's own certificate. So the switch is used and
the check is made by hand. What holds is ADR-029's rule exactly: the one
certificate the user accepted, whoever signed it, with the handshake's
signature still verified by `lettre`'s verifier, so the server holds its
key.

The difference from IMAP is when. `lettre` reads the greeting and sends
`EHLO` as part of connecting, so a server with the wrong certificate is
told `EHLO [127.0.0.1]` before it is left; IMAP's is told nothing. STARTTLS
fails closed in `lettre` itself: a server that does not offer it is sent
nothing after `EHLO`.

Proven by `make real-server` against GreenMail (`send`, `send-tls`: the
pin, a changed pin, no pin, and a plain port without STARTTLS). The claim
about a CA-marked certificate rests on reading `webpki` and was not run.

**What it costs.** The words `dangerous_accept_invalid_certs` in the
source, safe only while the comparison stays on the line after the
handshake; `lettre` holds the password in an ordinary `String` for the
length of the exchange, where the engine's own type zeroes it; and a send
blocked on the network cannot be ended from another thread (ADR-006), so
stopping waits for it, at most the minute a silent server is given.

## ADR-036 — A saved attachment is the user's file, wherever they say

`:save` writes an attachment to the download directory, or to the path
typed after it. That is a fifth place, which ADR-011 does not list, because
the file is not tuimex's: it is not read back, not swept, and not removed at
exit. Everything else in ADR-011 holds for it. It is created owner-only by
the same module, by a call that fails if the path exists, so a second save
goes beside the first under a numbered name and a typed path is never
written over. Its name is the one `tuimex-content` reduced to a bare file
name (ADR-007), and a typed relative path is read against the download
directory, never the current one.

A file handed to another program to open, and the browser copy, are still
tuimex's own: they go under the cache directory and are removed after a day
and at exit, as ADR-011 says.

**What it costs.** A file saved for someone else to read has to be given a
wider mode by hand. A download directory that does not exist is created
owner-only; one that exists keeps the mode it had.

## ADR-037 — The reader's text says its own structure, in how a line begins

`Body::text` is plain text, and it is the only thing the reader, the
preview and an agent are given (ADR-017). The design draws headings, quotes,
lists, tables, pictures and small print differently, so the text has to say
which is which, and it says it in a form that still reads as text: `# ` a
heading, `> ` a quote, `* ` a list item, `[image: alt]` a picture that was
not loaded or the file name of one that was attached, tabs between a
table's cells, and a line of dashes before the small print. `tuimex-ui`
styles a line by that and by nothing else, so a person and an agent are
still shown the same words.

A second, structured body beside the text was the alternative. It would be
a second thing to keep equal to the first, and ADR-017 exists because two
renderings of one message drift apart.

**What it costs.** `tuimex-content` writes headings, quotes, lists and
pictures this way today; a table arrives from it as aligned columns and a
rule not at all, so real mail gets no dim header row and no dim small print
until it does. Plain text that happens to begin `# ` or `> ` is drawn as a
heading or a quote, which for a quoted reply is what was wanted.

*2026-11-23, after use on real mail: emphasis has no marker.* Bold, italic,
code and raised text were written as `**`, `*`, a backtick and `^{}`, and
nothing drew them, so nested bold reached Bryan as `****words****`. The
text now holds the author's words alone, and a `**` in it is one they
typed; ADR-009's "emphasis" is not shown. Struck text keeps its combining
strokes, which are characters and change what a sentence says. The corpus
cases named `emphasis-…` hold this. Drawing bold would need the design to
draw it and this record to say where the ranges ride.

## ADR-040 — The interface owns what `u` reverses; the engine answers by serial

An action is an `Ask { serial, action, on }`. The interface shows its effect
at once (ADR-015) and keeps what it changed, newest last; `u` puts the
newest back on screen and hands on `Ask { Undo, on }` naming the messages
put back. The engine keeps, for each `Ask` of the run, the journal entries
it wrote. For an `Undo` it reverses the newest `Ask` it carried out over
exactly those messages, and a message waiting in the Outbox returns to
Drafts whichever run held it. `Engines` splits an `Ask` by account under
the one serial, so each account answers for its share.

A refusal is `ActionFailed { serial, .. }`: at once when nothing could be
done, or later, when the server refuses the journal entries that `Ask`
wrote. The interface puts back what that one `Ask` changed and `u` no
longer reaches it on either side. What belongs to no `Ask` of the run is
`Notice { account, error, text, hint }`: a change an earlier run journaled
and the server now refuses, a Sent copy it would not keep, originals it
will purge itself (ADR-014). The store already holds the outcome and the
interface only says it.

The engine and the interface were built apart. The engine had `u` take back
"the last action" with no messages named, and remembered one; the interface
had a stack and named the messages. Two owners of "the last action" disagree
as soon as one action is refused or a second window, the agent command line
or a restart comes between them. The interface is the owner because it is
what the user saw: its stack is the order they acted in.

An `Undo` names messages and not a serial because one case has no serial to
name: mail left in the Outbox by an earlier run, which `u` must still stop.
An `Ask` that changed nothing is remembered too, so that undoing it does
not reach past it to an older one over the same messages.

Proven by `crates/tuimex-engine/src/act/tests.rs` (a refusal names its
`Ask`; a refused `Ask` is not there for `u`; a journal entry from an earlier
run is refused as a `Notice`; two accounts answer under one serial) and
`crates/tuimex-ui/src/app/tests.rs` (`a_refusal_puts_back_only_what_the_refused_action_changed`,
`a_notice_is_said_as_the_engine_worded_it`).

**What it costs.** Two memories that must stay in step. The engine keeps the
last two hundred asks; a refusal of an older one is a `Notice`, and `u`
cannot reach it. After a restart nothing can be undone, on either side.

## ADR-041 — What is deleted for good is deleted by name, through the journal

The journal has a third change beside flags and a move: `Delete`. Made here
it removes the file at once; replayed, it marks the message `\Deleted` by
its UID and purges it with `UID EXPUNGE` naming that UID. A server without
UIDPLUS is left to purge what was marked, and the status line says so; a
bare `EXPUNGE` is never sent (ADR-014). `RemoteWrite::delete` is the one
call. A delete cannot be taken back: one the server refuses is reported,
and the folder's next pass brings the message here again.

Two things delete. `:empty-trash` deletes every message in an account's
Trash that has a UID there; one trashed so lately that the server has not
numbered it stays, because the server could not be told to remove it and
would send it back. And a later version of a draft replaces the earlier one
under the same name: the earlier one, where the server holds it, is deleted
there. `Discard` in compose is the same delete, of a draft only.

M4 left `:empty-trash` unowned and sent a superseded draft to the Trash,
under a new name each time. The interface names a draft once and goes on
naming it (`Written`, then `SaveDraft` or `Send`), so the engine keeps the
name; and a Trash that fills with every keystroke-saved version of a draft
is not what anyone asked for.

Proven by `emptying_the_trash_deletes_by_name_and_only_what_was_in_it`
(with and without UIDPLUS) and
`a_later_version_of_a_draft_the_server_keeps_deletes_the_servers_and_only_a_draft_is_discarded`
against `FakeRemote`, and through the interface by
`a_message_is_read_through_the_engine_and_the_trash_is_emptied_on_the_server`.
No real server has been asked to delete: `make real-server` does not empty
a Trash.

**What it costs.** The one change in the journal that undo cannot reverse.
A message deleted here while offline is gone here before the server has
agreed.

## ADR-042 — The code budget is 18,000 lines

`MAX_CODE_LINES` goes from 14,000 to 18,000. The other three budgets stand.

ADR-004 set the cap below its own estimate so that the last milestones
would pay for their lines or write a record. This is that record. With M0
to M9 merged the count is 14,831:

| Crate | Lines |
|---|---|
| `tuimex-ui` | 4,315 |
| `tuimex` (the binary: wiring, the agent commands, the live backend, setup, doctor) | 3,270 |
| `tuimex-engine` | 2,588 |
| `tuimex-fixtures` | 1,270 |
| `tuimex-core` | 1,193 |
| `tuimex-store` | 1,171 |
| `tuimex-content` | 1,024 |

The estimate behind 14,000 was a sum of guesses made before the design
existed, and it left out what the plan had not yet named: the fixtures that
play a mail server and Google's sign-in (1,270 lines that ship only because
`--fixtures` does), the backend that runs the interface over real accounts,
and the built-in agent. M9 itself is about 1,800: setup 690, doctor 280,
the OAuth2 sign-in and its stand-in 500, and the rest in the engine's
sign-in.

Bryan was asked on 2026-11-23 and said: "raise the cap if we need to, don't
cut features". Nothing in 0.1.0's list (PLAN §10) is thinner for the old
figure, and nothing was cut to stay under it.

18,000 is the count plus about 3,170: OpenPGP and invitations (M10), the
polish in M11, and what real use turns up between here and the release.
It is headroom and not a target. The cap still does its job at the new
figure: the next record of this kind has this table to argue with.

The binary is 16,209,864 bytes against 16,777,216, so that budget is not
crossed and is not raised here. It is close: M9 linked `ureq` and its
bundled roots for the first time, and M10 links nothing new. If it is
crossed, ADR-025's second table is where that record starts.

**What it costs.** A cap that has moved once is easier to move again, and
this is the first of the records ADR-004 said it would cost.

## ADR-043 — Setup adds to the config file and never rewrites it

`tuimex setup` is the only code that writes `config.toml`, and what it
writes is the file as it was, byte for byte, with new tables after it: an
`[[account]]`, and `[agent]` when the user says yes and there is none. The
whole text is read back through the same strict parser before it replaces
the file, through the store's file module, owner-only (ADR-011). A file
that does not parse is not touched, and neither is one that cannot be
added to (accounts written as one inline array): setup then prints the
table to add by hand. Signing in again to an account, or accepting its
server's new certificate, changes the keyring and the certificate file and
not the config.

The `toml` crate reads a file into values and cannot write one back with
its comments, and the crate that can is not on the list (ADR-005). A
config is a file people write notes in. So the choice was between
rewriting it and losing them, a hand-made editor of TOML text, and never
editing at all. Appending is always valid TOML for these tables, needs no
parser of our own, and cannot damage what it does not touch.

pullpen's rule, taken from a sibling project, is one writer for a config so that two
cannot disagree about its shape. That holds here: nothing else writes it.

**What it costs.** Setup cannot change an account's servers or remove an
account: that is an edit by hand. And an account added to a file that ends
in the middle of another table's keys lands after them, where it is
correct and may not be where its owner would have put it.


## ADR-044 — Mail in any charset is read: `mail-parser` with `full_encoding`, which brings `encoding_rs`

`mail-parser` is built with its `full_encoding` feature, and `encoding_rs`
joins the list in PLAN §1 as the crate that feature depends on. Nothing in
tuimex calls it: `mail-parser` hands every header and body part to it
when the part names a charset the parser's own tables do not cover.

Without the feature `mail-parser` decodes UTF-8, the ISO-8859 family, the
Windows code pages and a few more, and returns anything else with a
replacement character for every byte it could not place. That is Shift_JIS,
ISO-2022-JP, EUC-JP, GBK, GB2312, GB18030, Big5 and EUC-KR: most mail
written in Japanese, Chinese or Korean before UTF-8 took over, and a good
deal written since. Such a message was a subject of question marks in the
list and a body of them in the reader, and the same to an agent
(ADR-017). North star: "mail can be read".

`encoding_rs` replaces nothing here; there was no decoder for these to
replace. It is the decoder Firefox uses, it does no I/O, so
`tuimex-content` still has no dependency that does (ADR-007), and its
only dependency is `cfg-if`. Writing the tables by hand, or leaving the
scripts unread until someone asked, were the alternatives; the first is
tens of thousands of table entries nobody here could review, and the
second fails the first user who has such mail without telling them why.

It costs 172,032 bytes of the stripped release binary on Linux x86-64:
16,291,784 without the feature and 16,463,816 with it, measured on this
tree. The budget of ADR-025 is 16,777,216, and is not crossed.

Proven by seven messages in `tests/corpus` (`charset-*.eml`: Shift_JIS,
ISO-2022-JP, GBK, GB2312 in HTML, Big5, EUC-KR, and windows-1251 as one
that already worked), each read as text by the corpus table and its
subject and sender by
`a_subject_and_a_name_in_a_charset_of_their_own_are_read_as_written`. With
the feature off, six of the seven and that test fail.

**What it costs.** The bytes above, with 313,400 left under the binary
budget for M10 and M11. A charset label that lies is still decoded as
labelled; nothing here guesses.

## ADR-046 — An Archive is made after asking, by the move that first needs it

PLAN §4.1 says an account with no Archive folder gets one "created on
first use after asking", and nothing asked. `e` in such an account now
asks, in the confirm grammar, "`<account>` has no Archive folder. Create
one on its server?". A yes is `Action::CreateArchive`, which makes a folder
named Archive in the Maildir with the role in it (ADR-027), and the `e`
that was pressed is then carried out as any other. The server's folder is
made by `RemoteWrite::create` when the first move to it is replayed: a
folder the server has never numbered is created before a message is moved
to it, and takes the server's UIDVALIDITY from that moment, so the message
keeps its key and `u` still reaches it (ADR-034). The name is the one the
mapping looks for, so the next listing gives the folder its role with
nothing written to the config, which only setup writes (ADR-043).

The other way round was making the folder on the server at the yes and
archiving when it was listed. That needs a connection at the moment of the
question, and ADR-014 exists so that nothing a person does waits for one.

Every folder the server lists is also made known here before the first is
filled. A first sync shows the Inbox long before it reaches the Archive,
and `e` meanwhile said there was no Archive folder; the move now waits in
the journal for the folder's turn.

Trash and Sent are not made. A message is not sent to a Trash nobody
agreed to, and a Sent folder under a name the server's own client does not
use would be a second one. Without a Trash `d` is refused and says so;
without a Sent the copy stays on this machine and a notice says so.

Proven on GreenMail and Dovecot by `make real-server` (`tui-archive`; and
`tui-send-refused` for the missing Sent), against `FakeRemote` by
`an_archive_the_user_agreed_to_is_made_on_the_server_by_the_first_move_there_and_u_still_reaches_the_message`
and `every_folder_is_known_here_by_the_time_the_first_is_filled_so_what_has_arrived_can_be_archived_at_once`,
and through the interface by `an_archive_is_made_on_the_server_after_a_yes_and_never_before`.

**What it costs.** The design has no wording for the question; this one
is ours until it does. A server that keeps every folder under its Inbox
refuses a folder named Archive at the top, and the move is then refused
and put back, each time. A yes that is never followed by a move leaves a
folder here that the server does not have.

## ADR-047 — A Trash is made after asking too; a Sent folder still is not

ADR-046 refused to make a Trash: "a message is not sent to a Trash nobody
agreed to", and `d` in an account without one said `no Trash folder` and
moved nothing. That left an account whose server lists no Trash with no
way to delete from the keyboard except to name a folder in the config,
which is a long way from the key that was pressed.

The objection was to making one unasked, and asking answers it. `d` in
such an account now asks, in ADR-046's words, "`<account>` has no Trash
folder. Create one on its server?". A yes is `Action::CreateTrash`, and
everything after it is ADR-046's mechanism unchanged: a folder named Trash
here with the role in it, made on the server by the first move there, the
`d` that was pressed carried out, and `u` still reaching the message.
`:empty-trash` then has a Trash to empty.

The strongest case against is the server whose Trash has a name the
mapping does not know and no SPECIAL-USE attribute: a yes gives it a
second one. The question says a folder will be created, the answer can be
no, and `folders.trash = "…"` remains the way to name the one that is
there. ADR-046 took the same risk for the Archive.

Sent is still not made. The copy of a sent message is filed by no key the
user presses, so there is no moment to ask at; and a second Sent beside
the one the server's own client files into splits a person's sent mail in
two, which a second Trash does not do to anything they will look for.

This replaces the paragraph of ADR-046 that begins "Trash and Sent are
not made", and the last clause of PLAN §4.1.

Proven on GreenMail and Dovecot by `make real-server` (`tui-archive`),
against `FakeRemote` by
`a_trash_the_user_agreed_to_is_made_on_the_server_by_the_first_move_there_and_u_still_reaches_the_message`,
and through the interface by
`trashing_in_an_account_with_no_trash_asks_to_make_one_and_then_trashes`
and `a_trash_is_made_on_the_server_after_a_yes_and_never_before`.

**What it costs.** The design has no wording for this question either. A
yes that is never followed by a move leaves a folder here that the server
does not have, as with the Archive.

## ADR-048 — A folder made here is named under the Inbox where the server keeps its folders there

ADR-046 named the folder it makes `Archive`, and said what that cost: a
server that keeps every folder under its Inbox refuses a folder at the
top, so the move was refused and put back, each time. Courier does this,
and Dovecot wherever it is configured to; ADR-047 gave the Trash the same
fault.

Every pass over an account now reads the prefix from the folder list:
where the list has folders beside the Inbox and every one of them is
named `INBOX` and the list's delimiter and more, the prefix is that
(`INBOX.`), and otherwise it is nothing. It is recorded in the account's
Maildir, in a file as a folder's role is (ADR-027). A yes to either
question then makes `<prefix>Archive` or `<prefix>Trash`. The mapping
already finds a role by the last level of a name, so the next listing
gives `INBOX.Archive` the Archive role with nothing configured.

It is recorded, and not worked out at the yes, for ADR-046's reason:
nothing a person does waits for a connection. It is in the Maildir and not
the index because the index is a cache (ADR-013), and after a rebuild with
no server in reach the folder must still be named right.

`NAMESPACE` (RFC 2342) is the command that asks this outright, and it was
built first. The pinned `imap` crate (ADR-023) cannot carry it: its parser
has no rule for the untagged `* NAMESPACE` line, and a line it cannot
parse ends the command as if the server had said `BYE`. `make real-server`
found that on Dovecot, where every pass then failed. What the list shows
is what is left on every server, with no second mechanism.

The other way was to make `Archive` here and rename it to whatever the
server accepted when the first move was replayed. That renames a folder,
its messages' index rows and the journal entries that name it, across a
window a crash can fall in.

Proven on a Dovecot configured with `prefix = INBOX.` by `make
real-server` (the `dovecot-ns` leg: `tui-archive` with both folders made
under `INBOX.`), against `FakeRemote` refusing any folder outside its
prefix by
`an_archive_and_a_trash_are_made_under_the_servers_namespace_where_it_keeps_folders_under_the_inbox`,
and by
`the_namespace_is_the_inbox_and_its_delimiter_only_where_every_other_folder_is_under_it`
and `the_servers_namespace_is_kept_with_the_mail_and_outlives_the_index`.

**What it costs.** It is an inference. An account with no folder but its
Inbox shows nothing, and a folder made for it goes to the top as before;
so does one on a server that lists other people's shared folders beside
the person's own, and one whose prefix is something other than the Inbox.
`folders.archive` and `folders.trash` in the config remain the answer
there. A yes given with no connection, before any pass of this version
has listed the account, also names the folder at the top.

## ADR-049 — The binary budget is 17 MiB

`MAX_BINARY_BYTES` goes from 16,777,216 to 17,825,792. The other three
budgets stand.

ADR-025 kept 16 MiB against a scaffold that called each dependency once,
and said what that left unknown: how much more of a crate is linked when
it is used fully, and what tuimex's own code weighs. Both are known now.
With M0 to M9 merged and two rounds of fixes from real use, the stripped
release binary on x86-64 Linux (rustc 1.100.0, thin LTO, one codegen
unit) is 16,697,288 bytes, 15.92 MiB, and 79,928 short of the cap. It was
16,209,864 when ADR-042 was written at 14,831 lines of code; the code is
16,504 lines now, so a line has cost about 290 bytes.

ADR-042 said this record would start from ADR-025's second table, so
that table was measured again, this time on tuimex itself. In a copy of
this tree, kept outside the repository, the one function through which
tuimex reaches a dependency was made to return at once, so nothing behind
it was linked; the copy was built and not run.

| Left out | Bytes | MiB saved | In ADR-025 |
|---|---|---|---|
| Nothing | 16,697,288 | | |
| `keyring` (the Secret Service client and its D-Bus library) | 14,273,480 | 2.31 | 2.37 |
| Bundled SQLite (`rusqlite` linked against the system's library instead) | 14,588,960 | 2.01 | 2.07 |
| `ammonia` and `html2text`, with the HTML parser they share | 14,981,064 | 1.64 | not measured |
| `imap`, with its `rustls` 0.22 and tuimex's own IMAP client | 15,214,536 | 1.41 | 0.95 |
| `image` and `ratatui-image` | 15,636,392 | 1.01 | 1.00 |
| `ureq` | 16,074,696 | 0.59 | not measured |
| `lettre`'s connection, which shares its TLS with `ureq` | 16,553,928 | 0.14 | not measured |

Three of the four that were largest are the size they were; `imap`'s
figure now counts the client written over it; and reading HTML, which
ADR-025 did not leave out, stands third. None of the five can go without a
line of PLAN §10's list going with it: where the password is kept, the
index and its search, HTML mail as text and as the browser copy, the IMAP
client, attached pictures. Bryan was asked about the code cap on
2026-11-23 and said: "raise the cap if we need to, don't cut features".
That answers this cap too.

17 MiB is the measurement plus 1,128,504 bytes, 1.08 MiB. What is left
of the code budget is 1,496 lines, which at 290 bytes each is 0.41 MiB;
M10 links nothing new, because OpenPGP is the `gpg` binary (ADR-020), and
M11 is polish. The headroom is about two and a half times what the code
that may still be written should weigh, and it is not a target.

**What it costs.** The second budget to move. A dependency added by a record (ADR-005) now has this table to be
weighed against, and one the size of `keyring` would not fit.

## ADR-050 — A rename moves four things, in the order that can be stopped anywhere

An account's name is not a label. It is the directory its mail is in, the
entries the keyring keeps its sign-in under, the file its server's pinned
certificate is in, and the `name` in the config that ties those together.
`tuimex setup` has "Rename an account" in its menu, and a rename moves all
four or says which it has not.

The order is the decision. Each step finds its own work already done and
passes over it, so the same steps run again finish what a run left:

1. The sign-in is copied to the new name in the keyring. Copied, and
   first: a keyring that refuses stops the rename while the old name still
   opens everything.
2. The index is dropped. It names every message by its account, and one
   that outlived the move would name files that are gone; without it the
   next start reads the Maildir again (ADR-013).
3. The mail directory is given its new name by one `rename`, which has
   happened or has not. It is never copied: across filesystems the rename
   fails and nothing has moved. The journal is a file in that directory
   and names messages by key, folder and UID, never by account, so what
   the server is still owed is owed under the new name.
4. The pinned certificate is moved the same way.
5. The config's `name` line is changed where it stands, and each
   `[[section]]` query with the term `account:<old>`. Grants are a table
   under the account and come with it.
6. The old keyring entries are removed. One that will not go is said and
   left: nothing reads it.

A stop in steps 1 to 3 leaves everything under the old name, and the
rename is reported as not made. After step 3 the mail is under the new
name and the config may not be, which no later start could tell from an
account that has never synced. So a file in the data directory, written
before step 1 and removed after step 6, names the rename under way.
While it is there `tuimex`, `tuimex sync`, the agent command line and
`tuimex doctor` say so and open no mail, and `tuimex setup` finishes the
rename before it asks anything.

Nothing else may have the mail open while it moves. Every open store
holds a shared lock on a file in the data directory, and a rename takes
it alone or is refused. The lock is the system's and ends with the
process, so a crash leaves a file that holds nobody out.

This is the one place setup changes a line of the config, which ADR-043
said it never would; its reason was that a rewrite loses a person's
comments, and this is not one. Only the value between the quotes changes,
on lines found by reading each as TOML. The result is read back and must
be the config it was with exactly the name and those queries changed, and
pass the strict parser; otherwise nothing is written and the line to
change by hand is given. That check runs before step 1, so a config that
cannot be edited stops the rename before anything moves.

Refused before anything moves: a name setup would not give a new account,
one in use, and one that mail or a certificate is already under.

Proven by
`an_account_is_renamed_from_setup_s_menu_in_all_four_places_and_what_waited_for_the_server_still_does`,
`a_rename_stopped_after_any_step_is_all_old_or_is_finished_by_running_setup_again`,
`a_keyring_that_refuses_the_write_stops_the_rename_with_nothing_moved_and_one_that_will_not_forget_is_said`,
`a_rename_is_refused_while_another_tuimex_has_the_mail_open_and_where_the_new_name_is_taken_on_disk`,
`the_config_is_given_the_new_name_only_where_that_changes_nothing_else_and_is_otherwise_left_to_its_owner`,
`a_rename_that_was_not_finished_is_named_with_what_finishes_it`,
`nothing_opens_the_mail_while_a_rename_is_unfinished_and_each_says_what_finishes_it`,
`the_mail_is_taken_alone_only_while_no_store_is_open_and_a_store_does_not_open_while_it_is`,
and on GreenMail and Dovecot by `make real-server` (`tui-rename`). No test
reaches the system's keyring.

**What it costs.** The audit log and past log lines keep the old name:
they are history. The next start reads every message again to rebuild the
index. A lock of this kind is not kept by every network filesystem, and a
data directory on one is not protected. A config whose accounts are an
inline array, or whose name is written with an escape, is renamed by hand
and setup says how.

## ADR-051 — Releases are a workflow in this repository, built on four runners

PLAN §10 put the release workflow in M11 and said to copy pullpen's. It
is written now, with the weekly dependency check, the ruleset `main` is
to have, the two `make` targets and the Homebrew template, because the
repository is published before M10 and M11 and a public `main` with no
gate and no way to cut a release is the state those were meant to
prevent. The rest of M11 stands where it was.

`.github/workflows/release.yml` is pullpen's (its ADR-011) without
Windows (ADR-022). The argument against it is the one pullpen answered:
cargo-dist would also write installers and a tap update, in exchange for
generated code nobody here has read. Each step of this file can be read
in a minute. It runs `make check` again, because a tag can be pushed at a
commit CI never saw; refuses a tag that is not `v` and the version in
`Cargo.toml`; builds four targets; packages each as
`tuimex_<os>_<arch>.tar.gz` holding `tuimex`, `LICENSE` and `README.md`,
with no version in the name so `/releases/latest/download/` URLs hold;
writes `checksums.txt`; attests the archives and the checksums, and
publishes the bundle beside them as `attestation.jsonl`. Run by hand it
does everything but attest and publish.

The targets are the systems v1 supports on the architectures each is
sold with: `x86_64-unknown-linux-gnu` on `ubuntu-22.04`,
`aarch64-unknown-linux-gnu` on `ubuntu-22.04-arm`, and
`x86_64-apple-darwin` on `macos-latest` and `aarch64-apple-darwin` on
`macos-latest`. Bundled SQLite and `ring` are C, so every target needs a
C compiler for that target. A runner of the target's own system and
architecture has one, which is why ARM Linux is built on an ARM image and
not cross-compiled with a second toolchain and sysroot. The Ubuntu images
are the oldest offered, so the binaries start on any glibc at least that
old. GitHub has retired its Intel Mac images; Apple's clang builds both
architectures from one SDK, so both Mac targets build on Apple silicon,
on an image that is not pinned to a version that will be withdrawn.

The binaries are not signed with an Apple Developer ID and not notarised:
there is no such identity. What vouches for an archive is its attestation
(`gh attestation verify`) and `checksums.txt`.

`.github/workflows/outdated.yml` runs on Mondays at 06:31 UTC. It keeps
one issue for what `cargo update` would change and one for what `cargo
audit` reports, and opens no pull request (ADR-005). A crate pinned with
`=` is reported apart, with the record that pinned it (ADR-023).

`.github/rulesets/main.json` is what `main` requires, in the shape the
API returns it: every job of `ci.yml` by name, a pull request, linear
history, and CodeQL with no alert of high severity or worse.

Held by
`the_ruleset_requires_exactly_the_jobs_ci_has`,
`the_jobs_that_read_prose_are_not_skipped_for_prose_and_every_job_has_a_time_limit`,
`the_release_builds_exactly_the_four_targets_and_none_for_windows`,
`every_archive_the_formula_names_is_one_the_release_makes` and
`the_weekly_check_runs_when_the_plan_says`. The Linux x86-64 build, the
packaging, the checksums, the tag check, the weekly script and both
`make` targets were run by hand on 2026-11-24. The Mac and ARM builds,
the attestation, publishing and the ruleset are unproven until they first
run on GitHub.

**What it costs.** A Mac user who downloads an archive with a browser is
stopped by Gatekeeper, and has to allow the binary in System Settings or
remove its quarantine attribute; `curl` and Homebrew set none. No musl
build, so a system without glibc builds from source. No tap exists, so
the formula in `packaging/homebrew` is a template nobody can install
from yet. The code scanning rule holds every pull request until CodeQL
has run, so it must be turned on in the repository before the ruleset is
applied.

*2026-11-24, after the first runs on GitHub: three runners, not four.* The
ruleset, the Linux and ARM builds and both Mac builds have now run. One Mac
job builds both Mac targets, each still on `macos-latest`: with a job for
each, GitHub twice gave a runner to one and cancelled the other after
fifteen minutes in its queue, and a release that needs two scarce runners
at once waits twice as often as one that needs one. The Intel binary was
already a cross build, so nothing about what is built changed.

*2026-11-24, the first release: the attest and publish steps have run.*
`make release-tag` pushed `v0.1.0` at the head of `main` and run 45454590282
passed at its first attempt, attesting and publishing for the first time.
The release holds the four archives, `checksums.txt` and `attestation.jsonl`. What
verified it, by hand on Linux x86-64 that day: `tuimex_linux_amd64.tar.gz`
from its `/releases/latest/download/` link matched `checksums.txt`;
`gh attestation verify` exited 0 for it and for `checksums.txt`, naming
`https://slsa.dev/provenance/v1`, the four archives and `checksums.txt` as
subjects, and `.github/workflows/release.yml` at `refs/tags/v0.1.0`; a copy
with one byte appended failed it; and the unpacked binary printed
`tuimex 0.1.0`.
Still unproven: either Mac archive unpacked or run, the ARM Linux archive
run on an ARM machine, the Homebrew template, and `make release
VERSION=…` opening a bump pull request for a real release.

## ADR-052 — What is published about other projects' weaknesses

The review that most of the security records answer (ADR-007 to ADR-018)
found exploitable weaknesses in two other open-source mail clients, and
was written while this repository was private: project names, the commits
read, and for each finding the file and the line. None of it had been
reported to the projects' authors. Published as written it would have
handed those findings to an attacker before the people who could fix them.

So the public documents keep the lesson and lose the target. They say that
two existing terminal mail clients were reviewed, state each flaw as a
class ("a client that writes a message's HTML to a file and opens it runs
the sender's script in the reader's browser") and name the record and the
test tuimex answers it with. They do not say which project, its address,
the commit, a file, a line or a function, and they do not say which of the
two a flaw was seen in, since with two projects one answer gives the
other. The observations that are no use to an attacker are kept on the
same terms, so that nothing can be worked out by elimination. Neither
project is named anywhere in this repository, for any reason.

The weaknesses were to be reported to each author privately, with a
proposed ninety days before anything more was said in public.

*2026-11-24, the same day: no report was sent.* Bryan decided not to
contact the other projects. What stands is the removal: nothing published
here points at a weakness in either of them, and that does not depend on a
report having been made. The paragraph at the end of this record, about
saying more once reports are answered, has nothing to wait for; the details
stay out.

History files are append-only. This is the one exception: the prior-art
record and the account of how the plan came about were rewritten on
2026-11-24, and each says so in a dated note at its top. The originals are
kept by the owner, outside the published history. A test,
`nothing_published_points_at_a_weakness_in_another_project`, fails on a
source location that is not a file here, a finding numbered the way the
review numbered them, a commit hash in the history or the records, and a
link to a repository whose owner is not on its short list. It cannot name
what it guards against without publishing it, so it catches the shape of
the detail and not the two names; a reviewer still has to know them.

Whether to say more once the reports are answered or the ninety days have
passed is a later record.

**What it costs.** A reader can no longer check the review against its
sources, and has to take this project's word that the flaws were real. The
records lost the plainest form of their argument, "this project did that".
And one rule of the project, that history is not rewritten, now has an
exception that only a record explains.

## ADR-053 — `TUIMEX_KEYRING=none` turns the keyring off, and nothing turns another on

CLAUDE.md says a test never touches the real keyring. The tests that run
the binary kept that promise by giving it a session bus address nothing
listens at, which holds on Linux only: on macOS the `keyring` crate asks
the Keychain, which has no address to point elsewhere, and the macOS job
runs the whole suite.

The strongest argument against a switch is that a variable which changes
where a secret goes is a way to make a release binary keep one somewhere
worse, and a test-only build that honours it is not the binary that
ships. So the switch can only refuse. With `TUIMEX_KEYRING=none` every
call on the system keyring fails before an entry is made, with words no
keyring service says: "no keyring service could be reached
(TUIMEX_KEYRING is none, so none was tried)". There is no in-memory
keyring, no file, and no second value. Any other value refuses as well
and names itself, so a misspelling is read and not passed over. Unset,
which is every run that did not ask, the code path is the one it was. It
is compiled into every build, so the binary the tests run is the binary
that ships, and what it does there is what a machine with no keyring
service already gets (ADR-010): setup says to use a `password_cmd` and
saves nothing, and `tuimex doctor` prints the reason.

Every harness that starts the binary clears its environment and sets the
variable, and `make test` and CI set it for the tests that run in the
test process.

Proven by
`only_an_unset_switch_leaves_the_system_keyring_on_and_every_value_says_why_it_is_off`,
`a_keyring_turned_off_is_asked_nothing_by_any_call`,
`doctor_with_the_keyring_turned_off_asks_no_keyring_service_and_says_what_turned_it_off`
(which runs on macOS too), `every_harness_that_starts_the_binary_turns_the_keyring_off`,
and by `make real-server`, where setup's refusal and doctor's note are
now the switch's words.

**What it costs.** A person who exports the variable and forgets has a
tuimex that keeps no password, and is told why each time. A keyring that
works is still proven only on one in memory and by hand
(`docs/testing.md`).

## ADR-054 — The calendar is kept, in a place of its own

tuimex shows one calendar across every account, and keeps what it shows.
This replaces the sentence in ADR-021 that nothing is stored, and takes "a
calendar store" off ADR-022's list. Bryan asked for it on 2026-11-24: every
calendar and invitation in one view, by month, week, day and event; an
answer that lands in the calendar of the account the invitation came to;
meeting links that open; reminders.

The strongest argument against is the north star. tuimex is a triage
inbox, and a calendar with repeating events, time zones, a second kind of
sync and an editor is a second program about as hard as the first. For
Proton, and perhaps for Google, half of it can only be a copy tuimex
keeps (ADR-056). What survives that:

- It is built in stages that are each useful alone and each a pull request:
  see, answer, remind, edit (PLAN §10). A stage that proves too costly
  stops the ones after it, not the ones before.
- The north star is changed by this record and says so. The question a
  screen answers grows from "does this need me?" to "does this need me, and
  where do I need to be?". An invitation is mail that needs an answer, and
  the answer depends on the day it would land in.
- An event that exists only in tuimex is marked `local` wherever it is
  drawn (ADR-055). The view never implies a provider holds what it does not.

The calendar is not a section. `gc` opens it and `gm` returns to mail, and
each side keeps where it was. A section is a saved query over mail, moved
between with `h` and `l`; a grid of days needs those keys for days. Inside,
one rule carries the navigation: `Enter` goes deeper (month, day, event),
`Esc` comes back, and `h` `j` `k` `l` always move the cursor. `Tab` and
`1` `2` `3` change the view at the same date, as they change the section
in mail.

A key that mail uses does the same thing in the calendar, or nothing
(Bryan, 2026-11-24: "d doing 2 things is risky", then "all keys should
stay consistant"). `d` deletes an event as it trashes a message, so no
view is on a letter. `o` lists an event's links, the meeting's first, as
it lists a message's, and `Enter` opens one. `J` and `K` step through
events as they step through the list from the reader, and `n` and `N`
through what `/` found. `e` archives and `m` marks read in mail, so the
calendar binds neither, and editing an event is `E`. The one exception is
the one this record began with: `h` and `l` move by a day here and by a
section in mail.

The keys and every screen are
`design/tui.js`'s to state (ADR-002), and nothing is built until Bryan has
accepted that design.

ADR-024 made a draft the spec "when Bryan merges it", which was possible
when nothing drew it. Now a test holds the keymap in Rust to the design's
entry for entry, and another fails on any output of the design that is not
drawn, so a design cannot merge ahead of its code. The calendar's draft is
therefore a branch Bryan reads as rendered screens; his acceptance is
written under this record; and each milestone merges the part of the
accepted design it draws, with the Rust that draws it.

A meeting link is a link (ADR-018): `o` on an event opens it after the same
assessment, with its real host on the status line, and it is the user
asking for that one thing (ADR-008). Where an event carries several, the
one offered first is its conference property, then its URL, then a URL in
its location, then the links in its notes.

Not decided here, and each needs its own record before the stage that
meets it: how a repeating event is expanded, how a time zone is resolved
without a zone database (PLAN §1 has `chrono` for the clock only), and
whether any crate is added (ADR-005).

**What it costs.** The invariant no longer fits in one sentence about an
inbox. The code grows by about a third (ADR-059). And the second source of
truth the north star already counts as a cost gains a third: a provider's
calendar, which tuimex can read and mostly cannot write.

Nothing here is built. The wiki says so until a stage lands with its proof.

*2026-11-24: Bryan accepted the draft as drawn ("screens look good"),
after two changes of his: `d` deletes and no view is on a letter, and
every key mail uses means the same here, `h` and `l` excepted ("keep
it"). He also ruled that nothing is merged or pushed until there is a
working calendar: the milestones are built in order on one local branch,
`feat/calendar`, and reach `main` as one pull request, not one each as
PLAN §10 has it. Until the Rust draws the design, the three tests that
hold its tables to `design/tui.js` fail on that branch.*

*2026-11-24, while it was built.* Bryan left the build to run to one
pull request and gave the agent leading it the authority to change the
design where the build showed it had to ("you have authority to approve
changes to the design if needed"). One change so far: the fixture event
with a deceptive link gives its place as `bank.example.com`, not as an
address, because a place that is itself an address is a second link
(above) and the design's own drawing of that event's links shows one.
No output changed. Decided where the design was silent: an event's row
names its account only when more than one is configured, as a message's
does; narrowing to an account in the calendar leaves mail's narrowing
alone, and the reverse; `u` in the calendar undoes what was done in the
calendar and never reaches into mail; a mail command typed in the
calendar is refused and says `gm` returns to mail.

*2026-11-24, joined.* What the interface asks is carried out (PLAN §4.6),
and the wiki says what the calendar does instead of that it is not
built. Two more things decided where the design was silent, with no
output changed. An invitation that is in mail and in no calendar is drawn
as an event of the calendar an answer would put it in: the account's own
where that is written, otherwise its local one, so that such an event
carries the word `local` before it is stored anywhere. And the design
draws no cancelled event, so an event its organizer cancels is removed,
and a line on the command line and in `:messages` says so.

*2026-11-24, after Bryan used it.* Seven changes he asked for, each in
`design/tui.js` first. The agent leading the build approved them under the
authority above.

1. **F2 opens the calendar and F1 returns to mail.** Each is listed beside
   `gc` and `gm` in the keymap, so it is read exactly where its sequence
   is: the list, the reader, the calendar and an event's screen. In the
   event form, compose, the command line and a prompt `gc` and `gm` are
   letters, so there F1 and F2 do nothing. The month's hint names both
   keys, `gm F1 mail`.
2. **`h` and `l` choose on a field that takes no typed text**: Calendar,
   All day, Repeats and Reminder in the event form, and From in compose,
   which takes none either. The arrows still do. In a field that is typed
   into they are letters. The arrows are a row of their own in the keymap
   (`as h and l`), because one row holding all four keys made the help
   wider than 120 columns. Decided where the brief was silent: a form's
   hint names `h l` only while the focus is on a field they choose in. It
   used to offer `Left Right` on every field, where on most they chose
   nothing.
3. **Alt-l and Alt-h move the focus between the month's grid and the list
   beside it**, as they do between the list and the preview in mail.
   Alt-j and Alt-k are bound too, as in mail, where either key of a pair
   works wherever the preview lies; the hint names Alt-h, because the list
   is beside the grid at every width and never under it. The grid's cursor
   is dim while the list has the focus. Esc returns to the grid first. The
   focus goes back to the grid whenever another view is shown, and with no
   calendar there is no list to focus.
4. **Focused, the list moves as the day view does.** `j` `k` `J` `K` step
   through events across days and the grid's cursor follows, `Enter` opens
   the selected event, and `o` `i` `E` `d` act on it. `h` and `l` still
   move by a day and select its first event, as they do in the day view,
   which keeps the one exception this record began with. The day view has
   no `gg` or `G`, so the list has none. On a day with nothing on it no
   event is selected and `j` goes to the next there is, as in the day
   view. A day with more events than the list has rows scrolls: the
   selected event is the last row and the rows above it follow. That rule
   is in the one function both views draw their list with, so the day
   view, which could lose its selected event below the last row on a
   short terminal, scrolls the same way.
5. **What is kept local for want of a write check says so.** A new event
   or an answer kept local for an account whose calendar is read over
   CalDAV and not written (ADR-062) ends its result with `tuimex setup can
   try writing to its calendar`. Of a feed, which nothing can write, of an
   account with no calendar, and of a change to an event it is not said.
6. **In Starts and Ends only what changes is typed.** What is typed is
   merged with what the field held: a time alone keeps the date, a date
   alone keeps the time, a step is from the field's own day. `2pm` and
   `2:30pm` are times wherever a time is read; an hour alone and a time
   with no colon (`9`, `1430`) are times only in these two fields, because
   after a date four digits are its year and `gd 9` should stay an error.
   Chosen where two ways were offered: an end before the start is refused
   on save, as it already was, and is not moved to the next day, since a
   long event made by a slip of one digit is worse than a refusal that
   says why. A time typed into an event that lasts all day turns All day
   off, the reader having said it has a time. A field left empty keeps
   what it held, which is what Backspace on an untouched date now comes
   to; Esc still closes the form, as it does in compose.
7. **The week is a grid, and an event in it a block.** A faint rule down
   the left of each day and a dotted line across at each hour; the
   cursor's column a step brighter (`dim` for `faint`) with its header
   filled as the month's cursor is; today's header in the accent as in
   the month, and the half hour it is now a line in the accent across
   today's column. An event keeps one palette (ADR-003): no tint was
   added, so its block is a heavy edge in its account's colour on every
   row, a new glyph, with the title in the text's own colour and no grid
   behind it. The week no longer dims what has ended: a past week was
   grey from edge to edge, and the line of the present says what is past.
   The header's cursor dims once an event is selected. A rule parts what
   lasts all day from the hours. In sixteen colours `dim` and `faint` are
   one, so there the cursor's column is told by its header alone. Below
   120 columns it is three days, drawn the same way, and every row fits
   at each width from 80 up. Not done: a start time beside the title of
   an event one row long, which would cost most of the title at 120
   columns.

*2026-11-24, after Bryan used it again.* Two more, each in
`design/tui.js` first, approved by the agent leading the build. They
replace what items 1, 3 and 4 say of the hint and of the list.

8. **The first row of every screen is a bar of places**:
   `tuimex   F1 Mail   F2 Calendar`, the one in view marked. Bryan: "where
   is the indicator of what screen you are on. See how ../sentry
   implements f(keys)". Decided:
   - *A row of its own, as in sentry.* The argument against is the row it
     costs every screen. Sharing the row of tabs does not survive 80
     columns: mail's tabs take 70 of them with the Outbox, and more with a
     search, so the bar would fit there only as bare keys. The preview's
     thirty rows and its 45% are now counted under the bar.
   - *The place in view is drawn as the active tab is*, in the accent and
     bold. Sentry inverts it; this design fills nothing but the selected
     row, and in sixteen colours that fill is not there at all (ADR-003),
     so bold is what carries it without colour, as it does for a tab. Of
     the other place the key is the colour a hint's key is and the name is
     dim.
   - *Narrow*, which for two places is under 34 columns: the place not in
     view takes a short name (`Cal`), then is its key alone. The place in
     view and the program keep their names.
   - *Where the keys do nothing* (compose, the send summary, the event
     form, help) the bar is still drawn, so nothing moves by a row when a
     form opens, and names the places without their keys. It is the
     screen's and not the mode's: a command or a prompt typed over the
     list or the calendar leaves it as it was. The screen shown before
     any place is open (`Opening mail`) has no bar.
   - *No hint names the keys that change place.* The bar does, so the
     calendar's hints lose `gm F1 mail`, which read as one odd key, and
     the empty calendar's hint is `? help` alone. Help still lists `gc F2`
     and `gm F1`.

   *2026-11-24, later.* The program's name left the bar, and the place
   in view is bold in the ordinary foreground, not the accent, with the
   other dim, so the tabs under it are the only accent. Bryan: "clean up
   this top part a little. It's a bit crowded and tuimex is kinda lost in
   the header. Not sure it needs to be there at all."
9. **The month's list always has an event selected while it has the
   focus.** Bryan: "still not able to nav to the list on the right to nav
   events". Alt-l did arrive and `j` did step, in a terminal as in the
   tests; from a day with nothing on it the focus selected nothing, and
   only the hint showed it had moved. Now:
   - Alt-l leaves the list as it is drawn and selects the first event of
     the cursor's day, or the first listed after it. The row is filled as
     in the day view, and the grid's cursor dims and goes to that event's
     day: while the list has the focus the cursor is always on the
     selected event's day, so the status line counts that day. Alt-h,
     Alt-k and Esc leave it there.
   - `j` `k` (and `J` `K`) step through every event listed. The list
     stays where it began and scrolls by the rule of item 4, its days
     going on as far as the selection. Decided: `k` on the first event
     listed goes to the one before it, on whatever earlier day, and the
     list then begins there, since a list that stopped at its top would
     make the way back Alt-h, `h`, Alt-l. A step still looks 92 days
     either side of the cursor, so a gap longer than that ends it.
   - `h` `l` keep the exception: they are days. The brief gave an empty
     day the next event listed after it, which for `h` is the event just
     left; so each goes to the nearest day its way that has an event.
   - `t`, `gd`, `[` and `]` go to a day as in the grid; the list begins
     on it again and the first event from it on is selected. With none,
     the focus is the grid's. `I` selects the invitation it finds, and
     the list begins on its day. What is deleted, or hidden with `C`,
     leaves the selection to the next event listed; it used to leave none.
   - Where the list shows no event, Alt-l stays in the grid and says
     `the list shows no event`; with no calendar it says the same.
   - The hint is `j k move · Enter open`, then `i answer` and `o links`
     where they apply, `Alt-h grid` and `? help`. `E edit` and `d delete`
     still work and are left to help: with them the hint is 86 cells.

## ADR-055 — An event made here is a file; a provider's copy is a cache

This is ADR-013 again, for events. An event tuimex itself holds (one
accepted or created for an account whose calendar it cannot write) is one
iCalendar file under the data directory, in a directory per account, named
by a key tuimex makes and never by the event's UID, which is hostile text.
Those files are the truth. Everything fetched from a provider, and the
index over all of it, lives under the cache directory and can be deleted
and rebuilt: the feeds are fetched again and the files are read again.
Every file is made by the one module that makes files (ADR-011).

The strongest argument for SQLite alone is that there would be one thing
to keep right instead of two. But a local event has no server to restore
it, so it gets the treatment the mail gets: a file a person can read, back
up, and import elsewhere, which survives the index being wrong.

An event that is only a file here is drawn with the word `local`, in every
view and at every size. `:export` writes such events as `.ics`, which is
how one reaches a provider's own calendar: by the user importing it there.

When a provider's copy and a local file carry the same UID, they are one
event on screen. The provider's copy supplies what the organizer may have
changed; the local file supplies the answer given here, until the
provider's copy shows an answer of its own.

Deleting an event is undone with `u` (ADR-015). Nothing is removed from a
provider by a delete here unless tuimex can write that calendar.

**What it costs.** An event in two places can disagree, and the rule above
is the first of several that real feeds will ask for. A `local` event is
not on the user's phone, and the mark is the only thing that says so.

*2026-11-24, as built.* An event and the events standing in for single
days of it are one file, as a provider keeps them under one address, so
that a change to either is one write. Beside the accounts' directories is
`answers.json`: the invitations the reader ignored and the times they
proposed, which no event's file says and which are theirs, not a cache.
It is JSON, `{"ignored": [[account, uid]], "proposed": [[account, uid,
start]]}`. `:export` writes every local event into one calendar, to a
new file through the file module, and never over one. `u` reaches the
asks of this run: what each file held before an ask is kept in memory,
and after a restart nothing can be undone, as for mail (ADR-040).

## ADR-056 — A calendar is read from an address the user gave, and the address is a secret

What tuimex can do with a provider's calendar depends on what the provider
offers a program with no project registered anywhere:

- **Proton** has no calendar protocol. Bridge carries mail only. Proton
  Calendar can share a calendar by a private link that serves iCalendar,
  read-only.
- **Google** offers its API and its current CalDAV address to OAuth2 only,
  and OAuth2 needs a client registered in Google Cloud (ADR-019). Bryan,
  2026-11-24: "Does OAuth require me to setup something in gcp? if so I am
  not doing that". Google Calendar also has a secret address in iCalendar
  format, read-only. Whether its older CalDAV address still accepts an app
  password is **unverified**: nobody here has tried. It is tried once,
  by hand, against a real account, and the result is written under this
  record. If it works, CalDAV for Google gets its own record and stage. If
  not, Google is a feed, as Proton is.

So the mechanism every account can use is a feed: `tuimex setup` asks for
the address, and the engine fetches it on a thread of its own (ADR-006),
on the sync schedule and on `:sync`.

The address is a secret. Anyone holding it reads the calendar. It goes
where a password goes (ADR-010): the keyring, or a `password_cmd`, and into
no file tuimex writes. The log and the status line name the account and
the host, never the path.

ADR-008 says tuimex fetches nothing a message names. This is not that:
the address is typed by the user at setup, and no message can add or
change one. To keep it so, a fetch is `https` only, follows no redirect to
another host, is capped in size, and is the only thing fetched: nothing
the calendar itself names (an attachment, a picture, a link in an event's
notes) is requested. `ureq` does it, which PLAN §1 had for token requests
alone; that line is widened and no crate is added.

The bytes are hostile input. A shared or subscribed calendar is written by
other people, and an event from an invitation by whoever sent it. Only
`tuimex-content` parses them (ADR-007), from bytes the engine hands it.

A feed that cannot be fetched says so on the status line, with when it was
last read, as an account that cannot sync does. The events already held
stay on screen and are not presented as fresh.

**What it costs.** A feed is read-only and as late as the provider makes
it; how late each one is, nobody here has measured. An answer or an event made in
tuimex for these accounts is `local` (ADR-055) and the provider's own apps
do not know it. A leaked feed address cannot be revoked from tuimex, only
at the provider.

*2026-11-24, tried by hand.* A `PROPFIND` with depth 0 on
`https://www.google.com/calendar/dav/<address>/events/`, signed in with
the app password each account already keeps in the keyring, answered 207
for both of Bryan's Google accounts, one at gmail.com and one on a domain
Google hosts. The same request to
`https://apidata.googleusercontent.com/caldav/v2/<address>/events/`
answered 401 for both: that address wants OAuth2. So Google's older
address accepts an app password for reading a calendar's properties.
Whether it accepts a `PUT` is **unverified**: nothing was written, because
a write would have put an event in a real calendar. CalDAV for Google gets
its record and its stage (M17) on this result, and that record starts by
trying the write against a calendar made for the purpose.

## ADR-057 — An answer goes to the organizer by mail, and into the calendar of the account it came to

ADR-021 had three answers. There are five, on one question, from the
reader's card or from the event in the calendar:

- **Accept, tentative, decline** send a `REPLY` to the organizer and write
  the event, with the answer, to the calendar of the account that received
  the invitation: that provider's calendar where tuimex can write it,
  otherwise that account's local calendar (ADR-055). Never another
  account's.
- **Propose a time** sends a `COUNTER` with the new start, and the same
  proposal as a sentence in the body, because a provider may drop a
  `COUNTER` it does not handle (which ones do is unverified) and a person
  can still read a sentence. The event is shown as proposed until the
  organizer sends a new version.
- **Ignore** sends nothing. The invitation stops counting as waiting for an
  answer and is not drawn in the calendar. The mail is untouched and can be
  answered later.

The reply leaves from the account the invitation came to. The engine
already refuses a From that is not an identity of the account whose Outbox
holds the message, and "you" among the guests is that account's identity,
not any identity of any account. It waits in the Outbox for the send delay
like any mail, so `u` takes back both the reply and the calendar entry
(ADR-015).

An event made here that names guests is mailed to them as a `REQUEST`,
after the send summary lists who receives it.

A later message for the same UID with a higher `SEQUENCE` updates the
event, and a `CANCEL` strikes it. Either is believed only from the
organizer the event already names: a cancellation is mail, and mail is
hostile.

The strongest argument against writing to a calendar on accept is that
Gmail already adds an invitation to Google Calendar when it arrives, so
tuimex would hold a second copy. The two are one event on screen
(ADR-055), and for Proton there is no first copy: an invitation answered
outside Proton's apps reaches Proton Calendar by no route at all.

**What it costs.** For an account read by feed, the answer the provider's
own apps show stays "not answered" although the organizer has it. That is
said where the answer is given, in the result line: `in proton (local)`.

*2026-11-24, as built.* The result line reads `in proton, local`.

The calendar is written before the mail is handed to the Outbox, so an
answer the calendar refuses mails nobody; and when the engine then
refuses the mail, what was written is put back and the draft discarded.
`u` puts the calendar back first and then takes the mail out of the
Outbox; if the engine will not stop the mail, the calendar is made to
hold the answer again. Once the mail has been sent the answer stands and
`u` says so: taking back the entry alone would leave the organizer told
one thing and the calendar saying another.

An invitation is on the calendar from the moment its mail is read, not
from the moment it is answered. One that is in an Inbox and in no
calendar of its account is shown, unstored, until it is answered or
ignored. This is the ordinary case for Proton, whose feed holds only
what was answered in Proton's own apps, and it is what makes an
invitation "mail that needs an answer" in the view where the day is
(ADR-054). A request whose mail is marked answered and whose event has
gone was deleted by the reader, and does not come back. The work is
bounded: what each message carried, its method and UID, is kept in the
cache, so a start reads the messages that are new and the few still
waiting.

A later message is applied only to an event a local calendar holds: an
event at a provider is the provider's to update. The rule that says
whether it is believed is `invitation::later`, unchanged. Whether Gmail
has already put an invitation into the calendar by the time its mail is
read here is **unobserved**; either way the event is drawn once.

## ADR-058 — A reminder notifies while tuimex is open

PLAN §6.6 said nothing but new Inbox mail notifies. An event's reminder
now does too: its own alarm where it has one, otherwise a default lead
time. It rings the bell, puts the event and the key that joins it on the
status line, and raises a desktop notification.

Only while tuimex is running. Bryan chose this on 2026-11-24 over a
background helper. Nothing is installed, no timer is registered, and
nothing of tuimex runs after it is closed. The cost is plain: a closed
terminal reminds nobody.

The desktop notification is the system's own program for it (`notify-send`
on Linux, `osascript` on macOS), started with a fixed argument list and no
shell, as `password_cmd` is. An event's title is hostile text: it is
passed as one argument, stripped of control characters and of the markup a
notification service would render. A terminal's OSC 9 is the other route,
chosen in the config. No crate is added. A machine with neither still gets
the bell and the status line.

Reminders are turned off and on with a key and a command. The choice is
remembered in a file under the state directory, since setup only appends
to the config and nothing else writes it (ADR-043), and `[notify]` gives
the value a first run starts with. This is one switch, not the settings
screen ADR-022 leaves out. Off is shown on the status line; on is not,
since what is normal is silent.

**What it costs.** A reminder depends on a program tuimex does not ship,
and a notification service can show an event's title on a locked screen.
The title is all that is sent: no guest, place or link.

*2026-11-24, as built.* The bell is one byte on the stream the frames are
written to, so it is written after the frame that shows the reminder has
been flushed, never inside one. A calendar the reader has hidden and an
invitation they ignored remind nobody: both are the interface's to know,
and are asked of it at each look. Neither `notify-send` nor `osascript`
has been run by a test or seen to raise a notification by anyone here.

## ADR-059 — The code budget is 26,000 lines

`MAX_CODE_LINES` goes from 18,000 to 26,000. The other three budgets stand.

The count is 17,509, and the calendar (ADR-054) cannot fit in 491 lines.
Bryan's ruling of 2026-11-23 stands: "raise the cap if we need to, don't
cut features".

ADR-042 warned against exactly what this record does, a figure summed from
guesses before the design exists. The guesses are written down so that the
next record of this kind can say how wrong each was:

| What | Lines |
|---|---|
| Reading every event of a calendar and writing replies, proposals and invitations (`tuimex-content`) | 700 |
| Events, repeats, the words for a date (`tuimex-core`) | 700 |
| Event files and their index (`tuimex-store`) | 600 |
| Fetching feeds, the calendar's thread, writing an answer (`tuimex-engine`) | 600 |
| Month, week, day, event, the form, the panels, the keys (`tuimex-ui`) | 2,800 |
| Wiring, the setup step, reminders (`tuimex`) | 700 |
| Calendar fixtures (`tuimex-fixtures`) | 500 |
| OpenPGP, and what is left of M11 | 1,400 |

That is 8,000, and 17,509 with it is 25,509. The interface figure is the
least sure: the whole of mail's interface is about 4,300 lines, and the
calendar has more screens than the reader and compose together. CalDAV for
Google, if it comes (ADR-056), is not in the table and brings its own
record.

The binary budget is not raised. Nothing new is linked by ADR-054 to
ADR-058, and 8,000 lines at ADR-049's 290 bytes each is more than the
0.9 MiB that is left, so the stage that crosses it writes that record
against a measurement.

**What it costs.** The third budget record, and the first written ahead of
the code it pays for. The table is what makes it more than a number going
up: each stage's pull request states its own count beside its line here.

## ADR-060 — A repeating event is expanded by hand, for the rules people use

ADR-021 showed a recurrence as its rule in words. A calendar has to place
each occurrence on its day, so the rule is now worked out. `tuimex-core`
expands `FREQ` of daily, weekly, monthly and yearly with `INTERVAL`,
`COUNT`, `UNTIL`, `BYDAY` (with a position in monthly rules, as in "the
second Tuesday") and `BYMONTHDAY`, leaves out each `EXDATE`, and lets an
event carrying a `RECURRENCE-ID` stand in for the occurrence it names.
Expansion is asked for a range of days and returns what falls in it; it
never walks a rule from its start to the end of time.

A rule outside that set is not guessed at. The event is shown on its first
day, and says it repeats in a way tuimex does not work out.

The strongest argument against is that recurrence is where calendar
programs are wrong, and a crate (`rrule`) has already been wrong and fixed
more often than this code will be. It brings a zone database with it, and
ADR-049 left the binary 0.9 MiB of room. The set above is what the reviewed
feeds and invitations use, it is a pure function over days, and its test is
a table of rules with the days each must give, which a crate would need
too.

**What it costs.** An event with a rule outside the set appears once. That
is visible on the event and not in the month grid, where a missing
occurrence looks like a free day.

## ADR-061 — A time zone comes from the calendar, then the system, and is never guessed

PLAN §1 has `chrono` for the clock only, and no zone database is linked.
An event's time is resolved in this order:

1. UTC, or a time with no zone, which is the wall clock wherever the user
   is: no conversion.
2. The `VTIMEZONE` the calendar itself carries for that zone name, which
   `tuimex-content` already reads.
3. The system's zone files, by the zone's name, read by the engine and
   handed to the pure code as plain rules. `tuimex-content` still opens no
   file (ADR-007). A name is used only if it is made of the characters zone
   names have and resolves under the zone directory: it is hostile text and
   would otherwise be a path.
4. Neither: the time is shown as written, with the zone's name beside it,
   and is not placed on the clock of the week view as though it were local.

The local zone, to show times in, is the system's, as it is for mail.

The strongest argument against is that step 4 exists at all: a linked
database would never reach it. Calendars from Google and Proton carry
their zones, and Linux and macOS both ship the files, so step 4 is a
calendar that names a zone it does not define on a machine without zone
files. Saying so is better than a meeting shown an hour off.

**What it costs.** A reader for the system's zone-file format, and one
more thing that differs between the two platforms CI proves.

## ADR-062 — Google's calendar is CalDAV with the app password, and `roxmltree` reads the answers

ADR-056's note is the reason: Google's older address accepts the app
password an account already keeps. So an account whose calendar is
`kind = "caldav"` is read with `PROPFIND` and `REPORT` and, once writing
is proven, written with `PUT` and `DELETE`, each conditional on the entity
tag last seen so that a change made elsewhere is never overwritten
unseen. It signs in with the account's own credential; there is no second
secret, and the address is not one, so it is in the config. A feed
(ADR-056) remains the way for a provider with no such door, and for a
Google account if this door closes.

`ureq` sends the requests: it carries any method once told to allow them,
refuses anything but `https`, and is told to follow no redirect. Read in
its source at 3.5.2, not yet run.

A CalDAV answer is XML around iCalendar. `roxmltree` is added to the list
in PLAN §1, in `tuimex-content`, to read it: a read-only tree, no file or
network access, one dependency (`memchr`). It replaces a hand-written XML
reader, which is the thing this project should least write itself: a
parser for a format with entities and namespaces, facing a server's
bytes. A document type declaration is refused outright, which removes
entity expansion as a question. Its cost in the binary is not known until
it is linked; the figure goes under this record then, and if it does not
fit, the binary budget is raised by its own record against that
measurement (ADR-049).

Only what the calendar's own collection lists is fetched. An address
inside an answer that points at another host is not followed (ADR-008).

**What it costs.** One more crate parsing bytes from outside. Dependence
on an address Google documents as the old one: it is tried by hand before
each release (`docs/testing.md`), because no test server behaves as
Google does. And until the write is proven against a real calendar, the
Google accounts are read-only like the rest.

*2026-11-24, as built.* A calendar is a `[[calendar]]` table of its own
naming its account, not a table inside the account. Setup only ever adds
to the end of the config (ADR-043), and a table inside an account that is
not the last one cannot be added there. It also lets an account have more
than one. Linked but not yet called from the binary, `roxmltree` made the
release binary 49,152 bytes larger; the figure with the reader in use
goes here when the fetch is wired in.

*2026-11-24, as built.* The engine writes: `put_new` puts a new event at
`<calendar>/<name>.ics` with `If-None-Match: *`, where the name is a key
tuimex made and never an event's UID; `put_changed` and `delete` send
`If-Match` with the tag last read, and refuse before sending anything
when no tag is held. A `412` is "it changed at the provider", and nothing
held is touched; a delete answered `404` is done. An address that is not
under the calendar's own, on its own server, is refused before a request
is sent. What the scripted tests in `calendar::tests` prove is that
tuimex sends those requests and reads those answers as stated: they
answer from a script and say nothing about any server. The leg of
`make real-server` that writes to a Radicale in a container through the
real wire (`tests/caldav_server.rs`) passed on 2026-11-24 against
Radicale 3.5.4: a calendar made, an event written, read back the same,
refused a second time under the same name, changed, refused when it had
changed behind tuimex's back, answered with every other line the server
held kept, and deleted; a wrong password said as a refused sign-in; a
redirect not followed. That is the first time the real wire sent
anything. Writing to Google's calendar is **untried**: nothing has been
written there, by hand or by a test.

*2026-11-24, the write key.* "Once writing is proven" is now a key and a
check, decided by the agent leading the build, under Bryan's authority.
A `[[calendar]]` of kind `caldav` is written at
its provider only when its table says `write = true`. Left out it is
false, and every write for that account goes to its local calendar,
exactly as for a feed: a calendar is not written on a path nobody has
tried, and that is the default because the untried path is the one most
users have.

`tuimex setup` offers to find out, for a calendar it has just checked
and, from its menu, for one already in the config. It says that it will
add one event named "tuimex write test" a year from today and remove it
again, asks, does it with the same `put_new` and `delete` every other
write uses, and writes the key when both worked. A server that names no
tag for what it took is read once, so that the conditional delete has
one. Any failure is said as it happened and leaves the calendar
read-only; an event that could not be removed is named, so that its
owner can remove it.

Setup only appends (ADR-043), and a key cannot be appended to a table
that is not the last in the file. So for a calendar already there the
key is a `[[calendar]]` table of its own, holding `account` and `write`
and no `kind`, which adds to the account's table above it. The parser
takes that shape and no other second table. The cost is two ways to
write one fact; the alternative was a hand-made editor of TOML text,
which ADR-043 already declined.

The config also takes a `url` of `http://127.0.0.1:<port>/`, which the
engine already took: the binary is run against a CalDAV server in a
container by `make real-server`, and plain HTTP reaches no other host.

*2026-11-24, a refusal that is not heard.* Radicale's server answers a
wrong password without reading the request's body and closes, and about
one time in three (3 of 8 runs here) the client is told the connection
was reset before it has read the 401. tuimex then says "the connection
failed", not that the sign-in was refused. Nothing in the client can
read an answer the network has thrown away, so the test asks again when
that happens and holds the answer, once heard, to be the refused
sign-in. Whether Google's server does the same is **unobserved**. Until
it is, "the connection failed" from a calendar whose password was just
changed should be read as possibly a refused sign-in, and
`wiki/Calendar.md` says so.

*2026-11-24, the first write to Google.* It was refused by tuimex, not
by Google. Google took the `PUT` with 201, named no tag, and filed the
event under an address made from its UID; the address tuimex had asked
for answered 404, so the write check's delete missed, setup said the
calendar could not be written, and the test event stayed. A write that
is given no tag now asks the calendar for the events of the days the
event begins on, and keeps the event under the address and tag the
calendar shows for its UID, if that address is under the calendar.
With that, the write check passed on two Google accounts by hand, an
event made here was seen in Google Calendar, and one deleted there went
from here at the next read. The address is still never made from a UID
by tuimex: it is the server's, read back and checked.

## ADR-063 — The code budget is 28,000 lines

`MAX_CODE_LINES` goes from 26,000 to 28,000. The other three budgets
stand.

The count is 26,121 with the calendar joined, and OpenPGP is still to
build. ADR-059 asked that the next record of this kind say how wrong each
guess was. Counted as the Makefile counts, against the 17,509 the
calendar began from:

| What | Guessed | Built |
|---|---|---|
| Reading every event of a calendar and writing replies, proposals and invitations (`tuimex-content`) | 700 | 773 |
| Events, repeats, the words for a date (`tuimex-core`) | 700 | 953 |
| Event files and their index (`tuimex-store`) | 600 | 236 |
| Fetching feeds, the calendar's thread, writing an answer (`tuimex-engine`) | 600 | 991 |
| Month, week, day, event, the form, the panels, the keys (`tuimex-ui`) | 2,800 | 3,325 |
| Wiring, the setup step, reminders (`tuimex`) | 700 | 2,158 |
| Calendar fixtures (`tuimex-fixtures`) | 500 | 176 |
| OpenPGP, and what is left of M11 | 1,400 | not built |

The calendar came to 8,612 lines against 6,600 guessed. The engine's
figure holds CalDAV (ADR-062), which that table said it left out. The
binary's is three times its guess, and that is the one to learn from:
"wiring" was guessed as glue, and it is where an ask is carried out, kept
for `u`, mailed, refused and put back, where the invitations in mail are
found, and where setup tries a write. None of that was in any other
row.

28,000 is the count, the 1,400 still guessed for OpenPGP and the rest of
M11, and about 500 spare. Bryan's ruling of 2026-11-23 stands: "raise
the cap if we need to, don't cut features".

The binary budget is not raised here. The release binary has not been
measured since the calendar was joined, so whether it is still under
ADR-049's 17 MiB is **unverified**; if it is not, that is its own record
against a measurement, as ADR-059 said.

**What it costs.** The fourth budget record. The guess it makes, 1,400
for OpenPGP, is three records old and has been wrong in kind before.

*2026-11-24: measured; see ADR-064.*

## ADR-064 — The binary budget is 18 MiB

`MAX_BINARY_BYTES` goes from 17,825,792 to 18,874,368. The other three
budgets stand.

With the calendar joined, the stripped release binary on x86-64 Linux
(thin LTO, one codegen unit, `make budgets`) is 18,135,120 bytes, 17.30
MiB, and 309,328 over ADR-049's cap. It was 16,697,288 when that record
was written at 16,504 lines of code; the code is 26,121 lines now. So
9,617 lines and one new crate, `roxmltree` (ADR-062), cost 1,437,832
bytes: about 150 bytes a line, half of what ADR-049 measured, since the
calendar brought code of tuimex's own and almost nothing of anyone
else's. It is read and written over the `ureq` and TLS already linked.

The strongest argument against is ADR-049's own: each raise makes the
next one easier, and the cap is the only thing that makes anyone ask
what a feature weighs. The answer here is that the question was asked
and has a small answer. Nothing in the calendar is a dependency that
could be left out for size; what would come out is the feature. Bryan's
ruling of 2026-11-23 stands: "raise the cap if we need to, don't cut
features".

18 MiB is the measurement plus 739,248 bytes, 0.70 MiB. At 150 bytes a
line the 1,879 lines left under ADR-063 fit in it with room; an OpenPGP
library does not, and ADR-049 already said that crate needs a record of
its own against its own measurement.

**What it costs.** The second raise of this budget in two days. The
download is 1 MiB larger than the one released as 0.1.0, and that is
what a user pays for a calendar they may never open.

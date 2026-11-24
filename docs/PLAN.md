# tuimex — implementation plan

*tui mail exchange*

A terminal mail client for clearing an inbox across several accounts from one
vim-style interface, with a command line an agent can read mail and write
drafts through. Mail is treated as hostile input throughout.

This document is the architecture and the road. It fixes the decisions that
would otherwise stall work (stack, layout, module boundaries, order of
milestones). The reasons behind the decisions are numbered records in
[`decisions.md`](decisions.md); the aim is [`north-star.md`](north-star.md);
the working rules are [`CLAUDE.md`](../CLAUDE.md). Where this plan and the
design artifact disagree on how something *looks*, the design wins. Where
they disagree on how something *works*, this plan wins.

---

## 0. Inputs you have

| Input | What it is | How to use it |
|---|---|---|
| This file | Architecture, milestones, exit criteria | Work top to bottom. M1 to M5 need nothing else. |
| `docs/decisions.md` | The settled decisions | Read the titles every session; read a record before touching what it governs. |
| `docs/history/2026-11-23-prior-art.md` | The review of two existing mail clients that produced the security records, as published (ADR-052) | Read once. It lists the tests each finding becomes. |
| `docs/history/2026-11-23-how-we-got-here.md` | What was chosen, what is unconfirmed, what is unverified | Read once before M0. Its "nobody checked" list is M0's and M3's first risks. |
| `docs/history/2026-11-23-design-brief.md` | What the design was drafted from | History now; the artifact replaced it. |
| `design/tui.js` | The executable visual spec, drafted by an agent and accepted by Bryan on 2026-11-23 (ADR-024) | The source of every golden (§8). A look changes here first (ADR-002). |
| [pullpen](https://github.com/lampseen/pullpen) | The sibling project this scaffolding comes from | Copy its Makefile, CI, `tests/docs.rs`, release workflow and `gen-golden.mjs` shapes; adapt names and numbers. |

The name is **tuimex** everywhere: binary `tuimex`, config directory
`~/.config/tuimex/`, environment prefix `TUIMEX_`, repository
`github.com/lampseen/tuimex`.

---

## 1. Decisions

Each row is settled. The ones with a record are answered by its number.

| Decision | Choice | Record |
|---|---|---|
| Language / TUI | Rust, `ratatui` + `crossterm` | ADR-001 |
| Concurrency | Threads and channels; no async runtime | ADR-006 |
| IMAP | `imap`, pinned exactly, behind a `Remote` trait | ADR-023 |
| SMTP | `lettre`, blocking, sending the bytes `mail-builder` made less the `Bcc` header | ADR-035 |
| Message parsing and building | `mail-parser`, `mail-builder` | ADR-007 |
| HTML | `ammonia` to sanitise, `html2text` to read as text | ADR-009 |
| TLS | `rustls` in every client; certificates always verified | ADR-012 |
| Storage | Maildir++ for mail, SQLite (`rusqlite`, bundled, FTS5) for the index | ADR-013 |
| Secrets | `keyring`, or `password_cmd`; never the config | ADR-010 |
| Gmail | XOAUTH2 with a user-supplied client; token requests through `ureq` | ADR-019 |
| OpenPGP | the `gpg` binary | ADR-020 |
| Calendar | Google over CalDAV with the app password, any other provider as an iCalendar feed, both through `ureq`; events made here are files; `roxmltree` reads CalDAV's answers; repeats and zones worked out by hand; a provider's calendar is written only where `write = true` | ADR-054 to ADR-062 |
| Config | TOML at `$XDG_CONFIG_HOME/tuimex/config.toml`, or `--config`; never the current directory | ADR-011 |
| Platforms | Linux and macOS, both proven in CI | ADR-022 |
| Vocabulary | "folder" everywhere; Gmail labels are shown as folders | §4.1 |
| License | MIT | |

Crates (pin latest stable at implementation time, `imap` exactly). This list
is the dependency ceiling; adding a crate takes a record first (ADR-005):

`ratatui`, `crossterm`, `ratatui-image`, `image` (png, jpeg, gif, webp only),
`serde`, `serde_json`, `toml`, `clap`, `directories`, `thiserror`, `anyhow`,
`tracing` + `tracing-subscriber` (file log only, never stdout),
`unicode-width`, `strsim` (did-you-mean), `open`, `chrono` (clock only),
`imap`, `lettre`, `rustls` and `rustls-native-certs` (ADR-029), `mail-parser`
(with `full_encoding`, which brings `encoding_rs`: ADR-044),
`mail-builder`, `ammonia`, `html2text`, `rusqlite`, `keyring`, `zeroize`,
`ureq` (token requests, and calendars: ADR-056, ADR-062), `roxmltree`
(CalDAV's answers, in `tuimex-content`: ADR-062), `sha2`, `base64`,
`getrandom` (the last three for PKCE and fingerprints); dev only: `insta`.

Not on the list, deliberately: `tokio`, `reqwest`, `sqlx`, any browser
driver, any Lua, any syntax highlighter.

---

## 2. Repository layout

```
tuimex/
├── Cargo.toml                 # workspace; [workspace.lints.clippy] unwrap_used, expect_used = "warn"
├── clippy.toml                # allow-unwrap-in-tests, allow-expect-in-tests
├── rustfmt.toml               # as pullpen's
├── design/                    # the design artifact as delivered (before M6)
├── crates/
│   ├── tuimex/                # binary: main.rs, args, wiring, the agent commands, setup, doctor
│   ├── tuimex-core/           # domain, no I/O, no ratatui: Message, Thread, Query, Action, Event, Policy, link assessment, the Remote trait
│   ├── tuimex-content/        # hostile bytes to plain data: MIME, HTML to text, sanitiser, calendar, trackers (no I/O)
│   ├── tuimex-store/          # Maildir, the SQLite index, the journal, the one module that creates files
│   ├── tuimex-engine/         # the IMAP Remote, sync, SMTP, outbox, secrets, OAuth, gpg
│   ├── tuimex-ui/             # render and input: spans, palette, glyphs, views, keymap, command line
│   └── tuimex-fixtures/       # the design's fixture mailbox as Rust, a hostile-mail corpus, a fake Remote
├── tests/golden/              # generated from design/tui.js (§8)
├── tests/corpus/              # synthetic messages, one file each, named for what they try
├── scripts/gen-golden.mjs
├── Makefile                   # `make check`: fmt, clippy, tests, budgets
├── README.md                  # the front door: each section ends in a wiki page
├── CLAUDE.md  CONTRIBUTING.md  SECURITY.md
├── wiki/                      # the documentation for people using tuimex; copied to the GitHub wiki on merge
├── .github/
│   ├── workflows/             # ci.yml, and wiki.yml which copies wiki/ across
│   └── ISSUE_TEMPLATE/
└── docs/                      # for people changing tuimex
    ├── PLAN.md  north-star.md  decisions.md
    ├── config.example.toml    # parsed by a test
    ├── keymap.md              # generated from the keymap table in code
    ├── testing.md             # the by-hand script run against real servers before a release
    ├── launch.md              # the checklist for publishing the repository
    └── history/               # append-only; outside the prose budget
```

The wiki is where a claim about what tuimex does is made, and
`wiki/What-is-proven.md` lists each beside its test; `tests/docs.rs` holds
the wiki's links, commands, keys and test names to the code.

Unit tests live in a sibling `tests.rs` (`#[cfg(test)] mod tests;`), never
inline, so the budget counter is a filename filter (ADR-004).

Dependency direction, enforced by the manifests and asserted by a test that
reads them:

```
tuimex ──► tuimex-ui ───────────────► tuimex-core
   │                                      ▲   ▲
   ├─────► tuimex-engine ──► tuimex-store ┘   │
   │              └────────► tuimex-content ──┘
   └─────► tuimex-fixtures (also a dev-dependency of every crate)
```

`tuimex-ui` renders from fixtures with no network and no disk
(`tuimex --fixtures`), which is how all UI work and every snapshot test runs.
`tuimex-content` has no dependency that does I/O (ADR-007).

Three kinds of state, never mixed in one file:

| Kind | Where | Lost if deleted |
|---|---|---|
| What the user said | config directory: `config.toml`, pinned certificates | Settings |
| Mail | data directory: one Maildir++ per account, its `journal.jsonl`, drafts and outbox as folders | Unsynced changes, drafts |
| What tuimex worked out | cache directory: `index.sqlite`, browser copies, opened attachments | Nothing (ADR-013) |

Logs and the agent audit log go to the state directory.

---

## 3. Domain model (`tuimex-core`)

```rust
pub struct AccountId(pub String);                 // the config's account name
pub struct MessageId { pub account: AccountId, pub key: String }   // key: the Maildir unique name, stable across flag changes

pub struct Envelope {
    pub id: MessageId,
    pub folder: FolderId,
    pub date: Timestamp,
    pub from: Vec<Address>, pub to: Vec<Address>, pub cc: Vec<Address>,
    pub bcc: Vec<Address>,            // a draft's, or the Sent copy's: so a reopened draft keeps its blind copies
    pub reply_to: Vec<Address>,       // where `r` sends the answer, when the sender asked
    pub subject: String,
    pub message_id: Option<String>, pub references: Vec<String>,   // for threading
    pub flags: Flags,                 // seen, flagged, answered, draft
    pub size: u32,
    pub has_attachments: bool,
    pub security: Security,           // None | Signed | Encrypted | Both   (structure only; verification is the engine's)
    pub origin: Origin,               // Server | Local | Agent   (who wrote a draft, ADR-016)
}

pub struct Body {                     // what tuimex-content returns for one message
    pub text: String,                 // the reader's text, links replaced by [n]
    pub links: Vec<Link>,             // { n, url, shown }
    pub attachments: Vec<Attachment>, // { index, name, mime, size, inline: bool }
    pub invite: Option<Invite>,
    pub blocked_remote: Vec<String>,  // hosts the HTML wanted to load, for the notice
    pub remote_images: usize,         // how many pictures they would have served, for the notice
    pub image_hosts: Vec<String>,     // hosts the browser copy loads pictures from after a yes: what `B` names; never a tracker's
    pub loadable_images: usize,       // how many pictures that is: what `B` counts; never a tracker
    pub trackers: Vec<String>,        // named services, for the notice
    pub unsubscribe: Option<Unsubscribe>,
    pub pictures: Vec<Picture>,       // attached pictures the text shows: { index, width, height }; filled by whoever holds the bytes
}

pub enum Role { Inbox, Sent, Drafts, Archive, Trash, Junk, Outbox, Other }
pub struct Folder { pub id: FolderId, pub account: AccountId, pub name: String, pub role: Role, pub unseen: u32, pub total: u32 }

pub struct Thread { pub root: MessageId, pub members: Vec<ThreadNode> }   // by References / In-Reply-To, then subject within a window
pub fn thread(envelopes: &[Envelope]) -> Vec<Thread>;                     // pure; table-tested

pub struct Section { pub name: String, pub short: String, pub query: Query, pub sort: Sort, pub threaded: bool }
pub struct Query { /* §6.1 */ }

pub enum Action {
    Archive, Trash, Move(FolderId), Flag(bool), Seen(bool), Undo,
    Compose(Start),          // New | Reply | ReplyAll | Forward | FromDraft | MailTo
    Send, SaveDraft, Discard,
    OpenLink(usize), SaveAttachment(usize, PathBuf), OpenAttachment(usize),
    Browser { remote: bool }, Invite(Answer), Unsubscribe,
    Sync { all: bool }, EmptyTrash(AccountId),
    CreateArchive(AccountId), // the user said yes to an Archive the server lacks (§4.1, ADR-046)
    CreateTrash(AccountId),  // or to a Trash (ADR-047)
    Read,                    // the body, for the reader: answered by BodyReady, and Checked
    AgentLink(usize),        // ADR-031: answered by AgentAsked, then AgentAdvice or ActionFailed
}
pub struct Ask { pub serial: u64, pub action: Action, pub on: Vec<MessageId> }   // what the UI hands on, already shown (ADR-015, ADR-040)

pub enum Event {
    Synced { account, folder, changed: Vec<MessageId> },
    SyncError { account, msg, retry_in },      // retry_in: None when trying again cannot help
    BodyReady { id, body }, BodyFailed { id, reason },
    ActionDone { summary }, ActionFailed { serial, reason, hint },   // serial: the Ask refused, however late
    Emptied { serial, account, deleted, kept },  // what EmptyTrash did; kept: not yet numbered by the server (ADR-041)
    Saved { path },                            // an attachment written, `~` for the home directory
    Notice { account, error, text, hint },     // what answers no Ask of this run (ADR-040)
    FirstSync { account, folder, done, total },
    Sent { id, answered }, SendFailed { id, reason },   // answered: the messages it answers, now marked so
    NewMail { account, count },
    Checked { id, check },                     // what gpg said: Good | Bad | UnknownKey | Decrypted (ADR-020)
    AgentAsked { serial, replaced },           // the redacted text has left; what stood in placeholders
    AgentAdvice { serial, advice, why },       // shown beside the confirm; decides nothing (ADR-031)
}

// ADR-018
pub struct Assessment { pub url: String, pub host: String, pub warnings: Vec<Warning>, pub blocked: bool }
pub fn assess(url: &str, shown: &str) -> Assessment;

// ADR-016, ADR-030: one account's grants, one request about at most one folder
pub struct Grants { pub read: bool, pub folders: Vec<String>, pub draft: bool }
pub enum Request<'a> {
    Accounts,
    Folders { folder: Option<&'a str> }, List { folder: Option<&'a str> }, Search { folder: Option<&'a str> },
    Show { folder: &'a str }, Thread { folder: &'a str },
    Draft { reply_to: Option<&'a str> },   // the folder of the message answered
}
pub fn allows(grants: &Grants, request: &Request) -> Result<(), Denied>;
```

`MessageId.key` is what every surface uses to name a message: the UI's
cursor memory, the journal, the agent command line.

The interface owns what `u` reverses (ADR-040). It keeps the actions of a
run in order and shows each at once; `u` takes the newest back on screen and
hands on `Ask { Undo, on }` naming the messages put back. The engine
remembers what each `Ask` it carried out wrote to the journal, and reverses
the newest one over exactly those messages; a message waiting in the Outbox
goes back to Drafts whether or not this run put it there. A refusal carries
the serial of the `Ask` refused, at once or when the server is next reached,
and the interface puts back what that one `Ask` changed and drops it from
what `u` reaches. What belongs to no `Ask` of the run is a `Notice`.

---

## 4. Engine (`tuimex-engine`)

```rust
pub trait Remote: Send {
    fn folders(&mut self) -> Result<Vec<RemoteFolder>>;                    // name, SPECIAL-USE attributes, delimiter
    fn select(&mut self, folder: &str) -> Result<Selected>;                // uid_validity, uid_next, highest_modseq
    fn envelopes(&mut self, uids: UidRange) -> Result<Vec<Fetched>>;       // headers + flags + size, BODY.PEEK only
    fn flags(&mut self, since: Option<u64>) -> Result<Vec<FlagChange>>;    // CONDSTORE with a modseq; every flag without
    fn uids(&mut self) -> Result<Vec<u32>>;                                // for finding what was removed elsewhere
    fn body(&mut self, uid: u32) -> Result<Vec<u8>>;
    fn idle(&mut self, until: Deadline) -> Result<Woken>;
    fn capabilities(&self) -> &Capabilities;                               // uidplus, r#move, condstore, special_use, idle, gmail
    fn interrupt(&self) -> Interrupt;                                      // ends a blocked call from another thread (ADR-006)
}

pub trait RemoteWrite: Remote {                                            // M4
    fn store_flags(&mut self, uids: &[u32], set: Flags, clear: Flags) -> Result<()>;
    fn move_to(&mut self, uids: &[u32], target: &str) -> Result<MoveOutcome>;   // ADR-014
    fn delete(&mut self, uids: &[u32]) -> Result<bool>;                         // ADR-041; false: marked, left for the server to purge
    fn append(&mut self, folder: &str, message: &[u8], flags: Flags) -> Result<()>;
    fn find(&mut self, message_id: &str) -> Result<Vec<u32>>;                   // ADR-034
    fn create(&mut self, folder: &str) -> Result<()>;                           // ADR-046; one already there is the one asked for
}
```

The trait and its types are declared in `tuimex-core`, so that
`tuimex-fixtures` can implement it without depending on the engine.
`tuimex-fixtures` has a `FakeRemote` implementing it over an in-memory
mailbox, with a script of faults (drop the connection after N commands,
change UIDVALIDITY, refuse a move). Every sync rule in ADR-014 is a test
against it. The real implementation is tested against a server in CI (§9).

### 4.1 Mapping

| tuimex | IMAP server with SPECIAL-USE | Server without it | Gmail |
|---|---|---|---|
| Inbox | `INBOX` | `INBOX` | `INBOX` |
| Archive | `\Archive` | a folder named Archive, else created on first use after asking | `[Gmail]/All Mail` (`\All`) |
| Trash | `\Trash` | Trash / Deleted Items / Deleted Messages, else created on first use after asking | `[Gmail]/Trash` |
| Sent | `\Sent` | Sent / Sent Items / Sent Messages | `[Gmail]/Sent Mail` |
| Drafts | `\Drafts` | Drafts | `[Gmail]/Drafts` |
| Junk | `\Junk` | Junk / Spam | `[Gmail]/Spam` |
| archive a message | `UID MOVE` to Archive | copy, flag `\Deleted`, `UID EXPUNGE` (ADR-014) | `UID MOVE` to All Mail (removes the Inbox label) |
| trash a message | `UID MOVE` to Trash | as above | `UID MOVE` to `[Gmail]/Trash` |
| copy of sent mail | `APPEND` to Sent | `APPEND` to Sent | none: Gmail files it itself, and appending would duplicate |
| the same message in two folders | two messages | two messages | one message with two labels: shown once in a section, matched by `Message-ID` |

A role the mapping cannot fill is asked for in `tuimex setup` and stored in
the account's config as `folders.archive = "…"`.

An account still without an Archive is asked on the first `e`: "`<account>`
has no Archive folder. Create one on its server?" A yes makes a folder
named Archive, on the server when the first message is moved to it
(ADR-046). An account without a Trash is asked the same on the first `d`,
and a yes makes a folder named Trash the same way (ADR-047). Either is
named under the server's personal namespace, `INBOX.Archive` where the
server keeps folders under its Inbox (ADR-048). Sent is never
made: with no Sent, the message is sent, its copy stays in a Sent folder on
this machine alone, and that is said.

### 4.2 Errors

An error from a server becomes `ActionFailed { serial, reason, hint }`,
where `serial` is the `Ask` it refuses and `hint` is the next useful key; or
a `Notice` when no `Ask` of this run made the change refused (ADR-040); or
`SyncError` with backoff (1s, 2s, 4s… capped at 60s) shown on the status
line and never on the command line. Authentication failures do not retry:
they name the account and `tuimex setup`.

### 4.3 Sync

Per account: one connection that syncs, and one that idles on the Inbox. A
third opens only to fetch a body the reader is waiting for, and closes after
a minute unused. No account holds more than three.

A sync pass over a folder: select; if UIDVALIDITY differs, drop the folder
locally and start again; fetch envelopes for every UID the server has and
the Maildir does not; with
CONDSTORE ask what changed since the stored modseq, otherwise fetch every
flag; compare the UID list to find what was removed elsewhere; replay the
journal for this folder.

What is downloaded: envelopes for every message in every folder with a role,
plus folders named in `sync.folders`. Bodies for messages newer than
`sync.bodies` (thirty days by default) and for anything opened. A message
larger than `sync.max_body` arrives when opened, not before. Until then it
is its header block on disk (ADR-028).

The first sync of a large mailbox is resumable: progress is what is in the
Maildir, so a killed process loses at most one batch. While a folder is
filled for the first time each batch is followed by `FirstSync { done,
total }`, which the status line shows.

### 4.4 Sending

Compose produces a message with `mail-builder` and saves it as a draft;
sending moves it to the Outbox folder, once every address in it has parsed
and its `From` is an address of that account. After `send.delay` the engine
sends it (`lettre`, the bytes as written less the `Bcc` header, ADR-035).
When the server accepts it, a move from the Outbox to Sent is written to
the account's journal and then made: that entry is the record that the
server accepted it, and replaying it files the copy per §4.1. A failure to
send leaves the message in the Outbox with the reason on its row; a lost
connection is tried again, anything else waits to be sent again by hand.
Nothing is sent that is not a file in the Outbox first, and nothing leaves
the Outbox except through the journal, so a crash after the entry is
written cannot send twice or lose a message.

A draft is saved under the name the interface gave it and keeps that name
through every later version and into the Outbox; a version the server
holds is deleted there by name (ADR-041). The stored draft carries its
`Bcc`, so there is one build of a message and the engine withholds the
header when it sends.

### 4.5 Secrets, sign-in, certificates

`Secret` wraps `Zeroizing<String>`, has no `Display`, and prints as
`[redacted]` under `Debug`. Resolution order per account: `password_cmd`,
then the keyring entry. For Gmail the keyring holds the client secret and
the refresh token; an access token lives in memory only (ADR-019).

A pinned certificate (ADR-012) is a PEM file in the config directory named
by the account; it is added as a trust root for that account's connections
and no others.

### 4.6 The interface over the engines

The binary starts one `Engine` for each account over one store, and a
worker thread between them and the event loop. The loop's thread sends each
`Ask` to the worker and looks in one channel for what came back; while
accounts are running it wakes ten times a second to look, and draws only
when something arrived or a second passed that changes what is shown: the
countdown of a held message, the wait before a failing account is tried
again, the age of the last sync. Times are drawn in the machine's zone;
`--fixtures` and the goldens have no clock and no zone, and stay at the
design's afternoon in UTC. Reading the store, building a message and
opening a file are the worker's (ADR-006).

The list is drawn from memory: every message as a `Listed` row, read from
the index at start. An action changes the rows at once (ADR-015). After
every `Ask`, every `Synced`, every refusal and every message sent or read
the worker reads the messages concerned from the store again and hands
them over, and the store's word replaces what was shown; a reading taken
before a later `Ask` leaves that `Ask`'s messages alone. The cursor stays
on the message it was on. The engine's `ActionDone` is not shown: the
interface said what the action did when it was asked for. `:empty-trash`
is the one action whose outcome the interface cannot count, since a
message the server has not numbered stays (ADR-041): it says the emptying
is under way, and the engine's `Emptied { deleted, kept }` is then said
(`emptying_the_trash_says_how_many_were_deleted_and_what_was_kept_and_why`).

`:q` ends the loop, then the engines. A message being handed to its server
is not cut short (ADR-035); the terminal is the user's again by then and
says which account the wait is for.

**The calendar** is the worker's too (ADR-054). It reads what the calendar
store holds, joins each account's local events to its provider's copy of
them (ADR-055), adds the invitations that are in an Inbox and in no
calendar, and sends the whole calendar to the loop on a channel of its
own, with the serial of the last ask it was read after; the loop hands it
to the interface, which until then goes on showing what it changed
itself (ADR-040). A calendar thread beside the engines fetches (ADR-056,
ADR-062), and the worker reads the store again when it says a calendar
was read.

A calendar ask (`AnswerEvent`, `SaveEvent`, `DeleteEvent`, `UndoEvent`)
names its event and no message. The worker carries it out at once, on its
own thread, and waits there for a provider when there is one:

- **Where it is written.** At the provider when the account's
  `[[calendar]]` is CalDAV with `write = true`; otherwise as a file of the
  account's local calendar, `<data>/.calendars/<account>/<key>.ics`
  (ADR-055). An event and the events standing in for days of it are one
  file, or one address at the provider. A calendar that is only read is
  never changed: its event's answer is a local copy, and a delete or an
  edit of it is refused.
- **The mail.** A reply, a proposal, an invitation or a cancellation is
  built as every message is (§4.4), filed as a draft and sent through the
  account's Outbox under the ask's serial, so it waits out `send.delay`.
- **`u`.** For each ask of the run the worker keeps the text every file or
  address held before it, and the message it left in the Outbox.
  `UndoEvent` puts the text back, then takes the message out of the
  Outbox and discards it; mail's `Undo` of that message takes the calendar
  entry back with it; a refusal by the engine of the ask's `Send` puts the
  text back and discards the draft. Once the message has been sent the ask
  stands, and `UndoEvent` is refused.
- **Refusals** are `ActionFailed` naming the ask, with what to do as the
  hint: an event changed at its provider since it was read says to sync
  and do it again. `ActionDone` is never said for a calendar ask.
- **What the reader decided** and no event's file says, the invitations
  ignored and the times proposed, is `<data>/.calendars/answers.json`.
- **Invitations in mail.** At start, when mail arrives and when an
  invitation's body is read, the worker asks the index for the messages
  in an Inbox that carry a calendar part. One not looked at before is
  read: a later message for an event a local calendar holds is applied by
  `invitation::later` and said once as a `Notice`. A `REQUEST` whose
  event no calendar of its account holds, which was not ignored and whose
  mail is not marked answered, is shown on the calendar and stored
  nowhere until it is answered. What each message carried is cached
  (`<cache>/invitations.json`), so a later start reads what is new and
  what still waits.
- **Outside.** `OpenEventLink` is assessed again and opened as a
  message's link is, `AgentEventLink` is answered as `AgentLink` is, and
  `Export` writes the local events as one `.ics` through the store's file
  module.

A reminder is the loop's: once a second it asks which of the events last
handed over fall due, leaving out a calendar the interface says is hidden
and an invitation it says is ignored, and hands the first to the status
line; the bell is written after the frame that shows it (§6.6).

---

## 5. Content (`tuimex-content`)

Pure functions over bytes. No I/O (ADR-007).

| Function | Returns | Notes |
|---|---|---|
| `envelope(raw)` | `Envelope` fields | header decoding, address parsing |
| `body(raw, width)` | `Body` | prefers `text/plain`; falls back to HTML as text, marked as ADR-037 says |
| `html_to_text(html, width)` | text + links | drops hidden elements first (ADR-009, ADR-017) |
| `sanitise(html, images)` | sealed HTML for the browser copy | ADR-009; CSP header included. Layout survives: sizes, alignment and table attributes, and of a `<style>` the rules that only lay out and colour, rewritten into the page's one stylesheet. A picture left on another host is a box of its declared size, and a line above the message counts them |
| `attachment(raw, index)` | name, mime, bytes | name reduced to a safe file name |
| `invite(part)` | `Invite` | ADR-021 |
| `trackers(html)` | service names | a fixed table of hosts; 1×1 and hidden images |
| `unsubscribe(headers)` | `Unsubscribe` | `List-Unsubscribe`: `mailto` or `https` only |
| `structure(raw)` | signed / encrypted parts | for `gpg` (ADR-020); no cryptography here |

`tests/corpus/` holds one synthetic message per behaviour, named for what it
tries: `remote-img-leading-space.eml`, `remote-img-uppercase-scheme.eml`,
`css-import.eml`, `css-escaped-url.eml`, `hidden-instruction.eml`,
`link-text-names-another-host.eml`, `attachment-path-traversal.eml`,
`script-in-html.eml`, and so on. A table test lists every file with what
`body` and `sanitise` must and must not return. The prior-art record lists
the first set.

---

## 6. Behaviour

### 6.1 Sections and query syntax

A section is a saved query, as in pullpen. The tabs across the top are
sections; an account or a folder is a term in a query, not a separate mode.

```toml
[[section]]
name  = "Inbox"
short = "Inbox"
query = "in:inbox"

[[section]]
name  = "Unread"
short = "Unread"
query = "in:inbox is:unread"

[[section]]
name  = "Flagged"
short = "Flag"
query = "is:flagged"
```

Grammar (unknown terms are free text over subject, sender and body):
`in:<role|folder>`, `account:<name>`, `is:unread|read|flagged|answered|draft|signed|encrypted|agent-draft`,
`has:attachment|invite|unsubscribe`, `from:<text>`, `to:<text>`, `cc:<text>`,
`subject:<text>`, `after:<date|7d>`, `before:<date|7d>`, `larger:<size>`,
free text. `/` inside a section finds among its rows with the same grammar
while it is typed, and Enter then searches all mail with it.

Two things evaluate it, and they answer alike. The interface asks it of
the rows in memory, for the sections and for `/` as it is typed, so that
an action shows at once. The store asks it of the index, for the agent
command line and for the search Enter starts: `Action::Search` is carried
out on the store's thread and answered by `Event::Found`, so the
interface never waits on it, and its results are messages from every
account and folder, shown in place of the section until Esc. Text
matches by whole words in order, the last one possibly unfinished, in any
case; each has a test of the same cases. They differ in two places the
index is ahead: it looks through the whole body, where a row holds the
body's opening until the message is read, and it folds accents.

Defaults ship: Inbox, Unread, Flagged, Drafts, Sent, Archive. Every default
spans every account. Sections are threaded unless `threaded = false`.

### 6.2 Keymap

The keymap is a table in `tuimex-ui`; `docs/keymap.md` and the `?` overlay
are generated from it. **The design delivers the final keys.** What follows
is the brief's proposal, kept consistent with pullpen wherever the two share
an idea, so the design starts from it:

**Normal, everywhere:** `:` command line · `?` help · `q` back (quits from
the list) · `Esc` clear filter, marks, message · `h l` / `Tab S-Tab`
previous, next section · `1–9` section by position · `u` undo · `^l` redraw.

**List:** `j k` · `gg G` · `^d ^u` · `/` filter, `n N` next, previous match
· `Space` mark, `V` mark range · `Enter` open · `p` toggle preview · `e`
archive · `d` trash · `s` flag · `m` read/unread · `M` move (asks for a
folder, with completion) · `c` compose · `r` reply · `R` reply all · `f`
forward · `zo zc` unfold, fold a thread.

**Reader:** `j k ^d ^u gg G` scroll · `n b` next, previous message in the
thread · `J K` next, previous in the list · `o` links (then a number, or
`j k Enter`) · `a` attachments (then `Enter` save, `O` open) · `B` browser
copy · `e d s m M r R f` as in the list · `q Esc` back, cursor kept.

**Compose:** `Tab S-Tab` between fields · `Enter` on the body opens
`$EDITOR` (TUI suspended, restored after) · `^a` attach (path with
completion) · `^s` send summary · `^d` save draft · `Esc` asks: keep the
draft or discard.

**Calendar**, after `gc` (ADR-054), as the design delivered it: `h l`
previous, next day · `j k` a week in the month, else an event · `[ ]`
page · `J K` next, previous event · `n N` match · `I` next invitation to
answer · `t` today · `gd` go to a date · `Tab S-Tab` and `1 2 3` the view
· `Enter` deeper, `Esc` back · `/` find · `gt gT` account · `C` calendars
(then `j k`, `Space`) · `o` links · `i` answer (then `a t d p x`) · `c`
new event · `E` edit · `d` delete (a repeat asks `t` this one, `a` all) ·
`u` undo · `gn` reminders · `gm` mail. On an event: `j k ^d ^u gg G`
scroll, `zo zc`, `v` the mail it came in. In the form: `Tab S-Tab`,
`Left Right`, `Enter` notes in `$EDITOR`, `^s` save, `Esc`.
`docs/keymap.md` is the table itself.

**Command and confirm:** as pullpen's §6.2, unchanged: `Tab S-Tab` complete,
`↑ ↓` history, `^w ^u`, `Enter Esc`; in a confirm `y Enter` yes, `n Esc` no,
**any other key cancels and is discarded**.

Multi-key sequences use a pending-key buffer with a 500ms timeout. Keys are
remappable under `[keys]`.

### 6.3 Commands

| command | behaviour |
|---|---|
| `:archive` `:trash` `:flag` `:read` `:unread` `[target]` | As the keys. No confirm; `u` reverses (ADR-015). |
| `:move [target] <folder>` | Completes folder names. |
| `:compose [address]` `:reply` `:replyall` `:forward` | Open compose. |
| `:send` | The send summary, then confirm (ADR-015). |
| `:browser` | The sealed copy (ADR-009). Asks before it may load remote pictures, naming the hosts. |
| `:save <n> [path]` `:open <n>` | An attachment, to the download directory by default. Opening confirms with the file's type. |
| `:link <n>` | Open a link after assessment (ADR-018). |
| `:invite accept\|tentative\|decline\|propose\|ignore` | An answer to the invitation open in the reader or selected in the calendar (ADR-057). |
| `:calendar [month\|week\|day]` `:mail` | The calendar, where it was or in that view; back to mail (ADR-054). |
| `:goto <date>` | A date in the calendar: `2026-12-05`, `5 dec`, `fri`, `+2w`. |
| `:event [title]` | The form for a new event on the selected day. |
| `:export [path]` | Every local event as one `.ics`, in the download directory by default, never over a file (ADR-055). |
| `:notify on\|off` | Reminders (ADR-058). |
| `:unsubscribe` | Composes the `mailto` for review, or opens the `https` link after assessment. Never automatic. |
| `:agent reply [guidance]` `:agent tidy` `:agent link <n>` | Hands redacted text to the configured agent command and takes text back (ADR-031). Only when typed, or picked from the menu `A` opens; `reply` with no draft open starts the reply first. |
| `:sync [all]` | A pass over every account now. |
| `:section <name>` `:account [name]` | Switch section; show one account only, or with no name every account. `gt` and `gT` step through them. |
| `:preview auto\|right\|below\|off` | Where the preview sits; `ui.preview` is the same setting at start, `p` hides and shows it and `P` moves it. |
| `:empty-trash [account]` | Confirms with the count. Deletes by name on the server (ADR-041); `u` does not reach it. |
| `:messages` | The last 200 results and errors. |
| `:q` `:qa` | Back-or-quit; quit from anywhere. No key quits: Esc goes back. |

**Targets:** omitted means the selected row, or every marked row. A thread
row targets the whole thread unless unfolded.

**Messages:** success is past tense with details dim: `✓ Archived 3   u to
undo`. Errors start `E` and name the command. An unknown command offers the
nearest (`strsim`, Jaro-Winkler ≥ 0.8).

### 6.4 The send summary

One screen, before anything leaves, listing: the identity it sends as; every
recipient, each on its own line, grouped To / Cc / Bcc; the subject; each
attachment with its size, and the total; signed, encrypted or neither, with
any recipient who has no key named; and for a draft an agent wrote, a line
saying so. Confirm is `y`. A recipient whose domain appears nowhere in the
thread being answered is marked, because that is what a redirected reply
looks like.

### 6.5 Notices in the reader

Above the body, one line each, only when true: remote content was not loaded
(with the count of hosts and named trackers); the message is signed (good,
bad, unknown key) or encrypted; the sender's address differs from the one in
a thread it claims to continue; the message is a draft an agent wrote.

### 6.6 New mail

`NewMail` in a folder with the Inbox role rings the bell and sends an OSC 9
notification, both configurable under `[notify]`: not built. The one
thing that notifies today is an event's reminder, while tuimex is open
(ADR-058).

A reminder falls due at each of the event's own alarms, or `notify.lead`
before it when it has none; each occurrence of a repeat has its own. An
all-day event has none without an alarm, and a cancelled or declined event
has none; nor has an event of a calendar the reader has hidden, or an
invitation they ignored. The event loop looks at the clock and is handed each reminder
once; one that fell due while tuimex was closed is not raised late: after a
gap of more than five minutes, only what fell due in the last five, for an
event that has not begun. It goes on the status line, the bell is written
to the terminal once the frame that shows it has been flushed, and,
by `notify.via`, raises `notify-send`'s summary (`osascript` on macOS, an
argument to a fixed script) or an OSC 9. The title alone is sent, one
argument, with its control and direction characters gone and cut to 120
characters. A program that cannot start is said once, and reminders go on
by the bell and the status line. `gn` and `:notify on|off` switch them; the
choice is the file `reminders` under the state directory, `on` or `off`,
and `notify.events` is the value before there is one.

### 6.7 Setup and doctor

`tuimex setup` is plain prompts that work over SSH, as pullpen's is: the
address; the provider (a short built-in table: Gmail, Fastmail, iCloud,
Proton Bridge; otherwise hosts and ports by hand); how to sign in, each way
saying what it needs from the person (a password read with echo off and
stored in the keyring, which is offered first everywhere and for Gmail is
an app password; a `password_cmd`; or for Gmail its OAuth2 with the
walk-through of ADR-019); the certificate question
when the server's is not trusted (ADR-012); the folder roles the mapping
could not fill; whether an agent may read and draft, and whether `:agent`
may use the Claude CLI (ADR-031). It connects and signs in to IMAP and SMTP
before saving anything. A first launch with no config and a terminal runs
it and then opens the interface.

Setup is the one writer of the config file and only adds to it (ADR-043),
but for an account's name, which a rename changes where it stands
(ADR-050). Run again it shows a numbered menu: add another account, sign in
again to one that is there, accept that one's server's new certificate,
rename an account, or quit. Signing in and accepting leave the file alone.
A rename moves the account's mail directory, keyring entries and pinned
certificate with its name, and is refused while another tuimex has the mail
open; one that was begun and not finished is said by `tuimex`, `tuimex
sync` and `tuimex doctor`, and finished by `tuimex setup`. How TLS begins on a port typed by hand is found by trying:
from the first byte, then STARTTLS.

`tuimex doctor` checks each account end to end (connect, sign in, list, a
no-op SMTP session), the keyring, `gpg`, `$EDITOR`, the terminal's image
protocol, the `[agent]` command (found, never run) and file modes under
every directory tuimex owns, and prints what it found. It exits non-zero
when something that stops tuimex working is wrong; what is only worth
knowing is a note.

---

## 7. The agent command line

Subcommands of `tuimex`, each printing one JSON object and exiting. They
read the local store; none of them contacts a server. Freshness comes from a
running tuimex or from `tuimex sync`, which a person or a timer runs.

```
tuimex accounts
tuimex folders  [--account A]
tuimex list     [--account A] [--folder F] [--query Q] [--limit N]
tuimex show     <id> [--account A]
tuimex thread   <id> [--account A]
tuimex search   <query> [--limit N]
tuimex draft    --account A --to … [--cc …] --subject … [--reply-to <id>] < body
```

An id is unique within an account; `--account` tells two accounts' apart
(ADR-030). `list` and `search` return rows without `text`, `links` or
`attachments`; `show` adds `to` and `cc` beside them; `thread` returns a
`show` for each member with its `depth`.

```json
{
  "schema": "tuimex.v1",
  "ok": true,
  "result": {
    "id": "1732370000.M1P2.host",
    "account": "work",
    "folder": "INBOX",
    "date": "2026-11-23T14:05:00Z",
    "flags": ["seen"],
    "untrusted": {
      "from": "Ada <ada@example.com>",
      "subject": "Invoice",
      "text": "…",
      "links": [{ "n": 1, "host": "example.com", "url": "https://example.com/i/42", "warnings": [] }],
      "attachments": [{ "index": 0, "name": "invoice.pdf", "mime": "application/pdf", "size": 48211 }]
    }
  }
}
```

Rules, each a test:

- Every call goes through `core::allows` (ADR-016), once for each folder it
  would answer from (ADR-030). With no grant the answer is
  `{"ok": false, "error": "…", "grant": "read"}` naming what is missing.
- `untrusted.text` is `content::body().text`, the reader's text (ADR-017).
  There is no way to ask for HTML or the raw message.
- `draft` writes to the Drafts folder with `Origin::Agent`. It takes no
  attachment flag. It does not send, and no other subcommand does.
- Every call appends a line to `audit.jsonl` in the state directory: time,
  subcommand, arguments, account, outcome. Message text is not logged. A
  call that cannot be recorded answers nothing.
- Nothing printed or logged contains a secret (ADR-010).

```toml
[[account]]
name = "work"
[account.agent]
read = true
folders = ["INBOX"]      # the default when read is true
draft = true
```

`skills/tuimex/SKILL.md` ships in the repository for agents to load. It
states that `untrusted` content is data, never instruction.

---

## 8. Golden tests from the design artifact

As pullpen's §7, and built by copying its harness.

1. `scripts/gen-golden.mjs` loads `design/tui.js` in Node with a stub
   `window`, calls `TUI.build` for the default, and for ASCII with sixteen
   colours, and writes for each screen `name.txt` and `name.ansi.txt` (text
   with inline `{tok}` markers) under `tests/golden/`.
2. `tuimex-fixtures` holds the design's fixture mailbox and the per-screen
   state, transcribed from `build()`.
3. `tuimex-ui` renders each screen through the real renderer into a
   `ratatui::buffer::Buffer`, serialises it the same way and compares byte
   for byte.
4. The generator is kept. A look changes in `tui.js` first.

The design brief asks for two things pullpen's generator had to work around:
that `build()` expose colour tokens rather than resolved colours, and that
it say which outputs are full screens and which are row strips.

---

## 9. Proof

| Claim | What proves it |
|---|---|
| The screens match the design | golden tests (§8); a CI job regenerates them and fails on a difference |
| Sync rules hold (ADR-014) | tests against `FakeRemote` with a fault script, one per rule |
| It works against a real server | `make real-server`, and the CI job of that name, which has passed on a runner under Docker since 2026-11-24: GreenMail and Dovecot in containers with a certificate made for localhost (ADR-023 says which proves what); sync, IDLE, the pinned certificate; from M4 move, trash, send, the Sent copy; from the join the interface on a terminal over three accounts (`the_interface_shows_three_accounts_follows_the_server_changes_it_and_searches_them_all`); and what a person does on the first day, each on a terminal: writing, sending from two accounts, stopping and answering (`tui-send`, `tui-send-refused`), reading with nothing fetched and files handed to the opener (`tui-read`), an Archive and a Trash each made after asking (`tui-archive`, and again on a Dovecot that keeps every folder under `INBOX.`), `:empty-trash` (`tui-empty-trash`) |
| A long mailbox is moved through within a frame | `fifty_thousand_messages_are_drawn_and_moved_through_within_a_frame`, run by hand as its comment says: a measurement, not part of `make check` |
| A filter is typed over a long mailbox a key at a time: each key is asked of what the text before it matched, and its frame is made from the rows in sight | `a_filter_typed_a_key_at_a_time_shows_and_finds_what_asking_every_row_afresh_does` and `a_filter_narrows_the_one_before_it_only_where_it_can_match_nothing_more`; and run by hand as its comment says, `a_filter_is_typed_over_fifty_and_a_hundred_and_fifty_thousand_messages_a_key_at_a_time` |
| A key held down stops when it is let go, over a long mailbox too | `keys_that_were_waiting_together_are_all_handled_in_order_before_one_frame` and `a_frame_from_the_rows_near_the_cursor_and_what_is_kept_is_the_frame_from_them_all`; and run by hand as their comments say, `a_key_held_down_over_a_hundred_and_fifty_thousand_messages_stops_when_it_is_let_go` and `a_held_key_over_a_hundred_and_fifty_thousand_messages_is_handled_well_within_its_repeat`: measurements, not part of `make check` |
| Start says what it is doing: the screen is taken before the store is opened and says `Opening mail`, and an index of an older version is rebuilt with its count on the screen | `a_rebuild_on_open_says_how_many_messages_it_has_indexed_of_how_many`, on a pseudo-terminal `an_index_to_rebuild_is_counted_on_the_screen_before_the_list_is_drawn`, the `loading` goldens, and by hand `start_up_over_a_hundred_and_fifty_thousand_messages_with_an_index_and_with_one_to_rebuild`: on 2026-11-24, 150,000 messages of 2 KB, a current index is opened and every row read in 0.26 s, and a rebuild takes 7.0 s, all of it now counted on the screen |
| A secret reaches no file (ADR-010) | named CI job, asserted to have run |
| Only the built-in palette ships (ADR-003) | named CI job, asserted to have run |
| The content crate does no I/O (ADR-007) | source scan test, and its manifest |
| Files are owner-only and created in one place (ADR-011) | source scan test; a test that walks a scratch root after a full session and checks every mode |
| The index is a cache (ADR-013) | test: delete it, rebuild, compare |
| The agent can read and draft and nothing else (ADR-016) | table test over `allows`; a test that the binary has no subcommand outside the list |
| Agent and reader see the same text (ADR-017) | test over the corpus |
| Hostile mail is contained (ADR-008, ADR-009, ADR-018) | corpus table test (§5) |
| macOS works | CI job running the whole suite on a Mac. It has passed on `macos-latest` since 2026-11-24 (first on `main` in run 45436881760); nobody has used tuimex on a Mac by hand |
| The documents are true | `tests/docs.rs`, as pullpen's: budgets match the Makefile, every relative link resolves, every ADR number cited exists, the config example parses, no test module is inline, every crate edge is one §2 allows |
| Fixtures are synthetic | test: every address in `tests/` and `tuimex-fixtures` is under a reserved example domain |

| What is done in the calendar is carried out, kept for `u`, mailed and refused as §4.6 says (ADR-055, ADR-057) | `calendar::carry::tests` and `calendar::mail::tests`, over scratch stores, a provider in memory and a real engine on the fixtures' server; `live::tests` and `tests/terminal.rs` for the whole path from a key to a file; `make real-server` for GreenMail and Radicale together, which passed on 2026-11-24 |
| Setup saves nothing until both servers take the sign-in, and only adds to the config (ADR-043) | `setup::tests` against stand-ins; `make real-server`: a first launch on a terminal, the certificate shown and pinned, a second account, then `tuimex sync` and `tuimex doctor` with no answers |
| Gmail's sign-in (ADR-019) | `oauth::tests` and `tests/oauth.rs` against a Google played on a loopback port: the redirect, PKCE, the state, refresh, revocation; `make real-server`: XOAUTH2 over IMAP and SMTP against GreenMail, which takes the account's password as the token |
| No test reaches a keyring (ADR-053) | `every_harness_that_starts_the_binary_turns_the_keyring_off`; `doctor_with_the_keyring_turned_off_asks_no_keyring_service_and_says_what_turned_it_off`, on Linux and in the macOS job by name; `a_keyring_turned_off_is_asked_nothing_by_any_call`, asserted to have run in the `secret` job |
| `main` takes only what passed | `.github/rulesets/main.json`: `the_ruleset_requires_exactly_the_jobs_ci_has`, `the_jobs_that_read_prose_are_not_skipped_for_prose_and_every_job_has_a_time_limit`. Applied on 2026-11-24 as ruleset 28003326, after every job had passed on `main`, and equal to the file field by field that day |
| A release is the tagged commit, checked again, built for four targets, with checksums and an attestation (ADR-051) | `.github/workflows/release.yml`; `the_release_builds_exactly_the_four_targets_and_none_for_windows`, `every_archive_the_formula_names_is_one_the_release_makes`. Its dry run built and packaged all four targets and wrote their checksums on GitHub on 2026-11-24 (run 45448989344). A dry run does not attest or publish: those two steps have never run, and are unproven until the first tag |
| A dependency that moved or that an advisory names is noticed within a week | `.github/workflows/outdated.yml`, on Mondays at 06:31 UTC (`the_weekly_check_runs_when_the_plan_says`): one issue for each, never a pull request. It ran on GitHub once, started by hand on 2026-11-24 (run 45439993307), and opened the issue for updates; the audit found nothing, so no advisory issue has been opened yet, and the schedule has not fired yet |

What automation cannot cover (Gmail's own behaviour, Proton Bridge, a real
keyring) is a script in `docs/testing.md`, run by hand before a release and
recorded there with the date, as pullpen's is.

---

## 10. Milestones

Each milestone is one pull request with a checkable exit. M1 to M5 draw
nothing and need no design. `tuimex --fixtures` (offline, the fixture
mailbox, `FakeRemote`) exists from M0.

### What 0.1.0 is

Bryan's definition, 2026-11-23. The first release is cut when each line is
true. The repository is published ahead of it, and `docs/launch.md` is the
checklist for that.

| 0.1.0 means | Arrives with |
|---|---|
| Accounts are configured through a setup flow | M9 |
| Gmail connects, several accounts at once | M3, M9 (ADR-019) |
| Proton connects | M3, M9 (ADR-012) |
| Mail can be read | M3, M6, M8 |
| Mail can be archived or deleted | M4, M7 |
| A reply can be drafted without an agent | M8 |
| In compose, an agent drafts a reply from the thread or tidies what was written, when asked | M8 (ADR-031) |
| Mail can be sent from several accounts | M4, M8 |
| Links are assessed before they open | M1, M8 (ADR-018) |
| A suspicious link can be given to an agent to review, when asked; the agent never opens it | M8 (ADR-031) |
| An attachment opens in the system's program for its type | M8 |

OpenPGP (M10) and the polish and release workflow (M11) come after it.
The calendar (M12 to M17) was built ahead of both (ADR-054's notes).

### M0 — Scaffold
Repository `lampseen/tuimex`; workspace and seven crates with the edges of
§2; lints; `make check` (fmt, clippy `-D warnings`, tests, budgets) copied
from pullpen with tuimex's numbers (ADR-004); CI running it on Linux, the
suite on macOS, the named gates of §9 as they come to exist; `tests/docs.rs`;
`clap` args (`--config`, `--fixtures`, `--glyphs`, `--colors`, `--section`);
tracing to the state directory; a panic hook that restores the terminal;
README stating only what a test proves; `SECURITY.md`, `CONTRIBUTING.md`.
**Exit:** `cargo run -- --fixtures` opens a blank TUI and `q` exits cleanly.
`make check` passes. The binary is measured and ADR-004's figure is replaced
by a record.

### M1 — Content
Everything in §5, and `core::assess`, `core::thread`. The corpus and its
table test.
**Exit:** every corpus file passes. The no-I/O scan passes. Each finding in
the prior-art record marked *content* has a named test.

### M2 — Store
The Maildir (delivery through `tmp`, flags in names, `,U=` UIDs, the
UIDVALIDITY file); the one file-creating module; the index schema, built
from a Maildir; FTS5; the query grammar evaluated against it; the journal.
**Exit:** deleting the index and rebuilding gives the same query results
over the fixture mailbox. The mode walk passes. A query for every term in
§6.1 has a test.

### M3 — Engine, reading
The ADR-023 spike first. Then the `Remote` trait, the `imap` implementation,
`FakeRemote`, secrets, the pinned certificate, folder roles (§4.1), the sync
pass (§4.3), IDLE, bodies on demand, backoff.
**Exit:** `tuimex sync` against the CI server fills a Maildir; a second run
transfers nothing; a message added on the server arrives through IDLE.
Tests against `FakeRemote`: a UIDVALIDITY change, a dropped connection
mid-batch, a message removed elsewhere. The secret job passes.

### M4 — Engine, writing
Flags, move, archive, trash through the journal with replay and undo
(ADR-014, ADR-015); the Outbox and `send.delay`; SMTP; the Sent copy per
§4.1; drafts.
**Exit:** against the CI server: archive, trash, undo each, send to self and
find it in Sent once. Against `FakeRemote`: a move made offline replays on
reconnect; a refused move is undone locally; a server without UIDPLUS never
receives `EXPUNGE`; a crash between SMTP acceptance and filing does not send
twice.

### M5 — The agent command line
§7 entire: `core::allows`, the subcommands, the audit log, the skill file.
**Exit:** the table test over grants; `show` on `hidden-instruction.eml`
returns no hidden text; `draft` produces an `Origin::Agent` draft with no
attachment; the binary lists no subcommand that sends or mutates.

### M6 — Render core *(needs `design/tui.js`)*
Spans, fit, markup; palette and glyph tiers and their detection; frames in
synchronised output; the list with thread rows, tabs, status line, command
line row, preview; filter rendering; 80×24 and the ASCII, sixteen-colour
path. `gen-golden.mjs` and the fixtures.
**Exit:** the list goldens pass byte for byte. Resize is live.

### M7 — Input and modes
The modal dispatcher with the pending buffer; navigation, sections, marks,
filter, the command line with completion and history, confirm, messages,
`?`. Actions run against `FakeRemote`.
**Exit:** every command-line and flow golden passes. Every key in the table
does something. `docs/keymap.md` is generated.

### M8 — Reader and compose
The reader with notices (§6.5), links and the assessment prompt, attachments
(save, open, inline images through `ratatui-image`), the browser copy;
compose, `$EDITOR`, attach, drafts, the send summary (§6.4), reply and
forward quoting; the agent-draft banner.
**Exit:** reader, compose and summary goldens pass. The corpus's link and
attachment cases behave as their table says through the UI.

### The join — engine and interface
M4 and M7 to M8 were built side by side; this joins them. One model for
asks, undo and refusals (ADR-040); the binary's backend over real accounts
(§4.6), several accounts in one window; rows, counts and `has:` terms from
the index; first-sync progress and a failing account on the status line;
bodies on demand; `:empty-trash` (ADR-041); `--section`.
**Exit:** `make real-server` drives the interface on a terminal over two
accounts: both are listed, a message added on the server appears with no
key pressed, `e` and `u` reach the server, a refused action is put back
and said, `q` leaves with 0. Every golden still passes.

### M9 — Accounts, setup, Gmail
`tuimex setup` (§6.7); OAuth2 (ADR-019);
`tuimex doctor`; first launch.
**Exit:** `docs/testing.md` run by hand and recorded: an app-password
account and a Gmail account set up from nothing, synced, sent from, and
Proton Bridge pinned.

### M10 — OpenPGP and invitations
`gpg` for sign, encrypt, verify, decrypt (ADR-020). The invite card
(ADR-021) is built; its answers are M14's (ADR-057).
**Exit:** round trips against a scratch `GNUPGHOME` made by the test.

### M11 — Polish and release
`NO_COLOR`; mouse (click row, tab, wheel); `[keys]` remapping; notifications
(§6.6); `config.example.toml`. Written ahead of the rest, when the
repository was published (ADR-051): the release workflow with attested
artifacts and checksums, the two-step `make release` / `make release-tag`,
copied from pullpen, the weekly dependency check and a Homebrew formula
template.
**Exit:** a fresh machine: install, `tuimex setup`, read and answer a
message.

### The calendar (ADR-054)

Planned as five milestones, each a pull request; built in order on one
branch, `feat/calendar`, as one pull request (ADR-054's note), with CalDAV
(M17) and the join that carries the interface's asks out. Each milestone
below is followed by what was built. §4.6 says how the parts meet, §6 has
the keys and commands, and §11 the config.

Google's older CalDAV address accepts an app password for reading
(ADR-056's note), so CalDAV is how a Google account is read. It is
written only where `write = true` (ADR-062), which `tuimex setup` sets
once a write has been tried against that calendar.

### M12 — The calendar's design *(no Rust)*
`design/tui.js` gains the calendar: fixtures (three accounts, an invitation
not yet answered, a repeating event, an all-day event, two that overlap,
one that is `local`), the key tables, the commands, the wording, and the
screens and flows: month with the day's agenda, week, day, event, the
form, the calendars panel, the empty and failing states, and the answer,
propose, ignore, join, create, delete and reminder flows, each at 120x40
and at 80x24 in ASCII and sixteen colours. Drafted on a branch and read by
Bryan as rendered screens (ADR-024). The branch is not merged: the tests
that hold the Rust to the design would fail on screens nothing draws yet,
so each later milestone merges the part it draws (ADR-054).
**Exit:** Bryan accepts the draft, and a note under ADR-054 says so and
lists what he changed.

### M13 — See
Every event of a calendar read in `tuimex-content`; event files and their
index (ADR-055); feeds fetched from addresses kept as secrets, with the
setup step that asks for them (ADR-056); the month, week, day and event
views; `gc` and `gm`; going to a date; finding; narrowing to an account;
showing and hiding calendars; joining a meeting (ADR-054). Calendars are
read over CalDAV or as a feed (ADR-062), repeats and zones worked out as
ADR-060 and ADR-061 say. Read-only.
**Exit:** every calendar output of the design is drawn cell for cell; a
corpus of hostile calendars, one test each; the secret job finds no feed
address in any file; a feed that fails says so and keeps what it had.
*Built.* The reason a calendar could not be read is also kept for
`:messages`.

### M14 — Answer
The five answers (ADR-057), which takes the invitation half of M10: the
`REPLY` and `COUNTER` parts, the write to the receiving account's
calendar, ignore, undo, and updates and cancellations from the organizer
alone.
**Exit:** an invitation fixture answered five ways produces three valid
`REPLY` parts, one valid `COUNTER`, and for ignore nothing sent; under
`make real-server` the reply arrives from the account the invitation came
to; a cancellation from anyone but the organizer changes nothing.
*Built* (`calendar::carry::tests`, `calendar::mail::tests`). Beyond the
plan: an invitation that is in mail alone is on the calendar until it is
answered, which is every invitation of a Proton account; and the design
draws no cancelled event, so one the organizer cancels is removed and
said. The leg of `make real-server` that answers on a terminal and reads
the reply and the event back from the two servers passed on 2026-11-24.

### M15 — Remind
Reminders while open, and the switch (ADR-058).
**Exit:** on a clock the test sets, a reminder is raised once, to a
notifier that records and never to the real desktop; a hostile title
reaches it as one argument with nothing to render; off stays off across a
restart.
*Built*, with the status line and the bell (§6.6). No test runs
`notify-send` or `osascript`, and nobody has seen either raise one.

### M16 — Edit
The form; creating, changing and deleting an event, one occurrence or all;
a `REQUEST` to the guests after the send summary (ADR-057); `:export`.
**Exit:** an event made, changed and deleted round-trips through its file;
the invitation it sends is read back by M13's parser as the same event.
*Built.* The send summary is the form's own question, which names how
many guests are mailed; a cancellation is mailed when an organizer
deletes.

### M17 — CalDAV, and writing only where it was tried
A calendar read with `PROPFIND` and `REPORT` and written with conditional
`PUT` and `DELETE` (ADR-062). `write` in `[[calendar]]` is false until
`tuimex setup` has added and removed one event there, on the user's say.
**Exit:** `make real-server` writes, changes, answers and deletes an
event on Radicale through the real wire; every write for a calendar
without the key stays local.
*Built.* Writing to Google's calendar itself is untried.

---

## 11. Config reference (`docs/config.example.toml`)

```toml
[[account]]
name = "personal"
address = "you@example.com"
# identities = ["You <you@example.com>", "alias@example.com"]
imap = { host = "imap.example.com", port = 993 }   # also: starttls = true | false (left out: false on 993, true on any other port), user = "…" where it is not the address
smtp = { host = "smtp.example.com", port = 465 }   # the same keys; starttls left out: false on 465, true on any other port
# auth = "password"            # password (keyring) | command | gmail-oauth
# password_cmd = ["pass", "show", "mail/personal"]
# folders = { archive = "Archive" }   # only roles the mapping could not fill
# color = "head"              # link | head | ok | warn: this account's colour in the list, in place of its turn

[account.agent]                # absent means no agent access
read = true
folders = ["INBOX"]
draft = true

[[calendar]]                   # absent means no calendar for the account; at most one for each
account = "personal"           # an [[account]]'s name
kind = "caldav"                # caldav | feed (ADR-056, ADR-062). caldav signs in as the account's IMAP user, with its password, so not for gmail-oauth
url = "https://www.google.com/calendar/dav/you@example.com/events/"   # caldav only, required, https:// with no user@
# write = true                 # caldav only: events are written at the provider. Left out, every write for the account goes to its local calendar (ADR-062); `tuimex setup` adds it once a write has been tried
                               # a feed has no url: its address is a secret, kept in the keyring as "<account>/calendar-feed", or printed by address_cmd = ["pass", "show", "calendar/personal"] (argv, no shell, the first line)

[sync]
bodies = "30d"
max_body = "10MB"
folders = []                   # besides every folder with a role

[send]
delay = "10s"

[ui]
glyphs = "unicode"             # unicode | ascii | nerd
colors = "auto"                # auto | 16 | 256 | true
# palette = "~/.config/tuimex/palette.toml"
preview = "auto"               # auto | right | below | off: beside the list from 160 columns, else below

[notify]
bell = true                    # new mail: the bell, and
osc9 = true                    # an OSC 9 notification (§6.6)
events = true                  # whether a first run starts with reminders on; the switch then remembers (ADR-058)
lead = "10m"                   # how long before an event with no alarm of its own it is reminded: s, m or h
via = "desktop"                # desktop (notify-send; osascript on macOS) | osc9 | none

[pgp]
sign = "never"                 # never | always | ask
# key = "0xABCDEF"

[paths]
# downloads = "~/Downloads"

[keys]
# "list.archive" = "e"

[[section]]
name  = "Inbox"
short = "Inbox"
query = "in:inbox"

# [agent]                      # absent means there is no `:agent` (ADR-031)
# use = "claude"               # the Claude CLI, with arguments tuimex supplies; or `command = […]`, argv with no shell, for another program
# timeout = "120s"
```

Unknown keys are an error naming the key and the line, at start and from
`tuimex doctor`. A config that does not parse stops tuimex with that error;
it never falls back to defaults or a demo. With no config file, or one
that names no account, it stops and names `tuimex setup`.

---

## 12. Working notes for the implementing agent

- Read `CLAUDE.md`, `north-star.md` and the record titles in `decisions.md`
  first, every session.
- One pull request per milestone; its exit criteria are its checklist.
  Conventional commits; `main` takes changes only through pull requests.
- The two clients the prior-art record reviewed were references for
  behaviour and are not named in this repository (ADR-052). Do not paste
  from either. Anything ported in substance from any project is named in
  a `NOTICE` file with its licence.
- When a question is not answered here, in a record, or in the design:
  prefer what vim, ranger or mutt does, in that order, and say which in the
  pull request.
- When something here turns out to be wrong, the fix is a record, then this
  file, then the code.

# How the plan came about, 2026-11-23

> *Edited 2026-11-24, before this repository was published.* History files
> here are otherwise never changed. This one named the two mail clients that
> were reviewed and pointed at a weakness in one of them; those details are
> removed. A path on the owner's machine and a detail about his mail
> accounts went with them. The owner keeps the original. ADR-052 is the
> record of this exception.

The plan was written in one conversation between Bryan and Claude, held in
another repository. This is what that conversation settled, what it left
open, and what nobody checked, for an agent who was not there.

## The route

1. Bryan asked for a security and usability review of a third-party
   terminal mail client he had cloned. The findings are in
   [`2026-11-23-prior-art.md`](2026-11-23-prior-art.md).
2. He asked whether it was worth building on. Answer: parts of it were
   sound, parts were prototypes, and it was nowhere near his bar. Use it as
   a reference.
3. He asked about a second one. Answer: far better in substance, but
   measured rule by rule against pullpen's `CLAUDE.md` it failed most of
   the process rules (the prior-art record's last table of flaws has them).
4. He asked for a fresh client planned under his own scaffolding, held to
   the bar of pullpen, bothy and a third sibling project, and named it
   tuimex. He said the name had been searched and was free.

## Where the bar is written down

The standard is three sibling projects of Bryan's. This repository's
`CLAUDE.md` and Makefile are adapted from the first.

| Project | Where | Take from it |
|---|---|---|
| pullpen (Rust) | https://github.com/lampseen/pullpen | `CLAUDE.md`, `Makefile`, `.github/workflows/`, `crates/pullpen/tests/docs.rs`, `scripts/gen-golden.mjs`, the shape of every document here |
| bothy (Go) | https://github.com/lampseen/bothy | the origin of the records, budgets and instinct fences; `docs/decisions.md` for how a long-lived record set reads |
| a sibling project (Rust) | not published | tests that never touch real config or data (a scratch root per test process), a CI privacy scan, one writer for config files |

## What Bryan chose

Each was a question with options. The choice is in a record; the options he
turned down are here because the records mostly do not name them.

| Question | Chosen | Turned down |
|---|---|---|
| The invariant | Clear the inbox safely; the agent command line is a secondary surface | The agent command line as the product; both with equal weight |
| HTML mail | Text first, with a sanitised browser copy (ADR-009) | An own cell layout engine; rendering the message with an embedded browser engine |
| Agent command line | In v1, and the only write is a draft (ADR-016) | In v1 with send, move and delete as separate grants; not in v1 |
| Visual source of truth | Bryan delivers `design/tui.js` (ADR-002) | The agent writes `tui.js` from prose; reuse pullpen's visual system |
| Servers | IMAP/SMTP with app passwords, and Gmail by OAuth2 | Microsoft 365 by OAuth2; JMAP (both in ADR-022) |
| Storage | Maildir with a SQLite index (ADR-013) | Maildir alone; SQLite alone |
| Platforms | Linux and macOS (ADR-022) | Windows as well; Linux alone |
| In v1 | Several accounts, threading, OpenPGP, calendar invitations | (all four offered were taken) |

## What Claude chose, and Bryan has not confirmed

These are written as settled records because a plan needs an answer, but
Bryan was shown them only as a list at the end and has not replied. Build on
them; if one starts to cost more than its record admits, ask him rather than
work around it.

1. The budget figures in ADR-004 (14,000 lines, 16 MiB). Estimates.
2. No remote content inside tuimex at all, with no per-sender exception
   (ADR-008). Stricter than the option he picked described.
3. No async runtime (ADR-006).
4. The pinned alpha `imap` crate (ADR-023).
5. Gmail through a client ID the user registers (ADR-019).
6. Agent drafts carry no attachment, and an agent cannot archive or flag
   (ADR-016). A reading of "drafts only for writes".
7. No way to skip certificate checks; self-signed servers are pinned
   (ADR-012).
8. The keymap in PLAN §6.2, where `r` is reply and pullpen's is refresh. The
   design settles it.
9. The repository will be `github.com/lampseen/tuimex`. It did not exist
   when this was written.

## What nobody checked

Nothing in this repository has been compiled or run. In particular:

- **The CI server may not offer what M3's spike needs.** A job against
  GreenMail can cover sync, sending, folders, the Sent copy and IDLE. Whether
  GreenMail offers CONDSTORE, MOVE and UIDPLUS was not looked at. If it does
  not, the spike fails for the server's reasons and not the crate's; use a
  Dovecot container for those and say so in the pull request.
- **Crate details are from memory and from reading**, not from
  building: feature names, that `lettre`'s blocking transport will send
  bytes built elsewhere, that `imap` builds with rustls, that `keyring`
  works without a session bus in CI.
- **`html2text` does not drop hidden elements by itself.** ADR-009 and
  ADR-017 need a pass before it that removes them, and that pass has to read
  inline styles. Its size is not in the estimate.
- **`ammonia`'s defaults strip `cid:` and `data:` URLs.** That is how the
  prior-art record's inline pictures went missing. The sanitiser needs both
  allowed for images and nothing else.
- **One of the two reviewed clients was read in part and never built.**
  Claims about what it does are from its source and README.
- **The code estimate behind ADR-004** was a sum of guesses per crate
  (content 2,000, store 1,500, engine 3,500, core 1,500, UI 5,000, binary
  2,000), made before the design existed.

## What is not written anywhere else

- Nothing the review found had been reported to either project's author
  when this was written. (2026-11-24: none was sent; ADR-052 says what was
  done instead.)
- The clones the review read were temporary. The commits reviewed are named
  in the owner's original of the prior-art record and nowhere public.

## Added later the same day: what Bryan confirmed

By the evening of 2026-11-23 Bryan had installed a build of `main` and set
it up on three real accounts: two Gmail, one Proton Bridge. Asked
then how to treat the nine choices listed above as unconfirmed, he answered
"Accepted as built". They are confirmed as the records state them, with
these changes he made along the way:

- The budget figures (item 1) are no longer the estimates: ADR-042 raised
  the line cap on his instruction to raise a cap rather than cut a feature.
- No remote content inside tuimex (item 2) stands. The browser copy now asks
  before loading a message's remote images, each time (ADR-009's note).
- The keymap (item 8) is the design's, not PLAN §6.2's proposal, and has
  changed twice on his rulings since.
- The repository (item 9) is not created: nothing is pushed until a working
  0.1.0.

He also overturned one of his own earlier choices: the design artifact is
drafted by an agent and accepted by him (ADR-024), where the table above
records that he would deliver it.

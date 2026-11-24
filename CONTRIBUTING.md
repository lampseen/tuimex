# Contributing

Most of what you need is already written down; this file says where.

## Where things are written down

- **How to use tuimex** is the [wiki](https://github.com/lampseen/tuimex/wiki),
  whose pages live in [`wiki/`](wiki/Home.md) and are changed by pull
  request like anything else. `docs/` is for people changing the program.
- **What tuimex is for** is [`docs/north-star.md`](docs/north-star.md).
- **Why things are the way they are** is
  [`docs/decisions.md`](docs/decisions.md). A change may contradict a
  recorded decision; name which one, so the reasoning can be followed
  rather than rediscovered.
- **How it is built, and in what order** is [`docs/PLAN.md`](docs/PLAN.md).
- **The rules the code is written to** are in [`CLAUDE.md`](CLAUDE.md).

## The rules that carry most weight

- **`make check` before every commit.** Formatting, clippy with warnings
  denied, the tests, and the budgets (ADR-004). The budgets are failing
  checks, and that is their job.
- **Mail is hostile input.** Message bytes are parsed in one crate that can
  do no I/O (ADR-007), and nothing a message names is fetched or opened
  unasked (ADR-008).
- **The design leads.** A visual change starts in `design/tui.js`, then
  `make golden`, then the Rust (ADR-002).
- **A bug fix ships with a test**, and the test name carries the story.
- **Every claim names its proof.** A sentence in the README or the wiki
  about what tuimex does is listed on the wiki's
  [What is proven](wiki/What-is-proven.md) page beside the test that holds
  it, or it is listed there as unproven, or it goes.
- **A new dependency needs a record first** (ADR-005).
- **Fixtures are synthetic.** Every address in a test is under
  `example.com`, `example.org` or `example.net`, and no real message is
  ever committed.

## Running the checks

| command | does | needs |
|---|---|---|
| `make check` | formats, lints, runs every test but the server ones, and weighs the result against the budgets | Rust (stable), and Python 3, which the tests use to open a pseudo-terminal |
| `make golden` | regenerates `tests/golden/` from `design/tui.js` | Node |
| `make keymap` | rewrites `docs/keymap.md` from the keymap table | Rust |
| `make real-server` | runs the engine and the interface against GreenMail and Dovecot in containers | Rust, Python 3, `openssl`, and a container runtime: `podman` by default, or `make real-server CONTAINER=docker` |
| `make install-binary` | builds the release binary into `~/.local/bin/tuimex` | Rust |

No test touches a real mailbox, your config or your keyring: unit tests
get a scratch root under `target/`, and the server tests use containers
and throwaway accounts.

What automation cannot reach (Gmail as Google runs it, Proton Bridge, the
system keyring) is a script run by hand, [`docs/testing.md`](docs/testing.md),
with a table at its end for what a run showed.

## Commits and pull requests

**Conventional commits**: `fix(sync): …`, `docs(wiki): …`. One intent to a
commit; a refactor and a fix ship separately.

**`main` requires a pull request**, with the checks green. Open it when the
work is done, and say in it which recorded decision the change touches, if
any.

A suspected security hole goes through [`SECURITY.md`](SECURITY.md) rather
than the tracker.

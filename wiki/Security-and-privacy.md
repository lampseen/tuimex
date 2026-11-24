# Security and privacy

tuimex reads mail from strangers and holds the way in to your accounts. It
treats every message as hostile input. This page says what that buys, and
where it stops. To report a hole, use the private form in
[SECURITY.md](https://github.com/lampseen/tuimex/blob/main/SECURITY.md),
not a public issue.

Two existing terminal mail clients were reviewed before tuimex was
designed, and the rules below came out of that review.

## What a message cannot make tuimex do

| a message cannot | because |
|---|---|
| load anything | tuimex has no code that fetches what a message names: no picture, stylesheet, font, preview or tracking pixel. There is no setting that turns it on |
| run anything | HTML is read as text. The sealed copy has had scripts, event handlers, forms, frames and redirects removed, and carries a policy that forbids every request |
| open anything | a link, a file and an unsubscribe address each open only when you ask for that one, and a link that deceives is asked about first |
| drive your terminal | escape sequences are removed from a message's text, so it cannot retitle the window or write to the clipboard; so are the characters that reorder text on screen |
| write where it likes | a file's name is reduced to a plain name inside the download directory, and nothing is written over |
| send anything | sending needs your `y` on a summary of who receives what. An agent has no way to send at all |
| speak to an agent behind your back | an agent is given the reader's text, made by the same function, and nothing else of the message |

## What you can ask for, and what it costs

Three things do leave, and each only when you ask:

- **Opening a link** takes you to its site, in your browser, without its
  known tracking parameters.
- **Saying yes to a message's pictures** in the sealed copy lets those
  hosts know the message was opened, and from where. Known trackers are
  left out even then.
- **`:agent`** hands the text of a message, with names, addresses and
  numbers replaced by placeholders, to the command you configured.

## Where secrets live

A password, a Google client secret and a refresh token are kept in the
system keyring under the service `tuimex`, or printed each time by a
`password_cmd` you configured. The config file has no key that can hold
one. In memory a secret is kept in a type that wipes itself and prints as
`[redacted]`. A Google access token lives in memory only.

No secret is written to the config, the index, the log, the audit log, or
anything a command prints. A named CI job, `secret`, holds that: tests give
the engine, setup, Google's sign-in and doctor known secrets, and then
search every file written and everything said for them. The job fails if
either test did not run.

## Connections

Every connection to a mail server is TLS, and the certificate is always
checked. There is no option that turns the check off. A server that will
not start TLS on a plain connection is left before any password is sent.

A server with a certificate of its own, as Proton Bridge has, is *pinned*:
you are shown its fingerprint once, and after a yes that certificate is
the only one accepted for that account, and for no other. A changed
certificate stops the account and shows both fingerprints.

## Files

tuimex writes in four directories and nowhere else:

```
~/.config/tuimex/        config.toml, a pinned certificate, a palette
~/.local/share/tuimex/   your mail, one Maildir for each account
~/.cache/tuimex/         the index, sealed copies, a file being opened
~/.local/state/tuimex/   tuimex.log, audit.jsonl
```

Each follows its `XDG_…_HOME` variable when that is set, on macOS as on
Linux.

- Every file is created readable and writable by you alone (mode 0600), in
  a directory only you can enter (0700), by a call that fails if the file
  already exists. `tuimex doctor` walks all four and names anything wider.
- The system's shared temporary directory is never used.
- The config is never read from the current directory, so a directory you
  `cd` into cannot hand tuimex a config.
- A sealed copy and a file being opened are removed when tuimex exits, and
  at the next start if they are a day old.

The cache can be deleted at any time; the index is rebuilt from the mail.

## The audit log

Every call an outside agent makes is one line in
`~/.local/state/tuimex/audit.jsonl`: when, which subcommand, its
arguments, the account, and how it ended. It holds no message text. If the
line cannot be written, the call answers nothing.

## The limits

**The mail on disk is not encrypted by tuimex.** It is plain files that
only your user can read. Anything that runs as you can read them; disk
encryption is your system's job.

**The agent rules are a policy, not a sandbox.** They bound what the
`tuimex` command will do for whoever calls it. An agent with a shell on
your machine can read the mail directory itself. What the rules stop is
mail that tells an agent to send something, to a tool that cannot send.

**A custom `:agent` command is trusted to behave.** tuimex hands it text
and takes text back, and cannot see what it does in between. `use =
"claude"` runs the Claude CLI with its tools turned off; no test runs that
program, so that it then behaves as its help says is not proven here.

**Redaction is by pattern.** It replaces the names of the people in the
conversation, addresses, long numbers and link addresses. It does not
recognise a company's name, a street address, or what the message is
about.

**Hidden text is found by rule, and the rules are finite.** The reader and
the agent leave out an element a browser would not display: `display:
none`, `visibility: hidden`, a transparent colour, zero opacity, a zero
font size, a zero-sized box that clips, the `hidden` attribute. Text that
is technically visible is kept: white on white, a font one pixel high,
text pushed off the edge of the page. What protects you there is that the
agent sees exactly what you see, so such text is in front of you too, in
plain terminal text where no colour or size hides it.

**The link check is a few rules, not a reputation service.** It catches
text that names another host, a sign-in name in the address, a bare
address and mixed alphabets. A plausible name on a hostile site passes.
The real host is always shown; reading it is still yours to do.

**The sealed copy relies on your browser** to honour the policy at the top
of the page. The sanitiser removes what it knows; the policy is there for
what it does not.

**Text first means some mail reads badly**, and no remote content means no
pictures that were not attached. That is the trade tuimex makes, and v1
has no per-sender exception.

**Gmail's browser sign-in needs a client you register yourself.** Google
does not allow a small project to ship one for the mail scope.

---

The reasoning behind each rule is a numbered record in
[`docs/decisions.md`](https://github.com/lampseen/tuimex/blob/main/docs/decisions.md).

[Links and the browser copy](Links-and-the-browser-copy) ·
[The agent](The-agent) · [What is proven](What-is-proven)

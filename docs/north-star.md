# North star

What tuimex is, so that every milestone can be judged by whether it gets it
closer. The reasons things are the way they are live in
[`decisions.md`](decisions.md); the architecture and the road are in
[`PLAN.md`](PLAN.md).

## The invariant

tuimex exists to produce one thing: **an inbox you can clear from the
keyboard, across your accounts, where nothing a message contains can act for
you or report on you.**

The loop it serves is triage. Mail arrives in several accounts. You read it
as text, deal with each message in one key (reply, archive, trash, flag,
leave), and stop when the list is empty. An agent may read the same mail and
prepare replies, and it works under the same rule you do: a message is
something to read, never something that runs.

Every screen answers one question: *does this need me, and which key deals
with it?* The list answers it in a row, the reader in the first screenful,
and the send summary in the list of who receives what before anything leaves.
A feature that does not help answer that question, or act on the answer, does
not belong here.

The calendar is the same question asked about time: *where do I need to be,
and does this invitation fit?* It is a place beside the mail and not part of
the list (ADR-054), and the rule for a message holds for an event: it is
something to read, never something that runs.

## What that rules in

- Every account in one list, with one vocabulary. Gmail's labels and an IMAP
  server's folders are translated, not exposed (PLAN §4.1).
- Acting from the keyboard: reply, forward, archive, trash, flag, mark,
  singly or in batches. What can be undone is undone with `u` rather than
  confirmed first; what cannot (sending) is confirmed against a summary.
- HTML mail read as text: the words, the links with where they really go,
  the attachments by name. The message as its author styled it is one key
  away, in your browser, as a sanitised copy that cannot run scripts or load
  anything remote (ADR-009).
- Conversations: a reply sits with what it answers.
- Signed and encrypted mail, through the `gpg` you already have (ADR-020),
  and calendar invitations you can read and answer (ADR-021).
- One calendar across your accounts: each provider's calendar read from an
  address you give, an answer kept in the calendar of the account the
  invitation came to, and a reminder while tuimex is open (ADR-054 to
  ADR-058). Planned and not built.
- A command line for agents and scripts that can read what you allow and
  write drafts, and nothing else (ADR-016).
- Degrading honestly. Sixteen colours and ASCII glyphs carry the same
  information as truecolor and Unicode. An account that cannot sync says so
  on the status line instead of showing a stale list as if it were fresh.

## What that rules out

- Fetching anything a message points at from inside tuimex: remote images,
  tracking pixels, stylesheets, link previews (ADR-008).
- A browser engine, or a layout engine for HTML (ADR-009).
- An agent that sends, moves, deletes or flags (ADR-016).
- Passwords or tokens in the config file (ADR-010), and a switch that turns
  certificate checking off (ADR-012).
- Anything of tuimex running after it is closed: reminders stop with it
  (ADR-058).
- Composing HTML, scripting or plugins, server-side search, JMAP, Microsoft
  365 sign-in and Windows in v1 (ADR-022). The traits stay clean so that
  each can be added later by a record.

## What it costs

Stated plainly, because a document that lists only benefits is a sales
document.

**Text first means some mail reads badly.** A newsletter built from nested
tables becomes a column of text and links. The browser copy is the answer,
and it is a second step.

**No remote content means no pictures that were not attached.** Most
marketing mail shows its alt text. That is the trade the invariant makes,
and v1 offers no per-sender exception.

**The agent rules are a policy, not a sandbox.** They bound what the `tuimex`
command will do for whoever calls it. An agent with a shell on your machine
can read the Maildir with `cat`. What the rules do stop is the case that
matters most: mail that tells an agent to send something, to a tool that
cannot send.

**Two sources of truth need reconciling.** Mail lives on the server and in
the Maildir. Every way they can disagree (a UID reset, a move made offline,
a message deleted elsewhere) needs a rule and a test, and this is where most
of the engine's code goes.

**Gmail sign-in needs a client ID you register yourself** (ADR-019). Google
does not allow a small project to ship one for the mail scope without a paid
security assessment.

**A provider's calendar is mostly read, not written.** Proton offers a
program no way to write its calendar, and Google offers one only through
the same registration (ADR-056). What you answer or create in tuimex for
such an account is kept by tuimex, marked `local`, and is not on your
phone.

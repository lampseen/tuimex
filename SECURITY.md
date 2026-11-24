# Security

tuimex reads mail from strangers and holds the way in to your accounts. A
flaw that lets a message run something, load something, open something or
send something you did not ask for is a security issue. So is one that
writes a password or a token to a file.

## Reporting

Report it privately through GitHub's
[security advisory form](https://github.com/lampseen/tuimex/security/advisories/new),
not the public tracker. You will get an answer within a week.

Please do not include a real message or anyone's address in a report. A
synthetic message that shows the flaw is what the fix will be tested with:
every fixture in the repository uses `example.com`, `example.org` or
`example.net`.

No bounty. tuimex is maintained by one person.

## In scope

**The browser copy's sanitiser.** `B` writes a message's HTML to a file and
opens it in your browser. A way to get a script, an event handler, a form,
a frame, a redirect or any request past the sanitiser or the page's
content security policy is in scope (ADR-009). So is a way to make a yes
to pictures load anything but pictures from the hosts the question named,
or load a tracker.

**Anything fetched because a message named it.** tuimex itself should
request nothing a message points at (ADR-008). A message that makes it do
so is in scope.

**Link assessment.** A link that opens without its real host having been
shown, a scheme other than `http`, `https` or `mailto` that opens, or a
deceptive link that opens without the question (ADR-018).

**Hidden text reaching an agent.** Text the reader does not show a person
and the `tuimex` subcommands do show an agent (ADR-017). See the limits
below for what is not hidden by the reader's rules.

**The terminal.** A message that writes an escape sequence to the terminal,
or reorders what is displayed.

**Secrets reaching a file.** A password, a client secret or a token in the
config, the index, the log, the audit log, or anything a command prints
(ADR-010).

**File modes and paths.** A file tuimex creates that someone else can read,
a path under a shared temporary directory, an attachment's name that
writes outside the directory it was saved to, or a file written over
(ADR-011).

**TLS.** A connection made without TLS, a certificate accepted without
being checked, or a pinned certificate that is not the only one accepted
for its account (ADR-012).

**The agent policy.** A `tuimex` subcommand that reads a folder the account
did not grant, or that sends, moves, deletes or flags; a call that is not
recorded in the audit log (ADR-016). And, for `:agent`, a way for
`use = "claude"` to end up running with a tool it can open a link with
(ADR-031).

**The release pipeline.** Release archives are attested by the workflow
that builds them. A way to get an archive past `gh attestation verify`, or
to make the workflow publish something it did not build from the tagged
commit, is in scope.

## Not in scope

**That the agent rules are a policy and not a sandbox.** They bound what
the `tuimex` command does for whoever calls it. An agent with a shell on
the machine can read the mail directory itself; that is stated, not a
finding.

**A custom `:agent` command.** `command = […]` runs a program you chose.
tuimex hands it text and cannot see what it then does.

**What redaction and the hidden-text rules do not recognise.** Both work by
rule, and the wiki's
[Security and privacy](https://github.com/lampseen/tuimex/wiki/Security-and-privacy)
page lists what each misses. A new case of a known limit is a welcome
issue; a way around a rule that is claimed to hold is a security report.

**Your browser, your editor, and the program a file is opened in.** A hole
in one of them belongs to it. A way tuimex hands one of them something you
did not ask to open is in scope.

**Your mail provider, and the crates tuimex is built from.** A hole in one
belongs to its project. How tuimex uses it is in scope.

**Anything requiring root, or an attacker who already has your account.**
The mail on disk is plain files only your user can read, and tuimex cannot
defend a machine somebody already controls.

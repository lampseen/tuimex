# Troubleshooting

Start with `tuimex doctor`. It checks the config, every account's sign-in
to both of its servers, the keyring, the editor, the agent command and the
modes of tuimex's files, and each `WRONG` line says what mends it. It
changes nothing.

Inside tuimex, `:messages` keeps the last results and errors, for the one
that scrolled past.

## It will not start

**``there is no config at …/config.toml; `tuimex setup` writes one, on a
terminal, and `tuimex --fixtures` runs offline``**
tuimex was started with no config and no terminal to ask setup's questions
on, from a script or with its input redirected. Run `tuimex setup` in a
terminal.

**``the config has no account; `tuimex setup` adds one, and `tuimex
--fixtures` runs offline``**
The config file is there and names no account. tuimex never shows a demo in
place of your mail. Run `tuimex setup`.

**``the config …/config.toml`` then ``line 7: unknown key `colour` ``**
A key tuimex does not know, on that line. Fix the spelling; the keys are on
[Configuration](Configuration). The message names the key and never the
value, in case a password was typed under the wrong key.

**`--section Work: there is no section of that name`**
`--section` takes a section's name or short name as the config has it, or
one of the six defaults.

**`the mail under … is in use by another tuimex`**
Another tuimex, or a `tuimex sync`, has the mail open. Close it, or wait for
the sync to end.

**``the rename of account … to … was begun and not finished …; run `tuimex
setup`, which finishes it``**
A rename was interrupted. Run `tuimex setup`; it finishes the rename before
anything else.

## An account will not sign in

**``signing in to work failed; run `tuimex setup` ``**, from `tuimex sync`
or on the status line. The server refused the password. An app password
may have been revoked, or the provider wants one where you gave your
website password. Run `tuimex setup` and choose `Sign in again to work`.

**``Google no longer accepts the sign-in kept for work; run `tuimex setup`
to sign in again``**
The Google sign-in ended. If it happens every seven days, your Google
client is still in *Testing*: publish it, as
[Setup and accounts](Setup-and-accounts#gmails-browser-sign-in) says.

**`there is no password for work in the keyring, and no password_cmd`**
The keyring entry is gone, or this machine's keyring is not the one the
password was saved in. Run `tuimex setup` and sign in again.

**`the keyring could not be read for work: …`**, or from doctor **`the
keyring does not answer`**
No keyring service is running: usual on a server, in a container, or over
a bare SSH session. Give the account a command that prints its password
instead: `tuimex setup` offers it as the second way to sign in.

**`password_cmd for work failed: pass ended with …`**, or **`printed no
password`**
The command in the config did not produce a password. Run it yourself and
see what it says.

**`the certificate work's server presented is not the one that was
pinned`**, with both fingerprints
The server has a new certificate, which a server that signs its own makes
now and then, and which someone standing between you and it would also
present. Compare the second fingerprint with what the server itself shows,
then run `tuimex setup` and choose `Accept a new certificate for work`.

**`<server> refused the sign-in; nothing was saved`**, in setup
The address, the username or the password is not what that server takes.
Nothing was written; run setup again.

**`<server> could not be reached over TLS, at once or by STARTTLS: …`**, in
setup
The host or the port is wrong, or the server offers no TLS on it. tuimex
does not connect without TLS.

## The list looks wrong

**The status line says `work offline · retry in 16s · last synced 12:41`,
and rows say `stale`.** The account cannot reach its server. tuimex tries
again by itself, waiting longer each time up to a minute. A failure to
sign in is not retried: it names `tuimex setup`.

**A message's text says `Fetching this message.`** Its text was not on this
machine yet and is being downloaded. Offline, it arrives when the account
is reachable again.

**Search does not find a word you know is in an old message.** Search
covers text that has been downloaded: by default the last thirty days and
anything you opened. Raise `sync.bodies` in the
[configuration](Configuration).

**`E :move: work refused it · … · 3 messages back in Inbox`** The server
refused an action, for the reason given after the first dot. What the
action changed has been put back.

**`work has no Archive folder. Create one on its server?`** The first `e`
on an account with no folder for archived mail. A yes makes a folder named
Archive on the server; `d` asks the same about a Trash.

**Boxes or question marks where glyphs should be.** The terminal's font
lacks them. `tuimex --glyphs ascii` draws with plain characters.

**Colours look wrong or washed out.** tuimex goes by what the terminal
advertises. `--colors true`, `256` or `16` overrides it; see
[Where it runs](Where-it-runs).

**The screen is garbled after something else wrote to the terminal.** `^l`
redraws.

## Writing and the agent

**``the editor `nano` (from the environment) is not on the path; compose
cannot open it``**, from doctor. `$VISUAL` or `$EDITOR` names a program that
is not installed.

**A message stays in the Outbox with a reason on its row.** The server
refused it; the reason is the server's own. `Enter` opens it to change and
send again.

**`E not a command: :agent · no [agent] table in the config`**, or **`E A:
no agent is set up`** `:agent` exists only once the config has an `[agent]`
table. See [The agent](The-agent).

**`E :agent reply: the agent command took longer than 120s · nothing
changed`** The command was stopped at its `timeout`. Other endings say
`could not be started`, `exited with …` or `wrote nothing`; in every case
nothing was changed.

## An outside agent is refused

**`{"ok":false,"error":"work: read does not cover the folder Sent","grant":"read"}`**
The account's `[account.agent]` table does not grant what was asked. That
is the policy working. Widen `folders` if you mean to.

**`more than one account has a message …; name one with --account`** Two
accounts hold a message with that id.

## Something else

The log is `~/.local/state/tuimex/tuimex.log`; it never contains a password
or a token. Open an [issue](https://github.com/lampseen/tuimex/issues) with
what you did, the output of `tuimex doctor` and `tuimex --version`, and
your system and terminal. Do not paste a message's text or anyone's
address. Doctor's output names your accounts and servers: `tuimex doctor
--redact` replaces them with placeholders, and is still worth reading
before you post it.

---

[Setup and accounts](Setup-and-accounts) · [Configuration](Configuration) ·
[Where it runs](Where-it-runs)

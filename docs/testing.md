# Testing by hand

What automation cannot reach: Gmail as Google runs it, Proton Bridge, and a
keyring that is really the system's (PLAN §9). This is the script for those,
run before a release against accounts of your own, and recorded in the table
at the end.

**Nobody has run this script step by step.** One real run is in the table:
accounts were set up and synced, with nothing recorded about which steps
were followed. The script was written from the code and from the
tests that run the same paths against stand-ins: a GreenMail container, a
Google played on a loopback port, and a keyring in memory (`make
real-server`, `setup::tests`, `tests/oauth.rs`). No step below is marked as
passed, and none should be until the table has a row for it. Where what you
see differs from what a step says, the step may be what is wrong: write
down what you saw.

Use accounts you can afford to disturb. Step 5 archives a message and
step 6 sends one.

## 0. Before you start

```sh
make check                # the build under test
cargo build --release
alias tuimex=$PWD/target/release/tuimex
```

To start from nothing without losing what you have, give the run a home of
its own. tuimex puts its files in four places and nowhere else (ADR-011):

```sh
export XDG_CONFIG_HOME=$HOME/tuimex-trial/config XDG_DATA_HOME=$HOME/tuimex-trial/data \
       XDG_CACHE_HOME=$HOME/tuimex-trial/cache  XDG_STATE_HOME=$HOME/tuimex-trial/state
```

The keyring is not one of the four: it is the system's, and entries made
here stay in it. They are under the service `tuimex`, and step 8 lists them.

## 1. The system keyring

On Linux this is the Secret Service (GNOME Keyring, KWallet's, KeePassXC's);
on macOS the login keychain.

1. Type `tuimex doctor`.
   **See** a line `WRONG  reading the config …` (there is none yet) and,
   under `this machine`, `ok     the keyring answers`. If it says `the
   keyring does not answer`, with the reason and the advice about a command,
   this machine has no keyring service running: steps 2 and 3 cannot keep a
   secret here, and the route to test is the command in step 2b.

## 2. An account with an app password

Any provider that takes one: Fastmail, iCloud, or Gmail with an app password
(https://myaccount.google.com/apppasswords; it needs 2-Step Verification).

1. Type `tuimex` (with no config this runs setup; `tuimex setup` is the same
   questions).
   **See** `tuimex setup adds a mail account. Nothing is saved until its
   servers have taken the sign-in.` and then `Email address:`.
2. Type the address.
   **See** `Who provides this mailbox?` and the list, with yours offered in
   brackets after `Which` if the address is one of theirs.
3. Press Enter for the provider offered, or type its number.
4. At `A short name for this account`, press Enter or type one.
5. At `How should tuimex sign in to <address>?`, **see** under `1)` where
   the provider's app password is made, and `Which [1]:`. Press Enter.
   **See** `Password (it is not shown as you type):`.
6. Paste the app password and press Enter.
   **See** nothing appear as you type or paste. Then `Signed in to
   <imap host>:993 and found N folders.` and `Signed in to <smtp host>:<port>;
   nothing was sent.`
7. If a role has no folder (`No folder here is plainly the archive folder`),
   type the number of the folder that is it, or Enter for none.
8. Answer the two questions about an agent (`n` unless you mean to test
   that), and the one about the Claude CLI if it is asked.
   **See** `Saved <name> in …/config.toml. What signs it in is in the
   keyring, and in no file.` and then the interface, with your Inbox.
9. Type `:q` and Enter. Type `cat $XDG_CONFIG_HOME/tuimex/config.toml`.
   **See** the account with `auth = "password"`, and no password anywhere in
   the file. Type `ls -l $XDG_CONFIG_HOME/tuimex`: `config.toml` is
   `-rw-------`.

**2b. With a command instead** (the route for a machine with no keyring):
run `tuimex setup`, press Enter at `What would you like to do [1]:` (`1) Add
another account`), and at `How should tuimex sign in` type `2`, then the
program and its arguments one to a line (for example `pass`, `show`,
`mail/work`, and an empty line).
**See** the same two `Signed in` lines, and in the config `auth = "command"`
with the `password_cmd` you typed.

## 3. Gmail with Google's sign-in, two accounts

### 3a. The client, once (about ten minutes)

Google's console moves. These steps were checked against Google's own pages
on **2026-11-23**; the sources are at the end of this section. If the
console no longer matches, say so in the table and go by the console.

1. https://console.cloud.google.com → the project menu → **New project**.
   Name it anything; create it and select it.
2. Menu → **APIs & Services** → **Library** → search **Gmail API** →
   **Enable**. (Google's pages do not say IMAP needs this; they do say a
   scope cannot be picked in the next steps until its API is enabled.)
3. Menu → **Google Auth platform** → **Get started**. App name: anything.
   Support email: yours. **Audience: External.** Contact email: yours.
   Agree and create.
   - *External* and not *Personal*: Google's "Personal" audience lets only
     the account that made the client sign in, and this script signs in two.
4. **Audience** → **Test users** → **Add users**: both Gmail addresses.
5. **Data Access** → **Add or remove scopes** → choose
   `https://mail.google.com/` from the list, or enter it where the dialog
   takes a scope by hand → **Save**. This is the one scope
   Google has for IMAP and SMTP, and it is a restricted one, which is why
   the client has to be yours (ADR-019).
6. **Clients** → **Create client** → Application type **Desktop app** →
   name it → **Create**.
   **See** a dialog with the Client ID and the Client secret. **Copy both
   now**: Google shows the secret only at creation. A client made earlier
   whose secret is lost needs a new secret added.
7. While the app's status is *Testing*, Google ends every sign-in after
   seven days, and tuimex will then say `Google no longer accepts the
   sign-in kept for <account>; run tuimex setup`. To stop that, under
   **Audience** move the app's publishing status from *Testing* to *In
   production* (the name on the button was not checked). It stays
   unverified, which is fine for a client only you use; Google shows an
   "unverified app" page at sign-in either way.

### 3b. The first Gmail account

1. Type `tuimex setup`, press Enter at `What would you like to do [1]:`, and
   type the first Gmail address.
   **See** Gmail offered as the provider. Press Enter, and Enter again for
   the name.
2. At `How should tuimex sign in`, **see** `3) Sign in with Google in a
   browser`, and the app password offered as the default (`Which [1]:`).
   Type `3`.
   **See** the four console steps in short, then `Client ID:`.
3. Paste the Client ID, Enter. At `Client secret (it is not shown as you
   type):` paste the secret, Enter.
   **See** nothing echoed for the secret. Then `A browser is opening
   Google's sign-in.` and the address, and the browser at Google.
4. In the browser: choose the account, pass the "Google hasn't verified this
   app" page (Advanced → Go to …), and allow access to mail.
   **See** in the browser `Signed in. This window can be closed; tuimex has
   what it needs.` and in the terminal `Signed in to imap.gmail.com:993 and
   found N folders.` and `Signed in to smtp.gmail.com:465; nothing was
   sent.`
5. Answer the remaining questions.
   **See** `Saved gmail in …`. In the config the account has
   `auth = "gmail-oauth"` and nothing else about signing in.

### 3c. The second Gmail account, sharing the client

1. `tuimex setup` → Enter → the second Gmail address → Enter, Enter, `3`.
   **See** `Using the Google client already in the keyring.` and **no**
   `Client ID:` question. The browser opens; sign in as the second account.
   **See** the two `Signed in` lines and `Saved gmail-2 in …`.

### 3d. Over SSH, with no browser on the machine

1. From another machine: `ssh` in, set the same `XDG_` variables, and run
   `tuimex setup`; add a Gmail account again under another name (or first
   remove one from the config and the keyring).
   **See** no browser attempt, but `Open this address in a browser, on any
   machine, and sign in:` and a long address.
2. Open that address in the browser of the machine you are sitting at and
   sign in.
   **See** the browser fail to load an address beginning
   `http://127.0.0.1:`. That is expected.
3. Copy that whole address from the address bar and paste it at `That
   address, or the code alone:`.
   **See** the two `Signed in` lines.

Sources, all read on 2026-11-23: the click-path, Google Workspace's
"Create a Google Cloud project" (page dated 2026-11-10), "Enable Google
Workspace APIs" (2026-11-10), "Configure OAuth consent" (2026-09-03) and
"Create access credentials" (2026-10-12) under
https://developers.google.com/workspace/guides/ ; the scope, "OAuth 2.0
Mechanism" https://developers.google.com/workspace/gmail/imap/xoauth2-protocol
(2026-09-15) and the Gmail scopes table; the secret shown once and the
"Personal" audience, https://support.google.com/cloud/answer/15549257 and
https://support.google.com/cloud/answer/15549945 ; the seven days, and the
loopback redirect with PKCE for desktop clients,
https://developers.google.com/identity/protocols/oauth2 and
…/oauth2/native-app (both 2026-11-19). Not found on any Google page, and so
not verified: whether the Gmail API must be enabled for IMAP itself, and
whether an app password may be typed with its spaces.

## 4. Sync

1. Type `tuimex sync`.
   **See** one line for each folder of each account, such as `gmail/INBOX:
   212 changed`, and exit status 0 (`echo $?`).
2. Type it again.
   **See** `nothing new` on every line, or only what arrived in between.
3. Type `tuimex doctor`.
   **See**, for every account, `ok` for its sign-in, its IMAP line and its
   SMTP line, and at the end `nothing is wrong`. For the Gmail accounts the
   sign-in line is `Google gave an access token for the sign-in kept in the
   keyring`.

## 5. Read and archive

1. Type `tuimex`.
   **See** one list with mail from every account.
2. Move to a message you can spare with `j`/`k` and press Enter.
   **See** its text; Esc goes back.
3. On that message press `e`.
   **See** it leave the Inbox section and the status line say it was
   archived. In the provider's own web mail, **see** it gone from the Inbox:
   in the Archive folder, or for Gmail in All Mail without the Inbox label.
4. Press `u`.
   **See** it back in the list, and back in the Inbox in web mail.

## 6. Send, from each account

1. Press `c`. On the From line `Left`/`Right` choose who it is from: pick
   the first account. Write to an address of your own, a subject and a line
   of text, and press `^s`.
   **See** the send summary, naming the account it sends as and each
   recipient. Press `y`.
2. **See** the message arrive at the address you wrote to, and one copy in
   the account's Sent folder (for Gmail, exactly one: Gmail files it itself).
3. Repeat from each other account, the two Gmail accounts included: that is
   XOAUTH2 over SMTP, which no automated test runs against Google.

## 7. Proton Bridge, pinned

Bridge must be running and signed in. Its ports, username and password are
in its window under the account's **Mailbox details**; the password is
Bridge's own, not Proton's.

1. See what the certificate should be. In Bridge: **Settings → Advanced
   settings → Export TLS certificates**, into an empty folder; then
   `openssl x509 -in <that folder>/cert.pem -noout -fingerprint -sha256`.
   Note the fingerprint. (Bridge shows no fingerprint itself; the name of
   the exported file was not checked.)
2. `tuimex setup` → Enter → your Proton address.
   **See** Proton Bridge offered. Press Enter.
   **See** `Proton Bridge must be running on this computer…`, then `IMAP
   port [1143]:`.
3. Enter the two ports and the username as Bridge shows them (Enter keeps
   each default), a name, `1`, and Bridge's password.
4. **See** `127.0.0.1:1143 presents a certificate this system has no reason
   to trust …` and `SHA-256 fingerprint:` followed by **the fingerprint you
   noted in step 1**. If it differs, answer `n` and stop: record it.
5. Answer `y`.
   **See** `Signed in to 127.0.0.1:1143 …` and `Signed in to 127.0.0.1:1025;
   nothing was sent.`: the one pinned certificate has served IMAP and SMTP.
   Then `Saved proton in … Its server's certificate is beside the config.`
6. `ls $XDG_CONFIG_HOME/tuimex`: **see** `proton.pem`. `tuimex doctor`:
   **see** `its server's certificate is pinned: SHA-256 <the same>` and both
   of its servers `ok`.
7. `tuimex sync`, then read, archive and send as in steps 4 to 6.
8. A changed certificate. Put another certificate in place of
   `proton.pem` (`openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj
   /CN=localhost -keyout /dev/null -out $XDG_CONFIG_HOME/tuimex/proton.pem`),
   which is the same disagreement as Bridge having made a new one. Whether
   resetting or reinstalling Bridge makes a new certificate was not
   checked; if you do either, note what happened. Then `tuimex sync`.
   **See** it fail for proton with `the certificate proton's server
   presented is not the one that was pinned`, both fingerprints, and `run
   tuimex setup to accept the new one`.
9. `tuimex setup`, and at `What would you like to do [1]:` type the number
   beside `Accept a new certificate for proton`.
   **See** both fingerprints again, the second being the one from step 1;
   answer `y`.
   **See** `The certificate is now the one trusted for proton.`, and
   `tuimex sync` work again.

## 7b. Renaming an account

With no tuimex open on the account:

1. `tuimex setup`, and at `What would you like to do [1]:` type the number
   beside `Rename an account`; choose the account if asked, and type a new
   name.
   **See** `This renames <old> to <new> in four places: …` and `Go ahead?
   [y/N]`. Answer `y`.
   **See** `<old> is now <new>.`
2. `tuimex sync`: **see** no question, and every line end `nothing new`:
   no mail was downloaded again. `tuimex doctor`: **see** the account under
   its new name with both servers `ok`.
3. In the keyring's own program: **see** the account's entry under the new
   name and none under the old. This is the one part no test reaches.
4. With `tuimex` open in another terminal, try the rename again.
   **See** it refused: `… is in use by another tuimex: close the tuimex that
   is open, or wait for tuimex sync to end, and rename then`.

## 8. What is left behind

1. `tuimex doctor`: **see** under `files` that everything under the four
   directories is readable by you alone, and `nothing is wrong`.
2. `grep -r` each secret you typed (an app password, the client secret)
   under `$HOME/tuimex-trial`: **see** no match.
3. In the keyring's own program (Seahorse / "Passwords and Keys", or
   Keychain Access): **see** under `tuimex` one entry for each account that
   signs in with a password, `<name>/refresh-token` for each Gmail account,
   and `oauth/google/client-id` and `oauth/google/client-secret` once.
   Delete them when the trial is over, and remove `$HOME/tuimex-trial`.

## The record

One row for each run of each section. "Result" is `passed`, or what
happened in place of what the step says.

| Date | Build (`git rev-parse --short HEAD`) | System | Section | Provider | Result |
|---|---|---|---|---|---|
| 2026-11-23 | not recorded | Linux | setup and sync only; not run as this script | Gmail, two accounts; Proton, one | Bryan reported that all three synced with no issue, and that the setup prompts were confusing. Which way each signed in, sending, and every other step: not recorded. |

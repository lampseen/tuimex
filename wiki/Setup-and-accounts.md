# Setup and accounts

`tuimex setup` adds a mail account by asking plain questions on the
terminal, which work over SSH. It connects and signs in to the account's
IMAP server (where mail is read) and its SMTP server (where mail is sent)
before it saves anything, so an account that is in the config is one that
worked. A first `tuimex` with no config runs it for you.

## The questions

| it asks | what to answer |
|---|---|
| `Email address:` | the address of the mailbox |
| `Who provides this mailbox?` | Gmail, Fastmail, iCloud or Proton Bridge, whose servers tuimex knows; or `Someone else`, and you type the IMAP and SMTP servers' names and ports from your provider's help pages. Yours is offered when the address is one of theirs |
| `A short name for this account, to tell it from others in tuimex` | Enter for the one offered, such as `gmail`, or your own: `work`, `home`. It is what the list shows on each row |
| `How should tuimex sign in to <address>?` | see below |
| `Trust this certificate for <name>, and for nothing else?` | asked only when the server's certificate is one your system has no reason to trust. See [Proton Bridge](#proton-bridge-and-other-servers-that-sign-their-own-certificate) |
| `Which is the archive folder? Enter for none` | asked only for a role no folder plainly fills, with the folders to choose from |
| `May an agent read this account's Inbox?` and `May an agent write drafts in it, for you to send or discard?` | no to both, which Enter gives, unless you run an agent. See [The agent](The-agent) |
| `Let :agent use the Claude CLI found at <path>?` | asked only when that program is installed and no agent is configured yet. No, which Enter gives, unless you want `:agent` |

Then two lines say it worked, `Signed in to <server> and found N folders.`
and `Signed in to <server>; nothing was sent.`, and
`Saved <name> in …/config.toml.`

## Ways to sign in

**1. A password, typed now.** It is not shown as you type, and tuimex keeps
it in the system keyring and in no file. Most providers refuse the password
you use on their website and want an *app password*: a separate password
made in the provider's settings for one program. Setup says where each
provider it knows makes one: Google at
https://myaccount.google.com/apppasswords (the account needs 2-Step
Verification first), Fastmail in its settings under Privacy & Security,
Apple at https://account.apple.com under Sign-In and Security. For Gmail
this is the quickest way in.

**2. A command that prints the password**, for people who keep passwords in
a command-line password manager, and for a machine with no keyring service.
You type the program and its arguments one to a line, such as `pass`,
`show`, `mail/work`, then an empty line. tuimex runs it whenever it signs
in and no shell reads it.

**3. Sign in with Google in a browser**, offered for Gmail. tuimex is given
no password. It needs a client of your own in Google's console, which takes
about ten minutes once.

## Gmail's browser sign-in

Google lets no small project ship a client for the mail scope without a
paid security assessment, so you make your own, once, and every Gmail
account uses it. These steps were checked against Google's own pages on
2026-11-23; the console moves, and where it differs, go by the console.

1. https://console.cloud.google.com → the project menu → **New project**.
   Name it anything, create it and select it.
2. **APIs & Services** → **Library** → search **Gmail API** → **Enable**.
3. **Google Auth platform** → **Get started**. App name and the two email
   addresses are yours to choose. **Audience: External.**
4. **Audience** → **Test users** → **Add users**: every Gmail address you
   will sign in with.
5. **Data Access** → **Add or remove scopes** → `https://mail.google.com/`
   → **Save**.
6. **Clients** → **Create client** → application type **Desktop app** →
   **Create**. Copy the Client ID and the Client secret then: Google shows
   the secret only at creation.

Two things to get right:

- **Choose *External*, not *Personal*, for more than one account.** The
  Personal audience lets only the account that made the client sign in.
- **Publish the app, or the sign-in ends after seven days.** While the
  app's status is *Testing*, Google ends every sign-in after a week, and
  tuimex then says `Google no longer accepts the sign-in kept for
  <account>; run tuimex setup to sign in again`. Under **Audience**, move
  the publishing status from *Testing* to *In production*. The app stays
  unverified, which is fine for a client only you use; Google shows an
  "unverified app" page at sign-in either way.

Then in `tuimex setup`, choose `3`, paste the Client ID and the Client
secret (not shown as you type), and a browser opens Google's sign-in.
Choose the account, pass the unverified-app page (Advanced, then Go to …),
and allow access to mail. A second Gmail account says `Using the Google
client already in the keyring.` and goes straight to the browser.

On a machine with no browser, such as one reached over SSH, setup prints
the address instead. Open it in a browser on any machine and sign in; the
browser then fails to load an address beginning `http://127.0.0.1:`, which
is expected. Copy that whole address from the address bar and paste it at
`That address, or the code alone:`.

The client's ID and secret and each account's refresh token are in the
keyring. The access token Google gives for them lives in memory only.

This sign-in is proven against a stand-in for Google, not against Google:
see [What is proven](What-is-proven).

## Proton Bridge, and other servers that sign their own certificate

Proton's mail is reached through Proton Bridge, which must be running on
the same computer. Its window shows the ports, the username and the
password to use, under the account's mailbox details; the password is
Bridge's own, not your Proton password.

Bridge signs its own certificate, so your system has no reason to trust it,
and tuimex never skips the check. Setup shows the certificate's SHA-256
fingerprint and asks whether to trust it for this account and for nothing
else. To compare it first, export Bridge's certificate (Settings → Advanced
settings → Export TLS certificates) and run
`openssl x509 -in cert.pem -noout -fingerprint -sha256`. A yes saves the
certificate beside the config as `<name>.pem`, and from then on it is the
only one accepted for that account.

When the server makes a new certificate, tuimex stops and says so with both
fingerprints: `the certificate <name>'s server presented is not the one
that was pinned`. Run `tuimex setup` and choose `Accept a new certificate
for <name>`. It is the same for any server you run yourself.

## Running setup again

With accounts already there, `tuimex setup` shows a menu:

```
  1) Add another account
  2) Sign in again to work
  3) Accept a new certificate for work
  4) Rename an account
  5) Quit
```

Setup only adds to the config file and keeps what was there to the byte;
the one thing it changes in place is an account's name.

**Renaming** moves four things together: the account's mail on this
computer, its sign-in in the keyring, its pinned certificate, and its name
in the config. No mail is downloaded again and nothing changes on the
server. It is refused while another tuimex has the mail open.

**Removing an account** is not something setup does. Delete its
`[[account]]` table from the config, its directory under
`~/.local/share/tuimex`, and its entries in the keyring.

## tuimex doctor

`tuimex doctor` checks every account end to end and this machine, changes
nothing, and exits non-zero when something is wrong:

- the config reads, and has accounts
- for each account: where its password comes from, its certificate, a
  sign-in to IMAP with the folder count, a sign-in to SMTP that sends
  nothing, and any role no folder fills
- the keyring answers; whether `gpg` is on the path; which editor compose
  will open; whether the `[agent]` command exists (it is found, never run);
  whether this terminal draws pictures
- every file and directory under tuimex's four is readable by you alone

Each line begins `ok`, `note` or `WRONG`, and a `WRONG` line says what
mends it. The last line is `nothing is wrong`, or how many things are.

`tuimex doctor --redact` is the report to post in public. It is the same
line for line, with each account name, address, user name, server host and
certificate fingerprint replaced by a numbered placeholder (`account-1`,
`address-1`, `host-1`, `fingerprint-1`) and your home directory by `~`,
in a server's own words too. The same value gets the same placeholder
throughout, and it is printed whole at the end instead of line by line. It
replaces only what the config names: a folder's name in a path, or an
address a server mentions that is not yours, stays, so read it before you
post it.

`tuimex sync` brings every account up to date once and exits, printing a
line for each folder such as `work/INBOX: 12 changed` or
`work/INBOX: nothing new`. `--account <name>` does one account. It is for a
script or a timer; an open tuimex syncs by itself.

---

[Configuration](Configuration) · [Troubleshooting](Troubleshooting) ·
[Security and privacy](Security-and-privacy)

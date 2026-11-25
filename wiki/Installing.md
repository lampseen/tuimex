# Installing

## From a release

Each release carries an archive for every system it is built for, under
fixed names, so `releases/latest/download/` links keep working:

| | x86_64 (`amd64`) | arm64 |
|---|---|---|
| Linux | `tuimex_linux_amd64.tar.gz` | `tuimex_linux_arm64.tar.gz` |
| macOS | `tuimex_darwin_amd64.tar.gz` | `tuimex_darwin_arm64.tar.gz` |

```sh
curl -LO https://github.com/lampseen/tuimex/releases/latest/download/tuimex_linux_amd64.tar.gz
tar -xzf tuimex_linux_amd64.tar.gz tuimex
mkdir -p ~/.local/bin && mv tuimex ~/.local/bin/
```

Use the archive name for your system: `darwin_arm64` on an Apple silicon
Mac, `darwin_amd64` on an Intel one.

Every archive, and the `checksums.txt` that covers them, is attested by
the workflow that built it: a signature made without a private key,
recorded in a public log, that ties the file to one run of one workflow on
one commit. To check one, with GitHub's `gh`:

```sh
gh attestation verify tuimex_linux_amd64.tar.gz --repo lampseen/tuimex
```

What that is worth: it shows the archive came from this repository's
release workflow and was not replaced afterwards. It does not show the
code is safe; that is what reading it is for.

**macOS** may refuse to open it, saying the developer cannot be verified:
the binary is not signed with an Apple Developer ID or notarised.
Downloading with `curl` as above avoids that. If you used a browser, clear
the flag it set:

```sh
xattr -d com.apple.quarantine ~/.local/bin/tuimex
```

A Homebrew tap is to follow.

## From source

You need Rust and `make`, then, from a clone:

```sh
make install-binary
```

That builds the release binary and copies it to `~/.local/bin/tuimex`,
which is on `PATH` on most systems and needs no root. `tuimex --version`
confirms it.

## What it needs at run time

Nothing that has to be installed first on a desktop system. Three things
are used where they exist:

- **A keyring service** to keep passwords in: the Secret Service on Linux
  (GNOME Keyring, KWallet, KeePassXC), the login keychain on macOS. A
  machine with none, such as a server reached over SSH, uses a command
  that prints the password instead; setup offers both.
- **`$VISUAL` or `$EDITOR`** to write a message in; `vi` when neither is
  set.
- **A browser**, for Gmail's sign-in and for the sealed copy of a message.

`tuimex doctor` says which of these it found.

## Removing it

Delete the binary. The four directories listed on [Home](Home) hold
everything else: remove `~/.local/share/tuimex` only if you mean to delete
the local copy of your mail, which the server still has. Passwords are in
the system keyring under the service `tuimex`, and are removed there.

---

[Setup and accounts](Setup-and-accounts) · [Where it runs](Where-it-runs)

# tuimex

A terminal mail client for clearing an inbox across several accounts, where
nothing a message contains can act for you or report on you. The
[README](https://github.com/lampseen/tuimex) is the overview; these pages are
the detail. tuimex uses a few words precisely, and the rest of these pages
assume them.

## The words

| term | what it means here |
|---|---|
| **account** | one mailbox on one server, under the short name setup gave it, such as `work`. Every account is in the same list |
| **section** | one tab: a saved query, such as *Unread*. An account or a folder is a term in a query, not a separate mode |
| **folder** | where a server keeps mail. Gmail's labels are shown as folders. Inbox, Archive, Trash, Sent, Drafts and Junk are *roles* a folder has, whatever the server calls it |
| **thread** | a conversation: a reply sits with what it answers. A folded thread is one row, and an action on it takes all of it |
| **marks** | rows picked with `Space`. With marks present, an action takes all of them |
| **notice** | one line above a message saying something about it that its author did not write: remote content was not loaded, the sender differs from the thread, an agent wrote this draft |
| **the sealed copy** | the message as its author styled it, written to a private file and opened in your browser: no script, no form, and nothing loaded from a server unless you said yes to its pictures |
| **the agent** | a program that reads mail or writes text for you. There are two, and they are kept apart: the command `:agent` hands text to, and an outside agent that calls `tuimex` subcommands under grants you gave |
| **grant** | what an account lets an outside agent do: `read`, for the folders named, and `draft`. There is no grant to send, move, delete or flag |
| **local** | said of an event or a calendar that is on this machine and nowhere else: tuimex holds it because there is no provider's calendar it may write |
| **fixtures** | the design's synthetic mailbox, `tuimex --fixtures`, for trying tuimex with no account and no network |

Four directories, and nothing outside them:

```
~/.config/tuimex/        what you said: config.toml, a pinned certificate, a palette
~/.local/share/tuimex/   your mail, one Maildir for each account, and your local events
~/.cache/tuimex/         what can be worked out again: the index, sealed copies, fetched calendars
~/.local/state/tuimex/   the log, and the record of every agent call
```

## Start here

- **[Your first session](Your-first-session)**: the screen, and the keys that matter on day one.
- **[Installing](Installing)**: a release archive for your system, or building it.
- **[Setup and accounts](Setup-and-accounts)**: the questions setup asks, Gmail, Proton Bridge, and `tuimex doctor`.

## Doing the work

- **[Reading mail](Reading-mail)**: the list, threads, the preview, the reader and its notices.
- **[Links and the browser copy](Links-and-the-browser-copy)**: what a link is checked for, and the sealed copy.
- **[Attachments](Attachments)**: saving a file, opening one, pictures.
- **[Writing and sending](Writing-and-sending)**: compose, identities, the send summary, drafts.
- **[Calendar](Calendar)**: every account's calendar in one view, answering invitations, events of your own, reminders.
- **[The agent](The-agent)**: `:agent` inside tuimex, and the subcommands an outside agent may call.
- **[Sections and search](Sections-and-search)**: the tabs, the query grammar, `/`.
- **[Keys](Keys)** and **[Commands](Commands)**: everything there is.
- **[Configuration](Configuration)**: every key of the config file, and your own palette.

## When something is off

- **[Troubleshooting](Troubleshooting)**: the lines tuimex prints when it stops, and what to do.
- **[Security and privacy](Security-and-privacy)**: what a message can and cannot make tuimex do, and the limits.
- **[Where it runs](Where-it-runs)**: systems, terminals, providers, and what is untested.
- **[What is proven](What-is-proven)**: every claim with the test that holds it, and what no test reaches.

---

*Generated from [`wiki/`](https://github.com/lampseen/tuimex/tree/main/wiki)
in the main repository when a change merges. An edit made here is
overwritten by the next copy; corrections belong in a pull request.*

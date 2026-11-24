# Sections and search

## Sections

Each tab across the top is a *section*: a saved query. Six ship, and each
spans every account:

| section | query |
|---|---|
| Inbox | `in:inbox` |
| Unread | `in:inbox is:unread` |
| Flagged | `is:flagged` |
| Drafts | `in:drafts` |
| Sent | `in:sent` |
| Archive | `in:archive` |

An Outbox tab joins them while a message is waiting to go or was not sent.

`h` `l` or `Tab` `S-Tab` move between sections, a digit `1`–`9` goes to one
by position, and `:section <name>` by name. `tuimex --section <name>` opens
on one, by its name or its short name.

Your own replace the six. In the config:

```toml
[[section]]
name  = "Waiting on me"
short = "Wait"
query = "in:inbox is:flagged"

[[section]]
name  = "Invoices"
short = "Inv"
query = "has:attachment subject:invoice after:30d"
threaded = false
```

`name` is the tab, `short` a shorter name `--section` and `:section` also
take, and `threaded = false` lists every message on a row of its own. Naming any
section replaces all six defaults, so include the ones you still want.

## The query grammar

Terms are separated by spaces, and every term must match.

| term | matches |
|---|---|
| `in:<role or folder>` | a role (`inbox`, `archive`, `trash`, `sent`, `drafts`, `junk`, `outbox`) or a folder by its name |
| `account:<name>` | one account |
| `is:unread` `is:read` `is:flagged` `is:answered` `is:draft` | by flag |
| `is:signed` `is:encrypted` | what the message's structure says; nothing is verified |
| `is:agent-draft` | a draft an agent wrote |
| `has:attachment` `has:invite` `has:unsubscribe` | carries a file, an invitation, or a header saying how to leave its list |
| `from:<text>` `to:<text>` `cc:<text>` `subject:<text>` | text in that field |
| `after:<date or age>` `before:<date or age>` | `2026-11-01`, or an age such as `7d`. An age stays an age: a saved section means "the last week" on every day it is opened |
| `larger:<size>` | bigger than a size: bytes, or `500k`, `10m`, `1g` |
| anything else | free text over the subject, the sender and the body |

Double quotes keep spaces in one term: `subject:"board minutes"`. A word
that is no term the grammar knows, or whose value does not fit its term,
is free text; a query is never an error.

Text matches by whole words in order, in any case, and the last word may
be unfinished: `contr` finds *contract*.

## Finding and searching

`/` finds among the rows of the section you are in, as you type, in the
same grammar:

```
  3 here · Enter searches all mail
  /nov
```

`n` and `N` step to the next and previous match. `Esc` clears the find.

`Enter` on what you typed searches **all mail**: every account and every
folder, bodies included. The results take the place of the section, under
a `Search` tab, until `Esc`, which returns you to the row you pressed `/`
on. A search that finds nothing says how much it looked through:
`Nothing matches "zebra". Searched 9,240 messages in 2 accounts.`

Search covers what has been synced to this machine, and nothing is asked
of the server. A message whose text has not been downloaded yet is found
by its subject and sender, not by its body; `sync.bodies` in the
[configuration](Configuration) decides how far back text is fetched.
Search folds accents.

## One account at a time

`gt` shows the next account alone, `gT` the previous, and after the last
comes every account again. `:account <name>` goes to one, and `:account`
with no name shows all. The command line says which: `Showing work only ·
gt for the next, :account for all`. Sections keep working inside it.

To make a tab for one account, add `account:work` to a section's query.

---

[Reading mail](Reading-mail) · [Configuration](Configuration) ·
[Commands](Commands)

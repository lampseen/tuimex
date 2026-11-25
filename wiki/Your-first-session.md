# Your first session

Run `tuimex`. The first time, on a terminal, `tuimex setup` runs before
anything else: answer its questions, and once the account's servers have
taken the sign-in the list opens. [Setup and accounts](Setup-and-accounts)
goes through the questions.

To look around with no account at all, `tuimex --fixtures` opens the
design's synthetic mailbox instead. It reaches no server and opens nothing
outside the terminal.

## The screen

```
  tuimex   F1 Mail   F2 Calendar
  Inbox 9   Unread   Flagged 3   Drafts   Sent   Archive   Outbox 1

  Today
    work       Imani Castell     Invitation: Q4 roadmap review        invite   14:05
    personal   Jonas Becker    ▸ Cabin weekend: who is driving?        5 msg   13:40
  ▪ work       Backup Robot      [ops] Nightly backup finished                 12:58
▌   work       Priya Raman     ▸ Contract draft v3            3 msg · 2 files   11:32 ★
```

The first row is on every screen. It names the two places tuimex has,
mail and the [calendar](Calendar), each by the key that goes to it, and
the one you are in is the one in the accent colour and bold. In a draft,
the event form and help, where a function key does nothing, it names the
places without their keys.

The row under it is the sections, one tab each. A tab has a number only when the
number means "this needs you": unread rows in the Inbox, every row of a
section of flagged mail, and in the Outbox what was not sent. The Outbox
tab is there only while a message is waiting to go.

Each row is one message, or one conversation folded into a row (`▸`): the
account it is in, who it is from, the subject, what it carries, and when.
A row that needs you is bright: unread is bold, flagged is bright, and
the rest are dim. `▌` is the
cursor, `▪` a mark, `★` a flag. Rows are grouped by day: Today, Yesterday,
Past 7 days, Earlier.

Under the list, or beside it on a terminal 160 columns wide or more, the
preview is the message the cursor rests on, read in full.

The second row from the bottom is the status line: where you are on the
left, and on the right only what is not normal, such as a message waiting
to go, or an account that cannot sync. The bottom row is the command line,
which also holds questions and results.

## The keys for day one

| key | does |
|---|---|
| `j` `k` | down, up |
| `h` `l` or `Tab` | previous, next section |
| `Enter` | open the message in the reader; `Esc` comes back |
| `e` | archive |
| `d` | move to Trash |
| `u` | undo the last action |
| `r` `c` | reply, write a new message |
| `/` | find among these rows; `Enter` then searches all mail |
| `Space` | mark the row, so the next action takes every marked row |
| `?` | every key and command; type to filter |
| `:q` | leave |

No key quits. `Esc` goes back, and `:q` then `Enter` leaves.
[All the keys](Keys).

## Clearing an inbox

Go down the list. For each message, one key deals with it: `e` when it is
done with, `d` when it never mattered, `r` when it needs an answer, `s` to
flag what has to wait. Nothing asks you to confirm, because `u` takes the
last action back, on the server as well as on screen. Sending is the one
thing that cannot be taken back once it has gone, so it shows a summary of
who receives what and waits for `y`, and then holds the message for ten
seconds in which `u` still stops it.

When the Inbox section says `Nothing here.`, you are finished.

---

[Reading mail](Reading-mail) · [Writing and sending](Writing-and-sending) ·
[Keys](Keys)

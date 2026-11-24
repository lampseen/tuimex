# Writing and sending

Mail is written as plain text, in your own editor. Sending is the one
action tuimex confirms first, because it is the one `u` cannot reverse
once the message has gone.

## Opening compose

| key | command | does |
|---|---|---|
| `c` | `:compose [address]` | a new message |
| `r` | `:reply` | reply to the sender |
| `R` | `:replyall` | reply to everyone |
| `f` | `:forward` | forward the message |

A reply quotes the message under a line saying who wrote it and when, and
carries the headers that keep it in its conversation. A forward is the
message between two markers. From the reader, a reply quotes what the
reader shows. `Enter` on a row in Drafts opens that draft where you left
it, with its files, its Bcc and its whole text.

## The fields

```
▌   From     robin@example.com   robin.vale@example.org   desk@example.org
    To       Priya Raman <priya@example.org>
    Cc       billing@halden.example.com
    Bcc
    Subject  Re: Contract draft v3 for the Halden account
    Files    contract-v3-signed.pdf       512 KB
```

| key | does |
|---|---|
| `Tab` `S-Tab` | next, previous field |
| `Left` `Right` | on From, choose who the message is from |
| `Enter` | on the text, open your editor |
| `^a` | attach a file; `Tab` completes its path |
| `^s` | the send summary |
| `^d` | save the draft |
| `Esc` | close; asks whether to keep a draft that changed |

**From** offers every address you can send as: each account's own, then
the `identities` its config names. The message is filed and sent through
the account that address belongs to, so with several accounts choosing
From is choosing which server sends it. A reply starts on the account the
message came to.

**The text** is written in `$VISUAL` or `$EDITOR`, or `vi` when neither is
set. tuimex steps aside for the editor and comes back around it when the
editor exits. The text passes through a file readable by you alone under
`~/.cache/tuimex`, which is then removed.

## The send summary

`^s`, or `:send`, shows one screen before anything leaves:

- who it is sent as
- every recipient on a line of its own, under To, Cc and Bcc
- the subject, and each file with its size and the total
- for a draft an agent wrote or changed, a line saying so

A recipient whose domain appears nowhere in the conversation being
answered is marked `▲ not in this thread`. That is what a redirected reply
looks like: a message that claims to continue a conversation and quietly
adds or swaps an address.

An address that does not parse stops the summary and is named. Only `y`
sends.

## The delay, and stopping a message

After `y` the message waits in the Outbox, ten seconds by default
(`send.delay` in the config). The status line counts down, `sending in
6s`, and until it reaches zero `u` takes the message back: `Not sent · back
in Drafts`.

When the server accepts it, a copy is filed in the account's Sent folder.
Gmail files its own, so tuimex adds none there. Where the server has no
Sent folder to take the copy, the message is still sent, the copy stays on
this machine only, and tuimex says so: `E <account> would not keep a copy
of what was sent · it is in Sent on this machine alone`.

A message the server refuses stays in the Outbox with the reason on its
row, and the Outbox tab shows how many were not sent. `Enter` on it opens
it to edit and send again; `d` trashes it. A connection that drops is
tried again by itself.

`:q` while a message is being handed to its server waits for that to
finish, a minute at most, and says which account it is waiting for.

## Drafts

`^d` saves the draft to the account's Drafts folder. `Esc` asks whether to keep or
discard a draft that changed, and closes one that did not without asking.

## Help from the agent

With an `[agent]` table in the config, `:agent reply` writes a reply from
the conversation and `:agent tidy` cleans up what you wrote. Either marks
the draft `◇ Written by an agent` through to the summary, and `u` brings
your own text back. See [The agent](The-agent).

## Not built

Signing and encrypting with OpenPGP, and composing HTML. HTML is left out
of v1 on purpose; OpenPGP is planned.

---

[The agent](The-agent) · [Configuration](Configuration) · [Keys](Keys)

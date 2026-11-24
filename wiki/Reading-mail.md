# Reading mail

## The list

Every account's mail is in one list. With more than one account, each row
begins with its account's name in that account's colour; `gt` and `gT` step
through the accounts one at a time and back to all of them, and
`:account <name>` goes to one directly.

Rows are grouped by day: Today, Yesterday, Past 7 days, Earlier. A row
shows who the message is from, its subject, what it carries (`3 msg` for a
conversation, `2 files`, `invite`, `replied`) and when it came. A row that
needs you is bright: unread is bold, a flagged row is bright and ends in
`★`, and the rest are dim.

Move with `j` `k`, `gg` `G` for the first and last row, `^d` `^u` for half
a page. `h` `l`, `Tab` or a digit change section.

When an account cannot reach its server, the right of the status line says
which and when it will try again (`work offline · retry in 16s · last
synced 12:41`), and that account's rows say `stale`, instead of the list
looking fresh when it is not. A first sync of a large
mailbox says how far it has got.

## Threads

A reply sits with what it answers. A conversation is one folded row, `▸`;
`zo` unfolds it into its messages and `zc` folds it again. An action on a folded row takes the whole conversation, and
on an unfolded one only the message under the cursor. The status line says
which before you press the key.

Sections are threaded unless their config says `threaded = false`.

## The preview

The preview is the message the cursor rests on: its header, its notices and
its whole text, not an excerpt. It sits under the list, or beside it from
160 columns.

| key | does |
|---|---|
| `p` | hide or show it |
| `P` | move it: beside, below |
| `Alt-j` or `Alt-l` | focus it; `j` `k` `^d` `^u` `gg` `G` then scroll it |
| `Alt-k` or `Alt-h` | back to the list |

`:preview auto|right|below|off` does the same by name, and `ui.preview` in
the config sets it at start. While the preview is focused, `e`, `d` and the
other actions still take the row.

A message whose text is not on this machine yet says `Fetching this
message.` and shows it when it arrives. By default tuimex downloads the
text of messages from the last thirty days and of anything you open; see
`[sync]` in [Configuration](Configuration).

## The reader

`Enter` opens the message full screen and marks it read. `Esc` returns to
the list with the cursor where it was.

| key | does |
|---|---|
| `j` `k` `^d` `^u` `gg` `G` | scroll |
| `n` `b` | next, previous message of the thread |
| `J` `K` | next, previous message of the list |
| `o` | its links: see [Links and the browser copy](Links-and-the-browser-copy) |
| `a` | its files: see [Attachments](Attachments) |
| `B` | the sealed copy in your browser |
| `e` `d` `s` `m` `M` `r` `R` `f` | as in the list |

Mail is read as text. An HTML message becomes its words: headings,
paragraphs, lists, quotes, and a table as its rows. A link is its text
followed by a number, `Route notes and maps [1]`. A picture the message
keeps on a server is its description, `[image: Ridge loop at sunrise]`, and
nothing is fetched for it. Text the author hid from a reader (`display:
none`, zero size, the `hidden` attribute) is left out.

That costs something: a newsletter built from nested tables becomes a
column of text and links. `B` is the answer, one key away.

## Notices

Above the text, one line each, only when true:

| notice | what it means |
|---|---|
| `▲ Remote content not loaded · 9 images, 3 hosts · tracker: …` | the message wanted to load things from servers, and tuimex loaded none. A named tracker is a service that reports when and where you opened the message; it was not told |
| `▲ Sender differs from the thread · earlier messages came from …` | this message claims to continue a conversation, from an address the earlier messages did not come from. It is how a redirected reply looks; check before you act on it |
| `▲ Signature not checked · tuimex does not verify one yet` | the message is signed. Checking a signature is not built, so tuimex says so and never draws it as good |
| `▲ Encrypted · tuimex cannot open it yet` | the message is encrypted. Decrypting is not built |
| `◇ Written by an agent` | this draft was written by an agent, not by you. The mark stays through the send summary |

## An invitation

A calendar invitation is shown as a card: what, when, where, who. `i`
asks `Answer?`, and the next key answers it: `a` accept, `t` tentative,
`d` decline, `p` propose another time, `x` ignore. `:invite accept` and
the rest do the same.

An answer mails the organizer, from the account the invitation came to,
and puts the event with your answer in that account's calendar; the
result line says where, and `u` takes back both while the reply still
waits in the Outbox. Ignoring sends nothing. An invitation you have not
answered is on the calendar too, on its day, marked `needs answer`.
[Calendar](Calendar#answering-an-invitation) has the whole of it.

A card that answers nothing says why: the message's calendar part could
not be read, the account organizes the event itself, or the invitation
names no organizer to answer.

---

[Links and the browser copy](Links-and-the-browser-copy) ·
[Calendar](Calendar) · [Sections and search](Sections-and-search) ·
[Keys](Keys)

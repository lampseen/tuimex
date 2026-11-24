# Keys

The full table is
[`docs/keymap.md`](https://github.com/lampseen/tuimex/blob/main/docs/keymap.md).
It is written from the table in the code that the keys are read from, and
`?` inside tuimex is drawn from the same table, so the three cannot
disagree. This page is the ones to know first; a test checks that every
key on it is in that table.

`?` shows every key and command for where you are; type to filter it.

## Everywhere

| key | does |
|---|---|
| `:` | the command line |
| `?` | help |
| `Esc` | back; clears a find and marks |
| `h` `l` `Tab` `S-Tab` | previous, next section |
| `1-9` | section by position |
| `u` | undo the last action |
| `A` | what the agent can do here |
| `^l` | redraw |

No key quits: `:q` does.

## In the list

| key | does |
|---|---|
| `j` `k` `gg` `G` `^d` `^u` | move |
| `Enter` | open the message; a draft, to go on writing it |
| `/` then `n` `N` | find, next and previous match; `Enter` searches all mail |
| `Space` `V` | mark a row, mark a range |
| `e` `d` | archive, move to Trash |
| `s` `m` | flag or unflag, mark read or unread |
| `M` | move to a folder; `Tab` completes its name |
| `c` `r` `R` `f` | write, reply, reply to everyone, forward |
| `zo` `zc` | unfold, fold a thread |
| `p` `P` | show or hide the preview, move it |
| `Alt-j` `Alt-l` | focus the preview |
| `gt` `gT` | next, previous account alone |
| `B` | the sealed browser copy |

## In the reader

| key | does |
|---|---|
| `j` `k` `^d` `^u` `gg` `G` | scroll |
| `n` `b` | next, previous message of the thread |
| `J` `K` | next, previous message of the list |
| `o` | links, then a digit to open one |
| `a` | files, then `Enter` to save or `O` to open |
| `B` | the sealed browser copy |

The list's action keys work here too.

## In compose

| key | does |
|---|---|
| `Tab` `S-Tab` | next, previous field |
| `Left` `Right` | who it is from, on From |
| `Enter` | write the text in your editor |
| `^a` `^s` `^d` | attach a file, the send summary, save the draft |

## Answering a question

`y` or `Enter` is yes, `n` or `Esc` is no. Any other key cancels the
question and is dropped.

## How keys are read

`^d` is Control and `d`. `Alt-j` is Alt and `j`, or `Esc` then `j` typed
together, which is how most terminals send it. A sequence such as `gg` or
`zo` waits half a second for its second key.

Keys cannot be remapped yet. The config accepts a `[keys]` table and
nothing reads it.

---

[Commands](Commands) · [Your first session](Your-first-session)

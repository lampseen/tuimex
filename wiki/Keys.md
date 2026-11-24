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
| `gc` | the calendar |

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

## In the calendar

`gc` opens the calendar and `gm` returns to mail; each keeps where it
was. A key that mail uses means the same here, except `h` and `l`.
[Calendar](Calendar) says what each does at length.

| key | does |
|---|---|
| `h` `l` | previous, next day |
| `j` `k` | in the month, a week down or up; elsewhere the next, previous event |
| `[` `]` | previous, next page: a month, a week or a day |
| `J` `K` | next, previous event; in the month, the next day that has one |
| `I` | the next invitation that waits for an answer |
| `t` `gd` | today; go to a date, typed as `2026-12-05`, `5 dec`, `fri` or `+2w` |
| `Tab` `S-Tab` | next, previous view |
| `1` `2` `3` | month, week, day |
| `Enter` `Esc` | deeper, to the day and then the event; back |
| `/` then `n` `N` | find; `Enter` lists every match a year either side of today |
| `gt` `gT` | next, previous account alone |
| `C` | the calendars, each to show or hide with `Space` |
| `o` | the selected event's links, the meeting's first, each with the host it really opens; `Enter` or its number opens one |
| `i` | answer an invitation: then `a` accept, `t` tentative, `d` decline, `p` propose another time, `x` ignore |
| `c` `E` | a new event on the selected day; edit the selected event |
| `d` | delete the selected event. One that repeats asks: `t` this one, `a` all of them |
| `u` | take back the last thing done in the calendar |
| `gn` | reminders on, off |
| `gm` | back to mail |

On an event's own screen `j` `k` `^d` `^u` `gg` `G` scroll, `zo` and `zc`
unfold and fold what only some providers send, and `v` opens the mail the
event came in. In the form:

| key | does |
|---|---|
| `Tab` `S-Tab` | next, previous field |
| `Left` `Right` | choose, where a field offers choices |
| `Enter` | write the notes in your editor |
| `^s` | save; guests are mailed after a question |
| `Esc` | close; with changes, keep or discard |

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

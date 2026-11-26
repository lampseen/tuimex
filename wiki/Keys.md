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
| `gc` `F2` | the calendar |

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
| `h` `l` | who it is from, on From; `Left` `Right` do the same |
| `Enter` | write the text in your editor |
| `^a` `^s` `^d` | attach a file, the send summary, save the draft |

## In the calendar

`gc` or `F2` opens the calendar and `gm` or `F1` returns to mail; each
keeps where it was. The first row of every screen names both places by
those function keys and marks the one you are in. A key that mail uses
means the same here, except `h` and `l`.
[Calendar](Calendar) says what each does at length.

| key | does |
|---|---|
| `h` `l` | previous, next day |
| `j` `k` | in the month's grid, a week down or up; elsewhere the next, previous event |
| `Alt-l` `Alt-h` | in the month, focus the list of events beside the grid, on the first event it shows from the selected day on; back to the grid. `Alt-j` and `Alt-k` do the same. In the list `j` `k` step through every event it shows, `h` `l` go to the day before or after that has one, and `Enter` opens the selected event |
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
| `gm` `F1` | back to mail |

On an event's own screen `j` `k` `^d` `^u` `gg` `G` scroll, `zo` and `zc`
unfold and fold what only some providers send, and `v` opens the mail the
event came in. In the form:

| key | does |
|---|---|
| `Tab` `S-Tab` | next, previous field |
| `h` `l` | choose, where a field offers choices; `Left` `Right` do the same. In a field you type into they are letters |
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

## The mouse

Off until `:mouse` is typed, or from the start when the config has
`mouse = true` under `[ui]`. With it on, the terminal hands the pointer to
tuimex and no longer selects text by dragging; most terminals still do
while Shift is held. `:mouse` again turns it off, and `:mouse on` and
`:mouse off` say which. What `:mouse` sets lasts until tuimex is left.

It does nothing a key does not, and so far only in mail:

| press or wheel | does |
|---|---|
| a row | moves the cursor there; on the row the cursor has, opens it as `Enter` does |
| a tab | opens that section |
| a place on the top bar | goes there, where `F1` and `F2` are read |
| the preview | moves the focus into it, as `Alt-j` or `Alt-l` does |
| wheel over the rows | moves the cursor three rows |
| wheel over the preview | scrolls it three lines |
| a `[1]` in a message | lists the links with that one chosen; nothing opens |
| an entry of the links or the files | chooses it; on the one chosen, does what `Enter` does there |
| wheel over a message | scrolls it three lines |
| wheel over help or `:messages` | scrolls them as their keys do |
| a question's answer, such as `y yes` | answers it as that key does |
| a field of compose | moves the focus there, as `Tab` does; nothing is typed |

A first press only ever moves the cursor, so the press that brings the
terminal's window to the front opens nothing: a link opened by pressing
is asked about as one opened by its number is. The hint line at the
bottom names keys and is not pressed. While a question is open only its
answers are read, and a press anywhere else leaves it open. While a line
is being typed a press does nothing. The menus that open over the list,
such as `A`'s, take no press. In the calendar a press does nothing yet,
apart from the bar of places.

---

[Commands](Commands) · [Your first session](Your-first-session)

# Keys

Generated from the table in `crates/tuimex-ui/src/keymap/mod.rs`, which is the design's
`KEYMAP`; `make keymap` writes this file, so edit the table. The `?` screen is drawn from the same
table. A sequence such as `gg` waits 500 ms for its second key.

## Everywhere

| key | does |
|---|---|
| `:` | command line |
| `?` | help; type to filter |
| `Esc` | back; clear find, marks |
| `h S-Tab` | previous section |
| `l Tab` | next section |
| `1-9` | section by position |
| `u` | undo the last action |
| `A` | what the agent can do |
| `^l` | redraw |

## List

| key | does |
|---|---|
| `j k` | down, up |
| `gg G` | first, last row |
| `^d ^u` | half a page down, up |
| `/` | find; Enter: all mail |
| `n N` | next, previous match |
| `Space` | mark the row |
| `V` | mark a range |
| `Enter` | open; a draft, to edit |
| `p` | show or hide preview |
| `P` | preview beside, below |
| `B` | sealed browser copy |
| `Alt-j Alt-l` | focus the preview |
| `gt gT` | next, previous account |
| `e` | archive |
| `d` | move to Trash |
| `s` | flag or unflag |
| `m` | mark read or unread |
| `M` | move to a folder |
| `c` | write a new message |
| `r` | reply |
| `R` | reply to everyone |
| `f` | forward |
| `zo` | unfold a thread |
| `zc` | fold a thread |

## Preview, focused

| key | does |
|---|---|
| `j k ^d ^u gg G` | scroll |
| `Alt-k Alt-h` | back to the list |

## Reader

| key | does |
|---|---|
| `j k ^d ^u gg G` | scroll |
| `n b` | thread: next, previous |
| `J K` | list: next, previous |
| `o` | links |
| `a` | attachments |
| `i` | answer an invitation |
| `B` | sealed browser copy |
| `e d s m M r R f` | as in the list |

## Links (after o)

| key | does |
|---|---|
| `1-9` | open that link |
| `j k Enter` | choose, then open |
| `Esc` | close |

## Attachments (after a)

| key | does |
|---|---|
| `j k` | choose |
| `Enter` | save to downloads |
| `O` | open, after asking |
| `Esc` | close |

## Agent (after A)

| key | does |
|---|---|
| `r` | write a reply |
| `t` | tidy, in compose |
| `l` | review a link, in links |

## Invitation (after i)

| key | does |
|---|---|
| `a` | accept |
| `t` | tentative |
| `d` | decline |

## Compose

| key | does |
|---|---|
| `Tab S-Tab` | next, previous field |
| `Left Right` | identity, on From |
| `Enter` | edit body in $EDITOR |
| `^a` | attach a file |
| `^s` | send summary |
| `^d` | save the draft |
| `Esc` | close; keep or discard |

## Command line

| key | does |
|---|---|
| `Tab S-Tab` | complete, cycle |
| `Up Down` | history, by prefix |
| `^w ^u` | delete word, line |
| `Enter` | run |
| `Esc` | cancel |

## Confirm

| key | does |
|---|---|
| `y Enter` | yes |
| `n Esc` | no |
| `any other` | cancels; key dropped |

## Help (after ?)

| key | does |
|---|---|
| `^d ^u` | scroll |

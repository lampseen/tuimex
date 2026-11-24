# Design brief

What `design/tui.js` has to show before M6 can start (ADR-002). It is
written for whoever produces the design, and for the implementing agent to
know what will arrive. When the design is delivered this file moves to
`history/`, because the artifact replaces it.

The artifact has the same shape as pullpen's: pure JS with no DOM,
`TUI.build(mode)` returning every screen as arrays of styled cells, drawn
from fixture data that lives in the file.

## Two things to do differently from pullpen's artifact

pullpen's generator had to undo work the artifact had already done (§7 of
its plan). Asking for these up front removes that code:

1. **`build()` returns colour tokens, not resolved colours.** In sixteen
   colours `dim` and `faint` resolve to the same value and the token cannot
   be recovered.
2. **Each output says whether it is a full screen or a row strip**, and at
   what width.

## Fixture mailbox

One deterministic mailbox, enough to make every screen below true at once:

- two accounts (`personal`, `work`), each with Inbox, Sent, Drafts, Archive,
  Trash;
- about twenty-five inbox rows across both, mixed read and unread, some
  flagged, some with attachments;
- one thread five messages deep with a branch, one of three, the rest
  single;
- one HTML newsletter with remote images and a known tracker;
- one message whose link text names a different host from its target;
- one signed message, one encrypted, one with a bad signature;
- one calendar invitation;
- one draft written by an agent;
- one message waiting in the Outbox, and one that failed to send.

Every address is under `example.com`, `example.org` or `example.net`.

## Screens

Each at the default size, and the starred ones also at 80×24 in ASCII with
sixteen colours.

| Screen | Must show |
|---|---|
| List * | tabs with counts, header, rows with thread folding, the selected and marked treatments, preview pane, status line, the reserved command line |
| List, a thread unfolded | the tree, and which rows an action would take |
| List, filtering * | non-matching rows faint, the match underlined |
| List, empty and first sync | an empty section; a section mid-sync with progress |
| List, an account failing | the status line carrying the error, the stale rows marked as stale |
| Reader, plain text * | header block, body, scroll position |
| Reader, HTML as text | numbered link references, a table as rows, a quote, the remote-content notice with the tracker named |
| Reader, notices | signed good, signed bad, encrypted, sender differs from thread, agent draft |
| Reader, attachments | the list, an inline image placeholder at its cell size |
| Reader, invitation | the card and its three answers |
| Links overlay | the numbered list with real hosts; the confirm for a deceptive one |
| Compose | fields, identity choice, attachment list, body preview |
| Send summary | every recipient on its own line, attachments with sizes, signing state, the outside-the-thread mark, the agent-draft line |
| Move prompt | folder completion over the list |
| Help overlay | the key tables, filtered |
| Command line states | idle, typing with the target shown, completion menu, confirm, success with `u to undo`, error, unknown command with a suggestion, `:messages` |
| Flows, as row strips | archive then undo; trash a marked batch; send, held, then sent; send, held, then undone; a move the server refuses |

## What the design decides

The plan proposes and the design settles: the final keymap (PLAN §6.2), the
row's columns and their collapse order as the width shrinks, the palette
tokens and the three colour tiers, the glyphs and their ASCII fallbacks
(widths never changing between tiers), where the preview hides, and the
wording of every message and notice.

What the design does not decide, because a record already has: what can be
confirmed and what is undone (ADR-015), what a notice must exist for (PLAN
§6.5), what the send summary must list (PLAN §6.4), and that nothing remote
is ever shown as loadable from inside the reader (ADR-008).

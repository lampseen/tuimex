// tuimex: the visual spec (ADR-002, ADR-024).
//
//   TUI.build({ glyphs: 'unicode' | 'ascii', palette, accounts })
//
// returns every screen and flow, keyed by name. A screen is
//   { kind: 'screen', title, shows, width, height, glyphs, colors, rows }
// and a flow is
//   { kind: 'flow', title, steps: [{ kind: 'strip', key, caption, note, width, height, rows }] }
// A row is a list of runs { t, fg, bg, b, u } that always adds up to the
// declared width. `fg` and `bg` are palette tokens, never colour values;
// `bg: null` is the terminal's own background. `palette`, a loaded palette's
// truecolor values by token, with `accounts`, how many are configured,
// changes two things: which accents the accounts take (LAYOUT.accounts) and
// what colour the words of a row are (LAYOUT.words).
//
// TUI.palette, TUI.glyphs, TUI.keymap, TUI.commands, TUI.layout, TUI.wording
// and TUI.fixture are the tables the screens are drawn from; TUI.separation,
// TUI.contrast, TUI.accountOrder and TUI.wordsColour are the calculations a
// loaded palette needs.
//
// The look has one rule: what is normal is silent. Hierarchy is weight,
// indentation and blank rows; a colour appears where it says something a
// reader scans for (which account, where you are, what needs you, how bad),
// and never as a fill, a box or a badge.

(function () {
'use strict';

// ------------------------------------------------------------------ palette
//
// The only colour values in the repository (ADR-003). c256 holds xterm
// indices and c16 holds ANSI names. In sixteen colours `sel` has no fill and
// `dim` and `faint` are one colour, so no screen relies on the fill or on
// that difference alone: the cursor glyph, bold and the words carry the same
// facts. `link` is a light blue because a dark one cannot be read on this
// ground: plain ANSI blue is never used, and `head` takes bright yellow in
// sixteen colours, apart from `warn` by brightness and by where it appears.

const PALETTE = {
  // Five neutrals and seven accents, the seven a Dracula-family theme has
  // (`from`), so such a theme maps on one for one. Each accent has one job.
  // An account name borrows one of three of them (LAYOUT.accounts): in the
  // account column a colour says only which account, and nothing else in a
  // list row wears the same one. Cyan is two things that never meet: the
  // agent's mark, in notices, compose and the command line, and the words at
  // the right of a list row.
  roles: {
    bg: 'the ground; also the text colour of the command-line caret',
    sel: 'fill of the selected row',
    fg: 'text that needs you: unread, flagged, titles, what you are typing',
    dim: 'everything else: read mail, labels, date groups, details, hints',
    faint: 'rows a filter does not match; dividers; the cells of an inline image',
    acc: 'where you are: the cursor, the caret, the active tab',
    err: 'failed, refused, a bad signature',
    warn: 'flagged, and a notice to read before acting',
    ok: 'done or verified',
    agent: 'anything an agent wrote or said; in a list row, the words about it',
    link: 'link numbers',
    head: 'no job of its own: the second account colour',
  },
  from: { acc: 'Pink', err: 'Red', warn: 'Yellow', ok: 'Green', agent: 'Cyan', link: 'Purple', head: 'Orange' },
  truecolor: {
    bg: '#121316', sel: '#2d2f38',
    fg: '#e6e1d3', dim: '#989ba3', faint: '#666972',
    acc: '#f5aa4d', err: '#e8877c', warn: '#dccb7e', ok: '#b4d1af',
    agent: '#82bdbf', link: '#8cb2ee', head: '#e2a47b',
  },
  c256: {
    bg: 233, sel: 237,
    fg: 253, dim: 246, faint: 241,
    acc: 215, err: 210, warn: 186, ok: 151,
    agent: 109, link: 111, head: 180,
  },
  c16: {
    bg: 'black', sel: 'none',
    fg: 'default', dim: 'brightblack', faint: 'brightblack',
    acc: 'magenta', err: 'red', warn: 'yellow', ok: 'green',
    agent: 'cyan', link: 'brightblue', head: 'brightyellow',
  },
};

// ------------------------------------------------------------------- glyphs
//
// Every glyph is one cell wide in both tiers, so a layout never moves when
// the tier changes. Fixture text is ASCII; any other character on a screen
// comes from this table. A state that a word can say is said in a word, so
// the table stays short.

const GLYPHS = {
  cursor:    { unicode: '▌', ascii: '>', means: 'the selected row; the focused pane or field' },
  mark:      { unicode: '▪', ascii: '+', means: 'a marked row' },
  flag:      { unicode: '★', ascii: '!', means: 'flagged' },
  folded:    { unicode: '▸', ascii: '>', means: 'a folded thread' },
  unfolded:  { unicode: '▾', ascii: 'v', means: 'an unfolded thread' },
  ok:        { unicode: '✓', ascii: '+', means: 'done; verified' },
  bad:       { unicode: '✗', ascii: 'x', means: 'failed; not verified' },
  notice:    { unicode: '▲', ascii: '^', means: 'read this before acting' },
  agent:     { unicode: '◇', ascii: '%', means: 'written or said by an agent' },
  bar:       { unicode: '│', ascii: '|', means: 'quoted text; the divider beside the preview' },
  rule:      { unicode: '─', ascii: '-', means: 'the divider above the preview' },
  ellipsis:  { unicode: '…', ascii: '~', means: 'text cut to fit' },
  dot:       { unicode: '·', ascii: '-', means: 'separator' },
  shade:     { unicode: '░', ascii: ':', means: 'the cells an inline image occupies' },
  few:       { unicode: '∙', ascii: '.', means: 'a day with one event; two is a colon in both tiers' },
  many:      { unicode: '⋮', ascii: '#', means: 'a day with three events or more' },
};

const G = Object.fromEntries(Object.entries(GLYPHS).map(([name, g]) => [name, g.unicode]));
const TO_ASCII = Object.fromEntries(Object.values(GLYPHS).map((g) => [g.unicode, g.ascii]));

// ------------------------------------------------------------------- layout

const LAYOUT = {
  default: { width: 120, height: 40 },
  small: { width: 80, height: 24 },
  wide: { width: 180, height: 45 },
  wider: { width: 200, height: 50 },
  // Two left margins and no others: headings (tabs, date groups, titles,
  // the status line, the command line) start at `margin`, and what they
  // head (rows, header blocks, bodies, panel entries) at `indent`. The
  // cursor and the mark sit in the cells before `indent`.
  margin: 2,
  indent: 4,
  // List row after the indent: account, sender, a two-cell slot for the fold
  // mark, subject, then in fixed columns the words about the row, the date
  // and the flag. Columns are parted by gaps, widest where two kinds of text
  // meet: after the account and before the words. The account column is as
  // wide as the longest account name, to `account.most` letters, and is
  // drawn when more than one account is configured and the list is not
  // narrowed to one.
  row: { account: { most: 12, gap: 3, short: 4, shortGap: 2 }, from: 20, fromGap: 2, wordsGap: 3, words: 22, date: 6, dateGap: 2 },
  // The words have three forms: `4 messages · 3 files`, `4 msg · 3 files`
  // and `4m 3f`, and one colour, `agent`, so they read as one thing apart
  // from the subject and the date. A row with no words lets its subject run
  // on through their column.
  wordForms: { full: 22, mid: 15, tight: 9 },
  // Applied in order, each while the subject is narrower than its `under`:
  // the words give way first, so the subject is cut last.
  collapse: [
    { column: 'words', to: 15, under: 48, how: 'the words take their middle form' },
    { column: 'words', to: 9, under: 36, how: 'the words take their tight form' },
    { column: 'from', to: 16, under: 30, how: 'the sender narrows' },
    { column: 'account', to: 'short', under: 30, how: 'the account shortens to four letters' },
    { column: 'words', to: 0, under: 24, how: 'the words go' },
    { column: 'date', to: 0, under: 24, how: 'the date goes' },
    { column: 'account', to: 0, under: 24, how: 'the account goes' },
  ],
  // With the built-in palette an account's colour is its place in the
  // config, round `builtin`: blue, orange, green, and yellow for a fourth,
  // which the star in its row also wears. With a palette the
  // user loaded, the order is worked out from that palette's own colours by
  // `accountOrder()`: start with nothing chosen; at each step score every
  // token left in `pool` by its smallest separation (SEPARATION) from the
  // tokens already chosen and from the `beside` neutrals an account name
  // sits next to; take the best, a later token displacing an earlier one
  // only by more than `margin`; and when the best is under `floor`, a token
  // of `last` may take the step on the same terms. `warn` is last because
  // the star in the same row is `warn`; `acc`, `err` and `agent` are never
  // lent, `agent` because the words of the same row wear it. The
  // order is the same however many accounts there are, so adding one never
  // recolours the others. `[[account]] color = "<token>"` fixes one account
  // and the rest keep their turn.
  accounts: { builtin: ['link', 'head', 'ok', 'warn'], pool: ['link', 'head', 'ok'], last: ['warn'], never: ['acc', 'err', 'agent'], beside: ['fg', 'dim'], floor: 6, margin: 1 },
  // The words of a row are `builtin` with the built-in palette. With a
  // loaded palette, `wordsColour()` chooses once the accounts have theirs:
  // of `pool`, less what the configured accounts wear, the token whose
  // smallest separation from `beside` and from those account colours is
  // greatest, the earlier in `pool` on a tie, among those that keep
  // `onSel`:1 against the selected row's fill. If the best is under `floor`
  // there is no colour that would show: the words fall back to brightness,
  // numbers and the `bright` words in `fg` and the nouns in `dim`. Whatever
  // is chosen, the agent's own mark stays `agent`.
  words: { builtin: 'agent', pool: ['agent', 'link', 'head', 'ok'], never: ['acc', 'err', 'warn'], beside: ['fg', 'dim', 'warn'], floor: 6, onSel: 4.5, bright: ['invite', 'replied'] },
  // Rows are grouped by the day of their newest message, newest first. A
  // group label is not a row: the cursor steps over it.
  groups: ['Today', 'Yesterday', 'Past 7 days', 'Earlier'],
  // `ui.preview = auto | right | below | off`. Auto puts the preview beside
  // the list from `besideFrom` columns and below it otherwise, and hides it
  // when the terminal is shorter than `minHeight`. Beside, it takes `share`
  // of the width, no less than `least` and no more than `most` cells, so the
  // list gains from every column past the point where the text is at full
  // measure; below, `height` rows of the terminal, its divider included.
  preview: { besideFrom: 160, minHeight: 30, share: 0.38, least: 62, most: 82, height: (rows) => Math.floor(rows * 0.45) },
  // The header block is a label column and a value that wraps under itself.
  // Recipients and files wrap whole, one never split across rows, and stop
  // after `rows` rows with how many more there are: fewer in the preview,
  // which is a glance, than in the reader.
  header: { label: 9, rows: { preview: 2, reader: 6 } },
  // Body text wraps here however wide the terminal is.
  measure: 76,
  // The calendar (ADR-054). A week begins on Monday and the clock has 24
  // hours. In the month grid a day is `cell` cells: the cursor, two digits
  // and a mark for how much is on it. The week shows seven days from
  // `weekFrom` columns and three, around the cursor, below that; a row is
  // `slot` minutes, and the hours start at `dayFrom` unless the selected
  // event lies outside them. An event row is the account, its hours in
  // `time` cells, the title, and up to `words` cells of what is not normal
  // about it. Under `narrow` cells the account shortens, the hours become
  // the hour it starts, and the words keep room for the longest one alone,
  // so `needs answer` is never the thing that is cut.
  calendar: { cell: 4, weekFrom: 120, slot: 30, step: 2, dayFrom: '08:00', axis: 7, time: 11, timeNarrow: 7, words: 20, wordsNarrow: 12, narrow: 70 },
};

// ------------------------------------------------------------------- keymap
//
// The help screen and every hint line are drawn from this table. `id` is the
// name a `[keys]` entry remaps. `changes` lists what differs from the
// proposal in PLAN §6.2.

const KEYMAP = {
  timeoutMs: 500,
  changes: [
    '`q` is not a key. Esc goes back everywhere and, in the list, clears the filter and the marks; only `:q` and `:qa` quit.',
    'list: `P` moves the preview beside or below; Alt-j or Alt-l focuses it and Alt-k or Alt-h returns, by where it lies.',
    'list: `gt` and `gT` step to the next and previous account, then all again. `/` then Enter searches all mail.',
    '`A` opens the agent\'s menu wherever there is something for it to do; `a` is attachments and nothing else.',
    'reader: `i` answers an invitation (then `a`, `t` or `d`). The proposal had only `:invite`.',
    'compose: Left and Right choose the identity while From is focused. The proposal did not say.',
    'the list has no key for sync; `:sync` does it. `r` stays reply.',
    'nothing leaves the machine on `A` alone: it opens a menu, and the pick sends (ADR-031).',
    'the calendar is a place and not a section: `gc` opens it and `gm` returns, each side keeping where it was (ADR-054).',
    'calendar: `d` deletes, as it trashes in mail, so no key removes in one place and does something else in the other. The views have no letters: Tab and S-Tab step through them and 1, 2 and 3 pick one, as the digits pick a section in mail.',
    'calendar: a key mail already uses does the same thing here or nothing. `/` finds and `n` `N` step through matches; `J` `K` step through events as they step through the list from the reader; `o` lists links, the meeting\'s first, and Enter opens one; `i` answers; `zo` `zc` unfold and fold; `gt` `gT` step through accounts; `c` makes a new one of what the place holds. `e` is archive and `m` is read, so neither is bound: `E` edits.',
    'calendar: the one exception is the pair Bryan chose on 2026-11-24: `h` and `l` move by a day, where in mail they move by a section. Tab and the digits keep moving along the row of tabs in both.',
    'invitation: `p` proposes another time and `x` ignores, beside the three answers (ADR-057).',
  ],
  modes: [
    { mode: 'global', title: 'Everywhere', keys: [
      { id: 'global.command', keys: ':', does: 'command line' },
      { id: 'global.help', keys: '?', does: 'help; type to filter', hint: 'help' },
      { id: 'global.back', keys: 'Esc', does: 'back; clear find, marks', hint: 'back' },
      { id: 'global.section-prev', keys: 'h S-Tab', does: 'previous section' },
      { id: 'global.section-next', keys: 'l Tab', does: 'next section' },
      { id: 'global.section', keys: '1-9', does: 'section by position' },
      { id: 'global.undo', keys: 'u', does: 'undo the last action', hint: 'undo' },
      { id: 'global.agent', keys: 'A', does: 'what the agent can do', hint: 'agent' },
      { id: 'global.redraw', keys: '^l', does: 'redraw' },
      { id: 'global.calendar', keys: 'gc', does: 'the calendar', hint: 'calendar' },
    ] },
    { mode: 'list', title: 'List', keys: [
      { id: 'list.move', keys: 'j k', does: 'down, up' },
      { id: 'list.ends', keys: 'gg G', does: 'first, last row' },
      { id: 'list.page', keys: '^d ^u', does: 'half a page down, up' },
      { id: 'list.filter', keys: '/', does: 'find; Enter: all mail', hint: 'find' },
      { id: 'list.match', keys: 'n N', does: 'next, previous match' },
      { id: 'list.mark', keys: 'Space', does: 'mark the row', hint: 'mark' },
      { id: 'list.mark-range', keys: 'V', does: 'mark a range' },
      { id: 'list.open', keys: 'Enter', does: 'open; a draft, to edit', hint: 'open' },
      { id: 'list.preview', keys: 'p', does: 'show or hide preview' },
      { id: 'list.preview-place', keys: 'P', does: 'preview beside, below' },
      { id: 'list.browser', keys: 'B', does: 'sealed browser copy', hint: 'browser copy' },
      { id: 'list.focus-preview', keys: 'Alt-j Alt-l', does: 'focus the preview' },
      { id: 'list.account', keys: 'gt gT', does: 'next, previous account' },
      { id: 'list.archive', keys: 'e', does: 'archive', hint: 'archive' },
      { id: 'list.trash', keys: 'd', does: 'move to Trash', hint: 'trash' },
      { id: 'list.flag', keys: 's', does: 'flag or unflag', hint: 'flag' },
      { id: 'list.read', keys: 'm', does: 'mark read or unread', hint: 'read' },
      { id: 'list.move-to', keys: 'M', does: 'move to a folder', hint: 'move' },
      { id: 'list.compose', keys: 'c', does: 'write a new message', hint: 'write' },
      { id: 'list.reply', keys: 'r', does: 'reply', hint: 'reply' },
      { id: 'list.reply-all', keys: 'R', does: 'reply to everyone', hint: 'reply all' },
      { id: 'list.forward', keys: 'f', does: 'forward', hint: 'forward' },
      { id: 'list.unfold', keys: 'zo', does: 'unfold a thread', hint: 'unfold' },
      { id: 'list.fold', keys: 'zc', does: 'fold a thread', hint: 'fold' },
    ] },
    { mode: 'preview', title: 'Preview, focused', keys: [
      { id: 'preview.scroll', keys: 'j k ^d ^u gg G', does: 'scroll', hint: 'scroll' },
      { id: 'preview.focus-list', keys: 'Alt-k Alt-h', does: 'back to the list', hint: 'list' },
    ] },
    { mode: 'reader', title: 'Reader', keys: [
      { id: 'reader.scroll', keys: 'j k ^d ^u gg G', does: 'scroll' },
      { id: 'reader.thread', keys: 'n b', does: 'thread: next, previous', hint: 'next, previous' },
      { id: 'reader.list', keys: 'J K', does: 'list: next, previous' },
      { id: 'reader.links', keys: 'o', does: 'links', hint: 'links' },
      { id: 'reader.attachments', keys: 'a', does: 'attachments', hint: 'files' },
      { id: 'reader.invite', keys: 'i', does: 'answer an invitation', hint: 'answer' },
      { id: 'reader.browser', keys: 'B', does: 'sealed browser copy', hint: 'browser copy' },
      { id: 'reader.actions', keys: 'e d s m M r R f', does: 'as in the list' },
    ] },
    { mode: 'links', title: 'Links (after o)', keys: [
      { id: 'links.number', keys: '1-9', does: 'open that link', hint: 'open' },
      { id: 'links.choose', keys: 'j k Enter', does: 'choose, then open', hint: 'choose' },
      { id: 'links.close', keys: 'Esc', does: 'close', hint: 'close' },
    ] },
    { mode: 'attachments', title: 'Attachments (after a)', keys: [
      { id: 'attachments.choose', keys: 'j k', does: 'choose', hint: 'choose' },
      { id: 'attachments.save', keys: 'Enter', does: 'save to downloads', hint: 'save' },
      { id: 'attachments.open', keys: 'O', does: 'open, after asking', hint: 'open' },
      { id: 'attachments.close', keys: 'Esc', does: 'close', hint: 'close' },
    ] },
    { mode: 'agent', title: 'Agent (after A)', keys: [
      { id: 'agent.reply', keys: 'r', does: 'write a reply', hint: 'write a reply' },
      { id: 'agent.tidy', keys: 't', does: 'tidy, in compose', hint: 'tidy what I wrote' },
      { id: 'agent.link', keys: 'l', does: 'review a link, in links', hint: 'review this link' },
    ] },
    { mode: 'invite', title: 'Invitation (after i)', keys: [
      { id: 'invite.accept', keys: 'a', does: 'accept', hint: 'accept' },
      { id: 'invite.tentative', keys: 't', does: 'tentative', hint: 'tentative' },
      { id: 'invite.decline', keys: 'd', does: 'decline', hint: 'decline' },
      { id: 'invite.propose', keys: 'p', does: 'propose another time', hint: 'propose a time' },
      { id: 'invite.ignore', keys: 'x', does: 'ignore; nothing is sent', hint: 'ignore' },
    ] },
    // In the calendar these are searched before `global`, so h, l, Tab and
    // the digits move days and views here and sections only in mail.
    { mode: 'calendar', title: 'Calendar (after gc)', keys: [
      { id: 'calendar.day', keys: 'h l', does: 'previous, next day' },
      { id: 'calendar.move', keys: 'j k', does: 'month: week; else: event' },
      { id: 'calendar.period', keys: '[ ]', does: 'previous, next page' },
      { id: 'calendar.event', keys: 'J K', does: 'next, previous event', hint: 'next event' },
      { id: 'calendar.match', keys: 'n N', does: 'next, previous match', hint: 'next match' },
      { id: 'calendar.waiting', keys: 'I', does: 'next invite to answer', hint: 'next to answer' },
      { id: 'calendar.today', keys: 't', does: 'today', hint: 'today' },
      { id: 'calendar.goto', keys: 'gd', does: 'go to a date', hint: 'go to' },
      { id: 'calendar.view-cycle', keys: 'Tab S-Tab', does: 'next, previous view' },
      { id: 'calendar.view-number', keys: '1 2 3', does: 'month, week, day' },
      { id: 'calendar.open', keys: 'Enter', does: 'the day; then the event', hint: 'open' },
      { id: 'calendar.find', keys: '/', does: 'find; Enter: every day', hint: 'find' },
      { id: 'calendar.account', keys: 'gt gT', does: 'next, previous account' },
      { id: 'calendar.calendars', keys: 'C', does: 'show, hide calendars', hint: 'calendars' },
      { id: 'calendar.links', keys: 'o', does: 'links; the meeting first', hint: 'links' },
      { id: 'calendar.answer', keys: 'i', does: 'answer an invitation', hint: 'answer' },
      { id: 'calendar.new', keys: 'c', does: 'new event here', hint: 'new event' },
      { id: 'calendar.edit', keys: 'E', does: 'edit the event', hint: 'edit' },
      { id: 'calendar.delete', keys: 'd', does: 'delete the event', hint: 'delete' },
      { id: 'calendar.reminders', keys: 'gn', does: 'reminders on, off' },
      { id: 'calendar.mail', keys: 'gm', does: 'back to mail', hint: 'mail' },
    ] },
    { mode: 'event', title: 'Event', keys: [
      { id: 'event.scroll', keys: 'j k ^d ^u gg G', does: 'scroll' },
      { id: 'event.fold', keys: 'zo zc', does: 'unfold, fold More', hint: 'more' },
      { id: 'event.mail', keys: 'v', does: 'the mail it came in', hint: 'its mail' },
      { id: 'event.actions', keys: 'o i E d J K I', does: 'as in the calendar' },
    ] },
    { mode: 'calendars', title: 'Calendars (after C)', keys: [
      { id: 'calendars.choose', keys: 'j k', does: 'choose', hint: 'choose' },
      { id: 'calendars.toggle', keys: 'Space', does: 'show or hide', hint: 'show or hide' },
      { id: 'calendars.close', keys: 'Esc', does: 'close', hint: 'close' },
    ] },
    { mode: 'event-form', title: 'Event form', keys: [
      { id: 'event-form.field', keys: 'Tab S-Tab', does: 'next, previous field' },
      { id: 'event-form.choice', keys: 'Left Right', does: 'choose, where offered', hint: 'choose' },
      { id: 'event-form.notes', keys: 'Enter', does: 'edit notes in $EDITOR', hint: 'notes' },
      { id: 'event-form.save', keys: '^s', does: 'save; guests get mail', hint: 'save' },
      { id: 'event-form.close', keys: 'Esc', does: 'close; keep or discard', hint: 'close' },
    ] },
    { mode: 'compose', title: 'Compose', keys: [
      { id: 'compose.field', keys: 'Tab S-Tab', does: 'next, previous field' },
      { id: 'compose.identity', keys: 'Left Right', does: 'identity, on From', hint: 'identity' },
      { id: 'compose.editor', keys: 'Enter', does: 'edit body in $EDITOR', hint: 'edit' },
      { id: 'compose.attach', keys: '^a', does: 'attach a file', hint: 'attach' },
      { id: 'compose.summary', keys: '^s', does: 'send summary', hint: 'send' },
      { id: 'compose.save', keys: '^d', does: 'save the draft', hint: 'save draft' },
      { id: 'compose.close', keys: 'Esc', does: 'close; keep or discard', hint: 'close' },
    ] },
    { mode: 'command', title: 'Command line', keys: [
      { id: 'command.complete', keys: 'Tab S-Tab', does: 'complete, cycle' },
      { id: 'command.history', keys: 'Up Down', does: 'history, by prefix' },
      { id: 'command.erase', keys: '^w ^u', does: 'delete word, line' },
      { id: 'command.run', keys: 'Enter', does: 'run' },
      { id: 'command.cancel', keys: 'Esc', does: 'cancel' },
    ] },
    { mode: 'confirm', title: 'Confirm', keys: [
      { id: 'confirm.yes', keys: 'y Enter', does: 'yes', hint: 'yes' },
      { id: 'confirm.no', keys: 'n Esc', does: 'no', hint: 'no' },
      { id: 'confirm.other', keys: 'any other', does: 'cancels; key dropped' },
    ] },
    { mode: 'help', title: 'Help (after ?)', keys: [
      { id: 'help.scroll', keys: '^d ^u', does: 'scroll', hint: 'scroll' },
    ] },
  ],
};

const BINDING = Object.fromEntries(KEYMAP.modes.flatMap((m) => m.keys).map((k) => [k.id, k]));

// --------------------------------------------------------------- commands
//
// Every `:` command, in the order completion offers them: its name, what it
// takes, and the one line completion and help say of it. `choices` is what
// Tab offers after the name.

const COMMANDS = [
  { name: 'archive', args: '', does: 'archive what is selected' },
  { name: 'trash', args: '', does: 'move what is selected to Trash' },
  { name: 'flag', args: '', does: 'flag or unflag what is selected' },
  { name: 'read', args: '', does: 'mark what is selected read' },
  { name: 'unread', args: '', does: 'mark what is selected unread' },
  { name: 'move', args: '<folder>', does: 'move what is selected to a folder' },
  { name: 'section', args: '<name>', does: 'switch to a section by name' },
  { name: 'account', args: '[name]', does: 'show one account only; alone, every account' },
  { name: 'agent', args: 'reply|tidy|link', does: 'reply, tidy or link: hand text to the agent',
    choices: [
      { name: 'reply', args: '[guidance]', does: 'write the body from the thread' },
      { name: 'tidy', args: '', does: 'clean up the body you wrote' },
      { name: 'link', args: '<n>', does: 'ask about a link; the agent opens nothing' },
    ] },
  { name: 'compose', args: '[address]', does: 'write a new message' },
  { name: 'reply', args: '', does: 'reply to the sender' },
  { name: 'replyall', args: '', does: 'reply to everyone' },
  { name: 'forward', args: '', does: 'forward the message' },
  { name: 'send', args: '', does: 'the send summary, then confirm' },
  { name: 'browser', args: '', does: 'open the sealed browser copy; asks before images' },
  { name: 'save', args: '<n> [path]', does: 'save an attachment to downloads' },
  { name: 'open', args: '<n>', does: 'open an attachment, after asking' },
  { name: 'link', args: '<n>', does: 'open a link, after assessment' },
  { name: 'invite', args: 'accept|tentative|decline|propose|ignore', does: 'answer an invitation' },
  { name: 'calendar', args: '[month|week|day]', does: 'the calendar; alone, where it was' },
  { name: 'mail', args: '', does: 'back to mail, where it was' },
  { name: 'goto', args: '<date>', does: 'go to a date: 5 dec, fri, +2w' },
  { name: 'event', args: '[title]', does: 'a new event on the selected day' },
  { name: 'export', args: '[path]', does: 'write local events as .ics' },
  { name: 'notify', args: 'on|off', does: 'reminders on or off' },
  { name: 'unsubscribe', args: '', does: 'prepare an unsubscribe for review' },
  { name: 'preview', args: 'auto|right|below|off', does: 'where the preview sits' },
  { name: 'sync', args: '[all]', does: 'sync this section, or all' },
  { name: 'empty-trash', args: '[account]', does: 'delete an account\'s Trash for good' },
  { name: 'messages', args: '', does: 'the last results and errors' },
  { name: 'q', args: '', does: 'quit from the list; back elsewhere' },
  { name: 'qa', args: '', does: 'quit' },
];

// ------------------------------------------------------------------ fixture
//
// Synthetic throughout. Every address is under example.com, example.org or
// example.net, and every string is ASCII so the ASCII tier needs no
// transliteration.

const NOW = '2026-11-23 14:20';

const ACCOUNTS = [
  { name: 'personal', address: 'robin@example.com', identities: ['Robin Vale <robin@example.com>'] },
  { name: 'work', address: 'robin.vale@example.org', identities: ['Robin Vale <robin.vale@example.org>', 'Studio desk <desk@example.org>'] },
];

const FOLDERS = [
  { account: 'personal', name: 'Inbox', role: 'Inbox', total: 13 },
  { account: 'personal', name: 'Sent', role: 'Sent', total: 214 },
  { account: 'personal', name: 'Drafts', role: 'Drafts', total: 1 },
  { account: 'personal', name: 'Archive', role: 'Archive', total: 1902 },
  { account: 'personal', name: 'Trash', role: 'Trash', total: 37 },
  { account: 'work', name: 'Inbox', role: 'Inbox', total: 17 },
  { account: 'work', name: 'Sent', role: 'Sent', total: 388 },
  { account: 'work', name: 'Drafts', role: 'Drafts', total: 1 },
  { account: 'work', name: 'Archive', role: 'Archive', total: 6455 },
  { account: 'work', name: 'Trash', role: 'Trash', total: 212 },
];

const PEOPLE = {
  me: ['Robin Vale', 'robin@example.com'],
  meWork: ['Robin Vale', 'robin.vale@example.org'],
  imani: ['Imani Castell', 'imani@example.org'],
  priya: ['Priya Raman', 'priya@example.org'],
  priyaNet: ['Priya Raman', 'priya@example.net'],
  dmitri: ['Dmitri Volkov', 'dmitri@example.org'],
  lars: ['Lars Lindqvist', 'lars@example.org'],
  mei: ['Mei Tanaka', 'mei@example.org'],
  olu: ['Olu Adeyemi', 'olu@example.org'],
  ana: ['Ana Sousa', 'ana@example.org'],
  hiring: ['Hiring Desk', 'hiring@example.org'],
  backups: ['Backup Robot', 'backups@example.org'],
  status: ['Status Page', 'status@example.org'],
  tomas: ['Tomas Ferreira', 'tomas@example.net'],
  greta: ['Greta Holm', 'greta@example.com'],
  wen: ['Wen Zhao', 'wen@example.com'],
  jonas: ['Jonas Becker', 'jonas@example.com'],
  fernway: ['Fernway Weekly', 'news@fernway.example.net'],
  bank: ['Example Bank', 'security@examplebank.example.net'],
  parcels: ['Parcel Desk', 'track@parcels.example.net'],
  library: ['City Library', 'library@example.net'],
  rent: ['Rent Office', 'office@example.net'],
  billing: ['', 'billing@halden.example.com'],
};

const name = (who) => PEOPLE[who][0] || PEOPLE[who][1];
const address = (who) => PEOPLE[who][1];
const full = (who) => (PEOPLE[who][0] ? `${PEOPLE[who][0]} <${PEOPLE[who][1]}>` : PEOPLE[who][1]);

const HALDEN_BODY = [
  'Hi Robin,',
  'Attached is the third draft of the Halden contract. Legal went through it on Friday and the changes from v2 are small, but two of them need a decision from you before I can send it on.',
  '1. Payment terms. Halden asked for 45 days instead of 30. Finance can live with 45 if the first invoice is issued at signing rather than at delivery.',
  '2. The support clause. They want named contacts on our side. I have put you and Dmitri down for now; say if that should be someone else.',
  'Everything else is wording. The liability cap is unchanged, the term is still twelve months with a renewal notice of sixty days, and schedule B lists the same deliverables as before.',
  'If you can reply by Wednesday I will get the signed copy to them before their board meets on the 27th. If either point needs a call, I am free tomorrow after two.',
  'A note on process: Halden\'s procurement team will only accept the signed PDF from an address on our domain, so please do not forward this draft from a personal account.',
  { lines: ['Thanks,', 'Priya'] },
  { lines: ['-- ', 'Priya Raman', 'Commercial lead'] },
];

// Long enough to scroll in the preview at any size.
const MIGRATION_BODY = [
  'Hi Robin,',
  'Here is the plan for moving the staging database to the new cluster. Nothing in it is decided until you have read it, and the window itself is the part I most need your view on.',
  { h: 'The window' },
  'I would like Thursday from 20:00 to 23:00. Staging is quiet then, the nightly backup has not started, and the two of us who know the old cluster are both free. Friday is the fallback, though a Friday change leaves the weekend to find out what we missed.',
  { h: 'Before the window' },
  { list: ['Freeze schema changes on staging from Wednesday noon', 'Take a full dump and restore it on the new cluster as a rehearsal', 'Compare row counts and checksums table by table', 'Point one internal service at the rehearsal copy for a day'] },
  'The rehearsal matters more than the rest. Last time the restore itself was fine and the surprise was a collation difference that changed the order of two reports. If the checksums match and the reports read the same, I am comfortable.',
  { h: 'During the window' },
  { list: ['20:00 stop writers and take the final dump', '20:20 restore on the new cluster', '21:10 run the comparison again', '21:30 switch the connection strings and restart the services', '22:00 smoke tests, then an hour of watching'] },
  'If the comparison at 21:10 fails we stop there, start the writers again on the old cluster and try another night. Nothing has been switched at that point, so going back costs ten minutes.',
  { h: 'After' },
  'The old cluster stays up, read-only, for a week. After that I will ask you before it is removed. The backup job needs its target changed the same night, or the first nightly run will dump an idle database and report success.',
  { h: 'What I need from you' },
  'A yes or no on Thursday, and a second pair of eyes on the comparison script before Wednesday. It is short, and I would rather you found the hole in it than staging did.',
  { lines: ['Thanks,', 'Lars'] },
];

const NEWSLETTER_BODY = [
  { h: 'Fernway Weekly, 23 November' },
  { alt: 'Ridge loop at sunrise' },
  'Winter routes are open. Three of the five marked loops are groomed and the hut at Ridge saddle is staffed at weekends. Route notes and maps [1]',
  { h: 'Route status' },
  { table: [
    ['Route', 'Length', 'Status'],
    ['Ridge loop', '12 km', 'groomed'],
    ['Saddle traverse', '18 km', 'groomed'],
    ['Mill creek', '7 km', 'closed until 5 Dec'],
  ] },
  { h: 'Gear swap, 5 December' },
  'Bring what you have outgrown and take home what fits. Sign up for a table [2]',
  { quote: 'We cleared forty pairs of skis last year and nobody went home with what they brought. (Marta, swap organiser)' },
  { list: ['Doors at 10:00', 'Cash or card', 'Leftovers go to the school club'] },
  'Mill creek is closed for bridge work. Details and the detour [3]',
  { rule: true },
  { small: 'You get this because you joined Fernway in 2023. Unsubscribe [4] or view it in a browser [5]' },
];

const NEWSLETTER_LINKS = [
  { n: 1, host: 'fernway.example.net', shown: 'Route notes and maps', url: 'https://fernway.example.net/routes/winter', note: 'tracking removed' },
  { n: 2, host: 'shop.example.net', shown: 'Sign up for a table', url: 'https://shop.example.net/swap/tables' },
  { n: 3, host: 'fernway.example.net', shown: 'Details and the detour', url: 'https://fernway.example.net/closures/mill-creek' },
  { n: 4, host: 'fernway.example.net', shown: 'Unsubscribe', url: 'https://fernway.example.net/unsubscribe' },
  { n: 5, host: 'click.opentrack.example.net', shown: 'view it in a browser', url: 'https://click.opentrack.example.net/v/8f3a21', note: 'tracking service' },
];

const BANK_LINKS = [
  { n: 1, host: 'bank.example.com.device-check.example.net', shown: 'https://bank.example.com/confirm', url: 'https://bank.example.com.device-check.example.net/confirm', warning: 'text says bank.example.com' },
  { n: 2, host: 'examplebank.example.net', shown: 'Contact us', url: 'https://examplebank.example.net/contact' },
];

const AGENT_REPLY_BODY = [
  'Hi Priya,',
  '45 days is fine provided the first invoice goes out at signing. Dmitri and I are the right named contacts for the support clause.',
  'I have attached the signed copy and schedule B. Billing at Halden is copied so the invoice details reach them directly.',
  { lines: ['Thanks,', 'Robin'] },
];

// The reply open in compose when `:agent` is typed (ADR-031): the user's
// own, addressed by the user. `changed` marks what `:agent tidy` rewrote.
const OWN_REPLY = {
  id: 'draft-own', account: 'work', from: 'meWork', to: ['priya'], cc: ['dmitri'], date: '2026-11-23 14:21', origin: 'local',
  subject: 'Re: Contract draft v3 for the Halden account', replyTo: 'halden-1',
};
const AGENT_WROTE = [
  'Hi Priya,',
  '45 days is fine provided the first invoice goes out at signing. Dmitri and I are the right named contacts for the support clause.',
  'I will send the signed copy once the bank details are confirmed by phone.',
  { lines: ['Thanks,', 'Robin'] },
];
const AGENT_TIDIED = [
  'Hi Priya,',
  { changed: '45 days is fine, as long as the first invoice goes out at signing.' },
  { changed: 'Dmitri and I are the right contacts for the support clause.' },
  { lines: ['Thanks,', 'Robin'] },
];

const MAIL = [
  { id: 'invite', account: 'work', from: 'imani', to: ['meWork', 'lars', 'mei', 'dmitri'], date: '2026-11-23 14:05', unread: true,
    subject: 'Invitation: Q4 roadmap review, Thu 26 Nov 10:00',
    body: ['The pre-read is the roadmap document from last week. Please answer by Wednesday so I can book the right room.'],
    invite: { what: 'Q4 roadmap review', when: 'Thu 26 Nov 2026, 10:00 to 11:30', where: 'Room Fjord, or the call link [1]',
      who: 'Imani Castell, you and 3 others' },
    links: [{ n: 1, host: 'meet.example.org', shown: 'the call link', url: 'https://meet.example.org/r/q4-roadmap' }] },

  { id: 'cabin-1', thread: 'cabin', account: 'personal', from: 'tomas', to: ['me', 'greta', 'wen', 'jonas'], date: '2026-11-22 18:10',
    subject: 'Cabin weekend: who is driving?', snippet: 'We need two cars for six people and the skis.' },
  { id: 'cabin-2', thread: 'cabin', parent: 'cabin-1', account: 'personal', from: 'greta', to: ['tomas'], cc: ['me', 'wen', 'jonas'], date: '2026-11-22 19:02',
    subject: 'Re: Cabin weekend: who is driving?', snippet: 'I can take three if we leave by four.' },
  { id: 'cabin-3', thread: 'cabin', parent: 'cabin-2', account: 'personal', from: 'tomas', to: ['greta'], cc: ['me', 'wen', 'jonas'], date: '2026-11-22 21:30',
    subject: 'Re: Cabin weekend: who is driving?', snippet: 'Great. Then I will bring the food for Saturday.' },
  { id: 'cabin-4', thread: 'cabin', parent: 'cabin-1', account: 'personal', from: 'wen', to: ['tomas'], cc: ['me', 'greta', 'jonas'], date: '2026-11-23 08:15', unread: true,
    subject: 'Re: Cabin weekend: who is driving?', snippet: 'I cannot make Friday. Could someone collect me on Saturday morning?',
    body: ['I cannot make Friday. Could someone collect me on Saturday morning?'] },
  { id: 'cabin-5', thread: 'cabin', parent: 'cabin-3', account: 'personal', from: 'jonas', to: ['tomas'], cc: ['me', 'greta', 'wen'], date: '2026-11-23 13:40', unread: true,
    subject: 'Re: Cabin weekend: who is driving?', snippet: 'Count me in. Robin, can you take the second car?',
    body: ['Count me in. Robin, can you take the second car? I will split the fuel, and I can bring the roof box if yours is still at your brother\'s.'] },

  { id: 'backup', account: 'work', from: 'backups', to: ['meWork'], date: '2026-11-23 12:58', unread: true,
    subject: '[ops] Nightly backup finished with 2 warnings' },

  { id: 'halden-1', thread: 'halden', account: 'work', from: 'priya', to: ['meWork'], cc: ['dmitri'], date: '2026-11-23 09:14', flagged: true,
    subject: 'Contract draft v3 for the Halden account', snippet: 'Attached is the third draft of the Halden contract.',
    attachments: [{ name: 'contract-v3.pdf', size: '482 KB' }],
    security: { state: 'good', signer: 'priya', key: '7A1C55E0' }, body: HALDEN_BODY },
  { id: 'halden-2', thread: 'halden', parent: 'halden-1', account: 'work', from: 'dmitri', to: ['priya'], cc: ['meWork'], date: '2026-11-23 10:02',
    subject: 'Re: Contract draft v3 for the Halden account', snippet: 'Fine with being a named contact. 45 days works for me.' },
  { id: 'halden-3', thread: 'halden', parent: 'halden-2', account: 'work', from: 'priyaNet', to: ['meWork'], date: '2026-11-23 11:32', unread: true,
    subject: 'Re: Contract draft v3 for the Halden account', snippet: 'One more change before you sign: our bank details have changed.',
    attachments: [{ name: 'halden-bank-details.pdf', size: '96 KB' }],
    differs: { was: 'priya@example.org' },
    body: ['Hi Robin,', 'One more change before you sign: our bank details have changed, and the new ones are in the attached PDF. Please use them for the first invoice and confirm today, because the board date has moved up.', { lines: ['Priya'] }] },

  { id: 'newsletter', account: 'personal', from: 'fernway', to: ['me'], date: '2026-11-23 10:15', unread: true,
    subject: 'This week: winter routes, a gear swap and one trail closure',
    // Nine remote images on three hosts were not loaded; one of them is the
    // tracker's pixel on the tracker's host, which no answer ever loads.
    html: true, body: NEWSLETTER_BODY, links: NEWSLETTER_LINKS,
    remote: { images: 9, hosts: ['img.fernway.example.net', 'cdn.example.net', 'click.opentrack.example.net'], trackers: ['OpenTrack'], trackerHosts: ['click.opentrack.example.net'], trackerImages: 1 } },
  { id: 'migration', account: 'work', from: 'lars', to: ['meWork'], date: '2026-11-23 09:48', answered: true,
    subject: 'Staging database migration window', body: MIGRATION_BODY },
  { id: 'bank', account: 'personal', from: 'bank', to: ['me'], date: '2026-11-23 09:02', unread: true,
    subject: 'Action needed: confirm your new device', html: true, links: BANK_LINKS,
    body: ['We noticed a sign-in from a new device. To keep your account open, confirm the device within 24 hours:', 'https://bank.example.com/confirm [1]',
      'If you do not confirm, your account will be limited. Questions? Contact us [2]', { lines: ['Example Bank Security Team'] }] },
  { id: 'payroll', account: 'work', from: 'mei', to: ['meWork'], date: '2026-11-23 08:30', flagged: true,
    subject: 'Payroll corrections for November', security: { state: 'encrypted', key: '51F09A2C' },
    body: ['Two corrections before the run on Thursday: the overtime for the support rota, and one changed bank account. Both are in the sheet.'] },
  { id: 'photos', account: 'personal', from: 'greta', to: ['me'], date: '2026-11-22 20:44', unread: true,
    subject: 'Photos from Saturday',
    attachments: [
      { name: 'ridge-01.jpg', size: '2.4 MB', type: 'image/jpeg', inline: true, pixels: '1600x1067', cells: [48, 12] },
      { name: 'ridge-02.jpg', size: '3.1 MB', type: 'image/jpeg' },
      { name: 'group-shot.heic', size: '1.8 MB', type: 'image/heic' },
      { name: 'route.gpx', size: '84 KB', type: 'application/gpx+xml' },
    ],
    body: ['Here are the ones that came out. The first is the view from the saddle; the full set follows once I have sorted them.', { image: 0 }, { lines: ['G.'] }] },
  { id: 'parcel', account: 'personal', from: 'parcels', to: ['me'], date: '2026-11-22 16:20',
    subject: 'Your parcel is out for delivery' },
  { id: 'vault', account: 'work', from: 'olu', to: ['meWork'], date: '2026-11-22 11:05', unread: true,
    subject: 'New keys for the shared vault', security: { state: 'bad', signer: 'olu' },
    body: ['The vault keys rotate tonight. Install the attached key file before six or you will be locked out.'] },
  { id: 'budget', account: 'work', from: 'imani', to: ['meWork'], date: '2026-11-21 17:36', answered: true,
    subject: 'Budget numbers for the offsite' },
  { id: 'rack', account: 'personal', from: 'tomas', to: ['me'], date: '2026-11-21 12:12',
    subject: 'Bike rack: still want it?' },
  { id: 'loop', account: 'work', from: 'hiring', to: ['meWork'], date: '2026-11-21 09:40',
    subject: 'Interview loop: backend candidate, Tuesday', attachments: [{ name: 'candidate-cv.pdf', size: '210 KB' }] },
  { id: 'library', account: 'personal', from: 'library', to: ['me'], date: '2026-11-20 15:00',
    subject: 'Two items are due on 27 Nov' },
  { id: 'flames', account: 'work', from: 'dmitri', to: ['meWork'], date: '2026-11-20 11:18',
    subject: 'Lunch and learn: reading flame graphs' },
  { id: 'expense', account: 'work', from: 'ana', to: ['meWork'], date: '2026-11-20 09:05', unread: true, flagged: true,
    subject: 'Expense report needs one receipt' },
  { id: 'recipe', account: 'personal', from: 'wen', to: ['me'], date: '2026-11-19 19:27',
    subject: 'Recipe you asked for', attachments: [{ name: 'dumplings.pdf', size: '64 KB' }] },
  { id: 'resolved', account: 'work', from: 'status', to: ['meWork'], date: '2026-11-19 03:12',
    subject: 'Resolved: elevated error rate on the API' },
  { id: 'statement', account: 'personal', from: 'rent', to: ['me'], date: '2026-11-18 10:00',
    subject: 'December statement', attachments: [{ name: 'statement-2026-12.pdf', size: '118 KB' }] },
  { id: 'oncall', account: 'work', from: 'lars', to: ['meWork'], date: '2026-11-18 08:51',
    subject: 'On-call swap for the week of 7 Dec?' },
  { id: 'gym', account: 'personal', from: 'jonas', to: ['me'], date: '2026-11-17 18:03',
    subject: 'Climbing gym: new opening hours' },
  { id: 'vendor', account: 'work', from: 'mei', to: ['meWork'], date: '2026-11-14 16:45',
    subject: 'Notes from the vendor call', attachments: [{ name: 'vendor-call.md', size: '9 KB' }] },
  { id: 'bookclub', account: 'personal', from: 'greta', to: ['me'], date: '2026-11-12 21:10', answered: true,
    subject: 'Book club pick for December' },
];

const DRAFTS = [
  { id: 'draft-agent', account: 'work', from: 'meWork', to: ['priya', 'dmitri'], cc: ['billing'], date: '2026-11-23 13:52', origin: 'agent',
    subject: 'Re: Contract draft v3 for the Halden account', replyTo: 'halden-1', body: AGENT_REPLY_BODY,
    attachments: [{ name: 'contract-v3-signed.pdf', size: '512 KB' }, { name: 'schedule-b.xlsx', size: '38 KB' }], attachedTotal: '550 KB' },
  { id: 'draft-packing', account: 'personal', from: 'me', to: ['greta'], date: '2026-11-22 22:15', origin: 'local',
    subject: 'Packing list for the cabin' },
];

const OUTBOX = [
  { id: 'out-rack', account: 'personal', from: 'me', to: ['tomas'], date: '2026-11-23 14:20', state: 'held', sendsIn: '6s',
    subject: 'Re: Bike rack: still want it?' },
  { id: 'out-headcount', account: 'work', from: 'meWork', to: ['imani'], date: '2026-11-23 13:05', state: 'failed',
    subject: 'Offsite rooms', attachments: [{ name: 'rooms.xlsx', size: '31 MB' }],
    reason: 'refused by smtp.example.org: 552 message too large' },
];

// The calendar (ADR-054). Each account's own calendar is read from an
// address it gave (ADR-056); what tuimex alone holds is `local` (ADR-055).
const CALENDARS = [
  { account: 'personal', source: 'own', read: '2m' },
  { account: 'personal', source: 'local' },
  { account: 'work', source: 'own', read: '20s' },
  { account: 'work', source: 'local' },
];

const MEET = (room) => ({ host: 'meet.example.org', url: `https://meet.example.org/r/${room}` });

// `answer` is the user's own: needs, accepted, tentative, declined or
// proposed, and absent on an event nobody invited them to. `guests` are the
// others and what each has said. An event either has a `day`, with `days`
// when it runs over several, or repeats on weekdays between two dates.
const EVENTS = [
  { id: 'standup', account: 'work', title: 'Standup', start: '09:30', end: '09:45', weekdays: ['2026-11-02', '2026-12-31'], repeats: 'every weekday',
    link: MEET('standup'), organizer: 'lars', answer: 'accepted' },
  { id: 'climbing', account: 'personal', day: '2026-11-17', start: '18:00', end: '20:00', title: 'Climbing', where: 'the gym on Mill street' },
  { id: 'flames', account: 'work', day: '2026-11-20', start: '12:00', end: '13:00', title: 'Lunch and learn: reading flame graphs', organizer: 'dmitri', answer: 'accepted' },
  { id: 'halden-call', account: 'work', day: '2026-11-23', start: '15:00', end: '16:00', title: 'Halden contract call', link: MEET('halden'),
    organizer: 'priya', guests: [['dmitri', 'yes']], answer: 'accepted', reminder: '10 minutes before',
    notes: ['Payment terms and the support clause. Draft v3 is attached to Priya\'s mail of this morning.'] },
  { id: 'dentist', account: 'personal', day: '2026-11-24', start: '11:00', end: '11:45', title: 'Dentist', where: 'Kirkegata 12' },
  { id: 'one-to-one', account: 'work', day: '2026-11-25', start: '13:00', end: '13:30', title: '1:1 with Imani', organizer: 'imani', answer: 'accepted' },
  { id: 'roadmap', account: 'work', day: '2026-11-26', start: '10:00', end: '11:30', title: 'Q4 roadmap review', where: 'Room Fjord', link: MEET('q4-roadmap'),
    organizer: 'imani', guests: [['lars', 'yes'], ['mei', 'no'], ['dmitri', null]], answer: 'needs', mail: 'invite', reminder: '10 minutes before',
    notes: ['The pre-read is the roadmap document from last week. Please answer by Wednesday so I can book the right room.',
      { list: ['What shipped in Q3 and what slipped', 'The three bets for Q4', 'Headcount, once the budget is in'] }],
    more: [['Status', 'confirmed'], ['Shown as', 'busy'], ['Privacy', 'default'], ['By phone', '+47 21 00 00 00, code 4471'], ['Changed', '23 Nov 14:05, by the organizer']] },
  { id: 'vendor', account: 'work', day: '2026-11-26', start: '11:00', end: '12:00', title: 'Vendor call', link: MEET('vendor'), organizer: 'mei', answer: 'tentative' },
  // Accepted from tuimex on an account whose own calendar it cannot write.
  { id: 'migration', account: 'work', day: '2026-11-26', start: '20:00', end: '23:00', title: 'Staging migration window', organizer: 'lars', answer: 'accepted', local: true },
  // The link's text names one host and it opens another (ADR-018).
  { id: 'review', account: 'personal', day: '2026-11-27', start: '09:00', end: '09:30', title: 'Account review', where: 'https://bank.example.com/meet',
    link: { host: 'bank.example.com.device-check.example.net', url: 'https://bank.example.com.device-check.example.net/meet', shown: 'bank.example.com' },
    organizer: 'bank', answer: 'needs' },
  { id: 'library', account: 'personal', day: '2026-11-27', allDay: true, title: 'Library books due', local: true },
  { id: 'cabin', account: 'personal', day: '2026-11-27', days: 3, allDay: true, title: 'Cabin weekend', where: 'Ridge saddle hut' },
  { id: 'bookclub', account: 'personal', day: '2026-12-03', start: '19:00', end: '21:00', title: 'Book club', where: 'at Greta\'s' },
  { id: 'swap', account: 'personal', day: '2026-12-05', start: '10:00', end: '13:00', title: 'Gear swap', where: 'Fernway hall', local: true },
  { id: 'oncall', account: 'work', day: '2026-12-07', days: 7, allDay: true, title: 'On call' },
];

const SECTIONS = [
  { name: 'Inbox', query: 'in:inbox' },
  { name: 'Unread', query: 'in:inbox is:unread' },
  { name: 'Flagged', query: 'is:flagged' },
  { name: 'Drafts', query: 'in:drafts' },
  { name: 'Sent', query: 'in:sent' },
  { name: 'Archive', query: 'in:archive' },
  // Shown only while something is in the outbox.
  { name: 'Outbox', query: 'in:outbox', transient: true },
];

const SYNC = {
  outbox: { held: 1, sendsIn: '6s', failed: 1 },
  accounts: [
    { name: 'personal', state: 'ok', age: '1m' },
    { name: 'work', state: 'ok', age: '20s' },
  ],
};

// ------------------------------------------------------------------ wording
//
// Every message, notice and question, in the markup `m()` reads. One
// grammar: a mark only when there is something to mark, what happened in
// plain words, then the detail or the next key, dim, after a dot. A question
// is its text and the keys that answer it; `said()` keeps the keys on screen
// however narrow the terminal is.

const then = (detail) => `{dim| ${G.dot} ${detail}}`;
const YES_NO = [['y', 'yes'], ['n', 'no']];
const ask = (question, keys = YES_NO) => ({ ask: question, keys });
const count = (n, what) => (n === 1 ? `1 ${what}` : `${n} ${what}s`);

// As many host names as fit in `room` cells, then how many more.
function hostList(hosts, room) {
  const all = hosts.join(', ');
  if (all.length <= room) return all;
  const first = `${hosts[0]} and ${hosts.length - 1} more`;
  return first.length <= room ? first : count(hosts.length, 'host');
}

const SAY = {
  // Reader notices (PLAN §6.5): one line each, shown only when true. The
  // mark and what it says take the colour of how serious it is.
  remote: (r) => `{warn|${G.notice} Remote content not loaded}${then(`${count(r.images, 'image')}, ${count(r.hosts.length, 'host')} ${G.dot} tracker: ${r.trackers.join(', ')}`)}`,
  signedGood: (who, key) => `{ok|${G.ok} Signed by ${name(who)}}${then(`key ${key}`)}`,
  signedBad: (who) => `{err|${G.bad} Bad signature}${then(`changed after signing, or not signed by ${address(who)}`)}`,
  signedUnknown: (key) => `{warn|${G.notice} Signature not checked}${then(`gpg has no key ${key}`)}`,
  signedUnverified: `{warn|${G.notice} Signature not checked}${then('tuimex does not verify one yet')}`,
  encrypted: (key) => `{ok|${G.ok} Encrypted to you}${then(`key ${key}`)}`,
  encryptedUnopened: `{warn|${G.notice} Encrypted}${then('tuimex cannot open it yet')}`,
  differs: (was) => `{warn|${G.notice} Sender differs from the thread}${then(`earlier messages came from ${was}`)}`,
  agentDraft: (when) => `{agent|${G.agent} Written by an agent}${then(when)}`,

  // What the links panel says beside a link (ADR-018).
  link: {
    textSays: (host) => `text says ${host}`,
    credentials: 'address carries a sign-in name',
    bareIp: 'host is a bare IP address',
    mixedScripts: 'host mixes alphabets',
    blocked: 'will not open',
    noHost: '(no host)',
    tracking: 'tracking removed',
    tracker: 'tracking service',
  },

  // Send summary (PLAN §6.4).
  outside: `{warn|${G.notice} not in this thread}`,
  noKey: (who) => `not encrypted${then(`no key for ${who}`)}`,
  summaryAgent: (when) => `{agent|${G.agent} Written by an agent}${then(`${when} ${G.dot} files added by you`)}`,
  summaryAsk: (n, delay) => ask(`Send to ${n} recipients?`, [['y', `send, held ${delay}`], ['n', 'back']]),
  inviteAsk: (n, delay) => ask(`Save, and mail the invitation to ${n} guests?`, [['y', `save and send, held ${delay}`], ['n', 'back']]),

  // Results.
  archived: (what) => `{ok|${G.ok}} Archived ${what}${then('u to undo')}`,
  trashed: (what) => `{ok|${G.ok}} Trashed ${what}${then('u to undo')}`,
  moved: (what, folder) => `{ok|${G.ok}} Moved ${what} to ${folder}${then('u to undo')}`,
  flagged: (what) => `{ok|${G.ok}} Flagged ${what}${then('u to undo')}`,
  unflagged: (what) => `{ok|${G.ok}} Unflagged ${what}${then('u to undo')}`,
  markedRead: (what) => `{ok|${G.ok}} Marked ${what} read${then('u to undo')}`,
  markedUnread: (what) => `{ok|${G.ok}} Marked ${what} unread${then('u to undo')}`,
  undone: (what, where) => `{ok|${G.ok}} Undone${then(`${what} back in ${where}`)}`,
  ownTextBack: `{ok|${G.ok}} Undone${then('your own text is back')}`,
  held: (n, delay) => `Sending to ${n} recipients in ${delay}${then('u to stop')}`,
  sent: (n) => `{ok|${G.ok}} Sent to ${n} recipients`,
  unsent: `{ok|${G.ok}} Not sent${then('back in Drafts')}`,
  draftSaved: `{ok|${G.ok}} Draft saved`,
  draftDiscarded: `{ok|${G.ok}} Draft discarded`,
  attached: (file, size) => `{ok|${G.ok}} Attached ${file}${then(size)}`,
  // A message the server has not numbered yet cannot be deleted by name
  // (ADR-041); it stays and the result says how many did.
  emptied: (n, account, kept) => `{ok|${G.ok}} Deleted ${count(n, 'message')} from ${account}'s Trash${kept ? then(`${kept} kept, not yet on the server`) : ''}`,

  // Under way: no mark.
  opening: (what) => `Opening ${what}`,
  saving: (file) => `Saving ${file}`,
  openingCopy: (detail) => `Opening the sealed copy in your browser${then(detail)}`,
  copyLoads: (n, hosts) => `${count(n, 'image')} load from ${count(hosts, 'host')}`,
  copyLoadsNothing: 'nothing remote loads',
  openingUnsubscribe: 'Opening the unsubscribe page',
  syncing: (what) => `Syncing ${what}`,
  showingAll: 'Showing every account',
  showingOnly: (account) => `Showing ${account} only${then('gt for the next, :account for all')}`,
  agentStopped: `Agent stopped${then('nothing changed')}`,

  // Errors start with E and name the command or key; the detail ends on the
  // next useful key.
  refused: (account, what, reason) => `{err|E :move: ${account} refused it}${then(`${reason} ${G.dot} ${what} back in Inbox`)}`,
  noFolder: (typed, account) => `{err|E :move: no folder "${typed}" in ${account}}${then('Tab completes')}`,
  noRole: (command, role, account) => `{err|E ${command}: no ${role} folder in ${account}}`,
  nameFolder: `{err|E :move: name a folder}${then('Tab completes')}`,
  unknown: (typed, nearest) => `{err|E not a command: :${typed}}${then(`did you mean :${nearest}? Tab to fix`)}`,
  nothingSelected: (command) => `{err|E ${command}: nothing is selected}`,
  noTarget: (command) => `{err|E :${command}: a target cannot be named}${then('it takes the selected or marked rows')}`,
  nothingToUndo: `{err|E undo: nothing to undo}`,
  noSection: (typed) => `{err|E :section: no section "${typed}"}${then('Tab completes')}`,
  noAccount: (command, typed) => `{err|E :${command}: no account "${typed}"}${then('Tab completes')}`,
  noFilter: `{err|E n: no filter}${then('/ to find')}`,
  waitingToGo: `{err|E open: it is waiting to go}${then('u takes it back')}`,
  noMessageOpen: (command) => `{err|E :${command}: no message is open}${then('Enter opens one')}`,
  draftOpen: (command) => `{err|E :${command}: a draft is open}${then('Esc closes it')}`,
  giveNumber: (command, takes) => `{err|E :${command}: give a number}${then(`:${command} ${takes}`)}`,
  noLinks: `{err|E o: this message has no links}`,
  noFiles: `{err|E a: this message has no files}`,
  notInvitation: `{err|E i: this message is not an invitation}`,
  noLink: (command, n) => `{err|E ${command}: this message has no link ${n}}${then('o lists them')}`,
  noFile: (command, n) => `{err|E ${command}: this message has no file ${n}}${then('a lists them')}`,
  notOpened: (command, n, kind) => `{err|E ${command} ${n}: ${kind} not opened}${then('only http, https and mailto')}`,
  noUnsubscribe: `{err|E :unsubscribe: this message names no way to}`,
  nobodyToSendTo: `{err|E :send: there is nobody to send to}${then('fill To')}`,
  noDraft: (command) => `{err|E :${command}: no draft is open}${then('c writes one')}`,

  // Confirms: the question, then the keys.
  emptyTrash: (n, account) => ask(`Delete ${count(n, 'message')} in ${account}'s Trash for good?`),
  createFolder: (account, role) => ask(`${account} has no ${role} folder. Create one on its server?`),
  openLink: (host) => ask(`Open ${host}?`),
  // The agent is offered only when an [agent] command is configured.
  deceptive: (shown, host, agent) => ask(`{warn|${G.notice}} Text says ${shown}; opens ${host}. Open?`, agent ? [...YES_NO, ['A', 'ask the agent']] : YES_NO),
  openFile: (file, mime) => ask(`Open ${file} (${mime}) in the system's program for it?`),
  unsubscribeAt: (host) => ask(`Unsubscribe at ${host}?`),
  loadImages: (n, hosts, room) => ask(`Load ${count(n, 'image')} from ${hostList(hosts, room)}?`),
  answer: ask('Answer?', [['a', 'accept'], ['t', 'tentative'], ['d', 'decline'], ['p', 'propose a time'], ['x', 'ignore'], ['Esc', 'cancel']]),
  opens: (url) => `{dim|opens} ${url}`,

  // `:agent` (ADR-031). The agent's mark leads whatever the agent is doing
  // or has said; what it was not shown is on the status line.
  agentWriting: (n) => `{agent|${G.agent} Writing a reply}${then(`from the ${n} messages of the thread`)}`,
  agentTidying: `{agent|${G.agent} Tidying your text}`,
  agentTidied: (when) => `{agent|${G.agent} Tidied by an agent}${then(when)}`,
  agentAsking: `{agent|${G.agent} Asking the agent}${then('it reads the address and the message, and opens nothing')}`,
  agentAdvice: (advice, why) => `{agent|${G.agent} Agent advises ${advice}}${then(why)}`,
  adviceOnly: `{dim|Advice only. The agent did not open the link.}`,
  replaced: (what) => `{dim|agent saw placeholders for} ${what}`,
  wrote: `{ok|${G.ok}} Reply written${then('u to undo')}`,
  tidied: (n) => `{ok|${G.ok}} Tidied ${n} paragraphs${then('u for your own text')}`,
  agentFailed: (fn, why) => `{err|E :agent ${fn}: ${why}}${then('nothing changed')}`,
  agentAbsent: `{err|E not a command: :agent}${then('no [agent] table in the config')}`,
  agentTakes: `{err|E :agent: it takes reply, tidy or link}`,
  agentKeyAbsent: `{err|E A: no agent is set up}${then('add an [agent] table to the config')}`,
  agentUnsent: '{dim|nothing is sent until you pick}',

  // Before the first frame, and while a body or an answer is on its way.
  starting: 'Opening mail',
  rebuilding: (done, total) => `{dim|Rebuilding the index ${G.dot} ${done} of ${total}}`,
  fetching: '{dim|Fetching this message.}',
  fetchingForCopy: `Fetching this message${then('the browser copy opens when it is here')}`,
  searching: 'Searching all mail.',

  // Finding and searching.
  finding: (here) => `{dim|${here} here ${G.dot}} Enter {dim|searches all mail}`,
  noMatch: (query, messages, accounts) => [`Nothing matches "${query}".`, `Searched ${messages} messages in ${count(accounts, 'account')}.`],

  // The calendar (ADR-054). An answer says which account's calendar took
  // it, and `local` when that is tuimex's own copy (ADR-055, ADR-057).
  answered: (how, what, where) => `{ok|${G.ok}} ${how} ${what}${then(`in ${where} ${G.dot} u to undo`)}`,
  proposed: (when, who) => `{ok|${G.ok}} Proposed ${when} to ${who}${then('u to undo')}`,
  ignored: (what) => `{ok|${G.ok}} Ignored ${what}${then(`nothing sent ${G.dot} u to undo`)}`,
  unanswered: (what) => `{ok|${G.ok}} Undone${then(`${what} is not answered`)}`,
  added: (what, where) => `{ok|${G.ok}} Added ${what} to ${where}${then('u to undo')}`,
  deleted: (what) => `{ok|${G.ok}} Deleted ${what}${then('u to undo')}`,
  restored: (what) => `{ok|${G.ok}} Undone${then(`${what} is back`)}`,
  joining: (host) => `Opening ${host}`,
  // A repeating event is asked about before it is changed or deleted.
  whichOnes: (verb) => ask(`${verb} which?`, [['t', 'this one'], ['a', 'all of them'], ['Esc', 'cancel']]),
  deletedOne: (what, day) => `{ok|${G.ok}} Deleted ${what} on ${day}${then('u to undo')}`,
  matching: (n, query) => `{dim|${n} ${n === 1 ? 'event matches' : 'events match'} "${query}"}`,
  proposes: (when) => `{dim|proposes} ${when}`,
  goesTo: (day) => `{dim|goes to} ${day}`,
  remindersOff: `Reminders off${then('gn turns them on')}`,
  remindersOn: 'Reminders on',
  freeDay: 'Nothing on this day.',
  noCalendar: ['No calendar yet.', '`tuimex setup` asks each account for the address of its calendar.'],
  onlyHere: 'local, only in tuimex',
  readFrom: (age) => `its own calendar, read ${age} ago`,
  needsAnswer: (who) => `{warn|${G.notice} Needs your answer}${then(`asked by ${who}`)}`,
  onlyInTuimex: (account) => `{warn|${G.notice} Only in tuimex}${then(`${account}'s own calendar does not have it ${G.dot} :export writes it out`)}`,
  waiting: (n) => `{warn|${n} to answer}`,
  noEvent: (key) => `{err|E ${key}: no event is selected}`,
  noEventLink: `{err|E o: this event has no link}`,
  notInvited: `{err|E i: nobody asked for an answer to this event}`,
  notADate: (typed) => `{err|E gd: not a date: "${typed}"}${then('2026-12-05, 5 dec, fri, +2w')}`,
  // The right of the status line, with what mail puts there.
  reminder: (what, lead) => `{warn|${G.notice} ${what} in ${lead}}${then('o for its link')}`,
  remindersAreOff: '{dim|reminders off}',
  calendarUnread: (account, last) => `{err|${account}'s calendar not read}${then(`last read ${last} ago`)}`,

  // The right of the status line: only what is not normal (PLAN §4.2).
  sending: (delay) => `sending in ${delay}`,
  notSent: (n) => `{err|${n} not sent}`,
  syncError: (account, reason, retry, last) => `{err|${account} ${reason}}${then(`retry in ${retry} ${G.dot} last synced ${last}`)}`,
  firstSync: (account, done, total) => `${account} first sync${then(`${done} of ${total}`)}`,
  empty: (last) => ['Nothing here.', `Synced ${last} ago.`],
};

// --------------------------------------------------------------- primitives

const run = (text, fg = 'fg', style = {}) => ({ t: String(text), fg, bg: style.bg || null, b: !!style.b, u: !!style.u });
const gap = (n) => run(' '.repeat(Math.max(0, n)));
const cells = (row) => row.reduce((n, r) => n + [...r.t].length, 0);
const explode = (row) => row.flatMap((r) => [...r.t].map((ch) => ({ ...r, t: ch })));

function fit(row, width, ellipsis = true) {
  const n = cells(row);
  if (n <= width) return n < width ? [...row, gap(width - n)] : row.slice();
  if (width <= 0) return [];
  const keep = ellipsis ? width - 1 : width;
  const out = [];
  let used = 0;
  for (const r of row) {
    const chars = [...r.t];
    if (used + chars.length <= keep) { out.push(r); used += chars.length; continue; }
    if (keep > used) out.push({ ...r, t: chars.slice(0, keep - used).join('') });
    break;
  }
  if (ellipsis) out.push({ ...(out[out.length - 1] || run('')), t: G.ellipsis, u: false });
  return out;
}

// Cuts the middle and keeps both ends: a question that is too wide ends on
// the last part of the host it names, which is the part that says whose it
// is.
function fitMiddle(row, width) {
  const all = explode(row);
  if (all.length <= width) return row;
  const head = Math.floor((width - 1) / 2);
  return [...all.slice(0, head), { ...all[head], t: G.ellipsis }, ...all.slice(all.length - (width - 1 - head))];
}

const leftRight = (left, right, width) => [...fit(left, width - cells(right)), ...right];
const fill = (row, bg) => row.map((r) => (r.bg ? r : { ...r, bg }));
const indent = (rows, n = LAYOUT.indent) => rows.map((row) => [gap(n), ...row]);
const heading = (row) => [gap(LAYOUT.margin), ...row];

// `{fg|text}`, `{fg.b|text}`, `{fg.u|text}`; anything else is plain text.
function m(markup) {
  const out = [];
  const re = /\{([a-z0-9.]+)\|([^}]*)\}/g;
  let at = 0;
  let found;
  while ((found = re.exec(markup))) {
    if (found.index > at) out.push(run(markup.slice(at, found.index)));
    const [fg, ...flags] = found[1].split('.');
    out.push(run(found[2], fg, { b: flags.includes('b'), u: flags.includes('u') }));
    at = re.lastIndex;
  }
  if (at < markup.length) out.push(run(markup.slice(at)));
  return out;
}

function wrap(text, width) {
  const lines = [];
  let line = '';
  for (const word of text.split(' ')) {
    if (!line) line = word;
    else if (line.length + 1 + word.length <= width) line += ' ' + word;
    else { lines.push(line); line = word; }
  }
  if (line) lines.push(line);
  return lines;
}

// Underlines every case-insensitive occurrence of `needle`.
function highlight(text, needle, fg, style = {}) {
  if (!needle) return [run(text, fg, style)];
  const out = [];
  const lower = text.toLowerCase();
  let at = 0;
  for (let i = lower.indexOf(needle); i >= 0; i = lower.indexOf(needle, at)) {
    if (i > at) out.push(run(text.slice(at, i), fg, style));
    out.push(run(text.slice(i, i + needle.length), 'fg', { b: true, u: true }));
    at = i + needle.length;
  }
  if (at < text.length) out.push(run(text.slice(at), fg, style));
  return out;
}

function overlay(base, x, top) {
  const under = explode(base);
  explode(top).forEach((cell, i) => { under[x + i] = cell; });
  return under;
}

const caret = run(' ', 'bg', { bg: 'acc' });
const typed = (text) => heading([run(text), caret]);

// The command line saying something: a line of SAY, or a question with its
// keys. The keys always show; a question too wide for the row loses its
// middle.
function said(what, width = LAYOUT.default.width) {
  if (typeof what === 'string') return heading(m(what));
  const keys = m(`{dim|  }` + what.keys.map(([key, does]) => `${key} {dim|${does}}`).join(`{dim| ${G.dot} }`));
  return [...fitMiddle(heading(m(what.ask)), width - cells(keys) - LAYOUT.margin), ...keys];
}

// A label and its value, the form of every header block.
const field = (label, value) => [run(label.padEnd(9), 'dim'), ...value];

// The one hint line: a few keys for what is selected, drawn from the keymap.
function hint(...entries) {
  const parts = entries.map((entry) => (Array.isArray(entry) ? entry : [BINDING[entry].keys, BINDING[entry].hint]));
  return heading(m(parts.map(([key, does]) => `${key} {dim|${does}}`).join(`{dim| ${G.dot} }`)));
}

// A panel takes the rows above the status line instead of floating over
// them: a blank row, a title, its entries. It needs no border and no fill,
// so it reads the same in sixteen colours.
function panel(title, entries, selected) {
  return [[], heading([run(title, 'fg', { b: true })]), ...entries.map((row, i) => {
    const line = [i === selected ? run(G.cursor, 'acc') : gap(1), gap(LAYOUT.indent - 1), ...row];
    return i === selected ? { selected: true, row: line } : line;
  })];
}
const settle = (rows, width) => rows.map((row) => (row.selected ? fill(fit(row.row, width), 'sel') : fit(row, width)));

// What completion offers: a name, what it takes, and one line about it.
const choices = (list) => list.map((c) => [run(`${c.name} ${c.args || ''}`.padEnd(24)), run(c.does, 'dim')]);

// head, as much of the body as fits, a blank row, the status line, the
// command line.
function frame(width, height, head, body, tail, status, command) {
  const foot = [...settle(tail || [], width), [], status, command || []];
  const room = height - head.length - foot.length;
  const middle = body.slice(0, room);
  while (middle.length && !cells(middle[middle.length - 1])) middle.pop();
  while (middle.length < room) middle.push([]);
  return [...head, ...middle, ...foot];
}

// --------------------------------------------------------------------- time

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

const parts = (date) => { const [day, time] = date.split(' '); const [year, month, dom] = day.split('-').map(Number); return { year, month, dom, time }; };
const dayNumber = (date) => { const p = parts(date); return Date.UTC(p.year, p.month - 1, p.dom) / 86400000; };
const daysAgo = (date) => dayNumber(NOW) - dayNumber(date);

function groupOf(date) {
  const age = daysAgo(date);
  return LAYOUT.groups[age <= 0 ? 0 : age === 1 ? 1 : age <= 7 ? 2 : 3];
}

// The time, when the row sits under the label of its own day; otherwise
// the day and month.
function shortDate(date, group) {
  const p = parts(date);
  return daysAgo(date) <= 1 && groupOf(date) === group ? p.time : `${p.dom} ${MONTHS[p.month - 1]}`;
}

function longDate(date) {
  const p = parts(date);
  const weekday = DAYS[new Date(Date.UTC(p.year, p.month - 1, p.dom)).getUTCDay()];
  return `${weekday} ${p.dom} ${MONTHS[p.month - 1]} ${p.year} ${p.time}`;
}

// ------------------------------------------------------------- list: items

function threadsOf(mail) {
  const threads = new Map();
  for (const msg of mail) {
    const key = msg.thread || msg.id;
    if (!threads.has(key)) threads.set(key, []);
    threads.get(key).push(msg);
  }
  const latest = (msgs) => msgs.reduce((a, b) => (a.date > b.date ? a : b));
  return [...threads.entries()]
    .map(([key, msgs]) => ({ key, msgs, latest: latest(msgs) }))
    .sort((a, b) => (a.latest.date < b.latest.date ? 1 : -1));
}

const filesIn = (msgs) => msgs.reduce((n, x) => n + (x.attachments ? x.attachments.length : 0), 0);

const single = (msg) => ({
  msg, ids: [msg.id], size: 1, depth: 0, subject: msg.subject, date: msg.date, group: groupOf(msg.date),
  unread: !!msg.unread, flagged: !!msg.flagged, answered: !!msg.answered,
  files: filesIn([msg]), invite: !!msg.invite,
});

function folded(thread) {
  const { msgs, latest } = thread;
  const root = msgs.find((x) => !x.parent);
  return {
    ...single(latest), ids: msgs.map((x) => x.id), size: msgs.length, fold: 'folded', subject: root.subject,
    unread: msgs.some((x) => x.unread), flagged: msgs.some((x) => x.flagged), files: filesIn(msgs),
  };
}

// An unfolded thread is its root and then each reply under what it answers,
// two cells deeper. The whole thread stays in the group of its newest
// message.
function unfolded(thread) {
  const { msgs, latest } = thread;
  const root = msgs.find((x) => !x.parent);
  const group = groupOf(latest.date);
  const out = [{ ...single(root), fold: 'open', group }];
  const walk = (parent, depth) => {
    for (const child of msgs.filter((x) => x.parent === parent.id)) {
      out.push({ ...single(child), depth, subject: child.snippet, reply: true, group });
      walk(child, depth + 1);
    }
  };
  walk(root, 1);
  return out;
}

// Rows of a section, newest thread first. `open` names the threads shown
// unfolded.
function items(mail, open = []) {
  return threadsOf(mail).flatMap((thread) => {
    if (thread.msgs.length === 1) return [single(thread.msgs[0])];
    return open.includes(thread.key) ? unfolded(thread) : [folded(thread)];
  });
}

const without = (mail, ids) => mail.filter((x) => !ids.includes(x.id));
const patched = (mail, id, change) => mail.map((x) => (x.id === id ? { ...x, ...change } : x));
const indexOf = (rows, id) => rows.findIndex((item) => item.msg.id === id);
const sentTo = (msg) => 'to ' + msg.to.map(name).join(', ');

function outboxItem(msg) {
  return { ...single(msg), from: sentTo(msg), state: msg.state, reason: msg.reason, sendsIn: msg.sendsIn, unread: true };
}

// A search lists messages, not threads, from every folder; a row says its
// folder when that is not the Inbox, and shows the words around a match that
// is only in the body.
function searchItem(found) {
  const msg = found.msg || MAIL.find((x) => x.id === found.id);
  return { ...single(msg), from: found.folder === 'Sent' ? sentTo(msg) : undefined, folder: found.folder, excerpt: found.excerpt };
}

const INBOX = items(MAIL);
// A tab shows a number only when the number means "this needs you".
const countsOf = (rows) => ({ Inbox: rows.filter((item) => item.unread).length, Flagged: rows.filter((item) => item.flagged).length });
const COUNTS = { ...countsOf(INBOX), Outbox: OUTBOX.filter((x) => x.state === 'failed').length };

// What an action would take, for the left of the status line.
function target(item, marks) {
  if (marks && marks.length) return `${marks.length} marked`;
  if (item.fold === 'folded') return `thread of ${item.size}`;
  return item.reply || item.fold === 'open' ? '1 message of the thread' : '1 message';
}
const position = (at, total, what) => `{dim|${at} of ${total} ${G.dot}} ${what}`;
const ACCOUNT_NAMES = ACCOUNTS.map((a) => a.name);

// How far apart two colours are for a viewer with normal, protan or deutan
// vision: the smallest of the three distances, in OKLab times 100, where
// about 2 is just noticeable. Each colour goes from sRGB to linear light,
// through the viewer's matrix (Vienot, Brettel and Mollon, 1999; negatives
// cut to 0), then to OKLab.
const SEPARATION = {
  viewers: {
    normal: [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
    protan: [[0.11238, 0.88762, 0], [0.11238, 0.88762, 0], [0.00401, -0.00401, 1]],
    deutan: [[0.29275, 0.70725, 0], [0.29275, 0.70725, 0], [-0.02234, 0.02234, 1]],
  },
  toCone: [[0.4122214708, 0.5363325363, 0.0514459929], [0.2119034982, 0.6806995451, 0.1073969566], [0.0883024619, 0.2817188376, 0.6299787005]],
  toLab: [[0.2104542553, 0.793617785, -0.0040720468], [1.9779984951, -2.428592205, 0.4505937099], [0.0259040371, 0.7827717662, -0.808675766]],
};
const times = (matrix, v) => matrix.map((row) => row[0] * v[0] + row[1] * v[1] + row[2] * v[2]);
function separation(a, b) {
  const linear = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  const seen = (hex, viewer) => times(SEPARATION.toLab, times(SEPARATION.toCone, times(viewer, linear(hex)).map((v) => Math.max(0, v))).map(Math.cbrt));
  return Math.min(...Object.values(SEPARATION.viewers).map((viewer) => {
    const [p, q] = [seen(a, viewer), seen(b, viewer)];
    return 100 * Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]);
  }));
}

// The order accounts take colours in, from a loaded palette's own values
// (LAYOUT.accounts says how).
function accountOrder(palette) {
  const rule = LAYOUT.accounts;
  const order = [];
  const score = (token) => Math.min(...[...order, ...rule.beside].map((other) => separation(palette[token], palette[other])));
  const best = (tokens) => tokens.reduce((held, token) => (held == null || score(token) > score(held) + rule.margin ? token : held), null);
  let pool = rule.pool.slice();
  let last = rule.last.slice();
  while (pool.length + last.length) {
    let pick = best(pool);
    const late = best(last);
    if (late != null && (pick == null || (score(pick) < rule.floor && score(late) > score(pick) + rule.margin))) pick = late;
    order.push(pick);
    pool = pool.filter((token) => token !== pick);
    last = last.filter((token) => token !== pick);
  }
  return order;
}

// WCAG contrast of two colours, 1 to 21.
function contrast(a, b) {
  const luminance = (hex) => { const [r, g, b2] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)); return 0.2126 * r + 0.7152 * g + 0.0722 * b2; };
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// The colour of a row's words on a loaded palette, given the tokens the
// configured accounts wear; null when none would show (LAYOUT.words says
// how).
function wordsColour(palette, accountsInUse) {
  const rule = LAYOUT.words;
  const beside = [...rule.beside, ...accountsInUse];
  const score = (token) => Math.min(...beside.map((other) => separation(palette[token], palette[other])));
  const free = rule.pool.filter((token) => !accountsInUse.includes(token) && contrast(palette[token], palette.sel) >= rule.onSel);
  const best = free.reduce((held, token) => (held == null || score(token) > score(held) ? token : held), null);
  return best != null && score(best) >= rule.floor ? best : null;
}

// Set by build(): the built-in order and colour, or the ones worked out for
// the palette it was given.
let ACCOUNT_ORDER = LAYOUT.accounts.builtin;
let WORDS_COLOUR = () => LAYOUT.words.builtin;
const accountColour = (account, names = ACCOUNT_NAMES) => ACCOUNT_ORDER[names.indexOf(account) % ACCOUNT_ORDER.length];

// An account's name in `width` cells. A name that must be cut first loses
// what it shares with another account's, so `gmail-home` and `gmail-work`
// become `home` and `work` and never the same four letters.
function accountShort(account, width, names = ACCOUNT_NAMES) {
  if (account.length <= width) return account;
  const shared = (other) => { let n = 0; while (n < account.length && account[n] === other[n]) n++; return n; };
  const prefix = Math.max(0, ...names.filter((other) => other !== account).map(shared));
  return account.slice(prefix >= 3 ? prefix : 0).slice(0, width);
}

// -------------------------------------------------------------- list: rows

function columns(width, narrowed, accounts = ACCOUNT_NAMES) {
  const R = LAYOUT.row;
  const letters = accounts.length < 2 || narrowed ? 0 : Math.min(R.account.most, Math.max(...accounts.map((a) => accountShort(a, R.account.most, accounts).length)));
  const c = { letters, account: letters ? letters + R.account.gap : 0, from: R.from, words: R.words, date: R.date };
  const fixed = () => LAYOUT.indent + c.account + c.from + R.fromGap + 2 + (c.words ? R.wordsGap + c.words : 0) + (c.date ? R.dateGap + c.date : 0) + 2 + LAYOUT.margin;
  for (const step of LAYOUT.collapse) {
    if (width - fixed() >= step.under) continue;
    if (step.column === 'account' && step.to === 'short') { if (c.letters > R.account.short) { c.letters = R.account.short; c.account = R.account.short + R.account.shortGap; } }
    else if (step.column === 'account') { c.letters = 0; c.account = 0; }
    else c[step.column] = Math.min(c[step.column], step.to);
  }
  c.subject = width - fixed();
  return c;
}

// The words about a row, most useful first, in the form the column has room
// for: [full, middle, tight], each a number and a noun or a word alone.
function wordsOf(item, stale) {
  const words = [];
  const counted = (n, full, mid, tight) => words.push([[n, ` ${full}${n === 1 ? '' : 's'}`], [n, ` ${mid}${mid === 'msg' || n === 1 ? '' : 's'}`], [n, tight]]);
  const plain = (full, mid, tight, fg) => words.push([['', full, fg || (LAYOUT.words.bright.includes(full) ? 'bright' : 'noun')], ['', mid, fg || (LAYOUT.words.bright.includes(full) ? 'bright' : 'noun')], ['', tight, fg || (LAYOUT.words.bright.includes(full) ? 'bright' : 'noun')]]);
  if (stale) plain('stale', 'stale', 'old');
  if (item.sendsIn) plain(`sends in ${item.sendsIn}`, `in ${item.sendsIn}`, item.sendsIn, 'fg');
  if (item.msg.origin === 'agent') plain('agent draft', 'agent', 'agt', 'dim');
  if (item.folder && item.folder !== 'Inbox') plain(item.folder, item.folder, item.folder.slice(0, 3));
  if (item.invite) plain('invite', 'invite', 'inv');
  if (item.fold === 'folded') counted(item.size, 'message', 'msg', 'm');
  if (item.files) counted(item.files, 'file', 'file', 'f');
  if (item.answered) plain('replied', 'replied', 're');
  return words;
}

function wordRuns(item, width, stale, tone, accounts = ACCOUNT_NAMES) {
  const form = width >= LAYOUT.wordForms.full ? 0 : width >= LAYOUT.wordForms.mid ? 1 : 2;
  const between = form === 2 ? ' ' : ` ${G.dot} `;
  const chosen = WORDS_COLOUR(accounts.length);
  const colour = chosen ? { bright: chosen, noun: chosen } : { bright: 'fg', noun: 'dim' };
  const out = [];
  for (const forms of wordsOf(item, stale)) {
    const [number, noun, fg] = forms[form];
    const next = [...(out.length ? [run(between, tone(colour.noun))] : []), run(number, tone(colour.bright)), run(noun, tone(colour[fg || 'noun'] || fg))];
    if (cells(out) + cells(next) > width) break;
    out.push(...next);
  }
  return [gap(width - cells(out)), ...out];
}

// A row needs you when it is bright. Unread is bold, flagged is plain, and
// everything read is dim. The cursor is the accent where the focus is and
// dim where it is not.
function listRow(item, width, o = {}) {
  const c = columns(width, o.narrowed, o.accounts);
  const R = LAYOUT.row;
  const tone = (fg) => (o.faint ? 'faint' : fg);
  const text = tone(item.unread || item.flagged ? 'fg' : 'dim');
  const needle = o.faint ? null : o.needle;
  const account = item.msg.account;

  const row = [o.selected ? run(G.cursor, o.unfocused ? 'dim' : 'acc') : gap(1), gap(1), o.marked ? run(G.mark, tone('fg')) : gap(1), gap(1)];
  if (c.account) row.push(...fit([run(accountShort(account, c.letters, o.accounts), tone(accountColour(account, o.accounts)))], c.account, false));
  row.push(...fit(highlight(item.from || name(item.msg.from), needle, text, { b: item.unread && !o.faint }), c.from), gap(R.fromGap));
  row.push(item.fold ? run(item.fold === 'folded' ? G.folded : G.unfolded, tone('dim')) : gap(1), gap(1));

  const subject = [gap(2 * item.depth), ...highlight(item.subject, needle, text)];
  if (item.excerpt) subject.push(gap(2), ...highlight(item.excerpt, needle, tone('dim')));
  if (item.reason) subject.push(gap(2), run(item.reason, tone('err')));
  const words = c.words ? wordRuns(item, c.words, o.stale, tone, o.accounts) : [];
  if (words.some((r) => r.t.trim())) row.push(...fit(subject, c.subject), gap(R.wordsGap), ...words);
  else row.push(...fit(subject, c.subject + (c.words ? R.wordsGap + c.words : 0)));
  if (c.date) row.push(gap(R.dateGap), run(shortDate(item.date, item.group).padStart(c.date), tone('dim')));
  row.push(gap(1), item.flagged ? run(G.flag, tone('warn')) : gap(1), gap(LAYOUT.margin));
  return o.selected ? fill(row, 'sel') : row;
}

// Rows under their date groups, with a blank row between groups.
function grouped(rows, draw) {
  const lines = [];
  let group = null;
  rows.forEach((item, i) => {
    if (item.group !== group) {
      if (group !== null) lines.push([]);
      lines.push(heading([run(item.group, 'dim', { b: true })]));
      group = item.group;
    }
    lines.push(draw(item, i));
  });
  return lines;
}

// ------------------------------------------------------------------- chrome

// `search` is the query of a search being shown: it takes the place of the
// active tab, after the others.
function tabsRow(width, active, counts = COUNTS, search) {
  const tabs = [];
  for (const section of SECTIONS) {
    if (section.transient && counts[section.name] == null) continue;
    const on = section.name === active && search == null;
    tabs.push(run(section.name, on ? 'acc' : 'dim', { b: on }));
    const tone = { Flagged: 'warn', Outbox: 'err' }[section.name] || 'fg';
    if (counts[section.name]) tabs.push(gap(1), run(counts[section.name], tone, { b: tone === 'fg' }));
    tabs.push(gap(3));
  }
  if (search != null) tabs.push(run('Search', 'acc', { b: true }), gap(1), run(search));
  return fit(heading(tabs), width);
}

// Healthy accounts and an empty outbox say nothing.
function alerts(sync = SYNC) {
  const out = [];
  const { outbox } = sync;
  if (outbox && outbox.held) out.push(SAY.sending(outbox.sendsIn));
  if (outbox && outbox.failed) out.push(SAY.notSent(outbox.failed));
  for (const account of sync.accounts) {
    if (account.state === 'error') out.push(SAY.syncError(account.name, account.reason, account.retry, account.last));
    if (account.state === 'syncing') out.push(SAY.firstSync(account.name, account.done, account.total));
  }
  return out.join(`{dim| ${G.dot} }`);
}

function statusRow(width, left = '', right = alerts()) {
  const tail = m(right);
  const head = heading(m(left));
  const room = width - cells(head) - 2 - LAYOUT.margin;
  return leftRight(head, [gap(2), ...fit(tail, Math.min(cells(tail), room)), gap(LAYOUT.margin)], width);
}

// ------------------------------------------------------------------- bodies

const references = (line, fg) => line.split(/(\[\d+\])/).filter(Boolean).map((part) => run(part, /^\[\d+\]$/.test(part) ? 'link' : fg));

// An inline image is cells, not words: narrower than its picture, the
// placeholder is as wide as the measure.
function imageRows(file, measure) {
  const [w, h] = [Math.min(file.cells[0], measure), file.cells[1]];
  const label = ` ${file.name}  ${file.pixels} `.slice(0, w);
  const rows = Array.from({ length: h }, () => [run(G.shade.repeat(w), 'faint')]);
  const middle = Math.floor(h / 2);
  rows[middle] = overlay(rows[middle], Math.floor((w - label.length) / 2), [run(label, 'dim')]);
  return rows;
}

// A lead on the first row and its width in spaces on the rest: a hanging
// indent.
const hanging = (lead, text, measure, fg) => wrap(text, measure - lead.length).map((line, i) => [run(i ? ' '.repeat(lead.length) : lead, 'dim'), ...references(line, fg)]);

// The reader's text at `measure` cells. No word of a message is ever cut:
// everything wraps. A paragraph, a heading, a signature line and an image's
// alt text wrap to the measure; a quote wraps behind its bar; a list item
// wraps under its own text, clear of the dot. A table keeps its columns
// while a row fits, and when one does not, every row becomes its cells in a
// line, parted by dots, wrapped like a list item.
function bodyRows(msg, measure) {
  const rows = [];
  const wrapped = (text, fg, style) => wrap(text, measure).map((line) => (style ? [run(line, fg, style)] : references(line, fg)));
  for (const block of msg.body) {
    if (typeof block === 'string') rows.push(...wrapped(block, 'fg'));
    else if (block.h) rows.push(...wrapped(block.h, 'fg', { b: true }));
    else if (block.alt) rows.push(...wrap(`[image: ${block.alt}]`, measure).map((line) => [run(line, 'dim')]));
    else if (block.small) rows.push(...wrapped(block.small, 'dim'));
    else if (block.lines) rows.push(...block.lines.flatMap((line) => (line.trim() ? wrap(line, measure) : [line]).map((part) => [run(part)])));
    else if (block.rule) continue;
    else if (block.quote) rows.push(...wrap(block.quote, measure - 2).map((line) => [run(G.bar + ' ', 'dim'), run(line, 'dim')]));
    else if (block.list) rows.push(...block.list.flatMap((item) => hanging(`${G.dot} `, item, measure, 'fg')));
    else if (block.image != null) rows.push(...imageRows(msg.attachments[block.image], measure));
    else if (block.table) {
      const widths = block.table[0].map((_, col) => Math.max(...block.table.map((line) => line[col].length)));
      const fits = widths.reduce((n, w) => n + w + 3, -3) <= measure;
      block.table.forEach((line, i) => rows.push(...(fits
        ? [line.map((cell, col) => run(cell.padEnd(widths[col] + 3), i ? 'fg' : 'dim'))]
        : hanging('', line.join(` ${G.dot} `), measure, i ? 'fg' : 'dim').map((row, n) => (n ? [gap(2), ...row] : row)))));
    }
    rows.push([]);
  }
  rows.pop();
  return rows;
}

function noticeRows(msg) {
  const rows = [];
  if (msg.origin === 'agent') rows.push(m(SAY.agentDraft('23 Nov 13:52')));
  if (msg.remote) rows.push(m(SAY.remote(msg.remote)));
  if (msg.security && msg.security.state === 'good') rows.push(m(SAY.signedGood(msg.security.signer, msg.security.key)));
  if (msg.security && msg.security.state === 'bad') rows.push(m(SAY.signedBad(msg.security.signer)));
  if (msg.security && msg.security.state === 'unknown') rows.push(m(SAY.signedUnknown(msg.security.key)));
  if (msg.security && msg.security.state === 'encrypted') rows.push(m(SAY.encrypted(msg.security.key)));
  if (msg.differs) rows.push(m(SAY.differs(msg.differs.was)));
  return rows;
}

const person = (who) => (PEOPLE[who][0] ? `${name(who)} {dim|<${address(who)}>}` : address(who));

// Styled text wrapped at spaces to `width`, each word keeping its own style.
// A word is broken only when it alone is wider than the row.
function wrapRuns(row, width) {
  const words = [[]];
  for (const cell of explode(row)) {
    if (cell.t === ' ') words.push([cell], []);
    else words[words.length - 1].push(cell);
  }
  const lines = [[]];
  let space = null;
  for (const word of words) {
    if (word.length === 1 && word[0].t === ' ') { space = word[0]; continue; }
    if (!word.length) continue;
    let line = lines[lines.length - 1];
    if (line.length && line.length + 1 + word.length > width) { line = []; lines.push(line); }
    else if (line.length) line.push(space);
    for (let rest = word; rest.length;) {
      const room = width - line.length;
      line.push(...rest.slice(0, room));
      rest = rest.slice(room);
      if (rest.length) { line = []; lines.push(line); }
    }
  }
  return lines;
}

// Whole items (a recipient, a file) packed into rows of `width`, a comma
// between them; an item moves to the next row and is never split unless it
// alone is too wide. Past `most` rows the last row ends `and N more`.
function packed(items, width, most) {
  const comma = run(',', 'dim');
  const lines = [];
  for (const item of items) {
    const last = lines[lines.length - 1];
    const used = last ? last.reduce((n, x) => n + cells(x) + 2, -2) : 0;
    if (last && used + 2 + cells(item) < width) last.push(item);
    else lines.push([item]);
  }
  const shownIn = (rows) => rows.reduce((n, line) => n + line.length, 0);
  const kept = lines.slice(0, most);
  let tail = [];
  if (lines.length > most) {
    const last = kept[kept.length - 1];
    const more = () => run(` and ${items.length - shownIn(kept)} more`, 'dim');
    while (last.length > 1 && last.reduce((n, x) => n + cells(x) + 2, -2) + 1 + cells([more()]) > width) last.pop();
    tail = [more()];
  }
  return kept.flatMap((line, y) => {
    const joined = line.flatMap((item, i) => (i ? [comma, run(' ', 'dim'), ...item] : item));
    const ends = y < kept.length - 1 || tail.length ? [comma] : [];
    return wrapRuns([...joined, ...ends, ...(y === kept.length - 1 ? tail : [])], width);
  });
}

// A notice wrapped under its own text, clear of its mark. Each word keeps
// its colour, so a headline that wraps is still the colour of its mark.
function noticeLines(row, width) {
  const all = explode(row);
  return wrapRuns(all.slice(2), width - 2).map((line, i) => [...(i ? [gap(2)] : all.slice(0, 2)), ...line]);
}

// From, To, Cc, Date and Files, then the notices after a blank row, all
// within `width` and none of it cut. `brief` leaves out what the list row
// already says and keeps the lists short.
function headerBlock(msg, brief, width) {
  const label = LAYOUT.header.label;
  const most = LAYOUT.header.rows[brief ? 'preview' : 'reader'];
  const lines = (name_, value) => value.map((line, i) => [run((i ? '' : name_).padEnd(label), 'dim'), ...line]);
  const list = (who) => packed(who.map((x) => m(person(x))), width - label, most);
  const rows = [...lines('From', wrapRuns(m(`{fg.b|${name(msg.from)}} {dim|<${address(msg.from)}>}`), width - label)), ...lines('To', list(msg.to))];
  if (msg.cc) rows.push(...lines('Cc', list(msg.cc)));
  if (!brief) rows.push(field('Date', [run(longDate(msg.date))]));
  if (msg.attachments) rows.push(...lines('Files', packed(msg.attachments.map((a) => m(`${a.name} {dim|${a.size}}`)), width - label, most)));
  const notices = noticeRows(msg).flatMap((row) => noticeLines(row, width));
  return notices.length ? [...rows, [], ...notices] : rows;
}

// --------------------------------------------------------------------- list

// The newest message of the selected row, as the reader would open it: the
// subject, who and to whom, anything to read first, then the body. The
// subject is a heading, so it also says where the preview begins; focused,
// it carries the cursor and the whole message scrolls under it from `top`.
// A body not yet on this machine says so in its place.
function previewPane(item, width, height, o = {}) {
  const msg = item.msg;
  const measure = Math.min(LAYOUT.measure, width - LAYOUT.indent - LAYOUT.margin);
  const title = [o.focused ? run(G.cursor, 'acc') : gap(1), gap(LAYOUT.margin - 1), run(msg.subject, 'fg', { b: true })];
  const text = o.arriving ? [m(SAY.fetching)] : msg.body ? bodyRows(msg, measure) : [[run(msg.snippet || '')]];
  const body = indent([...headerBlock(msg, true, width - LAYOUT.indent - LAYOUT.margin), [], ...text]);
  const room = height - 2;
  const top = Math.min(o.top || 0, Math.max(0, body.length - room));
  const rows = [title, [], ...body.slice(top, top + room)].map((row) => fit(row, width - LAYOUT.margin));
  while (rows.length < height) rows.push([]);
  return { rows, lines: `{dim|lines ${top + 1}-${Math.min(body.length, top + room)} of ${body.length}}` };
}

function listHint(item, o) {
  if (o.waiting) return hint('global.back');
  if (o.marks && o.marks.length) return hint('list.archive', 'list.trash', 'list.move-to', ['Esc', 'clear marks'], 'global.help');
  if (!item) return o.search != null ? hint('global.back') : hint('list.compose', 'global.help');
  if (item.state === 'failed') return hint(['Enter', 'edit and resend'], 'list.trash', 'global.help');
  if (item.state === 'held') return hint(['u', 'back to Drafts'], 'global.help');
  if (o.search != null) return hint('list.open', 'list.archive', 'list.reply', 'global.back', 'global.help');
  if (item.msg.origin) return hint(['Enter', 'edit'], 'list.trash', 'global.help');
  if (item.fold === 'folded') return hint('list.open', 'list.unfold', 'list.archive', 'list.reply', 'global.help');
  if (item.fold === 'open' || item.reply) return hint('list.open', 'list.fold', 'list.archive', 'list.reply', 'global.help');
  return hint('list.open', 'list.archive', 'list.trash', 'list.reply', 'global.help');
}

// The preview's width beside the list.
const previewWidth = (width) => Math.max(LAYOUT.preview.least, Math.min(LAYOUT.preview.most, Math.floor(width * LAYOUT.preview.share)));

// Where the preview sits for this size, unless `preview` says otherwise.
function previewPlace(width, height, preview) {
  if (preview === false || preview === 'off' || height < LAYOUT.preview.minHeight) return 'off';
  if (preview === 'right' || preview === 'below') return preview;
  return width >= LAYOUT.preview.besideFrom ? 'right' : 'below';
}

// o: { width, height, section, rows, selected, marks, needle, matches,
//      preview, focus, top, arriving, stale, narrowed, accounts, search,
//      waiting, counts, empty, groups, panel, left, right, command }
function listScreen(o) {
  const { width, height } = o;
  const selected = o.rows[o.selected];
  const place = selected ? previewPlace(width, height, o.preview) : 'off';
  const focused = o.focus === 'preview';
  const paneWidth = previewWidth(width);
  const listWidth = place === 'right' ? width - paneWidth - 1 : width;
  const draw = (item, i) => listRow(item, listWidth, {
    selected: i === o.selected,
    unfocused: focused,
    marked: (o.marks || []).includes(item.msg.id),
    needle: o.needle,
    faint: o.needle != null && o.matches != null && !o.matches.includes(item.msg.id),
    stale: o.stale === item.msg.account,
    narrowed: o.narrowed,
    accounts: o.accounts,
  });
  let list = o.groups === false ? o.rows.map(draw) : grouped(o.rows, draw);
  if (o.empty) list = [[], ...o.empty.map((line, i) => [gap(LAYOUT.indent), run(line, i ? 'dim' : 'fg')])];

  const tail = settle(o.panel || [], width);
  const room = height - 2 - 3 - tail.length;
  // As many list lines as fit, never ending on a group label with no row
  // under it.
  const isLabel = (row) => row.length === 2 && row[1].b && row[1].fg === 'dim';
  const trimmed = (rows, n) => {
    const out = rows.slice(0, n);
    while (out.length && (!cells(out[out.length - 1]) || isLabel(out[out.length - 1]))) out.pop();
    while (out.length < n) out.push([]);
    return out;
  };

  let content;
  let pane = { lines: '' };
  if (place === 'below') {
    const paneHeight = LAYOUT.preview.height(height) - tail.length;
    pane = previewPane(selected, width, paneHeight - 1, { focused, top: o.top, arriving: o.arriving });
    content = [...trimmed(list, room - paneHeight), [run(G.rule.repeat(width), 'faint')], ...pane.rows];
  } else if (place === 'right') {
    pane = previewPane(selected, paneWidth, room, { focused, top: o.top, arriving: o.arriving });
    const left = trimmed(list, room);
    content = left.map((row, y) => [...fit(row, listWidth), run(G.bar, 'faint'), ...pane.rows[y]]);
  } else {
    content = trimmed(list, room);
  }

  const back = place === 'right' ? 'Alt-h' : 'Alt-k';
  const command = o.command || (focused ? hint(['j k', 'scroll'], [back, 'list'], 'list.open', 'global.help') : listHint(selected, o));
  const left = focused ? pane.lines : only(o) + (o.left || '');
  return [tabsRow(width, o.section, o.counts, o.search), [], ...content, ...tail, [], statusRow(width, left, o.right), command];
}

// How the status line begins while the list shows one account.
const only = (o) => (o.narrowed ? `{${accountColour(o.narrowed, o.accounts)}|${o.narrowed}} {dim|only ${G.dot}} ` : '');

// A strip is a few list rows, the status line and the command line: enough
// to show one step of a flow.
function strip(o) {
  const width = LAYOUT.default.width;
  const body = o.rows.slice(0, o.count || 5).map((item, i) => listRow(item, width, {
    selected: i === o.selected,
    marked: (o.marks || []).includes(item.msg.id),
    narrowed: o.narrowed,
    accounts: o.accounts,
  }));
  const tail = settle(o.panel || [], width);
  return [...body.slice(0, body.length - Math.max(0, tail.length - 1)), ...tail, [], statusRow(width, only(o) + (o.left || ''), o.right), o.command || listHint(o.rows[o.selected], o)];
}

// ------------------------------------------------------------------- reader

function readerHint(msg) {
  const extra = [];
  if (msg.invite) extra.push('reader.invite');
  if (msg.thread) extra.push('reader.thread');
  if (msg.attachments) extra.push('reader.attachments');
  if (msg.links) extra.push('reader.links');
  if (msg.remote) extra.push('reader.browser');
  return hint('list.reply', 'list.archive', ...extra.slice(0, 3), 'global.back');
}

// o: { width, height, msg, where, between, top, panel, left, right, command }
function readerScreen(o) {
  const { width, height, msg } = o;
  const measure = Math.min(LAYOUT.measure, width - LAYOUT.indent - LAYOUT.margin);
  const title = leftRight(heading([run(msg.subject, 'fg', { b: true })]), [gap(2), run(o.where || '', 'dim'), gap(LAYOUT.margin)], width);
  const head = [title, [], ...indent(headerBlock(msg, false, width - LAYOUT.indent - LAYOUT.margin)), []];

  const body = indent([...(o.between || []), ...bodyRows(msg, measure)]);
  const room = height - head.length - (o.panel ? o.panel.length : 0) - 3;
  const top = o.top || 0;
  const last = Math.min(body.length, top + room);
  const scroll = body.length <= room ? '' : `{dim|lines ${top + 1}-${last} of ${body.length}}`;
  return frame(width, height, head, body.slice(top, last), o.panel,
    statusRow(width, o.left != null ? o.left : scroll, o.right), o.command || readerHint(msg));
}

// What a yes to the B question would load: a message's remote images and
// hosts, less the trackers'.
const loadable = (remote) => ({ images: remote.images - remote.trackerImages, hosts: remote.hosts.filter((host) => !remote.trackerHosts.includes(host)) });

// The end of the reader, enough for one step of a flow.
function readerStrip(msg, o) {
  const width = LAYOUT.default.width;
  const notices = noticeRows(msg).flatMap((row) => noticeLines(row, width - LAYOUT.indent - LAYOUT.margin));
  return [...indent(notices), ...settle(o.panel || [], width), [], statusRow(width, o.left || '', o.right), o.command];
}

function inviteRows(msg) {
  const card = msg.invite;
  const rows = [field('What', [run(card.what, 'fg', { b: true })]), field('When', [run(card.when)]), field('Where', references(card.where, 'fg')), field('Who', [run(card.who)])];
  if (card.repeats) rows.push(field('Repeats', [run(card.repeats)]));
  rows.push(field('Calendar', m(`{${accountColour(msg.account)}|${msg.account}}{dim| ${G.dot} the account it came to}`)));
  return [...rows, []];
}

function linkEntries(links) {
  const hostWidth = Math.max(...links.map((l) => l.host.length)) + 3;
  return links.map((link) => {
    const row = [run(String(link.n).padStart(2) + '  ', 'link'), run(link.host.padEnd(hostWidth)), run(link.shown, 'dim')];
    if (link.warning) row.push(gap(3), ...m(`{warn|${G.notice}} ${link.warning}`));
    else if (link.note) row.push(gap(3), run(link.note, 'dim'));
    return row;
  });
}
const linksHint = () => hint('links.number', 'links.choose', 'global.agent', 'links.close');

function fileEntries(msg) {
  const nameWidth = Math.max(...msg.attachments.map((a) => a.name.length)) + 3;
  return msg.attachments.map((file, i) => [
    run(String(i + 1).padStart(2) + '  ', 'dim'), run(file.name.padEnd(nameWidth)), run(file.size.padStart(7) + '   ', 'dim'),
    run(file.type, 'dim'), run(file.inline ? '   shown above' : '', 'dim'),
  ]);
}

// ------------------------------------------------------------------ compose

// The draft's fields as one block; `focused` names the field the cursor is
// in. Recipients and files take a line each so nothing hides in a long row.
function draftFields(draft, o = {}) {
  const rows = [];
  const add = (label, values) => values.forEach((value, i) => rows.push({ label: i ? '' : label, value, focused: !i && label === o.focused }));
  add('From', [o.from]);
  add('To', draft.to.map((who) => o.person(who)));
  add('Cc', (draft.cc || []).map((who) => o.person(who)));
  add('Bcc', [[]]);
  add('Subject', [[run(draft.subject, 'fg', { b: true })]]);
  add('Files', (draft.attachments || []).map((file) => [run(file.name.padEnd(28)), run(file.size.padStart(7), 'dim')]));
  return rows.map(({ label, value, focused }) => [
    focused ? run(G.cursor, 'acc') : gap(1), gap(LAYOUT.indent - 1),
    run(label.padEnd(9), focused ? 'fg' : 'dim', { b: focused }), ...value,
  ]);
}

// The body of a draft. A paragraph `:agent tidy` rewrote carries the agent's
// mark in the gutter on each of its rows; `said` is a line tuimex puts where
// the body will be while the agent works.
function draftBody(body, measure) {
  const rows = [];
  for (const block of body) {
    if (block.said) rows.push(...indent([m(block.said)]));
    else if (block.changed) rows.push(...bodyRows({ body: [block.changed] }, measure).map((row) => [gap(LAYOUT.margin), run(G.agent, 'agent'), gap(1), ...row]));
    else rows.push(...indent(bodyRows({ body: [block] }, measure)));
    rows.push([]);
  }
  rows.pop();
  return rows;
}

// o: { draft, focused, notice, body, left, right, command }; with none of
// them, the agent's saved draft with From focused.
function composeScreen(width, height, o = {}) {
  const draft = o.draft || DRAFTS[0];
  const identities = ACCOUNTS.flatMap((a) => a.identities.map((identity) => identity.replace(/^.*<|>$/g, '')));
  const from = identities.flatMap((identity) => [identity === address(draft.from) ? run(identity, 'fg', { b: true }) : run(identity, 'dim'), gap(3)]);
  const notice = o.draft ? o.notice : SAY.agentDraft('23 Nov 13:52');
  const head = [
    heading([run(draft.subject, 'fg', { b: true })]),
    [],
    ...draftFields(draft, { focused: o.draft ? o.focused : 'From', from, person: (who) => m(person(who)) }),
    [],
    ...(notice ? indent([m(notice), []]) : []),
  ].map((row) => fit(row, width));
  const body = draftBody(o.body || draft.body, Math.min(LAYOUT.measure, width - LAYOUT.indent - LAYOUT.margin));
  return frame(width, height, head, body, null, statusRow(width, o.left != null ? o.left : '{dim|draft saved 13:58}', o.right),
    o.command || hint(['Tab', 'next field'], 'compose.identity', 'compose.editor', 'global.agent', 'compose.summary', 'compose.close'));
}

// The end of compose, enough for one step of a flow: the subject field, the
// body, the status line and the command line.
function composeStrip(o) {
  const width = LAYOUT.default.width;
  const subject = [gap(LAYOUT.indent), ...field('Subject', [run(OWN_REPLY.subject, 'fg', { b: true })])];
  const notice = o.notice ? [...indent([m(o.notice)]), []] : [];
  return [subject, [], ...notice, ...draftBody(o.body, LAYOUT.measure), ...settle(o.panel || [], width), [], statusRow(width, o.left || '', o.right || ''), o.command];
}

function summaryScreen(width, height) {
  const draft = DRAFTS[0];
  const recipient = (who) => [...fit(m(person(who)), 46), ...(who === 'billing' ? m(SAY.outside) : [])];
  const fields = draftFields(draft, { from: m(person(draft.from)), person: recipient }).filter((row) => cells(row) > LAYOUT.indent + 9);
  const head = [
    heading([run(draft.subject, 'fg', { b: true })]),
    [],
    ...fields,
    ...indent([field('', [run('total'.padEnd(28), 'dim'), run(draft.attachedTotal.padStart(7))]), field('Signing', m(`signed${then('key 51F09A2C')}`)), field('', m(SAY.noKey(address('billing')))), [], m(SAY.summaryAgent('23 Nov 13:52'))]),
  ].map((row) => fit(row, width));
  return frame(width, height, head, [], null, statusRow(width),
    said(SAY.summaryAsk(draft.to.length + draft.cc.length, '10s')));
}

// ----------------------------------------------------------------- calendar
//
// One calendar across the accounts, in a place of its own (ADR-054). Three
// views of the same events at the same date, and one rule for moving:
// Enter goes deeper (month, day, event), Esc comes back, and h j k l move
// the cursor in all of them.

const C = LAYOUT.calendar;
const MONTH_NAMES = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const WEEK = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'];
const TODAY = dayNumber(NOW);
const CLOCK = parts(NOW).time;

const civil = (n) => { const d = new Date(n * 86400000); return { year: d.getUTCFullYear(), month: d.getUTCMonth() + 1, dom: d.getUTCDate(), dow: d.getUTCDay() }; };
const minutes = (clock) => { const [h, min] = clock.split(':').map(Number); return h * 60 + min; };
const monday = (n) => n - (civil(n).dow + 6) % 7;
const dayShort = (n) => { const d = civil(n); return `${DAYS[d.dow]} ${d.dom} ${MONTHS[d.month - 1]}`; };
const dayLong = (n) => { const d = civil(n); return `${DAYS[d.dow]} ${d.dom} ${MONTH_NAMES[d.month - 1]}`; };
// A day's label in a list of days: the two nearest are named.
const dayLabel = (n) => `${n === TODAY ? `Today ${G.dot} ` : n === TODAY + 1 ? `Tomorrow ${G.dot} ` : ''}${dayLong(n)}`;

function occursOn(ev, n) {
  if (ev.weekdays) { const dow = civil(n).dow; return dow >= 1 && dow <= 5 && n >= dayNumber(ev.weekdays[0]) && n <= dayNumber(ev.weekdays[1]); }
  const first = dayNumber(ev.day);
  return n >= first && n < first + (ev.days || 1);
}

// What is on a day, as the views draw it: all-day events first, then by
// the clock. o: { narrowed, hidden, changed }, where `hidden` lists the
// calendars switched off as `account/source`, and `changed` replaces
// fields of events by id, for a step of a flow.
function eventsOn(n, o = {}) {
  const sourceOf = (ev) => `${ev.account}/${ev.local ? 'local' : 'own'}`;
  return [...EVENTS, ...(o.added || [])]
    .map((ev) => ({ ...ev, ...((o.changed || {})[ev.id] || {}), n }))
    .filter((ev) => !ev.gone && !(ev.skip || []).includes(n) && occursOn(ev, n) && (o.only == null || `${ev.title} ${ev.where || ''}`.toLowerCase().includes(o.only)) && (!o.narrowed || ev.account === o.narrowed) && !(o.hidden || []).includes(sourceOf(ev)))
    .sort((a, b) => (a.allDay ? '' : a.start).localeCompare(b.allDay ? '' : b.start));
}
const occurrence = (id, n, o) => eventsOn(n, o).find((ev) => ev.id === id);
const ended = (ev) => ev.n < TODAY || (ev.n === TODAY && !ev.allDay && ev.end <= CLOCK);
const isWaiting = (ev) => ev.answer === 'needs';

// The views as tabs, and at the right what the page shows.
function calendarTabs(width, view, title, search) {
  const tabs = ['Month', 'Week', 'Day'].flatMap((tab) => { const on = search == null && tab.toLowerCase() === view; return [run(tab, on ? 'acc' : 'dim', { b: on }), gap(3)]; });
  if (search != null) tabs.push(run('Search', 'acc', { b: true }), gap(1), run(search));
  return leftRight(heading(tabs), [gap(2), run(title, 'fg', { b: true }), gap(LAYOUT.margin)], width);
}

// What the right of the status line says in the calendar: what mail says
// there, then a calendar that could not be read and reminders being off.
function calendarAlerts(o = {}) {
  const out = [alerts()];
  if (o.reminder) out.unshift(SAY.reminder(...o.reminder));
  if (o.unread) out.push(SAY.calendarUnread(...o.unread));
  if (o.remindersOff) out.push(SAY.remindersAreOff);
  return out.filter(Boolean).join(`{dim| ${G.dot} }`);
}

// The left of the status line: the day and how much is on it, or which of
// the day's events is selected.
function dayStatus(n, o = {}) {
  const events = eventsOn(n, o);
  const waiting = events.filter(isWaiting).length;
  const at = o.selected ? events.findIndex((ev) => ev.id === o.selected.id) : -1;
  const count = at >= 0 ? `${at + 1} of ${events.length}` : events.length === 1 ? '1 event' : `${events.length} events`;
  return `{dim|${dayShort(n)} ${G.dot} ${count}}${waiting ? `{dim| ${G.dot} }${SAY.waiting(waiting)}` : ''}`;
}

function calendarHint(ev) {
  if (!ev) return hint('calendar.open', 'calendar.event', 'calendar.today', 'calendar.new', 'calendar.mail', 'global.help');
  const extra = [];
  if (isWaiting(ev)) extra.push('calendar.answer');
  if (ev.link) extra.push('calendar.links');
  return hint('calendar.open', ...extra, 'calendar.edit', 'calendar.delete', 'global.back', 'global.help');
}

// ------------------------------------------------------------ calendar: rows

// One month, seven days wide. A day is bright when something is on it and
// the accent when it is today; its mark says how much (one, two, more) and
// is the warning colour while an invitation on it waits for an answer.
function monthRows(year, month, o) {
  const first = Date.UTC(year, month - 1, 1) / 86400000;
  const length = new Date(Date.UTC(year, month, 0)).getUTCDate();
  const lead = (civil(first).dow + 6) % 7;
  const rows = [
    [gap(1), run(`${MONTH_NAMES[month - 1]} ${year}`, o.current ? 'fg' : 'dim', { b: !!o.current })],
    WEEK.flatMap((day) => [gap(1), run(day, 'dim'), gap(1)]),
  ];
  for (let at = -lead; at < length; at += 7) {
    rows.push(Array.from({ length: 7 }, (_, i) => {
      const dom = at + i + 1;
      if (dom < 1 || dom > length) return [gap(C.cell)];
      const n = first + dom - 1;
      const events = eventsOn(n, o);
      const mark = events.length ? run(events.length === 1 ? G.few : events.length === 2 ? ':' : G.many, events.some(isWaiting) ? 'warn' : 'dim') : gap(1);
      const cell = [n === o.cursor ? run(G.cursor, o.unfocused ? 'dim' : 'acc') : gap(1), run(String(dom).padStart(2), n === TODAY ? 'acc' : events.length ? 'fg' : 'dim', { b: n === TODAY }), mark];
      return n === o.cursor ? fill(cell, 'sel') : cell;
    }).flat());
  }
  return rows.map((row) => fit(row, 7 * C.cell));
}

// The cursor's month and as many of the months after it as fit whole.
function monthsColumn(room, o) {
  const at = civil(o.cursor);
  const rows = [];
  for (let i = 0; i < 12; i++) {
    const month = monthRows(at.year + Math.floor((at.month - 1 + i) / 12), (at.month - 1 + i) % 12 + 1, { ...o, current: i === 0 });
    if (rows.length + month.length + (i ? 1 : 0) > room) break;
    if (i) rows.push([]);
    rows.push(...month);
  }
  return rows;
}

// What is not normal about an event, most useful first.
function eventWords(ev) {
  const words = [];
  if (isWaiting(ev)) words.push(['needs answer', 'warn']);
  if (ev.answer === 'tentative') words.push(['tentative']);
  if (ev.answer === 'declined') words.push(['declined']);
  if (ev.answer === 'proposed') words.push([`proposed ${ev.proposed}`]);
  if (ev.local) words.push(['local']);
  return words;
}

// An event as a row: the account, its hours, the title and where, then the
// words. It needs you when it is bold; what has ended is dim.
function eventRow(ev, width, o = {}) {
  const narrow = width < C.narrow;
  const letters = o.narrowed ? 0 : narrow ? LAYOUT.row.account.short : Math.max(...ACCOUNT_NAMES.map((a) => a.length));
  const wordsWidth = narrow ? C.wordsNarrow : C.words;
  const tone = (fg) => (o.faint ? 'faint' : fg);
  const text = tone(ended(ev) ? 'dim' : 'fg');
  const needle = o.faint ? null : o.needle;

  const row = [o.selected ? run(G.cursor, o.unfocused ? 'dim' : 'acc') : gap(1), gap(LAYOUT.indent - 1)];
  if (letters) row.push(...fit([run(accountShort(ev.account, letters), tone(accountColour(ev.account)))], letters + (narrow ? 2 : 3), false));
  row.push(...fit([run(ev.allDay ? 'all day' : narrow ? ev.start : `${ev.start}-${ev.end}`, tone('dim'))], narrow ? C.timeNarrow : C.time, false), gap(2));

  const colour = WORDS_COLOUR(ACCOUNT_NAMES.length) || 'dim';
  const words = [];
  for (const [word, fg] of eventWords(ev)) {
    const next = [...(words.length ? [run(` ${G.dot} `, tone(colour))] : []), run(word, tone(fg || colour))];
    if (cells(words) + cells(next) > wordsWidth) break;
    words.push(...next);
  }
  const title = [...highlight(ev.title, needle, text, { b: isWaiting(ev) && !o.faint }), ...(ev.where && !ev.link ? [gap(2), ...highlight(ev.where, needle, tone('dim'))] : [])];
  const room = width - cells(row) - LAYOUT.margin;
  if (words.length) row.push(...fit(title, room - 3 - wordsWidth), gap(3 + wordsWidth - cells(words)), ...words);
  else row.push(...fit(title, room));
  row.push(gap(LAYOUT.margin));
  return o.selected ? fill(row, 'sel') : row;
}

// Days from `from` on, each under its label, in exactly `room` rows. A day
// with nothing on it is passed over, except the first, which says so. It
// never ends on a label with no row under it.
function agendaRows(from, room, width, o = {}) {
  const lines = [];
  const labels = new Set();
  for (let n = from; lines.length < room && n < from + 92; n++) {
    const events = eventsOn(n, o);
    if (!events.length && (n !== from || o.only != null)) continue;
    if (lines.length) lines.push([]);
    const label = heading([run(dayLabel(n), 'dim', { b: true })]);
    labels.add(label);
    lines.push(label);
    if (!events.length) lines.push([gap(LAYOUT.indent), run(SAY.freeDay, 'dim')]);
    for (const ev of events) {
      const selected = !!o.selected && o.selected.id === ev.id && o.selected.n === n;
      lines.push(eventRow(ev, width, { ...o, selected, faint: o.needle != null && !`${ev.title} ${ev.where || ''}`.toLowerCase().includes(o.needle) }));
    }
  }
  const out = lines.slice(0, room);
  while (out.length && (!cells(out[out.length - 1]) || labels.has(out[out.length - 1]))) out.pop();
  while (out.length < room) out.push([]);
  return out.map((row) => fit(row, width));
}

// ---------------------------------------------------------- calendar: views

// o: { width, height, cursor, narrowed, hidden, changed, panel, left, right,
//      command }, and for the day and the week, selected: { id, n }.
function monthScreen(o) {
  const { width, height } = o;
  const tail = settle(o.panel || [], width);
  const room = height - 2 - 3 - tail.length;
  const months = monthsColumn(room, o);
  const leftWidth = LAYOUT.indent + 7 * C.cell + LAYOUT.margin;
  const paneWidth = width - leftWidth - 1;
  const pane = o.empty ? [[], ...o.empty.map((line, i) => [gap(LAYOUT.margin), run(line, i ? 'dim' : 'fg')])] : agendaRows(o.cursor, room, paneWidth, o);
  const content = Array.from({ length: room }, (_, y) => [...fit([gap(LAYOUT.indent), ...(months[y] || [])], leftWidth), run(G.bar, 'faint'), ...fit(pane[y] || [], paneWidth)]);
  const at = civil(o.cursor);
  return [calendarTabs(width, 'month', `${MONTH_NAMES[at.month - 1]} ${at.year}`), [], ...content, ...tail, [],
    statusRow(width, only(o) + (o.left != null ? o.left : dayStatus(o.cursor, o)), o.right != null ? o.right : calendarAlerts(o)), o.command || calendarHint(null)];
}

// The day view is the list of mail again: events from the cursor's day on,
// and the selected one below the divider, or beside it where there is room.
// With `only`, it is what a search found, from the first day that has any,
// under a Search tab in place of the active one, as in mail.
function dayScreen(o) {
  const { width, height } = o;
  const ev = o.selected && occurrence(o.selected.id, o.selected.n, o);
  const place = ev ? previewPlace(width, height, o.preview) : 'off';
  const tail = settle(o.panel || [], width);
  const room = height - 2 - 3 - tail.length;
  const paneWidth = previewWidth(width);
  const listWidth = place === 'right' ? width - paneWidth - 1 : width;
  let content;
  if (place === 'below') {
    const paneHeight = LAYOUT.preview.height(height) - tail.length;
    content = [...agendaRows(o.cursor, room - paneHeight, width, o), [run(G.rule.repeat(width), 'faint')], ...eventPane(ev, width, paneHeight - 1)];
  } else if (place === 'right') {
    const pane = eventPane(ev, paneWidth, room);
    content = agendaRows(o.cursor, room, listWidth, o).map((row, y) => [...row, run(G.bar, 'faint'), ...pane[y]]);
  } else {
    content = agendaRows(o.cursor, room, width, o);
  }
  const at = civil(o.cursor);
  return [calendarTabs(width, 'day', o.only != null ? '' : `${dayLong(o.cursor)} ${at.year}`, o.only), [], ...content, ...tail, [],
    statusRow(width, only(o) + (o.left != null ? o.left : dayStatus(o.selected ? o.selected.n : o.cursor, o)), o.right != null ? o.right : calendarAlerts(o)), o.command || calendarHint(ev)];
}

// Days as columns against the clock. An event is a bar in its account's
// colour down the rows it lasts, its title beside the first and its hours
// beside the second. One that begins while another runs is drawn `step`
// cells in, over the other's rows, so both titles stay whole and the
// first one's bar still shows how long it lasts; two that begin together
// halve the column. The hour it is now is in the accent. Nothing is filled
// but the selected event.
function weekScreen(o) {
  const { width, height } = o;
  const count = width >= C.weekFrom ? 7 : 3;
  const first = count === 7 ? monday(o.cursor) : o.cursor - 1;
  const days = Array.from({ length: count }, (_, i) => first + i);
  const colWidth = Math.floor((width - LAYOUT.margin * 2 - C.axis) / count);
  const tail = settle(o.panel || [], width);
  const room = height - 2 - 3 - tail.length;
  const isSelected = (ev) => !!o.selected && o.selected.id === ev.id && o.selected.n === ev.n;

  // Row `y` of an event's block, `w` cells wide.
  const block = (ev, y, w) => {
    const selected = isSelected(ev);
    const lead = selected ? run(G.cursor, 'acc') : run(G.bar, accountColour(ev.account));
    const words = eventWords(ev)[0];
    const text = y === 0 ? [run(ev.title, ended(ev) ? 'dim' : 'fg', { b: isWaiting(ev) })]
      : y === 1 && !ev.allDay ? [run(`${ev.start}-${ev.end}`, 'dim')]
      : y === 2 && words ? [run(words[0], words[1] || 'dim')] : [];
    const row = [lead, ...fit(text, w - 2), gap(1)];
    return selected ? fill(row, 'sel') : row;
  };

  const head = [gap(LAYOUT.margin + C.axis), ...days.flatMap((n) => {
    const d = civil(n);
    return fit([gap(1), run(`${DAYS[d.dow]} ${d.dom}`, n === TODAY ? 'acc' : n === o.cursor ? 'fg' : 'dim', { b: n === TODAY || n === o.cursor })], colWidth);
  })];
  const allDay = days.map((n) => eventsOn(n, o).filter((ev) => ev.allDay));
  const allRows = Array.from({ length: Math.max(...allDay.map((list) => list.length)) }, (_, y) =>
    [gap(LAYOUT.margin), ...fit(y ? [] : [run('all day', 'dim')], C.axis), ...allDay.flatMap((list) => (list[y] ? block(list[y], 0, colWidth) : [gap(colWidth)]))]);

  // Each day's timed events, with the lane an overlap puts them in.
  const timed = days.map((n) => {
    const list = eventsOn(n, o).filter((ev) => !ev.allDay).map((ev) => ({ ...ev, from: Math.floor(minutes(ev.start) / C.slot) * C.slot, to: Math.max(minutes(ev.end), Math.floor(minutes(ev.start) / C.slot) * C.slot + C.slot) }));
    const lanes = [];
    for (const ev of list) {
      let lane = lanes.findIndex((free) => free <= ev.from);
      if (lane < 0) lane = lanes.length;
      lanes[lane] = ev.to;
      ev.lane = lane;
      const meets = list.filter((other) => other !== ev && other.from < ev.to && ev.from < other.to);
      ev.shares = meets.length > 0;
      ev.abreast = meets.some((other) => other.from === ev.from);
    }
    return { list, lanes: Math.max(1, lanes.length) };
  });

  const slots = room - 1 - allRows.length - (allRows.length ? 1 : 0);
  const picked = o.selected && timed.flatMap((day) => day.list).find(isSelected);
  let from = minutes(C.dayFrom);
  if (picked && picked.to > from + slots * C.slot) from = Math.ceil(picked.to / 60) * 60 - slots * C.slot;
  if (picked && picked.from < from) from = Math.floor(picked.from / 60) * 60;
  from = Math.max(0, Math.min(from, 24 * 60 - slots * C.slot));

  const now = days.includes(TODAY) ? Math.floor(minutes(CLOCK) / 60) * 60 : -1;
  const grid = Array.from({ length: slots }, (_, s) => {
    const at = from + s * C.slot;
    const clock = at % 60 ? [] : [run(`${String(at / 60).padStart(2, '0')}:00`, at === now ? 'acc' : 'dim', { b: at === now })];
    const columns = timed.flatMap((day) => {
      let cell = [gap(colWidth)];
      for (const ev of day.list) {
        if (at < ev.from || at >= ev.to) continue;
        const x = !ev.shares ? 0 : ev.abreast ? ev.lane * Math.floor(colWidth / day.lanes) : ev.lane * C.step;
        const w = ev.abreast ? Math.floor(colWidth / day.lanes) : colWidth - x;
        // A blank cell before a block drawn over another parts the two.
        const rows = block(ev, (at - ev.from) / C.slot, w);
        cell = x && !ev.abreast ? overlay(cell, x - 1, [gap(1), ...rows]) : overlay(cell, x, rows);
      }
      return cell;
    });
    return [gap(LAYOUT.margin), ...fit(clock, C.axis), ...columns];
  });

  const a = civil(days[0]);
  const b = civil(days[count - 1]);
  const title = a.month === b.month ? `${a.dom} to ${b.dom} ${MONTH_NAMES[b.month - 1]} ${b.year}` : `${a.dom} ${MONTHS[a.month - 1]} to ${b.dom} ${MONTHS[b.month - 1]} ${b.year}`;
  const ev = o.selected && occurrence(o.selected.id, o.selected.n, o);
  const content = [head, ...allRows, ...(allRows.length ? [[]] : []), ...grid].map((row) => fit(row, width));
  return [calendarTabs(width, 'week', title), [], ...content, ...tail, [],
    statusRow(width, only(o) + (o.left != null ? o.left : dayStatus(o.cursor, o)), o.right != null ? o.right : calendarAlerts(o)), o.command || calendarHint(ev)];
}

// ---------------------------------------------------------- calendar: event

function whenOf(ev) {
  const year = civil(ev.n).year;
  const first = ev.weekdays ? ev.n : dayNumber(ev.day);
  if (!ev.allDay) return `${dayShort(first)} ${year}, ${ev.start} to ${ev.end}`;
  return ev.days > 1 ? `${dayShort(first)} to ${dayShort(first + ev.days - 1)} ${year}, all day` : `${dayShort(first)} ${year}, all day`;
}

function fromNow(ev) {
  const days = ev.n - TODAY;
  return days === 0 ? 'today' : days === 1 ? 'tomorrow' : days === -1 ? 'yesterday' : days > 0 ? `in ${days} days` : `${-days} days ago`;
}

const ANSWERS = { needs: '{warn|not answered}', accepted: 'accepted', tentative: 'tentative', declined: 'declined', proposed: 'proposed another time' };
const SAID = { needs: '{warn|no answer}', accepted: '{dim|yes}', tentative: '{dim|maybe}', declined: '{dim|no}', proposed: '{dim|proposed another time}' };

// Everyone asked, one never split across rows: the organizer, you, then
// the others with what each has said.
function guestsOf(ev) {
  const others = (ev.guests || []).map(([who, said_]) => `${name(who)} {dim|${said_ || 'no answer'}}`);
  return [`${name(ev.organizer)} {dim|organizer}`, ...(ev.answer ? [`you ${SAID[ev.answer]}`] : []), ...others];
}

// The header block of an event, in the reader's form. `brief` is the
// preview's: it leaves out what the row already says.
function eventFields(ev, brief, width) {
  const label = LAYOUT.header.label;
  const lines = (label_, value) => value.map((line, i) => [run((i ? '' : label_).padEnd(label), 'dim'), ...line]);
  const calendar = CALENDARS.find((c) => c.account === ev.account && c.source === 'own');
  const rows = [field('When', m(`${whenOf(ev)}{dim| ${G.dot} ${fromNow(ev)}}`))];
  if (ev.where) rows.push(field('Where', [run(ev.where)]));
  // The host a link really opens comes first, as in the links panel.
  if (ev.link) rows.push(field('Join', m(`${ev.link.host} {dim|${ev.link.url}}`)));
  rows.push(field('Calendar', m(`{${accountColour(ev.account)}|${ev.account}}{dim| ${G.dot} ${ev.local ? SAY.onlyHere : SAY.readFrom(calendar.read)}}`)));
  if (ev.organizer) rows.push(...lines('Who', packed(guestsOf(ev).map(m), width - label, LAYOUT.header.rows[brief ? 'preview' : 'reader'])));
  if (!brief && ev.answer) rows.push(field('Answer', m(ANSWERS[ev.answer])));
  if (!brief && ev.repeats) rows.push(field('Repeats', [run(ev.repeats)]));
  if (!brief && ev.reminder) rows.push(field('Reminder', [run(ev.reminder)]));
  return rows.map((row) => fit(row, width));
}

function eventNotices(ev) {
  const rows = [];
  if (isWaiting(ev)) rows.push(m(SAY.needsAnswer(name(ev.organizer))));
  if (ev.local) rows.push(m(SAY.onlyInTuimex(ev.account)));
  return rows;
}

// The selected event as the day view shows it under the list.
function eventPane(ev, width, height) {
  const inner = width - LAYOUT.indent - LAYOUT.margin;
  const notes = ev.notes ? [[], ...bodyRows({ body: ev.notes }, Math.min(LAYOUT.measure, inner))] : [];
  const rows = [[gap(LAYOUT.margin), run(ev.title, 'fg', { b: true })], [], ...indent([...eventFields(ev, true, inner), ...notes])].slice(0, height).map((row) => fit(row, width));
  while (rows.length < height) rows.push(fit([], width));
  return rows;
}

// What only one provider sends is folded under More, so the fields every
// event has stay where the eye expects them.
function moreRows(ev, open) {
  if (!ev.more) return [];
  if (!open) return [[], [run(`${G.folded} More`, 'dim'), ...m(then(ev.more.map(([label]) => label.toLowerCase()).join(', ')))]];
  return [[], [run(`${G.unfolded} More`, 'dim')], ...ev.more.map(([label, value]) => [gap(2), ...field(label, [run(value)])])];
}

function eventHint(ev) {
  const extra = [];
  if (isWaiting(ev)) extra.push('calendar.answer');
  if (ev.link) extra.push('calendar.links');
  if (ev.mail) extra.push('event.mail');
  return hint(...extra, 'calendar.edit', 'calendar.delete', 'global.back');
}

// o: { width, height, ev, where, more, panel, left, right, command }
function eventScreen(o) {
  const { width, height, ev } = o;
  const inner = width - LAYOUT.indent - LAYOUT.margin;
  const title = leftRight(heading([run(ev.title, 'fg', { b: true })]), [gap(2), run(o.where || '', 'dim'), gap(LAYOUT.margin)], width);
  const head = [title, [], ...indent(eventFields(ev, false, inner)), []];
  const notices = eventNotices(ev).flatMap((row) => noticeLines(row, inner));
  const notes = ev.notes ? bodyRows({ body: ev.notes }, Math.min(LAYOUT.measure, inner)) : [];
  const body = indent([...notices, ...(notices.length && notes.length ? [[]] : []), ...notes, ...moreRows(ev, o.more)]);
  return frame(width, height, head, body, o.panel, statusRow(width, o.left || '', o.right != null ? o.right : calendarAlerts(o)), o.command || eventHint(ev));
}

// The form for a new event or a change to one, built as compose is: a field
// to a row, the focused one with the cursor and a bold label, and the notes
// below as the body. o: { ev, focused, typing, left, command }
function eventFormScreen(width, height, o) {
  const { ev } = o;
  const calendars = CALENDARS.filter((c) => c.source === 'local').flatMap((c) => [run(`${c.account} local`, c.account === ev.account ? 'fg' : 'dim', { b: c.account === ev.account }), gap(3)]);
  const fields = [
    ['Title', [run(ev.title, 'fg', { b: true })]],
    ['Calendar', calendars],
    ['Starts', [run(`${dayShort(ev.n)} ${civil(ev.n).year} ${ev.start}`)]],
    ['Ends', [run(`${dayShort(ev.n)} ${civil(ev.n).year} ${ev.end}`)]],
    ['All day', [run('no', 'dim')]],
    ['Repeats', [run(ev.repeats || 'never', ev.repeats ? 'fg' : 'dim')]],
    ['Where', ev.where ? [run(ev.where)] : []],
    ...(ev.invited || []).map((who, i) => [i ? '' : 'Guests', m(person(who))]),
    ...(ev.invited ? [] : [['Guests', []]]),
    ['Reminder', [run(ev.reminder || '10 minutes before')]],
  ].map(([label, value]) => {
    const on = !!label && label === o.focused;
    return [on ? run(G.cursor, 'acc') : gap(1), gap(LAYOUT.indent - 1), run(label.padEnd(LAYOUT.header.label), on ? 'fg' : 'dim', { b: on }), ...value, ...(on && o.typing ? [caret] : [])];
  });
  const head = [heading([run(o.heading, 'fg', { b: true })]), [], ...fields, []].map((row) => fit(row, width));
  const body = indent(ev.notes ? bodyRows({ body: ev.notes }, Math.min(LAYOUT.measure, width - LAYOUT.indent - LAYOUT.margin)) : [[run('No notes.', 'dim')]]);
  return frame(width, height, head, body, null, statusRow(width, o.left != null ? o.left : '{dim|not saved}', calendarAlerts()),
    o.command || hint(['Tab', 'next field'], 'event-form.choice', 'event-form.notes', 'event-form.save', 'event-form.close'));
}

// The calendars, each one a row to show or hide.
function calendarEntries(hidden = []) {
  return CALENDARS.map((c) => {
    const off = hidden.includes(`${c.account}/${c.source}`);
    const count = EVENTS.filter((ev) => ev.account === c.account && !!ev.local === (c.source === 'local')).length;
    return [run(c.account.padEnd(11), off ? 'faint' : accountColour(c.account)), run((c.source === 'local' ? 'local' : 'its own').padEnd(10), off ? 'faint' : 'fg'),
      run(`${count} event${count === 1 ? '' : 's'}`.padEnd(12), off ? 'faint' : 'dim'), run(c.source === 'local' ? 'only in tuimex' : `read ${c.read} ago`, off ? 'faint' : 'dim'), ...(off ? [gap(3), run('hidden', 'dim')] : [])];
  });
}

// An event's link as the links panel lists it: the host it really opens,
// and the warning when its text names another (ADR-018).
const eventLinks = (ev) => linkEntries([{ n: 1, host: ev.link.host, shown: ev.link.shown || 'the meeting', url: ev.link.url, warning: ev.link.shown && SAY.link.textSays(ev.link.shown) }]);

// A few rows of a day, the status line and the command line: one step of a
// flow in the calendar.
function calendarStrip(n, o = {}) {
  const width = LAYOUT.default.width;
  const rows = eventsOn(n, o).map((ev) => eventRow(ev, width, { ...o, selected: !!o.selected && o.selected === ev.id }));
  const tail = settle(o.panel || [], width);
  const picked = o.selected ? { id: o.selected, n } : null;
  return [heading([run(dayLabel(n), 'dim', { b: true })]), ...rows, ...tail, [],
    statusRow(width, o.left != null ? o.left : dayStatus(n, { ...o, selected: picked }), o.right != null ? o.right : calendarAlerts(o)), o.command || calendarHint(picked && occurrence(o.selected, n, o))];
}

// --------------------------------------------------------------------- help

// One page that scrolls from `top`. With nothing typed it is the keys in
// three columns and then every command. Typing filters it to what matches,
// in one column: a section whose title matches is shown whole, and a section
// with nothing to show is left out.
function helpScreen(width, height, needle, top = 0, place = 'mail') {
  const has = (...texts) => texts.some((text) => text.toLowerCase().includes(needle));
  const allKeys = KEYMAP.modes.flatMap((x) => x.keys);
  const commandText = (c) => `:${c.name} ${c.args}`;
  const title = (text) => [run(text, 'fg', { b: true })];
  let body;
  let shown = allKeys.length + COMMANDS.length;

  if (!needle) {
    const column = (modes) => {
      const sections = modes.map((mode) => KEYMAP.modes.find((x) => x.mode === mode));
      const keyWidth = Math.max(...sections.flatMap((x) => x.keys.map((k) => k.keys.length))) + 2;
      return sections.flatMap((x) => [title(x.title), ...x.keys.map((k) => [run(k.keys.padEnd(keyWidth)), run(k.does, 'dim')]), []]);
    };
    // The keys of the place help was asked from; filtering reaches them all.
    const columnsOf = (place === 'calendar' ? [
      ['global', 'command', 'confirm', 'help'],
      ['calendar'],
      ['event', 'invite', 'calendars', 'event-form', 'links'],
    ] : [
      ['global', 'command', 'confirm', 'compose', 'help'],
      ['list'],
      ['reader', 'preview', 'links', 'attachments', 'agent', 'invite'],
    ]).map(column);
    const widths = columnsOf.map((col) => Math.max(...col.map(cells)) + 2);
    const depth = Math.max(...columnsOf.map((col) => col.length));
    const keyRows = Array.from({ length: depth }, (_, y) => columnsOf.flatMap((col, i) => fit(col[y] || [], widths[i])));
    const nameWidth = Math.max(...COMMANDS.map((c) => commandText(c).length)) + 3;
    body = [...keyRows, title('Commands'), ...COMMANDS.map((c) => [run(commandText(c).padEnd(nameWidth)), run(c.does, 'dim')])];
  } else {
    const sections = [
      ...KEYMAP.modes.map((x) => ({ title: x.title, rows: x.keys.map((k) => [k.keys, k.does]) })),
      { title: 'Commands', rows: COMMANDS.map((c) => [commandText(c), c.does]) },
    ].map((x) => ({ ...x, rows: has(x.title) ? x.rows : x.rows.filter((row) => has(...row)) })).filter((x) => x.rows.length);
    const leftWidth = Math.max(0, ...sections.flatMap((x) => x.rows.map((row) => row[0].length))) + 3;
    body = sections.flatMap((x) => [highlight(x.title, needle, 'fg', { b: true }), ...x.rows.map(([left, does]) => [...fit(highlight(left, needle, 'fg'), leftWidth), ...highlight(does, needle, 'dim')]), []]);
    shown = sections.reduce((n, x) => n + x.rows.length, 0);
  }

  const lines = indent(body);
  const room = height - 2 - 3;
  const from = Math.max(0, Math.min(top, lines.length - room));
  const last = Math.min(lines.length, from + room);
  const scroll = lines.length > room ? ` ${G.dot} lines ${from + 1}-${last} of ${lines.length}` : '';
  const left = needle ? `{dim|${shown} of ${allKeys.length + COMMANDS.length} shown${scroll}}` : `{dim|${shown} keys and commands${scroll}}`;
  const command = needle ? typed('/' + needle) : hint(['type', 'to filter'], 'help.scroll', 'global.back');
  return [heading(title('Help')), [], ...lines.slice(from, last), ...Array.from({ length: room - (last - from) }, () => []), [], statusRow(width, left), command];
}

// -------------------------------------------------------------------- build

function finish(rows, width, glyphs, label) {
  return rows.map((row, y) => {
    const n = cells(row);
    if (n > width) throw new Error(`${label}: row ${y} is ${n} cells, wider than ${width}`);
    const out = [];
    for (const r of n < width ? [...row, gap(width - n)] : row) {
      if (!r.t) continue;
      const t = glyphs === 'ascii' ? [...r.t].map((ch) => TO_ASCII[ch] ?? ch).join('') : r.t;
      const last = out[out.length - 1];
      if (last && last.fg === r.fg && last.bg === r.bg && last.b === r.b && last.u === r.u) last.t += t;
      else out.push({ t, fg: r.fg, bg: r.bg, b: r.b, u: r.u });
    }
    return out;
  });
}

function build(mode = {}) {
  const glyphs = mode.glyphs === 'ascii' ? 'ascii' : 'unicode';
  const loaded = mode.palette && { ...PALETTE.truecolor, ...mode.palette };
  ACCOUNT_ORDER = loaded ? accountOrder(loaded) : LAYOUT.accounts.builtin;
  // A screen drawn with more accounts than `accounts` says is coloured for
  // the accounts it shows.
  WORDS_COLOUR = (shown) => (loaded ? wordsColour(loaded, ACCOUNT_ORDER.slice(0, Math.max(shown, mode.accounts || 0))) : LAYOUT.words.builtin);
  const { width: W, height: H } = LAYOUT.default;
  const small = LAYOUT.small;
  const wide = LAYOUT.wide;
  const out = {};

  const screen = (id, title, shows, rows, size = LAYOUT.default, tier = { glyphs, colors: 'truecolor' }) => {
    if (rows.length !== size.height) throw new Error(`${id}: ${rows.length} rows, expected ${size.height}`);
    out[id] = { kind: 'screen', title, shows, width: size.width, height: size.height, ...tier, rows: finish(rows, size.width, tier.glyphs, id) };
  };
  // A screen as a terminal with no Unicode and sixteen colours draws it.
  const alsoSmall = (id, title, rows) => screen(`${id}-80-ascii`, `${title}, 80x24, ASCII, 16 colours`, 'the same screen where nothing but ASCII and sixteen colours can be assumed', rows, small, { glyphs: 'ascii', colors: '16' });
  const flow = (id, title, steps) => {
    out[id] = { kind: 'flow', title, steps: steps.map((step) => ({ kind: 'strip', key: step.key, caption: step.caption, note: step.note, width: W, height: step.rows.length, glyphs, colors: 'truecolor', rows: finish(step.rows, W, glyphs, `${id}/${step.caption}`) })) };
  };

  // ---- list
  const haldenAt = indexOf(INBOX, 'halden-3');
  const marks = ['backup', 'parcel'];
  const at = (rows, i, marked) => position(i + 1, rows.length, target(rows[i], marked));
  const list = (size, more) => listScreen({ ...size, section: 'Inbox', rows: INBOX, selected: haldenAt, marks, left: at(INBOX, haldenAt, marks), ...more });
  screen('list', 'List', 'tabs, date groups, the account of each row, a folded thread under the cursor, two marked rows, the preview below its divider, the status line and the hint', list(LAYOUT.default));
  alsoSmall('list', 'List', list(small));
  screen('list-preview-focus', 'List, the preview focused', 'after Alt-j: the cursor moves to the preview\'s subject, the row\'s cursor dims, the status line gives the lines and the hint is the preview\'s', list(LAYOUT.default, { focus: 'preview' }));
  screen('list-wide', 'List, the preview beside it', 'from 160 columns the preview sits to the right of the list, behind a divider, and takes a share of the width so the list keeps room', list(wide), wide);
  screen('list-wider', 'List at 200 columns', 'the same with more room: the preview is near full measure and the list has the rest', list(LAYOUT.wider), LAYOUT.wider);
  screen('list-wide-preview-focus', 'List, the preview beside it and focused', 'after Alt-l: as below, with Alt-h to return', list(wide, { focus: 'preview' }), wide);

  // The whole message is in the preview, not an excerpt: focused, it scrolls.
  const newsAt = indexOf(INBOX, 'newsletter');
  const longAt = indexOf(INBOX, 'migration');
  const reading = (size, more, row = longAt) => listScreen({ ...size, section: 'Inbox', rows: INBOX, selected: row, left: at(INBOX, row), ...more });
  screen('list-preview-scrolled', 'List, the preview scrolled', 'the preview holds the whole message; focused and scrolled part-way, the status line says where it is', reading(LAYOUT.default, { focus: 'preview', top: 14 }));
  screen('list-wide-preview-scrolled', 'List, the preview beside it and scrolled', 'the same beside the list', reading(wide, { focus: 'preview', top: 14 }), wide);
  const inviteAt = indexOf(INBOX, 'invite');
  screen('list-wide-recipients', 'List, a preview with many recipients', 'in the preview a long To stops after two rows and says how many more; the reader shows up to six', listScreen({
    ...wide, section: 'Inbox', rows: INBOX, selected: inviteAt, left: at(INBOX, inviteAt),
  }), wide);
  const statementAt = indexOf(INBOX, 'statement');
  screen('list-preview-arriving', 'List, a body still arriving', 'a message whose body is not on this machine yet shows what is known and says the rest is being fetched', listScreen({
    width: W, height: H, section: 'Inbox', rows: INBOX, selected: statementAt, left: at(INBOX, statementAt), arriving: true,
  }));
  const listAsk = () => { const p = loadable(INBOX[newsAt].msg.remote); return said(SAY.loadImages(p.images, p.hosts, W - LAYOUT.margin * 2 - 'Load 9 images from ?  y yes - n no'.length)); };
  screen('list-browser-ask', 'List, B on a row', 'B from the list is the reader\'s B: the same question before the copy may load pictures; on a thread it is the newest message, the one the preview shows', reading(LAYOUT.default, {
    command: listAsk(),
  }, newsAt));

  const open = items(MAIL, ['cabin']);
  const wenAt = indexOf(open, 'cabin-4');
  screen('list-thread', 'List, a thread unfolded', 'a five-message thread with a branch, each reply under what it answers; the status line says what an action would take', listScreen({
    width: W, height: H, section: 'Inbox', rows: open, selected: wenAt, left: at(open, wenAt),
  }));

  const work = INBOX.filter((item) => item.msg.account === 'work');
  screen('list-account', 'List, one account', 'after gt twice, or :account work: only that account\'s rows, from the first, without the account column; the tabs count it alone and the status line names it', listScreen({
    width: W, height: H, section: 'Inbox', rows: work, selected: 0, narrowed: 'work', counts: { ...countsOf(work), Outbox: COUNTS.Outbox }, left: at(work, 0),
    command: said(SAY.showingOnly('work')),
  }));

  // Three accounts, two of them alike in name, drawn from the same mailbox:
  // the fixture has two, so this state says which rows a third would hold.
  const trio = ['gmail-home', 'gmail-work', 'proton'];
  const toProton = ['payroll', 'vault', 'resolved', 'oncall', 'vendor', 'budget'];
  const moved = (item) => ({ ...item, msg: { ...item.msg, account: toProton.includes(item.msg.id) ? 'proton' : item.msg.account === 'work' ? 'gmail-work' : 'gmail-home' } });
  const three = INBOX.map(moved);
  const bankAt = indexOf(three, 'bank');
  const threeAccounts = (size) => listScreen({ ...size, section: 'Inbox', rows: three, selected: bankAt, accounts: trio, left: at(three, bankAt) });
  screen('list-three-accounts', 'List, three accounts', 'three accounts, each name whole while there is room; two names that begin alike lose what they share when the column is cut', threeAccounts(LAYOUT.default));
  alsoSmall('list-three-accounts', 'List, three accounts', threeAccounts(small));
  const just = (account) => three.filter((item) => item.msg.account === account);
  const cycle = (account, key, caption, note) => {
    const rows = account ? just(account) : three;
    return { key, caption, note, rows: strip({ rows, selected: 0, accounts: trio, narrowed: account, left: at(rows, 0), command: said(account ? SAY.showingOnly(account) : SAY.showingAll) }) };
  };
  flow('account-cycle', 'gt, through three accounts', [
    cycle('gmail-home', 'gt', 'The first', 'From every account, gt shows the first alone, from its first row. The status line names it in its colour and the column goes.'),
    cycle('gmail-work', 'gt', 'The second', 'Each gt is the next account, in the order of the config; gT goes back.'),
    cycle('proton', 'gt', 'The third', 'The same wording for any account.'),
    cycle(null, 'gt', 'All again', 'After the last, every account, and the column returns.'),
  ]);

  const needle = 'nov';
  const matches = ['invite', 'payroll', 'library'];
  const filtering = (size) => listScreen({
    ...size, section: 'Inbox', rows: INBOX, selected: indexOf(INBOX, 'invite'), needle, matches, preview: false,
    left: SAY.finding(matches.length), command: typed('/' + needle),
  });
  screen('list-filter', 'List, finding', 'while a query is typed: rows here that do not match go faint and the match is underlined; Enter searches all mail', filtering(LAYOUT.default));
  alsoSmall('list-filter', 'List, finding', filtering(small));

  const found = [
    { id: 'invite' }, { id: 'payroll' }, { id: 'library' },
    { id: 'vendor', excerpt: 'next review is in November' },
    { folder: 'Sent', msg: { id: 'found-dinner', account: 'personal', from: 'me', to: ['greta'], date: '2026-11-09 19:40', subject: 'Re: Dinner in November?' } },
    { folder: 'Archive', msg: { id: 'found-invoices', account: 'work', from: 'ana', to: ['meWork'], date: '2026-11-03 10:12', subject: 'November invoice run' } },
  ].map(searchItem);
  screen('search', 'Search results', 'after / and Enter: every message that matches, from every folder and account, the body included; a row says its folder when it is not the Inbox; Esc returns', listScreen({
    width: W, height: H, section: 'Inbox', search: needle, needle, rows: found, selected: 3, left: at(found, 3),
  }));
  const mailTotal = FOLDERS.reduce((n, f) => n + f.total, 0).toLocaleString('en-US');
  screen('search-empty', 'Search, nothing found', 'a search with no result says so, and how much it looked through', listScreen({
    width: W, height: H, section: 'Inbox', search: 'zebra', rows: [], selected: -1, empty: SAY.noMatch('zebra', mailTotal, ACCOUNTS.length),
  }));

  screen('search-waiting', 'Search, awaiting its answer', 'between Enter and the results', listScreen({
    width: W, height: H, section: 'Inbox', search: needle, rows: [], selected: -1, waiting: true, empty: [SAY.searching],
  }));

  const drafts = DRAFTS.map((msg) => ({ ...single(msg), from: sentTo(msg) }));
  screen('list-drafts', 'List, the drafts', 'Enter on a saved draft opens it in compose, and the hint says so', listScreen({
    width: W, height: H, section: 'Drafts', rows: drafts, selected: 0, preview: false, left: at(drafts, 0),
  }));

  // Before the first frame there is no list to draw: one line says what is
  // happening, and a second counts when there is something to count.
  const loading = (size) => frame(size.width, size.height, [[], heading(m(SAY.starting)), heading(m(SAY.rebuilding('1,240', mailTotal)))], [], null, [], []);
  screen('loading', 'Starting', 'what the screen says before the first frame', loading(LAYOUT.default));
  alsoSmall('loading', 'Starting', loading(small));

  screen('list-empty', 'List, an empty section', 'an empty section says so, and how fresh that is', listScreen({
    width: W, height: H, section: 'Unread', rows: [], selected: -1, counts: { Flagged: COUNTS.Flagged, Outbox: COUNTS.Outbox }, empty: SAY.empty('1m'),
  }));

  const arrived = items(MAIL.filter((x) => x.account === 'personal' || x.date > '2026-11-22'));
  screen('list-first-sync', 'List, the first sync', 'one account still arriving: the rows so far can be used, and the status line counts', listScreen({
    width: W, height: H, section: 'Inbox', rows: arrived, selected: 0, left: at(arrived, 0), preview: false, counts: countsOf(arrived),
    right: alerts({ accounts: [{ name: 'work', state: 'syncing', done: '1,240', total: '3,812' }] }),
  }));

  screen('list-failing', 'List, an account failing', 'the error on the status line, never the command line; the rows of that account say they are stale', listScreen({
    width: W, height: H, section: 'Inbox', rows: INBOX, selected: 1, stale: 'work', left: at(INBOX, 1),
    right: alerts({ accounts: [{ name: 'work', state: 'error', reason: 'offline', retry: '16s', last: '12:41' }] }),
  }));

  const outbox = OUTBOX.map(outboxItem);
  screen('list-outbox', 'List, the outbox', 'one message held and one that failed, each saying so on its row', listScreen({
    width: W, height: H, section: 'Outbox', rows: outbox, selected: 1, groups: false, preview: false, left: `{dim|2 of 2 ${G.dot}} not sent`,
  }));

  // ---- reader
  const byId = (id) => MAIL.find((x) => x.id === id);
  const plain = (size) => readerScreen({ ...size, msg: byId('halden-1'), where: '1 of 3 in the thread' });
  screen('reader-plain', 'Reader, plain text', 'the subject, an aligned header block, one notice, the body at a readable measure, the scroll position', plain(LAYOUT.default));
  alsoSmall('reader-plain', 'Reader, plain text', plain(small));

  const newsletter = byId('newsletter');
  screen('reader-html', 'Reader, HTML as text', 'numbered link references, a table as rows, a quote, and the remote-content notice naming the tracker', readerScreen({ width: W, height: H, msg: newsletter }));

  screen('reader-attachments', 'Reader, attachments', 'the file list after a, and an inline image at the cell size the picture takes', readerScreen({
    width: W, height: H, msg: byId('photos'), panel: panel('Files', fileEntries(byId('photos')), 0), left: '',
    command: hint('attachments.save', 'attachments.open', 'attachments.close'),
  }));

  screen('reader-invite', 'Reader, invitation', 'what, when, where and who, and the three answers after i', readerScreen({
    width: W, height: H, msg: byId('invite'), between: inviteRows(byId('invite')), command: said(SAY.answer),
  }));

  // B asks every time the copy would load something (ADR-008): the count,
  // the hosts, and the same two keys. A yes never loads a tracker (ADR-009):
  // its images and hosts are left out of the question and of the copy, so
  // the question counts fewer than the notice, which counts all that was not
  // loaded. A message whose only remote images are trackers is not asked
  // about; its copy opens with SAY.copyLoadsNothing.
  const pictures = loadable(newsletter.remote);
  const loadRoom = (width) => width - LAYOUT.margin * 2 - 'Load 9 images from ?  y yes - n no'.length;
  const browserAsk = (size) => readerScreen({ ...size, msg: newsletter, command: said(SAY.loadImages(pictures.images, pictures.hosts, loadRoom(size.width)), size.width) });
  screen('browser-ask', 'Browser copy, the question', 'B on a message with remote pictures asks before the copy may load them, naming every host', browserAsk(LAYOUT.default));
  alsoSmall('browser-ask', 'Browser copy, the question', browserAsk(small));
  flow('browser-copy', 'The browser copy, either answer', [
    { key: 'B', caption: 'Asked', note: 'Asked each time; nothing is remembered per sender.',
      rows: readerStrip(newsletter, { command: said(SAY.loadImages(pictures.images, pictures.hosts, loadRoom(W))) }) },
    { key: 'y', caption: 'Yes', note: 'The copy may load pictures from those hosts over https, and nothing else. The tracker is not among them and does not load.',
      rows: readerStrip(newsletter, { command: said(SAY.openingCopy(SAY.copyLoads(pictures.images, pictures.hosts.length))) }) },
    { key: 'n', caption: 'No', note: 'The copy still opens, sealed: it shows what was attached and loads nothing.',
      rows: readerStrip(newsletter, { command: said(SAY.openingCopy(SAY.copyLoadsNothing)) }) },
  ]);

  // ---- links
  const bank = byId('bank');
  const bankLinks = panel('Links', linkEntries(BANK_LINKS), 0);
  const bankOpens = SAY.opens(BANK_LINKS[0].url);
  const deceptive = SAY.deceptive('bank.example.com', BANK_LINKS[0].host, true);
  screen('links', 'Links', 'after o: each link by number with the host it really opens, and the key that opens the agent\'s menu', readerScreen({
    width: W, height: H, msg: newsletter, panel: panel('Links', linkEntries(NEWSLETTER_LINKS), 0),
    left: SAY.opens(NEWSLETTER_LINKS[0].url), right: '', command: linksHint(),
  }));
  const confirming = (size) => readerScreen({ ...size, msg: bank, panel: bankLinks, left: bankOpens, right: '', command: said(deceptive, size.width) });
  screen('links-confirm', 'Links, a deceptive link', 'a link whose text names another host is confirmed, with both hosts in the question; the agent can be asked first', confirming(LAYOUT.default));
  alsoSmall('links-confirm', 'Links, a deceptive link', confirming(small));

  // ---- compose
  screen('compose', 'Compose', 'the fields, the identity choice on From, the files, the body as $EDITOR left it', composeScreen(W, H));
  screen('send-summary', 'Send summary', 'every recipient on a line, files with sizes, signing, the outside-the-thread mark, the agent-draft line', summaryScreen(W, H));

  // ---- :agent (ADR-031)
  const waiting = hint(['Esc', 'cancel']);
  const threadOut = SAY.replaced('3 people, 4 addresses');
  const yours = ['Hi Priya,', '45 days is ok i think so long as the 1st invoice goes at signing', 'me and Dmitri are the contacts for support', { lines: ['Thanks,', 'Robin'] }];
  const working = (line) => [{ said: line }];
  screen('compose-agent-working', 'Compose, the agent working', 'between :agent reply and the text arriving: one line where the body will be, what left the machine on the status line, and the key that cancels', composeScreen(W, H, {
    draft: OWN_REPLY, body: working(SAY.agentWriting(3)), left: '', right: threadOut, command: waiting,
  }));
  screen('compose-agent-reply', 'Compose, after :agent reply', 'the body the agent wrote, marked as an agent\'s; the status line says what was replaced before the thread left; u takes it back', composeScreen(W, H, {
    draft: OWN_REPLY, notice: SAY.agentDraft('23 Nov 14:21'), body: AGENT_WROTE, left: '', right: threadOut, command: said(SAY.wrote),
  }));
  screen('compose-agent-tidy', 'Compose, after :agent tidy', 'your text with the paragraphs the agent rewrote marked in the gutter; u restores what you wrote', composeScreen(W, H, {
    draft: OWN_REPLY, notice: SAY.agentTidied('23 Nov 14:21'), body: AGENT_TIDIED, left: '', right: SAY.replaced('3 people'), command: said(SAY.tidied(2)),
  }));

  // Once the agent is asked the question keeps its two answers: asking twice
  // is not offered.
  const bankOut = SAY.replaced('1 person, 2 addresses');
  const asked = said(SAY.deceptive('bank.example.com', BANK_LINKS[0].host, false));
  const advised = (...lines) => [...bankLinks, [], ...indent(lines.map((line) => m(line)))];
  const asking = advised(SAY.agentAsking);
  const verdict = advised(SAY.agentAdvice('against opening', 'the host is not the bank\'s, and the message presses for haste'), SAY.adviceOnly);
  const linkScreen = (rows) => readerScreen({ width: W, height: H, msg: bank, panel: rows, left: bankOpens, right: bankOut, command: asked });
  screen('links-agent-asking', 'Links, the agent asked', 'after A at the question, or :agent link 1: the same question, answerable at once, while the agent reads', linkScreen(asking));
  screen('links-agent-verdict', 'Links, the agent\'s advice', 'the advice beside the confirm, as advice: the question is unchanged and the agent opened nothing', linkScreen(verdict));

  flow('agent-reply', ':agent reply', [
    { key: ':agent reply agree to 45 days', caption: 'Asked', note: 'Typed in compose, with or without guidance. Nothing has left yet.',
      rows: composeStrip({ body: [''], command: typed(':agent reply agree to 45 days') }) },
    { key: 'Enter', caption: 'Working', note: 'The thread has left, redacted; the status line says what was replaced. Esc cancels and leaves the body as it was.',
      rows: composeStrip({ body: working(SAY.agentWriting(3)), right: threadOut, command: waiting }) },
    { key: 'text arrives', caption: 'Returned', note: 'The body is the agent\'s and says so. Recipients and files are still only yours.',
      rows: composeStrip({ notice: SAY.agentDraft('23 Nov 14:21'), body: AGENT_WROTE, right: threadOut, command: said(SAY.wrote) }) },
  ]);
  flow('agent-failed', ':agent failing, or not set up', [
    { key: ':agent tidy', caption: 'The command fails', note: 'E, the command, the reason. Your text is as you left it.',
      rows: composeStrip({ body: yours, right: SAY.replaced('3 people'), command: said(SAY.agentFailed('tidy', 'the agent command exited with 1')) }) },
    { key: ':agent tidy', caption: 'No [agent] in the config', note: 'Without an [agent] table the command does not exist, and nothing leaves.',
      rows: composeStrip({ body: yours, command: said(SAY.agentAbsent) }) },
  ]);
  const linkStrip = (rows, right, command, left = bankOpens) => [...settle(rows.slice(1), W), [], statusRow(W, left, right), command];
  const first5 = () => INBOX.slice(0, 5);
  flow('agent-link', 'Asking the agent about a link', [
    { key: '1', caption: 'The question', note: 'Opening a deceptive link asks first, and offers the agent. In the links list A then l asks about the selected link, and :agent link 1 does the same from anywhere in the reader.',
      rows: linkStrip(bankLinks, '', said(deceptive)) },
    { key: 'A', caption: 'Asking', note: 'The question stays and can be answered without waiting.',
      rows: linkStrip(asking, bankOut, asked) },
    { key: 'text arrives', caption: 'Advice', note: 'The agent neither opens nor blocks: y still opens the link, n still does not.',
      rows: linkStrip(verdict, bankOut, asked) },
  ]);
  // A opens a menu of what the agent can do here; the pick is what sends.
  const agentMenu = (...ids) => panel('Agent', ids.map((id) => [run(BINDING[id].keys.padEnd(4)), run(BINDING[id].hint, 'dim')]));
  const picking = hint(['Esc', 'cancel']);
  flow('agent-menu', 'A, the agent\'s menu', [
    { key: 'A', caption: 'On a row', note: 'r opens compose on a reply to the selected message and asks the agent for it, as :agent reply typed here does. Nothing has left yet.',
      rows: strip({ rows: first5(), selected: 1, left: SAY.agentUnsent, command: picking, panel: agentMenu('agent.reply') }) },
    { key: 'A', caption: 'In the reader', note: 'The same one choice, for the message being read.',
      rows: readerStrip(byId('halden-1'), { left: SAY.agentUnsent, command: picking, panel: agentMenu('agent.reply') }) },
    { key: 'A', caption: 'In compose', note: 'Write the reply from the thread, or tidy what is already written.',
      rows: composeStrip({ body: yours, left: SAY.agentUnsent, command: picking, panel: agentMenu('agent.reply', 'agent.tidy') }) },
    { key: 'A', caption: 'In the links list', note: 'About the selected link. The agent reads its address and the message, and opens nothing.',
      rows: linkStrip([...bankLinks, ...agentMenu('agent.link')], '', picking, SAY.agentUnsent) },
    { key: 'A', caption: 'No agent set up', note: 'With no [agent] table the key says what is missing, and nothing leaves.',
      rows: strip({ rows: first5(), selected: 3, left: at(INBOX, 3), command: said(SAY.agentKeyAbsent) }) },
  ]);

  flow('agent-completion', ':agent, completing', [
    { key: ':agent Tab', caption: 'Its functions', note: 'Tab after the name offers what it can do, each with a line.',
      rows: composeStrip({ body: yours, panel: panel(':agent', choices(COMMANDS.find((c) => c.name === 'agent').choices), 0), command: typed(':agent reply') }) },
    { key: ':agent link Tab', caption: 'The links', note: 'After link, the numbers of this message\'s links, each with the host it really opens.',
      rows: readerStrip(bank, { panel: panel(':agent link', BANK_LINKS.map((l) => [run(String(l.n).padEnd(4), 'link'), run(l.host)]), 0).slice(1), command: typed(':agent link 1') }) },
  ]);

  // ---- move, help
  const folders = FOLDERS.filter((f) => f.account === 'work' && f.name !== 'Inbox');
  const folderMenu = panel('Folders in work', folders.map((f) => [run(f.name.padEnd(12)), run(f.total.toLocaleString('en-US').padStart(6), 'dim')]), 0);
  screen('move', 'Move prompt', 'M opens the command line; the folders of that account complete above it', listScreen({
    width: W, height: H, section: 'Inbox', rows: INBOX, selected: haldenAt, left: at(INBOX, haldenAt),
    command: typed(':move Sent'), panel: folderMenu,
  }));

  screen('help', 'Help', 'the keys in three columns, then every command, on one page that scrolls', helpScreen(W, H, ''));
  screen('help-commands', 'Help, the commands', 'the same page scrolled to its end: each : command, what it takes, and one line about it', helpScreen(W, H, '', 999));
  screen('help-filtered', 'Help, filtered', 'typing shows only what matches, under the headings it belongs to', helpScreen(W, H, 'agent'));

  // ---- command line states
  const first = INBOX.slice(0, 5);
  const idle = at(INBOX, 1);
  const completion = panel('Commands', choices(COMMANDS.filter((c) => c.name.startsWith('a'))), 0);
  const log = (time, markup) => [run(time + '  ', 'dim'), ...m(markup)];
  const history = panel('Messages', [
    log('14:19', `{ok|${G.ok}} Archived 1 message`),
    log('14:12', `{err|E :move: no folder "Recipts" in personal}`),
    log('14:02', `{ok|${G.ok}} Sent to 1 recipient`),
    log('13:05', `{err|E send: ${OUTBOX[1].reason}}`),
    log('12:41', `{err|E sync: work offline}${then('retried 4 times')}`),
    log('12:40', `{ok|${G.ok}} Flagged 1 message`),
  ]);
  const afterCabin = items(without(MAIL, INBOX[1].ids));
  flow('command-line', 'Command line states', [
    { key: '', caption: 'Idle', note: 'The last row is always the command line. Idle, it holds the hint for what is selected.',
      rows: strip({ rows: first, selected: 1, left: idle }) },
    { key: ':trash', caption: 'Typing', note: 'Typing replaces the hint. The status line already names what the command will take.',
      rows: strip({ rows: first, selected: 1, left: idle, command: typed(':trash') }) },
    { key: ':a Tab', caption: 'Completion', note: 'Choices take the rows above the status line. Tab and S-Tab cycle and fill the line.',
      rows: strip({ rows: first, selected: 1, left: idle, command: typed(':archive'), panel: completion }) },
    { key: ':empty-trash work', caption: 'Confirm', note: 'The question, then its keys. Any other key cancels and is discarded.',
      rows: strip({ rows: first, selected: 1, left: idle, command: said(SAY.emptyTrash(212, 'work')) }) },
    { key: 'e', caption: 'Success', note: 'Past tense, the undo dim after it. It stays until the next key.',
      rows: strip({ rows: afterCabin, selected: 1, left: at(afterCabin, 1), command: said(SAY.archived('5 messages')) }) },
    { key: ':move Recipts', caption: 'Error', note: 'E, the command, the reason, then the next useful key.',
      rows: strip({ rows: first, selected: 1, left: idle, command: said(SAY.noFolder('Recipts', 'personal')) }) },
    { key: ':arhcive', caption: 'Unknown command', note: 'The nearest command is offered; Tab accepts it.',
      rows: strip({ rows: first, selected: 1, left: idle, command: said(SAY.unknown('arhcive', 'archive')) }) },
    { key: ':messages', caption: ':messages', note: 'Results and errors, newest first, including the ones that only reached the status line.',
      rows: strip({ rows: INBOX, count: 9, selected: 1, left: idle, command: hint(['j k', 'scroll'], 'global.back'), panel: history }) },
  ]);

  // ---- flows
  const afterArchive = items(without(MAIL, ['backup']));
  flow('archive-undo', 'Archive, then undo', [
    { key: '', caption: 'Before', note: 'The cursor is on a single message.',
      rows: strip({ rows: first, selected: 2, left: at(INBOX, 2) }) },
    { key: 'e', caption: 'Archived', note: 'No confirm. The row goes and the cursor takes the row that moved up.',
      rows: strip({ rows: afterArchive, selected: 2, left: at(afterArchive, 2), command: said(SAY.archived('1 message')) }) },
    { key: 'u', caption: 'Undone', note: 'The row returns where it was and the cursor returns to it.',
      rows: strip({ rows: first, selected: 2, left: at(INBOX, 2), command: said(SAY.undone('1 message', 'Inbox')) }) },
  ]);

  // An account with no Archive is asked once, by the move that first needs
  // one (ADR-046); Trash is asked the same way.
  flow('missing-folder', 'An account with no Archive', [
    { key: 'e', caption: 'Asked', note: 'Nothing has changed yet. The same question is asked for Trash.',
      rows: strip({ rows: first, selected: 2, left: at(INBOX, 2), command: said(SAY.createFolder('work', 'Archive')) }) },
    { key: 'y', caption: 'Created, then archived', note: 'The folder is made and the archive that asked for it goes ahead; u reverses the archive.',
      rows: strip({ rows: afterArchive, selected: 2, left: at(afterArchive, 2), command: said(SAY.archived('1 message')) }) },
  ]);

  flow('empty-trash', ':empty-trash, the result', [
    { key: 'y', caption: 'All deleted', note: 'Deleted for good: this is the one result with no undo.',
      rows: strip({ rows: first, selected: 1, left: idle, command: said(SAY.emptied(212, 'work', 0)) }) },
    { key: 'y', caption: 'Some kept', note: 'A message the server has not numbered yet stays, and the result says how many.',
      rows: strip({ rows: first, selected: 1, left: idle, command: said(SAY.emptied(209, 'work', 3)) }) },
  ]);

  const batch = ['backup', 'newsletter', 'bank'];
  const afterTrash = items(without(MAIL, batch));
  flow('trash-batch', 'Trash a marked batch', [
    { key: 'Space x3', caption: 'Marked', note: 'With marks set, every action takes the marked rows, wherever the cursor is.',
      rows: strip({ rows: INBOX, count: 7, selected: 6, marks: batch, left: at(INBOX, 6, batch) }) },
    { key: 'd', caption: 'Trashed', note: 'One key, no confirm, one undo for the whole batch.',
      rows: strip({ rows: afterTrash, count: 7, selected: 3, left: at(afterTrash, 3), command: said(SAY.trashed('3 messages')) }) },
  ]);

  const quiet = alerts({ accounts: [] });
  const answered = items(patched(patched(MAIL, 'halden-3', { unread: false, answered: true }), 'halden-1', { flagged: false }));
  const heldStep = { key: 'y', caption: 'Held', note: 'After the summary is confirmed the message waits in the outbox, and the status line counts down.',
    rows: strip({ rows: INBOX, selected: haldenAt, left: at(INBOX, haldenAt), right: alerts({ outbox: { held: 1, sendsIn: '10s' }, accounts: [] }), command: said(SAY.held(3, '10s')) }) };
  flow('send-sent', 'Send, held, then sent', [
    heldStep,
    { key: '10s later', caption: 'Sent', note: 'The thread goes quiet: read, and marked as replied.',
      rows: strip({ rows: answered, selected: haldenAt, left: at(answered, haldenAt), right: quiet, command: said(SAY.sent(3)) }) },
  ]);
  flow('send-undone', 'Send, held, then undone', [
    heldStep,
    { key: 'u', caption: 'Returned to Drafts', note: 'Within the delay, u takes it back. Nothing left the machine.',
      rows: strip({ rows: INBOX, selected: haldenAt, left: at(INBOX, haldenAt), right: quiet, command: said(SAY.unsent) }) },
  ]);

  const afterMove = items(without(MAIL, INBOX[haldenAt].ids));
  flow('move-refused', 'A move the server refuses', [
    { key: 'M Archive', caption: 'Asked', note: 'The status line names what will move while the folder is typed.',
      rows: strip({ rows: INBOX, selected: haldenAt, left: at(INBOX, haldenAt), command: typed(':move Archive') }) },
    { key: 'Enter', caption: 'Moved here', note: 'The move shows at once; the server has not answered yet.',
      rows: strip({ rows: afterMove, selected: haldenAt, left: at(afterMove, haldenAt), command: said(SAY.moved('3 messages', 'Archive')) }) },
    { key: 'server says no', caption: 'Refused and put back', note: 'The rows return; the error names the command and gives the reason in the server\'s words.',
      rows: strip({ rows: INBOX, selected: haldenAt, left: at(INBOX, haldenAt), command: said(SAY.refused('work', '3 messages', 'NO [OVERQUOTA] quota exceeded')) }) },
  ]);

  // ---- reader notices
  const noticeStrip = (msg) => indent([...headerBlock(msg, false, W - LAYOUT.indent - LAYOUT.margin), [], ...bodyRows(msg, LAYOUT.measure).slice(0, 2)]);
  const unknownKey = { ...byId('budget'), security: { state: 'unknown', key: '9D2E41B7' }, body: ['The numbers are in the sheet. Tell me if the travel line looks low.'] };
  flow('reader-notices', 'Reader notices', [
    { key: '', caption: 'Signed, good', note: 'gpg verified the signature against a key it has.', rows: noticeStrip(byId('halden-1')) },
    { key: '', caption: 'Signed, bad', note: 'The one notice in the error colour.', rows: noticeStrip(byId('vault')) },
    { key: '', caption: 'Signed, unknown key', note: 'Not good and not bad: unchecked, and it says so.', rows: noticeStrip(unknownKey) },
    { key: '', caption: 'Encrypted', note: 'Shown once gpg has opened it.', rows: noticeStrip(byId('payroll')) },
    { key: '', caption: 'Sender differs from the thread', note: 'Same name, another address, in a thread it claims to continue.', rows: noticeStrip(byId('halden-3')) },
    { key: '', caption: 'Agent draft', note: 'The same mark wherever an agent\'s draft appears: the reader, compose, the send summary.', rows: noticeStrip(DRAFTS[0]) },
  ]);

  // ---- calendar (ADR-054). The cursor is on Thursday, which holds one of
  // everything: an invitation not yet answered, two events at once, one
  // that is only in tuimex.
  const thu = dayNumber('2026-11-26');
  const fri = thu + 1;
  const roadmap = { id: 'roadmap', n: thu };
  const cut = (rows, n) => [...rows.slice(0, n), ...rows.slice(-3)];
  const month = (size, more) => monthScreen({ ...size, cursor: thu, ...more });
  screen('cal-month', 'Calendar, month', 'after gc: the cursor\'s month and the two after it, each day marked with how much is on it; beside them the cursor\'s day and the days that follow, each event with its account, its hours and what is not normal about it', month(LAYOUT.default));
  alsoSmall('cal-month', 'Calendar, month', month(small));
  screen('cal-month-account', 'Calendar, one account', 'after gt, or :account personal: only that account\'s events, in the grid\'s marks and in the list; the status line names it', month(LAYOUT.default, {
    narrowed: 'personal', command: said(SAY.showingOnly('personal')),
  }));
  const allHidden = CALENDARS.map((c) => `${c.account}/${c.source}`);
  screen('cal-empty', 'Calendar, none set up', 'with no calendar configured the grid still moves and the list says how to add one', monthScreen({
    width: W, height: H, cursor: TODAY, hidden: allHidden, empty: SAY.noCalendar, left: '', command: hint('calendar.mail', 'global.help'),
  }));
  screen('cal-unread', 'Calendar, one that could not be read', 'a calendar that cannot be fetched says so on the status line, with how old what is shown is; its events stay', month(LAYOUT.default, { unread: ['personal', '2h'] }));
  screen('cal-calendars', 'Calendars', 'after C: every calendar with where it is kept and when it was last read; Space hides one from every view', month(LAYOUT.default, {
    hidden: ['personal/local'], panel: panel('Calendars', calendarEntries(['personal/local']), 1), left: '', command: hint('calendars.choose', 'calendars.toggle', 'calendars.close'),
  }));

  const week = (size, more) => weekScreen({ ...size, cursor: thu, selected: roadmap, ...more });
  screen('cal-week', 'Calendar, week', 'after 2, or Tab: seven days against the clock; an event is a bar in its account\'s colour for as long as it lasts, two at once share a column, and the hour it is now is in the accent', week(LAYOUT.default));
  alsoSmall('cal-week', 'Calendar, week', week(small));

  const day = (size, more) => dayScreen({ ...size, cursor: thu, selected: roadmap, ...more });
  screen('cal-day', 'Calendar, day', 'after Enter on a day, or 3: the list of mail again, with events from that day on and the selected one below the divider', day(LAYOUT.default));
  alsoSmall('cal-day', 'Calendar, day', day(small));
  screen('cal-day-wide', 'Calendar, day, the event beside it', 'from 160 columns the selected event sits to the right of the list', day(wide), wide);
  screen('cal-find', 'Calendar, finding', 'after /: what is typed is underlined where it matches and every other event goes faint', dayScreen({
    width: W, height: H, cursor: TODAY, needle: 'call', left: '', command: typed('/call'),
  }));

  const halden = { id: 'halden-call', n: TODAY };
  screen('cal-search', 'Calendar, a search', 'after / then Enter: every event that matches, on any day, in place of the active view until Esc; n and N step through them', dayScreen({
    width: W, height: H, cursor: dayNumber('2026-11-01'), only: 'call', needle: 'call', selected: halden, left: SAY.matching(EVENTS.filter((ev) => ev.title.toLowerCase().includes('call')).length, 'call'),
    command: hint('calendar.open', 'calendar.match', 'global.back', 'global.help'),
  }));

  const theEvent = occurrence('roadmap', thu);
  const event = (size, more) => eventScreen({ ...size, ev: theEvent, where: `2 of 4 on ${dayShort(thu)}`, ...more });
  screen('cal-event', 'Event', 'after Enter on an event: when, where, the host its link really opens, which calendar holds it, everyone asked and what each has said, then its notes; what only one provider sends is folded under More', event(LAYOUT.default));
  alsoSmall('cal-event', 'Event', event(small));
  screen('cal-event-more', 'Event, More unfolded', 'after zo: the fields only this provider sends', event(LAYOUT.default, { more: true }));
  screen('cal-event-answer', 'Event, the question', 'after i: the five answers, the same question the reader asks of the invitation\'s mail', event(LAYOUT.default, { command: said(SAY.answer) }));
  screen('cal-event-local', 'Event, only in tuimex', 'an event tuimex alone holds says so before anything else', eventScreen({
    width: W, height: H, ev: occurrence('migration', thu), where: `4 of 4 on ${dayShort(thu)}`,
  }));

  const offsite = { id: 'offsite', account: 'work', n: thu, day: '2026-11-26', start: '14:00', end: '15:00', title: 'Offsite planning', where: 'Room Fjord', invited: ['imani', 'lars'], local: true };
  const form = (more) => eventFormScreen(W, H, { ev: offsite, heading: 'New event', focused: 'Title', typing: true, ...more });
  screen('cal-edit', 'Event form', 'after c or E: a field to a row as in compose; a date is typed as words and the status line says what it was taken to mean', form());

  screen('help-calendar', 'Help, in the calendar', '? in the calendar leads with the calendar\'s keys; typing filters every key and command as it does in mail', helpScreen(W, H, '', 0, 'calendar'));

  flow('cal-zoom', 'Enter goes deeper, Esc comes back', [
    { key: 'gc', caption: 'Month', note: 'From mail. The calendar opens where it was left; the first time, on today.', rows: cut(month(LAYOUT.default), 11) },
    { key: 'Enter', caption: 'Day', note: 'The cursor moves to the first event of the day. j and k step through events, across days.', rows: cut(day(LAYOUT.default), 11) },
    { key: 'Enter', caption: 'Event', note: 'J and K step to the events on either side without going back, as they do from a message.', rows: cut(event(LAYOUT.default), 11) },
    { key: 'Esc', caption: 'Day again', note: 'Esc undoes one Enter. gm returns to mail from anywhere.', rows: cut(day(LAYOUT.default), 11) },
  ]);

  const road = 'Q4 roadmap review';
  flow('cal-answer', 'Answering an invitation', [
    { key: 'i', caption: 'Asked', note: 'The same question in the calendar, on the event and in the reader.',
      rows: calendarStrip(thu, { selected: 'roadmap', command: said(SAY.answer) }) },
    { key: 'a', caption: 'Accepted', note: 'The reply waits in the Outbox for the send delay, from the account the invitation came to, and the event takes the answer in that account\'s calendar.',
      rows: calendarStrip(thu, { selected: 'roadmap', changed: { roadmap: { answer: 'accepted' } }, command: said(SAY.answered('Accepted', road, 'work')) }) },
    { key: 'u', caption: 'Undone', note: 'The reply never left and the event waits for an answer again.',
      rows: calendarStrip(thu, { selected: 'roadmap', command: said(SAY.unanswered(road)) }) },
  ]);
  flow('cal-propose', 'Proposing another time', [
    { key: 'i p', caption: 'Asked when', note: 'A date is typed as it is for gd.',
      rows: calendarStrip(thu, { selected: 'roadmap', left: '', command: typed('Propose: ') }) },
    { key: 'thu 14:00', caption: 'Typed', note: 'The status line says what the words were taken to mean, before anything is sent.',
      rows: calendarStrip(thu, { selected: 'roadmap', left: SAY.proposes('Thu 26 Nov 14:00 to 15:30'), command: typed('Propose: thu 14:00') }) },
    { key: 'Enter', caption: 'Proposed', note: 'The organizer gets the proposal as a calendar part and as a sentence. The event keeps its time until they answer.',
      rows: calendarStrip(thu, { selected: 'roadmap', changed: { roadmap: { answer: 'proposed', proposed: 'Thu 14:00' } }, command: said(SAY.proposed('Thu 26 Nov 14:00', name('imani'))) }) },
  ]);
  flow('cal-ignore', 'Ignoring an invitation', [
    { key: 'i', caption: 'Asked', note: 'An invitation nobody expected.',
      rows: calendarStrip(fri, { selected: 'review', command: said(SAY.answer) }) },
    { key: 'x', caption: 'Ignored', note: 'Nothing is sent and the event leaves the calendar. The mail stays and can still be answered.',
      rows: calendarStrip(fri, { changed: { review: { gone: true } }, command: said(SAY.ignored('Account review')) }) },
  ]);

  const reviewLink = occurrence('review', fri).link;
  const review = occurrence('review', fri);
  flow('cal-join', 'Joining a meeting', [
    { key: 'o', caption: 'Its links', note: 'o is what it is in the reader: the links, each with the host it really opens. The meeting comes first and is selected.',
      rows: calendarStrip(thu, { selected: 'roadmap', panel: panel('Links', eventLinks(theEvent), 0), left: SAY.opens(theEvent.link.url), command: linksHint() }) },
    { key: 'Enter', caption: 'Opened', note: 'Enter, or 1, opens it in the browser.',
      rows: calendarStrip(thu, { selected: 'roadmap', left: SAY.opens(theEvent.link.url), command: said(SAY.joining(theEvent.link.host)) }) },
    { key: 'o', caption: 'A link that lies', note: 'A link whose text names another host says so in the panel.',
      rows: calendarStrip(fri, { selected: 'review', panel: panel('Links', eventLinks(review), 0), left: SAY.opens(reviewLink.url), command: linksHint() }) },
    { key: 'Enter', caption: 'Asked first', note: 'And is confirmed before it opens, as in mail (ADR-018).',
      rows: calendarStrip(fri, { selected: 'review', left: SAY.opens(reviewLink.url), command: said(SAY.deceptive(reviewLink.shown, reviewLink.host, true)) }) },
    { key: 'n', caption: 'Not opened', note: 'Any key but y leaves it closed.',
      rows: calendarStrip(fri, { selected: 'review' }) },
  ]);

  flow('cal-new', 'A new event with guests', [
    { key: 'c', caption: 'The form', note: 'On the cursor\'s day, in the calendar of the account the view is narrowed to, or the first.', rows: cut(form(), 13) },
    { key: '^s', caption: 'Who gets mail', note: 'Guests are mailed an invitation, so saving asks as sending does.',
      rows: cut(form({ typing: false, command: said(SAY.inviteAsk(2, '10s')) }), 13) },
    { key: 'y', caption: 'Added', note: 'The event is tuimex\'s own, and says so.',
      rows: calendarStrip(thu, { selected: 'offsite', added: [offsite], command: said(SAY.added('Offsite planning', 'work, local')) }) },
  ]);
  flow('cal-delete', 'Deleting, and undoing it', [
    { key: 'd', caption: 'Deleted', note: 'No question first: u reverses it (ADR-015).',
      rows: calendarStrip(thu, { changed: { vendor: { gone: true } }, command: said(SAY.deleted('Vendor call')) }) },
    { key: 'u', caption: 'Undone', note: '',
      rows: calendarStrip(thu, { selected: 'vendor', command: said(SAY.restored('Vendor call')) }) },
  ]);

  flow('cal-repeating', 'A repeating event', [
    { key: 'd', caption: 'Asked which', note: 'Deleting or editing (E) one day of an event that repeats asks whether it means that day or every day. Nothing else in the calendar asks first.',
      rows: calendarStrip(thu, { selected: 'standup', command: said(SAY.whichOnes('Delete')) }) },
    { key: 't', caption: 'This one', note: 'Thursday\'s is gone and the others stay.',
      rows: calendarStrip(thu, { changed: { standup: { skip: [thu] } }, command: said(SAY.deletedOne('Standup', dayShort(thu))) }) },
    { key: 'u', caption: 'Undone', note: '',
      rows: calendarStrip(thu, { selected: 'standup', command: said(SAY.restored('Standup')) }) },
  ]);

  flow('cal-reminder', 'A reminder, and turning them off', [
    { key: '', caption: 'Ten minutes before', note: 'While tuimex is open: the bell, this line wherever you are, and a desktop notification carrying the title alone (ADR-058).',
      rows: calendarStrip(TODAY, { selected: 'halden-call', reminder: ['Halden contract call', '10 min'] }) },
    { key: 'o Enter', caption: 'Joined', note: 'The same two keys as anywhere: the links, then the one to open.',
      rows: calendarStrip(TODAY, { selected: 'halden-call', left: SAY.opens(occurrence('halden-call', TODAY).link.url), command: said(SAY.joining('meet.example.org')) }) },
    { key: 'gn', caption: 'Off', note: 'Remembered across runs. Off is shown; on is not.',
      rows: calendarStrip(TODAY, { selected: 'halden-call', remindersOff: true, command: said(SAY.remindersOff) }) },
    { key: 'gn', caption: 'On again', note: '',
      rows: calendarStrip(TODAY, { selected: 'halden-call', command: said(SAY.remindersOn) }) },
  ]);

  const dec5 = dayNumber('2026-12-05');
  flow('cal-goto', 'Going to a date', [
    { key: 'gd', caption: 'Asked', note: 'A date, a weekday, or a step: 2026-12-05, 5 dec, fri, +2w.',
      rows: calendarStrip(thu, { left: '', command: typed('Go to: ') }) },
    { key: '5 dec', caption: 'Typed', note: 'The status line says the day the words mean.',
      rows: calendarStrip(thu, { left: SAY.goesTo(`${dayShort(dec5)} 2026`), command: typed('Go to: 5 dec') }) },
    { key: 'Enter', caption: 'There', note: 'The view stays what it was; only the date moves.', rows: cut(month(LAYOUT.default, { cursor: dec5 }), 11) },
  ]);

  return out;
}

window.TUI = {
  build,
  palette: PALETTE,
  glyphs: GLYPHS,
  keymap: KEYMAP,
  commands: COMMANDS,
  layout: LAYOUT,
  wording: SAY,
  separation,
  contrast,
  accountOrder,
  wordsColour,
  fixture: { now: NOW, accounts: ACCOUNTS, folders: FOLDERS, sections: SECTIONS, mail: MAIL, drafts: DRAFTS, outbox: OUTBOX, calendars: CALENDARS, events: EVENTS },
};
})();

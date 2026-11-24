//! The list screen as `design/tui.js` draws it: `items`, `listRow`,
//! `grouped`, `listScreen` and `frame`, ported and kept behaviourally
//! identical (ADR-002).

mod chrome;
mod preview;

use std::borrow::Cow;

use tuimex_core::{AccountId, Check, Envelope, Folder, Listed, MessageId, Role, Sending, Timestamp, thread_at};

pub(crate) use self::chrome::thousands;
pub use self::chrome::{
    AccountSync, Sync, Tab, alerts, heading, hint, list_hint, panel, position, status_row, tabs_row, target, typed,
};
use crate::glyphs::{BAR, CURSOR, DOT, FLAG, FOLDED, MARK, RULE, UNFOLDED};
use crate::keymap;
use crate::layout::{self, Cut, INDENT, MARGIN, PREVIEW_MIN_HEIGHT};
use crate::palette::Tok;
use crate::span::{Line, Span, cells, fill, fit, gap, highlight, run};
use crate::when::{GROUPS, group_of, short_date, span_of};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    Folded,
    Open,
}

/// One row: a message, a folded thread, or one message of an unfolded one.
#[derive(Clone, Debug)]
pub struct Row<'a> {
    /// The message the row opens and previews: for a folded thread, its
    /// newest.
    pub msg: &'a Listed,
    /// The messages an action on the row takes: every one of a folded
    /// thread, and otherwise the row's own.
    pub ids: Vec<&'a MessageId>,
    /// The root of the row's thread, which is the name it is unfolded by.
    pub root: &'a MessageId,
    pub from: Cow<'a, str>,
    pub subject: &'a str,
    /// How many messages an action on the row takes.
    pub size: usize,
    pub depth: usize,
    pub fold: Option<Fold>,
    pub reply: bool,
    /// The date of the newest message of the row's thread, which is the
    /// group the whole thread stays in.
    pub newest: Timestamp,
    pub unread: bool,
    pub flagged: bool,
    pub files: usize,
    /// The folder a search found the message in, when it is not an inbox.
    pub folder: Option<&'a str>,
    /// The words around what a search matched in the body.
    pub excerpt: Option<&'a str>,
}

impl<'a> Row<'a> {
    fn single(msg: &'a Listed) -> Row<'a> {
        let e = &msg.envelope;
        let name = |a: &'a tuimex_core::Address| a.name.as_deref().filter(|n| !n.is_empty()).unwrap_or(&a.email);
        // A draft has not been sent by anyone yet: it is headed by who it
        // is to.
        let from = match e.flags.draft {
            true => Cow::Owned(format!("to {}", e.to.iter().map(name).collect::<Vec<_>>().join(", "))),
            false => Cow::Borrowed(e.from.first().map_or("", name)),
        };
        Row {
            msg,
            ids: vec![&e.id],
            root: &e.id,
            from,
            subject: &e.subject,
            size: 1,
            depth: 0,
            fold: None,
            reply: false,
            newest: e.date,
            unread: !e.flags.seen,
            flagged: e.flags.flagged,
            files: msg.attachments.len(),
            folder: None,
            excerpt: None,
        }
    }
}

/// One conversation among the mail it was found in: each message by its
/// place there and its depth, the root first, and the date of the newest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Threaded {
    pub members: Vec<(usize, u16)>,
    pub newest: Timestamp,
}

/// The conversations among the messages of `mail` at the places `at`,
/// newest first. This is the costly half of `rows`, and depends on nothing
/// but which messages those are, so a caller with a large mailbox keeps it
/// for as long as they are the same.
pub fn threads(mail: &[Listed], at: &[usize]) -> Vec<Threaded> {
    let envelopes: Vec<&Envelope> = at.iter().map(|&i| &mail[i].envelope).collect();
    let mut threads: Vec<Threaded> = thread_at(&envelopes)
        .into_iter()
        .map(|members| {
            let newest = members.iter().map(|&(i, _)| envelopes[i].date).max().unwrap_or(Timestamp(0));
            Threaded { members: members.into_iter().map(|(i, depth)| (at[i], depth)).collect(), newest }
        })
        .collect();
    threads.sort_by_key(|thread| std::cmp::Reverse(thread.newest));
    threads
}

/// The rows `threads` are drawn as. `open` names, by their roots, the
/// threads shown unfolded: the root, then each reply under what it answers.
pub fn rows_of<'a>(mail: &'a [Listed], threads: &[Threaded], open: &[MessageId]) -> Vec<Row<'a>> {
    let mut out = Vec::with_capacity(threads.len());
    for Threaded { members, newest } in threads {
        let Some(root) = members.first().map(|&(i, _)| &mail[i]) else { continue };
        if members.len() == 1 {
            out.push(Row::single(root));
        } else if open.contains(&root.envelope.id) {
            out.extend(members.iter().map(|&(i, depth)| {
                let msg = &mail[i];
                let row =
                    Row { depth: usize::from(depth), newest: *newest, root: &root.envelope.id, ..Row::single(msg) };
                if depth == 0 {
                    Row { fold: Some(Fold::Open), ..row }
                } else {
                    Row { subject: &msg.snippet, reply: true, ..row }
                }
            }));
        } else {
            let all = || members.iter().map(|&(i, _)| &mail[i]);
            let latest = all().max_by_key(|m| m.envelope.date).unwrap_or(root);
            out.push(Row {
                ids: all().map(|m| &m.envelope.id).collect(),
                root: &root.envelope.id,
                size: members.len(),
                fold: Some(Fold::Folded),
                subject: &root.envelope.subject,
                newest: *newest,
                unread: all().any(|m| !m.envelope.flags.seen),
                flagged: all().any(|m| m.envelope.flags.flagged),
                files: all().map(|m| m.attachments.len()).sum(),
                ..Row::single(latest)
            });
        }
    }
    out
}

/// The rows of a threaded section, newest thread first.
pub fn rows<'a>(mail: &'a [Listed], open: &[MessageId]) -> Vec<Row<'a>> {
    rows_of(mail, &threads(mail, &(0..mail.len()).collect::<Vec<_>>()), open)
}

/// The outbox, which is not threaded: each row is headed by who it is to,
/// and is bright because it has not gone.
pub fn outbox<'a>(mail: impl IntoIterator<Item = &'a Listed>) -> Vec<Row<'a>> {
    let name = |a: &tuimex_core::Address| a.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| a.email.clone());
    mail.into_iter()
        .map(|msg| {
            let to: Vec<String> = msg.envelope.to.iter().map(name).collect();
            Row { from: Cow::Owned(format!("to {}", to.join(", "))), unread: true, ..Row::single(msg) }
        })
        .collect()
}

/// A message a search found, which is a row of its own wherever it is
/// filed: headed by who it is to when it was sent from here, and saying
/// its folder unless that is an inbox.
pub fn found<'a>(msg: &'a Listed, folder: Option<&'a Folder>, excerpt: Option<&'a str>) -> Row<'a> {
    let sent = folder.is_some_and(|f| f.role == Role::Sent);
    let row = if sent { outbox([msg]).pop().unwrap_or_else(|| Row::single(msg)) } else { Row::single(msg) };
    let folder = folder.filter(|f| f.role != Role::Inbox).map(|f| f.name.as_str());
    Row { unread: !msg.envelope.flags.seen, folder, excerpt, ..row }
}

/// A row's columns after the indent, for one width: the design's
/// `columns`. `letters` is how much of an account's name is written in
/// `account` cells, and the subject takes what the others leave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Columns {
    pub letters: usize,
    pub account: usize,
    pub from: usize,
    pub subject: usize,
    pub words: usize,
    pub date: usize,
}

/// `accounts` are those a row names its own among: none when there is one
/// account, or the list is narrowed to one.
pub(crate) fn columns(width: usize, accounts: &[AccountId]) -> Columns {
    let longest = accounts.iter().map(|a| account_label(accounts, a, layout::ACCOUNT_MOST).chars().count()).max();
    let letters = longest.unwrap_or(0);
    let account = if letters > 0 { letters + layout::ACCOUNT_GAP } else { 0 };
    let mut c = Columns { letters, account, from: layout::FROM, subject: 0, words: layout::WORDS, date: layout::DATE };
    let fixed = |c: &Columns| {
        let words = if c.words > 0 { layout::WORDS_GAP + c.words } else { 0 };
        let date = if c.date > 0 { layout::DATE_GAP + c.date } else { 0 };
        INDENT + c.account + c.from + layout::FROM_GAP + 2 + words + date + 2 + MARGIN
    };
    for (cut, under) in layout::COLLAPSE {
        if width >= fixed(&c) + under {
            continue;
        }
        match cut {
            Cut::Words(to) => c.words = c.words.min(to),
            Cut::From(to) => c.from = c.from.min(to),
            Cut::Date(to) => c.date = c.date.min(to),
            Cut::AccountShort if c.letters > layout::ACCOUNT_SHORT => {
                (c.letters, c.account) = (layout::ACCOUNT_SHORT, layout::ACCOUNT_SHORT + layout::ACCOUNT_SHORT_GAP);
            }
            Cut::AccountShort => {}
            Cut::Account(_) => (c.letters, c.account) = (0, 0),
        }
    }
    c.subject = width.saturating_sub(fixed(&c));
    c
}

struct Look<'a> {
    selected: bool,
    marked: bool,
    needle: Option<&'a str>,
    /// A row the filter does not match.
    faint: bool,
    stale: bool,
    /// The colour of the row's account.
    account: Tok,
    /// The accounts a row names its own among, when accounts have a column.
    accounts: &'a [AccountId],
    /// The focus is in the preview, so the cursor here is dim.
    unfocused: bool,
    words: (Tok, Tok),
}

/// The words about a row, most useful first, each in its three forms: a
/// number and a noun, or a word alone. A word is drawn as the nouns are
/// unless it is one of `WORDS_BRIGHT`, which go with the numbers, or has a
/// colour of its own.
fn words_of(row: &Row, stale: bool) -> Vec<[(String, String, Option<Tok>); 3]> {
    let mut words = Vec::new();
    let plain = |forms: [&str; 3], own: Option<Tok>| {
        let bright = layout::WORDS_BRIGHT.contains(&forms[0]);
        // A bright word is all number, and the number's colour.
        forms.map(|form| match bright {
            true => (form.to_string(), String::new(), own),
            false => (String::new(), form.to_string(), own),
        })
    };
    let counted = |n: usize, full: &str, mid: &str, tight: &str| {
        let s = if n == 1 { "" } else { "s" };
        let mid = if mid == "msg" { " msg".to_string() } else { format!(" {mid}{s}") };
        [format!(" {full}{s}"), mid, tight.to_string()].map(|noun| (n.to_string(), noun, None))
    };
    if stale {
        words.push(plain(["stale", "stale", "old"], None));
    }
    if let Some(Sending::Held { seconds }) = row.msg.sending {
        let left = span_of(seconds);
        words.push(plain([&format!("sends in {left}"), &format!("in {left}"), &left], Some(Tok::Fg)));
    }
    if row.msg.envelope.origin == tuimex_core::Origin::Agent {
        words.push(plain(["agent draft", "agent", "agt"], Some(Tok::Dim)));
    }
    if let Some(folder) = row.folder {
        words.push(plain([folder, folder, &folder.chars().take(3).collect::<String>()], None));
    }
    if row.msg.invite {
        words.push(plain(["invite", "invite", "inv"], None));
    }
    if row.fold == Some(Fold::Folded) {
        words.push(counted(row.size, "message", "msg", "m"));
    }
    if row.files > 0 {
        words.push(counted(row.files, "file", "file", "f"));
    }
    if row.msg.envelope.flags.answered {
        words.push(plain(["replied", "replied", "re"], None));
    }
    words
}

/// As many of the row's words as fit in `width` cells, in the form that
/// column has room for, set to its right. Nothing when there are none.
/// `ink` is the colour of the numbers and of the nouns.
fn words(row: &Row, width: usize, stale: bool, ink: (Tok, Tok), tone: impl Fn(Tok) -> Tok) -> Line {
    let form = layout::WORD_FORMS.iter().position(|from| width >= *from).unwrap_or(2);
    let between = if form == 2 { " ".to_string() } else { format!(" {DOT} ") };
    let mut out = Line::new();
    for forms in words_of(row, stale) {
        let Some((number, noun, own)) = forms.into_iter().nth(form) else { continue };
        let mut next = vec![run(number, tone(own.unwrap_or(ink.0))), run(noun, tone(own.unwrap_or(ink.1)))];
        if !out.is_empty() {
            next.insert(0, run(between.as_str(), tone(ink.1)));
        }
        if cells(&out) + cells(&next) > width {
            break;
        }
        out.extend(next);
    }
    if !out.is_empty() {
        out.insert(0, gap(width - cells(&out)));
    }
    out
}

/// A row needs you when it is bright: unread is bold, flagged is plain,
/// and everything read is dim.
fn list_row(row: &Row, width: usize, now: Timestamp, look: &Look) -> Line {
    let c = columns(width, look.accounts);
    let tone = |fg: Tok| if look.faint { Tok::Faint } else { fg };
    let text = tone(if row.unread || row.flagged { Tok::Fg } else { Tok::Dim });
    let needle = look.needle.filter(|_| !look.faint);
    let glyph = |on: bool, c: char, fg: Tok| if on { run(c, fg) } else { gap(1) };

    let cursor = if look.unfocused { Tok::Dim } else { Tok::Acc };
    let mut line = vec![glyph(look.selected, CURSOR, cursor), gap(1), glyph(look.marked, MARK, tone(Tok::Fg)), gap(1)];
    if c.account > 0 {
        let name = account_label(look.accounts, &row.msg.envelope.id.account, c.letters);
        line.extend(fit(&[run(name, tone(look.account))], c.account));
    }
    let from = Span { bold: row.unread && !look.faint, ..run("", text) };
    line.extend(fit(&highlight(&row.from, needle, from), c.from));
    line.push(gap(layout::FROM_GAP));
    line.push(match row.fold {
        Some(fold) => run(if fold == Fold::Folded { FOLDED } else { UNFOLDED }, tone(Tok::Dim)),
        None => gap(1),
    });
    line.push(gap(1));

    let mut subject = vec![gap(2 * row.depth)];
    subject.extend(highlight(row.subject, needle, run("", text)));
    // What a search found in the body is shown when the row would not
    // otherwise show why it is there.
    let shows = |text: &str| needle.is_some_and(|n| text.to_lowercase().contains(&n.to_lowercase()));
    if let Some(excerpt) = row.excerpt.filter(|_| !shows(row.subject) && !shows(&row.from)) {
        subject.push(gap(2));
        subject.extend(highlight(excerpt, needle, run("", tone(Tok::Dim))));
    }
    if let Some(Sending::Failed { reason }) = &row.msg.sending {
        subject.extend([gap(2), run(reason.as_str(), tone(Tok::Err))]);
    }
    // A row with no words lets its subject run on through their column.
    let words = if c.words > 0 { words(row, c.words, look.stale, look.words, tone) } else { Line::new() };
    if words.is_empty() {
        line.extend(fit(&subject, c.subject + if c.words > 0 { layout::WORDS_GAP + c.words } else { 0 }));
    } else {
        line.extend(fit(&subject, c.subject));
        line.push(gap(layout::WORDS_GAP));
        line.extend(words);
    }
    if c.date > 0 {
        let group = group_of(row.newest, now);
        let date = short_date(row.msg.envelope.date, group, now);
        line.extend([gap(layout::DATE_GAP), run(format!("{date:>width$}", width = c.date), tone(Tok::Dim))]);
    }
    line.extend([gap(1), glyph(row.flagged, FLAG, tone(Tok::Warn)), gap(MARGIN)]);
    if look.selected { fill(line, Tok::Sel) } else { line }
}

/// The colours accounts take, in the config's order: each its turn round
/// `order`, but for one the config gave a colour of its own.
pub fn account_colours(order: &[Tok], accounts: &[AccountId], fixed: &[(AccountId, Tok)]) -> Vec<Tok> {
    let own = |account: &AccountId| fixed.iter().find(|(named, _)| named == account).map(|(_, colour)| *colour);
    let turn = |at: usize| order.get(at % order.len().max(1)).copied().unwrap_or(Tok::Fg);
    accounts.iter().enumerate().map(|(at, account)| own(account).unwrap_or_else(|| turn(at))).collect()
}

/// An account's name in `cells` cells: the design's `accountShort`. A
/// name that must be cut first loses what it shares with another
/// account's, when that is three characters or more, so `gmail-home` and
/// `gmail-work` are `home` and `work` and never the same four letters.
pub fn account_label(accounts: &[AccountId], account: &AccountId, cells: usize) -> String {
    let name: Vec<char> = account.0.chars().collect();
    if name.len() <= cells {
        return account.0.clone();
    }
    let shared = |other: &AccountId| name.iter().zip(other.0.chars()).take_while(|(a, b)| **a == *b).count();
    let prefix = accounts.iter().filter(|other| *other != account).map(shared).max().unwrap_or(0);
    name[if prefix >= 3 { prefix } else { 0 }..].iter().take(cells).collect()
}

/// A filter as it is drawn: what was typed, and the rows it matches.
#[derive(Clone, Copy, Debug)]
pub struct Filter<'a> {
    pub needle: &'a str,
    pub matches: &'a [MessageId],
}

/// `ui.preview`: where the preview is asked to be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preview {
    /// Beside the list in a wide terminal and under it otherwise.
    #[default]
    Auto,
    Right,
    Below,
    Off,
}

impl Preview {
    pub fn from_name(name: &str) -> Option<Preview> {
        [Preview::Auto, Preview::Right, Preview::Below, Preview::Off].into_iter().find(|p| p.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Preview::Auto => "auto",
            Preview::Right => "right",
            Preview::Below => "below",
            Preview::Off => "off",
        }
    }
}

/// Where the preview is in a terminal of one size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Off,
    Below,
    Right,
}

/// The design's `previewPlace`. A terminal too short has no preview, and
/// one too narrow for the list's sender column beside it has it below.
pub fn place(preview: Preview, width: usize, height: usize) -> Place {
    let fits_beside = width >= layout::preview_width(width) + 1 + INDENT + columns(0, &[]).from + 8 + MARGIN;
    match preview {
        _ if height < PREVIEW_MIN_HEIGHT => Place::Off,
        Preview::Off => Place::Off,
        Preview::Right if fits_beside => Place::Right,
        Preview::Auto if width >= layout::PREVIEW_BESIDE_FROM => Place::Right,
        _ => Place::Below,
    }
}

/// Everything the list screen shows; the design's `listScreen` options.
#[derive(Clone, Debug)]
pub struct List<'a> {
    pub tabs: &'a [Tab],
    /// The name of the tab that is open.
    pub section: &'a str,
    /// The query of the search being shown, which takes the active tab's
    /// place.
    pub search: Option<&'a str>,
    pub rows: &'a [Row<'a>],
    pub selected: Option<usize>,
    pub marks: &'a [MessageId],
    pub filter: Option<Filter<'a>>,
    pub preview: Preview,
    /// Whether the focus is in the preview, and how many of its lines are
    /// scrolled off the top.
    pub focus: bool,
    pub pane_top: usize,
    /// What `gpg` made of the selected row's message, once it was read.
    pub check: Option<&'a Check>,
    /// The selected row's message as it was read, when it has been, and
    /// whether its body is still on its way here.
    pub body: Option<&'a tuimex_core::Body>,
    pub arriving: bool,
    /// The accounts whose rows say they are stale.
    pub stale: &'a [AccountId],
    /// The accounts, in the config's order, and the colour of each
    /// (`account_colours`): with more than one, and none of them narrowed
    /// to, a row names its own in a column.
    pub accounts: &'a [AccountId],
    pub colours: &'a [Tok],
    /// The colour of the numbers in a row's words, and of the nouns
    /// (`Palette::words`).
    pub words: (Tok, Tok),
    /// The one account the list is narrowed to.
    pub narrowed: Option<&'a AccountId>,
    /// Whether rows sit under date groups. The outbox's do not.
    pub grouped: bool,
    /// The two lines that stand where rows would be when there are none.
    pub empty: [String; 2],
    /// The two ends of the status line.
    pub left: Line,
    pub right: Line,
    /// What the command line holds; the hint for the selected row when it
    /// is idle.
    pub command: Option<Line>,
    /// What `panel` made, which takes the rows above the status line: the
    /// end of a preview under the list, and otherwise of the list.
    pub panel: Vec<Line>,
    /// How many lines of the rows and their labels are scrolled off the
    /// top; `top` says what it has to be.
    pub top: usize,
    pub now: Timestamp,
}

/// What an empty section says: that, and how fresh it is once there has
/// been a sync to be fresh from.
pub fn nothing_here(synced: Option<u32>) -> [String; 2] {
    ["Nothing here.".to_string(), synced.map_or_else(String::new, |age| format!("Synced {} ago.", span_of(age)))]
}

/// How one size divides between the list and the preview.
struct Split {
    place: Place,
    /// The cells of a list row, and the lines the rows and labels have.
    list_width: usize,
    list_room: usize,
    /// The rows the preview has, its subject among them.
    pane_rows: usize,
}

fn split(o: &List, width: usize, height: usize) -> Split {
    let room = height.saturating_sub(2 + 3 + o.panel.len());
    let wanted =
        if o.selected.is_some_and(|i| i < o.rows.len()) { place(o.preview, width, height) } else { Place::Off };
    // Under the list the preview gives the panel its last rows, and with
    // fewer than its subject and one line left it is not drawn.
    let under = layout::preview_height(height).saturating_sub(o.panel.len());
    match wanted {
        Place::Right => Split {
            place: Place::Right,
            list_width: width - layout::preview_width(width) - 1,
            list_room: room,
            pane_rows: room,
        },
        Place::Below if under >= 4 && room > under => {
            Split { place: Place::Below, list_width: width, list_room: room - under, pane_rows: under - 1 }
        }
        _ => Split { place: Place::Off, list_width: width, list_room: room, pane_rows: 0 },
    }
}

/// How many lines the rows and their labels have.
pub fn room(o: &List, width: usize, height: usize) -> usize {
    split(o, width, height).list_room
}

/// Where the rows have to start for the selected one to show: `o.top`,
/// moved as little as that takes. A row that heads its group brings the
/// label with it, so the first row of a section is never shown without one.
pub fn top(o: &List, width: usize, height: usize) -> usize {
    let Some(selected) = o.selected.filter(|i| *i < o.rows.len()) else { return 0 };
    let (mut line, mut first, mut group) = (0, 0, None);
    for (i, row) in o.rows[..=selected].iter().enumerate() {
        let of = group_of(row.newest, o.now);
        if o.grouped && group != Some(of) {
            line += usize::from(group.is_some());
            first = line;
            line += 1;
            group = Some(of);
        } else {
            first = line;
        }
        line += usize::from(i < selected);
    }
    let room = room(o, width, height);
    if first < o.top {
        first
    } else if line >= o.top + room {
        (line + 1).saturating_sub(room)
    } else {
        o.top
    }
}

fn colour_of(o: &List, account: &AccountId) -> Tok {
    let at = o.accounts.iter().position(|a| a == account);
    at.and_then(|at| o.colours.get(at)).copied().unwrap_or(Tok::Fg)
}

/// As many lines of the rows and their labels as fit in `room`, from
/// `o.top`, never ending on a label with no row under it.
fn rows_in_sight(o: &List, width: usize, room: usize) -> Vec<Line> {
    let draw = |i: usize, row: &Row| {
        let id = &row.msg.envelope.id;
        let look = Look {
            selected: o.selected == Some(i),
            marked: o.marks.contains(id),
            needle: o.filter.map(|f| f.needle),
            faint: o.filter.is_some_and(|f| !f.matches.contains(id)),
            stale: o.stale.contains(&id.account),
            account: colour_of(o, &id.account),
            accounts: if o.accounts.len() > 1 && o.narrowed.is_none() { o.accounts } else { &[] },
            unfocused: o.focus,
            words: o.words,
        };
        list_row(row, width, o.now, &look)
    };
    // Only the lines in sight are made: a section may hold every message
    // of a long mailbox, and a frame shows a screenful of them. Each is
    // kept with whether it is a label.
    let (mut body, mut at): (Vec<(Line, bool)>, usize) = (Vec::new(), 0);
    let mut put = |label: bool, line: &dyn Fn() -> Line| {
        if at >= o.top && body.len() < room {
            body.push((line(), label));
        }
        at += 1;
    };
    if o.rows.is_empty() {
        put(false, &Line::new);
        put(false, &|| vec![gap(INDENT), run(o.empty[0].as_str(), Tok::Fg)]);
        put(false, &|| vec![gap(INDENT), run(o.empty[1].as_str(), Tok::Dim)]);
    }
    let mut group = None;
    for (i, row) in o.rows.iter().enumerate() {
        let of = group_of(row.newest, o.now);
        if o.grouped && group != Some(of) {
            if group.is_some() {
                put(false, &Line::new);
            }
            put(true, &|| vec![gap(MARGIN), run(GROUPS[of], Tok::Dim).bold()]);
            group = Some(of);
        }
        put(false, &|| draw(i, row));
    }
    while body.last().is_some_and(|(line, label)| *label || cells(line) == 0) {
        body.pop();
    }
    let mut lines: Vec<Line> = body.into_iter().map(|(line, _)| line).collect();
    lines.resize(room, Vec::new());
    lines
}

/// What `draw` drew beside the lines: where the preview is, and how far
/// it is scrolled and can be.
pub struct Drawn {
    pub lines: Vec<Line>,
    pub place: Place,
    pub pane_top: usize,
    pub pane_last_top: usize,
    pub pane_room: usize,
}

/// The whole screen, one line to a row: the tabs, as many rows as fit, the
/// preview under or beside them, the panel, a blank row, the status line
/// and the command line.
pub fn draw(o: &List, width: usize, height: usize) -> Drawn {
    let split = split(o, width, height);
    let selected = o.selected.and_then(|i| o.rows.get(i));
    let list = rows_in_sight(o, split.list_width, split.list_room);
    let pane = selected.filter(|_| split.place != Place::Off).map(|row| {
        let pane_width = if split.place == Place::Right { layout::preview_width(width) } else { width };
        preview::pane(row.msg, o, pane_width, split.pane_rows)
    });
    let mut lines = vec![tabs_row(width, o.tabs, o.section, o.search), vec![]];
    match (split.place, &pane) {
        (Place::Below, Some(pane)) => {
            lines.extend(list);
            lines.push(vec![run(RULE.to_string().repeat(width), Tok::Faint)]);
            lines.extend(pane.rows.iter().cloned());
        }
        (Place::Right, Some(pane)) => lines.extend(list.iter().zip(&pane.rows).map(|(row, beside)| {
            let mut line = fit(row, split.list_width);
            line.push(run(BAR, Tok::Faint));
            line.extend(beside.iter().cloned());
            line
        })),
        _ => lines.extend(list),
    }
    lines.extend(o.panel.iter().cloned());

    let focused = o.focus && pane.is_some();
    let back = if split.place == Place::Right { "Alt-h" } else { "Alt-k" };
    let command = o.command.clone().unwrap_or_else(|| match focused {
        true => hint(&[("j k", "scroll"), (back, "list"), keymap::hint("list.open"), keymap::hint("global.help")]),
        false => list_hint(selected, o.marks, o.search.is_some()),
    });
    let left = match (&pane, focused, o.narrowed) {
        (Some(pane), true, _) => {
            vec![run(format!("lines {}-{} of {}", pane.top + 1, pane.last, pane.total), Tok::Dim)]
        }
        (_, _, Some(account)) => {
            let mut left = vec![
                run(account.0.as_str(), colour_of(o, account)),
                gap(1),
                run(format!("only {DOT}"), Tok::Dim),
                gap(1),
            ];
            left.extend(o.left.iter().cloned());
            left
        }
        _ => o.left.clone(),
    };
    lines.extend([vec![], status_row(width, &left, &o.right), command]);
    let scrolled = pane.as_ref().map_or((0, 0, 0), |pane| (pane.top, pane.last_top, pane.room));
    Drawn { lines, place: split.place, pane_top: scrolled.0, pane_last_top: scrolled.1, pane_room: scrolled.2 }
}

pub fn screen(o: &List, width: usize, height: usize) -> Vec<Line> {
    draw(o, width, height).lines
}

#[cfg(test)]
mod tests;

//! The state behind the screen, and the keys that change it. A key is
//! looked up in the keymap for the mode that is open; nothing here names a
//! key of its own.

mod act;
mod calendar;
mod draft;
mod filter;
mod find;
mod read;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tuimex_core::{
    AccountId, Action, Ask, Identity, Is, Listed, MessageId, Query, Section, Sending, Start, Term, Timestamp,
};

use self::act::Change;
use crate::command::{self, COMMANDS};
use crate::compose::Compose;
use crate::editor::Editor;
use crate::keymap::{self, Found, Key};
use crate::list::{self, AccountSync, Filter, List, Place, Preview, Row, Sync, Tab, Threaded};
use crate::mailbox::Mailbox;
use crate::paint::{Theme, paint};
use crate::palette::Tok;
use crate::say::{self, Said};
use crate::span::{Line, run};
use crate::{help, when};

/// The tab that is there while a message is waiting to go or was refused.
const OUTBOX: &str = "Outbox";
/// PLAN §6.3: `:messages` keeps the last 200.
const LOG: usize = 200;

#[derive(Debug)]
enum Mode {
    Normal,
    Command(Editor),
    Filter(Editor),
    /// The question, and what `y` does.
    Confirm(say::Question, Action),
    /// What is typed to filter the keys and commands, and how many lines
    /// are scrolled off the top.
    Help(String, usize),
    /// How many entries are scrolled off the top.
    Messages(usize),
    /// `A` was pressed: the agent's menu is open, and the next key picks
    /// from it or closes it. Nothing has left the machine (ADR-031).
    Agent,
    /// Something the calendar asked for is being typed: what the command
    /// line shows before it, and the text so far.
    Prompt(&'static str, Editor),
}

/// The conversations of one tab, kept for as long as the same messages
/// are in it: threading a long mailbox takes longer than a frame, a frame
/// asks for several tabs' rows, and a flag set or a message arriving in
/// one folder leaves most tabs as they were.
#[derive(Debug)]
struct Kept {
    /// What `at` was worked out from: `App::version`, how many messages
    /// there were, and the account the tabs were narrowed to.
    of: (u64, usize, Option<AccountId>),
    /// The places in the mail of the tab's messages.
    at: Vec<usize>,
    threads: Rc<Vec<Threaded>>,
    /// The threads shown unfolded, in order: where each is among the
    /// threads, and how many rows it has beyond its first.
    unfolded: Rc<Vec<(usize, usize)>>,
    /// How many of the rows are unread, once that has been counted.
    unread: Cell<Option<usize>>,
}

impl Kept {
    /// Takes the messages at `gone` out, and says whether that could be
    /// done without threading the tab again: it can when each leaves with
    /// its whole conversation, which no message left behind was part of.
    fn leave(&mut self, gone: &[usize], mail: &[Listed]) -> bool {
        let mut emptied = Vec::new();
        for &place in gone {
            let Some((thread, _)) = find(&self.threads, mail, place) else { return false };
            if !self.threads[thread].members.iter().all(|(member, _)| gone.contains(member)) {
                return false;
            }
            emptied.push(thread);
        }
        emptied.sort_unstable();
        emptied.dedup();
        let threads = Rc::make_mut(&mut self.threads);
        let unfolded = Rc::make_mut(&mut self.unfolded);
        for thread in emptied.into_iter().rev() {
            threads.remove(thread);
            unfolded.retain(|(at, _)| *at != thread);
            unfolded.iter_mut().filter(|(at, _)| *at > thread).for_each(|(at, _)| *at -= 1);
        }
        self.at.retain(|place| !gone.contains(place));
        true
    }
}

/// Which of `threads`, newest first, holds the message at `place` in the
/// mail, and where among its members. A thread is no older than any
/// message of it, and most are one message, so the search starts at the
/// last thread as new as the message and goes back.
fn find(threads: &[Threaded], mail: &[Listed], place: usize) -> Option<(usize, usize)> {
    let date = mail.get(place)?.envelope.date;
    let as_new = threads.partition_point(|thread| thread.newest >= date);
    let holds = |thread: &Threaded| thread.members.iter().position(|(member, _)| *member == place);
    (0..as_new).rev().find_map(|at| Some((at, holds(&threads[at])?)))
}

/// The rows of a section's tab by their numbers, for whoever needs a few
/// of them and not a row made for every thread of a long mailbox.
struct Shown {
    threads: Rc<Vec<Threaded>>,
    unfolded: Rc<Vec<(usize, usize)>>,
}

impl Shown {
    fn len(&self) -> usize {
        self.threads.len() + self.unfolded.iter().map(|(_, more)| more).sum::<usize>()
    }

    /// The row a thread's first message is on.
    fn row_of(&self, thread: usize) -> usize {
        thread + self.unfolded.iter().take_while(|(at, _)| *at < thread).map(|(_, more)| more).sum::<usize>()
    }

    /// The thread a row is of.
    fn thread_of(&self, row: usize) -> usize {
        let mut before = 0;
        for &(at, more) in self.unfolded.iter() {
            if row <= at + before + more {
                return (row - before).min(at);
            }
            before += more;
        }
        row - before
    }
}

#[derive(Debug)]
pub struct App {
    /// Set when the user has asked to leave; the event loop ends on it.
    pub quit: bool,
    theme: Theme,
    mailbox: Mailbox,
    mode: Mode,
    /// The start of a sequence such as `gg`, until its next key or
    /// `timeout`.
    pending: Vec<Key>,
    tab: usize,
    selected: usize,
    top: usize,
    /// Rows of the open section, by the message each opens. Leaving the
    /// section clears them, so an action never takes a row out of sight.
    marks: Vec<MessageId>,
    /// The row Space last marked, where `V` starts from.
    anchor: Option<MessageId>,
    /// The threads shown unfolded, by their roots.
    open: Vec<MessageId>,
    filter: Option<String>,
    /// The one account `:account` narrowed every section to.
    account: Option<AccountId>,
    /// Where the preview is asked to be, and where `p` puts it back.
    preview: Preview,
    preview_was: Preview,
    /// Whether the focus is in the preview, the message it shows, and
    /// what was last drawn of it: where it lay, how far it was scrolled
    /// and could be, and how many lines it had.
    focus: bool,
    pane_of: Option<MessageId>,
    pane: (Place, usize, usize, usize),
    /// The search being shown in place of a section, and where the cursor
    /// and the scroll were when `/` was pressed, which typing moves.
    search: Option<find::Search>,
    sought_from: (usize, usize),
    said: Option<Said>,
    /// What Tab puts on the command line, after an error that offered it.
    fix: Option<String>,
    log: Vec<(Timestamp, Said)>,
    history: Vec<String>,
    undo: Vec<Change>,
    asks: Vec<Ask>,
    serial: u64,
    redraw: bool,
    /// How many lines the rows had when last drawn, which is the page `^d`
    /// and `^u` move by half of.
    room: usize,
    /// The message being written, which takes the screen while it is open.
    compose: Option<Compose>,
    /// The addresses mail can be sent from, in the config's order.
    identities: Vec<Identity>,
    /// Seconds a confirmed message is held before it goes (ADR-015).
    delay: u32,
    /// The accounts that are not in step with their servers, and why.
    syncing: Vec<(AccountId, AccountSync)>,
    /// The asks no refresh has yet been read after, with the messages each
    /// names: what is shown of those is newer than what a refresh holds.
    unsettled: Vec<(u64, Vec<MessageId>)>,
    /// When each account last finished a pass, in this run.
    synced_at: Vec<(AccountId, Timestamp)>,
    /// When each held message goes and each failing account is tried
    /// again, which `tick` counts the seconds on screen down to.
    due: HashMap<MessageId, Timestamp>,
    retry_at: Vec<(AccountId, Timestamp)>,
    /// Whether the config has an `[agent]` table (ADR-031).
    agent: bool,
    /// The accents accounts take in turn, which the palette decides, and
    /// the accounts the config gave a colour of their own.
    order: Vec<Tok>,
    fixed: Vec<(AccountId, Tok)>,
    reading: read::Reading,
    /// Counts the changes to the mail. Everything that changes a message,
    /// or which messages there are, goes through `message` or `mail_mut`,
    /// which count.
    version: u64,
    threads: RefCell<Vec<Option<Kept>>>,
    sifted: RefCell<filter::Sifted>,
    /// Whether any message is in an outbox, and the `version` that was
    /// true of.
    outbox: Cell<(u64, bool)>,
    /// The calendar, which is a place beside mail (ADR-054).
    cal: calendar::Place,
}

impl App {
    pub fn new(theme: Theme, mailbox: Mailbox) -> Self {
        let now = mailbox.now;
        App {
            quit: false,
            theme,
            mailbox,
            mode: Mode::Normal,
            pending: Vec::new(),
            tab: 0,
            selected: 0,
            top: 0,
            marks: Vec::new(),
            anchor: None,
            open: Vec::new(),
            filter: None,
            account: None,
            preview: Preview::Auto,
            preview_was: Preview::Auto,
            focus: false,
            pane_of: None,
            pane: (Place::Off, 0, 0, 0),
            search: None,
            sought_from: (0, 0),
            said: None,
            fix: None,
            log: Vec::new(),
            history: Vec::new(),
            undo: Vec::new(),
            asks: Vec::new(),
            serial: 0,
            redraw: false,
            room: 0,
            compose: None,
            identities: Vec::new(),
            delay: 10,
            syncing: Vec::new(),
            synced_at: Vec::new(),
            due: HashMap::new(),
            retry_at: Vec::new(),
            unsettled: Vec::new(),
            agent: false,
            order: theme.palette.account_order(),
            fixed: Vec::new(),
            reading: read::Reading::default(),
            version: 0,
            threads: RefCell::default(),
            sifted: RefCell::default(),
            outbox: Cell::new((u64::MAX, false)),
            cal: calendar::Place::new(now),
        }
    }

    /// What the user has asked for since this was last called, oldest
    /// first. Each has already taken effect here (ADR-015).
    pub fn asks(&mut self) -> Vec<Ask> {
        std::mem::take(&mut self.asks)
    }

    /// Whether `^l` was pressed since this was last called.
    pub fn take_redraw(&mut self) -> bool {
        std::mem::take(&mut self.redraw)
    }

    /// Whether a sequence is half typed, so the caller knows to call
    /// `timeout` when `keymap::TIMEOUT_MS` pass without a key.
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn timeout(&mut self) {
        self.pending.clear();
    }

    /// Shows `said` on the command line and keeps it for `:messages`.
    fn say(&mut self, said: Said) {
        self.note(self.mailbox.now, said.clone());
        self.said = Some(said);
    }

    /// Keeps `said` for `:messages` without showing it.
    pub fn note(&mut self, at: Timestamp, said: Said) {
        self.log.push((at, said));
        if self.log.len() > LOG {
            self.log.remove(0);
        }
    }

    // ---- what is on screen

    /// The messages in an outbox. A frame asks several times, of what may
    /// be a long mailbox with none, so whether there is one is kept.
    fn held(&self) -> impl Iterator<Item = &Listed> {
        let sending = |m: &&Listed| m.sending.is_some();
        if self.outbox.get().0 != self.version {
            self.outbox.set((self.version, self.mailbox.mail.iter().any(|m| sending(&m))));
        }
        let all = if self.outbox.get().1 { self.mailbox.mail.as_slice() } else { &[] };
        all.iter().filter(sending)
    }

    fn tab_count(&self) -> usize {
        self.mailbox.sections.len() + usize::from(self.held().next().is_some())
    }

    /// The open tab. The outbox's goes when the outbox empties, and the
    /// last section is then the one open.
    fn tab(&self) -> usize {
        self.tab.min(self.tab_count().saturating_sub(1))
    }

    fn tab_name(&self, tab: usize) -> &str {
        self.mailbox.sections.get(tab).map_or(OUTBOX, |s| &s.name)
    }

    /// Every message, to change. What is kept of the mail as it was is
    /// worked out again.
    fn mail_mut(&mut self) -> &mut Vec<Listed> {
        self.version += 1;
        &mut self.mailbox.mail
    }

    /// Whether a message is one of a section's tab.
    fn within<'a>(&'a self, section: &'a Section) -> impl Fn(&Listed) -> bool + 'a {
        let matches = self.mailbox.matcher(&section.query);
        let narrowed = |m: &Listed| self.account.as_ref().is_none_or(|a| m.envelope.id.account == *a);
        move |m| m.sending.is_none() && narrowed(m) && matches(m)
    }

    /// What is kept of the mail as it stands now.
    fn of(&self) -> (u64, usize, Option<AccountId>) {
        (self.version, self.mailbox.mail.len(), self.account.clone())
    }

    /// The conversations of a section's tab, and which are unfolded.
    fn shown(&self, tab: usize, section: &Section) -> Shown {
        let mut tabs = self.threads.borrow_mut();
        if tabs.len() <= tab {
            tabs.resize_with(tab + 1, || None);
        }
        let (mail, of) = (&self.mailbox.mail, self.of());
        if let Some(kept) = tabs[tab].as_ref().filter(|kept| kept.of == of) {
            return Shown { threads: kept.threads.clone(), unfolded: kept.unfolded.clone() };
        }
        let within = self.within(section);
        let at: Vec<usize> = (0..mail.len()).filter(|&i| within(&mail[i])).collect();
        // A message is changed in place and never swapped for another, so
        // the same places are the same messages, threaded as they were.
        let threads = match tabs[tab].take().filter(|kept| kept.at == at && kept.of.1 == of.1) {
            Some(kept) => kept.threads,
            None => Rc::new(list::threads(mail, &at)),
        };
        let root = |thread: &Threaded| thread.members.first().map(|&(place, _)| &mail[place].envelope.id);
        let more = |thread: &Threaded| thread.members.len() - 1;
        let open = |thread: &Threaded| more(thread) > 0 && root(thread).is_some_and(|id| self.open.contains(id));
        let unfolded: Vec<(usize, usize)> = match self.open.is_empty() {
            true => Vec::new(),
            false => threads.iter().enumerate().filter(|(_, t)| open(t)).map(|(at, t)| (at, more(t))).collect(),
        };
        let shown = Shown { threads, unfolded: Rc::new(unfolded) };
        let (threads, unfolded) = (shown.threads.clone(), shown.unfolded.clone());
        tabs[tab] = Some(Kept { of, at, threads, unfolded, unread: Cell::new(None) });
        shown
    }

    /// Says that the messages at `places` in the mail were changed where
    /// they are, in nothing a thread is made from.
    pub(super) fn touched(&mut self, places: &[usize]) {
        self.settle(self.of(), places);
    }

    /// Carries what is kept of the mail as `was` over to the mail as it
    /// is, when all that differs is the messages at `places`: changed in
    /// nothing a thread is made from, or added after the rest. A tab is
    /// kept on when they are still in it, or left it with their whole
    /// conversations; a tab one of them joined is worked out again.
    pub(super) fn settle(&mut self, was: (u64, usize, Option<AccountId>), places: &[usize]) {
        self.version += 1;
        let mut tabs = self.threads.take();
        for (tab, slot) in tabs.iter_mut().enumerate() {
            let (Some(mut kept), Some(section)) = (slot.take(), self.mailbox.sections.get(tab)) else { continue };
            let (within, mail) = (self.within(section), &self.mailbox.mail);
            let here = |place: &&usize| kept.at.binary_search(place).is_ok();
            let left: Vec<usize> = places.iter().filter(here).filter(|&&i| !within(&mail[i])).copied().collect();
            let joined = places.iter().any(|place| !here(&place) && within(&mail[*place]));
            if kept.of == was && !joined && kept.leave(&left, mail) {
                kept.of = self.of();
                kept.unread.set(None);
                *slot = Some(kept);
            }
        }
        self.threads.replace(tabs);
    }

    /// The rows of a tab: a section's threads, or the outbox.
    fn rows_of(&self, tab: usize) -> Vec<Row<'_>> {
        let narrowed = |m: &&Listed| self.account.as_ref().is_none_or(|a| m.envelope.id.account == *a);
        match self.mailbox.sections.get(tab) {
            Some(section) => list::rows_of(&self.mailbox.mail, &self.shown(tab, section).threads, &self.open),
            None => list::outbox(self.held().filter(narrowed)),
        }
    }

    /// The rows on screen: what the search found while one is shown, and
    /// otherwise the open tab's.
    fn rows(&self) -> Vec<Row<'_>> {
        self.found().unwrap_or_else(|| self.rows_of(self.tab()))
    }

    /// The section whose threads are the rows on screen: none while the
    /// outbox or what a search found is shown.
    fn sectioned(&self) -> Option<&Section> {
        self.mailbox.sections.get(self.tab()).filter(|_| self.search.is_none())
    }

    /// How many rows the open tab has.
    pub(super) fn count(&self) -> usize {
        match self.sectioned() {
            Some(section) => self.shown(self.tab(), section).len(),
            None => self.rows().len(),
        }
    }

    /// The rows of the open tab that are within `reach` rows of row `at`,
    /// and the number of the first of them. A frame shows a screenful and
    /// a key acts on one, of what may be every thread of a long mailbox.
    fn rows_near(&self, at: usize, reach: usize) -> (Vec<Row<'_>>, usize) {
        let Some(section) = self.sectioned() else { return (self.rows(), 0) };
        let shown = self.shown(self.tab(), section);
        let last = shown.threads.len().saturating_sub(1);
        let (from, to) =
            (shown.thread_of(at.saturating_sub(reach)).min(last), shown.thread_of(at.saturating_add(reach)).min(last));
        let threads = shown.threads.get(from..=to).unwrap_or_default();
        (list::rows_of(&self.mailbox.mail, threads, &self.open), shown.row_of(from))
    }

    /// The row of that number in the open tab.
    pub(super) fn row(&self, at: usize) -> Option<Row<'_>> {
        let (mut rows, first) = self.rows_near(at, 0);
        (at >= first && at - first < rows.len()).then(|| rows.swap_remove(at - first))
    }

    /// The number of the row an action on that message takes, when the
    /// open tab has one.
    pub(super) fn row_of(&self, id: &MessageId) -> Option<usize> {
        let Some(section) = self.sectioned() else {
            return self.rows().iter().position(|row| row.ids.contains(&id));
        };
        let shown = self.shown(self.tab(), section);
        let place = self.mailbox.mail.iter().position(|m| m.envelope.id == *id)?;
        let (thread, member) = find(&shown.threads, &self.mailbox.mail, place)?;
        let unfolded = shown.unfolded.iter().any(|(at, _)| *at == thread);
        Some(shown.row_of(thread) + if unfolded { member } else { 0 })
    }

    /// How many rows of a section's tab are unread.
    fn unread(&self, tab: usize, section: &Section) -> usize {
        let kept = |tabs: &Vec<Option<Kept>>| tabs.get(tab).and_then(Option::as_ref).and_then(|kept| kept.unread.get());
        self.shown(tab, section);
        if let Some(counted) = kept(&self.threads.borrow()) {
            return counted;
        }
        let shown = self.shown(tab, section);
        let unread = |&(place, _): &(usize, u16)| !self.mailbox.mail[place].envelope.flags.seen;
        let rows = |(at, thread): (usize, &Threaded)| match shown.unfolded.iter().any(|(open, _)| *open == at) {
            true => thread.members.iter().filter(|member| unread(member)).count(),
            false => usize::from(thread.members.iter().any(unread)),
        };
        let counted = shown.threads.iter().enumerate().map(rows).sum();
        if let Some(Some(kept)) = self.threads.borrow().get(tab) {
            kept.unread.set(Some(counted));
        }
        counted
    }

    /// A tab has a number only when the number means "this needs you":
    /// unread rows on the inbox, every row of a section of flagged mail,
    /// and what was not sent.
    fn tabs(&self) -> Vec<Tab> {
        let mut tabs: Vec<Tab> = (0..self.mailbox.sections.len())
            .map(|i| {
                let section = &self.mailbox.sections[i];
                let (count, tone) = match section.query.terms.as_slice() {
                    terms if terms.contains(&Term::Is(Is::Flagged)) => (self.shown(i, section).len(), Tok::Warn),
                    [Term::In(name)] if name.eq_ignore_ascii_case("inbox") => (self.unread(i, section), Tok::Fg),
                    _ => (0, Tok::Fg),
                };
                Tab { name: section.name.clone(), count, tone }
            })
            .collect();
        if self.held().next().is_some() {
            let failed = self.held().filter(|m| matches!(m.sending, Some(Sending::Failed { .. }))).count();
            tabs.push(Tab { name: OUTBOX.to_string(), count: failed, tone: Tok::Err });
        }
        tabs
    }

    /// The right of the status line: a reminder that is due, wherever
    /// the reader is (ADR-058), then what is not normal about the mail.
    fn alerts(&self) -> Line {
        let reminder = self.reminding().map(|(what, lead)| crate::calendar::parts::reminder(what, &lead));
        let parts: Vec<Line> =
            reminder.into_iter().chain([self.mail_alerts()]).filter(|part| !part.is_empty()).collect();
        parts.join(&run(format!(" {} ", crate::glyphs::DOT), Tok::Dim))
    }

    fn mail_alerts(&self) -> Line {
        let held = self.held().find_map(|m| match m.sending {
            Some(Sending::Held { seconds }) => Some(seconds),
            _ => None,
        });
        let failed = self.held().filter(|m| matches!(m.sending, Some(Sending::Failed { .. }))).count();
        let accounts = self.syncing.iter().map(|(account, state)| (account.0.clone(), state.clone())).collect();
        list::alerts(&Sync { held, failed, accounts })
    }

    /// The rows the filter matches, by the message each opens, when every
    /// row has been made: the outbox's, a search's, or a section's where
    /// `filter::Level` is not asked. A folded thread matches when any
    /// message of it does.
    fn matching(&self, rows: &[Row]) -> Option<Vec<MessageId>> {
        let query = Query::parse(self.filter.as_deref()?);
        let matches = self.mailbox.matcher(&query);
        // Asked of the tab's own messages, by their places in the mail.
        let hits: HashSet<&MessageId> = match self.mailbox.sections.get(self.tab()) {
            Some(section) => {
                let threads = self.shown(self.tab(), section).threads;
                let members = threads.iter().flat_map(|thread| thread.members.iter());
                let found = members.map(|&(i, _)| &self.mailbox.mail[i]).filter(|m| matches(m));
                found.map(|m| &m.envelope.id).collect()
            }
            None => self.held().filter(|m| matches(m)).map(|m| &m.envelope.id).collect(),
        };
        let hit = |row: &&Row| row.ids.iter().any(|id| hits.contains(*id));
        Some(rows.iter().filter(hit).map(|row| row.msg.envelope.id.clone()).collect())
    }

    /// The left of the status line: where the cursor is, and what an
    /// action would take; while a filter is typed, how many rows here
    /// match and what Enter does. `row` is the row `at` of `total`.
    fn left(&self, row: Option<&Row>, at: usize, total: usize, matches: Option<usize>) -> Line {
        if let Some(unsent) = self.agent_unsent() {
            return unsent;
        }
        if let (Mode::Filter(_), Some(matches)) = (&self.mode, matches) {
            let here = run(format!("{matches} here {}", crate::glyphs::DOT), Tok::Dim);
            return vec![here, run(" Enter ", Tok::Fg), run("searches all mail", Tok::Dim)];
        }
        let Some(row) = row else { return Line::new() };
        match &row.msg.sending {
            Some(sending) => {
                let what = if matches!(sending, Sending::Held { .. }) { "waiting to send" } else { "not sent" };
                list::position(at + 1, total, vec![run(what, Tok::Fg)])
            }
            None => list::position(at + 1, total, list::target(row, &self.marks)),
        }
    }

    /// The command line, and the panel over the status line, for the mode
    /// that is open. `fits` is how many entries a panel has room for.
    fn command_row(&self, width: usize, fits: usize) -> (Option<Line>, Vec<Line>) {
        let no_panel = Vec::new();
        match &self.mode {
            Mode::Normal | Mode::Help(..) => (self.said.as_ref().map(|said| said.line(width)), no_panel),
            Mode::Filter(editor) => (Some(list::typed(&format!("/{}", editor.text))), no_panel),
            Mode::Prompt(label, editor) => (Some(list::typed(&format!("{label}{}", editor.text))), no_panel),
            Mode::Confirm(question, _) => {
                (Some(self.images_question(width).unwrap_or_else(|| question.line(width))), no_panel)
            }
            Mode::Command(editor) => {
                let panel = editor.menu.as_ref().map_or(no_panel, |menu| {
                    let from = (menu.selected + 1).saturating_sub(fits);
                    let entries = menu.items.iter().skip(from).take(fits).map(|(_, drawn)| drawn.clone()).collect();
                    list::panel(&menu.title, entries, Some(menu.selected - from), width)
                });
                (Some(list::typed(&format!(":{}", editor.text))), panel)
            }
            Mode::Agent => {
                let entry = |id: &&str| {
                    let (key, does) = keymap::hint(id);
                    vec![run(format!("{key:<4}"), Tok::Fg), run(does, Tok::Dim)]
                };
                let entries = self.agent_offers().iter().map(entry).collect();
                (Some(list::hint(&[("Esc", "cancel")])), list::panel("Agent", entries, None, width))
            }
            Mode::Messages(top) => {
                let entry = |(at, said): &(Timestamp, Said)| {
                    let mut line = vec![run(format!("{}  ", when::clock(*at)), Tok::Dim)];
                    line.extend(said.logged());
                    line
                };
                let entries = self.log.iter().rev().skip(*top).take(fits).map(entry).collect();
                let keys = [("j k", "scroll"), keymap::hint("global.back")];
                (Some(list::hint(&keys)), list::panel("Messages", entries, None, width))
            }
        }
    }

    /// The screen, one line to a row.
    pub fn lines(&mut self, width: usize, height: usize) -> Vec<Line> {
        // A picture is where this frame's reader put it; a frame that draws
        // anything else over the reader has no picture to report.
        self.reading.pictures.clear();
        let right = self.alerts();
        if let Mode::Help(needle, top) = &self.mode {
            let drawn = help::screen(needle, *top, self.cal.open, &right, width, height);
            (self.room, self.mode) = (drawn.room, Mode::Help(needle.clone(), drawn.top));
            return drawn.lines;
        }
        if self.cal.open {
            let mail = self.mail_alerts();
            return self.calendar_lines(width, height, mail);
        }
        if let Some(lines) = self.compose_lines(width, height) {
            return lines;
        }
        if let Some(lines) = self.reader_lines(width, height) {
            return lines;
        }
        // The rows a frame can show are within a screen of the cursor; a
        // search's rows are all made.
        let reach = if self.search.is_some() { usize::MAX } else { 2 * height + 4 };
        self.list_lines(width, height, reach, right)
    }

    /// The list's screen, made from the rows within `reach` of the cursor.
    fn list_lines(&mut self, width: usize, height: usize, reach: usize, right: Line) -> Vec<Line> {
        let (tab, tabs, total) = (self.tab(), self.tabs(), self.count());
        let at = self.selected.min(total.saturating_sub(1));
        let (rows, first) = self.rows_near(at, reach);
        let selected = (!rows.is_empty()).then(|| at - first);
        // Lines are counted from the first row made, which a label of its
        // group is drawn over, out of sight: the lines the rows before it
        // took, less that label's one.
        let groups = self.groups_to(first);
        let sectioned = self.search.is_none() && tab < self.mailbox.sections.len();
        let above = if sectioned { first + (2 * groups).saturating_sub(2) } else { 0 };
        let query = self.filter.as_deref().or(self.search.as_ref().map(|search| search.query.as_str()));
        // Every row a search found matches it; a filter being typed is
        // asked of the section, and of the rows here where they are all
        // of them.
        let sifted = self.sifted().filter(|_| reach != usize::MAX);
        let matches = match (&self.search, &sifted) {
            (Some(_), _) => Some(rows.iter().map(|row| row.msg.envelope.id.clone()).collect()),
            (None, Some((level, shown))) => Some(level.among(shown, first, &rows)),
            (None, None) => self.matching(&rows),
        };
        let matched = sifted.map(|(level, shown)| level.count(&shown)).or(matches.as_ref().map(Vec::len));
        // What a row underlines is the first free text of the query; a
        // term such as `is:unread` has nothing to underline.
        let needle = Query::parse(query.unwrap_or("")).terms.into_iter().find_map(|t| match t {
            Term::Text(text) => Some(text),
            _ => None,
        });
        let filter = matches.as_ref().map(|m| Filter { needle: needle.as_deref().unwrap_or(""), matches: m });
        let (command, panel) = self.command_row(width, (height / 3).min(13).saturating_sub(2).max(1));
        // Rows of an account that is failing may be behind its server.
        let failing = self.syncing.iter().filter(|(_, state)| matches!(state, AccountSync::Failing { .. }));
        let stale: Vec<AccountId> = failing.map(|(account, _)| account.clone()).collect();
        let accounts: Vec<AccountId> = self.accounts().into_iter().map(|name| AccountId(name.to_string())).collect();
        let colours = list::account_colours(&self.order, &accounts, &self.fixed);
        let shown = selected.map(|at| rows[at].msg.envelope.id.clone());
        let pane_top = if shown == self.pane_of { self.pane.1 } else { 0 };
        let typing = matches!(self.mode, Mode::Filter(_));
        let mut list = List {
            accounts: &accounts,
            colours: &colours,
            words: self.theme.palette.words(&colours),
            narrowed: self.account.as_ref(),
            tabs: &tabs,
            section: self.tab_name(tab),
            search: self.search.as_ref().map(|search| search.query.as_str()),
            rows: &rows,
            selected,
            marks: &self.marks,
            filter,
            // The rows are what is being looked through while a filter is
            // typed, so they have the whole screen.
            preview: if typing { Preview::Off } else { self.preview },
            focus: self.focus,
            pane_top,
            check: shown.as_ref().and_then(|id| self.reading.check(id)),
            body: shown.as_ref().and_then(|id| self.body_of(id)),
            arriving: selected.is_some_and(|at| rows[at].msg.text.is_none())
                && shown.as_ref().is_some_and(|id| self.arriving(id)),
            stale: &stale,
            grouped: self.search.is_some() || tab < self.mailbox.sections.len(),
            empty: self
                .search
                .as_ref()
                .map_or_else(|| list::nothing_here(self.mailbox.synced), |s| s.empty(accounts.len())),
            left: self.left(selected.map(|near| &rows[near]), at, total, matched),
            right,
            command,
            panel,
            top: self.top.saturating_sub(above),
            now: self.mailbox.now,
        };
        list.top = list::top(&list, width, height);
        let (top, room) = (list.top + above, list::room(&list, width, height));
        let drawn = list::draw(&list, width, height);
        drop(rows);
        (self.top, self.room, self.pane_of) = (top, room, shown);
        // A preview that is not drawn cannot hold the focus.
        self.focus &= typing || drawn.place != Place::Off;
        if !typing {
            self.pane = (drawn.place, drawn.pane_top, drawn.pane_last_top, drawn.pane_room);
        }
        drawn.lines
    }

    /// How many date groups the open tab's rows have up to row `at`, the
    /// newest thread being first.
    fn groups_to(&self, at: usize) -> usize {
        let Some(section) = self.sectioned() else { return 0 };
        let shown = self.shown(self.tab(), section);
        let threads = &shown.threads[..(shown.thread_of(at) + 1).min(shown.threads.len())];
        let group = |thread: &Threaded| when::group_of(thread.newest, self.mailbox.now);
        let (mut groups, mut from) = (0, 0);
        while let Some(first) = threads.get(from) {
            groups += 1;
            from += threads[from..].partition_point(|thread| group(thread) == group(first));
        }
        groups
    }

    /// Every cell of `area` is written, so nothing of an earlier frame or
    /// size shows through.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let mut lines = self.lines(usize::from(area.width), usize::from(area.height));
        lines.resize(usize::from(area.height), Vec::new());
        paint(&lines, buf, area, &self.theme);
    }

    // ---- keys

    pub fn key(&mut self, key: Key) {
        let confirming = matches!(self.mode, Mode::Confirm(..));
        self.key_in_mode(key);
        // A question that is still there has not been answered.
        if confirming && !matches!(self.mode, Mode::Confirm(..)) {
            self.reader_answered();
            self.calendar_answered();
        }
    }

    fn key_in_mode(&mut self, key: Key) {
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal => self.normal_key(key),
            Mode::Command(editor) => self.command_key(editor, key),
            Mode::Filter(editor) => self.filter_key(editor, key),
            Mode::Prompt(label, editor) => self.prompt_key(label, editor, key),
            // `n`, Esc and every other key leave the question, and the key is
            // not then read as anything else.
            Mode::Confirm(question, action) => match keymap::find(&["confirm", "global"], &[key]) {
                Found::Bound(binding, _) if binding.id == "confirm.yes" => self.confirmed(action),
                Found::Bound(binding, i) => self.reader_declined(binding.id, i, &question, action),
                _ => {}
            },
            Mode::Help(mut needle, mut top) => {
                let half = (self.room / 2).max(1);
                match (keymap::find(&["help"], &[key]), key) {
                    (_, Key::Esc) => return,
                    (Found::Bound(_, 0), _) => top += half,
                    (Found::Bound(..), _) => top = top.saturating_sub(half),
                    (_, Key::Backspace) => {
                        needle.pop();
                    }
                    (_, Key::Char(c)) => needle.push(c),
                    _ => {}
                }
                self.mode = Mode::Help(needle, top);
            }
            Mode::Agent => {
                if let Found::Bound(binding, _) = keymap::find(&["agent"], &[key])
                    && self.agent_offers().contains(&binding.id)
                {
                    self.agent_pick(binding.id);
                }
            }
            Mode::Messages(top) => {
                self.mode = match keymap::find(&["list", "global"], &[key]) {
                    Found::Bound(binding, i) if binding.id == "list.move" => Mode::Messages(if i == 0 {
                        (top + 1).min(self.log.len().saturating_sub(1))
                    } else {
                        top.saturating_sub(1)
                    }),
                    Found::Bound(binding, _) if binding.id == "global.back" => Mode::Normal,
                    _ => Mode::Messages(top),
                }
            }
        }
    }

    fn normal_key(&mut self, key: Key) {
        if self.compose.is_some() {
            return self.compose_key(key);
        }
        self.said = None;
        let fix = self.fix.take();
        if let (Key::Tab, Some(fix)) = (key, fix) {
            let mut editor = Editor::with(&fix);
            // A fix that ends where a name belongs opens the choices.
            if fix.ends_with(' ') {
                self.complete(&mut editor, true);
            }
            self.mode = Mode::Command(editor);
            return;
        }
        if self.cal.open {
            return self.calendar_key(key);
        }
        if self.reading.id().is_some() {
            return self.reader_key(key);
        }
        self.pending.push(key);
        let modes: &[&str] = if self.focus { &["preview", "list", "global"] } else { &["list", "global"] };
        match keymap::find(modes, &self.pending) {
            Found::Prefix => {}
            Found::Nothing => {
                self.pending.clear();
                // A reminder offers its event's links by the key that
                // lists links in the calendar, which the list does not bind.
                if matches!(keymap::find(&["calendar"], &[key]), Found::Bound(b, _) if b.id == "calendar.links") {
                    self.reminder_links();
                }
            }
            Found::Bound(binding, i) => {
                self.pending.clear();
                self.run(binding.id, i);
            }
        }
    }

    /// What a binding of the list does; `i` is which of its keys was
    /// pressed. Compose's bindings are not looked up while the list is
    /// open, the reader's are `reader_run`'s, and a test walks the table
    /// to say the rest are all here.
    fn run(&mut self, id: &str, i: usize) {
        let (tabs, last) = (self.tab_count(), self.count().saturating_sub(1));
        let (at, half) = (self.selected.min(last), (self.room / 2).max(1));
        match (id, i) {
            ("global.command", _) => self.mode = Mode::Command(Editor::default()),
            ("global.help", _) => self.mode = Mode::Help(String::new(), 0),
            ("global.back", _) if self.focus => self.focus = false,
            ("global.back", _) if self.marks.is_empty() && self.search.is_some() => self.close_search(),
            ("global.back", _) => {
                self.marks.clear();
                self.anchor = None;
            }
            ("preview.scroll", i) => {
                let (_, top, last_top, room) = self.pane;
                let half = (room / 2).max(1);
                self.pane.1 = match i {
                    0 => (top + 1).min(last_top),
                    1 => top.saturating_sub(1),
                    2 => (top + half).min(last_top),
                    3 => top.saturating_sub(half),
                    4 => 0,
                    _ => last_top,
                };
            }
            ("preview.focus-list", _) => self.focus = false,
            ("list.focus-preview", _) => {
                self.focus = self.pane.0 != Place::Off;
                self.rest();
            }
            ("list.preview-place", _) => {
                self.preview = if self.pane.0 == Place::Right { Preview::Below } else { Preview::Right };
            }
            ("list.account", i) => {
                // Every account is the step between the last and the first.
                let accounts = self.accounts();
                let n = accounts.len() + 1;
                let at =
                    self.account.as_ref().and_then(|now| accounts.iter().position(|a| *a == now.0)).unwrap_or(n - 1);
                let next = accounts.get((at + if i == 0 { 1 } else { n - 1 }) % n).map(|name| name.to_string());
                self.narrow(next);
            }
            ("global.section-prev", _) => self.switch((self.tab() + tabs - 1) % tabs),
            ("global.section-next", _) => self.switch((self.tab() + 1) % tabs),
            ("global.section", n) if n < tabs => self.switch(n),
            ("global.section", n) => self.say(say::err(format!("section {}: there are {tabs}", n + 1))),
            ("global.undo", _) => self.undo(),
            ("global.agent", _) => self.agent_menu(),
            ("global.redraw", _) => self.redraw = true,
            ("global.calendar", _) => self.calendar_open(None),
            ("list.move", 0) => self.selected = (at + 1).min(last),
            ("list.move", _) => self.selected = at.saturating_sub(1),
            ("list.ends", 0) => self.selected = 0,
            ("list.ends", _) => self.selected = last,
            ("list.page", 0) => self.selected = (at + half).min(last),
            ("list.page", _) => self.selected = at.saturating_sub(half),
            ("list.filter", _) => {
                let editor = Editor::with(self.filter.as_deref().unwrap_or(""));
                self.filter = Some(editor.text.clone());
                self.sought_from = (at, self.top);
                self.mode = Mode::Filter(editor);
            }
            // Every row of a search is a match, and round its ends is the
            // next one.
            ("list.match", 0) if self.search.is_some() => self.selected = (at + 1) % (last + 1),
            ("list.match", _) if self.search.is_some() => self.selected = (at + last) % (last + 1),
            ("list.match", _) => self.say(say::err("n: no filter").key("/ to find")),
            ("list.mark", _) => self.mark(at, false),
            ("list.mark-range", _) => self.mark(at, true),
            ("list.preview", _) if self.preview == Preview::Off => self.preview = self.preview_was,
            ("list.preview", _) => (self.preview_was, self.preview) = (self.preview, Preview::Off),
            ("list.archive", _) => self.act(Action::Archive, false),
            ("list.trash", _) => self.act(Action::Trash, false),
            ("list.flag", _) => self.act(Action::Flag(true), true),
            ("list.read", _) => self.act(Action::Seen(true), true),
            ("list.move-to", _) => self.mode = Mode::Command(Editor::with("move ")),
            ("list.unfold", _) => self.fold(at, true),
            ("list.fold", _) => self.fold(at, false),
            ("list.open", _) if self.on_refused() => self.compose(Start::FromDraft),
            ("list.browser", _) => self.browser(),
            ("list.open", _) => self.open_selected(),
            ("list.compose", _) => self.compose(Start::New),
            ("list.reply", _) => self.compose(Start::Reply),
            ("list.reply-all", _) => self.compose(Start::ReplyAll),
            ("list.forward", _) => self.compose(Start::Forward),
            _ => {}
        }
    }

    fn switch(&mut self, tab: usize) {
        (self.tab, self.selected, self.top) = (tab, 0, 0);
        (self.filter, self.anchor, self.search, self.focus) = (None, None, None, false);
        self.marks.clear();
    }

    /// Shows one account only, or with `None` every account, from its
    /// first row, and says so.
    fn narrow(&mut self, account: Option<String>) {
        (self.account, self.selected, self.top) = (account.map(AccountId), 0, 0);
        self.marks.clear();
        self.say(match &self.account {
            Some(account) => say::plain(format!("Showing {} only", account.0)).key("gt for the next, :account for all"),
            None => say::plain("Showing every account"),
        });
    }

    /// `[[account]] color`: the accounts that do not take their turn.
    pub fn set_colours(&mut self, fixed: Vec<(AccountId, Tok)>) {
        self.fixed = fixed;
    }

    /// `ui.preview` and `:preview`.
    pub fn set_preview(&mut self, preview: Preview) {
        self.preview = preview;
        if preview != Preview::Off {
            self.preview_was = preview;
        }
    }

    /// Space marks or unmarks the row. `V` marks every row from the one
    /// Space last marked to this one.
    fn mark(&mut self, at: usize, range: bool) {
        let rows = self.rows();
        let id = |row: &Row| row.msg.envelope.id.clone();
        let from = self.anchor.as_ref().filter(|_| range).and_then(|a| rows.iter().position(|row| id(row) == *a));
        let taken: Vec<MessageId> = match (rows.get(at), from) {
            (None, _) => return,
            (Some(_), Some(from)) => rows[from.min(at)..=from.max(at)].iter().map(id).collect(),
            (Some(row), None) => vec![id(row)],
        };
        if let ([one], None) = (taken.as_slice(), from)
            && let Some(marked) = self.marks.iter().position(|m| m == one)
        {
            self.marks.remove(marked);
            return;
        }
        self.anchor = taken.last().cloned();
        for id in taken {
            if !self.marks.contains(&id) {
                self.marks.push(id);
            }
        }
    }

    /// `zo` and `zc`. Folding leaves the cursor on the thread's one row.
    fn fold(&mut self, at: usize, unfold: bool) {
        let rows = self.rows();
        let Some(row) = rows.get(at) else { return };
        let root = row.root.clone();
        match (unfold, row.fold, row.reply) {
            (true, Some(list::Fold::Folded), _) => {
                self.open.push(root);
                self.version += 1;
            }
            (false, Some(list::Fold::Open), _) | (false, None, true) => {
                self.open.retain(|open| *open != root);
                // Which threads are unfolded is kept with the threads.
                self.version += 1;
                self.selected = self.rows().iter().position(|row| *row.root == root).unwrap_or(at);
            }
            _ => {}
        }
    }

    /// Moves the cursor to the first match at or after `from`, going
    /// round the end of the list.
    fn seek(&mut self, from: usize) {
        if let Some((level, shown)) = self.sifted() {
            self.selected = level.next(&shown, from).unwrap_or(self.selected);
            return;
        }
        let rows = self.rows();
        let Some(matches) = self.matching(&rows).filter(|m| !m.is_empty()) else { return };
        let n = rows.len();
        let found = (0..n).map(|k| (from + k) % n).find(|i| matches.contains(&rows[*i].msg.envelope.id));
        self.selected = found.unwrap_or(self.selected);
    }

    fn filter_key(&mut self, mut editor: Editor, key: Key) {
        match keymap::find(&["command"], &[key]) {
            Found::Bound(binding, i) => match (binding.id, i) {
                ("command.cancel", _) => {
                    self.filter = None;
                    return;
                }
                ("command.run", _) => {
                    self.filter = None;
                    return self.find(editor.text.trim());
                }
                ("command.erase", 0) => editor.delete_word(),
                ("command.erase", _) => editor.clear(),
                _ => {}
            },
            _ => match key {
                Key::Char(c) => editor.push(c),
                Key::Backspace => editor.backspace(),
                _ => {}
            },
        }
        self.filter = Some(editor.text.clone());
        self.seek(self.selected);
        self.mode = Mode::Filter(editor);
    }

    fn command_key(&mut self, mut editor: Editor, key: Key) {
        match keymap::find(&["command"], &[key]) {
            Found::Bound(binding, i) => match (binding.id, i) {
                ("command.cancel", _) => return,
                ("command.run", _) => {
                    self.history.push(editor.text.clone());
                    return self.execute(&editor.text);
                }
                ("command.complete", i) => self.complete(&mut editor, i == 0),
                ("command.history", i) => editor.walk(&self.history, i == 0),
                ("command.erase", 0) => editor.delete_word(),
                _ => editor.clear(),
            },
            _ => match key {
                Key::Char(c) => editor.push(c),
                // Backspace on an empty line leaves it, as it does in vi.
                Key::Backspace if editor.text.is_empty() => return,
                Key::Backspace => editor.backspace(),
                _ => {}
            },
        }
        self.mode = Mode::Command(editor);
    }

    /// The accounts, in the config's order: that of the identities, then
    /// any account that only its folders name.
    fn accounts(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        let of_identities = self.identities.iter().map(|identity| &identity.account);
        for account in of_identities.chain(self.mailbox.folders.iter().map(|folder| &folder.account)) {
            if !names.contains(&account.0.as_str()) {
                names.push(&account.0);
            }
        }
        names
    }

    /// The colour an account's name is drawn in.
    pub(super) fn colour_of(&self, account: &AccountId) -> Tok {
        let accounts: Vec<AccountId> = self.accounts().into_iter().map(|name| AccountId(name.to_string())).collect();
        let colours = list::account_colours(&self.order, &accounts, &self.fixed);
        accounts.iter().position(|a| a == account).and_then(|at| colours.get(at).copied()).unwrap_or(Tok::Fg)
    }

    /// Tab on the command line: the commands while the first word is
    /// typed, and after it the names that command takes.
    fn complete(&self, editor: &mut Editor, forward: bool) {
        if editor.menu.is_some() {
            return editor.complete("", Vec::new(), forward);
        }
        fn plain(names: Vec<&str>) -> Vec<(&str, Line)> {
            names.into_iter().map(command::plain).collect()
        }
        let rows = self.rows();
        let on = rows.get(self.selected.min(rows.len().saturating_sub(1))).map(|row| &row.msg.envelope);
        let links = self.link_choices();
        let (title, choices) = match editor.text.split_once(' ').map(|(name, _)| name) {
            None => ("Commands".to_string(), COMMANDS.iter().map(command::choice).collect()),
            Some("move") => {
                let Some(on) = on else { return };
                let folders = self.mailbox.folders.iter().filter(|f| f.account == on.id.account && f.id != on.folder);
                let drawn = |f: &tuimex_core::Folder| {
                    vec![
                        run(format!("{:<12}", f.name), Tok::Fg),
                        run(format!("{:>6}", list::thousands(f.total)), Tok::Dim),
                    ]
                };
                let choices = folders.map(|f| (f.name.as_str(), drawn(f))).collect();
                (format!("Folders in {}", on.id.account.0), choices)
            }
            Some("section") => {
                ("Sections".to_string(), plain(self.mailbox.sections.iter().map(|s| s.name.as_str()).collect()))
            }
            Some("account" | "empty-trash") => ("Accounts".to_string(), plain(self.accounts())),
            Some("preview") => ("Preview".to_string(), plain(vec!["auto", "right", "below", "off"])),
            Some("calendar") => ("Views".to_string(), plain(vec!["month", "week", "day"])),
            Some("notify") => ("Reminders".to_string(), plain(vec!["on", "off"])),
            Some("invite") => {
                ("Answers".to_string(), plain(vec!["accept", "tentative", "decline", "propose", "ignore"]))
            }
            Some("agent") if self.agent => match editor.text["agent ".len()..].split_once(' ') {
                None => {
                    let agent = COMMANDS.iter().filter(|c| c.name == "agent");
                    (":agent".to_string(), agent.flat_map(|c| c.choices).map(command::choice).collect())
                }
                Some(("link", _)) => {
                    (":agent link".to_string(), links.iter().map(|(n, drawn)| (n.as_str(), drawn.clone())).collect())
                }
                Some(_) => return,
            },
            Some(_) => return,
        };
        let items = command::starting(choices, editor.word());
        editor.complete(&title, items, forward);
    }

    /// Runs a line of the command line (PLAN §6.3).
    fn execute(&mut self, line: &str) {
        let (name, rest) = command::split(line);
        let takes_rows = ["archive", "trash", "flag", "read", "unread"].contains(&name);
        // The list is out of sight while a draft is open, so nothing acts
        // on its rows from here.
        let writing = self.compose.is_some() && !["", "send", "agent", "messages"].contains(&name);
        match name {
            "" => {}
            _ if !writing && self.calendar_command(name, rest) => {}
            _ if writing => self.say(say::err(format!(":{name}: a draft is open")).then("Esc closes it")),
            "send" => self.send_command(),
            "agent" => self.agent_command(rest),
            "compose" if rest.is_empty() => self.compose(Start::New),
            "compose" => self.compose(Start::MailTo(rest.to_string())),
            "reply" => self.compose(Start::Reply),
            "replyall" => self.compose(Start::ReplyAll),
            "forward" => self.compose(Start::Forward),
            _ if takes_rows && !rest.is_empty() => {
                self.say(
                    say::err(format!(":{name}: a target cannot be named")).then("it takes the selected or marked rows"),
                );
            }
            "archive" => self.act(Action::Archive, false),
            "trash" => self.act(Action::Trash, false),
            "flag" => self.act(Action::Flag(true), true),
            "read" => self.act(Action::Seen(true), false),
            "unread" => self.act(Action::Seen(false), false),
            "move" if rest.is_empty() => self.offer(say::err(":move: name a folder"), "move "),
            "move" => self.act(Action::Move(tuimex_core::FolderId(rest.to_string())), false),
            "section" => {
                if !self.section(rest) {
                    self.offer(say::err(format!(":section: no section \"{rest}\"")), "section ");
                }
            }
            "account" if rest.is_empty() => self.narrow(None),
            "account" if self.accounts().contains(&rest) => self.narrow(Some(rest.to_string())),
            "account" => self.offer(say::err(format!(":account: no account \"{rest}\"")), "account "),
            "preview" => match Preview::from_name(rest) {
                Some(preview) => self.set_preview(preview),
                None => self.offer(say::err(":preview: it takes auto, right, below or off"), "preview "),
            },
            "sync" => self.sync(rest == "all"),
            "empty-trash" => self.ask_empty_trash(rest),
            "messages" => self.mode = Mode::Messages(0),
            "q" if self.reading.id().is_some() => self.close_reader(),
            "q" | "qa" => self.quit = true,
            "link" | "save" | "open" | "browser" | "unsubscribe" | "invite" => self.reader_command(name, rest),
            _ => match command::nearest(name) {
                Some(near) => {
                    let said = say::err(format!("not a command: :{name}"));
                    self.say(said.key(format!("did you mean :{near}? Tab to fix")));
                    self.fix = Some(format!("{near} {rest}").trim_end().to_string());
                }
                None => self.say(say::err(format!("not a command: :{name}"))),
            },
        }
    }

    /// Opens the section of that name or short name, whatever its case,
    /// and says whether there is one: `:section`, and `--section` at start.
    pub fn section(&mut self, name: &str) -> bool {
        let named = |tab: &usize| self.tab_name(*tab).eq_ignore_ascii_case(name);
        let short = |tab: &usize| self.mailbox.sections.get(*tab).is_some_and(|s| s.short.eq_ignore_ascii_case(name));
        let found = (0..self.tab_count()).find(|tab| named(tab) || short(tab));
        found.inspect(|tab| self.switch(*tab)).is_some()
    }

    /// An error whose next key is Tab, which puts `fix` on the command line
    /// with the choices open.
    fn offer(&mut self, said: Said, fix: &str) {
        self.say(said.key("Tab completes"));
        self.fix = Some(fix.to_string());
    }
}

#[cfg(test)]
mod tests;

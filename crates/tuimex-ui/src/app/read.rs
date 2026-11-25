//! The reader's state and keys: which message is open, how far it is
//! scrolled, and the list of links or files over it. Nothing here opens
//! anything: a link, a file and the browser copy are each an `Ask`
//! (ADR-001), made only once the user has named that one thing (ADR-008).

use std::collections::HashMap;
use std::path::PathBuf;

use tuimex_core::{
    Action, Attachment, Body, Check, Envelope, Event, Link, Listed, MessageId, Start, Unsubscribe, thread,
};

use super::{App, Mode};
use crate::calendar::Invited;
use crate::editor::Editor;
use crate::glyphs::{AGENT, DOT, NOTICE};
use crate::keymap::{self, Found, Key};
use crate::layout::MARGIN;
use crate::list;
use crate::palette::Tok;
use crate::reader::{self, Look, Placed, Reader};
use crate::say;
use crate::span::{Line, cells, gap, run};

#[derive(Debug, Default, PartialEq, Eq)]
enum Panel {
    #[default]
    None,
    /// The selected entry, counted from 0.
    Links(usize),
    Files(usize),
    /// `i` was pressed, and the next key answers the invitation.
    Invite,
}

#[derive(Debug)]
enum Advice {
    Asking,
    Given { advice: String, why: String },
    Failed(String),
}

/// What the agent was asked of a link (ADR-031): the serial of the ask and
/// what has come back for it, and what the agent saw placeholders for.
#[derive(Debug, Default)]
pub(super) struct Advised {
    advice: Option<(u64, Advice)>,
    replaced: Option<String>,
}

impl Advised {
    pub(super) fn asking(serial: u64) -> Advised {
        Advised { advice: Some((serial, Advice::Asking)), replaced: None }
    }

    /// Takes what came back for the ask, and hands back anything else.
    pub(super) fn told(&mut self, event: Event) -> Option<Event> {
        let asked = self.advice.as_ref().map(|(serial, _)| *serial);
        match event {
            Event::AgentAsked { serial, replaced } if asked == Some(serial) => self.replaced = Some(replaced),
            Event::AgentAdvice { serial, advice, why } if asked == Some(serial) => {
                self.advice = Some((serial, Advice::Given { advice, why }));
            }
            Event::ActionFailed { serial, reason, .. } if asked == Some(serial) => {
                self.advice = Some((serial, Advice::Failed(reason)));
            }
            other => return Some(other),
        }
        None
    }

    /// The rows under the list of links: that the agent is being asked,
    /// what it advises, or why it could not.
    pub(super) fn lines(&self) -> Vec<Line> {
        let told = |line: Line| [vec![gap(4)], line].concat();
        match &self.advice {
            Some((_, Advice::Asking)) => {
                let why = format!(" {DOT} it reads the address and the message, and opens nothing");
                vec![vec![], told(vec![run(format!("{AGENT} Asking the agent"), Tok::Agent), run(why, Tok::Dim)])]
            }
            Some((_, Advice::Given { advice, why })) => vec![
                vec![],
                told(vec![
                    run(format!("{AGENT} Agent advises {advice}"), Tok::Agent),
                    run(format!(" {DOT} {why}"), Tok::Dim),
                ]),
                told(vec![run("Advice only. The agent did not open the link.", Tok::Dim)]),
            ],
            Some((_, Advice::Failed(reason))) => {
                let nothing = run(format!(" {DOT} nothing changed"), Tok::Dim);
                vec![vec![], told(vec![run(format!("E :agent link: {reason}"), Tok::Err), nothing])]
            }
            None => Vec::new(),
        }
    }

    /// The right of the status line while the list is open: what the
    /// agent was not shown.
    pub(super) fn right(&self) -> Option<Line> {
        let what = self.replaced.as_ref()?;
        Some(vec![run("agent saw placeholders for", Tok::Dim), run(format!(" {what}"), Tok::Fg)])
    }
}

#[derive(Debug)]
struct Open {
    id: MessageId,
    /// The thread it was opened in, in the order `n` and `b` walk.
    thread: Vec<MessageId>,
    top: usize,
    panel: Panel,
    advised: Advised,
}

/// What the reader holds: the message open, and everything read so far.
#[derive(Debug, Default)]
pub(super) struct Reading {
    open: Option<Open>,
    bodies: HashMap<MessageId, Body>,
    checks: HashMap<MessageId, Check>,
    /// The serials of what was asked for outside the mailbox, so that a
    /// refusal of one is said and a refusal of nothing known is not.
    asked: Vec<u64>,
    /// A message on a row whose body was asked for because something
    /// needs it whole, and what: it is done when the body is here.
    wanted: Option<(MessageId, Wanted)>,
    /// The messages whose bodies were asked for to preview them and have
    /// not come, and those that could not be read.
    fetching: Vec<MessageId>,
    failed: Vec<MessageId>,
    /// The event each invitation's mail carries, where whoever read the
    /// message could read its calendar part whole: what an answer from
    /// the reader is given to.
    invited: HashMap<MessageId, Invited>,
    /// How far the body could scroll when last drawn.
    last_top: usize,
    /// Where the pictures of the last frame are.
    pub(super) pictures: Vec<Placed>,
}

/// How many read messages are kept.
const KEPT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wanted {
    /// `B` on a row: the browser copy.
    Copy,
    /// Enter on a draft: compose, on the draft as it was saved.
    Draft,
}

impl Reading {
    /// What `gpg` made of a message, once it has been read.
    pub(super) fn check(&self, id: &MessageId) -> Option<&Check> {
        self.checks.get(id)
    }

    pub(super) fn id(&self) -> Option<&MessageId> {
        self.open.as_ref().map(|open| &open.id)
    }
}

/// The question a link is opened on: what is wrong with it, when
/// something is, and always the host it really goes to.
/// One that deceives offers the agent's reading of it as a third key,
/// when `agent` says there is one to ask and it has not been asked.
pub(super) fn question(look: &Look, agent: bool) -> say::Question {
    match &look.warning {
        Some(warning) => {
            let mut said = warning.clone();
            if let Some(first) = said.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            let text = run(format!(" {said}; opens {}. Open?", look.host), Tok::Fg);
            let keys = [&say::YES_NO[..], &[(keymap::hint("global.agent").0, "ask the agent")]].concat();
            say::asked(vec![run(NOTICE, Tok::Warn), text], &keys[..if agent { 3 } else { 2 }])
        }
        None => say::ask(format!("Open {}?", look.host)),
    }
}

/// What `B` asks of a message whose pictures are on a server: how many,
/// and from which hosts. `room` is the cells the names may take: as many
/// as fit, then how many more.
fn load_images(body: &Body, room: usize) -> say::Question {
    let (hosts, images) = (&body.image_hosts, say::count(body.loadable_images, "image"));
    let all = hosts.join(", ");
    let first = format!("{} and {} more", hosts.first().map_or("", String::as_str), hosts.len().saturating_sub(1));
    // A host that is the only one is always named, cut or not.
    let named = match hosts.len() {
        n if n == 1 || all.chars().count() <= room => all,
        _ if first.chars().count() <= room => first,
        n => say::count(n, "host"),
    };
    say::ask(format!("Load {images} from {named}?"))
}

/// The address the sender's name first wrote from in the thread, when the
/// message `id` comes under that name from another (PLAN §6.5). The first
/// and not the latest: one forged reply does not make the next real one
/// the odd one out.
fn sender_was(mail: &[Listed], thread: &[MessageId], id: &MessageId) -> Option<String> {
    let of = |id: &MessageId| mail.iter().find(|m| m.envelope.id == *id);
    let sender = |m: &Listed| {
        let from = m.envelope.from.first()?;
        let name = from.name.as_deref().map(|name| name.trim().to_lowercase()).filter(|name| !name.is_empty())?;
        Some((name, from.email.clone()))
    };
    let this = of(id)?;
    let (name, email) = sender(this)?;
    let earlier = thread.iter().filter(|other| *other != id).filter_map(of);
    let received = earlier.filter(|m| m.envelope.date <= this.envelope.date && !m.envelope.flags.draft);
    let same = received.filter_map(|m| Some((m.envelope.date, sender(m)?))).filter(|(_, (other, _))| *other == name);
    let (_, (_, first)) = same.min_by_key(|(date, _)| *date)?;
    (!first.eq_ignore_ascii_case(&email)).then_some(first)
}

impl App {
    /// Where the pictures of the frame last drawn are, and the message
    /// they belong to, for a terminal that can draw them.
    pub fn pictures(&self) -> Option<(&MessageId, &[Placed])> {
        self.reading.id().map(|id| (id, self.reading.pictures.as_slice()))
    }

    /// The event the invitation in the message `id` is for, with the zones
    /// its calendar part defines, from whoever read the message; `None`
    /// when it has none or it could not be read. Without it the reader's
    /// card shows the invitation and cannot answer it.
    pub fn set_invited(&mut self, id: MessageId, invited: Option<Invited>) {
        // Bounded as the bodies are: past that many, only those whose
        // bodies are kept stay. It may come before its body or after.
        let reading = &mut self.reading;
        reading.invited.remove(&id);
        if reading.invited.len() >= KEPT {
            reading.invited.retain(|read, _| reading.bodies.contains_key(read));
        }
        reading.invited.extend(invited.map(|invited| (id, invited)));
    }

    pub(super) fn invited(&self, id: &MessageId) -> Option<&Invited> {
        self.reading.invited.get(id)
    }

    /// Hands on something asked for outside the mailbox and the calendars,
    /// and remembers its serial, so that a refusal of it is said.
    pub(super) fn outside(&mut self, action: Action) {
        let serial = self.ask(action, Vec::new());
        self.reading.asked.push(serial);
    }

    pub(super) fn read_body(&self) -> Option<(&MessageId, &Body)> {
        let id = self.reading.id()?;
        self.reading.bodies.get(id).map(|body| (id, body))
    }

    fn files(&self) -> Vec<Attachment> {
        let listed = || self.reading.id().and_then(|id| self.mailbox.mail.iter().find(|m| m.envelope.id == *id));
        self.read_body()
            .map(|(_, b)| b.attachments.clone())
            .or_else(|| listed().map(|m| m.attachments.clone()))
            .unwrap_or_default()
    }

    /// Opens `id` in the reader, asking for its body the first time and
    /// marking it read. `thread` is kept when `n` or `b` moved within one.
    /// A draft is compose's to open, so `n` and `b` walk past the drafts
    /// that answer a thread.
    pub(super) fn read(&mut self, id: MessageId, thread_kept: Option<Vec<MessageId>>) {
        let members = thread_kept.unwrap_or_else(|| {
            let account: Vec<Envelope> = self
                .mailbox
                .mail
                .iter()
                .filter(|m| m.envelope.id.account == id.account && (!m.envelope.flags.draft || m.envelope.id == id))
                .map(|m| m.envelope.clone())
                .collect();
            let found = thread(&account).into_iter().find(|t| t.members.iter().any(|n| n.id == id));
            found.map_or_else(|| vec![id.clone()], |t| t.members.into_iter().map(|n| n.id).collect())
        });
        if !self.reading.bodies.contains_key(&id) {
            self.ask(Action::Read, vec![id.clone()]);
        }
        if let Some(was) = sender_was(&self.mailbox.mail, &members, &id)
            && let Some(m) = self.message(&id)
        {
            m.sender_was = Some(was);
        }
        if self.message(&id).is_some_and(|m| !std::mem::replace(&mut m.envelope.flags.seen, true)) {
            self.ask(Action::Seen(true), vec![id.clone()]);
        }
        self.reading.open = Some(Open { id, thread: members, top: 0, panel: Panel::None, advised: Advised::default() });
    }

    /// The message a key pressed now is about: the one open in the
    /// reader, or else the selected row's, which for a thread is its
    /// newest.
    fn target(&self) -> Option<MessageId> {
        let on = || self.row(self.selected.min(self.count().saturating_sub(1))).map(|row| row.msg.envelope.id.clone());
        self.reading.id().cloned().or_else(on)
    }

    fn target_body(&self) -> Option<&Body> {
        self.reading.bodies.get(&self.target()?)
    }

    pub(super) fn body_of(&self, id: &MessageId) -> Option<&Body> {
        self.reading.bodies.get(id)
    }

    /// Whether the preview of that message waits for a body: none has been
    /// read, and none has failed to be.
    pub(super) fn arriving(&self, id: &MessageId) -> bool {
        !self.reading.bodies.contains_key(id) && !self.reading.failed.contains(id)
    }

    /// The message the preview shows, when the preview is in sight and its
    /// body has neither been read nor asked for.
    fn unread_preview(&self) -> Option<&MessageId> {
        let listed = matches!(self.mode, Mode::Normal) && self.compose.is_none() && self.reading.open.is_none();
        let id = self.pane_of.as_ref().filter(|_| listed && self.pane.0 != list::Place::Off)?;
        (self.arriving(id) && !self.reading.fetching.contains(id)).then_some(id)
    }

    /// Whether `rest` has a body to ask for.
    pub fn restless(&self) -> bool {
        self.unread_preview().is_some()
    }

    /// The cursor has rested on a row: the message its preview shows is
    /// asked for whole. Whoever feeds the keys calls this once none has
    /// come for a moment, so a key held down steps over rows and reads
    /// none of them.
    pub fn rest(&mut self) {
        if let Some(id) = self.unread_preview().cloned() {
            self.ask(Action::Read, vec![id.clone()]);
            self.reading.fetching.push(id);
        }
    }

    /// The whole text of a message that has been read, for whoever would
    /// otherwise have a row's opening of it.
    pub(super) fn text_of(&self, id: &MessageId) -> Option<&str> {
        self.reading.bodies.get(id).map(|body| body.text.as_str())
    }

    /// Asks for the selected row's message and does `what` when it is
    /// here. A draft whose body is here opens at once. The browser copy is
    /// made by whoever read the message, of what they last read, so it is
    /// read again for it even when its text is here.
    fn with_body(&mut self, what: Wanted) {
        let Some(id) = self.target() else { return };
        let here = self.reading.bodies.contains_key(&id);
        if here && what == Wanted::Draft {
            return self.compose(Start::FromDraft);
        }
        if !here && what == Wanted::Copy {
            self.say(say::plain("Fetching this message").then("the browser copy opens when it is here"));
        }
        self.ask(Action::Read, vec![id.clone()]);
        self.reading.wanted = Some((id, what));
    }

    /// The question before the copy may load pictures, or with none to
    /// load the copy itself.
    fn copy(&mut self) {
        match self.target_body() {
            Some(body) if body.loadable_images > 0 => {
                self.mode = Mode::Confirm(load_images(body, usize::MAX), Action::Browser { remote: true })
            }
            _ => self.reader_confirmed(Action::Browser { remote: false }),
        }
    }

    /// Enter on a row. A message in the outbox is compose's to open, and
    /// so is a draft, as it was saved.
    pub(super) fn open_selected(&mut self) {
        let Some(row) = self.row(self.selected.min(self.count().saturating_sub(1))) else { return };
        let (id, sending, draft) =
            (row.msg.envelope.id.clone(), row.msg.sending.is_some(), row.msg.envelope.flags.draft);
        // A refused message has opened in compose by now, so what is left
        // of the outbox is waiting to go.
        if sending {
            self.say(say::err("open: it is waiting to go").key("u takes it back"))
        } else if draft {
            self.with_body(Wanted::Draft)
        } else {
            self.read(id, None)
        }
    }

    pub(super) fn close_reader(&mut self) {
        self.reading.open = None;
    }

    /// The reader's screen, when a message is open.
    pub(super) fn reader_lines(&mut self, width: usize, height: usize) -> Option<Vec<Line>> {
        let open = self.reading.open.as_ref()?;
        let Some(msg) = self.mailbox.mail.iter().find(|m| m.envelope.id == open.id) else {
            self.reading.open = None;
            return None;
        };
        let body = self.reading.bodies.get(&open.id);
        let fits = (height / 3).max(3);
        let (command, menu) = self.command_row(width, fits.min(13).saturating_sub(2).max(1));
        let window = |entries: Vec<Line>, at: usize| {
            let from = (at + 1).saturating_sub(fits);
            (entries.into_iter().skip(from).take(fits).collect::<Vec<_>>(), Some(at - from))
        };
        let keys = |ids: &[&str]| list::hint(&ids.iter().map(|id| keymap::hint(id)).collect::<Vec<_>>());
        let (mut panel, left, hint) = match (&open.panel, body) {
            (Panel::Links(at), Some(body)) => {
                let url = body.links.get(*at).map(|link| reader::look(link, &body.trackers).url);
                let (entries, at) = window(reader::links(body), *at);
                let opens = vec![run("opens", Tok::Dim), run(format!(" {}", url.unwrap_or_default()), Tok::Fg)];
                let hint = keys(&["links.number", "links.choose", "global.agent", "links.close"]);
                (list::panel("Links", entries, at, width), Some(opens), Some(hint))
            }
            (Panel::Files(at), _) => {
                let shown: Vec<usize> = body.map_or_else(Vec::new, |b| b.pictures.iter().map(|p| p.index).collect());
                let (entries, at) = window(reader::files(&self.files(), &shown), *at);
                let hint = keys(&["attachments.save", "attachments.open", "attachments.close"]);
                (list::panel("Files", entries, at, width), Some(Line::new()), Some(hint))
            }
            (Panel::Invite, _) => (Vec::new(), None, Some(say::answer().line(width))),
            _ => (Vec::new(), None, None),
        };
        panel.extend(open.advised.lines());
        let right = match open.panel {
            Panel::Links(_) => open.advised.right().unwrap_or_default(),
            _ => self.alerts(),
        };
        let place = open.thread.iter().position(|id| *id == open.id).map_or(1, |at| at + 1);
        let agent = self.agent_unsent();
        let drawn = reader::screen(
            &Reader {
                msg,
                body,
                check: self.reading.checks.get(&open.id),
                me: &self.mailbox.me,
                account: self.colour_of(&msg.envelope.id.account),
                place: (place, open.thread.len()),
                top: open.top,
                panel: if agent.is_some() {
                    [panel, menu].concat()
                } else if menu.is_empty() {
                    panel
                } else {
                    menu
                },
                left: agent.or_else(|| self.prompt_left()).or(left),
                right,
                command: command.or(hint),
            },
            width,
            height,
        );
        (self.room, self.reading.last_top, self.reading.pictures) = (drawn.room, drawn.last_top, drawn.pictures);
        if let Some(open) = &mut self.reading.open {
            open.top = drawn.top;
        }
        Some(drawn.lines)
    }

    /// A key while a message is open: the reader's own table, or that of
    /// the list over it.
    pub(super) fn reader_key(&mut self, key: Key) {
        let Some(open) = &self.reading.open else { return };
        let modes: &[&str] = match open.panel {
            Panel::None => &["reader", "global"],
            Panel::Links(_) => &["links", "global"],
            Panel::Files(_) => &["attachments", "global"],
            Panel::Invite => &["invite"],
        };
        self.pending.push(key);
        match keymap::find(modes, &self.pending) {
            Found::Prefix => return,
            Found::Bound(binding, i) => {
                self.pending.clear();
                self.reader_run(binding.id, i);
            }
            // Any key that is not an answer leaves the invitation's question.
            Found::Nothing => self.set_panel(Panel::None, modes == ["invite"]),
        }
        self.pending.clear();
    }

    fn set_panel(&mut self, panel: Panel, when: bool) {
        if let Some(open) = self.reading.open.as_mut().filter(|_| when) {
            (open.panel, open.advised) = (panel, Advised::default());
        }
    }

    fn reader_run(&mut self, id: &str, i: usize) {
        let files = self.files().len();
        let Some(open) = &mut self.reading.open else { return };
        let (half, last_top) = ((self.room / 2).max(1), self.reading.last_top);
        let links = self.reading.bodies.get(&open.id).map_or(0, |b| b.links.len());
        let invited = self.reading.bodies.get(&open.id).is_some_and(|b| b.invite.is_some());
        // The entry chosen in the list that is open, and the one a step
        // from it.
        let (listed, at) = match open.panel {
            Panel::Links(at) | Panel::Files(at) => (true, at),
            _ => (false, 0),
        };
        let step = |len: usize| if i == 0 { (at + 1).min(len.saturating_sub(1)) } else { at.saturating_sub(1) };
        match id {
            "reader.scroll" => {
                open.top = match i {
                    0 => (open.top + 1).min(last_top),
                    1 => open.top.saturating_sub(1),
                    2 => (open.top + half).min(last_top),
                    3 => open.top.saturating_sub(half),
                    4 => 0,
                    _ => last_top,
                }
            }
            "reader.thread" => {
                let here = open.thread.iter().position(|id| *id == open.id).unwrap_or(0);
                let next = if i == 0 { here + 1 } else { here.wrapping_sub(1) };
                match open.thread.get(next).cloned() {
                    Some(id) => {
                        let thread = open.thread.clone();
                        self.read(id, Some(thread));
                    }
                    None => {
                        let which = if i == 0 { "later" } else { "earlier" };
                        self.say(say::err(format!("no {which} message in the thread")));
                    }
                }
            }
            "reader.list" => {
                let rows = self.rows();
                let here = self.selected.min(rows.len().saturating_sub(1));
                let next = if i == 0 { here + 1 } else { here.wrapping_sub(1) };
                let Some(id) = rows.get(next).map(|row| row.msg.envelope.id.clone()) else { return };
                drop(rows);
                self.selected = next;
                self.read(id, None);
            }
            "reader.links" if links > 0 => open.panel = Panel::Links(0),
            "reader.links" => self.say(say::err("o: this message has no links")),
            "reader.attachments" if files > 0 => open.panel = Panel::Files(0),
            "reader.attachments" => self.say(say::err("a: this message has no files")),
            "reader.invite" if invited => open.panel = Panel::Invite,
            "reader.invite" => self.say(say::err("i: this message is not an invitation")),
            "reader.browser" => self.browser(),
            "reader.actions" => match i {
                0 => self.act(Action::Archive, false),
                1 => self.act(Action::Trash, false),
                2 => self.act(Action::Flag(true), true),
                3 => self.act(Action::Seen(true), true),
                4 => self.mode = Mode::Command(Editor::with("move ")),
                5 => self.compose(Start::Reply),
                6 => self.compose(Start::ReplyAll),
                _ => self.compose(Start::Forward),
            },
            "links.close" | "attachments.close" => self.set_panel(Panel::None, true),
            "global.back" if listed => self.set_panel(Panel::None, true),
            "global.back" => self.close_reader(),
            "links.number" => self.link(i + 1),
            "links.choose" if i < 2 => open.panel = Panel::Links(step(links)),
            "links.choose" => self.link(at + 1),
            "attachments.choose" => open.panel = Panel::Files(step(files)),
            "attachments.save" => self.save(at + 1, ""),
            "attachments.open" => self.open_file(at + 1),
            "invite.accept" | "invite.tentative" | "invite.decline" | "invite.propose" | "invite.ignore" => {
                open.panel = Panel::None;
                self.answer_run(keymap::hint("reader.invite").0, id, true);
            }
            "global.command" => self.mode = Mode::Command(Editor::default()),
            "global.help" => self.mode = Mode::Help(String::new(), 0),
            "global.undo" => self.undo(),
            "global.agent" => self.agent_menu(),
            "global.redraw" => self.redraw = true,
            "global.calendar" => self.calendar_open(None),
            _ => {}
        }
    }

    /// The number of the link selected in the list of links, when that
    /// list is open.
    pub(super) fn link_selected(&self) -> Option<usize> {
        match self.reading.open.as_ref()?.panel {
            Panel::Links(at) => Some(at + 1),
            _ => None,
        }
    }

    /// The link numbered `n` and what is known of it, or why there is none.
    fn link_numbered(&mut self, command: &str, n: usize) -> Option<(Link, Look)> {
        let found = self.read_body().and_then(|(_, body)| {
            let link = body.links.iter().find(|link| link.n == n)?;
            Some((link.clone(), reader::look(link, &body.trackers)))
        });
        match found {
            Some((_, look)) if look.blocked => {
                let kind = match look.scheme.as_str() {
                    "" => "a link to no host is".to_string(),
                    scheme => format!("{scheme} links are"),
                };
                self.say(say::err(format!("{command} {n}: {kind} not opened")).then("only http, https and mailto"));
                None
            }
            None => {
                self.say(say::err(format!("{command}: this message has no link {n}")).key("o lists them"));
                None
            }
            found => found,
        }
    }

    /// Opens link `n` (ADR-018): refused when it is neither web nor mail,
    /// confirmed first when anything about it deceives, and otherwise
    /// opened with its real host said.
    fn link(&mut self, n: usize) {
        let Some((link, look)) = self.link_numbered(":link", n) else { return };
        if look.mailto {
            // Written for review, never sent from here (ADR-008).
            self.set_panel(Panel::None, true);
            return self.compose(Start::MailTo(link.url));
        }
        if look.warning.is_some() {
            // The list stays under the question, so the real host is on
            // the screen while it is answered.
            self.set_panel(Panel::Links(n - 1), true);
            self.mode = Mode::Confirm(question(&look, self.agent), Action::OpenLink(n));
        } else {
            self.set_panel(Panel::None, true);
            self.reader_confirmed(Action::OpenLink(n));
        }
    }

    /// `:agent link <n>`, and `a` over a link (ADR-031): the question is
    /// there at once, and the agent's advice joins it when it comes.
    fn agent_link(&mut self, n: usize) {
        let Some((_, look)) = self.link_numbered(":agent link", n) else { return };
        let Some(id) = self.reading.id().cloned() else { return };
        let serial = self.ask(Action::AgentLink(n), vec![id]);
        self.set_panel(Panel::Links(n - 1), true);
        if let Some(open) = &mut self.reading.open {
            open.advised = Advised::asking(serial);
        }
        self.mode = Mode::Confirm(question(&look, false), Action::OpenLink(n));
    }

    /// What Tab offers after `:agent link `: the number of each link of
    /// the message open that can be asked about, with its real host.
    pub(super) fn link_choices(&self) -> Vec<(String, Line)> {
        let Some((_, body)) = self.read_body() else { return Vec::new() };
        let looks = body
            .links
            .iter()
            .map(|link| (link.n, reader::look(link, &body.trackers)))
            .filter(|(_, look)| !look.blocked);
        looks
            .map(|(n, look)| (n.to_string(), vec![run(format!("{n:<4}"), Tok::Link), run(look.host, Tok::Fg)]))
            .collect()
    }

    /// `B` and `:browser` (ADR-009), of the message open or on the
    /// selected row, whose body is fetched first when it is not here. The
    /// copy of a message whose pictures are on a server may load them only
    /// after a yes, asked each time.
    pub(super) fn browser(&mut self) {
        match self.reading.open.is_some() && self.target_body().is_some() {
            true => self.copy(),
            false => self.with_body(Wanted::Copy),
        }
    }

    /// The question `B` asks, worded for the width it is drawn at.
    pub(super) fn images_question(&self, width: usize) -> Option<Line> {
        let Mode::Confirm(_, Action::Browser { remote: true }) = &self.mode else { return None };
        let body = self.target_body()?;
        // With no room the hosts are only counted, and what is left of
        // that line is everything but their names.
        let counted = say::count(body.image_hosts.len(), "host").len();
        let around = cells(&load_images(body, 0).line(usize::MAX)).saturating_sub(counted);
        Some(load_images(body, width.saturating_sub(around + MARGIN)).line(width))
    }

    /// A key at a confirm that is not a yes. Two questions have a third
    /// way out; every other is left with nothing done.
    pub(super) fn reader_declined(&mut self, id: &str, i: usize, question: &say::Question, action: Action) {
        let offered = |id: &str| question.keys.iter().any(|(key, _)| key == keymap::hint(id).0);
        match (id, action) {
            ("global.agent", Action::OpenLink(n)) if offered(id) => self.agent_link(n),
            ("global.agent", Action::OpenEventLink(_, n)) if offered(id) => self.event_link_asked(n),
            // `n` declines the pictures and not the copy; Esc leaves both.
            ("confirm.no", Action::Browser { remote: true }) if i == 0 => {
                self.reader_confirmed(Action::Browser { remote: false })
            }
            _ => {}
        }
    }

    fn file_numbered(&mut self, command: &str, n: usize) -> Option<Attachment> {
        let file = n.checked_sub(1).and_then(|at| self.files().get(at).cloned());
        if file.is_none() {
            self.say(say::err(format!("{command}: this message has no file {n}")).key("a lists them"));
        }
        file
    }

    fn save(&mut self, n: usize, path: &str) {
        let Some(file) = self.file_numbered(":save", n) else { return };
        self.reader_confirmed(Action::SaveAttachment(file.index, PathBuf::from(path)));
    }

    /// Opening hands the file to another program, so it is asked first,
    /// with the type that program will be chosen by.
    fn open_file(&mut self, n: usize) {
        let Some(file) = self.file_numbered(":open", n) else { return };
        let question = format!("Open {} ({}) in the system's program for it?", file.name, file.mime);
        self.mode = Mode::Confirm(say::ask(question), Action::OpenAttachment(file.index));
    }

    /// Hands on something the user has now asked for outright, and says
    /// it is under way. `y` at a reader's confirm comes here too.
    pub(super) fn reader_confirmed(&mut self, action: Action) {
        let Some(id) = self.target() else { return };
        let name =
            |index: &usize| self.files().into_iter().find(|a| a.index == *index).map_or_else(String::new, |a| a.name);
        let host = |n: &usize| {
            let link = self
                .read_body()
                .and_then(|(_, b)| b.links.iter().find(|l| l.n == *n).map(|l| reader::look(l, &b.trackers)));
            link.map_or_else(String::new, |look| look.host)
        };
        let loads = || {
            let (images, hosts) = self.target_body().map_or((0, 0), |b| (b.loadable_images, b.image_hosts.len()));
            let load = if images == 1 { "loads" } else { "load" };
            format!("{} {load} from {}", say::count(images, "image"), say::count(hosts, "host"))
        };
        let said = match &action {
            Action::OpenLink(n) => say::plain(format!("Opening {}", host(n))),
            Action::SaveAttachment(index, _) => say::plain(format!("Saving {}", name(index))),
            Action::OpenAttachment(index) => say::plain(format!("Opening {}", name(index))),
            Action::Browser { remote } => say::plain("Opening the sealed copy in your browser").then(if *remote {
                loads()
            } else {
                "nothing remote loads".to_string()
            }),
            Action::Unsubscribe => say::plain("Opening the unsubscribe page"),
            _ => return,
        };
        let serial = self.ask(action, vec![id]);
        self.reading.asked.push(serial);
        self.say(said);
    }

    /// A confirm was answered, either way: the list it was asked over
    /// closes, and the agent's advice with it.
    pub(super) fn reader_answered(&mut self) {
        let over = |open: &&mut Open| !matches!(open.panel, Panel::Invite);
        if let Some(open) = self.reading.open.as_mut().filter(over) {
            (open.panel, open.advised) = (Panel::None, Advised::default());
        }
    }

    /// The commands that act on the message open (PLAN §6.3).
    pub(super) fn reader_command(&mut self, name: &str, rest: &str) {
        let (first, more) = rest.split_once(' ').map_or((rest, ""), |(first, more)| (first, more.trim()));
        let number = |text: &str| text.parse::<usize>().ok();
        if self.reading.open.is_none() {
            return self.say(say::err(format!(":{name}: no message is open")).key("Enter opens one"));
        }
        match (name, number(first)) {
            ("link", Some(n)) if more.is_empty() => self.link(n),
            ("save", Some(n)) => self.save(n, more),
            ("open", Some(n)) if more.is_empty() => self.open_file(n),
            ("agent", _) if first == "link" && number(more).is_some() => self.agent_link(number(more).unwrap_or(0)),
            ("link" | "save" | "open" | "agent", _) => {
                let takes = if name == "save" { "<n> [path]" } else { "<n>" };
                let name = if name == "agent" { "agent link" } else { name };
                self.say(say::err(format!(":{name}: give a number")).then(format!(":{name} {takes}")));
            }
            ("browser", _) => self.browser(),
            ("unsubscribe", _) => match self.read_body().and_then(|(_, b)| b.unsubscribe.clone()) {
                None => self.say(say::err(":unsubscribe: this message names no way to")),
                // PLAN §6.3: composed for review, never automatic.
                Some(Unsubscribe::Mailto(to)) => self.compose(Start::MailTo(to)),
                Some(Unsubscribe::Https(url)) => {
                    let host = tuimex_core::assess(&url, "").host;
                    self.mode = Mode::Confirm(say::ask(format!("Unsubscribe at {host}?")), Action::Unsubscribe);
                }
            },
            _ => self.answer_command(rest, true),
        }
    }

    /// What came back for the reader: a body, a verdict, advice, a result.
    pub(super) fn reader_told(&mut self, event: Event) {
        let event = match self.reading.open.as_mut() {
            Some(open) => match open.advised.told(event) {
                Some(other) => other,
                None => return,
            },
            None => event,
        };
        match event {
            Event::BodyReady { id, body } => {
                // The preview reads every row rested on: what is kept is
                // bounded, and what is on screen is kept first.
                if self.reading.bodies.len() >= KEPT {
                    let shown = [self.reading.id().cloned(), self.pane_of.clone()];
                    self.reading.bodies.retain(|read, _| shown.iter().flatten().any(|id| id == read));
                }
                self.reading.fetching.retain(|asked| *asked != id);
                self.reading.bodies.insert(id.clone(), *body);
                // What waited for it is done if the cursor has too.
                let waited = self.reading.wanted.take_if(|(wanted, _)| *wanted == id);
                let still = matches!(self.mode, Mode::Normal) && self.compose.is_none() && self.reading.open.is_none();
                match waited.filter(|_| still && self.target() == Some(id)) {
                    Some((_, Wanted::Copy)) => {
                        self.said = None;
                        self.copy();
                    }
                    Some((_, Wanted::Draft)) => self.compose(Start::FromDraft),
                    None => {}
                }
            }
            Event::BodyFailed { id, reason } => {
                self.reading.fetching.retain(|asked| *asked != id);
                self.reading.failed.push(id.clone());
                if self.reading.wanted.take_if(|(wanted, _)| *wanted == id).is_some() {
                    self.say(say::err(format!("the message could not be read: {reason}")));
                }
            }
            Event::Checked { id, check } => {
                self.reading.checks.insert(id, check);
            }
            Event::ActionDone { summary } => self.say(say::ok(summary)),
            Event::Saved { path } => self.say(say::saved(path)),
            // Something asked for outside the mailbox did not happen. The
            // reason names its command, as every error does.
            Event::ActionFailed { serial, reason, hint } if self.reading.asked.contains(&serial) => {
                let said = say::err(reason);
                self.say(if hint.is_empty() { said } else { said.key(hint) });
            }
            _ => {}
        }
    }
}

//! Actions: each takes effect on the mail in memory at once and is handed
//! on as an `Ask`; `u` and a refusal put back what it changed (ADR-014,
//! ADR-015).

use std::collections::{HashMap, HashSet};

use tuimex_core::{AccountId, Action, Ask, Event, Folder, FolderId, Listed, MessageId, Role, Sending, Timestamp};

use super::{App, Mode};
use crate::list::AccountSync;
use crate::say::{self, count};
use crate::when::span_of;

/// Something done that `u` can take back.
#[derive(Debug)]
pub(super) struct Change {
    serial: u64,
    action: Action,
    /// The messages as they were.
    before: Vec<Listed>,
    /// Where the cursor was, and in which tab.
    cursor: MessageId,
    tab: usize,
}

/// The command an action is, as an error names it.
fn name(action: &Action) -> &'static str {
    match action {
        Action::Archive => ":archive",
        Action::Trash => ":trash",
        Action::Move(_) => ":move",
        Action::Flag(_) => ":flag",
        Action::Seen(true) => ":read",
        Action::Seen(false) => ":unread",
        _ => "",
    }
}

impl App {
    pub(super) fn ask(&mut self, action: Action, on: Vec<MessageId>) -> u64 {
        self.serial += 1;
        self.unsettled.push((self.serial, on.clone()));
        self.asks.push(Ask { serial: self.serial, action, on });
        self.serial
    }

    pub(super) fn message(&mut self, id: &MessageId) -> Option<&mut Listed> {
        self.mail_mut().iter_mut().find(|m| m.envelope.id == *id)
    }

    /// Where the messages `ids` name are in the mail, in its order.
    fn places<'a>(&self, ids: impl ExactSizeIterator<Item = &'a MessageId> + Clone) -> Vec<usize> {
        let mail = &self.mailbox.mail;
        if ids.len() <= 4 {
            let mut places: Vec<usize> = ids.filter_map(|id| mail.iter().position(|m| m.envelope.id == *id)).collect();
            places.sort_unstable();
            places.dedup();
            return places;
        }
        let named: HashSet<&MessageId> = ids.collect();
        (0..mail.len()).filter(|&place| named.contains(&mail[place].envelope.id)).collect()
    }

    /// Where an action puts a message of `account`, or what to say when
    /// that account has no such folder.
    fn destination(&self, action: &Action, account: &AccountId) -> Result<FolderId, say::Said> {
        let role = |role, word| {
            let none = || say::err(format!("{}: no {word} folder in {}", name(action), account.0));
            self.mailbox.folder(account, role).map(|f| f.id.clone()).ok_or_else(none)
        };
        match action {
            Action::Archive => role(Role::Archive, "Archive"),
            Action::Trash => role(Role::Trash, "Trash"),
            Action::Move(to) => {
                let named = |f: &&tuimex_core::Folder| f.account == *account && f.name.eq_ignore_ascii_case(&to.0);
                let none = || say::err(format!(":move: no folder \"{}\" in {}", to.0, account.0)).key("Tab completes");
                self.mailbox.folders.iter().find(named).map(|f| f.id.clone()).ok_or_else(none)
            }
            _ => Err(say::err("not an action on a message")),
        }
    }

    /// Carries out `action` on the message open in the reader, else on the
    /// marked rows, or on the selected one: a folded thread whole, an
    /// unfolded one message by message (PLAN §6.3).
    /// With `toggle`, a flag or a read mark is set unless every message
    /// taken already has it.
    pub(super) fn act(&mut self, action: Action, toggle: bool) {
        let at = self.selected.min(self.count().saturating_sub(1));
        let batch = !self.marks.is_empty();
        // One row, with no message open, is the key a hand is held on: it
        // is found by its number, and every row before it stays.
        let (ids, cursor, above): (Vec<MessageId>, MessageId, usize) = if batch || self.reading.id().is_some() {
            let rows = self.rows();
            let Some(row) = rows.get(at) else {
                return self.say(say::err(format!("{}: nothing is selected", name(&action))));
            };
            let marked = rows.iter().filter(|r| self.marks.contains(&r.msg.envelope.id));
            let ids: Vec<MessageId> = match self.reading.id() {
                Some(open) => vec![open.clone()],
                None => marked.flat_map(|r| r.ids.iter().copied().cloned()).collect(),
            };
            let above = rows[..at].iter().filter(|r| !ids.contains(&r.msg.envelope.id)).count();
            (ids, row.msg.envelope.id.clone(), above)
        } else {
            let Some(row) = self.row(at) else {
                return self.say(say::err(format!("{}: nothing is selected", name(&action))));
            };
            (row.ids.iter().copied().cloned().collect(), row.msg.envelope.id.clone(), at)
        };
        // Where the cursor goes when its own row leaves: the row that moved
        // up into its place, or after a batch the row above that, as the
        // design's two flows draw it.
        let fallback = above.saturating_sub(usize::from(batch));

        let places = self.places(ids.iter());
        let taken = || places.iter().map(|&place| &self.mailbox.mail[place]);
        let action = match action {
            Action::Flag(_) if toggle => Action::Flag(!taken().all(|m| m.envelope.flags.flagged)),
            Action::Seen(_) if toggle => Action::Seen(taken().any(|m| !m.envelope.flags.seen)),
            other => other,
        };
        let mut after: Vec<Listed> = Vec::new();
        for was in taken() {
            let mut now = was.clone();
            match &action {
                Action::Flag(on) => now.envelope.flags.flagged = *on,
                Action::Seen(on) => now.envelope.flags.seen = *on,
                moving => match self.destination(moving, &was.envelope.id.account) {
                    Ok(folder) => (now.envelope.folder, now.sending) = (folder, None),
                    // PLAN §4.1: an Archive or a Trash is made on first
                    // use, after asking. Nothing has been changed yet.
                    Err(_) if matches!(action, Action::Archive | Action::Trash) => {
                        let account = was.envelope.id.account.clone();
                        let (word, create) = match action {
                            Action::Archive => ("Archive", Action::CreateArchive(account.clone())),
                            _ => ("Trash", Action::CreateTrash(account.clone())),
                        };
                        let question = format!("{} has no {word} folder. Create one on its server?", account.0);
                        self.mode = Mode::Confirm(say::ask(question), create);
                        return;
                    }
                    Err(said) => {
                        self.say(said);
                        self.fix = matches!(action, Action::Move(_)).then(|| "move ".to_string());
                        return;
                    }
                },
            }
            after.push(now);
        }
        let what = count(ids.len(), "message");
        let said = match &action {
            Action::Archive => say::ok(format!("Archived {what}")),
            Action::Trash => say::ok(format!("Trashed {what}")),
            Action::Move(_) => {
                let folder = after.first().and_then(|m| self.mailbox.folder_of(m)).map_or("", |f| &f.name);
                say::ok(format!("Moved {what} to {folder}"))
            }
            Action::Flag(true) => say::ok(format!("Flagged {what}")),
            Action::Flag(false) => say::ok(format!("Unflagged {what}")),
            Action::Seen(true) => say::ok(format!("Marked {what} read")),
            _ => say::ok(format!("Marked {what} unread")),
        };
        let mut before = Vec::new();
        for (&place, now) in places.iter().zip(after) {
            before.push(std::mem::replace(&mut self.mailbox.mail[place], now));
        }
        self.touched(&places);
        let serial = self.ask(action.clone(), ids);
        // A message that has left its folder has left the reader too.
        if !matches!(action, Action::Flag(_) | Action::Seen(_)) {
            self.close_reader();
        }
        self.undo.push(Change { serial, action, before, cursor: cursor.clone(), tab: self.tab() });
        self.marks.clear();
        self.anchor = None;
        self.selected = self.row_of(&cursor).unwrap_or(fallback);
        self.say(said.key("u to undo"));
    }

    /// Puts back what `change` changed and nothing else of each message,
    /// and the cursor where it was. Says what came back.
    fn revert(&mut self, change: &Change) -> String {
        let places = self.places(change.before.iter().map(|was| &was.envelope.id));
        for &place in &places {
            let m = &mut self.mailbox.mail[place];
            let Some(was) = change.before.iter().find(|was| was.envelope.id == m.envelope.id) else { continue };
            match change.action {
                Action::Flag(_) => m.envelope.flags.flagged = was.envelope.flags.flagged,
                Action::Seen(_) => m.envelope.flags.seen = was.envelope.flags.seen,
                _ => (m.envelope.folder, m.sending) = (was.envelope.folder.clone(), was.sending.clone()),
            }
        }
        self.touched(&places);
        if change.tab == self.tab() {
            self.selected = self.row_of(&change.cursor).unwrap_or(self.selected);
        }
        let what = count(change.before.len(), "message");
        match change.action {
            Action::Flag(true) => format!("{what} unflagged"),
            Action::Flag(false) => format!("{what} flagged again"),
            Action::Seen(true) => format!("{what} unread again"),
            Action::Seen(false) => format!("{what} read again"),
            _ => {
                let folder = change.before.first().and_then(|m| self.mailbox.folder_of(m));
                format!("{what} back in {}", folder.map_or("its folder", |f| &f.name))
            }
        }
    }

    /// `u`: a held message under the cursor goes back to Drafts; otherwise
    /// the last action is reversed.
    pub(super) fn undo(&mut self) {
        let row = self.row(self.selected);
        let held = row.filter(|r| matches!(r.msg.sending, Some(Sending::Held { .. })));
        if let Some(id) = held.map(|r| r.msg.envelope.id.clone()) {
            self.undo.retain(|c| c.action != Action::Send || c.cursor != id);
            return self.unsend(id);
        }
        // An answer given from the reader is taken back here, in its turn.
        if self.answered_in_mail() > self.undo.last().map_or(0, |change| change.serial) {
            return self.calendar_undo(true);
        }
        match self.undo.pop() {
            None => self.say(say::err("undo: nothing to undo")),
            Some(change) if change.action == Action::Send => self.unsend(change.cursor),
            Some(change) => {
                let back = self.revert(&change);
                self.ask(Action::Undo, change.before.iter().map(|m| m.envelope.id.clone()).collect());
                self.say(say::ok("Undone").then(back));
            }
        }
    }

    /// A message confirmed for sending waits in the outbox for `seconds`,
    /// during which `u` takes it back (ADR-015). The send summary calls
    /// this once it is confirmed.
    pub fn send(&mut self, mut draft: Listed, seconds: u32) {
        let id = draft.envelope.id.clone();
        let e = &draft.envelope;
        let to = count(e.to.len() + e.cc.len() + e.bcc.len(), "recipient");
        draft.sending = Some(Sending::Held { seconds });
        match self.message(&id) {
            Some(m) => *m = draft,
            None => self.mail_mut().push(draft),
        }
        let serial = self.ask(Action::Send, vec![id.clone()]);
        self.undo.push(Change { serial, action: Action::Send, before: Vec::new(), cursor: id, tab: self.tab() });
        self.say(say::plain(format!("Sending to {to} in {}", span_of(seconds))).key("u to stop"));
    }

    fn unsend(&mut self, id: MessageId) {
        let drafts = self.mailbox.folder(&id.account, Role::Drafts).map(|f| f.id.clone());
        if let Some(m) = self.message(&id) {
            // An account always has a Drafts folder; one that lists none
            // still gets the message out of the outbox.
            (m.envelope.folder, m.sending) = (drafts.unwrap_or_else(|| FolderId("Drafts".to_string())), None);
        }
        self.ask(Action::Undo, vec![id]);
        self.say(say::ok("Not sent").then("back in Drafts"));
    }

    /// What whoever carries the asks out has to tell.
    pub fn event(&mut self, event: Event) {
        let Some(event) = self.calendar_told(event) else { return };
        match event {
            Event::ActionFailed { serial, reason, hint } => {
                let Some(at) = self.undo.iter().position(|c| c.serial == serial) else {
                    return self.reader_told(Event::ActionFailed { serial, reason, hint });
                };
                let change = self.undo.remove(at);
                let back = self.revert(&change);
                let account = change.before.first().map_or("the server", |m| m.envelope.id.account.0.as_str());
                let said = say::err(format!("{}: {account} refused it", name(&change.action)));
                self.say(said.then(format!("{reason} {} {back}", crate::glyphs::DOT)));
            }
            Event::Sent { id, answered } => {
                for original in &answered {
                    if let Some(m) = self.message(original) {
                        m.envelope.flags.answered = true;
                    }
                }
                let sent = self.mailbox.folder(&id.account, Role::Sent).map(|f| f.id.clone());
                let Some(m) = self.message(&id) else { return };
                let to = count(m.envelope.to.len() + m.envelope.cc.len() + m.envelope.bcc.len(), "recipient");
                m.sending = None;
                m.envelope.folder = sent.unwrap_or_else(|| FolderId("Sent".to_string()));
                self.undo.retain(|c| c.action != Action::Send || c.cursor != id);
                self.say(say::ok(format!("Sent to {to}")));
            }
            Event::SendFailed { id, reason } => {
                if let Some(m) = self.message(&id) {
                    m.sending = Some(Sending::Failed { reason: reason.clone() });
                }
                // The status line counts what was not sent; the reason is
                // on the message's row and in `:messages`.
                self.note(self.mailbox.now, say::err(format!("send: {reason}")));
            }
            Event::Emptied { account, deleted, kept, .. } => {
                let said = say::ok(format!("Deleted {} from {}'s Trash", count(deleted, "message"), account.0));
                self.say(if kept == 0 { said } else { said.then(format!("{kept} kept, not yet on the server")) });
            }
            Event::Notice { error, text, hint, .. } => {
                let said = if error { say::err(text) } else { say::plain(text) };
                self.say(if hint.is_empty() { said } else { said.then(hint) });
            }
            // Sync, bodies and new mail change what the store holds, and
            // whoever reads the store hands over the mailbox again.
            // What a pass changed arrives through `refresh`; this is only
            // how the account stands with its server.
            Event::Synced { account, .. } => {
                self.mailbox.synced = Some(0);
                self.syncing.retain(|(a, _)| *a != account);
                self.synced_at.retain(|(a, _)| *a != account);
                self.synced_at.push((account, self.mailbox.now));
            }
            Event::SyncError { account, msg, retry_in } => {
                // The engine names the account first; the status line
                // does that itself.
                let reason = msg.strip_prefix(&format!("{}: ", account.0)).unwrap_or(&msg).to_string();
                let retry = retry_in.map(|wait| u32::try_from(wait.as_secs()).unwrap_or(u32::MAX));
                let last = self.synced_at.iter().find(|(a, _)| *a == account).map(|(_, at)| *at);
                self.note(self.mailbox.now, say::err(format!("sync: {msg}")));
                self.syncing.retain(|(a, _)| *a != account);
                self.retry_at.retain(|(a, _)| *a != account);
                let again = retry.map(|wait| Timestamp(self.mailbox.now.0 + i64::from(wait)));
                self.retry_at.extend(again.map(|at| (account.clone(), at)));
                self.syncing.push((account, AccountSync::Failing { reason, retry, last }));
            }
            Event::FirstSync { account, done, total, .. } => {
                self.syncing.retain(|(a, _)| *a != account);
                if done < total {
                    self.syncing.push((account, AccountSync::FirstSync { done, total }));
                }
            }
            Event::NewMail { .. } => {}
            Event::Found { serial, hits, searched } => self.found_event(serial, hits, searched),
            Event::Edited { text } if self.form_is_open() => self.form_edited(text),
            event @ (Event::Edited { .. } | Event::Paths { .. } | Event::Attached { .. } | Event::AgentText { .. }) => {
                self.compose_event(event);
            }
            told => self.reader_told(told),
        }
    }

    /// Takes what the store held of the messages `asked` names, or with
    /// none named of every message, once every ask up to the serial `after`
    /// had been carried out: `rows` are those of them that were still
    /// there, and the rest had gone. A message an ask since then names is
    /// left as it is shown, for the reading that follows that ask. The
    /// cursor stays on the message it was on. A message in an outbox keeps
    /// what is known here of its sending, and one that was opened what was
    /// found of its sender, neither of which the store holds.
    pub fn refresh(
        &mut self,
        now: Timestamp,
        after: u64,
        asked: Option<&[MessageId]>,
        mut rows: Vec<Listed>,
        folders: Vec<Folder>,
    ) {
        self.unsettled.retain(|(serial, _)| *serial > after);
        let newer: HashSet<MessageId> = self.unsettled.iter().flat_map(|(_, on)| on.iter().cloned()).collect();
        rows.retain(|m| !newer.contains(&m.envelope.id));
        let cursor = self.row(self.selected.min(self.count().saturating_sub(1))).map(|r| r.msg.envelope.id.clone());
        let outbox = |m: &Listed| {
            let role = |f: &Folder| f.account == m.envelope.id.account && f.id == m.envelope.folder;
            folders.iter().find(|f| role(f)).is_some_and(|f| f.role == Role::Outbox)
        };
        // What a section asks of a folder; how many messages one holds is
        // only said.
        let named = |f: &Folder| (f.account.clone(), f.id.clone(), f.name.clone(), f.role);
        let refiled = !self.mailbox.folders.iter().map(named).eq(folders.iter().map(named));
        let delay = self.delay;
        let fresh: HashSet<&MessageId> = rows.iter().map(|m| &m.envelope.id).collect();
        // Most of what is read again has not left: the answer to an
        // action, a flag set elsewhere, mail that arrived. Those are
        // changed where they are or added after the rest, every other
        // message stays in its place, and the threads of the tabs they
        // are not in stand.
        let gone = |id: &&MessageId| !newer.contains(*id) && !fresh.contains(*id);
        let lost: Vec<&MessageId> = asked.map_or_else(Vec::new, |asked| asked.iter().filter(gone).collect());
        if asked.is_some() && fresh.len() == rows.len() && self.places(lost.iter().copied()).is_empty() {
            let places = self.places(rows.iter().map(|m| &m.envelope.id));
            let at: HashMap<MessageId, usize> =
                places.iter().map(|&p| (self.mailbox.mail[p].envelope.id.clone(), p)).collect();
            drop(fresh);
            let shape = |m: &Listed| {
                let e = &m.envelope;
                (e.message_id.clone(), e.references.clone(), e.subject.clone(), e.date)
            };
            let was = self.of();
            let (mut changed, mut reshaped) = (Vec::new(), false);
            for mut m in rows {
                let Some(&place) = at.get(&m.envelope.id) else {
                    m.sending = outbox(&m).then_some(Sending::Held { seconds: delay });
                    changed.push(self.mailbox.mail.len());
                    self.mailbox.mail.push(m);
                    continue;
                };
                let was = &mut self.mailbox.mail[place];
                m.sending = outbox(&m).then(|| was.sending.clone().unwrap_or(Sending::Held { seconds: delay }));
                m.sender_was = was.sender_was.clone();
                if m != *was {
                    reshaped |= shape(&m) != shape(was);
                    *was = m;
                    changed.push(place);
                }
            }
            (self.mailbox.now, self.mailbox.folders) = (now, folders);
            if reshaped {
                self.threads.take();
            }
            if reshaped || refiled {
                self.version += 1;
            } else if !changed.is_empty() {
                self.settle(was, &changed);
            }
            if refiled || !changed.is_empty() {
                let last = self.count().saturating_sub(1);
                self.selected = cursor.and_then(|id| self.row_of(&id)).unwrap_or(self.selected).min(last);
            }
            return;
        }
        drop(fresh);
        let mail = self.mail_mut();
        let mut sending: HashMap<MessageId, Sending> =
            mail.iter().filter_map(|m| Some((m.envelope.id.clone(), m.sending.clone()?))).collect();
        let mut sender_was: HashMap<MessageId, String> =
            mail.iter().filter_map(|m| Some((m.envelope.id.clone(), m.sender_was.clone()?))).collect();
        let fresh: HashSet<&MessageId> = rows.iter().map(|m| &m.envelope.id).collect();
        match asked {
            Some(asked) => {
                let asked: HashSet<&MessageId> = asked.iter().collect();
                let stays = |id: &MessageId| newer.contains(id) || !(asked.contains(id) || fresh.contains(id));
                mail.retain(|m| stays(&m.envelope.id));
            }
            None => mail.retain(|m| newer.contains(&m.envelope.id)),
        }
        mail.extend(rows.into_iter().map(|mut m| {
            let held = || sending.remove(&m.envelope.id).unwrap_or(Sending::Held { seconds: delay });
            m.sending = outbox(&m).then(held);
            m.sender_was = sender_was.remove(&m.envelope.id);
            m
        }));
        (self.mailbox.now, self.mailbox.folders) = (now, folders);
        if !self.marks.is_empty() {
            let here: HashSet<&MessageId> = self.mailbox.mail.iter().map(|m| &m.envelope.id).collect();
            self.marks.retain(|id| here.contains(id));
        }
        let last = self.count().saturating_sub(1);
        self.selected = cursor.and_then(|id| self.row_of(&id)).unwrap_or(self.selected).min(last);
    }

    /// Moves the clock to `now`: the seconds a held message has left, the
    /// seconds until a failing account is tried again and the age of the
    /// last sync are counted from it. Says whether anything drawn changed,
    /// so that a second in which nothing did costs no frame.
    pub fn tick(&mut self, now: Timestamp) -> bool {
        let was = self.mailbox.now;
        if now == was {
            return false;
        }
        let reminded = self.reminding().map(|(_, lead)| lead);
        (self.mailbox.now, self.cal.model.now) = (now, now);
        let left = |until: Timestamp| u32::try_from((until.0 - now.0).max(0)).unwrap_or(u32::MAX);
        // A date moves to another group, and a clock to another minute.
        let mut changed = now.0.div_euclid(60) != was.0.div_euclid(60);
        changed |= self.reminding().map(|(_, lead)| lead) != reminded;
        if let Some(last) = self.synced_at.iter().map(|(_, at)| *at).min() {
            let age = u32::try_from((now.0 - last.0).max(0)).unwrap_or(u32::MAX);
            changed |= self.mailbox.synced.map(span_of) != Some(span_of(age));
            self.mailbox.synced = Some(age);
        }
        // Not through `mail_mut`: the seconds left decide no thread and no
        // match, so nothing kept of the mail is stale for them.
        for m in &mut self.mailbox.mail {
            let Some(Sending::Held { seconds }) = &mut m.sending else { continue };
            // A message first seen held has all of what it shows left.
            let due = self.due.entry(m.envelope.id.clone()).or_insert(Timestamp(was.0 + i64::from(*seconds)));
            changed |= std::mem::replace(seconds, left(*due)) != left(*due);
        }
        let waiting = self.mailbox.mail.iter().filter(|m| matches!(m.sending, Some(Sending::Held { .. })));
        let held: HashSet<&MessageId> = waiting.map(|m| &m.envelope.id).collect();
        self.due.retain(|id, _| held.contains(id));
        for (account, state) in &mut self.syncing {
            let at = self.retry_at.iter().find(|(a, _)| a == account).map(|(_, at)| *at);
            if let (AccountSync::Failing { retry: Some(seconds), .. }, Some(at)) = (state, at) {
                changed |= std::mem::replace(seconds, left(at)) != left(at);
            }
        }
        changed
    }

    pub(super) fn sync(&mut self, all: bool) {
        self.ask(Action::Sync { all }, Vec::new());
        let what = if all { "every account".to_string() } else { self.tab_name(self.tab()).to_string() };
        self.say(say::plain(format!("Syncing {what}")));
    }

    /// `:empty-trash [account]`: the one action that is confirmed, because
    /// it is the one `u` cannot reverse.
    pub(super) fn ask_empty_trash(&mut self, named: &str) {
        let rows = self.rows();
        let here = rows.get(self.selected).map(|r| r.msg.envelope.id.account.clone());
        let account = if named.is_empty() { here } else { Some(AccountId(named.to_string())) };
        let trash = account.as_ref().and_then(|a| self.mailbox.folder(a, Role::Trash));
        let (Some(account), Some(trash)) = (account, trash) else {
            drop(rows);
            return self.offer(say::err(format!(":empty-trash: no account \"{named}\"")), "empty-trash ");
        };
        let messages = count(trash.total as usize, "message");
        let question = say::ask(format!("Delete {messages} in {}'s Trash for good?", account.0));
        drop(rows);
        self.mode = Mode::Confirm(question, Action::EmptyTrash(account));
    }

    /// `y` at a confirm.
    pub(super) fn confirmed(&mut self, action: Action) {
        if matches!(action, Action::OpenEventLink(..)) {
            return self.event_link_confirmed(action);
        }
        if let Action::CreateArchive(account) | Action::CreateTrash(account) = &action {
            // Shown at once, as everything is (ADR-015); the store's own
            // folder takes its place when it is next read. Then what was
            // asked for before the question: the rows are as they were.
            let (role, name, then) = match action {
                Action::CreateArchive(_) => (Role::Archive, "Archive", Action::Archive),
                _ => (Role::Trash, "Trash", Action::Trash),
            };
            let (id, account, name) = (FolderId(name.to_string()), account.clone(), name.to_string());
            self.mailbox.folders.push(Folder { id, account, name, role, unseen: 0, total: 0 });
            self.ask(action, Vec::new());
            return self.act(then, false);
        }
        let Action::EmptyTrash(account) = &action else { return self.reader_confirmed(action) };
        let Some(trash) = self.mailbox.folders.iter_mut().find(|f| f.account == *account && f.role == Role::Trash)
        else {
            return;
        };
        let id = trash.id.clone();
        (trash.total, trash.unseen) = (0, 0);
        self.mail_mut().retain(|m| m.envelope.id.account != *account || m.envelope.folder != id);
        // What put a message in the Trash can no longer be reversed.
        let gone = |m: &Listed| {
            m.envelope.id.account == *account && !self.mailbox.mail.iter().any(|x| x.envelope.id == m.envelope.id)
        };
        self.undo.retain(|change| !change.before.iter().any(gone));
        // How many go is the store's to count: a message the server has
        // not numbered in its Trash stays (ADR-041), and `Emptied` says.
        let said = say::plain(format!("Emptying {}'s Trash", account.0));
        self.ask(action, Vec::new());
        self.say(said);
    }
}

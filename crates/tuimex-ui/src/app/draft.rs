//! Compose inside the app: its keys, `:send` and `:agent`, and what comes
//! back from whoever edits, attaches, runs the agent and sends. The draft
//! is handed on as `Written`, then `SaveDraft` or `Send` naming it.

use tuimex_core::{
    Action, AgentAsk, Attachment, Envelope, Event, Flags, FolderId, Identity, Links, Listed, Role, Security, Start,
    redact,
};

use super::{App, Mode};
use crate::compose::quote::{self, Opening};
use crate::compose::summary::{self, Signing, Summary};
use crate::compose::{self, Compose, FIELDS, Field, Mark, Prompt, Stage, View};
use crate::editor::Editor;
use crate::keymap::{self, Found, Key};
use crate::list;
use crate::palette::Tok;
use crate::say::{self, count};
use crate::span::{Line, run};
use crate::spot::Spot;

#[cfg(test)]
mod tests;

/// A question on the command line, then its keys, and where each is.
fn question(text: &str, keys: &[(&str, &str)]) -> (Line, Vec<Spot>) {
    say::choices(list::heading(vec![run(text, Tok::Fg), run("  ", Tok::Dim)]), keys)
}

impl App {
    /// What the binary knows from the config: who mail can be from, and
    /// whether there is an `[agent]` table (ADR-031).
    pub fn set_up(&mut self, identities: Vec<Identity>, agent: bool) {
        (self.identities, self.agent) = (identities, agent);
    }

    /// How long a confirmed message waits in the outbox: the config's
    /// `send.delay`, which is what the engine holds it for.
    pub fn set_delay(&mut self, seconds: u32) {
        self.delay = seconds;
    }

    /// Opens compose. A reply, a forward and a draft are of the message
    /// open in the reader, or else under the cursor, with its whole text
    /// when that has been read. Replying to one's own unsent draft means
    /// nothing, so the reply and forward keys on a draft go on writing it,
    /// as Enter does.
    pub fn compose(&mut self, start: Start) {
        let name = match start {
            Start::Reply => ":reply",
            Start::ReplyAll => ":replyall",
            Start::Forward => ":forward",
            _ => ":compose",
        };
        if self.identities.is_empty() {
            return self.say(say::err(format!("{name}: the config has no account to send from")));
        }
        // The message open in the reader, with the text it shows; or else
        // the one under the cursor.
        let read = self.read_body().map(|(id, body)| (id.clone(), body.text.clone()));
        let on = match read {
            Some((id, text)) => self.message(&id).map(|m| Listed { text: Some(text), ..m.clone() }),
            None => {
                let on = self.row(self.selected.min(self.count().saturating_sub(1))).map(|row| row.msg.clone());
                on.map(|m| match self.text_of(&m.envelope.id) {
                    Some(text) => Listed { text: Some(text.to_string()), ..m },
                    None => m,
                })
            }
        };
        self.serial += 1;
        let opening = Opening { identities: &self.identities, key: quote::key(self.mailbox.now, self.serial) };
        let account = on.as_ref().map(|m| &m.envelope.id.account);
        let answers = matches!(start, Start::Reply | Start::ReplyAll | Start::Forward);
        let start =
            if answers && on.as_ref().is_some_and(|m| m.envelope.flags.draft) { Start::FromDraft } else { start };
        let opened = match (&start, &on) {
            (Start::New, _) => Some(opening.blank(account, "")),
            (Start::MailTo(url), _) if url.contains(':') => opening.mailto(account, url),
            (Start::MailTo(address), _) => Some(opening.blank(account, address)),
            (Start::Reply, Some(msg)) => Some(opening.reply(msg, false)),
            (Start::ReplyAll, Some(msg)) => Some(opening.reply(msg, true)),
            (Start::Forward, Some(msg)) => Some(opening.forward(msg)),
            (Start::FromDraft, Some(msg)) => Some(opening.from_draft(msg)),
            (_, None) => None,
        };
        match opened {
            Some(compose) => self.compose = Some(compose),
            None if on.is_none() => self.say(say::err(format!("{name}: nothing is selected"))),
            None => self.say(say::err(format!("{name}: not a mailto link"))),
        }
    }

    /// Whether Enter on the selected row opens compose: a message the
    /// server refused, to edit and send again, as its hint says.
    pub(super) fn on_refused(&self) -> bool {
        let rows = self.rows();
        let on = rows.get(self.selected.min(rows.len().saturating_sub(1))).map(|row| row.msg);
        on.is_some_and(|m| matches!(m.sending, Some(tuimex_core::Sending::Failed { .. })))
    }

    /// The compose screen, and where on it a press means something.
    pub(super) fn compose_lines(&mut self, width: usize, height: usize) -> Option<Vec<Line>> {
        let c = self.compose.as_ref()?;
        let fits = (height / 3).min(13).saturating_sub(2).max(1);
        let (said, mut panel) = self.command_row(width, fits);
        if c.stage == Stage::Summary
            && let Ok(draft) = c.draft(&self.identities)
        {
            let thread = compose::thread(&self.mailbox.mail, &draft.id.account, c.in_reply_to.as_deref());
            let (agent, signing, right) = (c.agent.as_ref(), &Signing::Neither, &self.alerts());
            let summary = Summary { draft: &draft, agent, thread: &thread, signing, delay: self.delay, right };
            let (lines, answers) = (summary::screen(&summary, width, height), summary::ask(&summary).1);
            self.answers_at(&lines, answers, width);
            return Some(lines);
        }
        let (asked, mut answers) = match &c.stage {
            Stage::Attach(prompt) => {
                if let Some((found, at)) = &prompt.menu {
                    let from = (at + 1).saturating_sub(fits);
                    let entries = found.iter().skip(from).take(fits).map(|path| vec![run(path.as_str(), Tok::Fg)]);
                    panel = list::panel("Files", entries.collect(), Some(at - from), width);
                }
                (Some(list::typed(&format!("attach {}", prompt.text))), Vec::new())
            }
            Stage::Closing => {
                let (line, answers) = question("Keep this draft?", &[("y", "keep"), ("n", "discard"), ("Esc", "back")]);
                (Some(line), answers)
            }
            Stage::Working { .. } => (Some(list::hint(&[("Esc", "cancel")])), Vec::new()),
            Stage::Fields | Stage::Summary => (None, self.confirm_answers(width)),
        };
        if matches!(self.mode, Mode::Command(_)) {
            answers.clear();
        }
        let command = if matches!(self.mode, Mode::Command(_)) { said } else { asked.or(said) };
        let (identities, left) = (&self.identities, self.agent_unsent());
        let (lines, spots) = c.draw(&View { identities, left, right: self.alerts(), command, panel }, width, height);
        self.spots = spots;
        self.answers_at(&lines, answers, width);
        Some(lines)
    }

    /// The draft as a row of Drafts and as what is handed on, or the
    /// address that does not parse (ADR-015). One filed under another
    /// account's id, before the identity changed, is taken out of there.
    fn filed(&mut self, c: &mut Compose) -> Result<Listed, String> {
        let draft = c.draft(&self.identities)?;
        let id = draft.id.clone();
        if let Some(old) = c.stored.take().filter(|old| *old != id) {
            self.mail_mut().retain(|m| m.envelope.id != old);
            self.ask(Action::Discard, vec![old]);
        }
        let drafts = self.mailbox.folder(&id.account, Role::Drafts).map(|f| f.id.clone());
        let file = |(index, f): (usize, &tuimex_core::Attached)| Attachment {
            index,
            name: f.name.clone(),
            mime: String::new(),
            size: f.size,
            inline: false,
        };
        let listed = Listed {
            envelope: Envelope {
                id: id.clone(),
                // An account always has a Drafts folder; one that lists
                // none still gets its draft a row.
                folder: drafts.unwrap_or_else(|| FolderId("Drafts".to_string())),
                date: self.mailbox.now,
                from: vec![draft.from.clone()],
                to: draft.to.clone(),
                cc: draft.cc.clone(),
                bcc: draft.bcc.clone(),
                reply_to: Vec::new(),
                subject: draft.subject.clone(),
                message_id: None,
                references: draft.references.clone(),
                flags: Flags { seen: true, draft: true, ..Flags::default() },
                size: 0,
                has_attachments: !draft.files.is_empty(),
                security: Security::None,
                origin: draft.origin,
            },
            snippet: draft.body.lines().find(|l| !l.is_empty() && !l.starts_with('>')).unwrap_or("").to_string(),
            attachments: draft.files.iter().enumerate().map(file).collect(),
            invite: false,
            unsubscribe: false,
            text: Some(draft.body.clone()),
            sender_was: None,
            sending: None,
        };
        self.ask(Action::Written(Box::new(draft)), vec![id.clone()]);
        c.stored = Some(id);
        Ok(listed)
    }

    /// `^d`, and `y` when closing. Whether it was saved.
    fn save_draft(&mut self, c: &mut Compose) -> bool {
        match self.filed(c) {
            Err(why) => self.say(say::err(format!("save: {why}"))),
            Ok(listed) => {
                let id = listed.envelope.id.clone();
                match self.message(&id) {
                    Some(m) => *m = listed,
                    None => self.mail_mut().push(listed),
                }
                self.ask(Action::SaveDraft, vec![id]);
                (c.saved, c.dirty) = (Some(self.mailbox.now), false);
                self.say(say::ok("Draft saved"));
            }
        }
        !c.dirty
    }

    /// `^s` and `:send`: the summary, once every address parses and there
    /// is someone to send to.
    fn summary(&mut self, c: &mut Compose) {
        match c.draft(&self.identities) {
            Err(why) => self.say(say::err(format!(":send: {why}"))),
            Ok(draft) if draft.to.len() + draft.cc.len() + draft.bcc.len() == 0 => {
                self.say(say::err(":send: there is nobody to send to").then("fill To"));
            }
            Ok(_) => c.stage = Stage::Summary,
        }
    }

    pub(super) fn compose_key(&mut self, key: Key) {
        (self.said, self.fix) = (None, None);
        let Some(mut c) = self.compose.take() else { return };
        let confirm = |id: &str, i: usize| matches!(keymap::find(&["confirm"], &[key]), Found::Bound(b, n) if b.id == id && n == i);
        let stage = std::mem::replace(&mut c.stage, Stage::Fields);
        let open = match stage {
            Stage::Fields => self.field_key(&mut c, key),
            Stage::Attach(prompt) => {
                c.stage = self.attach_key(prompt, key);
                true
            }
            // Only `y` sends: Enter is too near every other key to mean it.
            Stage::Summary if confirm("confirm.yes", 0) => match self.filed(&mut c) {
                Ok(listed) => {
                    self.send(listed, self.delay);
                    false
                }
                Err(why) => {
                    self.say(say::err(format!(":send: {why}")));
                    true
                }
            },
            Stage::Summary => true,
            Stage::Closing if confirm("confirm.yes", 0) => !self.save_draft(&mut c),
            Stage::Closing if confirm("confirm.no", 0) => {
                if let Some(id) = c.stored.take() {
                    self.mail_mut().retain(|m| m.envelope.id != id);
                    self.ask(Action::Discard, vec![id]);
                }
                self.say(say::ok("Draft discarded"));
                false
            }
            Stage::Closing => true,
            Stage::Working { .. } if matches!(keymap::find(&["compose"], &[key]), Found::Bound(b, _) if b.id == "compose.close") =>
            {
                self.ask(Action::AgentCancel, Vec::new());
                self.say(say::plain("Agent stopped").then("nothing changed"));
                true
            }
            working @ Stage::Working { .. } => {
                c.stage = working;
                true
            }
        };
        self.compose = open.then_some(c);
    }

    /// A key while the fields are open. Whether compose stays open.
    fn field_key(&mut self, c: &mut Compose, key: Key) -> bool {
        let at = FIELDS.iter().position(|f| *f == c.focus).unwrap_or(0);
        let people = self.identities.len().max(1);
        let bound = match keymap::find(&["compose"], &[key]) {
            // `h` and `l` are letters in a field that is typed into.
            Found::Bound(binding, _) if binding.id == "compose.identity" && c.typed().is_some() => None,
            Found::Bound(binding, i) => Some((binding.id, i)),
            _ => None,
        };
        match bound {
            Some(("compose.field", i)) => {
                c.focus = FIELDS[(at + if i == 0 { 1 } else { FIELDS.len() - 1 }) % FIELDS.len()]
            }
            Some(("compose.identity" | "compose.identity-arrows", i)) if c.focus == Field::From => {
                (c.identity, c.dirty) = ((c.identity + if i == 0 { people - 1 } else { 1 }) % people, true);
            }
            Some(("compose.identity" | "compose.identity-arrows", _)) => {}
            Some(("compose.editor", _)) if c.focus == Field::Body => {
                self.ask(Action::Edit(c.body.clone()), Vec::new());
            }
            Some(("compose.editor", _)) => c.focus = FIELDS[(at + 1) % FIELDS.len()],
            Some(("compose.attach", _)) => c.stage = Stage::Attach(Prompt::default()),
            Some(("compose.summary", _)) => self.summary(c),
            Some(("compose.save", _)) => {
                self.save_draft(c);
            }
            Some(("compose.close", _)) => return self.close(c),
            _ => {
                if let Some(text) = c.typed() {
                    let mut line = Editor::with(text);
                    match (keymap::find(&["command"], &[key]), key) {
                        (Found::Bound(binding, 0), _) if binding.id == "command.erase" => line.delete_word(),
                        (Found::Bound(binding, _), _) if binding.id == "command.erase" => line.clear(),
                        (_, Key::Char(ch)) => line.push(ch),
                        (_, Key::Backspace) => line.backspace(),
                        _ => return true,
                    }
                    (*text, c.dirty) = (line.text, true);
                    return true;
                }
                // On From and on the body nothing is typed, so the keys
                // that work everywhere do.
                if let Found::Bound(binding, _) = keymap::find(&["global"], &[key]) {
                    match binding.id {
                        "global.command" => self.mode = Mode::Command(Editor::default()),
                        "global.help" => self.mode = Mode::Help(String::new(), 0),
                        "global.agent" => self.agent_menu(),
                        "global.redraw" => self.redraw = true,
                        "global.back" => return self.close(c),
                        "global.undo" => match c.own.take() {
                            Some((body, mark)) => {
                                (c.body, c.agent) = (body, mark);
                                self.say(say::ok("Undone").then("your own text is back"));
                            }
                            None => self.say(say::err("undo: nothing to undo")),
                        },
                        _ => {}
                    }
                }
            }
        }
        true
    }

    /// Esc: a draft with changes asks first. Whether compose stays open.
    fn close(&mut self, c: &mut Compose) -> bool {
        if c.dirty {
            c.stage = Stage::Closing;
        }
        c.dirty
    }

    /// A key while a path is typed after `^a`: the command line's keys.
    fn attach_key(&mut self, mut prompt: Prompt, key: Key) -> Stage {
        let mut line = Editor::with(&prompt.text);
        match (keymap::find(&["command"], &[key]), key) {
            (Found::Bound(binding, i), _) => match (binding.id, i, &mut prompt.menu) {
                ("command.cancel", ..) => return Stage::Fields,
                ("command.run", ..) => {
                    self.ask(Action::Attach(prompt.text), Vec::new());
                    return Stage::Fields;
                }
                ("command.complete", i, Some((found, at))) => {
                    *at = (*at + if i == 0 { 1 } else { found.len() - 1 }) % found.len();
                    prompt.text = found[*at].clone();
                    return Stage::Attach(prompt);
                }
                ("command.complete", ..) => {
                    self.ask(Action::Paths(prompt.text.clone()), Vec::new());
                    return Stage::Attach(prompt);
                }
                ("command.erase", 0, _) => line.delete_word(),
                ("command.erase", ..) => line.clear(),
                _ => return Stage::Attach(prompt),
            },
            (_, Key::Char(ch)) => line.push(ch),
            (_, Key::Backspace) => line.backspace(),
            _ => return Stage::Attach(prompt),
        }
        Stage::Attach(Prompt { text: line.text, menu: None })
    }

    /// `:send`.
    pub(super) fn send_command(&mut self) {
        match self.compose.take() {
            Some(mut c) => {
                self.summary(&mut c);
                self.compose = Some(c);
            }
            None => self.say(say::err(":send: no draft is open").key("c writes one")),
        }
    }

    /// `A`: the menu of what the agent can do here. Opening it sends
    /// nothing; the pick does (ADR-031).
    pub(super) fn agent_menu(&mut self) {
        if self.agent {
            self.mode = Mode::Agent;
        } else {
            self.say(say::err("A: no agent is set up").then("add an [agent] table to the config"));
        }
    }

    /// What the menu offers where it was opened: over the links, a review
    /// of the selected one; in compose, the reply or a tidying of what is
    /// written; on a row or a message, the reply.
    pub(super) fn agent_offers(&self) -> &'static [&'static str] {
        match (self.compose.is_some(), self.link_selected()) {
            (true, _) => &["agent.reply", "agent.tidy"],
            (false, Some(_)) => &["agent.link"],
            (false, None) => &["agent.reply"],
        }
    }

    /// The left of the status line while the menu is open.
    pub(super) fn agent_unsent(&self) -> Option<Line> {
        matches!(self.mode, Mode::Agent).then(|| vec![run("nothing is sent until you pick", Tok::Dim)])
    }

    pub(super) fn agent_pick(&mut self, id: &str) {
        match (id, self.link_selected()) {
            ("agent.link", Some(n)) => self.agent_command(&format!("link {n}")),
            ("agent.tidy", _) => self.agent_command("tidy"),
            _ => self.agent_command("reply"),
        }
    }

    /// `:agent reply [guidance]` and `:agent tidy` (ADR-031): the text is
    /// redacted here, so the status line can say at once what was replaced.
    /// A reply asked for with no draft open starts one, to the message
    /// open or under the cursor.
    pub(super) fn agent_command(&mut self, rest: &str) {
        let (function, guidance) = crate::command::split(rest);
        if !self.agent {
            return self.say(say::err("not a command: :agent").then("no [agent] table in the config"));
        }
        if function == "reply" && self.compose.is_none() {
            self.compose(Start::Reply);
        }
        match function {
            "reply" | "tidy" => {}
            "link" if self.compose.is_some() => {
                return self.say(say::err(":agent link: a draft is open").then("Esc closes it"));
            }
            "link" => return self.reader_command("agent", rest),
            _ => return self.say(say::err(":agent: it takes reply, tidy or link")),
        }
        let Some(mut c) = self.compose.take() else {
            // A reply that could not be started has said why.
            if function == "tidy" {
                self.say(say::err(":agent tidy: no draft is open").key("r replies"));
            }
            return;
        };
        let from = self.identities.get(c.identity).cloned();
        let account = from.as_ref().map(|i| i.account.clone()).unwrap_or_else(|| tuimex_core::AccountId(String::new()));
        let thread = compose::thread(&self.mailbox.mail, &account, c.in_reply_to.as_deref());
        let mut others: Vec<_> = from.iter().map(|i| i.address.clone()).collect();
        others.extend(c.recipients().into_iter().flatten().flatten());
        let names = compose::names(&thread, &others);
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let (tidy, messages) = (function == "tidy", thread.len());
        let (text, problem) = match (tidy, &from) {
            (true, _) if c.body.trim().is_empty() => (String::new(), Some("there is no text to tidy")),
            (true, _) => (c.body.clone(), None),
            (false, Some(from)) if messages > 0 => {
                (format!("{guidance}\u{1f}{}", compose::conversation(&thread, &from.address)), None)
            }
            (false, _) => (String::new(), Some("this draft answers no message")),
        };
        if let Some(problem) = problem {
            self.say(say::err(format!(":agent {function}: {problem}")));
        } else {
            let redacted = redact(&text, &names, Links::Withheld);
            let ask = match redacted.text.split_once('\u{1f}') {
                Some((guidance, thread)) => {
                    AgentAsk::Reply { thread: thread.to_string(), guidance: guidance.to_string() }
                }
                None => AgentAsk::Tidy { body: redacted.text.clone() },
            };
            let serial = self.ask(Action::Agent(ask), Vec::new());
            c.replaced = Some(redacted.replaced());
            c.stage = Stage::Working { serial, tidy, messages, redacted };
        }
        self.compose = Some(c);
    }

    /// What came back for something compose asked for.
    pub(super) fn compose_event(&mut self, event: Event) {
        let Some(mut c) = self.compose.take() else { return };
        match (event, std::mem::replace(&mut c.stage, Stage::Fields)) {
            (Event::Edited { text: Ok(text) }, stage) => {
                c.stage = stage;
                let text = text.replace("\r\n", "\n");
                (c.dirty, c.body) = (c.dirty || text != c.body, text);
            }
            (Event::Edited { text: Err(why) }, stage) => {
                c.stage = stage;
                self.say(say::err(format!("editor: {why}")).then("the body is as it was"));
            }
            (Event::Paths { typed, found }, Stage::Attach(mut prompt)) => {
                if prompt.text == typed && !found.is_empty() {
                    prompt.text = found[0].clone();
                    prompt.menu = (found.len() > 1).then_some((found, 0));
                }
                c.stage = Stage::Attach(prompt);
            }
            (Event::Attached { file: Ok(file) }, stage) => {
                c.stage = stage;
                self.say(say::ok(format!("Attached {}", file.name)).then(crate::reader::size_text(file.size)));
                c.files.push(file);
                c.dirty = true;
            }
            (Event::Attached { file: Err(why) }, stage) => {
                c.stage = stage;
                self.say(say::err(format!("attach: {why}")));
            }
            (Event::AgentText { serial, text }, Stage::Working { serial: asked, tidy, redacted, .. })
                if serial == asked =>
            {
                let function = if tidy { "tidy" } else { "reply" };
                match text.map(|text| redacted.restore(&text)) {
                    Err(why) => self.say(say::err(format!(":agent {function}: {why}")).then("nothing changed")),
                    Ok(text) => {
                        // Nothing a command printed reaches a header or
                        // the terminal as anything but text.
                        let text: String =
                            text.chars().filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control()).collect();
                        let was: Vec<&str> = c.body.split("\n\n").collect();
                        let changed: Vec<String> =
                            text.split("\n\n").filter(|p| !was.contains(p)).map(str::to_string).collect();
                        self.say(if tidy {
                            say::ok(format!("Tidied {}", count(changed.len(), "paragraph"))).key("u for your own text")
                        } else {
                            say::ok("Reply written").key("u to undo")
                        });
                        let mark = Mark { at: self.mailbox.now, tidied: tidy.then_some(changed) };
                        c.own = Some((std::mem::replace(&mut c.body, text), c.agent.replace(mark)));
                        c.dirty = true;
                    }
                }
            }
            (_, stage) => c.stage = stage,
        }
        self.compose = Some(c);
    }
}

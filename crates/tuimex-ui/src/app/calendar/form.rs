//! The form for a new event or a change to one, inside the app: whether it
//! is open, the question it is waiting on, and the keys of the `event-form`
//! table. While it is open no other table is read, so a letter is a letter
//! typed. A saved event leaves through `form_saved` and nowhere else.

use tuimex_core::calendar::DAY;
use tuimex_core::{AccountId, Action};

use super::{View, key_name};
use crate::app::App;
use crate::calendar::form::{self, Form, Saved, Target};
use crate::calendar::parts::Scene;
use crate::editor::Editor;
use crate::keymap::{self, Found, Key};
use crate::palette::Tok;
use crate::say::{self, Question};
use crate::span::{Line, run};
use crate::when::span_of;

#[cfg(test)]
mod tests;

/// A question the form waits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asking {
    /// Whether to save and mail the invitation to this many guests.
    Invite(usize),
    /// Esc with changes: keep them or not.
    Closing,
}

#[derive(Debug, Default)]
pub(in crate::app) struct State {
    form: Option<Form>,
    asking: Option<Asking>,
    /// The accounts whose own calendar tuimex can write over CalDAV.
    writable: Vec<AccountId>,
}

/// The words a date can be typed in, as `gd` offers them.
const DATES: &str = "2026-12-05, 5 dec, fri, +2w";

impl App {
    /// The accounts whose own calendar tuimex can write, which the form
    /// offers beside each account's local one.
    pub fn set_writable_calendars(&mut self, accounts: Vec<AccountId>) {
        self.cal.form.writable = accounts;
    }

    pub(in crate::app) fn form_is_open(&self) -> bool {
        self.cal.form.form.is_some()
    }

    /// Every calendar an event can be saved to, by account in the config's
    /// order: the account's own where tuimex can write it, then its local
    /// one.
    fn targets(&self) -> Vec<Target> {
        let mut all = Vec::new();
        for account in self.accounts().into_iter().map(|name| AccountId(name.to_string())) {
            if self.cal.form.writable.contains(&account) {
                all.push(Target { account: account.clone(), local: false });
            }
            all.push(Target { account, local: true });
        }
        all
    }

    /// Where a new event starts: in the week, the hour of the event
    /// selected on the cursor's day; otherwise the next whole hour today
    /// and nine o'clock on any other day.
    fn new_start(&self) -> i64 {
        let day = self.day();
        let picked = self.picked_entry().filter(|e| self.cal.view == View::Week && !e.all_day);
        if let Some(entry) = picked.filter(|e| e.start.div_euclid(DAY) == day) {
            return entry.start - entry.start.rem_euclid(3600);
        }
        match day == self.cal.model.today() {
            true => (self.cal.model.wall().div_euclid(3600) + 1) * 3600,
            false => day * DAY + 9 * 3600,
        }
    }

    /// `c` and `:event [title]` for a new event on the cursor's day, in
    /// the calendar of the account the view is narrowed to or else the
    /// first; `E` for the selected one, which has to be there.
    pub(super) fn form_open(&mut self, edit: bool, title: &str) {
        let (targets, today) = (self.targets(), self.cal.model.today());
        let opened = if edit {
            let Some(entry) = self.picked_entry() else {
                return self.say(say::err(format!("{}: no event is selected", key_name("calendar.edit", 0))));
            };
            let at = Target { account: entry.account.clone(), local: entry.local };
            Form::edit(targets, at, entry.event, (entry.start, entry.end, entry.all_day), today)
        } else {
            let account = self.cal.account.clone().or_else(|| targets.first().map(|t| t.account.clone()));
            let calendar = targets.iter().position(|t| Some(&t.account) == account.as_ref()).unwrap_or(0);
            let mut opened = Form::new(targets, calendar, self.new_start(), title, today);
            // A title given to `:event` was typed, and closing asks first.
            opened.dirty = !title.is_empty();
            opened
        };
        (self.cal.form.form, self.cal.form.asking) = (Some(opened), None);
    }

    /// A key of the `event-form` table; `i` is which of the binding's keys.
    /// At a question, a key of the form's own is no answer and only leaves
    /// the question.
    pub(super) fn form_run(&mut self, id: &str, i: usize) {
        if self.cal.form.asking.take().is_some() {
            return;
        }
        let Some(mut form) = self.cal.form.form.take() else { return };
        match id {
            "event-form.field" => form.step(i == 0),
            "event-form.choice" => {
                if !form.choose(i == 1) {
                    self.say(say::err(format!(
                        "{}: {} has nothing to choose from",
                        key_name(id, i),
                        form.focus.label()
                    )));
                }
            }
            "event-form.notes" => {
                self.ask(Action::Edit(form.notes.clone()), Vec::new());
            }
            "event-form.save" => {
                self.cal.form.form = Some(form);
                return self.form_save(false);
            }
            "event-form.close" if form.dirty => self.cal.form.asking = Some(Asking::Closing),
            "event-form.close" => return,
            _ => {}
        }
        self.cal.form.form = Some(form);
    }

    /// `^s`, and `y` to keep the changes: the event, or why it cannot be
    /// saved. One with guests to mail asks first, unless `asked` says that
    /// has been answered.
    fn form_save(&mut self, asked: bool) {
        let Some(form) = &self.cal.form.form else { return };
        let key = key_name("event-form.save", 0);
        let undated = [form::Field::Starts, form::Field::Ends]
            .iter()
            .any(|f| form.words(*f).is_some() && form.meaning(*f).is_none());
        match form.saved(&self.cal.model.zone, &self.identities) {
            Err(why) if undated => self.say(say::err(format!("{key}: {why}")).key(DATES)),
            Err(why) => self.say(say::err(format!("{key}: {why}"))),
            Ok(saved) if saved.invite && !asked => {
                self.cal.form.asking = Some(Asking::Invite(saved.event.guests.len()))
            }
            Ok(saved) => {
                (self.cal.form.form, self.cal.form.asking) = (None, None);
                self.form_saved(saved);
            }
        }
    }

    /// Where a saved event is handed over, to be stored in `saved.to` and,
    /// when `saved.invite` says so, mailed to its guests. Nothing stores an
    /// event or mails an invitation yet, so the form says so as it closes.
    pub(in crate::app) fn form_saved(&mut self, _saved: Saved) {
        let key = key_name("event-form.save", 0);
        self.say(say::not_built(key, "storing events and mailing invitations"));
    }

    /// A key the form's table does not name: an answer to the question
    /// asked, or else typing into the focused field with the command line's
    /// editing keys.
    pub(super) fn form_typed(&mut self, key: Key) {
        let answer = |id: &str| matches!(keymap::find(&["confirm"], &[key]), Found::Bound(b, 0) if b.id == id);
        match self.cal.form.asking.take() {
            Some(Asking::Invite(_)) if answer("confirm.yes") => self.form_save(true),
            Some(Asking::Closing) if answer("confirm.yes") => self.form_save(false),
            Some(Asking::Closing) if answer("confirm.no") => {
                self.cal.form.form = None;
                self.say(say::ok("Changes discarded"));
            }
            Some(_) => {}
            None => {
                let Some(form) = self.cal.form.form.as_mut() else { return };
                form.edit_with(|text| {
                    let mut line = Editor::with(text);
                    match (keymap::find(&["command"], &[key]), key) {
                        (Found::Bound(binding, 0), _) if binding.id == "command.erase" => line.delete_word(),
                        (Found::Bound(binding, _), _) if binding.id == "command.erase" => line.clear(),
                        (_, Key::Char(c)) => line.push(c),
                        (_, Key::Backspace) => line.backspace(),
                        _ => return false,
                    }
                    *text = line.text;
                    true
                });
            }
        }
    }

    /// What `$EDITOR` left of the notes, as compose takes back a body.
    pub(in crate::app) fn form_edited(&mut self, text: Result<String, String>) {
        match (text, self.cal.form.form.as_mut()) {
            (Ok(text), Some(form)) => {
                let text = text.replace("\r\n", "\n");
                form.dirty |= text != form.notes;
                form.notes = text;
            }
            (Err(why), Some(_)) => self.say(say::err(format!("editor: {why}")).then("the notes are as they were")),
            (_, None) => {}
        }
    }

    /// The question on the command line, at `width` cells.
    fn form_question(&self, width: usize) -> Option<Line> {
        let question = match self.cal.form.asking? {
            Asking::Invite(guests) => {
                let ask = format!("Save, and mail the invitation to {}?", say::count(guests, "guest"));
                let send = format!("save and send, held {}", span_of(self.delay));
                say::asked(vec![run(ask, Tok::Fg)], &[("y", &send), ("n", "back")])
            }
            Asking::Closing => {
                let ask = vec![run("Keep the changes?", Tok::Fg)];
                say::asked(ask, &[("y", "save"), ("n", "discard"), ("Esc", "back")])
            }
        };
        Some(Question::line(&question, width))
    }

    pub(super) fn form_lines(&self, o: &Scene, width: usize, height: usize) -> Vec<Line> {
        match &self.cal.form.form {
            Some(opened) => form::screen(o, opened, self.form_question(width), width, height),
            None => Vec::new(),
        }
    }
}

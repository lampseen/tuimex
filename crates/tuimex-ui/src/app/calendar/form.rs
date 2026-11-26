//! The form for a new event or a change to one, inside the app: whether it
//! is open, the question it is waiting on, and the keys of the `event-form`
//! table. While it is open no other table is read, so a letter is a letter
//! typed. A saved event leaves through `form_saved` and nowhere else.

use tuimex_core::calendar::DAY;
use tuimex_core::{AccountId, Action, EventId, Stored};

use super::change::title;
use super::{DATES, View, key_name};
use crate::app::App;
use crate::calendar::Entry;
use crate::calendar::change::{Item, Patch};
use crate::calendar::form::{self, Form, Saved, Target};
use crate::calendar::parts::Scene;
use crate::editor::Editor;
use crate::glyphs::DOT;
use crate::keymap::{self, Found, Key};
use crate::palette::Tok;
use crate::say::{self, Question};
use crate::span::{Line, run};
use crate::spot::Spot;
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
    /// The accounts whose own calendar is read over CalDAV and has had no
    /// write tried on it.
    pub(super) unwritten: Vec<AccountId>,
}

impl State {
    pub(super) fn writable(&self) -> &[AccountId] {
        &self.writable
    }
}

impl App {
    /// The accounts whose own calendar tuimex can write, which the form
    /// offers beside each account's local one.
    pub fn set_writable_calendars(&mut self, accounts: Vec<AccountId>) {
        self.cal.form.writable = accounts;
    }

    /// The accounts whose own calendar is read over CalDAV and not
    /// written. What is kept local for one of them says that `tuimex
    /// setup` can find out whether it need be; of a feed, which nothing
    /// can write, that is never said.
    pub fn set_unwritten_calendars(&mut self, accounts: Vec<AccountId>) {
        self.cal.form.unwritten = accounts;
    }

    pub(in crate::app) fn form_is_open(&self) -> bool {
        self.cal.form.form.is_some()
    }

    /// Whether the binding `id` is a letter to type where the form's focus
    /// is: `h` and `l` choose only on a field nothing is typed into.
    pub(super) fn form_types(&self, id: &str) -> bool {
        id == "event-form.choice" && self.cal.form.form.as_ref().is_some_and(|form| form.focus.typed())
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

    /// `E` on the selected event: with `one`, the form holds the selected
    /// day of an event that repeats and no other, and what it saves stands
    /// in for that day.
    pub(super) fn form_open_which(&mut self, one: bool) {
        let Some(entry) = self.picked_entry() else { return self.form_open(true, "") };
        let Some(day) = self.one_day(&entry).filter(|_| one) else { return self.form_open(true, "") };
        let at = Target { account: entry.account.clone(), local: entry.local };
        let times = (entry.start, entry.end, entry.all_day);
        let opened = Form::edit(self.targets(), at, &day, times, self.cal.model.today());
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
            "event-form.choice" | "event-form.choice-arrows" => {
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

    /// A saved event leaves the form: the calendar holds it at once, it
    /// is selected, and the ask that stores it in `saved.to`, and mails
    /// its guests when `saved.invite` says so, goes out.
    pub(in crate::app) fn form_saved(&mut self, saved: Saved) {
        let Saved { to, mut event, replaces, invite } = saved;
        let what = title(&event);
        let of = |at: Target, uid: String| EventId {
            account: at.account,
            local: at.local,
            uid,
            occurrence: event.instead_of.clone(),
        };
        let stored = Stored {
            account: to.account.clone(),
            local: to.local,
            event: event.clone(),
            replaces: replaces.clone().map(|(at, uid)| of(at, uid)),
            invite,
        };
        // Whoever stores a new event gives it its UID; until it is read
        // back it is known here by one no calendar could hold.
        if replaces.is_none() {
            event.uid = format!("unsaved {}", self.serial + 1);
        }
        let mut items = Vec::new();
        if let Some((was, _)) = replaces.as_ref().filter(|(was, _)| *was != to) {
            items.push(Item::Event { at: was.clone(), event: Box::new(event.clone()), held: false, unsaved: false });
        }
        let unsaved = replaces.is_none();
        items.push(Item::Event { at: to.clone(), event: Box::new(event.clone()), held: true, unsaved });
        let (said, restored) = match &replaces {
            None => (format!("Added {what} to {}", self.kept_in(&to)), format!("{what} is gone")),
            Some(_) => (format!("Changed {what}"), format!("{what} is as it was")),
        };
        self.change(key_name("event-form.save", 0), Action::SaveEvent(Box::new(stored)), Patch(items), restored, false);
        // The cursor goes to the event saved: on the day it was on when
        // that day still has it, and else on the day it starts.
        let model = &self.cal.model;
        let starts = event.start.local_wall(&model.zones, &model.zone).map_or(self.day(), |wall| wall.div_euclid(DAY));
        let saved = |day: i64| {
            let days = model.days(day, day + 1, None);
            let is = |e: &&Entry| *e.account == to.account && e.local == to.local && *e.event == event;
            days.on(day).iter().find(is).map(Entry::pick)
        };
        if let Some(picked) = saved(self.day()).or_else(|| saved(starts)) {
            (self.cal.cursor, self.cal.picked) = (Some(picked.day), Some(picked));
        }
        self.say(match self.could_write(&to).filter(|_| replaces.is_none()) {
            Some(setup) => say::ok(said).then(format!("{setup} {DOT} u to undo")),
            None => say::ok(said).key("u to undo"),
        });
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

    pub(super) fn form_lines(&self, o: &Scene, width: usize, height: usize) -> (Vec<Line>, Vec<Spot>) {
        match &self.cal.form.form {
            Some(opened) => form::screen(o, opened, self.form_question(width), width, height),
            None => (Vec::new(), Vec::new()),
        }
    }

    /// The field the focus is on, by its place in `FIELDS`, while the form
    /// waits on no question.
    pub(super) fn form_field(&self) -> Option<usize> {
        let form = self.cal.form.form.as_ref().filter(|_| self.cal.form.asking.is_none())?;
        form::FIELDS.iter().position(|field| *field == form.focus)
    }
}

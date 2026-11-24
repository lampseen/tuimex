//! The calendar as a place (ADR-054): `gc` opens it and `gm` returns to
//! mail, each side keeping where it was. Here is the state every view
//! shares (the day the cursor is on, the view, the selected event, the
//! account it is narrowed to, what is being looked for) and the keys that
//! move it. What one view or one kind of change keeps and does is in the
//! file named for it, and this file reaches each through the functions that
//! file has.

mod answer;
mod change;
mod event;
mod form;
mod join;
#[cfg(test)]
mod tests;

use tuimex_core::calendar::date;
use tuimex_core::{AccountId, Timestamp};

use super::{App, Mode};
use crate::calendar::parts::{self, AHEAD, Scene, View};
use crate::calendar::{self, Calendar, Entry, Picked};
use crate::command::COMMANDS;
use crate::editor::Editor;
use crate::keymap::{self, Found, Key};
use crate::list;
use crate::palette::Tok;
use crate::say;
use crate::span::{Line, run};

/// What is typed after `gd` and after `/`: each is its prompt's label.
const GOTO: &str = "Go to: ";
const FIND: &str = "/";

/// How many days either side of today a search and `I` look through. A
/// repeating event has no last day, so "every day" has to end somewhere.
const SEARCHED: i64 = 366;

/// What is open over the status line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Panel {
    #[default]
    None,
    /// The calendars, to show or hide, and the row chosen.
    Calendars(usize),
    /// The selected event's links, and the one chosen.
    Links(usize),
    /// `i` was pressed, and the next key answers the invitation.
    Answer,
}

#[derive(Debug)]
pub(super) struct Place {
    /// Whether the calendar is what is on screen. Mail's own state is left
    /// as it is meanwhile.
    pub open: bool,
    pub model: Calendar,
    pub view: View,
    /// The day the cursor is on; today until it has been moved.
    cursor: Option<i64>,
    /// The selected event, which the day and the week draw and the month
    /// does not.
    pub picked: Option<Picked>,
    /// The views Enter went deeper from, which Esc returns to.
    back: Vec<View>,
    /// The one account the calendar is narrowed to.
    pub account: Option<AccountId>,
    pub panel: Panel,
    /// What `/` looks for, as typed; with `searching`, the search shown in
    /// place of the view.
    needle: Option<String>,
    searching: bool,
    /// Where the cursor was when the search began.
    sought_from: (Option<i64>, Option<Picked>),
    reminders_off: bool,
    pub event: event::State,
    pub form: form::State,
}

impl Place {
    pub(super) fn new(now: Timestamp) -> Place {
        Place {
            open: false,
            model: Calendar::empty(now),
            view: View::Month,
            cursor: None,
            picked: None,
            back: Vec::new(),
            account: None,
            panel: Panel::None,
            needle: None,
            searching: false,
            sought_from: (None, None),
            reminders_off: false,
            event: event::State::default(),
            form: form::State::default(),
        }
    }
}

/// The key of a binding that was pressed, as the keymap writes it.
fn key_name(id: &str, i: usize) -> &'static str {
    keymap::hint(id).0.split(' ').nth(i).unwrap_or("")
}

impl App {
    /// Takes every calendar as it now stands, from whoever reads them.
    /// Which calendars are hidden is the reader's choice, made here, and
    /// stays as it is.
    pub fn set_calendar(&mut self, calendar: Calendar) {
        let hidden = std::mem::take(&mut self.cal.model.hidden);
        self.cal.model = Calendar { hidden, ..calendar };
    }

    /// Whether reminders are on, for whoever keeps that between runs.
    pub fn reminders(&self) -> bool {
        !self.cal.reminders_off
    }

    pub fn set_reminders(&mut self, on: bool) {
        self.cal.reminders_off = !on;
    }

    /// The day the cursor is on.
    fn day(&self) -> i64 {
        self.cal.cursor.unwrap_or_else(|| self.cal.model.today())
    }

    /// `gc` and `:calendar`: the calendar where it was left, in `view` when
    /// one is named.
    pub(super) fn calendar_open(&mut self, view: Option<View>) {
        self.cal.open = true;
        if let Some(view) = view {
            self.end_search();
            self.cal.view = view;
            self.cal.back.clear();
            self.event_close();
        }
    }

    /// `gm` and `:mail`.
    fn calendar_close(&mut self) {
        (self.cal.open, self.cal.panel) = (false, Panel::None);
    }

    /// Whether the month is what is drawn: it selects a day and no event.
    fn on_month(&self) -> bool {
        self.cal.view == View::Month && !self.cal.searching && !self.event_is_open()
    }

    /// The events from `from` up to `to`, in the order the days draw them:
    /// of the account the calendar is narrowed to, and in a search only
    /// what it found.
    fn listed(&self, from: i64, to: i64) -> Vec<Picked> {
        let needle = self.cal.needle.as_deref().filter(|_| self.cal.searching).map(str::to_lowercase);
        let days = self.cal.model.days(from, to, self.cal.account.as_ref());
        let found = |entry: &&Entry| needle.as_deref().is_none_or(|needle| entry.matches(needle));
        days.all().filter(found).map(Entry::pick).collect()
    }

    /// The selected event, where a view selects one and it is still there.
    pub(super) fn picked_entry(&self) -> Option<Entry<'_>> {
        let picked = self.cal.picked.as_ref().filter(|_| !self.on_month())?;
        self.cal.model.find(picked, self.cal.account.as_ref())
    }

    /// Moves the cursor to `day`. A view that had an event selected selects
    /// the first of that day.
    fn go(&mut self, day: i64) {
        let had = self.cal.picked.is_some();
        self.cal.cursor = Some(day);
        self.cal.picked = if had { self.listed(day, day + 1).into_iter().next() } else { None };
    }

    /// The window of days a step looks through.
    fn reach(&self) -> (i64, i64) {
        let (middle, days) = match self.cal.searching {
            true => (self.cal.model.today(), SEARCHED),
            false => (self.day(), AHEAD),
        };
        (middle - days, middle + days + 1)
    }

    /// `J` and `K`, and `n` and `N` through what a search found: the event
    /// after or before the selected one, across days. The month selects no
    /// event, so there it is the next day that has one.
    fn step(&mut self, forward: bool) {
        let (cursor, (from, to)) = (self.day(), self.reach());
        let all = self.listed(from, to);
        if self.on_month() {
            let day = match forward {
                true => all.iter().map(|picked| picked.day).find(|day| *day > cursor),
                false => all.iter().rev().map(|picked| picked.day).find(|day| *day < cursor),
            };
            return day.into_iter().for_each(|day| self.go(day));
        }
        let at = self.cal.picked.as_ref().and_then(|picked| all.iter().position(|other| other == picked));
        let next = match (at, forward) {
            (Some(at), true) => all.get(at + 1),
            (Some(at), false) => at.checked_sub(1).and_then(|at| all.get(at)),
            (None, true) => all.iter().find(|picked| picked.day >= cursor),
            (None, false) => all.iter().rev().find(|picked| picked.day < cursor),
        };
        if let Some(next) = next.cloned() {
            (self.cal.cursor, self.cal.picked) = (Some(next.day), Some(next));
        }
    }

    /// `I`: the next invitation from today on that waits for an answer,
    /// and round to the first after the last.
    fn next_waiting(&mut self) {
        let (today, cursor, month) = (self.cal.model.today(), self.day(), self.on_month());
        let days = self.cal.model.days(today, today + SEARCHED, self.cal.account.as_ref());
        let waiting: Vec<Picked> = days.all().filter(|entry| entry.waiting()).map(Entry::pick).collect();
        let at = self.cal.picked.as_ref().filter(|_| !month).and_then(|p| waiting.iter().position(|w| w == p));
        let next = match at {
            Some(at) => waiting.get((at + 1) % waiting.len()),
            // The month's cursor is on a day whose invitations it already
            // shows, so the next is on a later one.
            None => waiting.iter().find(|w| w.day > cursor || (!month && w.day == cursor)).or(waiting.first()),
        };
        match next.cloned() {
            Some(next) => (self.cal.cursor, self.cal.picked) = (Some(next.day), Some(next)),
            None => self.say(say::err("I: no invitation waits for an answer")),
        }
    }

    /// `gt`, `gT` and `:account` in the calendar: one account's events
    /// only, or with `None` every account's.
    fn calendar_narrow(&mut self, account: Option<String>) {
        self.cal.account = account.map(AccountId);
        if self
            .cal
            .picked
            .as_ref()
            .is_some_and(|picked| self.cal.account.as_ref().is_some_and(|a| *a != picked.account))
        {
            self.go(self.day());
        }
        self.say(match &self.cal.account {
            Some(account) => say::plain(format!("Showing {} only", account.0)).key("gt for the next, :account for all"),
            None => say::plain("Showing every account"),
        });
    }

    /// `gn` and `:notify`.
    fn remind(&mut self, on: bool) {
        self.cal.reminders_off = !on;
        let turn_on = format!("{} turns them on", keymap::hint("calendar.reminders").0);
        self.say(if on { say::plain("Reminders on") } else { say::plain("Reminders off").key(turn_on) });
    }

    /// `gd` and `:goto`: the day `words` name, or why they name none.
    /// `name` is the key or the command, as the error says it.
    fn goto(&mut self, name: &str, words: &str) {
        match date::parse(words, self.cal.model.today(), self.day()) {
            Some(named) => {
                self.cal.open = true;
                self.go(named.day);
            }
            None => {
                let said = say::err(format!("{name}: not a date: \"{}\"", words.trim()));
                self.say(said.key("2026-12-05, 5 dec, fri, +2w"));
            }
        }
    }

    /// Enter after `/`: every event that matches, on any day searched, in
    /// place of the view until Esc, with the cursor on the first from
    /// today on.
    fn search(&mut self, query: &str) {
        if query.trim().is_empty() {
            return self.end_search();
        }
        self.cal.sought_from = (self.cal.cursor, self.cal.picked.take());
        (self.cal.needle, self.cal.searching) = (Some(query.trim().to_string()), true);
        let (today, (from, to)) = (self.cal.model.today(), self.reach());
        let found = self.listed(from, to);
        self.cal.picked = found.iter().find(|picked| picked.day >= today).or(found.last()).cloned();
        if let Some(first) = &self.cal.picked {
            self.cal.cursor = Some(first.day);
        }
    }

    /// Nothing is looked for any more: a search gives the view back, with
    /// the cursor where it was when the search began.
    fn end_search(&mut self) {
        if std::mem::take(&mut self.cal.searching) {
            (self.cal.cursor, self.cal.picked) = std::mem::take(&mut self.cal.sought_from);
        }
        self.cal.needle = None;
    }

    /// Esc: what is innermost closes, and then the view Enter left returns.
    fn calendar_back(&mut self) {
        if self.cal.panel != Panel::None {
            self.cal.panel = Panel::None;
        } else if self.event_close() {
        } else if self.cal.searching {
            self.end_search();
        } else if let Some(view) = self.cal.back.pop() {
            self.cal.view = view;
        }
    }

    // ---- keys

    /// The tables a key is looked up in, by what is open: the calendar's
    /// own before `global`, so that h, l, Tab and the digits move days and
    /// views here and sections only in mail.
    fn calendar_modes(&self) -> &'static [&'static str] {
        match self.cal.panel {
            _ if self.form_is_open() => &["event-form"],
            Panel::Calendars(_) => &["calendars", "global"],
            Panel::Links(_) => &["links", "global"],
            Panel::Answer => &["invite"],
            Panel::None if self.event_is_open() => &["event", "calendar", "global"],
            Panel::None => &["calendar", "global"],
        }
    }

    /// A key while the calendar is open.
    pub(super) fn calendar_key(&mut self, key: Key) {
        self.pending.push(key);
        match keymap::find(self.calendar_modes(), &self.pending) {
            Found::Prefix => return,
            Found::Bound(binding, i) => {
                self.pending.clear();
                self.calendar_run(binding.id, i);
            }
            Found::Nothing if self.form_is_open() => self.form_typed(key),
            // Any key that is not an answer leaves the question.
            Found::Nothing if self.cal.panel == Panel::Answer => self.cal.panel = Panel::None,
            Found::Nothing => {}
        }
        self.pending.clear();
    }

    /// What a binding does here, by the table it is of.
    fn calendar_run(&mut self, id: &str, i: usize) {
        match id.split_once('.').map_or(id, |(table, _)| table) {
            "event-form" => self.form_run(id, i),
            "event" => self.event_run(id, i),
            "invite" => {
                self.cal.panel = Panel::None;
                self.answer_run(id);
            }
            "calendars" => self.sources_run(id, i),
            "links" => self.event_links_run(id, i),
            // The event's screen takes the calendar's keys its own table
            // names and no others, but for the way back to mail.
            "calendar" if self.event_is_open() && id != "calendar.mail" => {}
            "calendar" => self.place_run(id, i),
            _ => self.calendar_global(id),
        }
    }

    /// The keys of `global`, as far as they mean anything here. Those that
    /// move between sections are mail's.
    fn calendar_global(&mut self, id: &str) {
        match id {
            "global.command" => self.mode = Mode::Command(Editor::default()),
            "global.help" => self.mode = Mode::Help(String::new(), 0),
            "global.back" => self.calendar_back(),
            "global.undo" => self.calendar_undo(),
            "global.agent" if matches!(self.cal.panel, Panel::Links(_)) => self.event_link_agent(),
            "global.agent" if self.agent => self.say(say::err("A: nothing here for the agent")),
            "global.agent" => self.agent_menu(),
            "global.redraw" => self.redraw = true,
            _ => {}
        }
    }

    /// The keys of the `calendar` table; `i` is which of a binding's keys
    /// was pressed, and `first` whether it was the first the table writes.
    pub(super) fn place_run(&mut self, id: &str, i: usize) {
        let (cursor, first) = (self.day(), i == 0);
        match (id, self.cal.view) {
            ("calendar.day", _) => self.go(if first { cursor - 1 } else { cursor + 1 }),
            ("calendar.move", View::Month) if self.on_month() => self.go(if first { cursor + 7 } else { cursor - 7 }),
            ("calendar.move" | "calendar.event", _) => self.step(first),
            ("calendar.match", _) if self.cal.searching => self.step(first),
            ("calendar.match", _) => {
                let find = format!("{} to find", keymap::hint("calendar.find").0);
                self.say(say::err(format!("{}: no filter", key_name(id, i))).key(find));
            }
            ("calendar.period", View::Month) => self.go(date::months_on(cursor, if first { -1 } else { 1 })),
            ("calendar.period", View::Week) => self.go(if first { cursor - 7 } else { cursor + 7 }),
            ("calendar.period", View::Day) => self.go(if first { cursor - 1 } else { cursor + 1 }),
            ("calendar.waiting", _) => self.next_waiting(),
            ("calendar.today", _) => self.go(self.cal.model.today()),
            ("calendar.goto", _) => self.mode = Mode::Prompt(GOTO, Editor::default()),
            ("calendar.view-cycle" | "calendar.view-number", view) => {
                let at = View::ALL.iter().position(|other| *other == view).unwrap_or(0);
                let to = match (id, first) {
                    ("calendar.view-number", _) => i,
                    (_, true) => at + 1,
                    (_, false) => at + View::ALL.len() - 1,
                };
                self.cal.view = View::ALL[to % View::ALL.len()];
                self.cal.back.clear();
            }
            ("calendar.open", _) if self.picked_entry().is_some() => self.event_open(),
            ("calendar.open", view) => {
                // Deeper from a view of days is the day; in the day it is
                // the first event, when the day has one.
                if view != View::Day {
                    self.cal.back.push(view);
                    self.cal.view = View::Day;
                }
                // An event already chosen on that day, as `I` chooses one
                // from the month, stays chosen.
                let on_day = self.listed(cursor, cursor + 1);
                let kept = self.cal.picked.take().filter(|picked| on_day.contains(picked));
                self.cal.picked = kept.or(on_day.into_iter().next());
            }
            ("calendar.find", _) => {
                // One thing is looked for at a time: a search being shown
                // gives the view back before the next is typed.
                self.end_search();
                self.cal.needle = Some(String::new());
                self.mode = Mode::Prompt(FIND, Editor::default());
            }
            ("calendar.account", _) => {
                // Every account is the step between the last and the first.
                let accounts = self.accounts();
                let n = accounts.len() + 1;
                let now = self.cal.account.as_ref().and_then(|now| accounts.iter().position(|a| *a == now.0));
                let at = now.unwrap_or(n - 1);
                let next = accounts.get((at + if first { 1 } else { n - 1 }) % n).map(|name| name.to_string());
                self.calendar_narrow(next);
            }
            ("calendar.calendars", _) => self.cal.panel = Panel::Calendars(0),
            ("calendar.links", _) => match self.picked_entry().map(|entry| parts::links(&entry).0.len()) {
                None => self.say(say::err(format!("{}: no event is selected", key_name(id, i)))),
                Some(0) => self.say(say::err(format!("{}: this event has no link", key_name(id, i)))),
                Some(_) => self.cal.panel = Panel::Links(0),
            },
            ("calendar.answer", _) => self.answer_ask(),
            ("calendar.new", _) => self.form_open(false, ""),
            ("calendar.edit", _) => self.form_open(true, ""),
            ("calendar.delete", _) => self.event_delete(),
            ("calendar.reminders", _) => self.remind(self.cal.reminders_off),
            ("calendar.mail", _) => self.calendar_close(),
            _ => {}
        }
    }

    /// The keys of the Calendars panel.
    fn sources_run(&mut self, id: &str, i: usize) {
        let Panel::Calendars(at) = self.cal.panel else { return };
        let sources = self.cal.model.sources();
        match (id, i) {
            ("calendars.choose", 0) => self.cal.panel = Panel::Calendars((at + 1).min(sources.len().saturating_sub(1))),
            ("calendars.choose", _) => self.cal.panel = Panel::Calendars(at.saturating_sub(1)),
            ("calendars.toggle", _) => {
                let Some((account, local)) = sources.get(at).map(|source| (source.account.clone(), source.local))
                else {
                    return;
                };
                self.cal.model.toggle(&account, local);
            }
            ("calendars.close", _) => self.cal.panel = Panel::None,
            _ => self.calendar_global(id),
        }
    }

    /// The keys of the links panel over an event.
    fn event_links_run(&mut self, id: &str, i: usize) {
        let Panel::Links(at) = self.cal.panel else { return };
        let links = self.picked_entry().map_or(0, |entry| parts::links(&entry).0.len());
        match (id, i) {
            ("links.number", n) if n < links => self.event_link_open(n + 1),
            ("links.number", n) => {
                let lists = format!("{} lists them", keymap::hint("calendar.links").0);
                self.say(say::err(format!("o: this event has no link {}", n + 1)).key(lists));
            }
            ("links.choose", 0) => self.cal.panel = Panel::Links((at + 1).min(links.saturating_sub(1))),
            ("links.choose", 1) => self.cal.panel = Panel::Links(at.saturating_sub(1)),
            ("links.choose", _) => self.event_link_open(at + 1),
            ("links.close", _) => self.cal.panel = Panel::None,
            _ => self.calendar_global(id),
        }
    }

    // ---- what is typed

    /// A key while `gd` or `/` is being typed after.
    pub(super) fn prompt_key(&mut self, label: &'static str, mut editor: Editor, key: Key) {
        match keymap::find(&["command"], &[key]) {
            Found::Bound(binding, i) => match (binding.id, i) {
                ("command.cancel", _) => return self.prompted(label, None),
                ("command.run", _) => return self.prompted(label, Some(&editor.text)),
                ("command.erase", 0) => editor.delete_word(),
                ("command.erase", _) => editor.clear(),
                _ => {}
            },
            _ => match key {
                Key::Char(c) => editor.push(c),
                // Backspace on an empty line leaves it, as it does in vi.
                Key::Backspace if editor.text.is_empty() => return self.prompted(label, None),
                Key::Backspace => editor.backspace(),
                _ => {}
            },
        }
        if label == FIND {
            self.cal.needle = Some(editor.text.clone());
        }
        self.mode = Mode::Prompt(label, editor);
    }

    /// What was typed was entered, or with `None` given up.
    fn prompted(&mut self, label: &str, entered: Option<&str>) {
        match (label, entered) {
            (GOTO, Some(words)) => self.goto(keymap::hint("calendar.goto").0, words),
            (FIND, Some(query)) => self.search(query),
            (FIND, None) => self.end_search(),
            _ => {}
        }
    }

    /// The left of the status line while something is typed: for a date,
    /// the day the words mean so far.
    fn prompt_left(&self) -> Option<Line> {
        let Mode::Prompt(label, editor) = &self.mode else { return None };
        let named = date::parse(&editor.text, self.cal.model.today(), self.day()).filter(|_| *label == GOTO);
        Some(named.map_or_else(Line::new, |named| {
            let year = tuimex_core::calendar::civil_from_days(named.day).0;
            vec![run("goes to", Tok::Dim), run(format!(" {} {year}", parts::day_short(named.day)), Tok::Fg)]
        }))
    }

    // ---- commands

    /// A command that is the calendar's, or that means something else
    /// while the calendar is open. Says whether it was one, so that every
    /// other command is run as mail runs it.
    pub(super) fn calendar_command(&mut self, name: &str, rest: &str) -> bool {
        match name {
            "calendar" if rest.is_empty() => self.calendar_open(None),
            "calendar" => match View::from_name(rest) {
                Some(view) => self.calendar_open(Some(view)),
                None => self.offer(say::err(":calendar: it takes month, week or day"), "calendar "),
            },
            "mail" => self.calendar_close(),
            "goto" if rest.is_empty() => {
                self.say(say::err(":goto: name a date").key("2026-12-05, 5 dec, fri, +2w"));
            }
            "goto" => self.goto(":goto", rest),
            "notify" if ["on", "off"].contains(&rest) => self.remind(rest == "on"),
            "notify" => self.offer(say::err(":notify: it takes on or off"), "notify "),
            "event" => {
                self.calendar_open(None);
                self.form_open(false, rest);
            }
            "export" => self.say(say::not_built(":export", "writing local events as .ics")),
            _ if !self.cal.open => return false,
            "account" if rest.is_empty() => self.calendar_narrow(None),
            "account" if self.accounts().contains(&rest) => self.calendar_narrow(Some(rest.to_string())),
            "invite" => self.answer_command(rest),
            // Back, as everywhere but the list: out of the event, then out
            // of the calendar.
            "q" => {
                if !self.event_close() {
                    self.calendar_close();
                }
            }
            "account" | "qa" | "sync" | "messages" | "preview" => return false,
            // The rows a command of mail's would take are out of sight.
            _ if COMMANDS.iter().any(|command| command.name == name) => {
                let mail = format!("{} returns to mail", keymap::hint("calendar.mail").0);
                self.say(say::err(format!(":{name}: the calendar is open")).key(mail));
            }
            _ => return false,
        }
        true
    }

    // ---- what is on screen

    /// The calendar's screen.
    pub(super) fn calendar_lines(&mut self, width: usize, height: usize, mail: Line) -> Vec<Line> {
        // An event that has gone, or is in a calendar now hidden, has no
        // screen to show.
        if self.event_is_open() && self.picked_entry().is_none() {
            self.event_close();
        }
        let fits = (height / 3).max(3);
        let (command, menu) = self.command_row(width, fits.min(13).saturating_sub(2).max(1));
        let accounts: Vec<AccountId> = self.accounts().into_iter().map(|name| AccountId(name.to_string())).collect();
        let colours = list::account_colours(&self.order, &accounts, &self.fixed);
        let place = &self.cal;
        let needle = place.needle.as_deref().map(str::to_lowercase).filter(|needle| !needle.is_empty());
        let base = Scene {
            calendar: &place.model,
            view: place.view,
            cursor: self.day(),
            picked: place.picked.as_ref().filter(|_| !self.on_month()),
            accounts: &accounts,
            colours: &colours,
            words: self.theme.palette.words(&colours).1,
            narrowed: place.account.as_ref(),
            needle: needle.as_deref(),
            search: place.needle.as_deref().filter(|_| place.searching),
            preview: self.preview,
            panel: Vec::new(),
            left: None,
            right: parts::alerts(&place.model, mail, None, place.reminders_off),
            command: None,
        };
        let entry = base.entry();
        let window = |entries: Vec<Line>, at: usize| {
            let at = at.min(entries.len().saturating_sub(1));
            let from = (at + 1).saturating_sub(fits);
            (entries.into_iter().skip(from).take(fits).collect::<Vec<_>>(), Some(at - from))
        };
        let keys = |ids: &[&str]| list::hint(&ids.iter().map(|id| keymap::hint(id)).collect::<Vec<_>>());
        let (panel, left, hint) = match (place.panel, &entry) {
            (Panel::Calendars(at), _) => {
                let (entries, at) = window(parts::sources(&base), at);
                let hint = keys(&["calendars.choose", "calendars.toggle", "calendars.close"]);
                (list::panel("Calendars", entries, at, width), Some(Line::new()), Some(hint))
            }
            (Panel::Links(at), Some(entry)) => {
                let (rows, urls) = parts::links(entry);
                let url = urls.get(at).cloned().unwrap_or_default();
                let (entries, at) = window(rows, at);
                let opens = vec![run("opens", Tok::Dim), run(format!(" {url}"), Tok::Fg)];
                let hint = keys(&["links.number", "links.choose", "global.agent", "links.close"]);
                (list::panel("Links", entries, at, width), Some(opens), Some(hint))
            }
            (Panel::Answer, _) => (Vec::new(), None, Some(say::answer().line(width))),
            _ => (Vec::new(), None, None),
        };
        let o = Scene {
            panel: if menu.is_empty() { panel } else { menu },
            left: self.prompt_left().or(left),
            command: command.or(hint),
            ..base
        };
        match (&entry, place.view) {
            _ if self.form_is_open() => self.form_lines(&o, width, height),
            (Some(entry), _) if self.event_is_open() => self.event_lines(&o, entry, width, height),
            _ if place.searching => calendar::day::screen(&o, width, height),
            (_, View::Month) => calendar::month::screen(&o, width, height),
            (_, View::Week) => calendar::week::screen(&o, width, height),
            (_, View::Day) => calendar::day::screen(&o, width, height),
        }
    }
}

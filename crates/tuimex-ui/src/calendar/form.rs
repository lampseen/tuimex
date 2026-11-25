//! The form for a new event or a change to one: the design's
//! `eventFormScreen`, a field to a row as compose has them and the notes
//! below as the body, and what the fields hold until they become an
//! `Event`.

use tuimex_core::calendar::{DAY, Event, Freq, Guest, Moment, Repeat, Rule, Said, Zone, civil_from_days, date};
use tuimex_core::{AccountId, Address, Identity, addresses, pieces};

use super::parts::{Scene, clock, day_short};
use crate::compose::quote::written as written_as;
use crate::compose::{field_row, frame, person};
use crate::keymap;
use crate::layout::{INDENT, MARGIN};
use crate::list::{heading, hint, status_row};
use crate::palette::Tok;
use crate::span::{Line, fit, gap, run, wrap};

#[cfg(test)]
mod tests;

/// The design's `LAYOUT.measure`: notes are read at the width mail is.
const MEASURE: usize = 76;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Title,
    Calendar,
    Starts,
    Ends,
    AllDay,
    Repeats,
    Where,
    Guests,
    Reminder,
}

pub const FIELDS: [Field; 9] = [
    Field::Title,
    Field::Calendar,
    Field::Starts,
    Field::Ends,
    Field::AllDay,
    Field::Repeats,
    Field::Where,
    Field::Guests,
    Field::Reminder,
];

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Title => "Title",
            Field::Calendar => "Calendar",
            Field::Starts => "Starts",
            Field::Ends => "Ends",
            Field::AllDay => "All day",
            Field::Repeats => "Repeats",
            Field::Where => "Where",
            Field::Guests => "Guests",
            Field::Reminder => "Reminder",
        }
    }

    /// Whether what is typed goes into it; the others are chosen from.
    pub fn typed(self) -> bool {
        matches!(self, Field::Title | Field::Starts | Field::Ends | Field::Where | Field::Guests)
    }
}

/// A calendar an event can be saved to: an account, and whether it is the
/// one tuimex alone holds for it (ADR-055).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub account: AccountId,
    pub local: bool,
}

impl Target {
    /// An account's own calendar is named by the account alone.
    pub fn label(&self) -> String {
        if self.local { format!("{} local", self.account.0) } else { self.account.0.clone() }
    }
}

/// What a save hands over: where the event goes, the event, the one it
/// takes the place of, and whether its guests are to be mailed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub to: Target,
    pub event: Event,
    /// The calendar and UID of the event this replaces; `None` for a new
    /// one. It may be in another calendar than `to` when the form moved it.
    pub replaces: Option<(Target, String)>,
    /// The guests get the invitation by mail (ADR-057).
    pub invite: bool,
}

/// The event a form was opened on, and the form as it was then: a field
/// that still shows what it showed is left as the event has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Was {
    pub at: Target,
    pub event: Event,
    pub opened: Box<Form>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub heading: String,
    pub focus: Field,
    pub title: String,
    pub calendars: Vec<Target>,
    pub calendar: usize,
    /// Wall times in the reader's zone. An all-day event is on the days of
    /// the two, and their times are kept for when it stops being one.
    pub starts: i64,
    pub ends: i64,
    /// Words typed into Starts or Ends that have not yet become its date:
    /// while the field is focused, or after leaving it with words that name
    /// none.
    pub starts_words: Option<String>,
    pub ends_words: Option<String>,
    pub all_day: bool,
    pub repeats: Vec<(String, Option<Repeat>)>,
    pub repeat: usize,
    pub place: String,
    pub guests: String,
    pub reminders: Vec<(String, Vec<i64>)>,
    pub reminder: usize,
    pub notes: String,
    /// The day the words of a date are read against.
    pub today: i64,
    pub dirty: bool,
    pub was: Option<Was>,
}

/// The rule each Repeats choice stands for, after `never`.
fn rule(freq: Freq, by_day: Vec<(i64, i64)>) -> Option<Repeat> {
    Some(Repeat::Rule(Rule { freq, interval: 1, count: None, until: None, by_day, by_month_day: Vec::new() }))
}

fn repeats() -> Vec<(String, Option<Repeat>)> {
    let weekdays = (1..=5).map(|weekday| (0, weekday)).collect();
    [
        ("never", None),
        ("daily", rule(Freq::Daily, Vec::new())),
        ("weekly", rule(Freq::Weekly, Vec::new())),
        ("every weekday", rule(Freq::Weekly, weekdays)),
        ("monthly", rule(Freq::Monthly, Vec::new())),
        ("yearly", rule(Freq::Yearly, Vec::new())),
    ]
    .into_iter()
    .map(|(label, repeat)| (label.to_string(), repeat))
    .collect()
}

/// How long before the start a reminder is, in words.
fn lead(seconds: i64) -> String {
    let unit = |n: i64, what: &str| format!("{n} {what}{} before", if n == 1 { "" } else { "s" });
    match seconds {
        0 => "at the start".to_string(),
        s if s % DAY == 0 => unit(s / DAY, "day"),
        s if s % 3600 == 0 => unit(s / 3600, "hour"),
        s => unit(s / 60, "minute"),
    }
}

/// The Reminder choices, and the one a new event starts on.
const LEADS: [i64; 7] = [0, 300, 600, 900, 1800, 3600, DAY];
const FIRST_REMINDER: usize = 3;

fn reminders() -> Vec<(String, Vec<i64>)> {
    let mut all = vec![("none".to_string(), Vec::new())];
    all.extend(LEADS.iter().map(|&s| (lead(s), vec![s])));
    all
}

/// A day and a time as the form writes them: `Thu 26 Nov 2026 14:00`.
pub fn written(wall: i64, all_day: bool) -> String {
    let day = wall.div_euclid(DAY);
    let year = civil_from_days(day).0;
    if all_day { format!("{} {year}", day_short(day)) } else { format!("{} {year} {}", day_short(day), clock(wall)) }
}

impl Form {
    /// A new event from `starts`, an hour long, in `calendars[calendar]`.
    pub fn new(calendars: Vec<Target>, calendar: usize, starts: i64, title: &str, today: i64) -> Form {
        Form {
            heading: "New event".to_string(),
            focus: Field::Title,
            title: title.to_string(),
            calendars,
            calendar,
            starts,
            ends: starts + 3600,
            starts_words: None,
            ends_words: None,
            all_day: false,
            repeats: repeats(),
            repeat: 0,
            place: String::new(),
            guests: String::new(),
            reminders: reminders(),
            reminder: FIRST_REMINDER,
            notes: String::new(),
            today,
            dirty: false,
            was: None,
        }
    }

    /// The form filled from `event`, held in `at`, as its occurrence from
    /// `start` to `end` is drawn. `at` is added to `calendars` when it is
    /// not among them, so the form can always say where the event is.
    pub fn edit(
        mut calendars: Vec<Target>,
        at: Target,
        event: &Event,
        (start, end, all_day): (i64, i64, bool),
        today: i64,
    ) -> Form {
        let calendar = calendars.iter().position(|c| *c == at).unwrap_or_else(|| {
            calendars.push(at.clone());
            calendars.len() - 1
        });
        let mut form = Form::new(calendars, calendar, start, "", today);
        form.heading = event.summary.clone();
        form.title = event.summary.clone();
        (form.ends, form.all_day) = (end, all_day);
        if all_day {
            // An all-day event ends at the midnight after its last day; the
            // form shows that day, and the hours it would have if it
            // stopped being all day.
            (form.starts, form.ends) = (start + 9 * 3600, (end - DAY).max(start) + 10 * 3600);
        }
        let shape = |repeat: &Option<Repeat>| match repeat {
            Some(Repeat::Rule(r)) => Some((r.freq, r.interval, r.by_day.clone(), r.by_month_day.is_empty())),
            _ => None,
        };
        // A rule of a choice's shape is that choice, with its own end kept.
        form.repeat = match form.repeats.iter().position(|(_, r)| shape(r) == shape(&event.repeat)) {
            Some(at) if event.repeat.is_none() || at > 0 => at,
            _ => {
                form.repeats.push(("its own rule".to_string(), None));
                form.repeats.len() - 1
            }
        };
        form.repeats[form.repeat].1 = event.repeat.clone();
        form.place = event.location.clone().unwrap_or_default();
        form.guests = event.guests.iter().map(|g| written_as(&g.who)).collect::<Vec<_>>().join(", ");
        form.reminder = form.reminders.iter().position(|(_, alarms)| *alarms == event.alarms).unwrap_or_else(|| {
            let more =
                if event.alarms.len() > 1 { format!(" and {} more", event.alarms.len() - 1) } else { String::new() };
            let first = event.alarms.first().copied().unwrap_or(0);
            form.reminders.push((format!("{}{more}", lead(first)), event.alarms.clone()));
            form.reminders.len() - 1
        });
        form.notes = event.description.clone().unwrap_or_default();
        let opened = Box::new(form.clone());
        form.was = Some(Was { at, event: event.clone(), opened });
        form
    }

    /// The words a date field holds while they have not become its date.
    pub fn words(&self, field: Field) -> Option<&String> {
        match field {
            Field::Starts => self.starts_words.as_ref(),
            Field::Ends => self.ends_words.as_ref(),
            _ => None,
        }
    }

    /// What the words typed into Starts or Ends make of the field, as a
    /// wall time: they are merged with what it held. A time alone keeps
    /// the field's date and a date alone its time; a step (`+1w`) is from
    /// the field's own day; no words at all leave it as it was. `None`
    /// when the words name neither.
    pub fn meaning(&self, field: Field) -> Option<i64> {
        let words = self.words(field)?;
        let own = if field == Field::Starts { self.starts } else { self.ends };
        if words.trim().is_empty() {
            return Some(own);
        }
        if let Some(minute) = date::time(words) {
            return Some(own.div_euclid(DAY) * DAY + minute * 60);
        }
        let named = date::parse(words, self.today, own.div_euclid(DAY))?;
        Some(named.day * DAY + named.minute.map_or(own.rem_euclid(DAY), |minute| minute * 60))
    }

    /// Whether a field's words give a time of day. On an event that lasts
    /// all day that is the reader saying it does not.
    fn timed(&self, field: Field) -> bool {
        let named = |words: &String| date::parse(words, self.today, self.today).and_then(|named| named.minute);
        self.words(field).is_some_and(|words| date::time(words).or(named(words)).is_some())
    }

    /// The date a field's words name becomes its date, and moving the
    /// start moves the end with it. Words that name none stay, and the
    /// field is drawn marked.
    pub fn settle(&mut self) {
        for field in [Field::Starts, Field::Ends] {
            let Some(wall) = self.meaning(field) else { continue };
            self.all_day &= !self.timed(field);
            if field == Field::Starts {
                (self.ends, self.starts, self.starts_words) = (self.ends + wall - self.starts, wall, None);
            } else {
                (self.ends, self.ends_words) = (wall, None);
            }
        }
    }

    /// Tab and S-Tab.
    pub fn step(&mut self, forward: bool) {
        self.settle();
        let at = FIELDS.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = FIELDS[(at + if forward { 1 } else { FIELDS.len() - 1 }) % FIELDS.len()];
    }

    /// `h` and `l`, or Left and Right. Whether the focused field offers a
    /// choice.
    pub fn choose(&mut self, forward: bool) -> bool {
        let step = |at: &mut usize, n: usize| *at = (*at + if forward { 1 } else { n.max(1) - 1 }) % n.max(1);
        match self.focus {
            Field::Calendar => step(&mut self.calendar, self.calendars.len()),
            Field::AllDay => self.all_day = !self.all_day,
            Field::Repeats => step(&mut self.repeat, self.repeats.len()),
            Field::Reminder => step(&mut self.reminder, self.reminders.len()),
            _ => return false,
        }
        self.dirty = true;
        true
    }

    /// Typing into the focused field: `change` edits its text and says
    /// whether it did. A date's words start empty, so what is typed
    /// replaces the date shown.
    pub fn edit_with(&mut self, change: impl FnOnce(&mut String) -> bool) -> bool {
        let changed = match self.focus {
            Field::Title => change(&mut self.title),
            Field::Where => change(&mut self.place),
            Field::Guests => change(&mut self.guests),
            Field::Starts | Field::Ends => {
                let words = if self.focus == Field::Starts { &mut self.starts_words } else { &mut self.ends_words };
                let mut text = words.clone().unwrap_or_default();
                let changed = change(&mut text);
                if changed {
                    *words = Some(text);
                }
                changed
            }
            _ => false,
        };
        self.dirty |= changed;
        changed
    }

    /// When the event is, as the event writes it. A field left as it was
    /// opened keeps the event's own way of writing its time; one moved
    /// moves that time by as much. A new time is an instant, so a guest
    /// elsewhere is asked for the same moment, except in a repeating event,
    /// whose time stays on the wall clock when the clocks change.
    fn timing(&self, zone: &Zone, repeating: bool) -> (Moment, Option<Moment>) {
        if self.all_day {
            let (first, last) = (self.starts.div_euclid(DAY), self.ends.div_euclid(DAY));
            return (Moment::Day(first), Some(Moment::Day(last + 1)));
        }
        let fresh = |wall: i64| match repeating {
            true => Moment::Wall { wall, zone: None },
            false => Moment::Utc(wall - zone.offset_at_wall(wall).unwrap_or(0)),
        };
        let shift = |moment: &Moment, by: i64| match moment {
            Moment::Day(day) => Moment::Day(day + by.div_euclid(DAY)),
            Moment::Utc(at) => Moment::Utc(at + by),
            Moment::Wall { wall, zone } => Moment::Wall { wall: wall + by, zone: zone.clone() },
        };
        match &self.was {
            Some(Was { event, opened, .. }) if !opened.all_day && !event.start.is_day() => {
                let (end, from) = match &event.end {
                    Some(end) => (end, opened.ends),
                    None => (&event.start, opened.starts),
                };
                (shift(&event.start, self.starts - opened.starts), Some(shift(end, self.ends - from)))
            }
            _ => (fresh(self.starts), Some(fresh(self.ends))),
        }
    }

    /// What `^s` hands over, or why it cannot be saved. `zone` is the
    /// reader's; `identities` say who the reader is in each account.
    pub fn saved(&self, zone: &Zone, identities: &[Identity]) -> Result<Saved, String> {
        for field in [Field::Starts, Field::Ends] {
            if let Some(words) = self.words(field).filter(|_| self.meaning(field).is_none()) {
                return Err(format!("{}: not a date: \"{}\"", field.label(), words.trim()));
            }
        }
        let mut form = self.clone();
        form.settle();
        let to = form.calendars.get(form.calendar).cloned().ok_or("there is no calendar to save it to")?;
        let (first, last) = (form.starts.div_euclid(DAY), form.ends.div_euclid(DAY));
        if (form.all_day && last < first) || (!form.all_day && form.ends < form.starts) {
            return Err("it ends before it starts".to_string());
        }
        let guests = addresses(&form.guests.replace('\n', ","))?;
        let own: Vec<&Address> = identities.iter().filter(|i| i.account == to.account).map(|i| &i.address).collect();
        let opened = form.was.as_ref().map(|was| &*was.opened);
        let kept = |same: fn(&Form, &Form) -> bool| opened.is_some_and(|opened| same(opened, &form));
        let repeat = form.repeats.get(form.repeat).and_then(|(_, repeat)| repeat.clone());
        let mut event = form.was.as_ref().map_or_else(blank, |was| was.event.clone());
        if !kept(|a, b| a.title == b.title) {
            event.summary = form.title.clone();
        }
        if !kept(|a, b| (a.starts, a.ends, a.all_day) == (b.starts, b.ends, b.all_day)) {
            (event.start, event.end) = form.timing(zone, repeat.is_some());
        }
        if !kept(|a, b| a.repeat == b.repeat) {
            event.repeat = repeat;
        }
        if !kept(|a, b| a.place == b.place) {
            event.location = Some(form.place.trim().to_string()).filter(|place| !place.is_empty());
        }
        if !kept(|a, b| a.guests == b.guests) {
            let said = |who: &Address| {
                let was = event.guests.iter().find(|g| g.who.email.eq_ignore_ascii_case(&who.email));
                was.map_or(Said::Nothing, |g| g.said)
            };
            let asked: Vec<Guest> = guests.iter().map(|who| Guest { who: who.clone(), said: said(who) }).collect();
            event.guests = asked;
            if event.organizer.is_none() && !event.guests.is_empty() {
                event.organizer = own.first().map(|me| (*me).clone());
            }
        }
        if !kept(|a, b| a.reminder == b.reminder) {
            event.alarms = form.reminders.get(form.reminder).map(|(_, alarms)| alarms.clone()).unwrap_or_default();
        }
        if !kept(|a, b| a.notes == b.notes) {
            event.description = Some(form.notes.clone()).filter(|notes| !notes.trim().is_empty());
        }
        // Only the organizer sends an invitation: a change to an event
        // somebody else asked the reader to stays with the reader.
        let organizes =
            event.organizer.as_ref().is_none_or(|o| own.iter().any(|me| me.email.eq_ignore_ascii_case(&o.email)));
        let invite = !event.guests.is_empty() && organizes;
        let replaces = form.was.as_ref().map(|was| (was.at.clone(), was.event.uid.clone()));
        Ok(Saved { to, event, replaces, invite })
    }

    /// The rows a field takes: one, or for Guests one to each guest.
    fn value(&self, field: Field) -> Vec<Line> {
        let on = field == self.focus;
        let date = |wall: i64, words: Option<&String>| match words {
            Some(words) => vec![run(words.as_str(), if on { Tok::Fg } else { Tok::Err })],
            None => vec![run(written(wall, self.all_day), Tok::Fg)],
        };
        let label = |labels: Vec<&String>, at: usize| labels.get(at).map(|label| label.to_string()).unwrap_or_default();
        let repeats = label(self.repeats.iter().map(|(label, _)| label).collect(), self.repeat);
        let reminder = label(self.reminders.iter().map(|(label, _)| label).collect(), self.reminder);
        vec![match field {
            Field::Title => vec![run(self.title.as_str(), Tok::Fg).bold()],
            Field::Calendar => {
                let choice = |(i, target): (usize, &Target)| {
                    let chosen = i == self.calendar;
                    let label = run(target.label(), if chosen { Tok::Fg } else { Tok::Dim });
                    [if chosen { label.bold() } else { label }, gap(3)]
                };
                self.calendars.iter().enumerate().flat_map(choice).collect()
            }
            Field::Starts => date(self.starts, self.starts_words.as_ref()),
            Field::Ends => date(self.ends, self.ends_words.as_ref()),
            Field::AllDay => vec![if self.all_day { run("yes", Tok::Fg) } else { run("no", Tok::Dim) }],
            Field::Repeats => vec![run(repeats, if self.repeat == 0 { Tok::Dim } else { Tok::Fg })],
            Field::Where if self.place.is_empty() => Line::new(),
            Field::Where => vec![run(self.place.as_str(), Tok::Fg)],
            Field::Reminder => vec![run(reminder, Tok::Fg)],
            Field::Guests => {
                let typed = self.guests.replace('\n', ",");
                let all = pieces(&typed);
                let last = all.len() - 1;
                // As in compose: a comma just typed opens the next row, and
                // one left behind leaves no empty row.
                let shown =
                    all.iter().enumerate().filter(|(i, piece)| !piece.is_empty() || *i == 0 || (on && *i == last));
                return shown
                    .map(|(_, piece)| match addresses(piece).as_deref() {
                        Ok([one]) => person(one),
                        _ => vec![run(*piece, Tok::Fg)],
                    })
                    .collect();
            }
        }]
    }

    /// The left of the status line: while a date is focused, what its
    /// words were taken to mean, as `gd` says it.
    fn left(&self) -> Line {
        let (word, own) = match self.focus {
            Field::Starts => ("starts", self.starts),
            Field::Ends => ("ends", self.ends),
            _ => return vec![run("not saved", Tok::Dim)],
        };
        let wall = if self.words(self.focus).is_some() { self.meaning(self.focus) } else { Some(own) };
        // A time typed into an event that lasts all day will end that.
        let all_day = self.all_day && !self.timed(self.focus);
        let said = |wall: i64| vec![run(word, Tok::Dim), run(format!(" {}", written(wall, all_day)), Tok::Fg)];
        wall.map_or_else(|| vec![run("not a date or a time", Tok::Dim)], said)
    }
}

/// An event with nothing in it, which a new one is filled into. Its UID is
/// left empty: whoever stores it makes one, as it makes the file's name.
fn blank() -> Event {
    Event {
        uid: String::new(),
        sequence: 0,
        instead_of: None,
        summary: String::new(),
        start: Moment::Day(0),
        end: None,
        location: None,
        description: None,
        organizer: None,
        guests: Vec::new(),
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

/// The form's screen. `asked` is a question on the command line, which
/// takes the caret away until it is answered.
pub fn screen(o: &Scene, form: &Form, asked: Option<Line>, width: usize, height: usize) -> Vec<Line> {
    let typing = asked.is_none();
    let mut head = vec![heading(vec![run(form.heading.as_str(), Tok::Fg).bold()]), Line::new()];
    for field in FIELDS {
        let on = field == form.focus;
        let mut values = form.value(field);
        if let Some(last) = values.last_mut().filter(|_| on && typing && field.typed()) {
            last.push(run(" ", Tok::Bg).on(Tok::Acc));
        }
        for (i, value) in values.into_iter().enumerate() {
            head.push(field_row(on && i == 0, if i == 0 { field.label() } else { "" }, value));
        }
    }
    head.push(Line::new());
    let head = head.iter().map(|row| fit(row, width)).collect();
    let measure = MEASURE.min(width.saturating_sub(INDENT + MARGIN));
    let mut body: Vec<Line> = Vec::new();
    for line in form.notes.lines() {
        let line = line.replace('\t', "    ");
        // A blank line stays one; `wrap` would drop it.
        let wrapped = if line.trim().is_empty() { vec![String::new()] } else { wrap(&line, measure) };
        body.extend(wrapped.into_iter().map(|text| vec![gap(INDENT), run(text, Tok::Fg)]));
    }
    if form.notes.trim().is_empty() {
        body = vec![vec![gap(INDENT), run("No notes.", Tok::Dim)]];
    }
    let left = o.left.clone().unwrap_or_else(|| form.left());
    // The keys that choose are letters in a field that is typed into, so
    // they are named only where they choose.
    let choice = (!form.focus.typed()).then(|| keymap::hint("event-form.choice"));
    // What is typed into a date is merged with what it held.
    let dated = matches!(form.focus, Field::Starts | Field::Ends).then_some(("14:30 or fri", "is enough"));
    let ids = ["event-form.notes", "event-form.save", "event-form.close"].map(keymap::hint);
    let keys: Vec<_> = [("Tab", "next field")].into_iter().chain(choice).chain(dated).chain(ids).collect();
    let idle = || hint(&keys);
    let command = asked.or_else(|| o.command.clone()).unwrap_or_else(idle);
    let foot = [o.panel.clone(), vec![Line::new(), status_row(width, &left, &o.right), command]].concat();
    frame(head, body, foot, height)
}

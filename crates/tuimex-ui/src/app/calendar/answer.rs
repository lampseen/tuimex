//! Answering an invitation (ADR-057): the question `i` asks of the
//! selected event, which is the one the reader asks of an invitation's
//! mail, and what each answer does. Both come to `answer_give`, the one
//! from the event a calendar holds and the other from the event the mail
//! carries.

use tuimex_core::calendar::{DAY, Event, Guest, Moment, Said, Standing, standing};
use tuimex_core::invitation::{self, Record};
use tuimex_core::{Action, Address, Answer, EventAnswer, EventId, MessageId};

use super::change::title;
use super::{DATES, PROPOSE, Panel, key_name};
use crate::app::{App, Mode};
use crate::calendar::change::{Item, Patch};
use crate::calendar::form::Target;
use crate::calendar::parts::{clock, day_short};
use crate::editor::Editor;
use crate::glyphs::DOT;
use crate::palette::Tok;
use crate::say;
use crate::span::{Line, run};

#[cfg(test)]
mod tests;

/// The invitation an answer is for.
#[derive(Clone, Debug)]
pub(in crate::app) struct Asked {
    /// The calendar that holds the event, or with `held` false the one
    /// the answer puts it in.
    at: Target,
    event: Event,
    held: bool,
    /// When it is, on the reader's clock.
    start: i64,
    end: i64,
    /// The mail it came in, where that is known.
    message: Option<MessageId>,
    in_mail: bool,
    /// The key or command that answers, as an error names it.
    name: &'static str,
}

/// The five answers by the words `:invite` takes, each with its key's
/// binding.
const ANSWERS: [(&str, &str); 5] = [
    ("accept", "invite.accept"),
    ("tentative", "invite.tentative"),
    ("decline", "invite.decline"),
    ("propose", "invite.propose"),
    ("ignore", "invite.ignore"),
];

impl App {
    /// The addresses of an account, its own first.
    fn mine(&self, at: &Target) -> Vec<Address> {
        let own = self.identities.iter().filter(|identity| identity.account == at.account);
        own.map(|identity| identity.address.clone()).collect()
    }

    /// Where an answer or an event goes, as its result says it: the
    /// account, and `local` when that is not the calendar its provider
    /// keeps (ADR-057).
    pub(super) fn kept_in(&self, at: &Target) -> String {
        let local = at.local || !self.cal.form.writable().contains(&at.account);
        if local { format!("{}, local", at.account.0) } else { at.account.0.clone() }
    }

    /// The selected event, as an invitation to answer.
    fn asked_of_event(&self, name: &'static str) -> Result<Asked, say::Said> {
        let Some(entry) = self.picked_entry() else { return Err(say::err(format!("{name}: no event is selected"))) };
        let came_in = self.cal.model.came_in.iter().find(|(uid, _)| *uid == entry.event.uid);
        Ok(Asked {
            at: Target { account: entry.account.clone(), local: entry.local },
            event: entry.event.clone(),
            held: true,
            start: entry.start,
            end: entry.end,
            message: came_in.map(|(_, id)| id.clone()),
            in_mail: false,
            name,
        })
    }

    /// The invitation of the message open in the reader: the event its
    /// calendar part carries, or the calendar's own copy of that event
    /// where the receiving account's calendar already holds one.
    fn asked_of_message(&self, name: &'static str) -> Result<Asked, say::Said> {
        let unread = || say::err(format!("{name}: this message's invitation could not be read"));
        let id = self.reading.id().ok_or_else(unread)?;
        let invited = self.invited(id).ok_or_else(unread)?;
        let model = &self.cal.model;
        let same = |event: &&Event| event.uid == invited.event.uid && event.instead_of == invited.event.instead_of;
        let mut shelves = model.shelves.iter().filter(|shelf| shelf.account == id.account);
        let held = shelves.find_map(|shelf| Some((shelf.local, shelf.events.iter().find(same)?.clone())));
        let (local, event, held) = match held {
            Some((local, event)) => (local, event, true),
            None => (!self.cal.form.writable().contains(&id.account), invited.event.clone(), false),
        };
        let wall = |moment: &Moment| {
            let placed =
                moment.local_wall(&invited.zones, &model.zone).or(moment.local_wall(&model.zones, &model.zone));
            placed.unwrap_or(moment.wall())
        };
        let start = wall(&event.start);
        let end = event.end.as_ref().map_or(start, wall);
        let (at, message) = (Target { account: id.account.clone(), local }, Some(id.clone()));
        Ok(Asked { at, event, held, start, end, message, in_mail: true, name })
    }

    /// `i`: the question, of an event somebody asked the reader to.
    pub(super) fn answer_ask(&mut self) {
        let key = key_name("calendar.answer", 0);
        let asked = match self.asked_of_event(key) {
            Ok(asked) => asked,
            Err(said) => return self.say(said),
        };
        let invited = self.picked_entry().is_some_and(|entry| entry.standing.is_some());
        match invitation::answer(&asked.event, &self.mine(&asked.at), &Answer::Accept) {
            _ if !invited => self.say(say::err(format!("{key}: nobody asked for an answer to this event"))),
            Err(why) => self.say(say::err(format!("{key}: {why}"))),
            Ok(_) => self.cal.panel = Panel::Answer,
        }
    }

    /// A key of the `invite` table, pressed at the question: `name` is the
    /// key or command that asked it, and `in_mail` whether it was asked of
    /// the message open in the reader.
    pub(in crate::app) fn answer_run(&mut self, name: &'static str, id: &str, in_mail: bool) {
        let asked = if in_mail { self.asked_of_message(name) } else { self.asked_of_event(name) };
        let asked = match asked {
            Ok(asked) => asked,
            Err(said) => return self.say(said),
        };
        match id {
            "invite.accept" => self.answer_give(asked, Answer::Accept, None),
            "invite.tentative" => self.answer_give(asked, Answer::Tentative, None),
            "invite.decline" => self.answer_give(asked, Answer::Decline, None),
            "invite.ignore" => self.answer_give(asked, Answer::Ignore, None),
            _ => match invitation::answer(&asked.event, &self.mine(&asked.at), &Answer::Accept) {
                Err(why) => self.say(say::err(format!("{name}: {why}"))),
                Ok(_) => {
                    self.cal.answering = Some(asked);
                    self.mode = Mode::Prompt(PROPOSE, Editor::default());
                }
            },
        }
    }

    /// `:invite <answer>`.
    pub(in crate::app) fn answer_command(&mut self, answer: &str, in_mail: bool) {
        match ANSWERS.iter().find(|(word, _)| *word == answer) {
            Some((_, id)) => self.answer_run(":invite", id, in_mail),
            None => self.offer(say::err(":invite: it takes accept, tentative, decline, propose or ignore"), "invite "),
        }
    }

    /// The time `words` propose for the invitation being answered, from
    /// its start to its end on the reader's clock: the day they name, at
    /// the time they give or else the event's own, for as long as it lasts.
    fn proposal(&self, words: &str) -> Option<(i64, i64)> {
        let asked = self.cal.answering.as_ref()?;
        let named = tuimex_core::calendar::date::parse(words, self.cal.model.today(), asked.start.div_euclid(DAY))?;
        let start = named.day * DAY + named.minute.map_or(asked.start.rem_euclid(DAY), |minute| minute * 60);
        Some((start, start + asked.end - asked.start))
    }

    /// The left of the status line while a time is typed: what the words
    /// were taken to mean, before anything is sent.
    pub(super) fn proposes(&self, words: &str) -> Line {
        let Some((start, end)) = self.proposal(words) else { return Line::new() };
        let (first, last) = (start.div_euclid(DAY), end.div_euclid(DAY));
        let whole = self.cal.answering.as_ref().is_some_and(|asked| asked.event.start.is_day());
        let said = match (whole, first == last) {
            (true, _) => day_short(first),
            (false, true) => format!("{} {} to {}", day_short(first), clock(start), clock(end)),
            (false, false) => format!("{} {} to {} {}", day_short(first), clock(start), day_short(last), clock(end)),
        };
        vec![run("proposes", Tok::Dim), run(format!(" {said}"), Tok::Fg)]
    }

    /// Enter after `Propose: `. A new time is an instant, so the organizer
    /// is asked for the same moment wherever they are; a whole day and a
    /// time with no zone stay what they are.
    pub(super) fn propose(&mut self, words: &str) {
        let proposal = self.proposal(words);
        let Some(asked) = self.cal.answering.take() else { return };
        let Some((start, end)) = proposal else {
            let said = say::err(format!("{}: not a date: \"{}\"", asked.name, words.trim()));
            return self.say(said.key(DATES));
        };
        let zone = &self.cal.model.zone;
        let moment = |wall: i64| match &asked.event.start {
            Moment::Day(_) => Moment::Day(wall.div_euclid(DAY)),
            Moment::Wall { zone: None, .. } => Moment::Wall { wall, zone: None },
            _ => Moment::Utc(wall - zone.offset_at_wall(wall).unwrap_or(0)),
        };
        let answer = Answer::Propose { start: moment(start), end: asked.event.end.as_ref().map(|_| moment(end)) };
        self.answer_give(asked, answer, Some(start));
    }

    /// Gives `answer`: the calendar shows it at once, and the ask that
    /// mails the organizer and writes the calendar goes out. `proposed` is
    /// the start of a time proposed, on the reader's clock.
    fn answer_give(&mut self, asked: Asked, answer: Answer, proposed: Option<i64>) {
        let mine = self.mine(&asked.at);
        let answering = match invitation::answer(&asked.event, &mine, &answer) {
            Ok(answering) => answering,
            Err(why) => return self.say(say::err(format!("{}: {why}", asked.name))),
        };
        let (account, uid, what) = (asked.at.account.clone(), asked.event.uid.clone(), title(&asked.event));
        let held =
            |event: Event| Item::Event { at: asked.at.clone(), event: Box::new(event), held: true, unsaved: false };
        let mut items = Vec::new();
        let mut restored = None;
        let said = match &answering.record {
            Record::Said(said) => {
                let mut event = asked.event.clone();
                let me = |guest: &&mut Guest| guest.who.email.eq_ignore_ascii_case(&answering.me.email);
                match event.guests.iter_mut().find(me) {
                    Some(guest) => guest.said = *said,
                    None => event.guests.push(Guest { who: answering.me.clone(), said: *said }),
                }
                items.extend([held(event), Item::Proposed { account, uid, start: None }]);
                let how = match said {
                    Said::Tentative => "Tentatively accepted",
                    Said::Declined => "Declined",
                    _ => "Accepted",
                };
                // The organizer may not know an address they did not ask.
                let from = if answering.uninvited { format!(" as {}", answering.me.email) } else { String::new() };
                say::ok(format!("{how} {what}")).then(format!("in {}{from} {DOT} u to undo", self.kept_in(&asked.at)))
            }
            Record::Proposed { .. } => {
                let start = proposed.unwrap_or(asked.start);
                items.extend((!asked.held).then(|| held(asked.event.clone())));
                items.push(Item::Proposed { account, uid, start: Some(start) });
                let who = asked
                    .event
                    .organizer
                    .as_ref()
                    .map_or_else(String::new, |o| o.name.clone().unwrap_or(o.email.clone()));
                let time = if asked.event.start.is_day() { String::new() } else { format!(" {}", clock(start)) };
                say::ok(format!("Proposed {}{time} to {who}", day_short(start.div_euclid(DAY)))).key("u to undo")
            }
            Record::Ignored => {
                items.push(Item::Ignored { account, uid, ignored: true });
                restored = Some(format!("{what} is back"));
                say::ok(format!("Ignored {what}")).then(format!("nothing sent {DOT} u to undo"))
            }
        };
        let emails: Vec<&str> = mine.iter().map(|me| me.email.as_str()).collect();
        let waited = matches!(standing(&asked.event, &emails), None | Some(Standing::Needs));
        let answered = if waited { format!("{what} is not answered") } else { format!("{what} is as it was") };
        let event = EventId {
            occurrence: asked.event.instead_of.clone(),
            account: asked.at.account,
            local: asked.at.local,
            uid: asked.event.uid,
        };
        let action = Action::AnswerEvent(Box::new(EventAnswer { event, answer, message: asked.message }));
        self.change(asked.name, action, Patch(items), restored.unwrap_or(answered), asked.in_mail);
        self.say(said);
    }
}

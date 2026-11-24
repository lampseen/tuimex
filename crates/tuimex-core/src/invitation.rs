//! The pure rules of an invitation (ADR-057): who answers it, to whom and
//! in what words, what an event the user made sends its guests, and what a
//! later message for the same event may change. Nothing here sends or
//! stores anything; each function says what should happen.
//!
//! Every word a person reads in such a mail is written in this module, so
//! that the interface and the mail say the same thing.

use crate::calendar::{DAY, Event, Method, Moment, Said, civil_from_days, weekday_of};
use crate::{Address, Answer};

#[cfg(test)]
mod tests;

/// The calendar message a mail carries beside its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carries {
    Reply(Said),
    Counter { start: Moment, end: Option<Moment> },
    Request,
    Cancel,
}

impl Carries {
    pub fn method(&self) -> Method {
        match self {
            Carries::Reply(_) => Method::Reply,
            Carries::Counter { .. } => Method::Counter,
            Carries::Request => Method::Request,
            Carries::Cancel => Method::Cancel,
        }
    }
}

/// One mail an event makes: who sends it, everyone it goes to, its words,
/// and the calendar message that rides with them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub from: Address,
    pub to: Vec<Address>,
    pub subject: String,
    pub body: String,
    pub carries: Carries,
}

/// What the receiving account's calendar should hold once the answer is
/// given (ADR-057). Writing it is the caller's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    /// The event, with `me` among its guests having said this.
    Said(Said),
    /// The event, shown as proposed for this time until the organizer
    /// sends a new version.
    Proposed { start: Moment, end: Option<Moment> },
    /// Not drawn, and no longer waiting for an answer.
    Ignored,
}

/// What answering an invitation comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answering {
    /// `None` for an invitation ignored.
    pub mail: Option<Outgoing>,
    pub record: Record,
    /// The identity whose answer this is.
    pub me: Address,
    /// None of the account's identities is among the guests, so the
    /// account's first answers. The interface says so: the organizer may
    /// not know that address.
    pub uninvited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unanswerable {
    #[error("the account has no address to answer from")]
    NoIdentity,
    #[error("the invitation names no organizer to answer")]
    NoOrganizer,
    #[error("you organize this event")]
    Organized,
    #[error("this event is not one you organize")]
    NotOrganized,
    #[error("the event has no guests")]
    NoGuests,
}

/// What answering `event` with `answer` comes to, for an account whose
/// addresses are `identities`, its own first. The reply goes to the
/// organizer alone, whoever else the invitation lists: it is the
/// organizer's to tell the rest.
pub fn answer(event: &Event, identities: &[Address], answer: &Answer) -> Result<Answering, Unanswerable> {
    let invited = identities.iter().find(|me| event.guests.iter().any(|g| same(&g.who, me)));
    let me = invited.or(identities.first()).ok_or(Unanswerable::NoIdentity)?.clone();
    let uninvited = invited.is_none();
    let summary = summary(event);
    let when = when(&event.start, event.end.as_ref());
    let replied = |said: Said, word: &str, sentence: &str| {
        (
            format!("{word}: {summary}"),
            format!("{sentence} {summary}, {when}."),
            Carries::Reply(said),
            Record::Said(said),
        )
    };
    let (subject, body, carries, record) = match answer {
        Answer::Ignore => return Ok(Answering { mail: None, record: Record::Ignored, me, uninvited }),
        Answer::Accept => replied(Said::Accepted, "Accepted", "I will be at"),
        Answer::Tentative => replied(Said::Tentative, "Tentative", "I may be at"),
        Answer::Decline => replied(Said::Declined, "Declined", "I will not be at"),
        Answer::Propose { start, end } => (
            format!("Proposed new time: {summary}"),
            format!("Could we move this to {}? It is currently {when}.", self::when(start, end.as_ref())),
            Carries::Counter { start: start.clone(), end: end.clone() },
            Record::Proposed { start: start.clone(), end: end.clone() },
        ),
    };
    let organizer = event.organizer.clone().filter(|o| !o.email.trim().is_empty()).ok_or(Unanswerable::NoOrganizer)?;
    if identities.iter().any(|mine| same(mine, &organizer)) {
        return Err(Unanswerable::Organized);
    }
    let mail = Outgoing { from: me.clone(), to: vec![organizer], subject, body, carries };
    Ok(Answering { mail: Some(mail), record, me, uninvited })
}

/// The invitation to an event the user organizes, for every guest.
pub fn invite(event: &Event, identities: &[Address]) -> Result<Outgoing, Unanswerable> {
    let summary = summary(event);
    let body = format!("You are invited to {summary}, {}.", when(&event.start, event.end.as_ref()));
    to_guests(event, identities, format!("Invitation: {summary}"), body, Carries::Request)
}

/// The cancellation of an event the user organizes, for every guest.
pub fn cancel(event: &Event, identities: &[Address]) -> Result<Outgoing, Unanswerable> {
    let summary = summary(event);
    let body = format!("{summary}, {}, is cancelled.", when(&event.start, event.end.as_ref()));
    to_guests(event, identities, format!("Cancelled: {summary}"), body, Carries::Cancel)
}

fn to_guests(
    event: &Event,
    identities: &[Address],
    subject: String,
    body: String,
    carries: Carries,
) -> Result<Outgoing, Unanswerable> {
    let organizer = event.organizer.as_ref().ok_or(Unanswerable::NotOrganized)?;
    let from = identities.iter().find(|me| same(me, organizer)).ok_or(Unanswerable::NotOrganized)?.clone();
    let mut to: Vec<Address> = Vec::new();
    for guest in &event.guests {
        let mine = identities.iter().any(|me| same(me, &guest.who));
        if !mine && !to.iter().any(|already| same(already, &guest.who)) {
            to.push(guest.who.clone());
        }
    }
    if to.is_empty() {
        return Err(Unanswerable::NoGuests);
    }
    Ok(Outgoing { from, to, subject, body, carries })
}

/// What a later message for an event already held asks, and whether it is
/// believed (ADR-057). It is mail, and mail is hostile: a change is taken
/// only from whoever the held event says may make it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Later {
    /// The held event is replaced by the incoming one.
    Update,
    /// The held event is cancelled.
    Strike,
    /// One guest of an event the user organizes has answered.
    Answered {
        guest: Address,
        said: Said,
    },
    /// One guest of an event the user organizes asks for another time.
    /// It is shown, and changes nothing.
    Proposed {
        guest: Address,
        start: Moment,
        end: Option<Moment>,
    },
    Nothing(Unchanged),
}

/// Why a later message changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unchanged {
    /// Another UID, or another occurrence of the same one.
    OtherEvent,
    /// A version the held event already has, or an older one.
    NotNewer,
    /// It changes the event and does not come from its organizer.
    NotFromOrganizer,
    /// It answers for a guest and does not come from that guest, or from
    /// anyone the event invites.
    NotFromGuest,
    /// It answers an event the user does not organize.
    NotOrganizedHere,
    /// Its method asks for nothing to change.
    Unasked,
}

/// What `incoming`, a calendar part with `method` in a message from
/// `sender`, may do to `held`. `mine` are the addresses of the account
/// that holds the event.
pub fn later(held: &Event, mine: &[Address], method: Method, incoming: &Event, sender: &Address) -> Later {
    // Two events without a UID are not thereby the same event.
    if held.uid.is_empty() || incoming.uid != held.uid || incoming.instead_of != held.instead_of {
        return Later::Nothing(Unchanged::OtherEvent);
    }
    let from_organizer = held.organizer.as_ref().is_some_and(|o| same(o, sender))
        && incoming.organizer.as_ref().is_none_or(|o| held.organizer.as_ref().is_some_and(|h| same(h, o)));
    let organized_here = held.organizer.as_ref().is_some_and(|o| mine.iter().any(|me| same(me, o)));
    let guest = held.guests.iter().find(|g| same(&g.who, sender));
    let answered = incoming.guests.iter().find(|g| same(&g.who, sender));
    match method {
        Method::Request | Method::Cancel if !from_organizer => Later::Nothing(Unchanged::NotFromOrganizer),
        Method::Request if incoming.sequence > held.sequence => Later::Update,
        // A cancellation carries the version it ends, or the one after;
        // one below the version held was sent before that version was.
        Method::Cancel if incoming.sequence >= held.sequence => Later::Strike,
        Method::Request | Method::Cancel => Later::Nothing(Unchanged::NotNewer),
        Method::Reply | Method::Counter if !organized_here => Later::Nothing(Unchanged::NotOrganizedHere),
        Method::Reply | Method::Counter => match (guest, answered) {
            (Some(_), Some(_)) if incoming.sequence < held.sequence => Later::Nothing(Unchanged::NotNewer),
            (Some(guest), Some(answer)) if method == Method::Reply => {
                Later::Answered { guest: guest.who.clone(), said: answer.said }
            }
            (Some(guest), Some(_)) => {
                Later::Proposed { guest: guest.who.clone(), start: incoming.start.clone(), end: incoming.end.clone() }
            }
            _ => Later::Nothing(Unchanged::NotFromGuest),
        },
        Method::Other => Later::Nothing(Unchanged::Unasked),
    }
}

/// Two addresses are one person when their mailboxes match, in any case.
fn same(a: &Address, b: &Address) -> bool {
    let (a, b) = (a.email.trim(), b.email.trim());
    !a.is_empty() && a.eq_ignore_ascii_case(b)
}

/// An event's title as one line: it is hostile text, and it goes into a
/// subject, where a line break would start a header of the sender's
/// choosing.
fn summary(event: &Event) -> String {
    let flat = one_line(&event.summary);
    if flat.is_empty() { "(no title)".to_string() } else { flat }
}

fn one_line(text: &str) -> String {
    let spaced: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// `Thu 26 Nov 2026`.
fn date(day: i64) -> String {
    let (year, month, dom) = civil_from_days(day);
    let weekday = usize::try_from(weekday_of(day)).ok().and_then(|w| WEEKDAYS.get(w)).unwrap_or(&"");
    let month = usize::try_from(month - 1).ok().and_then(|m| MONTHS.get(m)).unwrap_or(&"");
    format!("{weekday} {dom} {month} {year}")
}

fn clock(wall: i64) -> String {
    let seconds = wall.rem_euclid(DAY);
    format!("{:02}:{:02}", seconds / 3_600, seconds % 3_600 / 60)
}

/// The clock a time is written on, as the sentence names it; `None` for a
/// floating time, which is the same wherever it is read.
fn zone(moment: &Moment) -> Option<String> {
    match moment {
        Moment::Utc(_) => Some("UTC".to_string()),
        Moment::Wall { zone: Some(zone), .. } => Some(one_line(zone)).filter(|z| !z.is_empty()),
        _ => None,
    }
}

/// When an event is, as the organizer wrote it, in words:
/// `Thu 26 Nov 2026, 10:00 to 11:30 (Europe/Oslo)`. The organizer's own
/// clock is used, not the reader's, so the sentence means one thing to
/// both.
pub fn when(start: &Moment, end: Option<&Moment>) -> String {
    let named = |m: &Moment| zone(m).map_or_else(String::new, |z| format!(" ({z})"));
    let at = |m: &Moment| format!("{}, {}", date(m.wall().div_euclid(DAY)), clock(m.wall()));
    match (start, end) {
        (Moment::Day(first), Some(Moment::Day(after))) if after - 1 > *first => {
            format!("{} to {}", date(*first), date(after - 1))
        }
        (Moment::Day(day), _) => date(*day),
        (_, None) => format!("{}{}", at(start), named(start)),
        (_, Some(end)) if end.is_day() => {
            format!("{}{} to {}", at(start), named(start), date(end.wall().div_euclid(DAY)))
        }
        (_, Some(end)) => {
            let same_day = start.wall().div_euclid(DAY) == end.wall().div_euclid(DAY);
            let until = if same_day { clock(end.wall()) } else { at(end) };
            if zone(start) == zone(end) {
                format!("{} to {until}{}", at(start), named(start))
            } else {
                format!("{}{} to {until}{}", at(start), named(start), named(end))
            }
        }
    }
}

//! What the user asks for and what the engine answers (PLAN §3).

use std::path::PathBuf;
use std::time::Duration;

use crate::calendar::Moment;
use crate::{AccountId, AgentAsk, Attached, Body, Draft, FolderId, MessageId, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Archive,
    Trash,
    Move(FolderId),
    Flag(bool),
    Seen(bool),
    Undo,
    Compose(Start),
    Send,
    SaveDraft,
    Discard,
    OpenLink(usize),
    SaveAttachment(usize, PathBuf),
    OpenAttachment(usize),
    Browser {
        remote: bool,
    },
    Invite(Answer),
    Unsubscribe,
    Sync {
        all: bool,
    },
    /// One account's Trash, for good.
    EmptyTrash(AccountId),
    /// An Archive folder for an account whose server has none, which the
    /// user has just agreed to (PLAN §4.1). It is made on the server when
    /// the first message is moved there.
    CreateArchive(AccountId),
    /// A Trash folder, agreed to and made in the same way (ADR-047).
    CreateTrash(AccountId),
    /// What compose holds of the draft `on` names. It comes before the
    /// `SaveDraft` or `Send` of that draft, which name a message and do not
    /// carry one.
    Written(Box<Draft>),
    /// The body, to be edited in `$EDITOR`; answered by `Event::Edited`.
    Edit(String),
    /// The start of a path, for the files it could go on to name; answered
    /// by `Event::Paths`.
    Paths(String),
    /// A file to attach, by its path; answered by `Event::Attached`.
    Attach(String),
    /// Text for the configured agent command (ADR-031); answered by
    /// `Event::AgentText` naming the `Ask`'s serial.
    Agent(AgentAsk),
    /// Stop the agent command that is running.
    AgentCancel,
    /// The body of the message, for the reader. Answered by `BodyReady`,
    /// and by `Checked` when the message is signed or encrypted.
    Read,
    /// `:agent link <n>` (ADR-031): what the agent makes of the message's
    /// link of that number. Answered by `AgentAsked` once the redacted
    /// text has left, then by `AgentAdvice`, or by `ActionFailed`.
    AgentLink(usize),
    /// A query in the grammar of PLAN §6.1, asked of every message of
    /// every account wherever it is filed, its whole body included.
    /// Answered by `Event::Found`.
    Search(String),
    /// An answer to an invitation (ADR-057): the mail to its organizer,
    /// when the answer sends one, from the account the invitation came
    /// to, and the event with the answer in that account's calendar.
    AnswerEvent(Box<EventAnswer>),
    /// The event is deleted, or with an occurrence named that one alone
    /// (ADR-055).
    DeleteEvent(Box<EventId>),
    /// An event out of the form, to be stored (ADR-055) and, when it says
    /// so, mailed to its guests (ADR-057).
    SaveEvent(Box<Stored>),
    /// The event's link of that number, counted from 1 in the order
    /// `calendar::links_of` gives them. It is the user asking for that one
    /// thing (ADR-008), assessed and confirmed where that is called for
    /// (ADR-018).
    OpenEventLink(Box<EventId>, usize),
    /// What the agent makes of that link of the event (ADR-031); answered
    /// as `AgentLink` is.
    AgentEventLink(Box<EventId>, usize),
    /// Takes back what the calendar ask of this serial did: the calendar
    /// as it was before, and a mail of it still waiting in the Outbox back
    /// to Drafts (ADR-040, ADR-057).
    UndoEvent(u64),
    /// Every event tuimex alone holds, written as `.ics` to this path, or
    /// with an empty one to wherever downloads go (ADR-055). Answered by
    /// `Event::Saved`.
    Export(PathBuf),
}

/// An event as an ask names it: the calendar that holds it, or is to, and
/// its UID there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventId {
    pub account: AccountId,
    /// The calendar tuimex alone holds for the account; otherwise the
    /// account's own, at its provider.
    pub local: bool,
    pub uid: String,
    /// The start of the one occurrence meant, as the event's own calendar
    /// writes it: what an `EXDATE` or a `RECURRENCE-ID` carries. `None`
    /// for the event whole.
    pub occurrence: Option<Moment>,
}

/// An answer, and the invitation it is given to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventAnswer {
    /// The event, in the calendar that holds it; one no calendar holds yet
    /// is named in the calendar of the receiving account the answer puts
    /// it in.
    pub event: EventId,
    pub answer: Answer,
    /// The mail the invitation came in, when that is known: the answer
    /// threads under it, and an event no calendar holds is read from it.
    pub message: Option<MessageId>,
}

/// What the event form hands over to be kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub account: AccountId,
    /// As `EventId::local`: which of the account's calendars it goes to.
    pub local: bool,
    /// A new event has no UID, and whoever stores it makes one. One whose
    /// `instead_of` is set stands in for that occurrence of the event
    /// `replaces` names, which stays.
    pub event: crate::calendar::Event,
    /// The event this takes the place of, which may be in another
    /// calendar; `None` for a new one.
    pub replaces: Option<EventId>,
    /// The guests are mailed the invitation, which the user has agreed to.
    pub invite: bool,
}

/// An action and the messages it takes, as the UI hands it to whoever
/// carries it out. The UI has already shown its effect (ADR-015), so the
/// answer it waits for is only a refusal, which names `serial` (ADR-040).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// Counts up from 1 within a run.
    pub serial: u64,
    pub action: Action,
    /// For `Undo`, the messages put back: the newest `Ask` carried out
    /// over exactly these is the one reversed, and a message waiting in
    /// the Outbox goes back to Drafts.
    pub on: Vec<MessageId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    New,
    Reply,
    ReplyAll,
    Forward,
    FromDraft,
    /// A `mailto:` URL, from a link or `:unsubscribe`.
    MailTo(String),
}

/// The five answers to an invitation (ADR-057).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Accept,
    Tentative,
    Decline,
    /// Another time, sent to the organizer as a `COUNTER`.
    Propose {
        start: Moment,
        end: Option<Moment>,
    },
    /// Nothing is sent; the invitation stops waiting for an answer.
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Synced {
        account: AccountId,
        folder: FolderId,
        changed: Vec<MessageId>,
    },
    SyncError {
        account: AccountId,
        msg: String,
        /// None when trying again cannot help, as after a refused sign-in
        /// (PLAN §4.2).
        retry_in: Option<Duration>,
    },
    BodyReady {
        id: MessageId,
        body: Box<Body>,
    },
    BodyFailed {
        id: MessageId,
        reason: String,
    },
    ActionDone {
        summary: String,
    },
    /// What `EmptyTrash` did, which only whoever holds the store can
    /// count: `kept` are the messages trashed so lately that the server
    /// has not numbered them in its Trash, and could not be told to
    /// delete (ADR-041).
    Emptied {
        serial: u64,
        account: AccountId,
        deleted: usize,
        kept: usize,
    },
    /// An attachment was written to `path`, which is as a person would
    /// write it: `~` for their home directory.
    Saved {
        path: String,
    },
    /// `hint` is the next useful key (PLAN §4.2). `serial` is the `Ask`
    /// refused, whose effect the UI then takes back (ADR-014). It may come
    /// long after the `Ask`: a server refuses when it is next reached.
    ActionFailed {
        serial: u64,
        reason: String,
        hint: String,
    },
    /// What the engine has to say that answers no `Ask` of this run
    /// (ADR-040): a change journaled by an earlier run that the server now
    /// refuses, a copy of sent mail it would not keep, originals it will
    /// purge by itself. The store already holds the outcome.
    Notice {
        account: AccountId,
        error: bool,
        text: String,
        hint: String,
    },
    /// How far the first sync of a folder has got, in messages.
    FirstSync {
        account: AccountId,
        folder: FolderId,
        done: u32,
        total: u32,
    },
    Sent {
        id: MessageId,
        /// The messages it answers, which are marked answered from now.
        answered: Vec<MessageId>,
    },
    SendFailed {
        id: MessageId,
        reason: String,
    },
    NewMail {
        account: AccountId,
        count: u32,
    },
    /// What `$EDITOR` left, or why it could not be run.
    Edited {
        text: Result<String, String>,
    },
    Paths {
        typed: String,
        found: Vec<String>,
    },
    Attached {
        file: Result<Attached, String>,
    },
    /// What the agent command printed, or the design's reason it failed.
    AgentText {
        serial: u64,
        text: Result<String, String>,
    },
    /// What `gpg` made of a signed or encrypted message (ADR-020).
    Checked {
        id: MessageId,
        check: Check,
    },
    /// The text for the `Ask` of that serial has left for the agent, with
    /// `replaced` standing in placeholders: "1 person, 2 addresses".
    AgentAsked {
        serial: u64,
        replaced: String,
    },
    /// The agent's answer to `AgentLink`: `advice` completes "Agent advises
    /// …", and `why` is its reason. It is shown, and decides nothing.
    AgentAdvice {
        serial: u64,
        advice: String,
        why: String,
    },
    /// What the `Search` of that serial matched, newest first, and how
    /// many messages were looked through.
    Found {
        serial: u64,
        hits: Vec<Hit>,
        searched: u32,
    },
    /// The account's calendar was read whole; `changed` is whether what
    /// is held of it is now different.
    CalendarRead {
        account: AccountId,
        changed: bool,
    },
    /// The account's calendar could not be read. `reason` never holds a
    /// feed's address (ADR-056). What was held stays, read at `read_at`,
    /// or never.
    CalendarFailed {
        account: AccountId,
        reason: String,
        read_at: Option<Timestamp>,
    },
}

/// One message a search matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: MessageId,
    /// The words around what matched in the body, when the body matched.
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// `signer` is the name on the key.
    Good { signer: String, key: String },
    /// `signer` is the address the message claims to be from.
    Bad { signer: String },
    /// Signed with a key `gpg` does not have.
    UnknownKey { key: String },
    /// Opened with the reader's own key.
    Decrypted { key: String },
}

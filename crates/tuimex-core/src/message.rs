//! Messages, folders and threads as every surface sees them (PLAN §3).

/// The config's account name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(pub String);

/// What every surface uses to name a message: the UI's cursor memory, the
/// journal, the agent command line.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId {
    pub account: AccountId,
    /// The Maildir unique name, which a flag change does not alter.
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FolderId(pub String);

/// Seconds since the Unix epoch, UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(pub i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
}

/// What the message's structure says. Whether a signature is good is the
/// engine's to find out, through `gpg` (ADR-020).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    None,
    Signed,
    Encrypted,
    Both,
}

/// Who wrote a message into the store. An agent's draft says so wherever it
/// is shown (ADR-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Server,
    Local,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub id: MessageId,
    pub folder: FolderId,
    pub date: Timestamp,
    pub from: Vec<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    /// Only a message written here names its blind copies: a draft, or the
    /// copy of what was sent.
    pub bcc: Vec<Address>,
    /// Where the sender asks for answers to go, when not to `from`.
    pub reply_to: Vec<Address>,
    pub subject: String,
    pub message_id: Option<String>,
    /// `References`, then `In-Reply-To`: what threading follows.
    pub references: Vec<String>,
    pub flags: Flags,
    pub size: u32,
    pub has_attachments: bool,
    pub security: Security,
    pub origin: Origin,
}

/// What a message's own headers and structure say: the part of an
/// `Envelope` that `tuimex-content` can know. The rest (its id, folder,
/// flags, size and origin) is the store's and the server's to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// `None` when the message has no date that parses, so the caller can
    /// use the time it arrived instead of the epoch.
    pub date: Option<Timestamp>,
    pub from: Vec<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub reply_to: Vec<Address>,
    pub subject: String,
    pub message_id: Option<String>,
    /// `References`, then `In-Reply-To`: what threading follows.
    pub references: Vec<String>,
    pub has_attachments: bool,
    pub security: Security,
}

/// What `tuimex-content` returns for one message: the only form in which a
/// message's body reaches the reader or the agent (ADR-007, ADR-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    /// The reader's text, with each link replaced by `[n]`.
    pub text: String,
    pub links: Vec<Link>,
    pub attachments: Vec<Attachment>,
    pub invite: Option<Invite>,
    /// Hosts the HTML wanted to load from. None was contacted (ADR-008).
    pub blocked_remote: Vec<String>,
    /// How many pictures those hosts would have served.
    pub remote_images: usize,
    /// The hosts the browser copy loads pictures from after a yes, and how
    /// many pictures that is: what `B` asks about (ADR-009). A stylesheet,
    /// a script or a font is never loaded, so its host is not here; nor
    /// is an image that is there to report a reading, which `trackers`
    /// names and no answer loads.
    pub image_hosts: Vec<String>,
    pub loadable_images: usize,
    /// Tracking services recognised by name.
    pub trackers: Vec<String>,
    pub unsubscribe: Option<Unsubscribe>,
    /// The attached pictures the reader shows in the text. Whoever has the
    /// bytes fills this in; `tuimex-content` decodes no image.
    pub pictures: Vec<Picture>,
}

/// An attached picture, by its place in `Body::attachments`, and its size
/// in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picture {
    pub index: usize,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub n: usize,
    pub url: String,
    /// The text the author put on the link, which may name another host.
    pub shown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub index: usize,
    pub name: String,
    pub mime: String,
    pub size: u32,
    pub inline: bool,
}

/// A `text/calendar` part, as far as the card shows it (ADR-021).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// The event's own id, which a `REPLY` has to carry back.
    pub uid: String,
    pub summary: String,
    pub start: EventTime,
    pub end: Option<EventTime>,
    pub location: Option<String>,
    pub organizer: Option<Address>,
    pub attendees: Vec<Address>,
    /// The recurrence in words, or "repeats".
    pub recurrence: Option<String>,
}

/// When an invitation says something happens. Only `At` can be shown in the
/// reader's own zone (ADR-021); the others are shown as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventTime {
    At(Timestamp),
    /// A wall-clock time, held as if it were UTC, in a zone the invitation
    /// named without defining, or did not name.
    Local {
        wall: Timestamp,
        zone: Option<String>,
    },
    /// A whole day: midnight UTC of its date.
    Day(Timestamp),
}

/// `List-Unsubscribe`, reduced to the two schemes tuimex will act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsubscribe {
    Mailto(String),
    Https(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Trash,
    Junk,
    Outbox,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub account: AccountId,
    pub name: String,
    pub role: Role,
    pub unseen: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub root: MessageId,
    pub members: Vec<ThreadNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadNode {
    pub id: MessageId,
    /// Replies deep: 0 for the root.
    pub depth: u16,
}

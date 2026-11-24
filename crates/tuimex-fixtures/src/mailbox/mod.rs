//! The fixture mailbox: two accounts with the five folders
//! `docs/history/2026-11-23-design-brief.md` asks for. Each message is its raw bytes beside
//! the fields a correct parse of them gives, so a crate that may not parse
//! mail (ADR-007) can still be tested against real files.

use tuimex_core::{Address, Flags, Origin, Role, Security, Timestamp};

/// The moment the mailbox is looked at: 2026-11-23 12:00 UTC.
pub const NOW: Timestamp = Timestamp(1_795_435_200);

/// 2026-10-24 11:00 UTC, the hour before the mailbox's thirty days begin,
/// so a message `n` days in is just outside "the last `30 - n` days".
const FIRST: i64 = NOW.0 - 30 * 86_400 - 3600;

/// Every account has every one of these, each with a UIDVALIDITY.
pub const FOLDERS: [(&str, Role, u32); 5] = [
    ("INBOX", Role::Inbox, 11),
    ("Sent", Role::Sent, 12),
    ("Drafts", Role::Drafts, 13),
    ("Archive", Role::Archive, 14),
    ("Trash", Role::Trash, 15),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub account: &'static str,
    pub folder: &'static str,
    pub key: String,
    /// None for a message the server has not been given yet.
    pub uid: Option<u32>,
    pub flags: Flags,
    pub origin: Origin,
    pub date: Timestamp,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: &'static str,
    pub message_id: String,
    pub references: Vec<String>,
    pub has_attachments: bool,
    pub has_invite: bool,
    pub has_unsubscribe: bool,
    pub security: Security,
    /// What a reader would see of the body: nothing, when it is encrypted.
    pub text: &'static str,
    /// Minutes after `FIRST`.
    minute: i64,
}

fn who(name: &str, email: &str) -> Address {
    Address { name: Some(name.to_string()), email: email.to_string() }
}

fn ada(account: &str) -> Address {
    who("Ada Lovelace", if account == "work" { "ada@example.org" } else { "ada@example.com" })
}

impl Mail {
    /// A read message from `from` to the account's owner, `day` days and
    /// `minute` minutes after the mailbox's first day.
    fn new(at: (&'static str, &'static str, u32), day: i64, from: Address, subject: &'static str) -> Mail {
        let (account, folder, uid) = at;
        let minute = day * 1440 + i64::from(uid);
        let sent = matches!(folder, "Sent" | "Drafts");
        Mail {
            account,
            folder,
            key: format!("{}.M{uid}P1.fixture", FIRST + minute * 60),
            uid: Some(uid),
            flags: Flags { seen: true, draft: folder == "Drafts", ..Flags::default() },
            origin: Origin::Server,
            date: Timestamp(FIRST + minute * 60),
            to: vec![if sent { who("Grace Hopper", "grace@example.net") } else { ada(account) }],
            from,
            cc: Vec::new(),
            subject,
            message_id: format!("<{minute}.{uid}@{account}.example.net>"),
            references: Vec::new(),
            has_attachments: false,
            has_invite: false,
            has_unsubscribe: false,
            security: Security::None,
            text: "",
            minute,
        }
    }

    fn text(mut self, text: &'static str) -> Mail {
        self.text = text;
        self
    }

    fn with(mut self, change: impl FnOnce(&mut Mail)) -> Mail {
        change(&mut self);
        self
    }

    fn answers(mut self, earlier: &Mail) -> Mail {
        self.references = earlier.references.iter().cloned().chain([earlier.message_id.clone()]).collect();
        self
    }

    /// The message as a server would hand it over.
    pub fn raw(&self) -> Vec<u8> {
        let list = |all: &[Address]| {
            let one = |a: &Address| format!("{} <{}>", a.name.as_deref().unwrap_or_default(), a.email);
            all.iter().map(one).collect::<Vec<_>>().join(", ")
        };
        let (day, minute) = (self.minute.div_euclid(1440), self.minute.rem_euclid(1440));
        // No fixture is dated after November.
        let (d, month) = if day < 8 { (24 + day, "Oct") } else { (day - 7, "Nov") };
        let mut head = format!(
            "From: {}\r\nTo: {}\r\nSubject: {}\r\nDate: {d} {month} 2026 {:02}:{:02}:00 +0000\r\nMessage-ID: {}\r\n",
            list(std::slice::from_ref(&self.from)),
            list(&self.to),
            self.subject,
            11 + minute / 60,
            minute % 60,
            self.message_id
        );
        if !self.cc.is_empty() {
            head += &format!("Cc: {}\r\n", list(&self.cc));
        }
        if let Some(parent) = self.references.last() {
            head += &format!("In-Reply-To: {parent}\r\nReferences: {}\r\n", self.references.join(" "));
        }
        if self.has_unsubscribe {
            head += "List-Unsubscribe: <mailto:leave@example.org>\r\n";
        }
        let plain = format!("Content-Type: text/plain; charset=utf-8\r\n\r\n{}\r\n", self.text);
        let extra = if self.has_attachments {
            "Content-Type: application/pdf\r\nContent-Disposition: attachment; filename=\"paper.pdf\"\r\n\
             Content-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n"
        } else if self.has_invite {
            "Content-Type: text/calendar; method=REQUEST\r\n\r\nBEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n\
             UID:book-club@example.net\r\nDTSTART:20261203T190000Z\r\nSUMMARY:Book club\r\nEND:VEVENT\r\n\
             END:VCALENDAR\r\n"
        } else {
            ""
        };
        let body = match self.security {
            Security::Encrypted | Security::Both => String::from(
                "Content-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=\"b\"\r\n\r\n\
                 --b\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n\
                 --b\r\nContent-Type: application/octet-stream\r\n\r\n-----BEGIN PGP MESSAGE-----\r\n\r\n\
                 c3ludGhldGlj\r\n-----END PGP MESSAGE-----\r\n--b--\r\n",
            ),
            Security::Signed => format!(
                "Content-Type: multipart/signed; protocol=\"application/pgp-signature\"; boundary=\"b\"\r\n\r\n\
                 --b\r\n{plain}--b\r\nContent-Type: application/pgp-signature\r\n\r\n\
                 -----BEGIN PGP SIGNATURE-----\r\n\r\nc3ludGhldGlj\r\n-----END PGP SIGNATURE-----\r\n--b--\r\n"
            ),
            Security::None if !extra.is_empty() => {
                format!("Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n--b\r\n{plain}--b\r\n{extra}--b--\r\n")
            }
            Security::None => plain,
        };
        format!("{head}MIME-Version: 1.0\r\n{body}").into_bytes()
    }
}

/// Every message in the mailbox, in no order a caller may rely on.
pub fn mailbox() -> Vec<Mail> {
    let grace = || who("Grace Hopper", "grace@example.net");
    let linus = || who("Linus Okafor", "linus@example.net");
    let unread = |m: &mut Mail| m.flags.seen = false;

    let trip = Mail::new(("personal", "INBOX", 2), 24, linus(), "Boat trip planning")
        .text("Shall we take the boat out on Saturday? I can bring the charts.")
        .with(|m| m.flags.answered = true);
    let trip_reply = Mail::new(("personal", "Sent", 1), 25, ada("personal"), "Re: Boat trip planning")
        .text("Saturday works. I will bring lunch.")
        .with(|m| m.to = vec![linus()])
        .answers(&trip);
    let trip_again = Mail::new(("personal", "INBOX", 6), 26, linus(), "Re: Boat trip planning")
        .text("Perfect. The harbour at nine, then.")
        .with(unread)
        .answers(&trip_reply);
    let invoice = Mail::new(("work", "INBOX", 1), 27, who("Billing", "billing@example.com"), "Quarterly invoice")
        .text("The invoice for the third quarter is attached. Payment is due in thirty days.")
        .with(unread)
        .with(|m| m.has_attachments = true);
    let standup = Mail::new(("work", "INBOX", 4), 29, who("Émile Durand", "emile@example.org"), "Standup moved")
        .text("Tomorrow's standup moves to ten. Same room.")
        .with(|m| (m.cc, m.flags.answered) = (vec![who("Platform Team", "team@example.org")], true));

    vec![
        Mail::new(("personal", "INBOX", 1), 2, who("The Weekly Compiler", "news@example.org"), "Issue 212: parsers")
            .text("This week: recursive descent, and why your grammar is ambiguous.")
            .with(|m| m.has_unsubscribe = true),
        Mail::new(("personal", "INBOX", 3), 25, grace(), "Photos from the lake")
            .text("Here are the photographs from the weekend.")
            .with(|m| (m.has_attachments, m.flags.flagged) = (true, true)),
        Mail::new(("personal", "INBOX", 4), 28, who("Book Club", "club@example.net"), "Invitation: Book club")
            .text("You are invited to the December meeting.")
            .with(unread)
            .with(|m| m.has_invite = true),
        Mail::new(("personal", "INBOX", 5), 30, grace(), "Lunch on Thursday?")
            .text("Are you free for lunch on Thursday? The café by the canal.")
            .with(unread),
        Mail::new(("personal", "Drafts", 1), 30, ada("personal"), "Notes for the trip")
            .text("Charts, lunch, spare rope.")
            .with(|m| (m.uid, m.origin) = (None, Origin::Local)),
        Mail::new(("personal", "Archive", 1), 0, who("Harbour Office", "office@example.org"), "Your mooring receipt")
            .text("Thank you for your payment. Your receipt number is 4471."),
        Mail::new(("personal", "Trash", 1), 20, who("Prize Desk", "prizes@example.net"), "You have won")
            .text("Claim your prize today."),
        Mail::new(("work", "INBOX", 2), 28, who("Release Robot", "releases@example.org"), "Release keys rotated")
            .text("The signing keys for the release pipeline were rotated today.")
            .with(|m| m.security = Security::Signed),
        Mail::new(("work", "INBOX", 3), 29, who("People Team", "people@example.org"), "Offer details")
            .with(unread)
            .with(|m| m.security = Security::Encrypted),
        Mail::new(("work", "Drafts", 1), 30, ada("work"), "Re: Quarterly invoice")
            .text("Thank you. The invoice has been passed to accounts for payment.")
            .with(|m| (m.uid, m.origin, m.to) = (None, Origin::Agent, vec![invoice.from.clone()]))
            .answers(&invoice),
        Mail::new(("work", "Sent", 1), 29, ada("work"), "Re: Standup moved")
            .text("Ten is fine for me.")
            .with(|m| m.to = vec![standup.from.clone()])
            .answers(&standup),
        Mail::new(("work", "Archive", 1), 1, who("People Team", "people@example.org"), "Welcome aboard")
            .text("Your first day is Monday. Bring identification."),
        trip,
        trip_reply,
        trip_again,
        invoice,
        standup,
    ]
}

#[cfg(test)]
mod tests;

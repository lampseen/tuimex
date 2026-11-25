//! The design's fixture mailbox: `MAIL`, `OUTBOX`, `SECTIONS` and `NOW` of
//! `design/tui.js`, and the state its `build()` gives each list screen,
//! transcribed so the renderer can be compared with the design's own
//! drawing (ADR-002). A message's key is its `id` there.
//!
//! What only the reader draws of a message is in `bodies`, and `CALENDARS`
//! with `EVENTS` in `calendar`. `DRAFTS` is not here beyond the one the send
//! flows hold.

use tuimex_core::{
    AccountId, Address, Attachment, Envelope, Flags, FolderId, Listed, MessageId, Origin, Security, Sending, Timestamp,
};

mod bodies;
mod calendar;
mod compose;
mod session;

pub use self::bodies::{addresses, body, check};
pub use self::calendar::{DEC_5, FRI, Kept, THU, busy, calendars};
pub use self::compose::{AGENT_TIDIED, AGENT_WROTE, YOURS, agent_draft, drafts, identities, own_reply};
pub use self::session::{Fake, draft, folders, folders_of, sections};

/// 2026-11-23 14:20, read as UTC.
pub const NOW: Timestamp = at(23, 14, 20);

/// The tabs, in order. Outbox is not one of them: it is shown only while
/// something is in the outbox.
pub const SECTIONS: [(&str, &str); 6] = [
    ("Inbox", "in:inbox"),
    ("Unread", "in:inbox is:unread"),
    ("Flagged", "is:flagged"),
    ("Drafts", "in:drafts"),
    ("Sent", "in:sent"),
    ("Archive", "in:archive"),
];

/// `list` and `list-80-ascii`: the cursor is on the Halden thread, and
/// these two are marked.
pub const MARKED: [&str; 2] = ["backup", "parcel"];
/// The newest message of the Halden thread, which its folded row opens.
pub const HALDEN: &str = "halden-3";
/// `list-thread`: this thread is unfolded, by its root, with the cursor on
/// the second of these.
pub const UNFOLDED: (&str, &str) = ("cabin-1", "cabin-4");
/// `list-filter`: what is typed after `/`, and the rows it matches.
pub const FILTER: (&str, [&str; 3]) = ("nov", ["invite", "payroll", "library"]);
/// `list-failing`: the account that cannot sync, why, the seconds until it
/// tries again, and when it last could.
pub const FAILING: (&str, &str, u32, Timestamp) = ("work", "offline", 16, at(23, 12, 41));
/// `list-first-sync`: the account still arriving, and how far it has got.
pub const ARRIVING: (&str, u32, u32) = ("work", 1240, 3812);
/// Seconds since the account synced longest ago did, which `list-empty`
/// says.
pub const SYNCED: u32 = 60;

/// A day, hour and minute of November 2026.
const fn at(day: i64, hour: i64, minute: i64) -> Timestamp {
    // 2026-11-01 00:00 UTC.
    Timestamp(1_793_491_200 + (day - 1) * 86_400 + hour * 3600 + minute * 60)
}

type Person = (&'static str, &'static str);
const ME: Person = ("Robin Vale", "robin@example.com");
const ME_WORK: Person = ("Robin Vale", "robin.vale@example.org");
const IMANI: Person = ("Imani Castell", "imani@example.org");
const PRIYA: Person = ("Priya Raman", "priya@example.org");
const PRIYA_NET: Person = ("Priya Raman", "priya@example.net");
const DMITRI: Person = ("Dmitri Volkov", "dmitri@example.org");
const LARS: Person = ("Lars Lindqvist", "lars@example.org");
const MEI: Person = ("Mei Tanaka", "mei@example.org");
const OLU: Person = ("Olu Adeyemi", "olu@example.org");
const ANA: Person = ("Ana Sousa", "ana@example.org");
const HIRING: Person = ("Hiring Desk", "hiring@example.org");
const BACKUPS: Person = ("Backup Robot", "backups@example.org");
const STATUS: Person = ("Status Page", "status@example.org");
const TOMAS: Person = ("Tomas Ferreira", "tomas@example.net");
const GRETA: Person = ("Greta Holm", "greta@example.com");
const WEN: Person = ("Wen Zhao", "wen@example.com");
const JONAS: Person = ("Jonas Becker", "jonas@example.com");
const FERNWAY: Person = ("Fernway Weekly", "news@fernway.example.net");
const BANK: Person = ("Example Bank", "security@examplebank.example.net");
const PARCELS: Person = ("Parcel Desk", "track@parcels.example.net");
const LIBRARY: Person = ("City Library", "library@example.net");
const RENT: Person = ("Rent Office", "office@example.net");
const BILLING: Person = ("", "billing@halden.example.com");

const KB: u32 = 1024;
const PERSONAL: &str = "personal";
const WORK: &str = "work";

fn address((name, email): Person) -> Address {
    Address { name: Some(name.to_string()), email: email.to_string() }
}

pub fn id(account: &str, key: &str) -> MessageId {
    MessageId { account: AccountId(account.to_string()), key: key.to_string() }
}

fn message_id(key: &str) -> String {
    format!("<{key}@design.example.net>")
}

/// A read message with nothing attached.
fn mail(key: &str, account: &str, from: Person, to: &[Person], date: Timestamp, subject: &str) -> Listed {
    Listed {
        envelope: Envelope {
            id: id(account, key),
            folder: FolderId("Inbox".to_string()),
            date,
            from: vec![address(from)],
            to: to.iter().copied().map(address).collect(),
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            subject: subject.to_string(),
            message_id: Some(message_id(key)),
            references: Vec::new(),
            flags: Flags { seen: true, ..Flags::default() },
            size: 0,
            has_attachments: false,
            security: Security::None,
            origin: Origin::Server,
        },
        snippet: String::new(),
        attachments: Vec::new(),
        invite: false,
        unsubscribe: false,
        text: None,
        sender_was: None,
        sending: None,
    }
}

/// The changes the design's table makes to a message, by name.
trait Change: Sized {
    fn unread(self) -> Self;
    fn flagged(self) -> Self;
    fn answered(self) -> Self;
    fn security(self, security: Security) -> Self;
    fn cc(self, cc: &[Person]) -> Self;
    fn snippet(self, snippet: &str) -> Self;
    fn text(self, text: &str) -> Self;
    fn file(self, name: &str, mime: &str, size: u32) -> Self;
    /// `ancestors` from the thread's root down to the message answered.
    fn answers(self, ancestors: &[&str]) -> Self;
    fn invite(self) -> Self;
    fn sender_was(self, address: &str) -> Self;
    fn inline_first(self) -> Self;
}

impl Change for Listed {
    fn unread(mut self) -> Self {
        self.envelope.flags.seen = false;
        self
    }
    fn flagged(mut self) -> Self {
        self.envelope.flags.flagged = true;
        self
    }
    fn answered(mut self) -> Self {
        self.envelope.flags.answered = true;
        self
    }
    fn security(mut self, security: Security) -> Self {
        self.envelope.security = security;
        self
    }
    fn cc(mut self, cc: &[Person]) -> Self {
        self.envelope.cc = cc.iter().copied().map(address).collect();
        self
    }
    fn snippet(mut self, snippet: &str) -> Self {
        self.snippet = snippet.to_string();
        self
    }
    fn text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }
    fn file(mut self, name: &str, mime: &str, size: u32) -> Self {
        let index = self.attachments.len();
        self.attachments.push(Attachment {
            index,
            name: name.to_string(),
            mime: mime.to_string(),
            size,
            inline: false,
        });
        self.envelope.has_attachments = true;
        self
    }
    fn answers(mut self, ancestors: &[&str]) -> Self {
        self.envelope.references = ancestors.iter().map(|key| message_id(key)).collect();
        self
    }
    fn invite(mut self) -> Self {
        self.invite = true;
        self
    }
    fn sender_was(mut self, address: &str) -> Self {
        self.sender_was = Some(address.to_string());
        self
    }
    fn inline_first(mut self) -> Self {
        if let Some(first) = self.attachments.first_mut() {
            first.inline = true;
        }
        self
    }
}

/// `MAIL`: both accounts' inboxes, in the design's order.
pub fn inbox() -> Vec<Listed> {
    const PDF: &str = "application/pdf";
    const CABIN: &str = "Re: Cabin weekend: who is driving?";
    const HALDEN_RE: &str = "Re: Contract draft v3 for the Halden account";
    let others = [ME, GRETA, WEN, JONAS];
    let without = |who: Person| others.into_iter().filter(|p| *p != who).collect::<Vec<_>>();
    vec![
        mail("invite", WORK, IMANI, &[ME_WORK, LARS, MEI, DMITRI], at(23, 14, 5), "Invitation: Q4 roadmap review, Thu 26 Nov 10:00")
            .unread()
            .text("The pre-read is the roadmap document from last week. Please answer by Wednesday so I can book the right room.")
            .invite(),
        mail("cabin-1", PERSONAL, TOMAS, &others, at(22, 18, 10), "Cabin weekend: who is driving?")
            .snippet("We need two cars for six people and the skis."),
        mail("cabin-2", PERSONAL, GRETA, &[TOMAS], at(22, 19, 2), CABIN)
            .cc(&without(GRETA))
            .snippet("I can take three if we leave by four.")
            .answers(&["cabin-1"]),
        mail("cabin-3", PERSONAL, TOMAS, &[GRETA], at(22, 21, 30), CABIN)
            .cc(&without(GRETA))
            .snippet("Great. Then I will bring the food for Saturday.")
            .answers(&["cabin-1", "cabin-2"]),
        mail("cabin-4", PERSONAL, WEN, &[TOMAS], at(23, 8, 15), CABIN)
            .cc(&without(WEN))
            .unread()
            .snippet("I cannot make Friday. Could someone collect me on Saturday morning?")
            .answers(&["cabin-1"]),
        mail("cabin-5", PERSONAL, JONAS, &[TOMAS], at(23, 13, 40), CABIN)
            .cc(&without(JONAS))
            .unread()
            .snippet("Count me in. Robin, can you take the second car?")
            .text(
                "Count me in. Robin, can you take the second car? I will split the fuel, and I can bring the roof \
                 box if yours is still at your brother's.",
            )
            .answers(&["cabin-1", "cabin-2", "cabin-3"]),
        mail("backup", WORK, BACKUPS, &[ME_WORK], at(23, 12, 58), "[ops] Nightly backup finished with 2 warnings").unread(),
        mail("halden-1", WORK, PRIYA, &[ME_WORK], at(23, 9, 14), "Contract draft v3 for the Halden account")
            .cc(&[DMITRI])
            .flagged()
            .snippet("Attached is the third draft of the Halden contract.")
            .file("contract-v3.pdf", PDF, 482 * KB)
            .security(Security::Signed)
            .text(HALDEN_BODY),
        mail("halden-2", WORK, DMITRI, &[PRIYA], at(23, 10, 2), HALDEN_RE)
            .cc(&[ME_WORK])
            .snippet("Fine with being a named contact. 45 days works for me.")
            .answers(&["halden-1"]),
        mail("halden-3", WORK, PRIYA_NET, &[ME_WORK], at(23, 11, 32), HALDEN_RE)
            .unread()
            .snippet("One more change before you sign: our bank details have changed.")
            .file("halden-bank-details.pdf", PDF, 96 * KB)
            .text(
                "Hi Robin,\n\nOne more change before you sign: our bank details have changed, and the new ones are \
                 in the attached PDF. Please use them for the first invoice and confirm today, because the board \
                 date has moved up.\n\nPriya",
            )
            .sender_was(PRIYA.1)
            .answers(&["halden-1", "halden-2"]),
        mail("newsletter", PERSONAL, FERNWAY, &[ME], at(23, 10, 15), "This week: winter routes, a gear swap and one trail closure")
            .unread(),
        mail("migration", WORK, LARS, &[ME_WORK], at(23, 9, 48), "Staging database migration window")
            .answered()
            .text(bodies::MIGRATION),
        mail("bank", PERSONAL, BANK, &[ME], at(23, 9, 2), "Action needed: confirm your new device").unread().text(
            "We noticed a sign-in from a new device. To keep your account open, confirm the device within 24 \
             hours:\n\nhttps://bank.example.com/confirm [1]\n\nIf you do not confirm, your account will be limited. \
             Questions? Contact us [2]\n\nExample Bank Security Team",
        ),
        mail("payroll", WORK, MEI, &[ME_WORK], at(23, 8, 30), "Payroll corrections for November")
            .flagged()
            .security(Security::Encrypted)
            .text(
                "Two corrections before the run on Thursday: the overtime for the support rota, and one changed \
                 bank account. Both are in the sheet.",
            ),
        mail("photos", PERSONAL, GRETA, &[ME], at(22, 20, 44), "Photos from Saturday")
            .unread()
            .file("ridge-01.jpg", "image/jpeg", 2_516_582)
            .file("ridge-02.jpg", "image/jpeg", 3_250_586)
            .file("group-shot.heic", "image/heic", 1_887_437)
            .file("route.gpx", "application/gpx+xml", 84 * KB)
            .inline_first(),
        mail("parcel", PERSONAL, PARCELS, &[ME], at(22, 16, 20), "Your parcel is out for delivery"),
        mail("vault", WORK, OLU, &[ME_WORK], at(22, 11, 5), "New keys for the shared vault")
            .unread()
            .security(Security::Signed)
            .text("The vault keys rotate tonight. Install the attached key file before six or you will be locked out."),
        mail("budget", WORK, IMANI, &[ME_WORK], at(21, 17, 36), "Budget numbers for the offsite").answered(),
        mail("rack", PERSONAL, TOMAS, &[ME], at(21, 12, 12), "Bike rack: still want it?"),
        mail("loop", WORK, HIRING, &[ME_WORK], at(21, 9, 40), "Interview loop: backend candidate, Tuesday")
            .file("candidate-cv.pdf", PDF, 210 * KB),
        mail("library", PERSONAL, LIBRARY, &[ME], at(20, 15, 0), "Two items are due on 27 Nov"),
        mail("flames", WORK, DMITRI, &[ME_WORK], at(20, 11, 18), "Lunch and learn: reading flame graphs"),
        mail("expense", WORK, ANA, &[ME_WORK], at(20, 9, 5), "Expense report needs one receipt").unread().flagged(),
        mail("recipe", PERSONAL, WEN, &[ME], at(19, 19, 27), "Recipe you asked for").file("dumplings.pdf", PDF, 64 * KB),
        mail("resolved", WORK, STATUS, &[ME_WORK], at(19, 3, 12), "Resolved: elevated error rate on the API"),
        mail("statement", PERSONAL, RENT, &[ME], at(18, 10, 0), "December statement").file("statement-2026-12.pdf", PDF, 118 * KB),
        mail("oncall", WORK, LARS, &[ME_WORK], at(18, 8, 51), "On-call swap for the week of 7 Dec?"),
        mail("gym", PERSONAL, JONAS, &[ME], at(17, 18, 3), "Climbing gym: new opening hours"),
        mail("vendor", WORK, MEI, &[ME_WORK], at(14, 16, 45), "Notes from the vendor call").file("vendor-call.md", "text/markdown", 9 * KB),
        mail("bookclub", PERSONAL, GRETA, &[ME], at(12, 21, 10), "Book club pick for December").answered(),
    ]
}

const HALDEN_BODY: &str = "Hi Robin,\n\n\
Attached is the third draft of the Halden contract. Legal went through it on Friday and the changes from v2 are \
small, but two of them need a decision from you before I can send it on.\n\n\
1. Payment terms. Halden asked for 45 days instead of 30. Finance can live with 45 if the first invoice is issued \
at signing rather than at delivery.\n\n\
2. The support clause. They want named contacts on our side. I have put you and Dmitri down for now; say if that \
should be someone else.\n\n\
Everything else is wording. The liability cap is unchanged, the term is still twelve months with a renewal notice \
of sixty days, and schedule B lists the same deliverables as before.\n\n\
If you can reply by Wednesday I will get the signed copy to them before their board meets on the 27th. If either \
point needs a call, I am free tomorrow after two.\n\n\
A note on process: Halden's procurement team will only accept the signed PDF from an address on our domain, so \
please do not forward this draft from a personal account.\n\n\
Thanks,\nPriya\n\n-- \nPriya Raman\nCommercial lead";

/// `OUTBOX`: one message held for the send delay and one the server
/// refused.
pub fn outbox() -> Vec<Listed> {
    let held = |mut m: Listed, sending| {
        (m.envelope.folder, m.envelope.origin, m.sending) =
            (FolderId("Outbox".to_string()), Origin::Local, Some(sending));
        m
    };
    vec![
        held(
            mail("out-rack", PERSONAL, ME, &[TOMAS], at(23, 14, 20), "Re: Bike rack: still want it?"),
            Sending::Held { seconds: 6 },
        ),
        held(
            mail("out-headcount", WORK, ME_WORK, &[IMANI], at(23, 13, 5), "Offsite rooms").file(
                "rooms.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                31 * KB * KB,
            ),
            Sending::Failed { reason: "refused by smtp.example.org: 552 message too large".to_string() },
        ),
    ]
}

/// `search`: what is typed after `/`, and what all mail holds of it in the
/// design's order, with the words around a match that is only in a body.
pub const SEARCH: (&str, [(&str, Option<&str>); 6]) = (
    "nov",
    [
        ("invite", None),
        ("payroll", None),
        ("library", None),
        ("vendor", Some("next review is in November")),
        ("found-dinner", None),
        ("found-invoices", None),
    ],
);

/// The two messages `search` finds outside the inboxes: one sent, one
/// archived.
pub fn filed() -> Vec<Listed> {
    let filed = |mut m: Listed, folder: &str| {
        m.envelope.folder = FolderId(folder.to_string());
        m
    };
    vec![
        filed(mail("found-dinner", PERSONAL, ME, &[GRETA], at(9, 19, 40), "Re: Dinner in November?"), "Sent"),
        filed(mail("found-invoices", WORK, ANA, &[ME_WORK], at(3, 10, 12), "November invoice run"), "Archive"),
    ]
}

/// `list-three-accounts` and `account-cycle`: three accounts, two of them
/// alike in name, and the messages the third holds. The rest of `work`'s
/// are `gmail-work`'s and of `personal`'s `gmail-home`'s.
pub const TRIO: [&str; 3] = ["gmail-home", "gmail-work", "proton"];
pub const TO_PROTON: [&str; 6] = ["payroll", "vault", "resolved", "oncall", "vendor", "budget"];
/// The row `list-three-accounts` has the cursor on.
pub const PAYROLL: &str = "payroll";

/// `mail` as the three accounts hold it.
pub fn among_three(mut mail: Vec<Listed>) -> Vec<Listed> {
    for m in &mut mail {
        let id = &mut m.envelope.id;
        let account = match (TO_PROTON.contains(&id.key.as_str()), id.account.0.as_str()) {
            (true, _) => TRIO[2],
            (false, WORK) => TRIO[1],
            (false, _) => TRIO[0],
        };
        id.account = AccountId(account.to_string());
    }
    mail
}

/// `list-first-sync`: everything of `personal`, and of `work` only what has
/// arrived since the start of the 22nd.
pub fn arrived() -> Vec<Listed> {
    let from = at(22, 0, 0);
    inbox().into_iter().filter(|m| m.envelope.id.account.0 == PERSONAL || m.envelope.date >= from).collect()
}

#[cfg(test)]
mod tests;

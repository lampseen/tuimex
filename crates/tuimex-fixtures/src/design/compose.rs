//! What the design's compose screens are drawn from: `ACCOUNTS` as the
//! identities mail can be from, `DRAFTS[0]` whole, `OWN_REPLY`, and the
//! texts its `:agent` screens show.

use tuimex_core::{AccountId, Identity, Listed};

use super::{Change, DMITRI, KB, ME, ME_WORK, PERSONAL, PRIYA, WORK, address, at, mail, session};

/// `ACCOUNTS`: every identity, in the order From offers them.
pub fn identities() -> Vec<Identity> {
    let identity = |account: &str, who| Identity { account: AccountId(account.to_string()), address: address(who) };
    vec![identity(PERSONAL, ME), identity(WORK, ME_WORK), identity(WORK, ("Studio desk", "desk@example.org"))]
}

/// `DRAFTS[0]` as compose opens it: the agent's reply to the first Halden
/// message, with the two files the user added.
pub fn agent_draft() -> Listed {
    let sheet = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
    session::draft()
        .answers(&["halden-1"])
        .file("contract-v3-signed.pdf", "application/pdf", 512 * KB)
        .file("schedule-b.xlsx", sheet, 38 * KB)
        .text(
            "Hi Priya,\n\n45 days is fine provided the first invoice goes out at signing. Dmitri and I are the right \
             named contacts for the support clause.\n\nI have attached the signed copy and schedule B. Billing at \
             Halden is copied so the invoice details reach them directly.\n\nThanks,\nRobin",
        )
}

/// `DRAFTS`: the agent's reply and the user's own unfinished note.
pub fn drafts() -> Vec<Listed> {
    let mut packing =
        mail("draft-packing", PERSONAL, ME, &[super::GRETA], at(22, 22, 15), "Packing list for the cabin");
    (packing.envelope.folder, packing.envelope.origin) = (session::drafts(), tuimex_core::Origin::Local);
    (packing.envelope.flags.draft, packing.envelope.flags.seen) = (true, true);
    vec![agent_draft(), packing]
}

/// `OWN_REPLY`: the user's own reply to the first Halden message, saved
/// with `body` as its text.
pub fn own_reply(body: &str) -> Listed {
    let subject = "Re: Contract draft v3 for the Halden account";
    let mut draft = mail("draft-own", WORK, ME_WORK, &[PRIYA], at(23, 14, 21), subject).cc(&[DMITRI]);
    draft = draft.answers(&["halden-1"]).text(body);
    (draft.envelope.folder, draft.envelope.origin) = (session::drafts(), tuimex_core::Origin::Local);
    draft.envelope.flags.draft = true;
    draft
}

/// `yours`: what the user wrote before `:agent tidy`.
pub const YOURS: &str = "Hi Priya,\n\n45 days is ok i think so long as the 1st invoice goes at signing\n\n\
me and Dmitri are the contacts for support\n\nThanks,\nRobin";

/// `AGENT_WROTE`: what `:agent reply` returns.
pub const AGENT_WROTE: &str = "Hi Priya,\n\n45 days is fine provided the first invoice goes out at signing. Dmitri \
and I are the right named contacts for the support clause.\n\nI will send the signed copy once the bank details are \
confirmed by phone.\n\nThanks,\nRobin";

/// `AGENT_TIDIED`: what `:agent tidy` returns for `YOURS`.
pub const AGENT_TIDIED: &str = "Hi Priya,\n\n45 days is fine, as long as the first invoice goes out at signing.\n\n\
Dmitri and I are the right contacts for the support clause.\n\nThanks,\nRobin";

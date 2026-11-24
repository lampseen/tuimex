//! What a session over the design's mailbox needs beyond its list: the
//! `FOLDERS` and `SECTIONS` of `design/tui.js` as the UI takes them, the
//! agent's draft of `DRAFTS`, and a stand-in for whatever carries actions
//! out, which can be told to refuse a move as a server would.

use tuimex_core::{
    AccountId, Action, Ask, Event, Folder, FolderId, Hit, Listed, MessageId, Origin, Query, Role, Section, Sort,
};

use super::{
    BILLING, DMITRI, ME_WORK, PERSONAL, PRIYA, SEARCH, SECTIONS, WORK, address, at, body, check, filed, inbox, mail,
};

/// `FOLDERS`, in the design's order.
pub fn folders() -> Vec<Folder> {
    let mut all = folders_of(PERSONAL, [13, 214, 1, 1902, 37]);
    all.extend(folders_of(WORK, [17, 388, 1, 6455, 212]));
    all
}

/// An account's five folders, each named for its role, with how many
/// messages each holds.
pub fn folders_of(account: &str, totals: [u32; 5]) -> Vec<Folder> {
    let roles = [Role::Inbox, Role::Sent, Role::Drafts, Role::Archive, Role::Trash];
    let folder = |(role, total): (Role, u32)| {
        let name = format!("{role:?}");
        Folder { id: FolderId(name.clone()), account: AccountId(account.to_string()), name, role, unseen: 0, total }
    };
    roles.into_iter().zip(totals).map(folder).collect()
}

/// `SECTIONS`, less the outbox, which is a tab only while it holds mail.
pub fn sections() -> Vec<Section> {
    let section = |(name, query): &(&str, &str)| Section {
        name: name.to_string(),
        short: name.to_string(),
        query: Query::parse(query),
        sort: Sort::NewestFirst,
        threaded: true,
    };
    SECTIONS.iter().map(section).collect()
}

pub(super) fn drafts() -> FolderId {
    FolderId("Drafts".to_string())
}

/// `DRAFTS[0]`: the reply an agent drafted to the Halden thread, which the
/// send flows hold and send.
pub fn draft() -> Listed {
    let mut draft = mail(
        "draft-agent",
        WORK,
        ME_WORK,
        &[PRIYA, DMITRI],
        at(23, 13, 52),
        "Re: Contract draft v3 for the Halden account",
    );
    draft.envelope.cc = vec![address(BILLING)];
    (draft.envelope.folder, draft.envelope.origin) = (FolderId("Drafts".to_string()), Origin::Agent);
    draft.envelope.flags.draft = true;
    draft
}

/// Agrees to every ask and carries none out. It answers what a session
/// waits on: a message's body, the agent's advice, a search, how much an
/// emptied Trash held, and a move into a folder it was told to refuse, as
/// a server's refusal.
#[derive(Debug, Default)]
pub struct Fake {
    refuses: Vec<(FolderId, String)>,
    /// Every ask it was handed, oldest first.
    pub asked: Vec<Ask>,
}

impl Fake {
    /// Moves into `folder` are refused, in these words.
    pub fn refusing(folder: &str, reason: &str) -> Fake {
        Fake { refuses: vec![(FolderId(folder.to_string()), reason.to_string())], asked: Vec::new() }
    }

    /// What comes back for `ask`: the body of a message read, with what
    /// `gpg` said of it, the agent's advice on the bank's link, a refusal.
    pub fn ask(&mut self, ask: Ask) -> Vec<Event> {
        let serial = ask.serial;
        let read = |id: &MessageId| {
            // Every message `--fixtures` lists can be read, wherever it is filed.
            let all = inbox().into_iter().chain(filed()).chain(super::drafts());
            let body = all.clone().find(|m| m.envelope.id == *id).as_ref().map(body);
            let body = body.map(|body| Event::BodyReady { id: id.clone(), body: Box::new(body) });
            body.into_iter().chain(check(&id.key).map(|check| Event::Checked { id: id.clone(), check }))
        };
        let events: Vec<Event> = match &ask.action {
            Action::Move(to) => {
                let refusal = self.refuses.iter().find(|(folder, _)| folder == to);
                let failed = |(_, reason): &(FolderId, String)| Event::ActionFailed {
                    serial,
                    reason: reason.clone(),
                    hint: String::new(),
                };
                refusal.map(failed).into_iter().collect()
            }
            Action::Read => ask.on.iter().flat_map(read).collect(),
            Action::EmptyTrash(account) => {
                let trash = folders().into_iter().find(|f| f.account == *account && f.role == Role::Trash);
                let deleted = trash.map_or(0, |f| f.total as usize);
                vec![Event::Emptied { serial, account: account.clone(), deleted, kept: 0 }]
            }
            // The design searches for one word; anything else is in no
            // message of its mailbox.
            Action::Search(query) => {
                let all = inbox().into_iter().chain(filed());
                let hit = |(key, excerpt): &(&str, Option<&str>)| {
                    let id = all.clone().find(|m| m.envelope.id.key == *key)?.envelope.id;
                    Some(Hit { id, excerpt: excerpt.map(str::to_string) })
                };
                let hits = if *query == SEARCH.0 { SEARCH.1.iter().filter_map(hit).collect() } else { Vec::new() };
                vec![Event::Found { serial, hits, searched: folders().iter().map(|f| f.total).sum() }]
            }
            Action::AgentLink(_) => vec![
                Event::AgentAsked { serial, replaced: "1 person, 2 addresses".to_string() },
                Event::AgentAdvice {
                    serial,
                    advice: "against opening".to_string(),
                    why: "the host is not the bank's, and the message presses for haste".to_string(),
                },
            ],
            _ => Vec::new(),
        };
        self.asked.push(ask);
        events
    }
}

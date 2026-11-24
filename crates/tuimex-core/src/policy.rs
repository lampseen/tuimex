//! What an account lets the agent command line do (ADR-016). There is no
//! grant for sending, moving, deleting or flagging, so no request for them.

#[cfg(test)]
mod tests;

/// One account's `[account.agent]` table. An account without one grants
/// nothing, which is what `Grants::default()` is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    pub read: bool,
    /// The folders `read` covers, by name. Without `read` they cover nothing.
    pub folders: Vec<String>,
    pub draft: bool,
}

/// One thing a subcommand asks of one account. A call that spans accounts
/// or folders asks once for each it would touch (ADR-030), so the folder
/// here is always the one a message or a count would come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request<'a> {
    /// To be named at all.
    Accounts,
    /// `None` in these three asks whether the account lets an agent read
    /// anything, before any folder is known.
    Folders {
        folder: Option<&'a str>,
    },
    List {
        folder: Option<&'a str>,
    },
    Search {
        folder: Option<&'a str>,
    },
    Show {
        folder: &'a str,
    },
    Thread {
        folder: &'a str,
    },
    /// `reply_to` is the folder of the message being answered: a reply
    /// carries that message's ids, so it has to be one the agent may read.
    Draft {
        reply_to: Option<&'a str>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    Read,
    Draft,
}

impl Grant {
    /// The key in `[account.agent]` that would have allowed the request.
    pub fn name(self) -> &'static str {
        match self {
            Grant::Read => "read",
            Grant::Draft => "draft",
        }
    }
}

/// A refusal names the grant that would have allowed the request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct Denied {
    pub grant: Grant,
    pub reason: String,
}

/// The whole policy (ADR-016). Every subcommand's every answer passes
/// through here, and nothing else decides what an agent may have.
pub fn allows(grants: &Grants, request: &Request) -> Result<(), Denied> {
    let denied = |grant, reason: String| Err(Denied { grant, reason });
    let read = match *request {
        Request::Accounts if grants.read || grants.draft => return Ok(()),
        Request::Accounts => return denied(Grant::Read, "the account grants an agent nothing".into()),
        Request::Folders { folder } | Request::List { folder } | Request::Search { folder } => Some(folder),
        Request::Show { folder } | Request::Thread { folder } => Some(Some(folder)),
        Request::Draft { .. } if !grants.draft => return denied(Grant::Draft, "draft is not granted".into()),
        Request::Draft { reply_to } => reply_to.map(Some),
    };
    match read {
        None => Ok(()),
        Some(_) if !grants.read => denied(Grant::Read, "read is not granted".into()),
        Some(Some(folder)) if !grants.folders.iter().any(|covered| same_folder(covered, folder)) => {
            denied(Grant::Read, format!("read does not cover the folder {folder}"))
        }
        Some(_) => Ok(()),
    }
}

/// Folder names are exact, except INBOX, which IMAP spells in any case.
fn same_folder(a: &str, b: &str) -> bool {
    a == b || (a.eq_ignore_ascii_case("INBOX") && b.eq_ignore_ascii_case("INBOX"))
}

//! Which of a server's folders is the Inbox, the Archive, the Trash and so
//! on (PLAN §4.1).

use tuimex_core::{RemoteFolder, Role};

/// The SPECIAL-USE attribute for each role, then the names a server
/// without the attribute usually gives the folder, most likely first.
const KNOWN: [(Role, &str, &[&str]); 5] = [
    (Role::Archive, "\\Archive", &["Archive"]),
    (Role::Trash, "\\Trash", &["Trash", "Deleted Items", "Deleted Messages"]),
    (Role::Sent, "\\Sent", &["Sent", "Sent Items", "Sent Messages"]),
    (Role::Drafts, "\\Drafts", &["Drafts"]),
    (Role::Junk, "\\Junk", &["Junk", "Spam"]),
];

/// Gmail has no `\Archive`: archiving there is leaving a message in All
/// Mail alone. Elsewhere `\All` is a view over other folders, never a
/// place to move a message to.
const GMAIL_ARCHIVE: &str = "\\All";

/// Every folder that can be opened, with its role. `configured` is what
/// the account's config says (`folders.archive = "…"`), which wins; then a
/// SPECIAL-USE attribute; then a usual name. A role goes to one folder
/// only, and a role nothing fills is simply absent.
pub fn assign(folders: &[RemoteFolder], configured: &[(Role, String)], gmail: bool) -> Vec<(String, Role)> {
    let has = |f: &RemoteFolder, attribute: &str| f.attributes.iter().any(|a| a.eq_ignore_ascii_case(attribute));
    let open: Vec<&RemoteFolder> =
        folders.iter().filter(|f| !has(f, "\\Noselect") && !has(f, "\\NonExistent")).collect();
    let inbox = |f: &RemoteFolder| f.name.eq_ignore_ascii_case("INBOX");
    // A name is matched below the server's namespace: `INBOX.Trash` on a
    // server that keeps everything under the Inbox is still the Trash.
    let leaf = |f: &RemoteFolder| f.delimiter.and_then(|d| f.name.rsplit(d).next()).unwrap_or(&f.name).to_string();
    let mut assigned: Vec<(String, Role)> = Vec::new();
    for (role, attribute, names) in KNOWN {
        let free = |f: &&&RemoteFolder| !inbox(f) && !assigned.iter().any(|(name, _)| *name == f.name);
        let by_config =
            configured.iter().find(|(r, _)| *r == role).and_then(|(_, name)| open.iter().find(|f| f.name == *name));
        let by_attribute = || {
            let all_mail = |f: &&&RemoteFolder| gmail && role == Role::Archive && has(f, GMAIL_ARCHIVE);
            open.iter().filter(free).find(|f| has(f, attribute) || all_mail(f))
        };
        let by_name =
            || names.iter().find_map(|name| open.iter().filter(free).find(|f| leaf(f).eq_ignore_ascii_case(name)));
        if let Some(found) = by_config.or_else(by_attribute).or_else(by_name) {
            assigned.push((found.name.clone(), role));
        }
    }
    let role_of = |f: &RemoteFolder| match assigned.iter().find(|(name, _)| *name == f.name) {
        _ if inbox(f) => Role::Inbox,
        Some((_, role)) => *role,
        None => Role::Other,
    };
    open.iter().map(|f| (f.name.clone(), role_of(f))).collect()
}

/// What the name of a folder a person makes has to begin with, as far as
/// the listing shows it (ADR-048): `INBOX.` where every folder beside the
/// Inbox is named under it, as on a server that keeps a person's folders
/// there, and otherwise nothing. An Inbox alone shows nothing either way.
pub(crate) fn namespace(folders: &[RemoteFolder]) -> String {
    let inbox = |f: &&RemoteFolder| f.name.eq_ignore_ascii_case("INBOX");
    let Some((name, delimiter)) = folders.iter().find(inbox).and_then(|f| Some((&f.name, f.delimiter?))) else {
        return String::new();
    };
    let prefix = format!("{name}{delimiter}");
    let mut others = folders.iter().filter(|f| !inbox(f)).peekable();
    let under = others.peek().is_some() && others.all(|f| f.name.starts_with(&prefix));
    if under { prefix } else { String::new() }
}

#[cfg(test)]
mod tests;

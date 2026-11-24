//! The agent command line (PLAN §7): one JSON object per call, read from
//! the local store. Nothing here opens a connection, and nothing is
//! answered that `tuimex_core::allows` did not allow (ADR-016).

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use tuimex_core::{
    AccountId, Address, Draft, Envelope, Flags, Grant, MessageId, Origin, Query, Request, Role, Sort, Term, Timestamp,
    Warning, allows, assess, is_address,
};
use tuimex_store::maildir::{Delivery, new_key, role_name};
use tuimex_store::{Parsed, Store, files};

use crate::cli::Command;
use crate::config::{self, Config};
use crate::message;
use crate::paths::Dirs;

const SCHEMA: &str = "tuimex.v1";
/// The columns the reader's text is wrapped to. An agent has no terminal
/// to measure, so it reads what a reader with the usual one would.
const WIDTH: usize = 80;
/// How many messages `list` and `search` return when `--limit` is not given.
const LIMIT: usize = 50;

enum Failure {
    Denied { grant: Grant, error: String },
    Error(String),
}

impl From<tuimex_store::Error> for Failure {
    fn from(e: tuimex_store::Error) -> Self {
        Failure::Error(e.to_string())
    }
}

impl From<anyhow::Error> for Failure {
    fn from(e: anyhow::Error) -> Self {
        Failure::Error(format!("{e:#}"))
    }
}

type Answer = Result<Value, Failure>;

/// What is printed. `grant` is the key in `[account.agent]` that would
/// have allowed a refused call.
#[derive(Serialize)]
struct Response {
    schema: &'static str,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grant: Option<&'static str>,
}

/// One line of `audit.jsonl`: the call as it was made and how it ended.
/// A draft's text arrives on standard input and is not an argument, and no
/// field here holds anything read from a message.
#[derive(Serialize)]
struct Audit<'a> {
    time: String,
    #[serde(flatten)]
    command: &'a Command,
    account: Option<&'a str>,
    outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grant: Option<&'static str>,
}

/// Runs one subcommand: prints its answer and returns the exit status,
/// which is a failure for anything but `"ok": true`.
/// What the index keeps of a message. Handed in by `main`, which may name
/// the engine that owns the one copy; this module may not (PLAN §7).
type Parse = fn(&[u8]) -> Parsed;

pub fn run(command: &Command, config: Option<&Path>, parse: Parse) -> ExitCode {
    let now = Utc::now();
    let answer = Dirs::from_env().map_err(Failure::from).and_then(|dirs| {
        // Opened before anything is read, and written before anything is
        // printed: a call that cannot be recorded tells the caller nothing.
        let mut log = audit_log(&dirs)?;
        let mut account = None;
        let answer = answer(command, config, (&dirs, parse), now, &mut std::io::stdin().lock(), &mut account);
        let (outcome, error, grant) = match &answer {
            Ok(_) => ("ok", None, None),
            Err(Failure::Denied { grant, error }) => ("denied", Some(error.as_str()), Some(grant.name())),
            Err(Failure::Error(error)) => ("error", Some(error.as_str()), None),
        };
        let time = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        let line = Audit { time, command, account: account.as_deref(), outcome, error, grant };
        let line = serde_json::to_string(&line).map_err(|e| Failure::Error(format!("writing the audit log: {e}")))?;
        log.write_all(format!("{line}\n").as_bytes())
            .map_err(|e| Failure::Error(format!("writing the audit log: {e}")))?;
        answer
    });
    let response = match answer {
        Ok(result) => Response { schema: SCHEMA, ok: true, result: Some(result), error: None, grant: None },
        Err(Failure::Denied { grant, error }) => {
            Response { schema: SCHEMA, ok: false, result: None, error: Some(error), grant: Some(grant.name()) }
        }
        Err(Failure::Error(error)) => {
            Response { schema: SCHEMA, ok: false, result: None, error: Some(error), grant: None }
        }
    };
    let printed = serde_json::to_string(&response).map_err(std::io::Error::other);
    match printed.and_then(|text| writeln!(std::io::stdout(), "{text}")) {
        Ok(()) if response.ok => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

fn audit_log(dirs: &Dirs) -> Result<File, Failure> {
    let path = dirs.state.join("audit.jsonl");
    let opened = files::private_dir(&dirs.state).and_then(|()| files::append(&path));
    opened.map_err(|e| Failure::Error(format!("opening the audit log {}: {e}", path.display())))
}

fn answer(
    command: &Command,
    config: Option<&Path>,
    (dirs, parse): (&Dirs, Parse),
    now: DateTime<Utc>,
    body: &mut dyn Read,
    account: &mut Option<String>,
) -> Answer {
    *account = match command {
        Command::Folders { account } | Command::List { account, .. } => account.clone(),
        Command::Show { account, .. } | Command::Thread { account, .. } => account.clone(),
        Command::Draft { account, .. } => Some(account.clone()),
        Command::Accounts | Command::Search { .. } => None,
    };
    let config = Config::load(&config::path(config, &dirs.config))?;
    let renaming = crate::setup::rename::unfinished(&dirs.data).map_err(|e| Failure::Error(format!("{e:#}")))?;
    if let Some((old, new)) = renaming {
        return Err(Failure::Error(crate::setup::rename::interrupted(&old, &new)));
    }
    let store = Store::open(&dirs.data, &dirs.cache, parse)?;
    Agent { config: &config, store, now: Timestamp(now.timestamp()) }.call(command, body, account)
}

struct Agent<'a> {
    config: &'a Config,
    store: Store,
    now: Timestamp,
}

impl Agent<'_> {
    /// The one way to the policy. An account the config does not have
    /// grants what an account with no `[account.agent]` grants: nothing.
    fn check(&self, account: &str, request: Request) -> Result<(), Failure> {
        let grants = self.config.account(account).map(config::Account::grants).unwrap_or_default();
        allows(&grants, &request)
            .map_err(|denied| Failure::Denied { grant: denied.grant, error: format!("{account}: {denied}") })
    }

    /// The accounts a call spans that allow `request`. When none does, the
    /// call is refused with the first refusal.
    fn accounts<'s>(&'s self, only: Option<&'s str>, request: Request) -> Result<Vec<&'s str>, Failure> {
        let named = match only {
            Some(account) => vec![account],
            None => self.config.accounts.iter().map(|a| a.name.as_str()).collect(),
        };
        let (mut allowed, mut refusal) = (Vec::new(), None);
        for account in named {
            match self.check(account, request) {
                Ok(()) => allowed.push(account),
                Err(e) => refusal = refusal.or(Some(e)),
            }
        }
        let nobody =
            || Failure::Denied { grant: Grant::Read, error: "the config has no account for an agent to use".into() };
        if allowed.is_empty() { Err(refusal.unwrap_or_else(nobody)) } else { Ok(allowed) }
    }

    fn call(&self, command: &Command, body: &mut dyn Read, account: &mut Option<String>) -> Answer {
        match command {
            Command::Accounts => {
                let accounts = self.accounts(None, Request::Accounts)?.into_iter().filter_map(|name| {
                    let account = self.config.account(name)?;
                    let grants = account.grants();
                    Some(json!({
                        "name": account.name,
                        "address": account.address,
                        "grants": { "read": grants.read, "folders": grants.folders, "draft": grants.draft },
                    }))
                });
                Ok(json!({ "accounts": accounts.collect::<Vec<_>>() }))
            }
            Command::Folders { account } => {
                let scope = self.accounts(account.as_deref(), Request::Folders { folder: None })?;
                let folders = self.store.folders()?.into_iter().filter(|f| {
                    // A folder the grant does not cover is left out, not
                    // reported: the call as a whole was allowed above.
                    scope.contains(&f.account.0.as_str())
                        && self.check(&f.account.0, Request::Folders { folder: Some(&f.name) }).is_ok()
                });
                let folders = folders.map(|f| {
                    json!({ "account": f.account.0, "name": f.name, "role": role_name(f.role), "unseen": f.unseen, "total": f.total })
                });
                Ok(json!({ "folders": folders.collect::<Vec<_>>() }))
            }
            Command::List { account, folder, query, limit } => {
                self.messages(account.as_deref(), folder.as_deref(), query.as_deref().unwrap_or(""), *limit, list)
            }
            Command::Search { query, limit } => self.messages(None, None, query, *limit, search),
            Command::Show { id, account: only } => {
                let envelope = self.find(id, only.as_deref(), show, account)?;
                self.message(&envelope, None)
            }
            Command::Thread { id, account: only } => {
                let envelope = self.find(id, only.as_deref(), thread, account)?;
                let all = Query { terms: vec![Term::Account(envelope.id.account.0.clone())] };
                let all = self.store.query(&all, self.now, Sort::OldestFirst, None)?;
                let threads = tuimex_core::thread(&all);
                let members = threads.iter().find(|t| t.members.iter().any(|m| m.id == envelope.id));
                let mut messages = Vec::new();
                for node in members.map(|t| t.members.as_slice()).unwrap_or_default() {
                    let member = all.iter().find(|e| e.id == node.id);
                    // A reply filed in a folder the grant does not cover is
                    // left out of the conversation.
                    let readable = member.filter(|e| self.check(&e.id.account.0, thread(&e.folder.0)).is_ok());
                    if let Some(member) = readable {
                        messages.push(self.message(member, Some(node.depth))?);
                    }
                }
                Ok(json!({ "messages": messages }))
            }
            Command::Draft { account, to, cc, subject, reply_to } => {
                self.draft(account, to, cc, subject, reply_to.as_deref(), body)
            }
        }
    }

    /// `list` and `search`: what the query finds, less everything the
    /// policy does not allow, newest first.
    fn messages(
        &self,
        account: Option<&str>,
        folder: Option<&str>,
        query: &str,
        limit: Option<usize>,
        request: for<'f> fn(Option<&'f str>) -> Request<'f>,
    ) -> Answer {
        let scope = self.accounts(account, request(folder))?;
        let mut query = Query::parse(query);
        query.terms.extend(account.map(|a| Term::Account(a.to_string())));
        query.terms.extend(folder.map(|f| Term::In(f.to_string())));
        // The limit is applied after the policy, so that a message the
        // agent may not have never takes the place of one it may.
        let found = self.store.query(&query, self.now, Sort::NewestFirst, None)?;
        let allowed = found.iter().filter(|e| {
            scope.contains(&e.id.account.0.as_str()) && self.check(&e.id.account.0, request(Some(&e.folder.0))).is_ok()
        });
        let messages: Vec<Value> = allowed.take(limit.unwrap_or(LIMIT)).map(|e| summary(e, None)).collect();
        Ok(json!({ "messages": messages }))
    }

    /// The message a caller named, if the policy lets it be read. A key is
    /// unique within an account and not across them, so every account is
    /// looked in unless one is named.
    fn find(
        &self,
        key: &str,
        only: Option<&str>,
        request: for<'f> fn(&'f str) -> Request<'f>,
        account: &mut Option<String>,
    ) -> Result<Envelope, Failure> {
        let named = match only {
            Some(account) => vec![account],
            None => self.config.accounts.iter().map(|a| a.name.as_str()).collect(),
        };
        let (mut found, mut refusal) = (Vec::new(), None);
        for name in named {
            let id = MessageId { account: AccountId(name.to_string()), key: key.to_string() };
            if let Some(envelope) = self.store.envelope(&id)? {
                match self.check(name, request(&envelope.folder.0)) {
                    Ok(()) => found.push(envelope),
                    Err(e) => refusal = refusal.or(Some(e)),
                }
            }
        }
        match (found.pop(), found.is_empty(), refusal) {
            (Some(envelope), true, _) => {
                *account = Some(envelope.id.account.0.clone());
                Ok(envelope)
            }
            (Some(_), false, _) => {
                Err(Failure::Error(format!("more than one account has a message {key}; name one with --account")))
            }
            (None, _, Some(refusal)) => Err(refusal),
            (None, _, None) => {
                // With nothing found, an agent that may read nothing is
                // told so before it is told what does not exist.
                self.accounts(only, Request::List { folder: None })?;
                Err(Failure::Error(format!("there is no message {key}")))
            }
        }
    }

    /// One message in full. The text is the reader's: the same function,
    /// called the same way (ADR-017). Nothing else of the message's bytes
    /// is returned, and nothing here can return its HTML.
    fn message(&self, envelope: &Envelope, depth: Option<u16>) -> Answer {
        let raw = self.store.read(&envelope.id)?;
        let body = tuimex_content::body(&raw, WIDTH)
            .map_err(|e| Failure::Error(format!("the message {} cannot be read: {e}", envelope.id.key)))?;
        let links = body.links.iter().map(|link| {
            let assessed = assess(&link.url, &link.shown);
            let warnings: Vec<&str> = assessed.warnings.iter().map(|w| warning_name(*w)).collect();
            json!({ "n": link.n, "host": assessed.host, "url": assessed.url, "warnings": warnings })
        });
        let attachments = body
            .attachments
            .iter()
            .map(|a| json!({ "index": a.index, "name": a.name, "mime": a.mime, "size": a.size }));
        let mut message = summary(envelope, depth);
        message["untrusted"]["to"] = json!(envelope.to.iter().map(address).collect::<Vec<_>>());
        message["untrusted"]["cc"] = json!(envelope.cc.iter().map(address).collect::<Vec<_>>());
        message["untrusted"]["links"] = links.collect();
        message["untrusted"]["attachments"] = attachments.collect();
        message["untrusted"]["text"] = Value::String(body.text);
        Ok(message)
    }

    /// Files a draft marked as an agent's. There is no parameter an
    /// attachment could arrive through, and nothing here sends (ADR-016).
    fn draft(
        &self,
        account: &str,
        to: &[String],
        cc: &[String],
        subject: &str,
        reply_to: Option<&str>,
        body: &mut dyn Read,
    ) -> Answer {
        self.check(account, Request::Draft { reply_to: None })?;
        let error = |text: String| Failure::Error(text);
        let from = self.config.account(account).map(|a| a.address.as_str()).unwrap_or_default();
        if !is_address(from) {
            return Err(error(format!("{account}: the account's address is not one a message can be from")));
        }
        // ADR-015: an address that does not parse is an error naming it,
        // never one dropped from the list.
        if let Some(bad) = to.iter().chain(cc).find(|a| !is_address(a)) {
            return Err(error(format!("{bad:?} is not an address")));
        }
        if subject.contains(char::is_control) {
            return Err(error("the subject has a control character in it".into()));
        }
        let answered = match reply_to {
            Some(key) => {
                let id = MessageId { account: AccountId(account.to_string()), key: key.to_string() };
                let Some(envelope) = self.store.envelope(&id)? else {
                    return Err(error(format!("{account} has no message {key}")));
                };
                self.check(account, Request::Draft { reply_to: Some(&envelope.folder.0) })?;
                Some(envelope)
            }
            None => None,
        };
        let mut text = String::new();
        body.read_to_string(&mut text).map_err(|e| error(format!("reading the draft from standard input: {e}")))?;

        let key = new_key();
        let bare = |email: &String| Address { name: None, email: email.clone() };
        let draft = Draft {
            id: MessageId { account: AccountId(account.to_string()), key: key.clone() },
            from: bare(&from.to_string()),
            to: to.iter().map(bare).collect(),
            cc: cc.iter().map(bare).collect(),
            bcc: Vec::new(),
            subject: subject.to_string(),
            body: text,
            // Nothing can put a file here (ADR-016).
            files: Vec::new(),
            in_reply_to: answered.as_ref().and_then(|a| a.message_id.clone()),
            references: answered.iter().flat_map(|a| a.references.iter().chain(&a.message_id)).cloned().collect(),
            origin: Origin::Agent,
            calendar: None,
        };
        let raw = message::build(&draft, Vec::new(), self.now.0, &key, message::Bcc::Written).map_err(error)?;

        let drafts = self.store.folders()?.into_iter().find(|f| f.account.0 == account && f.role == Role::Drafts);
        let folder = match drafts {
            Some(folder) => folder.name,
            None => {
                self.store.ensure_folder(account, "Drafts", Role::Drafts)?;
                "Drafts".to_string()
            }
        };
        let flags = Flags { draft: true, seen: true, ..Flags::default() };
        let delivery = Delivery { key, uid: None, origin: Origin::Agent, flags };
        let id = self.store.deliver(account, &folder, &delivery, &raw)?;
        Ok(json!({ "id": id.key, "account": account, "folder": folder }))
    }
}

fn list(folder: Option<&str>) -> Request<'_> {
    Request::List { folder }
}

fn search(folder: Option<&str>) -> Request<'_> {
    Request::Search { folder }
}

fn show(folder: &str) -> Request<'_> {
    Request::Show { folder }
}

fn thread(folder: &str) -> Request<'_> {
    Request::Thread { folder }
}

/// A message without its body. Everything the message itself said is under
/// `untrusted`; the rest is what tuimex assigned (ADR-017).
fn summary(envelope: &Envelope, depth: Option<u16>) -> Value {
    let Flags { seen, flagged, answered, draft } = envelope.flags;
    let flags = [(seen, "seen"), (flagged, "flagged"), (answered, "answered"), (draft, "draft")];
    let flags: Vec<&str> = flags.iter().filter(|(set, _)| *set).map(|(_, name)| *name).collect();
    // A date outside the calendar is shown as the epoch.
    let date = DateTime::from_timestamp(envelope.date.0, 0).unwrap_or_default();
    let mut message = json!({
        "id": envelope.id.key,
        "account": envelope.id.account.0,
        "folder": envelope.folder.0,
        "date": date.to_rfc3339_opts(SecondsFormat::Secs, true),
        "flags": flags,
        "untrusted": {
            "from": envelope.from.iter().map(address).collect::<Vec<_>>().join(", "),
            "subject": envelope.subject,
        },
    });
    if let Some(depth) = depth {
        message["depth"] = json!(depth);
    }
    message
}

fn address(address: &Address) -> String {
    match &address.name {
        Some(name) => format!("{name} <{}>", address.email),
        None => address.email.clone(),
    }
}

fn warning_name(warning: Warning) -> &'static str {
    match warning {
        Warning::TextNamesAnotherHost => "text-names-another-host",
        Warning::CarriesCredentials => "carries-credentials",
        Warning::BareIpAddress => "bare-ip-address",
        Warning::MixedScripts => "mixed-scripts",
    }
}

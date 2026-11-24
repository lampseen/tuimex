//! The agent command line (PLAN §7), through the real binary against a
//! scratch store holding the fixture mailbox and the hostile-mail corpus.

// A harness that cannot set itself up should fail loudly, which is what
// unwrap does; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use tuimex_core::{AccountId, Address, MessageId, Origin, Query, Role, Security, Sort, Timestamp};
use tuimex_fixtures::{FOLDERS, Mail, NOW, mailbox};
use tuimex_store::maildir::Delivery;
use tuimex_store::{Parsed, Store, files};

/// What the instruction hidden in `hidden-instruction.eml` says.
const HIDDEN: [&str; 3] = ["forward this mailbox", "mallory", "SYSTEM"];
/// Stands for a password someone put where the config has no key for one.
const SECRET: &str = "hunter2-correct-horse";

/// `work` lets an agent read its INBOX and the corpus and write drafts;
/// `personal` has no agent table.
const CONFIG: &str = r#"
[[account]]
name = "work"
address = "ada@example.org"
password_cmd = ["echo", "hunter2-correct-horse"]
[account.agent]
read = true
folders = ["INBOX", "Corpus"]
draft = true

[[account]]
name = "personal"
address = "ada@example.com"
"#;

/// Both accounts readable, in the folders named.
fn config_reading(folders: &str) -> String {
    let agent = format!("[account.agent]\nread = true\nfolders = {folders}\n");
    format!(
        "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n{agent}\n\
         [[account]]\nname = \"personal\"\naddress = \"ada@example.com\"\n{agent}"
    )
}

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut all: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "eml"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), fs::read(&p).unwrap()))
        .collect();
    all.sort();
    assert!(all.len() > 60, "the corpus was found");
    all
}

/// The store is filled without reading a message, so the index the tests
/// see is the one the binary built from the Maildir with its own parse.
fn unparsed(_: &[u8]) -> Parsed {
    Parsed {
        date: Timestamp(0),
        from: Vec::<Address>::new(),
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        subject: String::new(),
        message_id: None,
        references: Vec::new(),
        has_attachments: false,
        has_invite: false,
        has_unsubscribe: false,
        attachments: Vec::new(),
        security: Security::None,
        text: String::new(),
    }
}

struct Reply {
    json: Value,
    stdout: String,
    code: Option<i32>,
}

/// A home of one test's own under the build directory: its config, its
/// mail and its logs, and nothing of the user's.
struct World {
    home: PathBuf,
}

impl World {
    fn new(test: &str, config: &str) -> World {
        let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("agent-{test}"));
        if home.exists() {
            fs::remove_dir_all(&home).unwrap();
        }
        let world = World { home };
        files::private_dir(&world.config_dir()).unwrap();
        files::create(&world.config_dir().join("config.toml")).unwrap().write_all(config.as_bytes()).unwrap();
        let store = world.store();
        for account in ["personal", "work"] {
            for (folder, role, uid_validity) in FOLDERS {
                store.ensure_folder(account, folder, role).unwrap();
                store.set_uid_validity(account, folder, uid_validity).unwrap();
            }
        }
        for mail in mailbox() {
            let delivery = Delivery { key: mail.key.clone(), uid: mail.uid, origin: mail.origin, flags: mail.flags };
            store.deliver(mail.account, mail.folder, &delivery, &mail.raw()).unwrap();
        }
        store.ensure_folder("work", "Corpus", Role::Other).unwrap();
        for (name, raw) in corpus() {
            let delivery = Delivery { key: name, uid: None, origin: Origin::Server, flags: Default::default() };
            store.deliver("work", "Corpus", &delivery, &raw).unwrap();
        }
        drop(store);
        fs::remove_dir_all(world.home.join(".cache/tuimex")).unwrap();
        world
    }

    fn config_dir(&self) -> PathBuf {
        self.home.join(".config/tuimex")
    }

    fn store(&self) -> Store {
        Store::open(&self.home.join(".local/share/tuimex"), &self.home.join(".cache/tuimex"), unparsed).unwrap()
    }

    /// Runs the binary with nothing of the caller's environment but the
    /// path, and holds it to PLAN §7: one JSON object on stdout, nothing
    /// on stderr, and success only for `"ok": true`.
    fn run(&self, args: &[&str], stdin: &str) -> Reply {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tuimex"))
            .args(args)
            .env_clear()
            .env("TUIMEX_KEYRING", "none")
            .env("HOME", &self.home)
            .env("PATH", std::env::var_os("PATH").unwrap())
            .current_dir(&self.home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stderr), "", "{args:?} wrote to stderr");
        assert_eq!(stdout.trim_end().lines().count(), 1, "{args:?} printed one line: {stdout:?}");
        let json: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{args:?}: {e}: {stdout:?}"));
        assert_eq!(json["schema"], "tuimex.v1", "{args:?}");
        assert_eq!(out.status.code() == Some(0), json["ok"] == true, "{args:?}: the exit status follows `ok`");
        Reply { json, stdout, code: out.status.code() }
    }

    fn ok(&self, args: &[&str]) -> Value {
        let reply = self.run(args, "");
        assert_eq!(reply.json["ok"], true, "{args:?}: {}", reply.stdout);
        reply.json["result"].clone()
    }

    /// The refusal PLAN §7 gives, key for key.
    fn denied(&self, args: &[&str], grant: &str) -> String {
        let reply = self.run(args, "");
        assert_eq!(keys(&reply.json), ["error", "grant", "ok", "schema"], "{args:?}: {}", reply.stdout);
        assert_eq!((&reply.json["ok"], &reply.json["grant"]), (&json!(false), &json!(grant)), "{args:?}");
        reply.json["error"].as_str().unwrap().to_string()
    }

    /// A failure that is not a refusal: no `grant`, because none would help.
    fn error(&self, args: &[&str]) -> String {
        let reply = self.run(args, "");
        assert_eq!(keys(&reply.json), ["error", "ok", "schema"], "{args:?}: {}", reply.stdout);
        reply.json["error"].as_str().unwrap().to_string()
    }

    fn audit(&self) -> Vec<Value> {
        let log = fs::read_to_string(self.home.join(".local/state/tuimex/audit.jsonl")).unwrap();
        log.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
    }

    /// Every file under the home with its content, but the config, which
    /// is the user's own and not one tuimex writes.
    fn written(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
                match entry.path() {
                    path if path.is_dir() => walk(&path, out),
                    path if path.ends_with("config.toml") => {}
                    path => out.push((path.clone(), fs::read(&path).unwrap())),
                }
            }
        }
        let mut all = Vec::new();
        walk(&self.home, &mut all);
        assert!(all.iter().any(|(path, _)| path.ends_with("audit.jsonl")), "the walk reached the audit log");
        all
    }
}

fn keys(object: &Value) -> Vec<&str> {
    object.as_object().unwrap().keys().map(String::as_str).collect()
}

fn the(subject: &str, account: &str, folder: &str) -> Mail {
    mailbox().into_iter().find(|m| (m.subject, m.account, m.folder) == (subject, account, folder)).unwrap()
}

/// The subjects of a `messages` result, in the order given.
fn subjects(result: &Value) -> Vec<&str> {
    result["messages"].as_array().unwrap().iter().map(|m| m["untrusted"]["subject"].as_str().unwrap()).collect()
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle.as_bytes())
}

#[test]
fn accounts_names_only_the_accounts_that_grant_something() {
    let world = World::new("accounts", CONFIG);
    let expected = json!({ "accounts": [{
        "name": "work",
        "address": "ada@example.org",
        "grants": { "read": true, "folders": ["INBOX", "Corpus"], "draft": true },
    }] });
    assert_eq!(world.ok(&["accounts"]), expected);
}

#[test]
fn folders_lists_what_read_covers_and_nothing_else() {
    let world = World::new("folders", CONFIG);
    let folders = world.ok(&["folders"]);
    let inbox = json!({ "account": "work", "name": "INBOX", "role": "inbox", "unseen": 2, "total": 4 });
    assert_eq!(folders["folders"][1], inbox);
    let names: Vec<&str> = folders["folders"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Corpus", "INBOX"]);
    assert_eq!(world.ok(&["folders", "--account", "work"]), folders);
    assert_eq!(world.denied(&["folders", "--account", "personal"], "read"), "personal: read is not granted");
}

#[test]
fn list_returns_only_messages_in_folders_read_covers() {
    let world = World::new("list", CONFIG);
    let inbox = world.ok(&["list", "--folder", "INBOX"]);
    assert_eq!(subjects(&inbox), ["Standup moved", "Offer details", "Release keys rotated", "Quarterly invoice"]);
    let everything = world.ok(&["list", "--limit", "1000"]);
    let folders: BTreeSet<(&str, &str)> = everything["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["account"].as_str().unwrap(), m["folder"].as_str().unwrap()))
        .collect();
    assert_eq!(folders, BTreeSet::from([("work", "INBOX"), ("work", "Corpus")]));

    assert_eq!(subjects(&world.ok(&["list", "--query", "is:unread in:inbox"])), ["Offer details", "Quarterly invoice"]);
    assert_eq!(subjects(&world.ok(&["list", "--folder", "INBOX", "--limit", "1"])), ["Standup moved"]);
    // A query cannot widen what the policy allows.
    assert_eq!(subjects(&world.ok(&["list", "--query", "in:sent"])), [""; 0]);
    assert_eq!(subjects(&world.ok(&["list", "--query", "account:personal"])), [""; 0]);

    let invoice = the("Quarterly invoice", "work", "INBOX");
    let row = &inbox["messages"][3];
    let expected = json!({
        "id": invoice.key,
        "account": "work",
        "folder": "INBOX",
        "date": "2026-11-20T11:01:00Z",
        "flags": [],
        "untrusted": { "from": "Billing <billing@example.com>", "subject": "Quarterly invoice" },
    });
    assert_eq!(row, &expected, "a row has no text, and everything the message said is under `untrusted`");
}

#[test]
fn list_is_refused_where_read_does_not_reach() {
    let world = World::new("list-denied", CONFIG);
    assert_eq!(world.denied(&["list", "--folder", "Sent"], "read"), "work: read does not cover the folder Sent");
    assert_eq!(world.denied(&["list", "--account", "personal"], "read"), "personal: read is not granted");
    assert_eq!(world.denied(&["list", "--account", "nobody"], "read"), "nobody: read is not granted");
}

#[test]
fn search_finds_only_what_the_agent_may_read() {
    let world = World::new("search", CONFIG);
    // The reply in work's Drafts matches too, and Drafts is not covered.
    let found = world.ok(&["search", "invoice", "--limit", "1000"]);
    let found: BTreeSet<(&str, &str)> = found["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["folder"].as_str().unwrap(), m["untrusted"]["subject"].as_str().unwrap()))
        .collect();
    assert!(
        found.contains(&("INBOX", "Quarterly invoice"))
            && found.contains(&("Corpus", "An instruction in display:none"))
    );
    assert!(
        found.iter().all(|(folder, subject)| *folder != "Drafts" && *subject != "Re: Quarterly invoice"),
        "{found:?}"
    );
    // The receipt is in personal's Archive.
    assert_eq!(subjects(&world.ok(&["search", "subject:invoice in:inbox"])), ["Quarterly invoice"]);
    assert_eq!(subjects(&world.ok(&["search", "receipt"])), [""; 0]);
    assert_eq!(subjects(&world.ok(&["search", "rotated", "--limit", "5"])), ["Release keys rotated"]);
}

/// M5's exit, and the reason ADR-017 exists.
#[test]
fn show_on_the_hidden_instruction_returns_no_hidden_text() {
    let world = World::new("hidden", CONFIG);
    let reply = world.run(&["show", "hidden-instruction"], "");
    let text = reply.json["result"]["untrusted"]["text"].as_str().unwrap();
    assert!(text.contains("Hi Grace, the invoice is attached.") && text.contains("Thanks, Ada"), "{text:?}");
    for hidden in HIDDEN {
        assert!(!reply.stdout.contains(hidden), "{hidden:?} reached the agent: {}", reply.stdout);
    }
    for reply in [world.run(&["thread", "hidden-instruction"], ""), world.run(&["search", "forward mailbox"], "")] {
        assert!(HIDDEN.iter().all(|hidden| !reply.stdout.contains(hidden)), "{}", reply.stdout);
    }
    assert_eq!(subjects(&world.ok(&["search", "mallory"])), [""; 0], "hidden text is not searchable either");
}

/// ADR-017, over the corpus: what `show` gives as text is what
/// `tuimex_content::body` gives the reader, and nothing of the message
/// arrives outside `untrusted` or in any other form.
#[test]
fn show_gives_the_readers_text_for_every_corpus_message() {
    let world = World::new("corpus", CONFIG);
    for (name, raw) in corpus() {
        let reply = world.run(&["show", &name], "");
        let body = match tuimex_content::body(&raw, 80) {
            Ok(body) => body,
            Err(e) => {
                assert_eq!(
                    reply.json["ok"], false,
                    "{name}: the reader cannot read it ({e}), so neither can the agent"
                );
                continue;
            }
        };
        let message = &reply.json["result"];
        assert_eq!(keys(message), ["account", "date", "flags", "folder", "id", "untrusted"], "{name}");
        let untrusted = &message["untrusted"];
        assert_eq!(keys(untrusted), ["attachments", "cc", "from", "links", "subject", "text", "to"], "{name}");
        assert_eq!(untrusted["text"], body.text.as_str(), "{name}");
        let links: Vec<(u64, &str)> = untrusted["links"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| (l["n"].as_u64().unwrap(), l["url"].as_str().unwrap()))
            .collect();
        let assessed: Vec<(u64, String)> =
            body.links.iter().map(|l| (l.n as u64, tuimex_core::assess(&l.url, &l.shown).url)).collect();
        assert_eq!(links.len(), assessed.len(), "{name}");
        for ((n, url), (want_n, want_url)) in links.iter().zip(&assessed) {
            assert_eq!((n, *url), (want_n, want_url.as_str()), "{name}");
        }
        let attachments: Vec<&str> =
            untrusted["attachments"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(attachments, body.attachments.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), "{name}");
        for markup in ["<html", "<body", "<script", "<img", "Content-Type:", "MIME-Version"] {
            assert!(!reply.stdout.contains(markup), "{name}: {markup} reached the agent: {}", reply.stdout);
        }
    }
}

#[test]
fn a_link_is_given_with_its_real_host_and_what_is_wrong_with_it() {
    let world = World::new("links", CONFIG);
    let message = world.ok(&["show", "link-text-names-another-host"]);
    let link = &message["untrusted"]["links"][0];
    assert_eq!(keys(link), ["host", "n", "url", "warnings"]);
    assert_eq!(link["warnings"], json!(["text-names-another-host"]));
    assert!(link["url"].as_str().unwrap().contains(link["host"].as_str().unwrap()));
}

#[test]
fn show_is_refused_outside_the_grant_and_says_so_in_the_shape_the_plan_gives() {
    let world = World::new("show-denied", CONFIG);
    let lunch = the("Lunch on Thursday?", "personal", "INBOX");
    assert_eq!(world.denied(&["show", &lunch.key], "read"), "personal: read is not granted");
    let sent = the("Re: Standup moved", "work", "Sent");
    assert_eq!(world.denied(&["show", &sent.key], "read"), "work: read does not cover the folder Sent");
    assert_eq!(world.denied(&["thread", &sent.key], "read"), "work: read does not cover the folder Sent");
    assert_eq!(world.error(&["show", "no-such-message"]), "there is no message no-such-message");
    assert_eq!(world.error(&["show", "../../../etc/passwd"]), "there is no message ../../../etc/passwd");
}

#[test]
fn with_no_grant_anywhere_every_subcommand_is_refused() {
    let none = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n";
    let world = World::new("no-grant", none);
    let invoice = the("Quarterly invoice", "work", "INBOX");
    for args in [
        &["accounts"][..],
        &["folders"],
        &["list"],
        &["search", "invoice"],
        &["show", &invoice.key],
        &["thread", &invoice.key],
        &["show", "no-such-message"],
    ] {
        world.denied(args, "read");
    }
    let draft = ["draft", "--account", "work", "--to", "grace@example.net", "--subject", "Hello"];
    assert_eq!(world.denied(&draft, "draft"), "work: draft is not granted");
    let empty = World::new("no-account", "");
    assert_eq!(empty.denied(&["accounts"], "read"), "the config has no account for an agent to use");
}

#[test]
fn an_id_two_accounts_share_needs_the_account_named() {
    let world = World::new("shared-id", &config_reading("[\"Drafts\"]"));
    let (work, personal) =
        (the("Re: Quarterly invoice", "work", "Drafts"), the("Notes for the trip", "personal", "Drafts"));
    assert_eq!(work.key, personal.key, "the fixture mailbox has one key in two accounts");
    let error = world.error(&["show", &work.key]);
    assert_eq!(error, format!("more than one account has a message {}; name one with --account", work.key));
    let shown = world.ok(&["show", &work.key, "--account", "personal"]);
    assert_eq!((&shown["account"], &shown["untrusted"]["subject"]), (&json!("personal"), &json!("Notes for the trip")));
    assert_eq!(shown["flags"], json!(["seen", "draft"]));
}

#[test]
fn thread_gives_the_conversation_less_what_read_does_not_cover() {
    let trip = the("Boat trip planning", "personal", "INBOX");
    let world = World::new("thread", &config_reading("[\"INBOX\", \"Sent\"]"));
    let thread = world.ok(&["thread", &trip.key]);
    let members: Vec<(&str, &str, u64)> = thread["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["folder"].as_str().unwrap(),
                m["untrusted"]["text"].as_str().unwrap().trim(),
                m["depth"].as_u64().unwrap(),
            )
        })
        .collect();
    let expected = [
        ("INBOX", "Shall we take the boat out on Saturday? I can bring the charts.", 0),
        ("Sent", "Saturday works. I will bring lunch.", 1),
        ("INBOX", "Perfect. The harbour at nine, then.", 2),
    ];
    assert_eq!(members, expected);

    let narrower = World::new("thread-inbox", &config_reading("[\"INBOX\"]"));
    let thread = narrower.ok(&["thread", &trip.key]);
    let folders: Vec<&str> =
        thread["messages"].as_array().unwrap().iter().map(|m| m["folder"].as_str().unwrap()).collect();
    assert_eq!(folders, ["INBOX", "INBOX"], "the reply in Sent is left out");
}

/// M5's exit: a draft marked as an agent's, with no attachment, that
/// nothing sent.
#[test]
fn draft_files_an_agents_draft_with_no_attachment_and_sends_nothing() {
    let world = World::new("draft", CONFIG);
    let invoice = the("Quarterly invoice", "work", "INBOX");
    let text = "Thank you.\n\nThe invoice has been passed on.\n";
    let args = [
        "draft",
        "--account",
        "work",
        "--to",
        "billing@example.com,grace@example.net",
        "--cc",
        "team@example.org",
        "--subject",
        "Re: Quarterly invoice",
        "--reply-to",
        &invoice.key,
    ];
    let reply = world.run(&args, text);
    assert_eq!(reply.json["ok"], true, "{}", reply.stdout);
    let result = &reply.json["result"];
    assert_eq!(keys(result), ["account", "folder", "id"]);
    assert_eq!((&result["account"], &result["folder"]), (&json!("work"), &json!("Drafts")));

    let store = world.store();
    let id = MessageId { account: AccountId("work".into()), key: result["id"].as_str().unwrap().to_string() };
    let envelope = store.envelope(&id).unwrap().expect("the draft is in the store");
    assert_eq!(envelope.origin, Origin::Agent);
    assert_eq!(envelope.folder.0, "Drafts");
    assert!(envelope.flags.draft);
    assert!(!envelope.has_attachments);
    let marked = store.query(&Query::parse("is:agent-draft account:work"), NOW, Sort::NewestFirst, None).unwrap();
    assert!(marked.iter().any(|e| e.id == id), "the reader's own query for agent drafts finds it");

    let raw = store.read(&id).unwrap();
    let body = tuimex_content::body(&raw, 80).unwrap();
    assert_eq!(body.attachments, [], "an agent's draft carries no attachment (ADR-016)");
    assert_eq!(body.text.trim(), text.trim());
    let head = tuimex_content::envelope(&raw).unwrap();
    let emails = |list: &[Address]| list.iter().map(|a| a.email.clone()).collect::<Vec<_>>();
    assert_eq!(emails(&head.from), ["ada@example.org"]);
    assert_eq!(emails(&head.to), ["billing@example.com", "grace@example.net"]);
    assert_eq!(emails(&head.cc), ["team@example.org"]);
    assert_eq!(head.subject, "Re: Quarterly invoice");
    let parent = invoice.message_id.trim_matches(['<', '>']);
    assert_eq!(head.references, [parent], "the draft answers the message it was told to");
    // The machine's name is not something a recipient is owed.
    let message_id = head.message_id.unwrap();
    assert!(message_id.ends_with("@example.org"), "{message_id}");
    assert!(!contains(&raw, "localhost"), "{}", String::from_utf8_lossy(&raw));

    // Nothing left: every other folder holds what it held.
    for folder in store.folders().unwrap() {
        let before = mailbox().iter().filter(|m| m.account == folder.account.0 && m.folder == folder.name).count();
        let expected = match (folder.account.0.as_str(), folder.name.as_str()) {
            ("work", "Drafts") => before + 1,
            ("work", "Corpus") => corpus().len(),
            _ => before,
        };
        assert_eq!(folder.total as usize, expected, "{}/{}", folder.account.0, folder.name);
        assert_ne!(folder.role, Role::Outbox, "nothing made an outbox");
    }
}

#[test]
fn draft_has_no_way_to_attach_and_refuses_what_it_cannot_do_whole() {
    let world = World::new("draft-refused", CONFIG);
    fn with<'a>(more: &[&'a str]) -> Vec<&'a str> {
        [&["draft", "--account", "work", "--subject", "Hello"], more].concat()
    }
    let drafts = || world.store().folders().unwrap().into_iter().find(|f| f.name == "Drafts" && f.account.0 == "work");

    for flag in ["--attach", "--attachment", "--file", "--bcc", "--send"] {
        let out = Command::new(env!("CARGO_BIN_EXE_tuimex"))
            .args(with(&["--to", "grace@example.net", flag, "/etc/hostname"]))
            .env_clear()
            .env("TUIMEX_KEYRING", "none")
            .env("HOME", &world.home)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{flag} is not an argument");
        assert!(String::from_utf8_lossy(&out.stderr).contains(flag));
    }
    // ADR-015: an address that does not parse is an error naming it.
    let bad = with(&["--to", "grace@example.net,Mallory <mallory@example.net>"]);
    assert_eq!(world.error(&bad), "\"Mallory <mallory@example.net>\" is not an address");
    let folded =
        ["draft", "--account", "work", "--to", "grace@example.net", "--subject", "Hi\r\nBcc: mallory@example.net"];
    assert_eq!(world.error(&folded), "the subject has a control character in it");
    let missing = with(&["--to", "grace@example.net", "--reply-to", "no-such-message"]);
    assert_eq!(world.error(&missing), "work has no message no-such-message");
    // A reply carries the ids of what it answers, so that has to be readable.
    let sent = the("Re: Standup moved", "work", "Sent");
    let unread = with(&["--to", "grace@example.net", "--reply-to", &sent.key]);
    assert_eq!(world.denied(&unread, "read"), "work: read does not cover the folder Sent");
    let elsewhere = ["draft", "--account", "personal", "--to", "grace@example.net", "--subject", "Hello"];
    assert_eq!(world.denied(&elsewhere, "draft"), "personal: draft is not granted");

    assert_eq!(drafts().unwrap().total, 1, "none of those left a draft behind");
}

#[test]
fn every_call_is_one_line_in_the_audit_log_with_no_message_text() {
    let world = World::new("audit", CONFIG);
    let invoice = the("Quarterly invoice", "work", "INBOX");
    let lunch = the("Lunch on Thursday?", "personal", "INBOX");
    world.ok(&["accounts"]);
    world.ok(&["show", &invoice.key]);
    world.denied(&["show", &lunch.key], "read");
    world.error(&["show", "no-such-message"]);
    world.ok(&["search", "invoice", "--limit", "3"]);
    let draft = ["draft", "--account", "work", "--to", "grace@example.net", "--subject", "Hello"];
    let made = world.run(&draft, "The body of the draft, which is not an argument.\n");
    assert_eq!(made.json["ok"], true);

    let lines = world.audit();
    assert_eq!(lines.len(), 6, "one line per call");
    for line in &lines {
        let time = line["time"].as_str().unwrap();
        assert!(time.len() == 20 && time.ends_with('Z'), "{time}");
    }
    let without_time = |line: &Value| {
        let mut line = line.clone();
        line.as_object_mut().unwrap().remove("time");
        line
    };
    let expected = [
        json!({ "subcommand": "accounts", "account": null, "outcome": "ok" }),
        json!({ "subcommand": "show", "arguments": { "id": invoice.key, "account": null }, "account": "work", "outcome": "ok" }),
        json!({
            "subcommand": "show", "arguments": { "id": lunch.key, "account": null }, "account": null,
            "outcome": "denied", "grant": "read", "error": "personal: read is not granted",
        }),
        json!({
            "subcommand": "show", "arguments": { "id": "no-such-message", "account": null }, "account": null,
            "outcome": "error", "error": "there is no message no-such-message",
        }),
        json!({ "subcommand": "search", "arguments": { "query": "invoice", "limit": 3 }, "account": null, "outcome": "ok" }),
        json!({
            "subcommand": "draft",
            "arguments": { "account": "work", "to": ["grace@example.net"], "cc": [], "subject": "Hello", "reply_to": null },
            "account": "work", "outcome": "ok",
        }),
    ];
    for (line, expected) in lines.iter().zip(&expected) {
        assert_eq!(&without_time(line), expected);
    }
    let log = fs::read(world.home.join(".local/state/tuimex/audit.jsonl")).unwrap();
    for text in [invoice.text, "Quarterly", "billing@example.com", "The body of the draft"] {
        assert!(!contains(&log, text), "{text:?} is in the audit log");
    }
}

#[test]
fn a_call_that_cannot_be_recorded_answers_nothing() {
    let world = World::new("audit-refused", CONFIG);
    let invoice = the("Quarterly invoice", "work", "INBOX");
    let state = world.home.join(".local/state/tuimex");
    files::private_dir(&state).unwrap();
    // Appending through a link would write wherever it points.
    std::os::unix::fs::symlink(world.home.join("elsewhere"), state.join("audit.jsonl")).unwrap();
    let reply = world.run(&["show", &invoice.key], "");
    let error = reply.json["error"].as_str().unwrap();
    assert!(error.starts_with("opening the audit log "), "{error}");
    assert!(!reply.stdout.contains("Quarterly"), "{}", reply.stdout);
    assert!(!world.home.join("elsewhere").exists());
    assert_eq!(reply.code, Some(1));
}

/// ADR-010, for the agent command line: its share of the test that record
/// asks for. The config has no key for a secret; one put in a command, or
/// under a key that does not exist, reaches nothing printed or written.
#[test]
fn no_agent_call_prints_or_writes_a_secret() {
    let invoice = the("Quarterly invoice", "work", "INBOX");
    let wrong_key = CONFIG.replace("password_cmd = [\"echo\", ", "password = [\"echo\", ");
    let wrong_type = CONFIG.replace("read = true", &format!("read = \"{SECRET}\""));
    assert!(wrong_key.contains(SECRET) && wrong_type.contains(SECRET) && CONFIG.contains(SECRET));
    for (name, config) in [("secret", CONFIG), ("secret-key", &wrong_key), ("secret-type", &wrong_type)] {
        let world = World::new(name, config);
        let draft = ["draft", "--account", "work", "--to", "grace@example.net", "--subject", "Hello"];
        let calls: [&[&str]; 11] = [
            &["accounts"],
            &["folders"],
            &["list"],
            &["list", "--folder", "Sent"],
            &["search", "invoice"],
            &["show", &invoice.key],
            &["show", "no-such-message"],
            &["thread", &invoice.key],
            &draft,
            &["draft", "--account", "personal", "--to", "grace@example.net", "--subject", "Hello"],
            &["draft", "--account", "work", "--to", "nobody", "--subject", "Hello"],
        ];
        for args in calls {
            let reply = world.run(args, "Hello.\n");
            assert!(!reply.stdout.contains(SECRET), "{name}: {args:?} printed it: {}", reply.stdout);
            assert_eq!(reply.json["ok"] == true, name == "secret" && reply.code == Some(0), "{name}: {args:?}");
        }
        for (path, content) in world.written() {
            assert!(!contains(&content, SECRET), "{name}: the secret is in {}", path.display());
        }
        assert_eq!(world.audit().len(), calls.len());
    }
    let world = World::new("secret-message", &CONFIG.replace("password_cmd = [\"echo\", ", "password = [\"echo\", "));
    assert_eq!(
        world.error(&["accounts"]),
        format!("the config {}: line 5: unknown key `password`", world.config_dir().join("config.toml").display())
    );
}

/// ADR-011, over everything an agent session leaves behind.
#[test]
fn everything_an_agent_session_creates_is_owner_only() {
    let world = World::new("modes", CONFIG);
    world.ok(&["list"]);
    let made = world.run(&["draft", "--account", "work", "--to", "grace@example.net", "--subject", "Hello"], "Hi.\n");
    assert_eq!(made.json["ok"], true);
    fn walk(dir: &Path, seen: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
            let (path, meta) = (entry.path(), entry.metadata().unwrap());
            let want = if meta.is_dir() { 0o700 } else { 0o600 };
            assert_eq!(meta.permissions().mode() & 0o777, want, "{}", path.display());
            if meta.is_dir() {
                walk(&path, seen);
            }
            seen.push(path);
        }
    }
    let mut seen = Vec::new();
    walk(&world.home, &mut seen);
    for made in ["audit.jsonl", "index.sqlite", "Drafts"] {
        assert!(seen.iter().any(|p| p.ends_with(made) || p.ends_with(format!(".{made}"))), "the walk reached {made}");
    }
}

/// ADR-011: the config is the one in the config directory or the one
/// named, and a file in the current directory is never either.
#[test]
fn the_config_is_never_read_from_the_current_directory() {
    let world = World::new("cwd", CONFIG);
    let config = world.config_dir().join("config.toml");
    let planted = world.home.join("config.toml");
    fs::rename(&config, &planted).unwrap();
    files::private_dir(&world.home.join("tuimex")).unwrap();
    fs::copy(&planted, world.home.join("tuimex/config.toml")).unwrap();
    fs::copy(&planted, world.home.join("tuimex.toml")).unwrap();
    let error = world.error(&["accounts"]);
    assert!(error.starts_with(&format!("reading the config {}: ", config.display())), "{error}");
    let named = world.ok(&["--config", planted.to_str().unwrap(), "accounts"]);
    assert_eq!(named["accounts"][0]["name"], "work");
    let after = world.ok(&["accounts", "--config", planted.to_str().unwrap()]);
    assert_eq!(after, named);
}

/// The long flags a `--help` lists under `Options:`.
fn long_flags(help: &str) -> BTreeSet<String> {
    let options = help.split("\nOptions:\n").nth(1).expect("the help lists its options");
    let leading = |line: &str| -> Vec<String> {
        let words = line.split_whitespace().map(|w| w.trim_end_matches(',').to_string());
        words.take_while(|w| w.starts_with('-')).filter(|w| w.starts_with("--")).collect()
    };
    options.lines().flat_map(leading).collect()
}

fn help(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_tuimex"))
        .args(args)
        .arg("--help")
        .env_clear()
        .env("TUIMEX_KEYRING", "none")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    String::from_utf8(out.stdout).unwrap()
}

/// M5's exit and PLAN §9: the agent can read and draft and nothing else.
/// A subcommand added to the binary fails here until it is put on one of
/// these two lists, where a reviewer sees which it was put on.
#[test]
fn the_binary_has_no_subcommand_outside_the_list() {
    /// PLAN §7, with every flag each takes.
    const AGENT: [(&str, &[&str]); 7] = [
        ("accounts", &[]),
        ("folders", &["--account"]),
        ("list", &["--account", "--folder", "--query", "--limit"]),
        ("show", &["--account"]),
        ("thread", &["--account"]),
        ("search", &["--limit"]),
        ("draft", &["--account", "--to", "--cc", "--subject", "--reply-to"]),
    ];
    /// Subcommands a person runs, which the agent policy does not cover.
    const PERSON: [&str; 4] = ["doctor", "palette", "setup", "sync"];

    let top = help(&[]);
    let expected: BTreeSet<String> = AGENT.iter().map(|(name, _)| *name).chain(PERSON).map(String::from).collect();
    let first_words: BTreeSet<String> = top
        .split("\nCommands:\n")
        .nth(1)
        .expect("`tuimex --help` lists its commands")
        .lines()
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next().map(String::from))
        .collect();
    assert_eq!(first_words, expected);
    for (name, flags) in AGENT {
        let expected: BTreeSet<String> = flags.iter().chain(&["--config", "--help"]).map(|f| f.to_string()).collect();
        assert_eq!(long_flags(&help(&[name])), expected, "the flags of `tuimex {name}`");
    }
    let doctor: BTreeSet<String> = ["--redact", "--config", "--help"].map(String::from).into();
    assert_eq!(long_flags(&help(&["doctor"])), doctor, "the flags of `tuimex doctor`");
    for verb in ["send", "move", "delete", "trash", "archive", "flag", "raw", "html", "help"] {
        let out = Command::new(env!("CARGO_BIN_EXE_tuimex"))
            .arg(verb)
            .env_clear()
            .env("TUIMEX_KEYRING", "none")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "`tuimex {verb}` is not a command");
    }
}

/// PLAN §7: none of them contacts a server. The module that answers has no
/// name for a socket, the engine or another program.
#[test]
fn the_agent_module_cannot_reach_the_network() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agent");
    let sources: Vec<PathBuf> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| !p.ends_with("tests.rs")).collect();
    assert!(sources.iter().any(|p| p.ends_with("mod.rs")), "the scan reached the module");
    for source in sources {
        let text = fs::read_to_string(&source).unwrap();
        for name in
            ["tuimex_engine", "std::net", "TcpStream", "process::Command", "Command::new", "lettre", "imap", "ureq"]
        {
            assert!(!text.contains(name), "{} names {name}", source.display());
        }
    }
}

/// ADR-050: while a rename is unfinished the mail and the config do not
/// agree, and neither an agent's call nor `tuimex sync` opens the mail.
#[test]
fn nothing_opens_the_mail_while_a_rename_is_unfinished_and_each_says_what_finishes_it() {
    let world = World::new("renaming", CONFIG);
    files::create(&world.home.join(".local/share/tuimex/renaming")).unwrap().write_all(b"work\noffice\n").unwrap();
    let said = "the rename of account work to office was begun and not finished, so its mail and its config do not \
                agree; run `tuimex setup`, which finishes it";
    assert_eq!(world.error(&["accounts"]), said);
    let sync = Command::new(env!("CARGO_BIN_EXE_tuimex"))
        .arg("sync")
        .env_clear()
        .env("TUIMEX_KEYRING", "none")
        .env("HOME", &world.home)
        .env("PATH", std::env::var_os("PATH").unwrap())
        .output()
        .unwrap();
    assert_eq!(sync.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&sync.stderr).contains(said), "{}", String::from_utf8_lossy(&sync.stderr));
    assert!(!world.home.join(".cache/tuimex/index.sqlite").exists(), "the store was not opened");
}

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tuimex_core::{AccountId, Action, AgentAsk, Ask, Body, Event, Link, MessageId};
use tuimex_engine::agent::Agent;

use super::{Desk, agent};
use crate::config::Config;
use crate::term::Backend;

/// Stands for the mail backend: it keeps what is passed on to it, and has
/// read one message, the bank's.
#[derive(Default)]
struct Inner(Vec<Ask>);

fn bank() -> MessageId {
    MessageId { account: AccountId("personal".into()), key: "bank".into() }
}

impl Backend for Inner {
    fn ask(&mut self, ask: Ask) {
        self.0.push(ask);
    }
    fn events(&mut self) -> Vec<Event> {
        Vec::new()
    }
    fn shown(&mut self, id: &MessageId) -> Option<(Vec<String>, Body)> {
        let link = |n, url: &str, shown: &str| Link { n, url: url.into(), shown: shown.into() };
        let body = Body {
            text: "Dear Robin, confirm at https://bank.example.com/confirm [1] or write to help@example.net".into(),
            links: vec![link(1, "https://198.51.100.7/confirm?utm_source=mail", "https://bank.example.com/confirm")],
            attachments: Vec::new(),
            invite: None,
            blocked_remote: Vec::new(),
            remote_images: 0,
            image_hosts: Vec::new(),
            loadable_images: 0,
            trackers: Vec::new(),
            unsubscribe: None,
            pictures: Vec::new(),
        };
        (*id == bank()).then(|| (vec!["Example Bank".to_string(), "Robin Vale".to_string()], body))
    }
}

/// A directory of this test's own under the build directory, emptied.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("desk-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// A desk whose home and cache are under `root`, with a line of shell for
/// the agent command when one is given. No test starts a real agent.
fn desk(root: &Path, script: Option<&str>) -> Desk<Inner> {
    let command = |script: &str| ["sh", "-c", script].map(String::from).to_vec();
    let agent =
        script.map(|s| Agent { command: command(s), timeout: Duration::from_secs(20), dir: root.join("cache/agent") });
    let mut desk = Desk::new(Inner::default(), agent, &root.join("cache"));
    desk.home = Some(root.join("home"));
    desk
}

fn ask(serial: u64, action: Action) -> Ask {
    Ask { serial, action, on: Vec::new() }
}

/// What the desk tells once the agent's thread has finished, or nothing
/// if it does not within `wait`.
fn told(desk: &mut Desk<Inner>, wait: Duration) -> Vec<Event> {
    let until = Instant::now() + wait;
    loop {
        let events = desk.events();
        if !events.is_empty() || Instant::now() >= until {
            return events;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// An editor that is `script`, as the value of `$EDITOR`: `sh` and the
/// file to read it from. The file is never itself run. A test that writes
/// a program and runs it at once is refused now and then with "Text file
/// busy", when another test's thread starts a process of its own while
/// the file is still open for writing here.
fn editor(root: &Path, script: &str) -> String {
    let path = root.join("editor.sh");
    fs::write(&path, format!("{script}\n")).unwrap();
    format!("sh {}", path.display())
}

fn modes_under(dir: &Path, out: &mut Vec<(PathBuf, u32)>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let meta = entry.metadata().unwrap();
        out.push((entry.path(), meta.permissions().mode() & 0o777));
        if meta.is_dir() {
            modes_under(&entry.path(), out);
        }
    }
}

#[test]
fn the_body_goes_through_the_editor_in_an_owner_only_file_that_is_then_removed() {
    let root = scratch("edit");
    let desk = desk(&root, None);
    // The editor records where it was pointed and with what modes, then
    // appends a line, as a person would.
    let seen = root.join("seen");
    let script = format!(
        "ls -ld \"$1\" > {0}; ls -ld \"$(dirname \"$1\")\" >> {0}; cat \"$1\" >> {0}; echo 'added by hand' >> \"$1\"",
        seen.display()
    );
    let text = desk.edit("Hi Priya,\n", &editor(&root, &script)).unwrap();
    assert_eq!(text, "Hi Priya,\nadded by hand\n");
    let seen = fs::read_to_string(seen).unwrap();
    let mut lines = seen.lines();
    let (file, dir) = (lines.next().unwrap(), lines.next().unwrap());
    let within = root.join("cache/compose").display().to_string();
    assert!(file.starts_with("-rw-------") && file.contains(&within) && file.ends_with("/body.txt"), "{file}");
    assert!(dir.starts_with("drwx------") && dir.contains(&within), "{dir}");
    assert_eq!(lines.next(), Some("Hi Priya,"), "the editor is given the body as it is");
    assert_eq!(fs::read_dir(root.join("cache/compose")).unwrap().count(), 0, "nothing of the edit is left");
    let mut modes = Vec::new();
    modes_under(&root.join("cache"), &mut modes);
    assert!(modes.iter().all(|(_, mode)| *mode == 0o700), "{modes:?}");
}

#[test]
fn an_editor_that_fails_or_is_not_there_changes_nothing_and_leaves_nothing() {
    let root = scratch("edit-fails");
    let desk = desk(&root, None);
    let failed = desk.edit("as it was", &editor(&root, "echo scribble > \"$1\"; exit 3")).unwrap_err();
    assert!(failed.contains("sh ended with exit status: 3"), "{failed}");
    let missing = desk.edit("as it was", "/nonexistent/editor --wait").unwrap_err();
    assert!(missing.starts_with("/nonexistent/editor could not be started: "), "{missing}");
    assert_eq!(fs::read_dir(root.join("cache/compose")).unwrap().count(), 0);
}

#[test]
fn the_editor_s_words_are_arguments_and_never_shell() {
    let root = scratch("edit-words");
    let desk = desk(&root, None);
    let named = format!("{} --flag $(echo not-run)", editor(&root, "echo \"$1 $2 $3 $#\" > \"$4\""));
    assert_eq!(
        desk.edit("", &named).unwrap(),
        "--flag $(echo not-run) 4\n",
        "each word is one argument, and the file is the last"
    );
}

#[test]
fn a_path_completes_to_the_files_it_could_name() {
    let root = scratch("paths");
    for dir in ["home/docs/reports", "home/docs/.hidden-dir"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in ["home/docs/report one.pdf", "home/docs/receipt.png", "home/docs/.secret", "home/notes.txt"] {
        fs::write(root.join(file), "x").unwrap();
    }
    let desk = desk(&root, None);
    let abs = |rest: &str| format!("{}/home/{rest}", root.display());
    assert_eq!(desk.paths("~/docs/re"), ["~/docs/receipt.png", "~/docs/report one.pdf", "~/docs/reports/"]);
    assert_eq!(
        desk.paths(&abs("docs/re")),
        [abs("docs/receipt.png"), abs("docs/report one.pdf"), abs("docs/reports/")]
    );
    assert_eq!(
        desk.paths("~/docs/"),
        ["~/docs/receipt.png", "~/docs/report one.pdf", "~/docs/reports/"],
        "nothing hidden"
    );
    assert_eq!(desk.paths("~/docs/."), ["~/docs/.hidden-dir/", "~/docs/.secret"]);
    assert_eq!(desk.paths("~/"), ["~/docs/", "~/notes.txt"]);
    assert_eq!(desk.paths("~/docs/zz"), [] as [&str; 0]);
    assert_eq!(desk.paths("~/nowhere/x"), [] as [&str; 0]);
}

#[test]
fn a_file_is_attached_by_its_name_and_size_and_anything_else_says_why() {
    let root = scratch("attach");
    fs::create_dir_all(root.join("home/docs")).unwrap();
    fs::write(root.join("home/docs/report one.pdf"), vec![0u8; 2048]).unwrap();
    let mut desk = desk(&root, None);
    desk.ask(ask(1, Action::Attach(" ~/docs/report one.pdf".into())));
    desk.ask(ask(2, Action::Attach("~/docs".into())));
    desk.ask(ask(3, Action::Attach("~/docs/absent.pdf".into())));
    let events = desk.events();
    let [Event::Attached { file: Ok(file) }, Event::Attached { file: Err(dir) }, Event::Attached { file: Err(absent) }] =
        &events[..]
    else {
        panic!("{events:?}");
    };
    assert_eq!((file.name.as_str(), file.size), ("report one.pdf", 2048));
    assert_eq!(file.path.as_deref(), Some(root.join("home/docs/report one.pdf").as_path()));
    assert_eq!(dir, "~/docs: not a file");
    assert!(absent.starts_with("~/docs/absent.pdf: No such file"), "{absent}");
    assert!(desk.inner.0.is_empty(), "none of it is the mail backend's");
}

#[test]
fn the_agent_command_answers_for_the_ask_that_started_it() {
    let root = scratch("agent");
    let mut desk = desk(&root, Some("echo 'Dear [person-1a],'; grep -c 'agree to 45 days'"));
    let reply = AgentAsk::Reply { thread: "From: [person-1]".into(), guidance: "agree to 45 days".into() };
    desk.ask(ask(7, Action::Agent(reply)));
    assert_eq!(
        told(&mut desk, Duration::from_secs(10)),
        [Event::AgentText { serial: 7, text: Ok("Dear [person-1a],\n1".into()) }]
    );
    let mode = fs::metadata(root.join("cache/agent")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "it ran in an owner-only directory of its own");
    assert_eq!(fs::read_dir(root.join("cache/agent")).unwrap().count(), 0);
}

#[test]
fn a_failing_agent_command_is_told_in_the_design_s_words() {
    let root = scratch("agent-fails");
    let mut desk = desk(&root, Some("echo not this; exit 1"));
    desk.ask(ask(3, Action::Agent(AgentAsk::Tidy { body: "me and Dmitri".into() })));
    let failed = Event::AgentText { serial: 3, text: Err("the agent command exited with 1".into()) };
    assert_eq!(told(&mut desk, Duration::from_secs(10)), [failed]);
}

#[test]
fn a_cancelled_agent_command_is_killed_and_tells_nothing() {
    let root = scratch("agent-cancel");
    let mut desk = desk(&root, Some("exec sleep 30"));
    desk.ask(ask(4, Action::Agent(AgentAsk::Tidy { body: "x".into() })));
    desk.ask(ask(5, Action::AgentCancel));
    assert_eq!(told(&mut desk, Duration::from_secs(2)), []);
    assert!(desk.running.is_none());
}

fn link(serial: u64, n: usize, on: MessageId) -> Ask {
    Ask { serial, action: Action::AgentLink(n), on: vec![on] }
}

/// ADR-031: the link's address, the text it was under, what the
/// assessment found and the redacted message go in; advice comes back.
#[test]
fn agent_link_hands_over_the_link_and_the_redacted_message_and_tells_the_advice() {
    let root = scratch("agent-link");
    let mut desk = desk(
        &root,
        Some("cat > ../saw; echo ' AGAINST opening : [person-1a] is hurried, and the host is a number'; echo more"),
    );
    desk.ask(link(9, 1, bank()));
    assert!(desk.working(), "the loop is told to ask again without a key");
    let asked = Event::AgentAsked { serial: 9, replaced: "1 person, 1 address".into() };
    assert_eq!(desk.events(), [asked], "what left is said at once, before the agent answers");
    let advice = Event::AgentAdvice {
        serial: 9,
        advice: "against opening".into(),
        why: "Robin is hurried, and the host is a number".into(),
    };
    assert_eq!(told(&mut desk, Duration::from_secs(10)), [advice], "one line, with its placeholder put back");
    assert!(!desk.working());
    let saw = fs::read_to_string(root.join("cache/saw")).unwrap();
    assert!(
        saw.contains(
            "The link opens: https://198.51.100.7/confirm\nIt is shown as: https://bank.example.com/confirm\n"
        ),
        "{saw}"
    );
    assert!(
        saw.contains("its text names a different host from the one it opens; its host is a bare IP address"),
        "{saw}"
    );
    assert!(
        saw.ends_with("Dear [person-1a], confirm at https://bank.example.com/confirm [1] or write to [address-1]\n"),
        "{saw}"
    );
    assert!(desk.inner.0.is_empty(), "the mail backend is asked nothing, and nothing opens the link");
}

#[test]
fn agent_link_that_cannot_be_answered_is_a_refusal_naming_why() {
    let root = scratch("agent-link-fails");
    let failed = |serial, reason: &str| Event::ActionFailed { serial, reason: reason.into(), hint: String::new() };
    let mut vague = desk(&root, Some("echo 'It looks fine to me, go ahead.'"));
    vague.ask(link(1, 1, bank()));
    assert_eq!(vague.events().len(), 1, "what left is said first");
    assert_eq!(told(&mut vague, Duration::from_secs(10)), [failed(1, "the agent's answer was not advice")]);
    let mut broken = desk(&root, Some("exit 1"));
    broken.ask(link(2, 1, bank()));
    broken.ask(link(3, 7, bank()));
    broken.ask(link(4, 1, MessageId { key: "unread".into(), ..bank() }));
    let mut events = broken.events();
    while broken.working() {
        events.extend(told(&mut broken, Duration::from_secs(10)));
    }
    assert!(events.contains(&failed(2, "the agent command exited with 1")), "{events:?}");
    assert!(events.contains(&failed(3, "the message has no such link")), "{events:?}");
    assert!(events.contains(&failed(4, "the message has not been read")), "{events:?}");
    // With no `[agent]` table it is not the desk's to answer.
    let mut none = desk(&root, None);
    none.ask(link(5, 1, bank()));
    assert_eq!((none.events(), none.inner.0.len()), (Vec::new(), 1));
}

#[test]
fn with_no_agent_table_nothing_is_run_and_everything_else_is_passed_on() {
    let root = scratch("agent-absent");
    let mut desk = desk(&root, None);
    desk.ask(ask(1, Action::Agent(AgentAsk::Tidy { body: "x".into() })));
    desk.ask(ask(2, Action::Send));
    assert_eq!(desk.events(), [Event::AgentText { serial: 1, text: Err("no [agent] table in the config".into()) }]);
    assert_eq!(desk.inner.0, [ask(2, Action::Send)]);
    assert!(!root.join("cache/agent").exists());
}

#[test]
fn the_agent_table_is_a_command_and_how_long_it_may_take() {
    let cache = Path::new("/cache");
    let table = |toml: &str| agent(Config::parse(toml).unwrap().agent.as_ref(), cache).map_err(|e| e.to_string());
    assert_eq!(table("").unwrap(), None, "no table, no command");
    let plain = table("[agent]\ncommand = [\"agent\", \"--quiet\"]\n").unwrap().unwrap();
    assert_eq!(
        (plain.command, plain.timeout, plain.dir),
        (vec!["agent".to_string(), "--quiet".into()], Duration::from_secs(120), cache.join("agent"))
    );
    let timed = |timeout: &str| table(&format!("[agent]\ncommand = [\"a\"]\ntimeout = \"{timeout}\"\n"));
    assert_eq!(timed("90s").unwrap().unwrap().timeout, Duration::from_secs(90));
    assert_eq!(timed("2m").unwrap().unwrap().timeout, Duration::from_secs(120));
    assert_eq!(timed("2h").unwrap_err(), "agent.timeout = \"2h\": the unit is one of s, m");
    assert_eq!(timed("soon").unwrap_err(), "agent.timeout = \"soon\": the unit is one of s, m");
    assert_eq!(timed("s").unwrap_err(), "agent.timeout = \"s\" does not start with a number");
    assert_eq!(table("[agent]\ncommand = []\n").unwrap_err(), "agent.command names no program");
    assert_eq!(table("[agent]\n").unwrap_err(), "[agent] needs `use = \"claude\"`, or a `command`");
    let unknown = format!("{:#}", Config::parse("[agent]\ncommand = [\"a\"]\ntools = true\n").unwrap_err());
    assert_eq!(unknown, "line 3: unknown key `tools`");
}

/// ADR-031: with `use = "claude"` the command is tuimex's own, and the
/// user's config cannot add to it or take from it.
#[test]
fn the_built_in_agent_is_named_and_never_mixed_with_a_command() {
    let cache = Path::new("/cache");
    let table = |toml: &str| agent(Config::parse(toml).unwrap().agent.as_ref(), cache).map_err(|e| e.to_string());
    let claude = table("[agent]\nuse = \"claude\"\n").unwrap().unwrap();
    assert_eq!(claude.command, tuimex_engine::agent::CLAUDE);
    assert_eq!(claude.timeout, Duration::from_secs(120), "the timeout need not be written");
    assert_eq!(
        table("[agent]\nuse = \"claude\"\ncommand = [\"claude\", \"-p\"]\n").unwrap_err(),
        "[agent] has both `use` and `command`; it takes one"
    );
    assert_eq!(
        table("[agent]\nuse = \"another\"\n").unwrap_err(),
        "agent.use = \"another\": the built-in agent is \"claude\""
    );
}

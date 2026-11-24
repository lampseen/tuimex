//! M9 as a person meets it, against a real server: a first launch on a
//! pseudo-terminal runs setup, which adds an account from nothing, shows
//! and pins the server's own certificate, signs in to IMAP and SMTP and
//! writes the config, and then opens the interface; `tuimex setup` adds a
//! second account; `tuimex sync` then needs no further input; `tuimex
//! doctor` passes, and fails on each thing then broken.
//!
//! The server is the one the environment names (`tuimex-engine`'s
//! `tests/spike.rs` lists the variables), with `TUIMEX_SERVER_SMTPS_PORT`
//! for its SMTP port and `TUIMEX_SERVER_USER2` for a second throwaway
//! account with the same password. With any missing the test says it did
//! not run.
//!
//! The user's keyring is never reached: `TUIMEX_KEYRING=none` turns it
//! off (ADR-053), so the accounts here sign in with a `password_cmd`, and
//! the keyring route is proven only as far as its refusal. What a
//! keyring that works does is `setup::tests`, on a keyring in memory.

// A test that cannot reach its server should stop with the reason.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const ENTER_SCREEN: &str = "\x1b[?1049h";
/// What the switch says, and no keyring service does: the refusal seen is
/// the switch's and not that of a machine which happens to have no keyring.
const KEYRING_OFF: &str = "no keyring service could be reached (TUIMEX_KEYRING is none, so none was tried)";

struct Server {
    host: String,
    imap: String,
    smtp: String,
    ca: String,
    /// The two accounts: user, password, and the variable its
    /// `password_cmd` reads the password from.
    users: [(String, String, &'static str); 2],
}

fn server() -> Option<Server> {
    let var = |name: &str| env::var(name).ok();
    let found = (|| {
        Some(Server {
            host: var("TUIMEX_SERVER_HOST")?,
            imap: var("TUIMEX_SERVER_PORT")?,
            smtp: var("TUIMEX_SERVER_SMTPS_PORT")?,
            ca: var("TUIMEX_SERVER_CA")?,
            users: [
                (var("TUIMEX_SERVER_USER")?, var("TUIMEX_SERVER_PASSWORD")?, "TUIMEX_SERVER_PASSWORD"),
                (var("TUIMEX_SERVER_USER2")?, var("TUIMEX_SERVER_PASSWORD")?, "TUIMEX_SERVER_PASSWORD"),
            ],
        })
    })();
    if found.is_none() {
        println!("real-server setup: NOT RUN, no server with an SMTP port and a second account is named");
    }
    found
}

struct Home {
    home: PathBuf,
    server: Server,
}

impl Home {
    fn config(&self) -> PathBuf {
        self.home.join(".config/tuimex")
    }

    /// Nothing of the caller's environment reaches the program: its home
    /// is a scratch one, its path a directory of stand-ins and the
    /// system's own, and its keyring turned off.
    fn isolate(&self, command: &mut Command) {
        let path = format!("{}:/usr/bin:/bin", self.home.join("bin").display());
        command.env_clear().env("HOME", &self.home).env("PATH", path).env("TERM", "xterm-256color").env("LC_ALL", "C");
        command.env("TUIMEX_KEYRING", "none");
        for (_, password, variable) in &self.server.users {
            command.env(variable, password);
        }
    }

    /// `tuimex <args>` on a pseudo-terminal, each question answered once
    /// it has been asked. What the terminal showed, and the exit status.
    fn on_a_pty(&self, args: &[&str], steps: &[(&str, &str)]) -> (String, i32) {
        let mut command = Command::new("python3");
        self.isolate(&mut command);
        let steps: Vec<[&str; 2]> = steps.iter().map(|(question, answer)| [*question, *answer]).collect();
        command
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/answer_on_pty.py"))
            .arg(serde_json::to_string(&steps).unwrap())
            .arg(env!("CARGO_BIN_EXE_tuimex"))
            .args(args);
        let out = command.output().expect("python3 runs the pty harness");
        assert!(out.stderr.is_empty(), "the pty harness failed: {}", String::from_utf8_lossy(&out.stderr));
        (String::from_utf8_lossy(&out.stdout).replace('\r', ""), out.status.code().unwrap())
    }

    /// `tuimex <args>` with no terminal and nothing to read.
    fn run(&self, args: &[&str]) -> (String, i32) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tuimex"));
        self.isolate(&mut command);
        let Output { status, stdout, stderr } = command.args(args).stdin(Stdio::null()).output().unwrap();
        (format!("{}{}", String::from_utf8_lossy(&stdout), String::from_utf8_lossy(&stderr)), status.code().unwrap())
    }

    /// The answers that add account `n` under `name`, signing in the way
    /// `sign_in` says.
    fn adding<'a>(&'a self, n: usize, name: &'a str, sign_in: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let (user, _, _) = &self.server.users[n];
        let mut steps = vec![
            ("Email address:", if n == 0 { "spike@example.com" } else { "second@example.com" }),
            ("Which [5]:", ""),
            ("IMAP server", self.server.host.as_str()),
            ("IMAP port", self.server.imap.as_str()),
            ("SMTP server:", self.server.host.as_str()),
            ("SMTP port", self.server.smtp.as_str()),
            ("Username", user.as_str()),
            ("short name", name),
        ];
        steps.extend_from_slice(sign_in);
        steps.push(("Trust this certificate", "y"));
        // A role is asked about for each folder other tests have left on
        // the server that could fill one; none of them does.
        steps.extend([("?Which is the", ""); 5]);
        steps
    }
}

fn fingerprint(pem: &Path) -> String {
    let out =
        Command::new("openssl").args(["x509", "-noout", "-fingerprint", "-sha256", "-in"]).arg(pem).output().unwrap();
    assert!(out.status.success(), "openssl could not read {}", pem.display());
    String::from_utf8(out.stdout).unwrap().trim().rsplit('=').next().unwrap().to_string()
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        out.push(path.clone());
        if path.is_dir() {
            files_under(&path, out);
        }
    }
}

#[test]
fn setup_adds_two_accounts_from_nothing_and_sync_and_doctor_then_need_no_answers() {
    let Some(server) = server() else { return };
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("setup-real");
    if home.exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    // Stand-ins for the programs doctor looks for. One that was run
    // would say so; doctor is only to find them.
    fs::create_dir_all(home.join("bin")).unwrap();
    for program in ["claude", "vi"] {
        let script = format!("#!/bin/sh\ntouch {}/{program}-was-run\n", home.display());
        fs::write(home.join("bin").join(program), script).unwrap();
        fs::set_permissions(home.join("bin").join(program), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let here = Home { home: home.clone(), server };
    let (config, ca_print) = (here.config(), fingerprint(Path::new(&here.server.ca)));
    let mut shown = Vec::new();
    let command = |variable: &'static str| format!("printf %s \"${variable}\"");

    // 1. A first launch: `tuimex` alone, with no config and a terminal,
    //    asks setup's questions and then opens the interface on the
    //    account they made, which `:q` leaves. The account's password
    //    comes from a command.
    let by_command = command("TUIMEX_SERVER_PASSWORD");
    let sign_in =
        [("Which [1]:", "2"), ("Program:", "sh"), ("Argument:", "-c"), ("Argument:", &by_command), ("Argument:", "")];
    let mut steps = here.adding(0, "work", &sign_in);
    steps.extend([("read this account", "y"), ("write drafts", "y"), ("Claude CLI", "y"), ("Unread", ":q")]);
    let (launched, status) = here.on_a_pty(&[], &steps);
    assert_eq!(status, 0, "{launched}");
    let (first, interface) = launched.split_once(ENTER_SCREEN).expect("the interface opened once setup was done");
    assert!(interface.contains("Inbox") && interface.contains("Unread"), "the interface drew its sections");
    let first = first.to_string();
    assert!(
        first.contains(&format!("SHA-256 fingerprint: {ca_print}")),
        "the certificate shown is the server's:\n{first}"
    );
    let (imap, smtp) =
        (format!("{}:{}", here.server.host, here.server.imap), format!("{}:{}", here.server.host, here.server.smtp));
    assert!(first.contains(&format!("Signed in to {imap} and found")), "{first}");
    assert!(first.contains(&format!("Signed in to {smtp}; nothing was sent.")), "{first}");
    assert_eq!(fingerprint(&config.join("work.pem")), ca_print, "the certificate accepted is the one pinned");
    let (user, user2) = (&here.server.users[0].0, &here.server.users[1].0);
    let account = |name: &str, address: &str, user: &str, variable: &str| {
        format!(
            "[[account]]\nname = \"{name}\"\naddress = \"{address}\"\n\
             imap = {{ host = \"{host}\", port = {}, starttls = false, user = \"{user}\" }}\n\
             smtp = {{ host = \"{host}\", port = {}, starttls = false, user = \"{user}\" }}\n\
             auth = \"command\"\npassword_cmd = [\"sh\", \"-c\", 'printf %s \"${variable}\"']\n",
            here.server.imap,
            here.server.smtp,
            host = here.server.host,
        )
    };
    let work = format!(
        "{}\n[account.agent]\nread = true\nfolders = [\"INBOX\"]\ndraft = true\n\n[agent]\nuse = \"claude\"\n",
        account("work", "spike@example.com", user, "TUIMEX_SERVER_PASSWORD")
    );
    assert_eq!(fs::read_to_string(config.join("config.toml")).unwrap(), work);
    shown.push(first.clone());

    // 2. A second, with a typed password. It is not echoed; both servers
    //    take it; and with no keyring service to keep it, setup says what
    //    to do instead and saves nothing.
    let typed = here.server.users[1].1.clone();
    let mut steps = vec![("What would you like to do [1]:", "")];
    steps.extend(here.adding(1, "second", &[("Which [1]:", "1"), ("not shown as you type", &typed)]));
    steps.extend([("read this account", "n"), ("write drafts", "n")]);
    let (refused, status) = here.on_a_pty(&["setup"], &steps);
    assert_eq!(status, 1, "{refused}");
    assert!(!refused.contains(&typed), "the password was echoed:\n{refused}");
    assert!(refused.contains(&format!("Signed in to {smtp}; nothing was sent.")), "{refused}");
    assert!(refused.contains(&format!("E setup: the keyring would not keep it: {KEYRING_OFF}")), "{refused}");
    assert!(refused.contains("give the account a command that prints its password instead"), "{refused}");
    assert_eq!(fs::read_to_string(config.join("config.toml")).unwrap(), work, "nothing was saved");
    assert!(!config.join("second.pem").exists());
    shown.push(refused);

    // 3. The second again, with a command: added after the first.
    let by_command = command("TUIMEX_SERVER_PASSWORD");
    let sign_in =
        [("Which [1]:", "2"), ("Program:", "sh"), ("Argument:", "-c"), ("Argument:", &by_command), ("Argument:", "")];
    let mut steps = vec![("What would you like to do [1]:", "")];
    steps.extend(here.adding(1, "second", &sign_in));
    steps.extend([("read this account", "n"), ("write drafts", "n")]);
    let (second, status) = here.on_a_pty(&["setup"], &steps);
    assert_eq!(status, 0, "{second}");
    assert!(second.contains("Accounts here: work."), "{second}");
    let both = format!("{work}\n{}", account("second", "second@example.com", user2, "TUIMEX_SERVER_PASSWORD"));
    assert_eq!(fs::read_to_string(config.join("config.toml")).unwrap(), both);
    assert_eq!(fingerprint(&config.join("second.pem")), ca_print);
    shown.push(second);

    // 4. `tuimex sync` with nothing to read from: both accounts.
    let (synced, status) = here.run(&["sync"]);
    assert_eq!(status, 0, "{synced}");
    assert!(synced.contains("work/INBOX: ") && synced.contains("second/INBOX: "), "{synced}");
    assert!(home.join(".local/share/tuimex/work/cur").exists() && home.join(".local/share/tuimex/second/cur").exists());
    shown.push(synced.clone());

    // 5. `tuimex doctor`: both accounts end to end, and nothing wrong.
    let (well, status) = here.run(&["doctor"]);
    assert_eq!(status, 0, "{well}");
    for line in [
        format!("  ok     its server's certificate is pinned: SHA-256 {ca_print}"),
        "  ok     its `password_cmd` printed a password".to_string(),
        format!("  ok     IMAP {imap}: signed in, "),
        format!("  ok     SMTP {smtp}: signed in, and nothing was sent"),
        format!("  note   the keyring does not answer: {KEYRING_OFF}"),
        format!("  ok     `:agent` uses the built-in claude, found at {}/bin/claude; it was not run", home.display()),
        "nothing is wrong".to_string(),
    ] {
        assert!(well.contains(&line), "wanted {line:?} in:\n{well}");
    }
    assert_eq!(well.matches("signed in, and nothing was sent").count(), 2);
    shown.push(well.clone());

    // 6. Doctor on what is then broken: a certificate that is not the one
    //    pinned, a password the server does not take, a file others can read.
    let other = home.join("other");
    fs::create_dir_all(&other).unwrap();
    let made = Command::new("openssl")
        .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=localhost", "-keyout"])
        .arg(other.join("key.pem"))
        .arg("-out")
        .arg(other.join("cert.pem"))
        .output()
        .unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let other_print = fingerprint(&other.join("cert.pem"));
    // As the pin was: only what the file holds is changed.
    fs::write(config.join("work.pem"), fs::read(other.join("cert.pem")).unwrap()).unwrap();
    let (changed, status) = here.run(&["doctor"]);
    assert_eq!(status, 1, "{changed}");
    assert!(changed.contains("is not the one that was pinned"), "{changed}");
    let both_prints = format!("           pinned:    {other_print}\n           presented: {ca_print}\n");
    assert!(changed.contains(&both_prints), "{changed}");
    assert!(changed.ends_with("2 things are wrong\n"), "IMAP and SMTP of the one account:\n{changed}");
    shown.push(changed.clone());

    // 7. Setup, at "Accept a new certificate for work", shows both and
    //    takes the new one.
    let steps = [("What would you like to do [1]:", "3"), ("Trust this certificate", "y")];
    let (again, status) = here.on_a_pty(&["setup"], &steps);
    assert_eq!(status, 0, "{again}");
    let menu = "Accounts here: work, second.\n  1) Add another account\n  2) Sign in again to work\n  \
                3) Accept a new certificate for work\n  4) Sign in again to second\n  \
                5) Accept a new certificate for second\n  6) Rename an account\n  7) Quit\n";
    assert!(again.contains(menu), "{again}");
    assert!(
        again.contains(&format!("pinned:    {other_print}")) && again.contains(&format!("presented: {ca_print}")),
        "{again}"
    );
    assert_eq!(fingerprint(&config.join("work.pem")), ca_print);
    assert_eq!(fs::read_to_string(config.join("config.toml")).unwrap(), both, "the config was not rewritten");
    shown.push(again);

    // The second account's command, which is the last, prints another.
    let (kept, last) = both.rsplit_once("\"$TUIMEX_SERVER_PASSWORD\"").unwrap();
    let wrong_password = format!("{kept}not-the-password{last}");
    fs::write(config.join("config.toml"), &wrong_password).unwrap();
    fs::set_permissions(config.join("config.toml"), fs::Permissions::from_mode(0o644)).unwrap();
    let (broken, status) = here.run(&["doctor"]);
    assert_eq!(status, 1, "{broken}");
    assert!(
        broken.contains(&format!("  WRONG  IMAP {imap}: signing in to second failed; run `tuimex setup`")),
        "{broken}"
    );
    assert!(broken.contains(&format!("  WRONG  {} is 644, not 600", config.join("config.toml").display())), "{broken}");
    assert!(
        broken.contains("  ok     SMTP") && broken.ends_with("3 things are wrong\n"),
        "the first account is still well:\n{broken}"
    );
    shown.push(broken.clone());
    fs::remove_file(config.join("config.toml")).unwrap();

    // Everything tuimex made is the owner's alone, and holds no password;
    // nor does anything it showed. Nothing that was only to be found ran.
    let mut made = Vec::new();
    for dir in [".config/tuimex", ".local/share/tuimex", ".cache/tuimex"] {
        made.push(home.join(dir));
        files_under(&home.join(dir), &mut made);
    }
    assert!(made.len() > 12, "{made:?}");
    for path in &made {
        let meta = fs::metadata(path).unwrap();
        let (mode, wanted) = (meta.permissions().mode() & 0o777, if meta.is_dir() { 0o700 } else { 0o600 });
        assert_eq!(mode, wanted, "{}", path.display());
        for (_, password, _) in &here.server.users {
            let held =
                meta.is_file() && fs::read(path).unwrap().windows(password.len()).any(|w| w == password.as_bytes());
            assert!(!held, "{} holds a password", path.display());
        }
    }
    for (_, password, _) in &here.server.users {
        assert!(shown.iter().all(|text| !text.contains(password.as_str())), "a password was shown");
    }
    assert!(!home.join("claude-was-run").exists() && !home.join("vi-was-run").exists());

    println!("real-server: ran setup\n$ tuimex\n{first}\n$ tuimex sync\n{synced}\n$ tuimex doctor\n{well}");
    println!("real-server: ran doctor-broken\n$ tuimex doctor\n{changed}\n$ tuimex doctor\n{broken}");
}

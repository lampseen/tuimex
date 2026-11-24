//! M3's exit as a person meets it: `tuimex sync` against a real server
//! fills a Maildir, and run again it transfers nothing. The server is the
//! one the environment names (`crates/tuimex-engine/tests/spike.rs` lists
//! the variables); with none, the test says it was skipped.

// A test that cannot reach its server should stop with the reason.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn tuimex_sync_fills_a_maildir_and_a_second_run_transfers_nothing() {
    let Ok(host) = env::var("TUIMEX_SERVER_HOST") else {
        println!("real-server tuimex-sync: SKIPPED, no server: set TUIMEX_SERVER_HOST and the rest");
        return;
    };
    let var = |name: &str| env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let home = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("tuimex-real-{}", std::process::id()));
    if home.exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    let config = home.join(".config/tuimex");
    fs::create_dir_all(&config).unwrap();
    // The certificate the user accepted, where `tuimex setup` will put it.
    fs::copy(var("TUIMEX_SERVER_CA"), config.join("work.pem")).unwrap();
    // The password comes from a command, here one that reads this test's
    // environment: it is in no file.
    let text = format!(
        "[[account]]\nname = \"work\"\naddress = \"spike@example.com\"\n\
         imap = {{ host = \"{host}\", port = {}, starttls = false, user = \"{}\" }}\n\
         password_cmd = [\"sh\", \"-c\", 'printf %s \"$TUIMEX_SERVER_PASSWORD\"']\n",
        var("TUIMEX_SERVER_PORT"),
        var("TUIMEX_SERVER_USER"),
    );
    fs::write(config.join("config.toml"), text).unwrap();
    let sync = || {
        let out = Command::new(env!("CARGO_BIN_EXE_tuimex"))
            .arg("sync")
            .env_clear()
            .env("TUIMEX_KEYRING", "none")
            .env("HOME", &home)
            .env("PATH", var("PATH"))
            .env("TUIMEX_SERVER_PASSWORD", var("TUIMEX_SERVER_PASSWORD"))
            .output()
            .unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "tuimex sync failed:\n{stdout}{stderr}");
        stdout
    };
    let first = sync();
    let cur = home.join(".local/share/tuimex/work/cur");
    let files = fs::read_dir(&cur).unwrap().count();
    assert!(files >= 1, "the server's Inbox holds what the exit tests before this one left there");
    assert!(first.contains(&format!("work/INBOX: {files} changed")), "{first}");

    let second = sync();
    assert!(!second.is_empty() && second.lines().all(|line| line.ends_with(": nothing new")), "{second}");
    assert_eq!(fs::read_dir(&cur).unwrap().count(), files);
    let password = var("TUIMEX_SERVER_PASSWORD");
    assert!(!first.contains(&password) && !second.contains(&password));
    println!("real-server: ran tuimex-sync\n$ tuimex sync\n{first}$ tuimex sync\n{second}");
}

/// A person at a terminal over accounts of the real server, and everyone
/// else on it: a home directory with a config, `on_pty.py` to type and read
/// the screen, and `mail_server.py` as the other client.
struct Desk {
    home: PathBuf,
    /// The command line of `mail_server.py`, for a script's `run` lines.
    helper: String,
    /// No two runs share a subject, so what an earlier one left on the
    /// server is not mistaken for this one's.
    run: String,
}

fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// Where the first `name` on this test's own `PATH` is.
fn tool(name: &str) -> PathBuf {
    let found = env::split_paths(&var("PATH")).map(|dir| dir.join(name)).find(|path| path.is_file());
    found.unwrap_or_else(|| panic!("{name} is not on the PATH"))
}

impl Desk {
    /// None, with a line saying the test did not run, when the environment
    /// names no server: a skip must never read as a pass.
    fn new(test: &str) -> Option<Desk> {
        if env::var("TUIMEX_SERVER_HOST").is_err() {
            println!("real-server {test}: SKIPPED, no server: set TUIMEX_SERVER_HOST and the rest");
            return None;
        }
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let home =
            manifest.join("../../target/test-scratch").join(format!("tuimex-real-{test}-{}", std::process::id()));
        if home.exists() {
            fs::remove_dir_all(&home).unwrap();
        }
        // Everything tuimex and the script can run is in this directory:
        // the tools named here, and stand-ins for whatever would open a
        // browser or a viewer, which only write down what they were handed.
        // No program that opens anything can be reached from it.
        let bin = home.join("bin");
        fs::create_dir_all(&bin).unwrap();
        for name in ["sh", "python3", "cat", "cp", "sleep", "ls", "grep", "test"] {
            std::os::unix::fs::symlink(tool(name), bin.join(name)).unwrap();
        }
        let script = |name: &str, text: String| {
            use std::os::unix::fs::PermissionsExt;
            fs::write(bin.join(name), format!("#!{}\n{text}\n", bin.join("sh").display())).unwrap();
            fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o700)).unwrap();
        };
        let opened = home.join("opened");
        fs::create_dir_all(&opened).unwrap();
        // What `open` tries, in its order. Each writes its argument to a
        // list and, when that is a file, keeps a copy with its mode.
        for opener in ["xdg-open", "gio", "gnome-open", "kde-open", "kde-open5", "kde-open6", "wslview", "open"] {
            let dir = opened.display();
            let keep = format!(
                "for last; do :; done\nprintf '%s\\n' \"$last\" >> {dir}/list\n\
                 if test -f \"$last\"; then cp -p \"$last\" {dir}/; fi"
            );
            script(opener, keep);
        }
        script("editor", "printf 'written in the editor\\n' >> \"$1\"".to_string());
        let helper = format!("python3 {}", manifest.join("tests/mail_server.py").display());
        let run =
            format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
        Some(Desk { home, helper, run })
    }

    /// Writes the config: each account as its name, the server's user, the
    /// variable its password is in, and the user it hands mail over as
    /// when the server takes mail. `rest` is every other table.
    fn config(&self, accounts: &[(&str, &str, &str, &str)], rest: &str) {
        let config = self.home.join(".config/tuimex");
        fs::create_dir_all(&config).unwrap();
        let (host, port) = (var("TUIMEX_SERVER_HOST"), var("TUIMEX_SERVER_PORT"));
        let mut text = String::new();
        for (name, user, password, sender) in accounts {
            // The certificate the user accepted, and a password that is in
            // no file: a command reads it from this test's environment.
            fs::copy(var("TUIMEX_SERVER_CA"), config.join(format!("{name}.pem"))).unwrap();
            text += &format!(
                "[[account]]\nname = \"{name}\"\naddress = \"{user}@example.com\"\n\
                 imap = {{ host = \"{host}\", port = {port}, starttls = false, user = \"{user}\" }}\n\
                 password_cmd = [\"sh\", \"-c\", 'printf %s \"${password}\"']\n"
            );
            if let Ok(smtps) = env::var("TUIMEX_SERVER_SMTPS_PORT") {
                text +=
                    &format!("smtp = {{ host = \"{host}\", port = {smtps}, starttls = false, user = \"{sender}\" }}\n");
            }
            text += "\n";
        }
        fs::write(config.join("config.toml"), text + rest).unwrap();
    }

    /// Has the other client do something, which must succeed.
    fn server(&self, arguments: &str) {
        let ran = Command::new("sh").arg("-c").arg(format!("{} {arguments}", self.helper)).output().unwrap();
        assert!(ran.status.success(), "{arguments}: {}", String::from_utf8_lossy(&ran.stderr));
    }

    /// Appends `message` to a folder as another client would.
    fn put(&self, user: &str, folder: &str, message: &str) {
        let file = self.home.join("message.eml");
        fs::write(&file, message.replace('\n', "\r\n")).unwrap();
        self.server(&format!("{user} put {folder} {}", file.display()));
    }

    /// Runs tuimex on a terminal through `steps`, which end by leaving it,
    /// and returns everything it wrote there. `zone` is the `TZ` it runs in.
    fn terminal(&self, steps: &[String], zone: &str) -> String {
        let seen = self.on_a_terminal(&[], steps, zone);
        let left = seen.rfind("\x1b[?1049l").expect("it left the alternate screen");
        assert!(!seen[left..].contains("\x1b[?1049h"), "and did not go back to it");
        seen
    }

    /// The home, the tools and the server's variables, and nothing else
    /// of this test's own environment.
    fn at_this_desk(&self, command: &mut Command, zone: &str) {
        let bin = self.home.join("bin");
        let server = ["HOST", "PORT", "PASSWORD", "PASSWORD3", "USER3", "CA"];
        command
            .env_clear()
            .env("TUIMEX_KEYRING", "none")
            .env("HOME", &self.home)
            .env("PATH", &bin)
            .env("TERM", "xterm-256color")
            .env("TZ", zone)
            .env("EDITOR", bin.join("editor"))
            .envs(server.iter().filter_map(|key| {
                let name = format!("TUIMEX_SERVER_{key}");
                env::var(&name).ok().map(|value| (name, value))
            }));
    }

    /// Runs `tuimex <args>` on a terminal through `steps`, which end by
    /// leaving it with 0, and returns everything it wrote there.
    fn on_a_terminal(&self, args: &[&str], steps: &[String], zone: &str) -> String {
        let script = self.home.join("steps");
        fs::write(&script, steps.join("\n") + "\n").unwrap();
        let mut command = Command::new(self.home.join("bin/python3"));
        command.arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/on_pty.py")).arg("");
        command.arg(env!("CARGO_BIN_EXE_tuimex")).args(args);
        self.at_this_desk(&mut command, zone);
        let out = command.env("ON_PTY_SCRIPT", &script).env("ON_PTY_SIZE", "40x120").output().unwrap();
        let (seen, said) = (String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr));
        assert_eq!(out.status.code(), Some(0), "tuimex did not leave with 0:\n{said}");
        for password in ["PASSWORD", "PASSWORD3"].iter().filter_map(|key| env::var(format!("TUIMEX_SERVER_{key}")).ok())
        {
            assert!(!seen.contains(&password), "a password is not on the screen");
        }
        seen
    }

    /// `tuimex sync` with nobody to answer a question, which must succeed.
    fn sync(&self) -> String {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tuimex"));
        self.at_this_desk(&mut command, "UTC");
        let out = command.arg("sync").stdin(std::process::Stdio::null()).output().unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "tuimex sync failed:\n{stdout}{stderr}");
        stdout
    }

    /// What the stand-ins for the system's opener were handed, in order.
    fn opened(&self) -> Vec<String> {
        let list = fs::read_to_string(self.home.join("opened/list")).unwrap_or_default();
        list.lines().map(String::from).collect()
    }
}

/// The join's exit, as a person meets it: `tuimex` on a terminal over
/// three accounts of a real server, two of them named alike.
#[test]
fn the_interface_shows_three_accounts_follows_the_server_changes_it_and_searches_them_all() {
    let Some(desk) = Desk::new("tui") else { return };
    let (first, second, third) = (var("TUIMEX_SERVER_USER"), var("TUIMEX_SERVER_USER2"), var("TUIMEX_SERVER_USER3"));
    desk.config(
        &[
            ("gmail-work", &first, "TUIMEX_SERVER_PASSWORD", &first),
            ("gmail-home", &second, "TUIMEX_SERVER_PASSWORD", &second),
            ("proton", &third, "TUIMEX_SERVER_PASSWORD3", &third),
        ],
        "",
    );
    let (helper, run) = (&desk.helper, &desk.run);
    let (at_work, at_home, late) = (format!("join-work-{run}"), format!("join-home-{run}"), format!("join-late-{run}"));
    for user in [&first, &second, &third] {
        desk.server(&format!("{user} create Archive"));
        desk.server(&format!("{user} create Trash"));
    }
    desk.server(&format!("{first} append INBOX {at_work}"));
    desk.server(&format!("{second} append INBOX {at_home}"));
    // One message in each account whose body, and nothing else of it,
    // holds a word no other message has. The third account's is filed.
    let word = format!("zb{run}");
    let hidden = |user: &str, folder: &str, who: &str| {
        let subject = format!("find-{who}-{run}");
        let id = format!("{subject}@example.net");
        let body = format!("\nThe ferry leaves for {word} at noon on Friday.\n");
        desk.put(
            user,
            folder,
            &mail(&format!("Sender {who} <{who}@example.net>"), "ada@example.org", &subject, &id, &body),
        );
    };
    hidden(&first, "INBOX", "one");
    hidden(&second, "INBOX", "two");
    hidden(&third, "Archive", "three");
    let steps = [
        format!("wait {at_work}"),
        format!("wait {at_home}"),
        // Each row names its account, whole while there is room.
        "wait gmail-work   Sender one".to_string(),
        "wait gmail-home   Sender two".to_string(),
        // Another client adds a message, and nothing is typed.
        format!("run {helper} {second} append INBOX {late}"),
        format!("wait {late}"),
        // The filter puts the cursor on its one match; Esc drops the
        // filter and leaves the cursor there.
        format!("keys /{late}"),
        "wait 1 here".to_string(),
        "keys \\x1b".to_string(),
        "wait 1 message".to_string(),
        "keys e".to_string(),
        format!("gone {late}"),
        format!("run {helper} {second} holds Archive {late} 1"),
        format!("run {helper} {second} holds INBOX {late} 0"),
        "keys u".to_string(),
        format!("wait {late}"),
        format!("run {helper} {second} holds INBOX {late} 1"),
        format!("run {helper} {second} holds Archive {late} 0"),
        // A search of all mail finds the word in the three bodies, in
        // every account and in a folder that is no inbox.
        format!("keys /{word}\\r"),
        format!("wait Search {word}"),
        "wait 1 of 3".to_string(),
        "wait gmail-work   Sender one".to_string(),
        "wait gmail-home   Sender two".to_string(),
        "wait proton       Sender three".to_string(),
        // Its row says its folder too, which leaves room for less of the excerpt.
        format!("wait find-three-{run}  The ferry leaves for"),
        "keys \\x1b".to_string(),
        format!("gone Search {word}"),
        format!("wait {late}"),
        // `gt` shows each account alone, in the config's order, then all.
        "keys gt".to_string(),
        "wait Showing gmail-work only".to_string(),
        format!("gone {late}"),
        "keys gt".to_string(),
        "wait gmail-home only".to_string(),
        format!("wait {late}"),
        "keys gt".to_string(),
        "wait Showing proton only".to_string(),
        "keys gt".to_string(),
        "wait Showing every account".to_string(),
        format!("wait {at_work}"),
        // With no Archive on the server, the server refuses the move.
        format!("keys /{late}"),
        "wait 1 here".to_string(),
        "keys \\x1b".to_string(),
        format!("run {helper} {second} delete Archive"),
        "keys e".to_string(),
        "wait gmail-home refused it".to_string(),
        format!("wait {late}"),
        format!("run {helper} {second} holds INBOX {late} 1"),
        "keys :q\\r".to_string(),
    ];
    desk.terminal(&steps, "UTC");
    println!(
        "real-server: ran tui: three accounts listed ({at_work}, {at_home}, find-three-{run}); {late} arrived with no \
         key pressed; e archived it and u brought it back, on the server; a search for {word} found it in the body \
         of a message of each account, one of them archived; gt showed each account alone; a refused archive was \
         put back and said; :q left with 0"
    );
}

/// Whether the server takes mail to send; a test that needs it says it
/// did not run where it does not.
fn takes_mail(test: &str) -> bool {
    let takes = env::var("TUIMEX_SERVER_SMTPS_PORT").is_ok();
    if !takes {
        println!("real-server {test}: NOT RUN, this server takes no mail to send (TUIMEX_SERVER_SMTPS_PORT)");
    }
    takes
}

/// A message as another client would have left it in a folder.
fn mail(from: &str, to: &str, subject: &str, id: &str, rest: &str) -> String {
    format!(
        "From: {from}\nTo: {to}\nSubject: {subject}\nMessage-ID: <{id}>\n\
         Date: Mon, 23 Nov 2026 03:04:00 +0000\n{rest}"
    )
}

/// The keys that write a new message to `to` and ask to send it: the
/// body comes from `$EDITOR`.
fn write_to(to: &str, subject: &str) -> Vec<String> {
    vec![
        "wait New message".to_string(),
        format!("keys {to}\\t\\t\\t{subject}\\t\\r"),
        "wait written in the editor".to_string(),
        "keys :send\\r".to_string(),
        "wait Send to 1 recipient?".to_string(),
    ]
}

/// PLAN §10, "Mail can be sent from several accounts", as a person does
/// it: written, summarised, held, sent, stopped and answered, from two
/// accounts whose servers know them by different passwords.
#[test]
fn mail_is_written_sent_stopped_and_answered_from_two_accounts() {
    let Some(desk) = Desk::new("tui-send") else { return };
    if !takes_mail("tui-send") {
        return;
    }
    let (first, third) = (var("TUIMEX_SERVER_USER"), var("TUIMEX_SERVER_USER3"));
    let accounts =
        [("work", &*first, "TUIMEX_SERVER_PASSWORD", &*first), ("home", &*third, "TUIMEX_SERVER_PASSWORD3", &*third)];
    desk.config(&accounts, "[send]\ndelay = \"4s\"\n");
    let (helper, run) = (&desk.helper, &desk.run);
    let subject = |what: &str| format!("send-{what}-{run}");
    let (one, two, stopped, asked) = (subject("one"), subject("two"), subject("stopped"), subject("asked"));
    for user in [&first, &third] {
        desk.server(&format!("{user} create Sent"));
        // What is answered below, as both of them hold it.
        let (from, to) = (format!("Third <{third}@example.com>"), format!("{first}@example.com"));
        desk.put(user, "INBOX", &mail(&from, &to, &asked, &format!("{asked}@example.com"), "\nShall we?\n"));
    }
    let mut steps = vec![format!("wait {asked}"), ":account work\\r".to_string(), "keys c".to_string()];
    steps[1] = "keys :account work\\r".to_string();
    // From the account the cursor is in.
    steps.extend(write_to(&format!("{third}@example.com"), &one));
    steps.extend([
        format!("wait From     {first}@example.com"),
        "keys y".to_string(),
        "wait Sending to 1 recipient in 4s".to_string(),
        // The seconds fall with no key pressed.
        "wait sending in 2s".to_string(),
        "wait Sent to 1 recipient".to_string(),
        format!("run {helper} {third} holds INBOX {one} 1"),
        format!("run {helper} {first} holds Sent {one} 1"),
        // From the other account: Shift-Tab to From, Right to its identity.
        "keys c".to_string(),
        "wait New message".to_string(),
        "keys \\x1b[Z\\x1b[C\\t".to_string(),
    ]);
    steps.extend(write_to(&format!("{first}@example.com"), &two));
    steps.extend([
        format!("wait From     {third}@example.com"),
        "keys y".to_string(),
        "wait Sending to 1 recipient in 4s".to_string(),
        "wait Sent to 1 recipient".to_string(),
        format!("run {helper} {first} holds INBOX {two} 1"),
        format!("run {helper} {third} holds Sent {two} 1"),
        format!("run {helper} {first} holds Sent {two} 0"),
        // `u` within the delay: back in Drafts, and nothing leaves.
        "keys c".to_string(),
    ]);
    steps.extend(write_to(&format!("{third}@example.com"), &stopped));
    steps.extend([
        "keys y".to_string(),
        "wait Sending to 1 recipient in 4s".to_string(),
        "keys u".to_string(),
        "wait Not sent".to_string(),
        "run sleep 6".to_string(),
        format!("run {helper} {third} holds INBOX {stopped} 0"),
        "keys :section Drafts\\r".to_string(),
        format!("wait {stopped}"),
        // A reply carries what threads it under the message answered.
        "keys :section Inbox\\r".to_string(),
        format!("keys /{asked}"),
        "wait 1 here".to_string(),
        "keys \\x1b".to_string(),
        "wait 1 message".to_string(),
        "keys r".to_string(),
        format!("wait Re: {asked}"),
        "keys \\r".to_string(),
        "wait written in the editor".to_string(),
        "keys :send\\r".to_string(),
        "wait Send to 1 recipient?".to_string(),
        "keys y".to_string(),
        "wait Sending to 1 recipient in 4s".to_string(),
        "wait Sent to 1 recipient".to_string(),
        format!("run {helper} {third} header INBOX {asked} 'In-Reply-To: <{asked}@example.com>'"),
        format!("run {helper} {third} header INBOX {asked} 'References: <{asked}@example.com>'"),
        // Where both are, the reply sits under what it answers.
        "keys :account home\\r".to_string(),
        format!("wait ▸ {asked}"),
        "keys :q\\r".to_string(),
    ]);
    desk.terminal(&steps, "UTC");
    println!(
        "real-server: ran tui-send ({one} from work and {two} from home, each delivered once and in its own \
         account's Sent once; {stopped} was stopped with u and never left; the reply to {asked} arrived with \
         In-Reply-To and References and is threaded under it)"
    );
}

/// Each account hands mail to its own server as itself: `work` is given
/// a name its server does not know, and `home` a password that is only
/// its own. What `work` cannot send stays in the Outbox and says why.
#[test]
fn a_send_uses_its_own_accounts_sign_in_and_one_the_server_refuses_stays_in_the_outbox_with_the_reason() {
    let Some(desk) = Desk::new("tui-send-refused") else { return };
    if !takes_mail("tui-send-refused") {
        return;
    }
    let (first, third) = (var("TUIMEX_SERVER_USER"), var("TUIMEX_SERVER_USER3"));
    let accounts =
        [("work", &*first, "TUIMEX_SERVER_PASSWORD", "nobody"), ("home", &*third, "TUIMEX_SERVER_PASSWORD3", &*third)];
    desk.config(&accounts, "[send]\ndelay = \"1s\"\n");
    let (helper, run) = (&desk.helper, &desk.run);
    let (refused, taken) = (format!("refused-{run}"), format!("taken-{run}"));
    desk.server(&format!("{first} append INBOX refused-here-{run}"));
    // Nor has `home` a folder for sent mail under any name PLAN §4.1 tries.
    for folder in ["Sent", "Sent\\ Items", "Sent\\ Messages"] {
        desk.server(&format!("{third} absent {folder}"));
    }
    let mut steps = vec![format!("wait refused-here-{run}"), "keys :account work\\r".to_string(), "keys c".to_string()];
    steps.extend(write_to(&format!("{third}@example.com"), &refused));
    steps.extend([
        "keys y".to_string(),
        "wait 1 not sent".to_string(),
        "keys :section Outbox\\r".to_string(),
        format!("wait {refused}"),
        "wait signing in to work failed".to_string(),
        format!("run {helper} {third} holds INBOX {refused} 0"),
        // The other account, in the same window, sends.
        "keys c".to_string(),
        "wait New message".to_string(),
        "keys \\x1b[Z\\x1b[C\\t".to_string(),
    ]);
    steps.extend(write_to(&format!("{first}@example.com"), &taken));
    steps.extend([
        format!("wait From     {third}@example.com"),
        "keys y".to_string(),
        "wait Sending to 1 recipient".to_string(),
        format!("run {helper} {first} holds INBOX {taken} 1"),
        // Sent, and the copy is kept here: the server has nowhere for it.
        "wait E home would not keep a copy of what was sent".to_string(),
        "wait it is in Sent on this".to_string(),
        "keys :account\\r".to_string(),
        "keys :section Sent\\r".to_string(),
        format!("wait {taken}"),
        "keys :section Outbox\\r".to_string(),
        format!("wait {refused}"),
        "keys :q\\r".to_string(),
    ]);
    desk.terminal(&steps, "UTC");
    println!(
        "real-server: ran tui-send-refused ({refused} stayed in the Outbox with the reason on its row; {taken} \
         went from the other account under its own sign-in, and with no Sent folder on its server the copy stayed \
         here and that was said)"
    );
}

/// Reading, as PLAN §10 promises it of a first release: a body that was
/// not downloaded arrives when the cursor rests on its row, the preview
/// shows all of it, and it is kept; HTML is
/// read with nothing fetched; a link that deceives is asked about; a file
/// is saved, and opened by handing its path to the system's opener.
#[test]
fn mail_is_read_with_nothing_fetched_and_its_links_and_files_go_to_the_systems_opener_only_when_asked() {
    let Some(desk) = Desk::new("tui-read") else { return };
    let first = var("TUIMEX_SERVER_USER");
    desk.config(
        &[("work", &first, "TUIMEX_SERVER_PASSWORD", &first)],
        "[sync]\nmax_body = \"1KB\"\n\n[paths]\ndownloads = \"~/Downloads\"\n",
    );
    // Whatever a message names that is not attached is here: a connection
    // to it is the fetch that must never happen (ADR-008).
    let remote = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    remote.set_nonblocking(true).unwrap();
    let there = format!("127.0.0.1:{}", remote.local_addr().unwrap().port());
    let run = &desk.run;
    let (late, page, files) = (format!("late-{run}"), format!("page-{run}"), format!("files-{run}"));
    let (from, to) = ("Grace Hopper <grace@example.net>", format!("{first}@example.com"));
    let long = format!(
        "\nThe first line of a long body.\n{}",
        "Another line of it, to be more than the config allows.\n".repeat(40)
    );
    desk.put(&first, "INBOX", &mail(from, &to, &late, &format!("{late}@example.net"), &long));
    let html = format!(
        "MIME-Version: 1.0\nContent-Type: text/html; charset=utf-8\n\n\
         <html><body><script>document.location='http://{there}/script'</script>\n\
         <p>The quarterly letter.</p><img src=\"http://{there}/banner.png\" alt=\"banner\">\n\
         <img src=\"https://img.example.net/chart.png\" alt=\"chart\" width=\"560\" height=\"240\">\n\
         <img src=\"https://photos.example.org/team.jpg\" alt=\"team\">\n\
         <img src=\"https://open.mailtrack.io/pixel.gif\" width=\"1\" height=\"1\">\n\
         <img src=\"http://{there}/open.gif\" width=\"1\" height=\"1\">\n\
         <p><a href=\"http://{there}/login\">https://example.com/login</a></p></body></html>\n"
    );
    desk.put(&first, "INBOX", &mail(from, &to, &page, &format!("{page}@example.net"), &html));
    let part = |kind: &str, name: &str, text: &str| {
        format!("--edge\nContent-Type: {kind}\nContent-Disposition: attachment; filename=\"{name}\"\n\n{text}\n")
    };
    let office = "application/vnd.openxmlformats-officedocument";
    let attached = format!(
        "MIME-Version: 1.0\nContent-Type: multipart/mixed; boundary=edge\n\n\
         --edge\nContent-Type: text/plain\n\nFive files.\n{}{}{}{}{}--edge--\n",
        part("application/pdf", "report.pdf", "%PDF-1.4 synthetic"),
        part(&format!("{office}.wordprocessingml.document"), "notes.docx", "synthetic docx"),
        part(&format!("{office}.spreadsheetml.sheet"), "../../sheet.xlsx", "synthetic xlsx"),
        part("image/png", "picture.png", "synthetic png"),
        part("application/octet-stream", "README", "synthetic and without an extension"),
    );
    desk.put(&first, "INBOX", &mail(from, &to, &files, &format!("{files}@example.net"), &attached));
    let cur = desk.home.join(".local/share/tuimex/work/cur").display().to_string();
    let find = |subject: &str| {
        vec![
            format!("keys /{subject}"),
            "wait 1 here".to_string(),
            "keys \\x1b".to_string(),
            "wait 1 message".to_string(),
        ]
    };
    let mut steps = vec![format!("wait {late}"), format!("wait {page}"), format!("wait {files}")];
    // ADR-028: a header block, marked in its name, until it is opened.
    steps.push(format!("run grep -l {late} {cur}/* | grep -q H=1"));
    steps.extend(find(&late));
    // The cursor rests on its row and nothing is typed: the preview asks
    // for the body, shows the whole message, and scrolls to its end.
    steps.extend([
        "wait The first line of a long body.".to_string(),
        format!("run grep -l 'The first line of a long body' {cur}/* | grep -qv H=1"),
        "keys \\x1bj".to_string(),
        "wait lines 1-".to_string(),
        "keys G".to_string(),
        "gone lines 1-".to_string(),
        "wait Another line of it, to be more than the config allows.".to_string(),
        "keys \\x1bk".to_string(),
        "gone lines ".to_string(),
    ]);
    steps.extend([
        "keys \\r".to_string(),
        "wait The first line of a long body.".to_string(),
        // 03:04 UTC, on a wall clock nine hours east of it.
        "wait Mon 23 Nov 2026 12:04".to_string(),
        format!("run grep -l 'The first line of a long body' {cur}/* | grep -qv H=1"),
        "keys :q\\r".to_string(),
        "wait Enter open".to_string(),
    ]);
    steps.extend(find(&page));
    steps.extend([
        "keys \\r".to_string(),
        "wait The quarterly letter.".to_string(),
        "wait Remote content not loaded".to_string(),
        "wait tracker: Mailtrack".to_string(),
        "keys o".to_string(),
        "wait opens".to_string(),
        "keys 1".to_string(),
        "wait opens 127.0.0.1. Open?".to_string(),
        "keys y".to_string(),
        "wait Opening 127.0.0.1".to_string(),
        "keys B".to_string(),
        // Two pictures a yes would load, so it asks, naming their hosts
        // and neither tracker's.
        "wait Load 2 images from img.example.net, photos.example.org?".to_string(),
        "keys y".to_string(),
        "wait 2 images load from 2 hosts".to_string(),
        "keys :q\\r".to_string(),
        "wait Enter open".to_string(),
    ]);
    steps.extend(find(&files));
    steps.extend([
        "keys \\r".to_string(),
        "wait Five files.".to_string(),
        "keys a".to_string(),
        "wait Files".to_string(),
    ]);
    steps.extend(["keys \\r".to_string(), "wait Saved ".to_string()]);
    for (n, name) in [(1, "report.pdf"), (2, "notes.docx"), (3, "sheet.xlsx"), (4, "picture.png"), (5, "README")] {
        steps.extend([
            format!("keys :open {n}\\r"),
            format!("wait Open {name} ("),
            "keys y".to_string(),
            format!("wait Opening {name}"),
        ]);
    }
    steps.extend(["keys :save 3\\r".to_string(), "wait Saved ".to_string(), "keys :qa\\r".to_string()]);
    desk.terminal(&steps, "Asia/Tokyo");

    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &std::path::Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let opened = desk.opened();
    let shown = desk.home.join(".cache/tuimex/shown").display().to_string();
    assert_eq!(opened[0], format!("http://{there}/login"), "the link, as it really is");
    let names: Vec<&str> = opened[1..].iter().map(|path| path.rsplit('/').next().unwrap()).collect();
    assert_eq!(names, ["message.html", "report.pdf", "notes.docx", "sheet.xlsx", "picture.png", "README"]);
    for path in &opened[1..] {
        // Under the cache directory, in a directory of its own, and gone
        // again now that tuimex has left (ADR-011).
        let own = path.strip_prefix(&format!("{shown}/")).unwrap_or_else(|| panic!("{path} is not under {shown}"));
        assert_eq!(own.matches('/').count(), 1, "{path}");
        assert!(!std::path::Path::new(path).exists(), "{path} was left behind");
        let kept = desk.home.join("opened").join(path.rsplit('/').next().unwrap());
        assert_eq!(mode(&kept), 0o600, "{path} was handed over readable by its owner alone");
    }
    let copy = fs::read_to_string(desk.home.join("opened/message.html")).unwrap();
    assert!(copy.contains("The quarterly letter.") && copy.contains("Content-Security-Policy"), "{copy}");
    assert!(!copy.contains("<script") && !copy.contains("document.location"), "{copy}");
    // The yes was to the two pictures: the copy may load from their
    // hosts alone, and holds neither tracker's address to ask for.
    let policy = "img-src data: https://img.example.net https://photos.example.org;";
    assert!(copy.contains(policy) && copy.matches("src=\"http").count() == 2, "{copy}");
    assert!(!copy.contains("mailtrack") && !copy.contains("open.gif") && !copy.contains("banner.png"), "{copy}");
    assert_eq!(fs::read_to_string(desk.home.join("opened/report.pdf")).unwrap().trim_end(), "%PDF-1.4 synthetic");
    for saved in ["report.pdf", "sheet.xlsx"] {
        let path = desk.home.join("Downloads").join(saved);
        assert_eq!(mode(&path), 0o600, "{saved}");
    }
    assert!(!desk.home.join("sheet.xlsx").exists(), "a name is never a path out of the download directory");
    let came = remote.accept().map(|(_, from)| from);
    assert!(came.is_err(), "something connected to what a message named: {came:?}");
    println!(
        "real-server: ran tui-read ({late} was a header block until the cursor rested on its row, when the preview \
         showed the whole message and scrolled to its end, and is whole on disk now, shown in the reader at \
         12:04 in Tokyo; {page} said what it did not load, a yes to its two pictures allowed their hosts and \
         neither tracker, and nothing connected to {there}; its deceptive link was asked about; the opener was handed the link and then, each under the cache directory, {names:?})"
    );
}

/// PLAN §4.1 on a server that marks no folder and has none of the usual
/// names: the Archive is made after asking, by the move that first needs
/// it, and `u` still brings the message back. A Trash is made the same
/// way (ADR-047).
#[test]
fn an_archive_and_a_trash_are_each_made_on_the_server_after_asking() {
    let Some(desk) = Desk::new("tui-archive") else { return };
    let third = var("TUIMEX_SERVER_USER3");
    desk.config(&[("solo", &third, "TUIMEX_SERVER_PASSWORD3", &third)], "");
    let (helper, run) = (&desk.helper, &desk.run);
    let kept = format!("keep-{run}");
    // What the server's names for a person's folders begin with, where
    // that is anything: the folders are made under it (ADR-048).
    let prefix = env::var("TUIMEX_SERVER_PREFIX").unwrap_or_default();
    let (archive, trash) = (format!("{prefix}Archive"), format!("{prefix}Trash"));
    for folder in ["Archive", "Trash", "Deleted\\ Items", "Deleted\\ Messages"] {
        desk.server(&format!("{third} absent {prefix}{folder}"));
    }
    if !prefix.is_empty() {
        // The listing shows where folders go only once there is one.
        desk.server(&format!("{third} create {prefix}Notes"));
    }
    desk.server(&format!("{third} append INBOX {kept}"));
    let mut steps = vec![format!("wait {kept}"), format!("keys /{kept}")];
    steps.extend(["wait 1 here", "keys \\x1b", "wait 1 message", "keys e"].map(String::from));
    steps.extend([
        "wait solo has no Archive folder. Create one on its server?".to_string(),
        "keys y".to_string(),
        "wait Archived 1 message".to_string(),
        format!("run {helper} {third} holds {archive} {kept} 1"),
        format!("run {helper} {third} holds INBOX {kept} 0"),
        "keys u".to_string(),
        "wait Undone".to_string(),
        format!("run {helper} {third} holds INBOX {kept} 1"),
        format!("run {helper} {third} holds {archive} {kept} 0"),
        // Asked once: the folder is there now.
        "keys e".to_string(),
        "wait Archived 1 message".to_string(),
        format!("run {helper} {third} holds {archive} {kept} 1"),
        "keys u".to_string(),
        "wait Undone".to_string(),
        format!("run {helper} {third} holds INBOX {kept} 1"),
        "keys d".to_string(),
        "wait solo has no Trash folder. Create one on its server?".to_string(),
        format!("run {helper} {third} holds INBOX {kept} 1"),
        "keys y".to_string(),
        "wait Trashed 1 message".to_string(),
        format!("run {helper} {third} holds {trash} {kept} 1"),
        format!("run {helper} {third} holds INBOX {kept} 0"),
        "keys u".to_string(),
        "wait Undone".to_string(),
        format!("run {helper} {third} holds INBOX {kept} 1"),
        format!("run {helper} {third} holds {trash} {kept} 0"),
        "keys :q\\r".to_string(),
    ]);
    desk.terminal(&steps, "UTC");
    println!(
        "real-server: ran tui-archive (under {prefix:?}, {kept}: asked \"solo has no Archive folder. Create one on its server?\", \
         then archived into a folder made on the server, and u brought it back; \
         d asked \"solo has no Trash folder. Create one on its server?\", trashed into one made there, and u brought it back)"
    );
}

/// `:empty-trash` on a real server: what was in the Trash is gone from the
/// server, by name, and nothing else is.
#[test]
fn the_trash_is_emptied_on_the_server_and_nothing_else_is() {
    let Some(desk) = Desk::new("tui-empty-trash") else { return };
    let second = var("TUIMEX_SERVER_USER2");
    desk.config(&[("home", &second, "TUIMEX_SERVER_PASSWORD", &second)], "");
    let (helper, run) = (&desk.helper, &desk.run);
    let (gone, stays) = (format!("trashed-{run}"), format!("bystander-{run}"));
    let trash = desk.home.join(".local/share/tuimex/home/.Trash/cur").display().to_string();
    desk.server(&format!("{second} create Trash"));
    desk.server(&format!("{second} append INBOX {gone}"));
    desk.server(&format!("{second} append INBOX {stays}"));
    let mut steps = vec![format!("wait {stays}"), format!("keys /{gone}")];
    steps.extend(["wait 1 here", "keys \\x1b", "wait 1 message", "keys d"].map(String::from));
    steps.extend([
        "wait Trashed 1 message".to_string(),
        format!("run {helper} {second} holds Trash {gone} 1"),
        // The move ends by giving the copy here its number in the Trash,
        // which is what the server is then told to delete (ADR-041): the
        // number is in the file's name.
        format!("until grep -l {gone} {trash}/*,U=*"),
        "keys :empty-trash home\\r".to_string(),
        "wait in home's Trash for good?".to_string(),
        "keys y".to_string(),
        "wait Deleted 1 message from home's Trash".to_string(),
        format!("run {helper} {second} holds Trash {gone} 0"),
        format!("run {helper} {second} holds INBOX {gone} 0"),
        format!("run {helper} {second} holds INBOX {stays} 1"),
        "keys :q\\r".to_string(),
    ]);
    desk.terminal(&steps, "UTC");
    println!(
        "real-server: ran tui-empty-trash ({gone} is gone from the server's Trash and {stays} is still in its Inbox)"
    );
}

/// ADR-050 against a real server: an account renamed from setup's menu
/// keeps the mail it has, signs in as before under its pinned
/// certificate, and still owes the server what it owed it. Named to run
/// after the tests that count the rows of a shared Inbox on one screen.
#[test]
fn with_a_new_name_from_setup_an_account_keeps_its_mail_its_sign_in_and_what_it_owed_the_server() {
    let Some(desk) = Desk::new("tui-rename") else { return };
    let second = var("TUIMEX_SERVER_USER2");
    desk.config(&[("home", &second, "TUIMEX_SERVER_PASSWORD", &second)], "");
    let run = &desk.run;
    let (stays, owed) = (format!("stays-{run}"), format!("owed-{run}"));
    desk.server(&format!("{second} create Archive"));
    desk.server(&format!("{second} append INBOX {stays}"));
    desk.server(&format!("{second} append INBOX {owed}"));
    let (config, data) = (desk.home.join(".config/tuimex"), desk.home.join(".local/share/tuimex"));
    // Every message's file under an account, by the key it was given
    // when it arrived: a message fetched again would have another.
    let keys = |account: &str| -> Vec<String> {
        let mut keys = Vec::new();
        for folder in ["cur", ".Archive/cur", ".Trash/cur", ".Sent/cur"] {
            let files = fs::read_dir(data.join(account).join(folder)).into_iter().flatten().map(Result::unwrap);
            keys.extend(files.map(|file| file.file_name().to_string_lossy().split(',').next().unwrap().to_string()));
        }
        keys.sort();
        keys
    };
    desk.sync();
    let before = keys("home");
    assert!(before.len() >= 2, "{before:?}");

    // Archived with the server out of reach: the move waits in the journal.
    let reachable = fs::read_to_string(config.join("config.toml")).unwrap();
    let port = format!("port = {}", var("TUIMEX_SERVER_PORT"));
    assert!(reachable.contains(&port));
    fs::write(config.join("config.toml"), reachable.replace(&port, "port = 9")).unwrap();
    let mut steps = vec![format!("wait {owed}"), format!("keys /{owed}")];
    steps.extend(["wait 1 here", "keys \\x1b", "wait 1 message", "keys e"].map(String::from));
    steps.extend(["wait Archived 1 message".to_string(), "keys :q\\r".to_string()]);
    desk.terminal(&steps, "UTC");
    fs::write(config.join("config.toml"), &reachable).unwrap();
    desk.server(&format!("{second} holds INBOX {owed} 1"));
    let journal = fs::read_to_string(data.join("home/journal.jsonl")).unwrap();
    assert_eq!(journal.lines().count(), 1, "{journal}");

    let asked = [
        "wait 4) Rename an account",
        "keys 4\\r",
        "wait The new name for home:",
        "keys den\\r",
        "wait This renames home to den in four places",
        "wait Go ahead? [y/N]",
        "keys y\\r",
    ];
    let seen = desk.on_a_terminal(&["setup"], &asked.map(String::from), "UTC");
    assert!(seen.contains("home is now den."), "{seen}");
    let renamed = fs::read_to_string(config.join("config.toml")).unwrap();
    assert_eq!(renamed, reachable.replace("name = \"home\"", "name = \"den\""));
    assert!(config.join("den.pem").exists() && !config.join("home.pem").exists());
    assert!(data.join("den/journal.jsonl").exists() && !data.join("home").exists());

    // No question is asked of nobody, the pinned certificate is the one
    // presented, and the server is told what it was owed.
    let first = desk.sync();
    desk.server(&format!("{second} holds Archive {owed} 1"));
    desk.server(&format!("{second} holds INBOX {owed} 0"));
    desk.server(&format!("{second} holds INBOX {stays} 1"));
    assert!(first.contains("den/INBOX: ") && !first.contains("home/"), "{first}");
    assert_eq!(keys("den"), before, "every message is the file it was: nothing was downloaded again");
    let again = desk.sync();
    assert!(!again.is_empty() && again.lines().all(|line| line.ends_with(": nothing new")), "{again}");
    println!(
        "real-server: ran tui-rename (home became den from setup's menu; {owed}, archived while the server was out \
         of reach, reached its Archive afterwards; the {} messages here kept their files; a second sync \
         transferred nothing)",
        before.len()
    );
}

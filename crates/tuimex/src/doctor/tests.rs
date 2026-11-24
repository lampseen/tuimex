use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tuimex_engine::oauth::{self, Endpoints};
use tuimex_engine::{Keyring, Secret, pin};
use tuimex_fixtures::FakeGoogle;
use tuimex_store::files;

use super::{Doctor, Known, on_path, redact};
use crate::config::Config;
use crate::paths::Dirs;
use crate::setup::tests::{CLIENT_ID, CLIENT_SECRET, FakeNet, MemoryKeyring, PASSWORD, Run, scratch};

fn dirs(root: &Path) -> Dirs {
    Dirs { config: root.join("config"), data: root.join("data"), cache: root.join("cache"), state: root.join("state") }
}

/// A directory of programs that do nothing, to be the whole path.
fn bin(root: &Path, programs: &[&str]) -> String {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for program in programs {
        fs::write(bin.join(program), "#!/bin/sh\n").unwrap();
        fs::set_permissions(bin.join(program), fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin.to_string_lossy().into_owned()
}

struct Case<'a> {
    root: &'a Path,
    keyring: &'a MemoryKeyring,
    net: &'a FakeNet,
    google: Option<&'a FakeGoogle>,
    path: String,
    editor: Option<&'a str>,
}

impl Case<'_> {
    /// Whether nothing was wrong, and what was printed.
    fn run(&self) -> (bool, String) {
        self.go(false)
    }

    /// As `--redact` prints it, with the scratch root for a home.
    fn redacted(&self) -> (bool, String) {
        self.go(true)
    }

    fn go(&self, redact: bool) -> (bool, String) {
        let dirs = dirs(self.root);
        let env = |key: &str| match key {
            "PATH" => Some(self.path.clone()),
            "HOME" => Some(self.root.to_string_lossy().into_owned()),
            "EDITOR" => self.editor.map(String::from),
            _ => None,
        };
        let endpoints = self.google.map_or_else(
            || Endpoints { authorize: "http://127.0.0.1:9/auth".into(), token: "http://127.0.0.1:9/token".into() },
            |google| Endpoints { authorize: google.authorize_url(), token: google.token_url() },
        );
        let doctor = Doctor {
            keyring: self.keyring,
            net: self.net,
            dirs: &dirs,
            config_path: &dirs.config.join("config.toml"),
            endpoints,
            env: &env,
            pictures: "pictures: not asked, as this is not a terminal".to_string(),
            redact,
        };
        let mut printed = Vec::new();
        let fine = doctor.run(&mut |line| printed.push(line));
        (fine, printed.join("\n"))
    }
}

/// A config as setup leaves it: owner-only, in an owner-only directory.
fn config(root: &Path, text: &str) {
    files::private_dir(&root.join("config")).unwrap();
    files::replace(&root.join("config/config.toml"), text.as_bytes()).unwrap();
}

const WORK: &str = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n\
                    imap = { host = \"imap.example.org\", port = 993 }\nsmtp = { host = \"smtp.example.org\", port = 587 }\n";

#[test]
fn a_working_setup_is_reported_line_by_line_and_nothing_is_wrong() {
    let root = scratch("doctor-fine");
    let keyring = MemoryKeyring::default();
    keyring.set("work", &Secret::new(PASSWORD.into())).unwrap();
    let net = FakeNet::self_signed(7);
    config(&root, &format!("{WORK}\n[agent]\nuse = \"claude\"\n"));
    pin::save(&root.join("config"), "work", &[7; 32]).unwrap();
    let path = bin(&root, &["gpg", "vi", "claude"]);
    let case = Case { root: &root, keyring: &keyring, net: &net, google: None, path, editor: None };
    let (fine, printed) = case.run();
    assert!(fine, "{printed}");
    let bin = root.join("bin");
    let expected = format!(
        "
config {config}
  ok     it reads, and has 1 account

account work (ada@example.org)
  ok     its server's certificate is pinned: SHA-256 {print}
  ok     its password is in the keyring
  ok     IMAP imap.example.org:993: signed in, 6 folders
  ok     SMTP smtp.example.org:587: signed in, and nothing was sent

this machine
  ok     the keyring answers
  ok     gpg is {bin}/gpg
  ok     a message is written in {bin}/vi (as neither $VISUAL nor $EDITOR is set)
  note   pictures: not asked, as this is not a terminal
  ok     `:agent` uses the built-in claude, found at {bin}/claude; it was not run

files
  ok     3 files and directories under tuimex's four are readable by you alone

nothing is wrong",
        config = root.join("config/config.toml").display(),
        print = pin::fingerprint(&[7; 32]),
        bin = bin.display(),
    );
    assert_eq!(printed, expected);
    assert!(!printed.contains(PASSWORD));

    let (fine, hidden) = case.redacted();
    assert!(fine, "{hidden}");
    let expected = "
config ~/config/config.toml
  ok     it reads, and has 1 account

account account-1 (address-1)
  ok     its server's certificate is pinned: SHA-256 fingerprint-1
  ok     its password is in the keyring
  ok     IMAP host-1:993: signed in, 6 folders
  ok     SMTP host-2:587: signed in, and nothing was sent

this machine
  ok     the keyring answers
  ok     gpg is ~/bin/gpg
  ok     a message is written in ~/bin/vi (as neither $VISUAL nor $EDITOR is set)
  note   pictures: not asked, as this is not a terminal
  ok     `:agent` uses the built-in claude, found at ~/bin/claude; it was not run

files
  ok     3 files and directories under tuimex's four are readable by you alone

nothing is wrong";
    assert_eq!(hidden, expected);
}

/// What `--redact` prints for a case: the report's own lines under their
/// own marks, holding nothing of the `WORK` fixture, its home or a
/// fingerprint.
fn redacted_names_nothing(case: &Case, printed: &str) -> String {
    let hidden = case.redacted().1;
    let marks = |text: &str| text.lines().map(|line| line.chars().take(8).collect()).collect::<Vec<String>>();
    assert_eq!(marks(&hidden), marks(&printed.replace(&*case.root.to_string_lossy(), "~")), "{hidden}");
    let (home, prints) = (case.root.to_string_lossy(), [pin::fingerprint(&[7; 32]), pin::fingerprint(&[9; 32])]);
    for held in ["work", "ada@", "example", "office", &*home, &prints[0], &prints[1]] {
        assert!(!hidden.to_lowercase().contains(&held.to_lowercase()), "{held:?} is in:\n{hidden}");
    }
    hidden
}

fn known(config: &str, named: &[&str], home: Option<&str>) -> Known {
    let named: Vec<String> = named.iter().map(|name| name.to_string()).collect();
    Known::new(&Config::parse(config).unwrap().accounts, &named, home)
}

#[test]
fn redact_replaces_each_kind_of_value_where_it_stands_whole_and_nothing_else() {
    const TWO: &str = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n\
                       identities = [\"Ada L <countess@example.net>\"]\n\
                       imap = { host = \"imap.mail.example.org\", port = 993, user = \"ada.l\" }\n\
                       smtp = { host = \"mail.example.org\", port = 587 }\n\
                       [[account]]\nname = \"work-2\"\naddress = \"bob@example.org\"\n\
                       imap = { host = \"127.0.0.1\", port = 1143 }\n";
    let (a, b) = (pin::fingerprint(&[7; 32]), pin::fingerprint(&[9; 32]));
    let two = known(TWO, &["office"], Some("/home/ada/"));
    let table: [(&Known, &str, &str); 14] = [
        // Every kind of value; ports, marks and the words around them stay.
        (&two, "account work (ada@example.org)", "account account-1 (address-1)"),
        (&two, "account work-2 (bob@example.org)", "account account-2 (address-3)"),
        (&two, "  ok     IMAP imap.mail.example.org:993: signed in", "  ok     IMAP host-1:993: signed in"),
        (&two, "SMTP mail.example.org:587; IMAP 127.0.0.1:1143", "SMTP host-2:587; IMAP host-3:1143"),
        (&two, "user ada.l, as countess@example.net", "user address-4, as address-2"),
        (&two, "the rename of account work to office", "the rename of account account-1 to account-3"),
        (&two, "gpg is /home/ada/.local/bin/gpg", "gpg is ~/.local/bin/gpg"),
        (&two, "/home/ada/.config/tuimex/work.pem is 644", "~/.config/tuimex/account-1.pem is 644"),
        // The same fingerprint is the same number; another is the next.
        (
            &two,
            &format!("pinned {a}, shown {b}, kept {}", a.to_lowercase()),
            "pinned fingerprint-1, shown fingerprint-2, kept fingerprint-1",
        ),
        // A server's own words: any case, and an address's halves apart.
        (
            &two,
            "NO [AUTH] <ADA@Example.ORG> unknown; ada is not at mx.example.org (relay for countess+x@example.net)",
            "NO [AUTH] <address-1> unknown; user-1 is not at mx.domain-1 (relay for user-2+x@domain-2)",
        ),
        // A name is not found inside a longer word, nor a host or a home
        // inside a longer one of its kind.
        (&two, "the network is not working; rework work-old", "the network is not working; rework work-old"),
        (&two, "127.0.0.11 /home/adam xmail.example.org", "127.0.0.11 /home/adam xmail.domain-1"),
        // An account named as a placeholder is: nothing put in is read again.
        (
            &known(&WORK.replace("\"work\"", "\"host\""), &[], None),
            "host: IMAP imap.example.org:993",
            "account-1: IMAP host-1:993",
        ),
        // No account: the home and a fingerprint are all there is to know.
        (
            &known("", &[], Some("/home/ada")),
            "config /home/ada/c.toml: work, ada@example.org",
            "config ~/c.toml: work, ada@example.org",
        ),
    ];
    for (known, report, expected) in table {
        assert_eq!(redact(report, known), expected, "{report}");
    }
    assert_eq!(redact(&format!("é {a} ü"), &known("", &[], None)), "é fingerprint-1 ü");
}

#[test]
fn each_thing_that_is_broken_is_named_with_what_mends_it_and_the_run_fails() {
    let root = scratch("doctor-broken");
    let keyring = MemoryKeyring::default();
    let path = bin(&root, &["vi"]);
    let net = FakeNet::trusted();
    let case = Case { root: &root, keyring: &keyring, net: &net, google: None, path, editor: None };
    let wrong = |case: &Case, needle: &str| {
        let (fine, printed) = case.run();
        assert!(!fine && printed.contains(needle), "wanted {needle:?} in:\n{printed}");
        redacted_names_nothing(case, &printed);
        printed
    };

    // No config at all, then one with a key that is not one.
    wrong(&case, "  WRONG  reading the config");
    config(&root, "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\npassword = \"hunter2\"\n");
    let printed = wrong(&case, "  WRONG  the config");
    assert!(printed.contains("line 4: unknown key `password`") && !printed.contains("hunter2"), "{printed}");
    config(&root, "");
    wrong(&case, "  WRONG  it has no account; `tuimex setup` adds one");
    // A config refused for a name in it is not one whose names are known
    // from its accounts.
    config(&root, &format!("{WORK}{WORK}"));
    wrong(&case, ": two accounts are named work");
    assert!(case.redacted().1.contains(": two accounts are named account-1"));

    // Nothing in the keyring for the account.
    config(&root, WORK);
    wrong(&case, "  WRONG  there is no password for work in the keyring, and no password_cmd; run `tuimex setup`");
    // A password the server no longer takes.
    keyring.set("work", &Secret::new("an-old-password".into())).unwrap();
    let printed = wrong(&case, "  WRONG  IMAP imap.example.org:993: signing in to work failed; run `tuimex setup`");
    assert!(
        printed.contains("  WRONG  SMTP smtp.example.org:587: signing in to work failed")
            && printed.ends_with("2 things are wrong")
    );
    keyring.set("work", &Secret::new(PASSWORD.into())).unwrap();
    assert!(case.run().0);

    // A server whose certificate nothing vouches for, and one whose
    // certificate is no longer the pinned one.
    let unknown = FakeNet::self_signed(7);
    let printed = wrong(&Case { net: &unknown, ..case_of(&case) }, "invalid peer certificate: UnknownIssuer");
    assert!(
        printed.contains(
            "`tuimex setup`, at \"Accept a new certificate for work\", shows it and asks whether to trust it"
        ),
        "{printed}"
    );
    pin::save(&root.join("config"), "work", &[7; 32]).unwrap();
    let renewed = FakeNet::self_signed(9);
    let printed = wrong(&Case { net: &renewed, ..case_of(&case) }, "is not the one that was pinned");
    assert!(
        printed.contains(&pin::fingerprint(&[7; 32])) && printed.contains(&pin::fingerprint(&[9; 32])),
        "{printed}"
    );
    fs::remove_file(root.join("config/work.pem")).unwrap();

    // No keyring service: wrong for an account that keeps its password
    // there, a note for one whose password a command prints.
    let down = MemoryKeyring { down: Some("no such service"), ..MemoryKeyring::default() };
    let printed =
        wrong(&Case { keyring: &down, ..case_of(&case) }, "  WRONG  the keyring does not answer: no such service");
    assert!(printed.contains("give the account a command that prints its password instead"), "{printed}");
    config(&root, &format!("{WORK}password_cmd = [\"printf\", \"%s\", \"{PASSWORD}\"]\n"));
    let (fine, printed) = Case { keyring: &down, ..case_of(&case) }.run();
    assert!(fine && printed.contains("  note   the keyring does not answer: no such service"), "{printed}");
    assert!(printed.contains("  ok     its `password_cmd` printed a password"), "{printed}");
    assert!(printed.contains("  note   no `gpg` on the path"), "{printed}");
    assert!(!printed.contains(PASSWORD), "the config holds it here, and doctor still does not print it");

    // An editor that is not there; an account that cannot send; an auth
    // that is not one.
    config(&root, WORK);
    wrong(
        &Case { editor: Some("no-such-editor --wait"), ..case_of(&case) },
        "the editor `no-such-editor` (from the environment)",
    );
    config(&root, &WORK.replace("smtp = { host = \"smtp.example.org\", port = 587 }\n", ""));
    wrong(&case, "  WRONG  it has no `smtp` server in the config, so nothing can be sent from it");
    config(&root, &format!("{WORK}auth = \"oauth\"\n"));
    wrong(&case, "  WRONG  account work: auth is one of \"password\", \"command\", \"gmail-oauth\"");
}

/// The same case again, for a struct update that changes one field.
fn case_of<'a>(case: &Case<'a>) -> Case<'a> {
    Case { path: case.path.clone(), ..*case }
}

#[test]
fn the_agent_command_is_found_and_never_run_and_a_custom_one_is_not_vouched_for() {
    let root = scratch("doctor-agent");
    let keyring = MemoryKeyring::default();
    keyring.set("work", &Secret::new(PASSWORD.into())).unwrap();
    let net = FakeNet::trusted();
    // Programs that would leave a file behind if anything ran them.
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    for program in ["vi", "my-agent"] {
        fs::write(bin_dir.join(program), format!("#!/bin/sh\ntouch {}/ran\n", root.display())).unwrap();
        fs::set_permissions(bin_dir.join(program), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = bin_dir.to_string_lossy().into_owned();
    let case = Case { root: &root, keyring: &keyring, net: &net, google: None, path, editor: None };

    config(&root, &format!("{WORK}\n[agent]\nuse = \"claude\"\n"));
    let (fine, printed) = case.run();
    assert!(
        !fine && printed.contains("  WRONG  [agent] use = \"claude\", and no `claude` is on the path"),
        "{printed}"
    );

    config(&root, &format!("{WORK}\n[agent]\ncommand = [\"my-agent\", \"--answer\"]\n"));
    let (fine, printed) = case.run();
    assert!(fine, "{printed}");
    assert!(
        printed.contains(&format!("  ok     `:agent` runs {}/my-agent; it was not run", bin_dir.display())),
        "{printed}"
    );
    assert!(printed.contains("tuimex hands it text and cannot vouch for what it then does, opening a link included"));

    config(&root, &format!("{WORK}\n[agent]\ncommand = [\"not-installed\"]\n"));
    assert!(case.run().1.contains("  WRONG  [agent] command: `not-installed` is not a program on the path"));
    config(&root, &format!("{WORK}\n[agent]\nuse = \"claude\"\ncommand = [\"my-agent\"]\n"));
    assert!(case.run().1.contains("  WRONG  [agent] has both `use` and `command`; it takes one"));
    config(&root, WORK);
    assert!(case.run().1.contains("  note   no [agent] table in the config, so there is no `:agent`"));
    assert!(!root.join("ran").exists(), "doctor ran a program it was only to find");

    // A file that is there and may not be executed is not a program.
    fs::set_permissions(bin_dir.join("vi"), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(on_path("vi", OsStr::new(&case.path)), None);
    assert_eq!(on_path(&format!("{}/my-agent", bin_dir.display()), OsStr::new("")), Some(bin_dir.join("my-agent")));
}

#[test]
fn a_file_or_directory_others_can_read_is_named_under_each_of_the_four() {
    let root = scratch("doctor-modes");
    let keyring = MemoryKeyring::default();
    keyring.set("work", &Secret::new(PASSWORD.into())).unwrap();
    let net = FakeNet::trusted();
    config(&root, WORK);
    let path = bin(&root, &["vi"]);
    let case = Case { root: &root, keyring: &keyring, net: &net, google: None, path, editor: None };
    let all = dirs(&root);
    for dir in [&all.data, &all.cache, &all.state] {
        files::private_dir(&dir.join("inner")).unwrap();
        drop(files::create(&dir.join("inner/file")).unwrap());
    }
    let (fine, printed) = case.run();
    assert!(fine && printed.contains("  ok     11 files and directories under tuimex's four"), "{printed}");

    let open = |path: PathBuf, mode: u32| fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    open(all.config.join("config.toml"), 0o644);
    open(all.data.join("inner"), 0o755);
    open(all.cache.join("inner/file"), 0o640);
    open(all.state.clone(), 0o750);
    // A link is not followed out of the four directories.
    std::os::unix::fs::symlink("/etc", all.state.join("elsewhere")).unwrap();
    let (fine, printed) = case.run();
    assert!(!fine && printed.ends_with("4 things are wrong"), "{printed}");
    for line in [
        format!("  WRONG  {} is 644, not 600", all.config.join("config.toml").display()),
        format!("  WRONG  {} is 755, not 700", all.data.join("inner").display()),
        format!("  WRONG  {} is 640, not 600", all.cache.join("inner/file").display()),
        format!("  WRONG  {} is 750, not 700", all.state.display()),
        "  note   to mend: chmod -R u=rwX,go= ".to_string(),
    ] {
        assert!(printed.contains(&line), "wanted {line:?} in:\n{printed}");
    }
    assert!(!printed.contains("/etc/"), "{printed}");
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() { files_under(&path, out) } else { out.push(path) }
    }
}

/// ADR-010 over setup, Google's sign-in and doctor, plain and with
/// `--redact`: the app password, the
/// client secret, and every refresh and access token Google issued are
/// looked for in every file under the scratch root and in everything
/// shown, asked, printed or given as an error. The engine's half is
/// `tuimex-engine/tests/secret.rs`.
#[test]
fn a_secret_never_reaches_a_file_or_anything_setup_or_doctor_says() {
    let root = scratch("secret");
    let google = FakeGoogle::start(CLIENT_ID, CLIENT_SECRET).unwrap();
    let keyring = MemoryKeyring::default();
    let net = FakeNet { google: Some(google.clone()), ..FakeNet::self_signed(7) };
    let run = Run { root: &root, keyring: &keyring, net: &net, google: Some(&google), browser: true, claude: true };
    let said: std::cell::RefCell<Vec<String>> = std::cell::RefCell::default();
    let setup = |run: &Run, answers: &[&str]| {
        let (result, script) = run.go(answers);
        said.borrow_mut().extend(script.said);
        said.borrow_mut().extend(result.err().map(|error| format!("{error:#} {error:?}")));
    };

    // A password account with a pinned certificate, then two Gmail
    // accounts through the browser, the second with the kept client.
    let by_hand =
        ["ada@example.org", "5", "imap.example.org", "", "smtp.example.org", "587", "", "work", "1", PASSWORD];
    setup(&run, &[&by_hand[..], &["y", "y", "y", "y"]].concat());
    setup(&run, &["", "ada@gmail.com", "", "", "3", CLIENT_ID, CLIENT_SECRET, "y", "n", "n"]);
    setup(&run, &["", "bob@gmail.com", "", "", "3", "y", "n", "n"]);
    assert_eq!(keyring.names().len(), 5, "three sign-ins and the client's two entries");
    // Signing in again to each kind, afresh and with what is kept.
    setup(&run, &["2", PASSWORD]);
    setup(&run, &["4"]);
    setup(&run, &["7"]);

    // Each way of failing: a password and a client secret that are refused
    // (the token server repeats what it was sent in its refusal), a
    // certificate declined, a keyring that will not write or read.
    let refusing = FakeNet { password: "another-password", google: Some(google.clone()), ..FakeNet::self_signed(7) };
    setup(&Run { net: &refusing, ..run }, &["2", PASSWORD]);
    setup(
        &Run { net: &refusing, ..run },
        &["", "eve@example.org", "5", "i.example.org", "", "s.example.org", "", "", "", "1", PASSWORD, "y"],
    );
    let renewed = FakeNet { google: Some(google.clone()), ..FakeNet::self_signed(9) };
    setup(&Run { net: &renewed, ..run }, &["2", PASSWORD, "n"]);
    let down = MemoryKeyring { down: Some("the keyring is locked"), ..MemoryKeyring::default() };
    setup(
        &Run { keyring: &down, ..run },
        &["", "eve@example.org", "5", "i.example.org", "", "s.example.org", "", "", "", "1", PASSWORD, "y", "n", "n"],
    );
    setup(&Run { keyring: &down, ..run }, &["5"]);
    let strangers = MemoryKeyring::default();
    let elsewhere = scratch("secret-elsewhere");
    let wrong_client = ["ada@gmail.com", "", "", "3", CLIENT_ID, "GOCSPX-synthetic-wrong-secret-99"];
    setup(&Run { root: &elsewhere, keyring: &strangers, ..run }, &wrong_client);
    assert!(strangers.names().is_empty() && !elsewhere.join("config").exists());

    // A rename, whose lines say what is moved and never what is kept
    // there; the account is `work` again afterwards.
    setup(&run, &["8", "1", "office", "y"]);
    assert!(keyring.get("office").as_deref() == Some(PASSWORD) && keyring.get("work").is_none());
    setup(&run, &["8", "1", "work", "y"]);
    assert!(said.borrow().iter().any(|line| line == "office is now work."));

    // Doctor over all three accounts, working; then with every sign-in
    // refused; then with Google having revoked both.
    let secrets = |google: &FakeGoogle| {
        let mut all = google.tokens();
        all.extend([PASSWORD.to_string(), CLIENT_SECRET.to_string(), "GOCSPX-synthetic-wrong-secret-99".to_string()]);
        all
    };
    let mut secrets_seen = secrets(&google);
    let path = bin(&root, &["vi", "claude"]);
    let case = Case { root: &root, keyring: &keyring, net: &net, google: Some(&google), path, editor: None };
    let (fine, printed) = case.run();
    assert!(fine, "{printed}");
    said.borrow_mut().push(printed);
    said.borrow_mut().push(case.redacted().1);
    for broken in [Case { net: &refusing, ..case_of(&case) }, Case { keyring: &down, ..case_of(&case) }] {
        said.borrow_mut().extend([broken.run().1, broken.redacted().1]);
    }
    secrets_seen.extend(secrets(&google));
    google.revoke();
    said.borrow_mut().push(case.redacted().1);
    let (fine, printed) = case.run();
    assert!(
        !fine && printed.contains("Google no longer accepts the sign-in kept for gmail; run `tuimex setup`"),
        "{printed}"
    );
    said.borrow_mut().push(printed);
    setup(&run, &["5"]);

    let said = said.into_inner();
    // What was looked for is really there to find: in the keyring, where
    // it belongs, and nowhere in what follows.
    secrets_seen.sort();
    secrets_seen.dedup();
    assert!(secrets_seen.len() >= 9, "{} secrets were in play", secrets_seen.len());
    let kept: Vec<String> = keyring.names().iter().filter_map(|name| keyring.get(name)).collect();
    assert!(kept.iter().any(|k| k == PASSWORD) && kept.iter().any(|k| k == CLIENT_SECRET));
    assert!(kept.iter().filter(|k| k.starts_with("1//synthetic-refresh-")).count() == 2, "{kept:?}");

    let mut written = Vec::new();
    files_under(&root, &mut written);
    files_under(&elsewhere.parent().unwrap().join(elsewhere.file_name().unwrap()), &mut written);
    let names: Vec<String> =
        written.iter().map(|path| path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    for expected in ["config.toml", "work.pem", "gmail.pem", "gmail-2.pem"] {
        assert!(names.iter().any(|name| name == expected), "{expected} was written: {names:?}");
    }
    assert!(said.len() > 150, "too little was said for setup and doctor to have been exercised: {}", said.len());
    for secret in &secrets_seen {
        for path in &written {
            let content = fs::read(path).unwrap();
            let held = content.windows(secret.len()).any(|window| window == secret.as_bytes());
            assert!(!held && !path.to_string_lossy().contains(secret.as_str()), "{} holds a secret", path.display());
        }
        let leaks: Vec<&String> = said.iter().filter(|line| line.contains(secret.as_str())).collect();
        assert_eq!(leaks, Vec::<&String>::new(), "things said that hold a secret");
    }
    // The config names the accounts and how each signs in, and that is all.
    let config = fs::read_to_string(root.join("config/config.toml")).unwrap();
    assert_eq!(config.matches("auth = \"gmail-oauth\"").count(), 2);
    assert!(oauth::client(&keyring).unwrap().is_some());
}

#[test]
fn a_rename_that_was_not_finished_is_named_with_what_finishes_it() {
    let root = scratch("doctor-rename");
    let keyring = MemoryKeyring::default();
    let (path, net) = (bin(&root, &["vi"]), FakeNet::trusted());
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join("data/renaming"), "work\noffice\n").unwrap();
    let case = Case { root: &root, keyring: &keyring, net: &net, google: None, path, editor: None };
    let (fine, printed) = case.run();
    let said = "  WRONG  the rename of account work to office was begun and not finished, so its mail and its config \
                do not agree; run `tuimex setup`, which finishes it";
    assert!(!fine && printed.contains(said), "{printed}");
    let hidden = redacted_names_nothing(&case, &printed);
    assert!(hidden.contains("  WRONG  the rename of account account-1 to account-2 was begun"), "{hidden}");
}

use std::ffi::OsStr;

use super::{Keyring, Secret, SecretError, SystemKeyring, resolve, turned_off};

/// A keyring that exists only in the test: the user's is never opened.
struct FakeKeyring(Result<Option<&'static str>, &'static str>);

impl Keyring for FakeKeyring {
    fn password(&self, _account: &str) -> Result<Option<Secret>, String> {
        self.0.map(|found| found.map(|p| Secret::new(p.to_string()))).map_err(ToString::to_string)
    }

    fn set(&self, _entry: &str, _secret: &Secret) -> Result<(), String> {
        Err("this keyring is read only".to_string())
    }
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(ToString::to_string).collect()
}

#[test]
fn a_secret_prints_as_redacted_however_it_is_formatted() {
    let secret = Secret::new("hunter2-synthetic".to_string());
    assert_eq!(format!("{secret:?}"), "[redacted]");
    assert_eq!(format!("{secret:#?}"), "[redacted]");
    assert_eq!(format!("{:?}", Some(&secret)), "Some([redacted])");
    assert_eq!(secret.expose(), "hunter2-synthetic");
}

#[test]
fn the_keyring_is_asked_when_there_is_no_command() {
    let found = resolve("work", None, &FakeKeyring(Ok(Some("from-keyring")))).unwrap();
    assert_eq!(found.expose(), "from-keyring");
}

#[test]
fn the_command_wins_over_the_keyring_and_its_first_line_is_the_password() {
    let cmd = argv(&["printf", "from-command\\nsecond line\\n"]);
    let found = resolve("work", Some(&cmd), &FakeKeyring(Ok(Some("from-keyring")))).unwrap();
    assert_eq!(found.expose(), "from-command");
}

#[test]
fn the_commands_words_are_arguments_and_never_shell() {
    let cmd = argv(&["printf", "%s", "a; echo b $(echo c)"]);
    let found = resolve("work", Some(&cmd), &FakeKeyring(Ok(None))).unwrap();
    assert_eq!(found.expose(), "a; echo b $(echo c)");
}

#[test]
fn a_failing_command_is_an_error_that_does_not_fall_back_or_repeat_its_output() {
    let cmd = argv(&["sh", "-c", "echo printed-before-failing; echo also-on-stderr >&2; exit 3"]);
    let error = resolve("work", Some(&cmd), &FakeKeyring(Ok(Some("from-keyring")))).unwrap_err();
    assert!(matches!(error, SecretError::Failed { .. }), "{error:?}");
    let said = format!("{error} {error:?}");
    assert!(said.contains("work") && said.contains("sh"), "{said}");
    assert!(!said.contains("printed-before-failing") && !said.contains("also-on-stderr"), "{said}");
}

#[test]
fn a_command_that_cannot_run_or_prints_nothing_is_an_error_naming_the_account() {
    let keyring = FakeKeyring(Ok(None));
    let missing = resolve("work", Some(&argv(&["tuimex-no-such-program"])), &keyring).unwrap_err();
    assert!(matches!(missing, SecretError::Spawn { .. }), "{missing:?}");
    assert!(matches!(resolve("work", Some(&argv(&["true"])), &keyring), Err(SecretError::Empty { .. })));
    assert!(matches!(resolve("work", Some(&[]), &keyring), Err(SecretError::NoProgram { .. })));
}

#[test]
fn an_empty_or_unreadable_keyring_is_an_error_naming_the_account() {
    let missing = resolve("work", None, &FakeKeyring(Ok(None))).unwrap_err();
    assert!(matches!(missing, SecretError::Missing { .. }));
    assert!(missing.to_string().contains("work"));
    let locked = resolve("work", None, &FakeKeyring(Err("the keyring is locked"))).unwrap_err();
    assert_eq!(locked.to_string(), "the keyring could not be read for work: the keyring is locked");
}

/// ADR-053. A run that did not set the variable is refused nothing, and a
/// value that is not `none` is refused by name: nothing is passed over in
/// either direction.
#[test]
fn only_an_unset_switch_leaves_the_system_keyring_on_and_every_value_says_why_it_is_off() {
    assert_eq!(turned_off(None), None, "a run that says nothing asks the system's keyring");
    let off = turned_off(Some(OsStr::new("none"))).unwrap();
    assert_eq!(off, "no keyring service could be reached (TUIMEX_KEYRING is none, so none was tried)");
    for other in ["", "None", "NONE", "0", "off", "system", "memory", " none"] {
        let said = turned_off(Some(OsStr::new(other))).unwrap_or_else(|| panic!("{other:?} was passed over"));
        assert!(said.contains("TUIMEX_KEYRING") && said.contains(&format!("{other:?}")), "{said}");
        assert!(said.ends_with("no keyring was tried"), "{said}");
    }
}

/// Every call the trait has stops at the switch, before an entry is made:
/// the refusal is the switch's own words, which no keyring service says.
/// `make test` and CI set the variable for every test (ADR-053); run
/// without it, this test asks nothing and says so.
#[test]
fn a_keyring_turned_off_is_asked_nothing_by_any_call() {
    if std::env::var_os(super::SWITCH).is_none_or(|v| v != "none") {
        println!("keyring-off: NOT RUN, TUIMEX_KEYRING is not none");
        return;
    }
    let why = "no keyring service could be reached (TUIMEX_KEYRING is none, so none was tried)";
    assert_eq!(SystemKeyring.password("work").map(|found| found.is_some()), Err(why.to_string()));
    assert_eq!(SystemKeyring.set("work", &Secret::new("synthetic".to_string())), Err(why.to_string()));
    assert_eq!(SystemKeyring.remove("work"), Err(why.to_string()));
    println!("keyring-off: ran");
}

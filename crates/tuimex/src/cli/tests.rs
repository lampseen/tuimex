use std::path::PathBuf;

use clap::Parser;

use super::{Args, Colors, Command, Glyphs, Palette, Run};

#[test]
fn every_flag_parses_to_its_field() {
    let args = Args::try_parse_from([
        "tuimex",
        "--config",
        "/etc/elsewhere.toml",
        "--fixtures",
        "--glyphs",
        "ascii",
        "--colors",
        "16",
        "--section",
        "Unread",
    ])
    .unwrap();
    assert_eq!(args.config, Some(PathBuf::from("/etc/elsewhere.toml")));
    assert!(args.fixtures);
    assert_eq!(args.glyphs, Some(Glyphs::Ascii));
    assert_eq!(args.colors, Some(Colors::C16));
    assert_eq!(args.section.as_deref(), Some("Unread"));
}

#[test]
fn nothing_is_assumed_when_no_flag_is_given() {
    let args = Args::try_parse_from(["tuimex"]).unwrap();
    assert!(!args.fixtures);
    assert_eq!((args.config, args.glyphs, args.colors, args.section), (None, None, None, None));
}

#[test]
fn a_tier_outside_the_list_is_refused() {
    assert!(Args::try_parse_from(["tuimex", "--glyphs", "emoji"]).is_err());
    assert!(Args::try_parse_from(["tuimex", "--colors", "8"]).is_err());
}

#[test]
fn a_subcommand_parses_to_its_arguments_and_the_config_may_follow_it() {
    let args = Args::try_parse_from(["tuimex", "list", "--folder", "INBOX", "--limit", "3", "--config", "/c.toml"]);
    let args = args.unwrap();
    assert_eq!(args.config, Some(PathBuf::from("/c.toml")));
    let expected = Command::List { account: None, folder: Some("INBOX".into()), query: None, limit: Some(3) };
    assert_eq!(args.command, Some(Run::Agent(expected)));
}

#[test]
fn draft_takes_recipients_repeated_or_separated_by_commas_and_needs_one() {
    let base = ["tuimex", "draft", "--account", "work", "--subject", "Hello"];
    let parse = |more: &[&str]| Args::try_parse_from(base.iter().chain(more)).map(|args| args.command);
    let Ok(Some(Run::Agent(Command::Draft { to, cc, .. }))) =
        parse(&["--to", "a@example.com,b@example.com", "--to", "c@example.com", "--cc", "d@example.com"])
    else {
        panic!("a draft with recipients parses");
    };
    assert_eq!(to, ["a@example.com", "b@example.com", "c@example.com"]);
    assert_eq!(cc, ["d@example.com"]);
    assert!(parse(&[]).is_err(), "a draft to nobody is refused");
    assert!(parse(&["--to", "a@example.com", "--attach", "/etc/hostname"]).is_err());
}

#[test]
fn palette_import_takes_the_theme_file() {
    let args = Args::try_parse_from(["tuimex", "palette", "import", "/themes/mine"]).unwrap();
    assert_eq!(args.command, Some(Run::Palette { action: Palette::Import { file: PathBuf::from("/themes/mine") } }));
    assert!(Args::try_parse_from(["tuimex", "palette"]).is_err());
    assert!(Args::try_parse_from(["tuimex", "palette", "import"]).is_err());
}

/// A command is written to the audit log as it serialises.
#[test]
fn a_command_serialises_to_its_name_and_arguments() {
    let show = Command::Show { id: "k1".into(), account: None };
    assert_eq!(
        serde_json::to_string(&show).unwrap(),
        r#"{"subcommand":"show","arguments":{"id":"k1","account":null}}"#
    );
    assert_eq!(serde_json::to_string(&Command::Accounts).unwrap(), r#"{"subcommand":"accounts"}"#);
}

#[test]
fn sync_takes_an_account_or_none_and_is_not_an_agent_command() {
    let parse = |args: &[&str]| Args::try_parse_from(args).unwrap().command;
    assert_eq!(parse(&["tuimex", "sync"]), Some(Run::Sync { account: None }));
    assert_eq!(parse(&["tuimex", "sync", "--account", "work"]), Some(Run::Sync { account: Some("work".into()) }));
}

#[test]
fn setup_takes_the_config_and_nothing_else_and_doctor_takes_redact_too() {
    let parse = |args: &[&str]| Args::try_parse_from(args).map(|args| (args.command, args.config));
    assert_eq!(parse(&["tuimex", "setup"]).unwrap(), (Some(Run::Setup), None));
    let elsewhere = parse(&["tuimex", "doctor", "--config", "/c.toml"]).unwrap();
    assert_eq!(elsewhere, (Some(Run::Doctor { redact: false }), Some(PathBuf::from("/c.toml"))));
    assert_eq!(parse(&["tuimex", "doctor", "--redact"]).unwrap(), (Some(Run::Doctor { redact: true }), None));
    assert!(parse(&["tuimex", "doctor", "--redact=yes"]).is_err() && parse(&["tuimex", "setup", "--redact"]).is_err());
    assert!(parse(&["tuimex", "setup", "--password", "x"]).is_err(), "no flag takes a secret");
}

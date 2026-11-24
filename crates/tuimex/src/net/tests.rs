use super::{Auth, auth, servers};
use crate::config::Config;

fn account(more: &str) -> Config {
    Config::parse(&format!("[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n{more}")).unwrap()
}

#[test]
fn auth_is_what_the_config_says_or_what_a_password_cmd_implies() {
    let kind = |more: &str| auth(&account(more).accounts[0]).map_err(|e| e.to_string());
    assert_eq!(kind("").unwrap(), Auth::Password);
    assert_eq!(kind("auth = \"password\"\n").unwrap(), Auth::Password);
    assert_eq!(kind("password_cmd = [\"pass\"]\n").unwrap(), Auth::Command);
    assert_eq!(kind("auth = \"command\"\npassword_cmd = [\"pass\"]\n").unwrap(), Auth::Command);
    assert_eq!(kind("auth = \"gmail-oauth\"\n").unwrap(), Auth::Gmail);
    assert_eq!(kind("auth = \"command\"\n").unwrap_err(), "account work: auth = \"command\" needs a `password_cmd`");
    assert_eq!(
        kind("auth = \"gmail-oauth\"\npassword_cmd = [\"pass\"]\n").unwrap_err(),
        "account work: `password_cmd` is set, and `auth` says the password comes from elsewhere"
    );
    // The value is not repeated: it may be a password typed under the wrong key.
    assert_eq!(
        kind("auth = \"hunter2\"\n").unwrap_err(),
        "account work: auth is one of \"password\", \"command\", \"gmail-oauth\""
    );
}

#[test]
fn a_server_starts_tls_at_once_on_its_own_port_and_by_starttls_elsewhere() {
    let both = "imap = { host = \"i.example.org\", port = 993 }\nsmtp = { host = \"s.example.org\", port = 587, user = \"ada\" }\n";
    let (imap, smtp) = servers(&account(both).accounts[0]).unwrap();
    let smtp = smtp.unwrap();
    assert_eq!((imap.host.as_str(), imap.starttls, imap.user.as_str()), ("i.example.org", false, "ada@example.org"));
    assert_eq!((smtp.port, smtp.starttls, smtp.user.as_str()), (587, true, "ada"));
    let bridge = "imap = { host = \"127.0.0.1\", port = 1143 }\nsmtp = { host = \"127.0.0.1\", port = 465 }\n";
    let (imap, smtp) = servers(&account(bridge).accounts[0]).unwrap();
    assert!(imap.starttls && !smtp.unwrap().starttls);
    let forced = "imap = { host = \"i.example.org\", port = 143, starttls = false }\n";
    assert!(!servers(&account(forced).accounts[0]).unwrap().0.starttls);
    assert_eq!(servers(&account("").accounts[0]).unwrap_err().to_string(), "account work has no `imap` server");
}

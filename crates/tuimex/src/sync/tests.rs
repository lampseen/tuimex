use std::path::Path;
use std::time::Duration;

use tuimex_core::Role;
use tuimex_engine::sync::Policy;

use super::{account, policy, send_delay};
use crate::config::Config;

fn config(text: &str) -> Config {
    Config::parse(text).unwrap()
}

const DELAY: Duration = Duration::from_secs(10);
const WORK: &str = "[[account]]\nname = \"work\"\naddress = \"ada@example.org\"\n";

#[test]
fn the_sync_table_becomes_numbers_and_what_is_left_out_keeps_the_default() {
    assert_eq!(policy(None).unwrap(), Policy::default());
    let c = config("[sync]\nbodies = \"7d\"\nmax_body = \"2MB\"\nfolders = [\"Projects\"]\n");
    let want = Policy { bodies: 7 * 86_400, max_body: 2_000_000, folders: vec!["Projects".to_string()] };
    assert_eq!(policy(c.sync.as_ref()).unwrap(), want);
    assert_eq!(policy(config("[sync]\nmax_body = \"500kb\"\n").sync.as_ref()).unwrap().max_body, 500_000);
}

#[test]
fn an_amount_that_is_not_a_number_and_a_known_unit_is_an_error_naming_the_key() {
    for (text, key) in
        [("bodies = \"30\"", "bodies"), ("bodies = \"soon\"", "bodies"), ("max_body = \"10 furlongs\"", "max_body")]
    {
        let error = policy(config(&format!("[sync]\n{text}\n")).sync.as_ref()).unwrap_err().to_string();
        assert!(error.contains(&format!("sync.{key}")), "{error}");
    }
}

#[test]
fn tls_is_implicit_on_993_and_starttls_elsewhere_unless_the_config_says() {
    let starttls = |imap: &str| {
        let c = config(&format!("{WORK}imap = {imap}\n"));
        account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere"))
            .map(|(_, reach)| reach.server.starttls)
    };
    assert!(!starttls("{ host = \"imap.example.org\", port = 993 }").unwrap());
    assert!(starttls("{ host = \"imap.example.org\", port = 143 }").unwrap());
    assert!(!starttls("{ host = \"imap.example.org\", port = 1993, starttls = false }").unwrap());
    assert!(starttls("{ host = \"imap.example.org\", port = 993, starttls = true }").unwrap());
}

#[test]
fn the_account_signs_in_as_its_address_unless_a_user_is_named_and_carries_its_roles() {
    let c = config(&format!(
        "{WORK}imap = {{ host = \"imap.example.org\", port = 993 }}\npassword_cmd = [\"pass\", \"show\", \"work\"]\nfolders = {{ archive = \"Old\" }}\n"
    ));
    let (account_, reach) = account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap();
    assert_eq!((reach.server.user.as_str(), reach.server.host.as_str()), ("ada@example.org", "imap.example.org"));
    assert_eq!(reach.password_cmd.as_deref(), Some(&["pass".to_string(), "show".to_string(), "work".to_string()][..]));
    assert_eq!(account_.roles, [(Role::Archive, "Old".to_string())]);
    let c = config(&format!("{WORK}imap = {{ host = \"imap.example.org\", port = 993, user = \"ada\" }}\n"));
    assert_eq!(account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap().1.server.user, "ada");
}

#[test]
fn an_account_without_a_server_or_with_an_unknown_role_is_an_error_naming_it() {
    let no_server = account(&config(WORK).accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).err().unwrap();
    assert!(no_server.to_string().contains("account work has no `imap` server"));
    let c =
        config(&format!("{WORK}imap = {{ host = \"h.example.org\", port = 993 }}\nfolders = {{ inbox = \"X\" }}\n"));
    let bad_role = account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).err().unwrap();
    assert!(bad_role.to_string().contains("folders.inbox"));
}

#[test]
fn the_send_delay_is_ten_seconds_unless_the_config_says() {
    assert_eq!(send_delay(None).unwrap(), DELAY);
    assert_eq!(send_delay(config("[send]\n").send.as_ref()).unwrap(), DELAY);
    assert_eq!(send_delay(config("[send]\ndelay = \"0s\"\n").send.as_ref()).unwrap(), Duration::ZERO);
    assert_eq!(send_delay(config("[send]\ndelay = \"2m\"\n").send.as_ref()).unwrap(), Duration::from_secs(120));
    let error = send_delay(config("[send]\ndelay = \"soon\"\n").send.as_ref()).unwrap_err().to_string();
    assert!(error.contains("send.delay"), "{error}");
}

#[test]
fn the_account_sends_from_its_address_and_its_identities_through_its_smtp_server() {
    let c = config(&format!(
        "{WORK}identities = [\"Ada <ada@example.org>\", \"lovelace@example.net\"]\n\
         imap = {{ host = \"imap.example.org\", port = 993 }}\nsmtp = {{ host = \"smtp.example.org\", port = 465 }}\n"
    ));
    let (account_, reach) = account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap();
    assert_eq!(account_.identities, ["ada@example.org", "ada@example.org", "lovelace@example.net"]);
    assert_eq!(account_.send_delay, DELAY);
    let smtp = reach.smtp.unwrap();
    assert_eq!((smtp.host.as_str(), smtp.starttls, smtp.user.as_str()), ("smtp.example.org", false, "ada@example.org"));
    let c = config(&format!(
        "{WORK}imap = {{ host = \"imap.example.org\", port = 993 }}\nsmtp = {{ host = \"smtp.example.org\", port = 587, user = \"ada\" }}\n"
    ));
    let smtp = account(&c.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap().1.smtp.unwrap();
    assert_eq!((smtp.starttls, smtp.user.as_str()), (true, "ada"));
    let none = config(&format!("{WORK}imap = {{ host = \"imap.example.org\", port = 993 }}\n"));
    assert!(account(&none.accounts[0], &Policy::default(), DELAY, Path::new("/nowhere")).unwrap().1.smtp.is_none());
}

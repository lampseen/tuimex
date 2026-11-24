use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tuimex_fixtures::FakeGoogle;

use super::{CLIENT_ID, CLIENT_SECRET, Client, Endpoints, OauthError, SignIn, Tokens, challenge, refresh_entry};
use crate::{Keyring, Secret};

const ID: &str = "synthetic-client.apps.example.com";
const CLIENT: &str = "GOCSPX-synthetic-client-secret";
const WAIT: Duration = Duration::from_secs(20);

/// A keyring that exists only in the test: the user's is never opened.
#[derive(Default)]
struct MemoryKeyring(Mutex<HashMap<String, String>>);

impl Keyring for MemoryKeyring {
    fn password(&self, entry: &str) -> Result<Option<Secret>, String> {
        Ok(self.0.lock().unwrap().get(entry).map(|kept| Secret::new(kept.clone())))
    }

    fn set(&self, entry: &str, secret: &Secret) -> Result<(), String> {
        self.0.lock().unwrap().insert(entry.to_string(), secret.expose().to_string());
        Ok(())
    }
}

fn google() -> (FakeGoogle, Endpoints, Client) {
    let google = FakeGoogle::start(ID, CLIENT).unwrap();
    let endpoints = Endpoints { authorize: google.authorize_url(), token: google.token_url() };
    (google, endpoints, Client { id: ID.to_string(), secret: Secret::new(CLIENT.to_string()) })
}

/// A browser following the redirect Google answered with: one GET to the
/// loopback address, and what the page said.
fn visit(address: &str) -> String {
    let rest = address.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut stream = TcpStream::connect(host).unwrap();
    stream.write_all(format!("GET /{path} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes()).unwrap();
    let mut page = String::new();
    stream.read_to_string(&mut page).unwrap();
    page
}

/// Signs in through the fake and leaves the keyring as setup would.
fn signed_in(google: &FakeGoogle, endpoints: &Endpoints, client: &Client, keyring: &MemoryKeyring) {
    let sign_in = SignIn::begin(endpoints, &client.id, "ada@example.com").unwrap();
    let code = sign_in.pasted(&google.authorize(sign_in.url()).unwrap()).unwrap();
    let (refresh, _) = sign_in.finish(endpoints, client, &code).unwrap();
    keyring.set(CLIENT_ID, &Secret::new(client.id.clone())).unwrap();
    keyring.set(CLIENT_SECRET, &client.secret).unwrap();
    keyring.set(&refresh_entry("gmail"), &refresh).unwrap();
}

#[test]
fn the_challenge_is_rfc_7636_s_s256() {
    assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
}

#[test]
fn a_browser_sign_in_returns_to_a_loopback_port_and_the_server_checks_the_verifier() {
    let (google, endpoints, client) = google();
    let sign_in = SignIn::begin(&endpoints, &client.id, "ada@example.com").unwrap();
    assert!(sign_in.url().contains("login_hint=ada%40example.com"), "{}", sign_in.url());
    let back = google.authorize(sign_in.url()).unwrap();
    assert!(back.starts_with("http://127.0.0.1:"), "{back}");
    let browser = thread::spawn(move || {
        // A browser asks for an icon as well, which is not the answer.
        let icon = visit(&format!("{}favicon.ico", back.split_once('?').unwrap().0));
        (icon, visit(&back))
    });
    let code = sign_in.wait(WAIT).unwrap();
    let (icon, page) = browser.join().unwrap();
    assert!(icon.starts_with("HTTP/1.1 404") && page.starts_with("HTTP/1.1 200"), "{icon}\n{page}");
    assert!(page.contains("This window can be closed"), "{page}");

    let (refresh, access) = sign_in.finish(&endpoints, &client, &code).unwrap();
    assert!(google.accepts(access.token.expose()) && google.tokens().contains(&refresh.expose().to_string()));
    let asked = google.asked().remove(0);
    let sent = |key: &str| asked.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap();
    assert_eq!(sent("grant_type"), "authorization_code");
    assert!(sent("redirect_uri").starts_with("http://127.0.0.1:") && sent("code_verifier").len() >= 43);
}

#[test]
fn a_code_is_not_exchanged_by_a_sign_in_that_did_not_ask_for_it() {
    let (google, endpoints, client) = google();
    let (first, second) = (
        SignIn::begin(&endpoints, &client.id, "ada@example.com").unwrap(),
        SignIn::begin(&endpoints, &client.id, "ada@example.com").unwrap(),
    );
    let back = google.authorize(first.url()).unwrap();
    // Another sign-in's address has another state, and is refused here.
    assert!(matches!(second.pasted(&back), Err(OauthError::State)));
    // Given the bare code it has only its own verifier, and is refused there.
    let code = back.split("code=").nth(1).unwrap().split('&').next().unwrap();
    let stolen = second.pasted(code).unwrap();
    let refused = second.finish(&endpoints, &client, &stolen).unwrap_err();
    assert!(matches!(refused, OauthError::Refused("invalid_grant")), "{refused:?}");
    assert!(google.tokens().is_empty());
}

#[test]
fn a_return_with_the_wrong_state_or_a_refusal_ends_the_wait_without_a_code() {
    let (google, endpoints, client) = google();
    let sign_in = SignIn::begin(&endpoints, &client.id, "ada@example.com").unwrap();
    let back = google.authorize(sign_in.url()).unwrap();
    let state = back.split("state=").nth(1).unwrap().split('&').next().unwrap().to_string();
    let forged = back.replace(&state, "a-state-someone-else-made-up");
    let browser = thread::spawn(move || visit(&forged));
    assert!(matches!(sign_in.wait(WAIT), Err(OauthError::State)));
    assert!(browser.join().unwrap().starts_with("HTTP/1.1 400"));

    let declined = format!("{}?error=access_denied&state={state}", back.split_once('?').unwrap().0);
    let said = sign_in.pasted(&declined).err().unwrap().to_string();
    assert_eq!(said, "the sign-in was not completed: the browser came back with `access_denied`");
    let odd = format!("{}?error=<script>&state={state}", back.split_once('?').unwrap().0);
    assert!(!sign_in.pasted(&odd).err().unwrap().to_string().contains("script"));
    assert!(matches!(sign_in.pasted("  "), Err(OauthError::NoCode)));
    assert!(matches!(sign_in.pasted("https://example.com/"), Err(OauthError::NoCode)));
    assert!(matches!(sign_in.wait(Duration::from_millis(120)), Err(OauthError::TimedOut(_))));
}

#[test]
fn an_access_token_is_kept_in_memory_and_asked_for_again_when_it_is_about_to_end() {
    let (google, endpoints, client) = google();
    let keyring = MemoryKeyring::default();
    signed_in(&google, &endpoints, &client, &keyring);
    let tokens = Tokens::new(endpoints);
    let refreshes =
        || google.asked().iter().filter(|form| form.contains(&("grant_type".into(), "refresh_token".into()))).count();

    let first = tokens.access("gmail", &keyring).unwrap();
    let again = tokens.access("gmail", &keyring).unwrap();
    assert_eq!((first.expose(), refreshes()), (again.expose(), 1), "an hour's token is asked for once");
    assert!(google.accepts(first.expose()));

    // One that ends within the minute is not handed out a second time.
    google.lifetime(30);
    tokens.forget();
    let short = tokens.access("gmail", &keyring).unwrap();
    let next = tokens.access("gmail", &keyring).unwrap();
    assert_ne!(short.expose(), next.expose());
    assert_eq!(refreshes(), 3);
    // Nothing but the three entries setup made is in the keyring.
    assert_eq!(keyring.0.lock().unwrap().len(), 3);
}

#[test]
fn a_revoked_sign_in_says_to_run_setup_and_repeats_nothing_the_server_said() {
    let (google, endpoints, client) = google();
    let keyring = MemoryKeyring::default();
    signed_in(&google, &endpoints, &client, &keyring);
    let kept = keyring.password(&refresh_entry("gmail")).unwrap().unwrap();
    google.revoke();
    let tokens = Tokens::new(endpoints.clone());
    let error = tokens.access("gmail", &keyring).unwrap_err();
    assert!(matches!(error, OauthError::Revoked { .. }), "{error:?}");
    let said = format!("{error} {error:?} ");
    let said = said + &crate::Error::from(error).advice("gmail");
    assert!(said.contains("Google no longer accepts the sign-in kept for gmail; run `tuimex setup`"), "{said}");
    // The fake's refusal describes itself with everything it was sent.
    assert!(!said.contains(kept.expose()) && !said.contains(CLIENT), "{said}");

    // A client secret Google does not know is named as that, and no more.
    let wrong = Client { id: ID.to_string(), secret: Secret::new("GOCSPX-not-the-secret".to_string()) };
    let refused = super::refresh(&endpoints, &wrong, &kept).unwrap_err();
    assert_eq!(refused.to_string(), "the token server refused the request: invalid_client");
    assert!(!format!("{refused:?}").contains("GOCSPX"));
}

#[test]
fn a_missing_keyring_entry_and_a_plain_http_endpoint_are_errors_before_anything_is_sent() {
    let (google, endpoints, client) = google();
    let keyring = MemoryKeyring::default();
    let tokens = Tokens::new(endpoints);
    let none = tokens.access("gmail", &keyring).unwrap_err();
    assert_eq!(none.to_string(), "the Google client is not in the keyring; run `tuimex setup`");
    keyring.set(CLIENT_ID, &Secret::new(ID.to_string())).unwrap();
    keyring.set(CLIENT_SECRET, &client.secret).unwrap();
    let none = tokens.access("gmail", &keyring).unwrap_err();
    assert_eq!(none.to_string(), "the Gmail sign-in for gmail is not in the keyring; run `tuimex setup`");

    let elsewhere = Endpoints { authorize: google.authorize_url(), token: "http://example.com/token".to_string() };
    let refused = super::refresh(&elsewhere, &client, &Secret::new("1//synthetic".to_string())).unwrap_err();
    assert!(matches!(refused, OauthError::Unreachable(_)), "{refused:?}");
    assert!(google.asked().is_empty());
    assert_eq!(Endpoints::google().token, "https://oauth2.googleapis.com/token");
}

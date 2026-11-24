//! Gmail's sign-in (ADR-019): a browser visit that ends on a loopback
//! address of this machine, with PKCE, exchanged for a refresh token that
//! is the keyring's and an access token that is never anywhere but memory
//! (ADR-010). Nothing here prints, and no error carries what a token
//! server answered beyond the name of its refusal.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::{Keyring, Secret};

/// Full access to the mailbox over IMAP and SMTP, which is the only scope
/// Google offers for either.
pub const SCOPE: &str = "https://mail.google.com/";

/// The keyring entries of the one client every Gmail account shares. The
/// slash keeps them apart from any account's entry: `tuimex setup` gives
/// no account a name with one in it.
pub const CLIENT_ID: &str = "oauth/google/client-id";
pub const CLIENT_SECRET: &str = "oauth/google/client-secret";

/// The keyring entry of `account`'s refresh token.
pub fn refresh_entry(account: &str) -> String {
    format!("{account}/refresh-token")
}

/// How long a server may take over a token request.
const PATIENCE: Duration = Duration::from_secs(30);
/// An access token this close to its end is replaced before it is used.
const MARGIN: Duration = Duration::from_secs(60);
/// The refusals RFC 6749 names. One a server words differently is not
/// repeated: the text is the server's, and may quote what it was sent.
const NAMED: [&str; 7] = [
    "invalid_request",
    "invalid_client",
    "invalid_grant",
    "unauthorized_client",
    "unsupported_grant_type",
    "invalid_scope",
    "access_denied",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub authorize: String,
    pub token: String,
}

impl Endpoints {
    pub fn google() -> Endpoints {
        Endpoints {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token: "https://oauth2.googleapis.com/token".to_string(),
        }
    }
}

/// The client the user registered in Google's console.
#[derive(Debug, Clone)]
pub struct Client {
    pub id: String,
    pub secret: Secret,
}

#[derive(Debug, thiserror::Error)]
pub enum OauthError {
    #[error("this system gave no random bytes to sign in with")]
    Random,
    #[error("no loopback address could be listened on for the browser to return to: {0}")]
    Listen(std::io::Error),
    #[error("the browser did not come back within {}s", .0.as_secs())]
    TimedOut(Duration),
    #[error("the sign-in was not completed: the browser came back with `{0}`")]
    Declined(String),
    #[error("the address that came back was not for this sign-in (its state differs); nothing was exchanged")]
    State,
    #[error("that is neither the address the browser ended on nor a code")]
    NoCode,
    #[error("Google no longer accepts the sign-in kept for {account}; run `tuimex setup` to sign in again")]
    Revoked { account: String },
    #[error("the token server refused the request: {0}")]
    Refused(&'static str),
    #[error("the token server could not be asked: {0}")]
    Unreachable(String),
    #[error("the token server's answer had no {0}")]
    Incomplete(&'static str),
    #[error("{0} is not in the keyring; run `tuimex setup`")]
    NotKept(String),
    #[error("the keyring could not be read: {0}")]
    Keyring(String),
}

fn random(bytes: usize) -> Result<String, OauthError> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|_| OauthError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// The S256 challenge for a verifier (RFC 7636).
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn encode(text: &str) -> String {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"-._~".contains(&b);
    text.bytes().map(|b| if plain(b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn decode(text: &str) -> String {
    let (bytes, mut out, mut i) = (text.as_bytes(), Vec::new(), 0);
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 2;
            }
            (b'+', _) => out.push(b' '),
            (other, _) => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The value of `key` in the query of an address, or of a request's path.
fn query(address: &str, key: &str) -> Option<String> {
    let (_, query) = address.split_once('?')?;
    let query = query.split(['#', ' ']).next().unwrap_or_default();
    query.split('&').filter_map(|pair| pair.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| decode(v))
}

/// What the browser brought back: proof for one exchange, good once.
pub struct Code(Secret);

/// A sign-in under way: the loopback address is held from before the
/// browser is sent off until the code has been exchanged, so nothing else
/// on the machine can be listening there in between.
pub struct SignIn {
    listener: TcpListener,
    verifier: Secret,
    state: String,
    redirect: String,
    url: String,
}

impl SignIn {
    /// `address` is offered to Google as the account to sign in to, so a
    /// browser holding several does not pick another.
    pub fn begin(endpoints: &Endpoints, client_id: &str, address: &str) -> Result<SignIn, OauthError> {
        // Port 0: whichever is free, so no port is this program's to be
        // squatted on (RFC 8252 §7.3). The literal address and not
        // `localhost`, which a hosts file may point elsewhere.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(OauthError::Listen)?;
        let port = listener.local_addr().map_err(OauthError::Listen)?.port();
        let (verifier, state, redirect) = (random(48)?, random(24)?, format!("http://127.0.0.1:{port}"));
        let parameters = [
            ("client_id", client_id),
            ("redirect_uri", &redirect),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("code_challenge", &challenge(&verifier)),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("login_hint", address),
            // What Google's web flow needs before it gives a refresh
            // token; its page for desktop clients says one always comes.
            // Sent so that setup run a second time gets one either way.
            ("access_type", "offline"),
            ("prompt", "consent"),
        ];
        let pairs: Vec<String> = parameters.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect();
        let url = format!("{}?{}", endpoints.authorize, pairs.join("&"));
        Ok(SignIn { listener, verifier: Secret::new(verifier), state, redirect, url })
    }

    /// Where the browser is sent.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The code in `address`, once its state has been checked to be this
    /// sign-in's: an address made by anyone else is refused.
    fn returned(&self, address: &str) -> Result<Code, OauthError> {
        if let Some(error) = query(address, "error") {
            let named = NAMED.iter().find(|name| **name == error).map_or("an error it did not name", |name| name);
            return Err(OauthError::Declined(named.to_string()));
        }
        let code = query(address, "code").ok_or(OauthError::NoCode)?;
        if query(address, "state").as_deref() != Some(self.state.as_str()) {
            return Err(OauthError::State);
        }
        Ok(Code(Secret::new(code)))
    }

    /// Waits for the browser to come back to the loopback address.
    pub fn wait(&self, patience: Duration) -> Result<Code, OauthError> {
        let deadline = Instant::now() + patience;
        self.listener.set_nonblocking(true).map_err(OauthError::Listen)?;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    // A browser also asks for an icon, and anything on the
                    // machine can connect: only a request that carries an
                    // answer ends the wait.
                    if let Some(answer) = self.serve(stream) {
                        return answer;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(OauthError::TimedOut(patience));
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(OauthError::Listen(e)),
            }
        }
    }

    fn serve(&self, stream: TcpStream) -> Option<Result<Code, OauthError>> {
        // A connection that cannot be read from is one more that carried
        // no answer; the wait goes on.
        stream.set_nonblocking(false).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).ok()?;
        let path = line.split(' ').nth(1).unwrap_or_default();
        let answer = (query(path, "code").is_some() || query(path, "error").is_some()).then(|| self.returned(path));
        let (status, words) = match &answer {
            Some(Ok(_)) => ("200 OK", "Signed in. This window can be closed; tuimex has what it needs."),
            Some(Err(_)) => ("400 Bad Request", "The sign-in was not completed. The terminal says why."),
            None => ("404 Not Found", "Nothing is here."),
        };
        let page = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{words}",
            words.len()
        );
        // The browser's page is a courtesy: the answer is already in hand.
        let _ = (&stream).write_all(page.as_bytes());
        answer
    }

    /// What the user pasted where the browser is on another machine: the
    /// address it ended on, or the code alone. A bare code has no state
    /// to check; the verifier still ties it to this sign-in, since the
    /// token server refuses a code issued for another challenge.
    pub fn pasted(&self, text: &str) -> Result<Code, OauthError> {
        let text = text.trim();
        if text.contains('?') {
            return self.returned(text);
        }
        if text.is_empty() || text.contains(char::is_whitespace) || text.contains("://") {
            return Err(OauthError::NoCode);
        }
        Ok(Code(Secret::new(decode(text))))
    }

    /// Exchanges the code. The refresh token is the caller's to keep in
    /// the keyring; the access token is good for the sign-in test.
    pub fn finish(self, endpoints: &Endpoints, client: &Client, code: &Code) -> Result<(Secret, Access), OauthError> {
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code.0.expose()),
            ("code_verifier", self.verifier.expose()),
            ("redirect_uri", &self.redirect),
            ("client_id", &client.id),
            ("client_secret", client.secret.expose()),
        ];
        let answer = ask(endpoints, &form)?;
        let refresh = text(&answer, "refresh_token")?;
        Ok((refresh, access(&answer)?))
    }
}

/// A token a server takes in place of a password, and when it stops.
#[derive(Debug, Clone)]
pub struct Access {
    pub token: Secret,
    until: Instant,
}

impl Access {
    fn fresh(&self) -> bool {
        Instant::now() + MARGIN < self.until
    }
}

fn text(answer: &serde_json::Value, key: &'static str) -> Result<Secret, OauthError> {
    let found = answer.get(key).and_then(|v| v.as_str()).filter(|v| !v.is_empty());
    found.map(|v| Secret::new(v.to_string())).ok_or(OauthError::Incomplete(key))
}

fn access(answer: &serde_json::Value) -> Result<Access, OauthError> {
    // A server that does not say how long is taken at an hour, as
    // Google's are; one that ends sooner is refused by the mail server
    // and replaced on the next connection.
    let seconds = answer.get("expires_in").and_then(serde_json::Value::as_u64).unwrap_or(3600);
    Ok(Access { token: text(answer, "access_token")?, until: Instant::now() + Duration::from_secs(seconds) })
}

/// One request to the token endpoint. The answer is parsed and never
/// quoted: a refusal becomes its RFC 6749 name, anything else its status.
fn ask(endpoints: &Endpoints, form: &[(&str, &str)]) -> Result<serde_json::Value, OauthError> {
    let loopback = endpoints.token.starts_with("http://127.0.0.1:");
    if !endpoints.token.starts_with("https://") && !loopback {
        return Err(OauthError::Unreachable("a token endpoint is asked over TLS or not at all".to_string()));
    }
    let config = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(PATIENCE)).build();
    // The transport's error names the host and what failed, never the body.
    let unreachable = |e: ureq::Error| OauthError::Unreachable(e.to_string());
    let mut response =
        config.new_agent().post(&endpoints.token).send_form(form.iter().copied()).map_err(unreachable)?;
    let status = response.status();
    let body = response.body_mut().with_config().limit(1 << 16).read_to_string().map_err(unreachable)?;
    let answer: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    if status.is_success() && answer.get("error").is_none() {
        return Ok(answer);
    }
    let said = answer.get("error").and_then(|e| e.as_str()).unwrap_or_default();
    match NAMED.iter().find(|name| **name == said) {
        Some(name) => Err(OauthError::Refused(name)),
        None => Err(OauthError::Unreachable(format!("it answered {status} with an error it did not name"))),
    }
}

/// A new access token from the kept refresh token.
pub fn refresh(endpoints: &Endpoints, client: &Client, refresh: &Secret) -> Result<Access, OauthError> {
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.expose()),
        ("client_id", &client.id),
        ("client_secret", client.secret.expose()),
    ];
    access(&ask(endpoints, &form)?)
}

/// The shared client, as `tuimex setup` kept it.
pub fn client(keyring: &dyn Keyring) -> Result<Option<Client>, OauthError> {
    let kept = |entry| keyring.password(entry).map_err(OauthError::Keyring);
    Ok(match (kept(CLIENT_ID)?, kept(CLIENT_SECRET)?) {
        (Some(id), Some(secret)) => Some(Client { id: id.expose().to_string(), secret }),
        _ => None,
    })
}

/// One Gmail account's access token for as long as the process runs: asked
/// for when first needed and again when it is about to end, and shared by
/// the account's connections.
#[derive(Debug)]
pub struct Tokens {
    endpoints: Endpoints,
    held: Mutex<Option<Access>>,
}

impl Tokens {
    pub fn new(endpoints: Endpoints) -> Tokens {
        Tokens { endpoints, held: Mutex::new(None) }
    }

    /// A token good for at least the next minute.
    pub fn access(&self, account: &str, keyring: &dyn Keyring) -> Result<Secret, OauthError> {
        // A thread that panicked while holding the token left it whole or
        // absent, and either is fine to go on from.
        let mut held = self.held.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(access) = held.as_ref().filter(|access| access.fresh()) {
            return Ok(access.token.clone());
        }
        let client = client(keyring)?.ok_or_else(|| OauthError::NotKept("the Google client".to_string()))?;
        let kept = keyring.password(&refresh_entry(account)).map_err(OauthError::Keyring)?;
        let kept = kept.ok_or_else(|| OauthError::NotKept(format!("the Gmail sign-in for {account}")))?;
        let access = refresh(&self.endpoints, &client, &kept).map_err(|error| match error {
            OauthError::Refused("invalid_grant") => OauthError::Revoked { account: account.to_string() },
            other => other,
        })?;
        Ok(held.insert(access).token.clone())
    }

    /// Forgets the token, for when a mail server has refused it.
    pub fn forget(&self) {
        *self.held.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

#[cfg(test)]
mod tests;

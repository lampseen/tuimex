//! Google's sign-in, played on a loopback port (ADR-019): an authorisation
//! endpoint that a test's "browser" visits by calling `authorize`, and a
//! token endpoint that is real HTTP, since the token requests are what
//! tuimex makes itself. It checks what Google checks: the client, the
//! redirect address, and that the verifier hashes to the challenge.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// A consent given and not yet exchanged.
struct Pending {
    code: String,
    challenge: String,
    redirect: String,
    client_id: String,
}

#[derive(Default)]
struct State {
    client: (String, String),
    pending: Vec<Pending>,
    /// Refresh tokens still good, and the access tokens issued for each.
    refresh: Vec<String>,
    access: Vec<String>,
    issued: usize,
    lifetime: u64,
    /// The one access token handed out, where a test's mail server takes
    /// a token it chose and no other.
    fixed: Option<String>,
    /// Every form posted to the token endpoint, as it arrived.
    asked: Vec<Vec<(String, String)>>,
}

#[derive(Clone)]
pub struct FakeGoogle {
    state: Arc<Mutex<State>>,
    port: u16,
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

fn pairs(query: &str) -> Vec<(String, String)> {
    query.split('&').filter_map(|pair| pair.split_once('=')).map(|(k, v)| (k.to_string(), decode(v))).collect()
}

fn value<'a>(pairs: &'a [(String, String)], key: &str) -> &'a str {
    pairs.iter().find(|(k, _)| k == key).map_or("", |(_, v)| v.as_str())
}

impl FakeGoogle {
    /// Starts the token endpoint. `client` is the ID and secret it was
    /// "registered" with; access tokens last an hour until `lifetime`
    /// says otherwise.
    pub fn start(client_id: &str, client_secret: &str) -> std::io::Result<FakeGoogle> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let state =
            State { client: (client_id.to_string(), client_secret.to_string()), lifetime: 3600, ..State::default() };
        let google = FakeGoogle { state: Arc::new(Mutex::new(state)), port };
        let serving = google.clone();
        // The thread ends with the test's process; nothing waits on it.
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                // A request that cannot be read or answered is the
                // client's failure to report, not this server's.
                let _ = serving.serve(stream);
            }
        });
        Ok(google)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn authorize_url(&self) -> String {
        format!("http://127.0.0.1:{}/auth", self.port)
    }

    pub fn token_url(&self) -> String {
        format!("http://127.0.0.1:{}/token", self.port)
    }

    /// The user consenting in a browser sent to `url`: the address the
    /// browser is sent back to, or why Google would show an error page
    /// instead of sending it anywhere.
    pub fn authorize(&self, url: &str) -> Result<String, String> {
        let asked = pairs(url.split_once('?').map_or("", |(_, query)| query));
        let mut state = self.state();
        let redirect = value(&asked, "redirect_uri");
        let loopback = redirect.strip_prefix("http://127.0.0.1:").is_some_and(|port| port.parse::<u16>().is_ok());
        for (wrong, why) in [
            (value(&asked, "client_id") != state.client.0, "the client is not one Google knows"),
            (!loopback, "the redirect is not a loopback address"),
            (value(&asked, "response_type") != "code", "the response type is not code"),
            (value(&asked, "scope") != "https://mail.google.com/", "the scope is not the mail scope"),
            (value(&asked, "code_challenge_method") != "S256", "the challenge is not S256"),
            (value(&asked, "code_challenge").len() != 43, "the challenge is not a SHA-256"),
            (value(&asked, "state").len() < 16, "the state is too short to be unguessable"),
            (value(&asked, "access_type") != "offline", "no refresh token was asked for"),
        ] {
            if wrong {
                return Err(why.to_string());
            }
        }
        state.issued += 1;
        let code = format!("4/synthetic-code-{}", state.issued);
        state.pending.push(Pending {
            code: code.clone(),
            challenge: value(&asked, "code_challenge").to_string(),
            redirect: redirect.to_string(),
            client_id: value(&asked, "client_id").to_string(),
        });
        Ok(format!(
            "{redirect}/?state={}&code={}&scope=https://mail.google.com/",
            value(&asked, "state"),
            code.replace('/', "%2F")
        ))
    }

    /// How long the access tokens issued from now on last, in seconds.
    pub fn lifetime(&self, seconds: u64) {
        self.state().lifetime = seconds;
    }

    /// Makes every access token issued from now on `token`, or one of
    /// this server's own making again.
    pub fn access_tokens_are(&self, token: Option<&str>) {
        self.state().fixed = token.map(String::from);
    }

    /// The user removing the app's access in their Google account.
    pub fn revoke(&self) {
        let mut state = self.state();
        state.refresh.clear();
        state.access.clear();
    }

    /// Whether a mail server asking Google would be told this token is good.
    pub fn accepts(&self, access_token: &str) -> bool {
        self.state().access.iter().any(|token| token == access_token)
    }

    /// Every token this has issued and still honours: what a leak would be of.
    pub fn tokens(&self) -> Vec<String> {
        let state = self.state();
        state.refresh.iter().chain(&state.access).cloned().collect()
    }

    /// The forms posted to the token endpoint so far.
    pub fn asked(&self) -> Vec<Vec<(String, String)>> {
        self.state().asked.clone()
    }

    fn grant(&self, form: &[(String, String)]) -> Result<String, &'static str> {
        let mut state = self.state();
        state.asked.push(form.to_vec());
        if (value(form, "client_id"), value(form, "client_secret"))
            != (state.client.0.as_str(), state.client.1.as_str())
        {
            return Err("invalid_client");
        }
        state.issued += 1;
        let access = state.fixed.clone().unwrap_or_else(|| format!("ya29.synthetic-access-{}", state.issued));
        let lifetime = state.lifetime;
        let refresh = match value(form, "grant_type") {
            "authorization_code" => {
                let at = state.pending.iter().position(|p| p.code == value(form, "code")).ok_or("invalid_grant")?;
                // A code is good once, whatever comes of the attempt.
                let pending = state.pending.remove(at);
                let hashed = URL_SAFE_NO_PAD.encode(Sha256::digest(value(form, "code_verifier").as_bytes()));
                if hashed != pending.challenge || value(form, "redirect_uri") != pending.redirect {
                    return Err("invalid_grant");
                }
                if pending.client_id != state.client.0 {
                    return Err("invalid_client");
                }
                let refresh = format!("1//synthetic-refresh-{}", state.issued);
                state.refresh.push(refresh.clone());
                format!(", \"refresh_token\": \"{refresh}\"")
            }
            "refresh_token" => {
                if !state.refresh.iter().any(|kept| kept == value(form, "refresh_token")) {
                    return Err("invalid_grant");
                }
                String::new()
            }
            _ => return Err("unsupported_grant_type"),
        };
        state.access.push(access.clone());
        Ok(format!(
            "{{\"access_token\": \"{access}\", \"expires_in\": {lifetime}, \"token_type\": \"Bearer\", \
             \"scope\": \"https://mail.google.com/\"{refresh}}}"
        ))
    }

    fn serve(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let (mut request, mut length) = (String::new(), 0);
        reader.read_line(&mut request)?;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        let form = pairs(&String::from_utf8_lossy(&body));
        let (status, answer) = if !request.starts_with("POST /token ") {
            ("404 Not Found", "{}".to_string())
        } else {
            match self.grant(&form) {
                Ok(answer) => ("200 OK", answer),
                // As Google does, the refusal describes itself; here it
                // also repeats every secret it was sent, so that a client
                // which printed the description would be caught.
                Err(error) => {
                    let sent: Vec<&str> = form.iter().map(|(_, v)| v.as_str()).collect();
                    let described = sent.join(" ").replace(['"', '\\'], "");
                    ("400 Bad Request", format!("{{\"error\": \"{error}\", \"error_description\": \"{described}\"}}"))
                }
            }
        };
        let head =
            format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", answer.len());
        stream.write_all(format!("{head}Connection: close\r\n\r\n{answer}").as_bytes())
    }
}

#[cfg(test)]
mod tests;

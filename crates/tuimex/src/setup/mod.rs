//! `tuimex setup` (PLAN §6.7): the questions that add a mail account, or
//! sign in again to one that is there. Both servers take the sign-in
//! before anything is saved; a secret then goes to the keyring and to no
//! file (ADR-010), an accepted certificate beside the config (ADR-012),
//! and the account to the end of the config file.

mod prompt;
mod providers;
pub mod rename;
mod write;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tuimex_core::{RemoteError, RemoteFolder, Role};
use tuimex_engine::oauth::{self, Endpoints, OauthError, SignIn, Tokens};
use tuimex_engine::{Credential, Keyring, Protocol, Secret, Server, pin};

use crate::config::{self, Config};
use crate::net::{self, Auth, Net};
use crate::outside::Opener;
pub use prompt::{Prompt, Terminal};
use prompt::{number, or, yes};
use providers::{ANY_PASSWORD, PROVIDERS};

/// How long the browser has to come back from Google's sign-in.
const BROWSER: Duration = Duration::from_secs(300);

/// What to do on a machine with no keyring service (ADR-010).
pub const NO_KEYRING: &str = "On a machine with no keyring service (a server, a container, a bare SSH session), \
give the account a command that prints its password instead: `tuimex setup` asks for one, and any program will \
do, such as `pass show mail/work`.";

/// ADR-019's walk-through. The steps are Google's and move; docs/testing.md
/// has them at length, with the date they were last checked.
const GOOGLE: &str = "\
Signing in with Google needs an \"OAuth client\": an entry in Google's cloud console that tells Google which
program is asking. Google lets no small project ship one, so you make your own, once, and every Gmail account
here uses it. In a browser:
  1. https://console.cloud.google.com : make a project, and enable the Gmail API in it.
  2. Google Auth platform > Audience: External, with each Gmail address you will use added as a test user.
  3. Data Access: add the scope https://mail.google.com/
  4. Clients: create a client of type \"Desktop app\". Copy its ID and its secret then: the secret is shown once.
Google will say the app is unverified when you sign in: it is yours, and only you use it. While the app's status
is \"Testing\" Google ends each sign-in after seven days; putting it in production, under Audience, stops that.
The client's ID ends .apps.googleusercontent.com; its secret is the shorter string beside it.";

pub struct Setup<'a> {
    pub prompt: &'a mut dyn Prompt,
    pub keyring: &'a dyn Keyring,
    pub net: &'a dyn Net,
    /// Where pinned certificates are kept.
    pub config_dir: &'a Path,
    /// Where the mail is, and the index of it: a rename moves the one
    /// and drops the other.
    pub data: &'a Path,
    pub cache: &'a Path,
    pub config_path: &'a Path,
    pub endpoints: Endpoints,
    /// What opens a browser at an address. None where there is no browser
    /// to open, as over SSH.
    pub browser: Option<&'a mut dyn Opener>,
    /// The Claude CLI, where one is on the path.
    pub claude: Option<PathBuf>,
}

/// What signing in needs kept once the servers have taken it: keyring
/// entries, each with its secret.
type Keep = Vec<(String, Secret)>;

fn at(server: &Server) -> String {
    format!("{}:{}", server.host, server.port)
}

/// An account's name is its directory, its keyring entry and its
/// certificate's file name, so it is kept to what is safe as all three.
fn valid_name(name: &str) -> bool {
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_.".contains(c);
    !name.is_empty() && !name.starts_with('.') && name.chars().all(plain)
}

/// What tuimex puts in the folder with `role`, for someone asked which of
/// their folders that is.
fn holds(role: Role) -> &'static str {
    match role {
        Role::Archive => "what you archive with `e`",
        Role::Trash => "what you delete with `d`",
        Role::Sent => "a copy of what you send",
        Role::Drafts => "what you have not finished writing",
        _ => "what is marked as spam",
    }
}

impl Setup<'_> {
    fn say(&mut self, text: &str) {
        self.prompt.say(text);
    }

    pub fn run(&mut self) -> Result<()> {
        let existing = match std::fs::read_to_string(self.config_path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", self.config_path.display())),
        };
        let config = Config::parse(&existing).with_context(|| {
            let path = self.config_path.display();
            format!("the config {path} does not read, and setup adds only to one that does")
        })?;
        if let Some((old, new)) = rename::unfinished(self.data)? {
            self.say(&format!("The rename of {old} to {new} was begun and not finished. Finishing it now."));
            return self.renamed(&config, &old, &new);
        }
        self.say("tuimex setup adds a mail account. Nothing is saved until its servers have taken the sign-in.");
        if !config.accounts.is_empty() {
            let names: Vec<&str> = config.accounts.iter().map(|account| account.name.as_str()).collect();
            let mut menu = vec!["  1) Add another account".to_string()];
            for name in &names {
                menu.push(format!("  {}) Sign in again to {name}", menu.len() + 1));
                menu.push(format!("  {}) Accept a new certificate for {name}", menu.len() + 1));
            }
            let rename = menu.len() + 1;
            menu.push(format!("  {rename}) Rename an account"));
            let quit = rename + 1;
            menu.push(format!("  {quit}) Quit"));
            self.say(&format!("\nAccounts here: {}.\n{}", names.join(", "), menu.join("\n")));
            match number(self.prompt, "What would you like to do", quit, Some(1))? {
                Some(n) if n == quit => {
                    self.say("Nothing was changed.");
                    return Ok(());
                }
                Some(n) if n == rename => return self.rename(&existing, &config),
                // Two lines to an account, after the first.
                Some(n) if n > 1 => return self.again(&config.accounts[(n - 2) / 2], n % 2 == 0),
                _ => {}
            }
        }
        self.add(&existing, &config)
    }

    /// `Rename an account`: which, to what, what that moves, and a yes.
    fn rename(&mut self, existing: &str, config: &Config) -> Result<()> {
        let names: Vec<&str> = config.accounts.iter().map(|account| account.name.as_str()).collect();
        let old = match names.as_slice() {
            [only] => only,
            _ => {
                let listed: Vec<String> = names.iter().enumerate().map(|(i, n)| format!("  {}) {n}", i + 1)).collect();
                self.say(&format!("\n{}", listed.join("\n")));
                loop {
                    if let Some(n) = number(self.prompt, "Rename which", names.len(), None)? {
                        break names[n - 1];
                    }
                }
            }
        };
        let new = loop {
            let new = self.prompt.ask(&format!("The new name for {old}:"))?;
            match rename::refused(&self.places(config, old), config, existing, old, &new) {
                None => break new,
                Some(why) => self.say(&why),
            }
        };
        self.say(&format!(
            "\nThis renames {old} to {new} in four places: its mail on this computer, its saved sign-in in the \
             keyring, its server's certificate if one was accepted, and its name in the config.\nNo mail is \
             downloaded again, and nothing changes on the server."
        ));
        if !yes(self.prompt, "Go ahead?", false)? {
            self.say("Nothing was changed.");
            return Ok(());
        }
        self.renamed(config, old, &new)
    }

    fn places<'a>(&'a self, config: &Config, name: &str) -> rename::Places<'a> {
        let by_command = config.account(name).is_some_and(|account| account.password_cmd.is_some());
        rename::Places {
            data: self.data,
            cache: self.cache,
            config_dir: self.config_dir,
            config_path: self.config_path,
            keyring: (!by_command).then_some(self.keyring),
        }
    }

    /// Carries a rename out, or the rest of one, and says how it ended.
    fn renamed(&mut self, config: &Config, old: &str, new: &str) -> Result<()> {
        // Whichever name the config has now is the account's.
        let named = if config.account(old).is_some() { old } else { new };
        let notes = rename::carry_out(&self.places(config, named), old, new)?;
        notes.iter().for_each(|note| self.say(note));
        self.say(&format!("{old} is now {new}."));
        Ok(())
    }

    /// An answer `accept` takes, asked until it does.
    fn asked(&mut self, question: &str, accept: impl Fn(&str) -> bool, wanted: &str) -> Result<String> {
        loop {
            let answer = self.prompt.ask(question)?;
            if accept(&answer) {
                return Ok(answer);
            }
            self.say(wanted);
        }
    }

    fn port(&mut self, question: &str, default: u16) -> Result<u16> {
        loop {
            match or(self.prompt, question, &default.to_string())?.parse() {
                Ok(port) => return Ok(port),
                Err(_) => self.say("A port is a number from 1 to 65535."),
            }
        }
    }

    /// How TLS begins on a server whose port was typed: from the first
    /// byte if that works, otherwise by STARTTLS. That order, because a
    /// server waiting for STARTTLS answers a handshake at once with
    /// something that is not one, while a server waiting for a handshake
    /// answers a client waiting for its greeting with silence. There is
    /// no third way to find (ADR-012).
    fn tls(&mut self, server: Server, protocol: Protocol) -> Result<Server> {
        let direct = Server { starttls: false, ..server.clone() };
        if self.net.certificate(&direct, protocol).is_ok() {
            return Ok(direct);
        }
        let upgraded = Server { starttls: true, ..server };
        let unreachable = |e| anyhow!("{} could not be reached over TLS, at once or by STARTTLS: {e}", at(&upgraded));
        self.net.certificate(&upgraded, protocol).map_err(unreachable)?;
        Ok(upgraded)
    }

    /// Both servers: a provider's, or typed.
    fn servers(&mut self, address: &str, provider: Option<usize>) -> Result<(Server, Server)> {
        let known = provider.map(|at| &PROVIDERS[at]);
        let (mut imap, mut imap_port) = known.map_or((String::new(), 993), |p| (p.imap.0.to_string(), p.imap.1));
        let (mut smtp_host, mut smtp_port) = known.map_or((String::new(), 465), |p| (p.smtp.0.to_string(), p.smtp.1));
        let mut user = address.to_string();
        // A provider's servers are known. Bridge's are this machine's, on
        // ports and under a username it chooses.
        if known.is_none_or(|p| p.short == "proton") {
            let host = |answer: &str| !answer.is_empty() && !answer.contains(char::is_whitespace);
            if known.is_none() {
                self.say(
                    "\nYour provider's help pages give the next four answers, usually on a page about setting up \
                     a mail program\n(or \"IMAP and SMTP settings\"). IMAP is how mail is read, and SMTP how it \
                     is sent.",
                );
                let wanted = "A server's name, such as imap.example.com, with no spaces.";
                imap =
                    self.asked("IMAP server: the one mail is read from (such as imap.example.com):", host, wanted)?;
            }
            imap_port = self.port("IMAP port", imap_port)?;
            if known.is_none() {
                let wanted = "A server's name, such as smtp.example.com, with no spaces.";
                smtp_host = self.asked("SMTP server: the one mail is sent through:", host, wanted)?;
            }
            smtp_port = self.port("SMTP port", smtp_port)?;
            user = or(self.prompt, "Username for signing in (usually the whole address)", address)?;
        }
        let server = |host: String, port: u16, implicit: u16, user: &str| Server {
            host,
            port,
            starttls: port != implicit,
            user: user.to_string(),
        };
        let local = address.split('@').next().unwrap_or(address);
        let reading = if known.is_some_and(|p| p.imap_user_is_local) { local } else { &user };
        let (imap, smtp) = (server(imap, imap_port, 993, reading), server(smtp_host, smtp_port, 465, &user));
        if known.is_none_or(|p| p.short == "proton") {
            return Ok((self.tls(imap, Protocol::Imap)?, self.tls(smtp, Protocol::Smtp)?));
        }
        Ok((imap, smtp))
    }

    fn password(&mut self, name: &str, keep: &mut Keep) -> Result<Credential> {
        let typed = self.prompt.secret("Password (it is not shown as you type):")?;
        if typed.expose().is_empty() {
            bail!("no password was typed; nothing was saved");
        }
        keep.push((name.to_string(), typed.clone()));
        Ok(Credential::Password(typed))
    }

    fn command(&mut self, name: &str) -> Result<(Credential, Vec<String>)> {
        self.say(
            "Type the command one word to a line: the program, then each argument, then an empty line to finish.\n\
             For `pass show mail/work` that is `pass`, `show`, `mail/work`, and Enter alone.",
        );
        let mut words = Vec::new();
        loop {
            let word = self.prompt.ask(if words.is_empty() { "Program:" } else { "Argument:" })?;
            if word.is_empty() {
                break;
            }
            words.push(word);
        }
        let password = tuimex_engine::resolve(name, Some(&words), self.keyring)?;
        Ok((Credential::Password(password), words))
    }

    fn pasted(&mut self, sign_in: &SignIn) -> Result<oauth::Code> {
        self.say(&format!(
            "Open this address in a browser, on any machine, and sign in:\n\n  {}\n\nGoogle then sends the browser \
             to an address beginning http://127.0.0.1 which, on another machine, does not load. That is expected: \
             copy the whole address from the address bar.",
            sign_in.url()
        ));
        loop {
            match sign_in.pasted(&self.prompt.ask("That address, or the code alone:")?) {
                Ok(code) => return Ok(code),
                Err(OauthError::NoCode) => self.say("That holds no code."),
                Err(other) => return Err(other.into()),
            }
        }
    }

    /// Google's sign-in (ADR-019), with the one client every Gmail
    /// account here shares.
    fn google(&mut self, name: &str, address: &str, keep: &mut Keep) -> Result<Credential> {
        let unreadable = |reason| anyhow!("the keyring could not be read: {reason}\n{NO_KEYRING}");
        let client = match oauth::client(self.keyring) {
            Ok(Some(client)) => {
                self.say("Using the Google client already in the keyring.");
                client
            }
            Ok(None) => {
                self.say(GOOGLE);
                let id = self.asked("Client ID:", |id| !id.is_empty(), "The ID ends .apps.googleusercontent.com")?;
                let secret = self.prompt.secret("Client secret (it is not shown as you type):")?;
                keep.push((oauth::CLIENT_ID.to_string(), Secret::new(id.clone())));
                keep.push((oauth::CLIENT_SECRET.to_string(), secret.clone()));
                oauth::Client { id, secret }
            }
            Err(OauthError::Keyring(reason)) => return Err(unreadable(reason)),
            Err(other) => return Err(other.into()),
        };
        let sign_in = SignIn::begin(&self.endpoints, &client.id, address)?;
        // A browser that will not start is the same as none: the address
        // is printed either way.
        let opened = self.browser.as_mut().is_some_and(|browser| browser.open(sign_in.url()).is_ok());
        let code = if opened {
            self.say(&format!(
                "A browser is opening Google's sign-in. If none did, open this address yourself:\n\n  {}\n\n\
                 Waiting for the sign-in to finish there.",
                sign_in.url()
            ));
            match sign_in.wait(BROWSER) {
                Ok(code) => code,
                Err(OauthError::TimedOut(_)) => self.pasted(&sign_in)?,
                Err(other) => return Err(other.into()),
            }
        } else {
            self.pasted(&sign_in)?
        };
        let (refresh, access) = sign_in.finish(&self.endpoints, &client, &code)?;
        keep.push((oauth::refresh_entry(name), refresh));
        Ok(Credential::Bearer(access.token))
    }

    /// The certificate to pin for the account, or None for servers this
    /// system's roots already vouch for, and whether that is a change to
    /// save. An account has one pin, for both of its servers (ADR-012).
    fn trust(&mut self, name: &str, imap: &Server, smtp: Option<&Server>) -> Result<(Option<Vec<u8>>, bool)> {
        let reach = |server: &Server, protocol| {
            self.net.certificate(server, protocol).map_err(|e| anyhow!("{} could not be reached: {e}", at(server)))
        };
        let seen = reach(imap, Protocol::Imap)?;
        let sending = smtp.map(|smtp| reach(smtp, Protocol::Smtp)).transpose()?;
        let pinned = pin::load(self.config_dir, name)?;
        if pinned.is_none() && seen.trusted && sending.as_ref().is_none_or(|s| s.trusted) {
            return Ok((None, false));
        }
        if let Some(other) = sending.filter(|s| s.certificate != seen.certificate) {
            bail!(
                "the two servers present different certificates, and an account trusts one for both:\n  {}: {}\n  \
                 {}: {}\nnothing was saved",
                at(imap),
                pin::fingerprint(&seen.certificate),
                smtp.map(at).unwrap_or_default(),
                pin::fingerprint(&other.certificate)
            );
        }
        let presented = pin::fingerprint(&seen.certificate);
        match pinned {
            Some(pinned) if pinned == seen.certificate => {
                self.say(&format!("The server still presents the certificate pinned for {name}."));
                return Ok((Some(pinned), false));
            }
            Some(pinned) => self.say(&format!(
                "{} presents a certificate that is not the one pinned for {name}.\n  pinned:    {}\n  presented: \
                 {presented}\nA server that signs its own certificate makes a new one now and then; so does someone \
                 standing between you and it. Compare the second line with what the server itself shows.",
                at(imap),
                pin::fingerprint(&pinned)
            )),
            None => self.say(&format!(
                "\nA certificate is how a server proves it is the one you meant to reach.\n{} presents a \
                 certificate this system has no reason to trust, as a server that signs its own does (Proton \
                 Bridge is one, and so is a server you run yourself). Someone standing between you and the \
                 server would present one too.\n  SHA-256 fingerprint: {presented}\nThe fingerprint is a short \
                 form of the certificate. Where the server or whoever runs it shows one, compare the two; a \
                 server on this computer, as Bridge is, has nobody in between.",
                at(imap)
            )),
        }
        if !yes(self.prompt, &format!("Trust this certificate for {name}, and for nothing else?"), false)? {
            bail!("the certificate was not accepted; nothing was saved");
        }
        Ok((Some(seen.certificate), true))
    }

    /// Signs in to both servers with what is in hand.
    fn try_out(
        &mut self,
        name: &str,
        imap: &Server,
        smtp: Option<&Server>,
        credential: &Credential,
        pin: &Option<Vec<u8>>,
    ) -> Result<(Vec<RemoteFolder>, bool)> {
        let refused = |server: &Server, error: tuimex_engine::Error| match error {
            tuimex_engine::Error::Remote(RemoteError::Auth) => {
                anyhow!("{} refused the sign-in; nothing was saved", at(server))
            }
            other => anyhow!("{}: {other}; nothing was saved", at(server)),
        };
        let found = self.net.folders(imap, name, credential, pin.clone()).map_err(|e| refused(imap, e.into()))?;
        let (count, plural) = (found.0.len(), if found.0.len() == 1 { "" } else { "s" });
        self.say(&format!("Signed in to {} and found {count} folder{plural}.", at(imap)));
        if let Some(smtp) = smtp {
            self.net.smtp(smtp, name, credential, pin.clone()).map_err(|e| refused(smtp, e))?;
            self.say(&format!("Signed in to {}; nothing was sent.", at(smtp)));
        }
        Ok(found)
    }

    /// The roles the mapping could not fill (PLAN §4.1), asked for from
    /// the folders that have none.
    fn roles(&mut self, folders: &[RemoteFolder], gmail: bool) -> Result<Vec<(&'static str, String)>> {
        let assigned = tuimex_engine::roles(folders, &[], gmail);
        let mut free: Vec<String> =
            assigned.iter().filter(|(_, r)| *r == Role::Other).map(|(n, _)| n.clone()).collect();
        let (mut chosen, mut unfilled) = (Vec::new(), Vec::new());
        for (role, key) in net::ROLES {
            if assigned.iter().any(|(_, has)| *has == role) {
                continue;
            }
            if free.is_empty() {
                unfilled.push(key);
                continue;
            }
            let listed: Vec<String> = free.iter().enumerate().map(|(i, name)| format!("  {}) {name}", i + 1)).collect();
            self.say(&format!(
                "\nNo folder here is plainly the {key} folder, which is where tuimex puts {}. The others are:\n{}",
                holds(role),
                listed.join("\n")
            ));
            match number(self.prompt, &format!("Which is the {key} folder? Enter for none"), free.len(), None)? {
                Some(n) => chosen.push((key, free.remove(n - 1))),
                None => unfilled.push(key),
            }
        }
        if !unfilled.is_empty() {
            self.say(&format!(
                "There is no folder for: {}. tuimex asks before it makes an Archive or a Trash, the first time \
                 you archive or delete. For the others, make the folder with your provider and name it under \
                 `folders` in the config.",
                unfilled.join(", ")
            ));
        }
        Ok(chosen)
    }

    fn keep(&mut self, keep: &Keep) -> Result<()> {
        for (entry, secret) in keep {
            let refused = |reason| anyhow!("the keyring would not keep it: {reason}\n{NO_KEYRING}\nNothing was saved.");
            self.keyring.set(entry, secret).map_err(refused)?;
        }
        Ok(())
    }

    fn add(&mut self, existing: &str, config: &Config) -> Result<()> {
        let is_address = |a: &str| a.split_once('@').is_some_and(|(l, d)| !l.is_empty() && d.contains('.'));
        let address = self.asked("\nEmail address:", |a| is_address(a) && !a.contains(' '), "An address, as a@b.c")?;
        let listed: Vec<String> =
            PROVIDERS.iter().enumerate().map(|(i, p)| format!("  {}) {}", i + 1, p.name)).collect();
        let other = PROVIDERS.len() + 1;
        self.say(&format!(
            "\nWho provides this mailbox? tuimex knows the servers of the first {}.\n{}\n  {other}) Someone else: \
             you type its servers' names",
            PROVIDERS.len(),
            listed.join("\n")
        ));
        let guess = providers::guess(&address).map_or(other, |at| at + 1);
        let provider = number(self.prompt, "Which", other, Some(guess))?.filter(|n| *n != other).map(|n| n - 1);
        if let Some(first) = provider.and_then(|at| PROVIDERS[at].first) {
            self.say(&format!("\n{first}"));
        }
        let (imap, smtp) = self.servers(&address, provider)?;

        let taken = |name: &str| config.account(name).is_some();
        let base = provider.map_or_else(
            || address.rsplit('@').next().and_then(|d| d.split('.').next()).unwrap_or("mail").to_string(),
            |at| PROVIDERS[at].short.to_string(),
        );
        let free = (2..).map(|n| format!("{base}-{n}")).find(|name| !taken(name)).unwrap_or_default();
        let offered = if taken(&base) || !valid_name(&base) { free } else { base };
        let name = loop {
            let name = or(self.prompt, "A short name for this account, to tell it from others in tuimex", &offered)?;
            match (valid_name(&name), taken(&name)) {
                (true, false) => break name,
                (false, _) => self.say("Letters, digits, `-`, `_` and `.`; not starting with a dot."),
                (true, true) => self.say("There is an account of that name already."),
            }
        };

        let oauth = provider.is_some_and(|at| PROVIDERS[at].oauth);
        let mut ways = format!(
            "\nHow should tuimex sign in to {address}?\n\n  1) {}\n     tuimex keeps it in this computer's keyring \
             (the system's password store),\n     and in no file.\n  2) A command that prints the password: for \
             people who keep passwords in a\n     command-line password manager. You type the command, such as \
             `pass show\n     mail/{name}`, and tuimex runs it whenever it signs in. This is also the way\n     on \
             a machine with no keyring, such as a server reached over SSH.",
            provider.map_or(ANY_PASSWORD, |at| PROVIDERS[at].password)
        );
        if oauth {
            ways += "\n  3) Sign in with Google in a browser: tuimex is given no password. The first\n     time \
                     takes about ten minutes, because Google has you create an \"OAuth\n     client\" in its \
                     cloud console first. tuimex lists the steps, then asks for\n     the client's ID and \
                     secret. A second Gmail account reuses them.";
        }
        self.say(&ways);
        let (mut keep, mut password_cmd) = (Keep::new(), Vec::new());
        // The typed password is offered first everywhere: it needs
        // nothing made or installed beforehand but the password itself.
        let (auth, credential) = match number(self.prompt, "Which", if oauth { 3 } else { 2 }, Some(1))? {
            Some(3) => (Auth::Gmail, self.google(&name, &address, &mut keep)?),
            Some(2) => {
                let (credential, words) = self.command(&name)?;
                password_cmd = words;
                (Auth::Command, credential)
            }
            _ => (Auth::Password, self.password(&name, &mut keep)?),
        };

        let (pin, _) = self.trust(&name, &imap, Some(&smtp))?;
        let (folders, gmail) = self.try_out(&name, &imap, Some(&smtp), &credential, &pin)?;
        let folders = self.roles(&folders, gmail)?;

        self.say(
            "\nAn agent is an AI assistant or a script you run yourself, which reads mail by calling the `tuimex` \
             command.\nIt can never send, move or delete. If you use none, answer no to both: Enter does.",
        );
        let read = yes(self.prompt, "May an agent read this account's Inbox?", false)?;
        let draft = yes(self.prompt, "May an agent write drafts in it, for you to send or discard?", false)?;
        let mut tables = vec![write::account(&write::Entry {
            name: name.clone(),
            address,
            imap,
            smtp,
            auth,
            password_cmd,
            folders,
            agent: (read, draft),
        })];
        if let (None, Some(claude)) = (&config.agent, self.claude.clone()) {
            let question = format!(
                "\nLet `:agent` use the Claude CLI found at {}? It drafts or tidies a reply, or advises on a link, \
                 from the message with names, addresses and numbers replaced; that text goes to Anthropic, and \
                 only when you type `:agent`.",
                claude.display()
            );
            if yes(self.prompt, &question, false)? {
                tables.push("[agent]\nuse = \"claude\"\n".to_string());
            }
        }

        // Made before anything is kept, so a config that cannot take the
        // account leaves no password in the keyring for an account that
        // is not there.
        let text = write::appended(existing, &tables)?;
        self.keep(&keep)?;
        if let Some(certificate) = &pin {
            pin::save(self.config_dir, &name, certificate)?;
        }
        write::save(self.config_path, &text, self.config_dir)?;
        self.say(&format!(
            "\nSaved {name} in {}.{}{}\n`tuimex doctor` checks it again; `tuimex` opens the mail.",
            self.config_path.display(),
            if keep.is_empty() { "" } else { " What signs it in is in the keyring, and in no file." },
            if pin.is_some() { " Its server's certificate is beside the config." } else { "" },
        ));
        Ok(())
    }

    /// An account that is already in the config: with `fresh`, a new
    /// sign-in; without, what is kept, to accept its server's certificate
    /// if that changed. The config itself is not rewritten.
    fn again(&mut self, account: &config::Account, fresh: bool) -> Result<()> {
        let (name, kind) = (account.name.clone(), net::auth(account)?);
        let (imap, smtp) = net::servers(account)?;
        let mut keep = Keep::new();
        let kept = |keyring: &dyn Keyring, endpoints: &Endpoints| {
            let tokens = (kind == Auth::Gmail).then(|| Tokens::new(endpoints.clone()));
            tuimex_engine::credential(&name, account.password_cmd.as_deref(), tokens.as_ref(), keyring)
        };
        let credential = match kind {
            Auth::Command => {
                self.say("Its password comes from the `password_cmd` in the config, which is yours to change there.");
                kept(self.keyring, &self.endpoints).map_err(|e| anyhow!(e.advice(&name)))?
            }
            _ if !fresh => kept(self.keyring, &self.endpoints).map_err(|e| anyhow!(e.advice(&name)))?,
            Auth::Gmail => self.google(&name, &account.address, &mut keep)?,
            _ => self.password(&name, &mut keep)?,
        };
        let (pin, changed) = self.trust(&name, &imap, smtp.as_ref())?;
        self.try_out(&name, &imap, smtp.as_ref(), &credential, &pin)?;
        self.keep(&keep)?;
        if let Some(certificate) = pin.filter(|_| changed) {
            pin::save(self.config_dir, &name, &certificate)?;
            self.say(&format!("The certificate is now the one trusted for {name}."));
        } else if !fresh {
            self.say(&format!("The certificate of {name}'s server has not changed; there was nothing to accept."));
        }
        if !keep.is_empty() {
            self.say("The new sign-in is in the keyring.");
        }
        Ok(())
    }
}

/// `tuimex setup`, on the terminal, against the network and the keyring.
/// Whether an account was set up; why not has been printed.
pub fn command(config: Option<&Path>) -> Result<bool> {
    let dirs = crate::paths::Dirs::from_env()?;
    let path = config::path(config, &dirs.config);
    let env = |key: &str| std::env::var_os(key).is_some();
    // A browser opened on a machine reached over SSH is on the wrong
    // screen, and a Linux session with no display has none to open.
    let headless = cfg!(target_os = "linux") && !env("DISPLAY") && !env("WAYLAND_DISPLAY");
    let mut system = crate::live::opener();
    let browser: Option<&mut dyn Opener> =
        if env("SSH_CONNECTION") || env("SSH_TTY") || headless { None } else { Some(&mut system) };
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let mut setup = Setup {
        prompt: &mut Terminal,
        keyring: &tuimex_engine::SystemKeyring,
        net: &net::Live,
        config_dir: &dirs.config,
        data: &dirs.data,
        cache: &dirs.cache,
        config_path: &path,
        endpoints: Endpoints::google(),
        browser,
        claude: crate::doctor::on_path("claude", &path_var),
    };
    let done = setup.run();
    if let Err(error) = &done {
        eprintln!("E setup: {error:#}");
    }
    Ok(done.is_ok())
}

#[cfg(test)]
pub(crate) mod tests;

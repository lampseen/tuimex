//! The real server the spike and the exit tests talk to, named by the
//! environment (`tests/spike.rs` lists the variables), and a raw IMAP
//! session on it to play the other client.

// Each test binary uses its own part of this.
#![allow(dead_code)]

use std::env;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use imap::extensions::idle::SetReadTimeout;
use imap::{Client, Session};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use tuimex_engine::{ImapAccount, Keyring, Secret};

pub struct Tls(StreamOwned<ClientConnection, TcpStream>);

impl Read for Tls {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Tls {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl SetReadTimeout for Tls {
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> imap::Result<()> {
        self.0.sock.set_read_timeout(timeout).map_err(imap::Error::Io)
    }
}

pub struct Server {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub ca: String,
    /// Where the server takes mail to send, with TLS from the first byte
    /// and then on a plain connection, when it does at all.
    pub smtp: Option<(u16, u16)>,
}

fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is not set, and TUIMEX_SERVER_HOST is"))
}

/// The server the environment names, or None with a line saying the test
/// did not run: a skip must never read as a pass.
pub fn server(test: &str) -> Option<Server> {
    let Ok(host) = env::var("TUIMEX_SERVER_HOST") else {
        println!("{test}: SKIPPED, no server: set TUIMEX_SERVER_HOST and the rest (see tests/spike.rs)");
        return None;
    };
    let port = var("TUIMEX_SERVER_PORT").parse().expect("TUIMEX_SERVER_PORT is a port");
    let (user, password, ca) = (var("TUIMEX_SERVER_USER"), var("TUIMEX_SERVER_PASSWORD"), var("TUIMEX_SERVER_CA"));
    let port_in = |name: &str| env::var(name).ok().map(|port| port.parse().expect("a port"));
    let smtp = port_in("TUIMEX_SERVER_SMTPS_PORT").zip(port_in("TUIMEX_SERVER_SMTP_PORT"));
    Some(Server { host, port, user, password, ca, smtp })
}

/// No keyring: the user's is never opened by a test.
struct NoKeyring;

impl Keyring for NoKeyring {
    fn password(&self, _account: &str) -> Result<Option<Secret>, String> {
        Ok(None)
    }

    fn set(&self, _entry: &str, _secret: &Secret) -> Result<(), String> {
        Err("there is no keyring".to_string())
    }
}

impl Server {
    /// A TLS connection that trusts the one certificate file and nothing
    /// else, with the greeting read.
    pub fn client(&self) -> Client<Tls> {
        let mut roots = RootCertStore::empty();
        for cert in CertificateDer::pem_file_iter(&self.ca).expect("TUIMEX_SERVER_CA is a PEM file") {
            roots.add(cert.unwrap()).unwrap();
        }
        let config = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = ServerName::try_from(self.host.clone()).unwrap();
        let tls = ClientConnection::new(Arc::new(config), name).unwrap();
        let tcp = TcpStream::connect((self.host.as_str(), self.port)).unwrap();
        let mut client = Client::new(Tls(StreamOwned::new(tls, tcp)));
        client.read_greeting().unwrap();
        client
    }

    pub fn session(&self) -> Session<Tls> {
        self.client().login(&self.user, &self.password).map_err(|(e, _)| e).unwrap()
    }

    /// The account as the engine reaches it, signing in as `tuimex sync`
    /// would with a `password_cmd`. `config` is where its pin is looked for.
    pub fn account(&self, config: &Path) -> ImapAccount {
        let at =
            |port, starttls| tuimex_engine::Server { host: self.host.clone(), port, starttls, user: self.user.clone() };
        let server = at(self.port, false);
        let password_cmd = ["printf", "%s", &self.password].map(String::from).to_vec();
        ImapAccount {
            name: "work".to_string(),
            server,
            smtp: self.smtp.map(|(implicit, _)| at(implicit, false)),
            password_cmd: Some(password_cmd),
            gmail: None,
            keyring: Box::new(NoKeyring),
            config_dir: config.to_path_buf(),
        }
    }
}

pub fn message(subject: &str) -> Vec<u8> {
    let head = "From: Spike <spike@example.com>\r\nTo: Spike <spike@example.com>\r\nMessage-ID: <";
    format!("{head}{subject}@example.com>\r\nSubject: {subject}\r\n\r\nsynthetic\r\n").into_bytes()
}

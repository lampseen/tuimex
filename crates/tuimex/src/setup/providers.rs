//! The mail providers `tuimex setup` knows the servers of (PLAN §6.7).
//! Anything else is hosts and ports typed by hand.

pub struct Provider {
    pub name: &'static str,
    /// The account name setup offers.
    pub short: &'static str,
    /// The address domains that are this provider's.
    pub domains: &'static [&'static str],
    pub imap: (&'static str, u16),
    pub smtp: (&'static str, u16),
    /// Whether Google's sign-in is offered (ADR-019).
    pub oauth: bool,
    /// Whether IMAP is signed in to with the part of the address before
    /// the `@`, as iCloud has it, where SMTP takes the whole address.
    pub imap_user_is_local: bool,
    /// The first way to sign in, as the question offers it: which
    /// password this provider takes from a mail program, and where a
    /// person gets it.
    pub password: &'static str,
    /// What a person has to have in front of them before the questions
    /// about its servers can be answered.
    pub first: Option<&'static str>,
}

/// The first way to sign in to a server typed by hand.
pub const ANY_PASSWORD: &str = "The password for this mailbox, typed now. Many providers refuse the password\n     \
                                you use on their website and want an \"app password\" made in their settings.";

pub const PROVIDERS: [Provider; 4] = [
    Provider {
        name: "Gmail",
        short: "gmail",
        domains: &["gmail.com", "googlemail.com"],
        imap: ("imap.gmail.com", 993),
        smtp: ("smtp.gmail.com", 465),
        oauth: true,
        imap_user_is_local: false,
        password: "An app password, typed now (the quickest): a separate 16-character password\n     \
                   Google makes for one program. Make one at\n     \
                   https://myaccount.google.com/apppasswords (the account needs 2-Step\n     \
                   Verification turned on first). It is not your Google password.",
        first: None,
    },
    Provider {
        name: "Fastmail",
        short: "fastmail",
        domains: &["fastmail.com", "fastmail.fm"],
        imap: ("imap.fastmail.com", 993),
        smtp: ("smtp.fastmail.com", 465),
        oauth: false,
        imap_user_is_local: false,
        password: "An app password, typed now: a separate password Fastmail makes for one\n     \
                   program, in its settings under Privacy & Security, Connected apps & API\n     \
                   tokens. It is not your Fastmail password.",
        first: None,
    },
    Provider {
        name: "iCloud",
        short: "icloud",
        domains: &["icloud.com", "me.com", "mac.com"],
        imap: ("imap.mail.me.com", 993),
        smtp: ("smtp.mail.me.com", 587),
        oauth: false,
        imap_user_is_local: true,
        password: "An app-specific password, typed now: a separate password Apple makes for\n     \
                   one program, at https://account.apple.com under Sign-In and Security.\n     \
                   It is not your Apple Account password.",
        first: None,
    },
    Provider {
        name: "Proton Bridge",
        short: "proton",
        domains: &["proton.me", "protonmail.com", "pm.me"],
        imap: ("127.0.0.1", 1143),
        smtp: ("127.0.0.1", 1025),
        oauth: false,
        imap_user_is_local: false,
        password: "The password Proton Bridge shows, typed now. It is Bridge's own, not your\n     \
                   Proton password.",
        first: Some(
            "Proton Bridge must be running on this computer: tuimex reaches Proton's mail through it.\n\
             Its window shows the ports, the username and the password to use here, under the\n\
             account's mailbox details.",
        ),
    },
];

/// Where a calendar's private address is found (ADR-056), told only for
/// the providers where that is known for certain, and what it is.
pub const GOOGLE_FEED: &str = "Google Calendar's private address is in its settings on the web: Settings, then the \
calendar, then \"Secret address in iCal format\".\nAnyone with the address can read the calendar, so tuimex keeps it \
in the keyring and in no file.";
pub const PROTON_FEED: &str = "Proton Calendar's private address is made in its settings on the web: Settings, then \
the calendar, then \"Share via link\", with full view.\nAnyone with the address can read the calendar, so tuimex \
keeps it in the keyring and in no file.";
pub const ANY_FEED: &str = "Many providers give a calendar a private address in iCalendar format, often ending \
.ics, in the calendar's settings.\nAnyone with the address can read the calendar, so tuimex keeps it in the keyring \
and in no file.";

/// The provider an address's domain is known to be, as a place in the table.
pub fn guess(address: &str) -> Option<usize> {
    let domain = address.rsplit('@').next().unwrap_or_default().to_ascii_lowercase();
    PROVIDERS.iter().position(|provider| provider.domains.contains(&domain.as_str()))
}

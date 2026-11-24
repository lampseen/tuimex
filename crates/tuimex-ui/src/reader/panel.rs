//! The two lists the reader opens over its body: the design's
//! `linkEntries` and `fileEntries`, and what is known of a link before it
//! opens (ADR-018).

use tuimex_core::{Attachment, Body, Link, Url, Warning, assess};

use super::size_text;
use crate::glyphs::NOTICE;
use crate::palette::Tok;
use crate::span::{Line, gap, run};

/// A link as its row and its confirm word it.
pub(crate) struct Look {
    /// The target, less its tracking parameters.
    pub url: String,
    /// The real host. A link that will not open shows its scheme first,
    /// which is why.
    pub host: String,
    pub scheme: String,
    /// Why it is confirmed before it opens, when it is.
    pub warning: Option<String>,
    /// Shown and never opened: neither the web nor mail (ADR-018).
    pub blocked: bool,
    pub mailto: bool,
    note: Option<&'static str>,
}

pub(crate) fn look(link: &Link, body: &Body) -> Look {
    let found = assess(&link.url, &link.shown);
    let target = Url::parse(&link.url);
    // The host the text claims, as a reader would say it.
    let named = link.shown.split_whitespace().map(|word| Url::parse(word).host).find(|host| !host.is_empty());
    let warning = found.warnings.first().map(|warning| match warning {
        Warning::TextNamesAnotherHost => format!("text says {}", named.as_deref().unwrap_or(&link.shown)),
        Warning::CarriesCredentials => "address carries a sign-in name".to_string(),
        Warning::BareIpAddress => "host is a bare IP address".to_string(),
        Warning::MixedScripts => "host mixes alphabets".to_string(),
    });
    // A tracker is listed by its service's name, or by its host when
    // nobody has a name for it.
    let tracks = |tracker: &String| {
        let name = tracker.to_lowercase().replace(' ', "");
        found.host == name || found.host.split('.').any(|label| label == name)
    };
    let note = if body.trackers.iter().any(tracks) {
        Some("tracking service")
    } else if found.url != target.text {
        Some("tracking removed")
    } else {
        None
    };
    let host = match (found.blocked, target.scheme.is_empty()) {
        (true, true) => "(no host)".to_string(),
        (true, false) => format!("{}:{}", target.scheme, found.host),
        (false, _) => found.host,
    };
    let mailto = target.scheme == "mailto";
    Look { url: found.url, host, scheme: target.scheme, warning, blocked: found.blocked, mailto, note }
}

/// Each link by number, with the host it really opens.
pub(crate) fn links(body: &Body) -> Vec<Line> {
    let looks: Vec<Look> = body.links.iter().map(|link| look(link, body)).collect();
    let host_width = looks.iter().map(|l| l.host.chars().count()).max().unwrap_or(0) + 3;
    let entry = |(link, look): (&Link, &Look)| {
        let mut row = vec![
            run(format!("{:>2}  ", link.n), Tok::Link),
            run(format!("{:<host_width$}", look.host), Tok::Fg),
            run(link.shown.as_str(), Tok::Dim),
        ];
        let warning = if look.blocked { Some("will not open") } else { look.warning.as_deref() };
        match (warning, look.note) {
            (Some(warning), _) => row.extend([gap(3), run(NOTICE, Tok::Warn), run(format!(" {warning}"), Tok::Fg)]),
            (None, Some(note)) => row.extend([gap(3), run(note, Tok::Dim)]),
            (None, None) => {}
        }
        row
    };
    body.links.iter().zip(&looks).map(entry).collect()
}

/// Each file by number, with its size and type. `shown` are the pictures
/// the body draws.
pub(crate) fn files(files: &[Attachment], shown: &[usize]) -> Vec<Line> {
    let name_width = files.iter().map(|a| a.name.chars().count()).max().unwrap_or(0) + 3;
    let entry = |(i, a): (usize, &Attachment)| {
        let above = if shown.contains(&a.index) { "   shown above" } else { "" };
        vec![
            run(format!("{:>2}  ", i + 1), Tok::Dim),
            run(format!("{:<name_width$}", a.name), Tok::Fg),
            run(format!("{:>7}   {}{above}", size_text(a.size), a.mime), Tok::Dim),
        ]
    };
    files.iter().enumerate().map(entry).collect()
}

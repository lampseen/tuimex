//! What a view asks of one event beyond where it falls: how the reader
//! stands to it (ADR-057) and which of its links is the meeting's
//! (ADR-054).

use super::{Event, Said};
use crate::{Link, Url};

#[cfg(test)]
mod tests;

/// How the reader stands to an event they were asked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Asked, and nothing said yet.
    Needs,
    Accepted,
    Tentative,
    Declined,
}

/// What the reader has said to an event, or `None` for one nobody asked
/// them to. `mine` are the addresses of the account the event came to and
/// of no other: "you" among the guests is that account's identity, so an
/// invitation one account received is not answered by another's (ADR-057).
pub fn standing(event: &Event, mine: &[&str]) -> Option<Standing> {
    let me = |guest: &&super::Guest| mine.iter().any(|address| address.eq_ignore_ascii_case(&guest.who.email));
    event.guests.iter().find(me).map(|guest| match guest.said {
        Said::Nothing => Standing::Needs,
        Said::Accepted => Standing::Accepted,
        Said::Tentative => Standing::Tentative,
        Said::Declined => Standing::Declined,
    })
}

/// The addresses written out in `text`, in order: a word with a scheme
/// and `//` after it, or a `mailto:`. A calendar's text has no anchors, so
/// an address is only ever the word itself.
fn written(text: &str) -> impl Iterator<Item = &str> {
    fn bare(word: &str) -> &str {
        word.trim_start_matches(['<', '(', '[', '"', '\'']).trim_end_matches(['>', ')', ']', '"', '\'', '.', ',', ';'])
    }
    text.split_whitespace().map(bare).filter(|word| {
        let url = Url::parse(word);
        !url.scheme.is_empty() && (word.contains("://") || url.scheme == "mailto")
    })
}

/// An event's links in the order they are offered (ADR-054): its
/// conference property and its `URL`, which `Event::links` holds in that
/// order, then an address in its location, then those in its notes, each
/// once. The first is the meeting's.
///
/// `shown` is what `assess` is to hold a link against (ADR-018). An
/// address found in the event's text is its own text. One that came from a
/// property has no text, and what the event says of where it is held is
/// its location, so the location is the text of the meeting's link: an
/// event that says `bank.example.com` and joins somewhere else is the
/// link whose text names another host.
pub fn links_of(event: &Event) -> Vec<Link> {
    let location = event.location.as_deref().unwrap_or("");
    let mut links: Vec<Link> = Vec::new();
    let mut offer = |url: &str, shown: &str| {
        if !url.trim().is_empty() && !links.iter().any(|link| link.url == url) {
            links.push(Link { n: links.len() + 1, url: url.to_string(), shown: shown.to_string() });
        }
    };
    for (at, url) in event.links.iter().enumerate() {
        offer(url, if at == 0 { location } else { "" });
    }
    let notes = event.description.as_deref().unwrap_or("");
    for url in written(location).chain(written(notes)) {
        offer(url, url);
    }
    links
}

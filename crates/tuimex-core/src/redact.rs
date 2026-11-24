//! What is taken out of text before `:agent` hands it to a command, and put
//! back in what the command returns (ADR-031). By pattern, so it misses
//! whatever has none; `replaced` is what the status line says was caught.

#[cfg(test)]
mod tests;

/// Whether link addresses leave. `reply` and `tidy` withhold them;
/// `:agent link` is asked about one, and so sees them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Links {
    Withheld,
    Kept,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Link,
    Address,
    /// Which of `names`, and which form of it: 0 the whole name, then its
    /// words from 1.
    Person(usize, usize),
    Number,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted {
    /// The text with a placeholder wherever something was taken out.
    pub text: String,
    /// Each placeholder, its kind and what it stands for.
    map: Vec<(String, Kind, String)>,
    /// How many of `names` were found, in whatever forms.
    people: usize,
}

impl Redacted {
    /// `text` with every placeholder handed out here replaced by what it
    /// stood for. Anything else in brackets is the agent's own text and
    /// stays as written.
    pub fn restore(&self, text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(open) = rest.find('[') {
            let known = self.map.iter().find(|(placeholder, ..)| rest[open..].starts_with(placeholder.as_str()));
            let (written, taken) = known.map_or(("[", 1), |(placeholder, _, was)| (was.as_str(), placeholder.len()));
            out += &rest[..open];
            out += written;
            rest = &rest[open + taken..];
        }
        out + rest
    }

    /// What was replaced, by kind: `3 people, 4 addresses`. Empty when
    /// nothing was.
    pub fn replaced(&self) -> String {
        let of = |kind: Kind| self.map.iter().filter(|(_, k, _)| *k == kind).count();
        let counts = [
            (self.people, "person", "people"),
            (of(Kind::Address), "address", "addresses"),
            (of(Kind::Number), "number", "numbers"),
            (of(Kind::Link), "link", "links"),
        ];
        let word = |(n, one, many): &(usize, &str, &str)| format!("{n} {}", if *n == 1 { one } else { many });
        counts.iter().filter(|(n, ..)| *n > 0).map(word).collect::<Vec<_>>().join(", ")
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// Where a link starting at the head of `at` ends: before a space or a
/// bracket, and before the punctuation of the sentence it sits in.
fn link_end(at: &str) -> usize {
    let end = at.find(|c: char| c.is_whitespace() || "<>\"'".contains(c)).unwrap_or(at.len());
    at[..end].trim_end_matches(|c: char| ".,;:!?)]".contains(c)).len()
}

/// The digits and single separators of a phone or account number at the
/// head of `at`, when there are enough digits to be one: seven with
/// separators, six in an unbroken run. A date written `2026-11-23` is not.
fn number_end(at: &str) -> Option<usize> {
    let bytes = at.as_bytes();
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    let iso = |i: usize| (0..10).all(|k| if k == 4 || k == 7 { bytes.get(i + k) == Some(&b'-') } else { digit(i + k) });
    if iso(0) && !digit(10) {
        return None;
    }
    let (mut end, mut digits, mut run, mut longest) = (0, 0, 0, 0);
    let mut i = usize::from(bytes.first() == Some(&b'+'));
    while i < bytes.len() {
        if digit(i) {
            (digits, run, end) = (digits + 1, run + 1, i + 1);
            longest = longest.max(run);
        } else if b" -.()/".contains(&bytes[i]) && (digit(i + 1) || (bytes[i] == b')' && digit(i + 2))) {
            run = 0;
        } else {
            break;
        }
        i += 1;
    }
    (digits >= 7 || longest >= 6).then_some(end)
}

/// Every span of `text` to take out, in the order found; an earlier kind
/// keeps what a later one would also match.
fn spans(text: &str, names: &[&str], links: Links) -> Vec<(usize, usize, Kind)> {
    let mut spans: Vec<(usize, usize, Kind)> = Vec::new();
    let free = |spans: &[(usize, usize, Kind)], from: usize, to: usize| spans.iter().all(|s| to <= s.0 || s.1 <= from);
    let lower = text.to_ascii_lowercase();
    if links == Links::Withheld {
        for scheme in ["https://", "http://", "mailto:"] {
            for (at, _) in lower.match_indices(scheme) {
                let end = at + link_end(&text[at..]);
                if free(&spans, at, end) {
                    spans.push((at, end, Kind::Link));
                }
            }
        }
    }
    let local = |c: char| c.is_ascii_alphanumeric() || "._%+-".contains(c);
    let domain = |c: char| c.is_ascii_alphanumeric() || ".-".contains(c);
    for (at, _) in text.match_indices('@') {
        let from = at - text[..at].chars().rev().take_while(|c| local(*c)).count();
        let after: String = text[at + 1..].chars().take_while(|c| domain(*c)).collect();
        let to = at + 1 + after.trim_end_matches(['.', '-']).len();
        if from < at && text[at + 1..to].contains('.') && free(&spans, from, to) {
            spans.push((from, to, Kind::Address));
        }
    }
    // Whole names before their words, so `Ada Lovelace` is one placeholder
    // and a lone `Ada` another form of the same person.
    let whole = names.iter().enumerate().map(|(person, name)| (person, 0, name.trim()));
    let words = names.iter().enumerate().flat_map(|(person, name)| {
        let parts = name.split_whitespace().map(|word| word.trim_matches(|c| !is_word(c)));
        parts.enumerate().map(move |(form, word)| (person, form + 1, word))
    });
    let proper = |word: &str| word.chars().count() >= 2 && word.starts_with(char::is_uppercase);
    for (person, form, name) in whole.chain(words.filter(|(.., word)| proper(word))) {
        for (at, _) in text.match_indices(name).filter(|_| !name.is_empty()) {
            let end = at + name.len();
            let bounded = !text[..at].ends_with(is_word) && !text[end..].starts_with(is_word);
            if bounded && free(&spans, at, end) {
                spans.push((at, end, Kind::Person(person, form)));
            }
        }
    }
    let mut at = 0;
    while at < text.len() {
        let starts = text[at..].starts_with(|c: char| c.is_ascii_digit() || c == '+') && !text[..at].ends_with(is_word);
        match number_end(&text[at..]).filter(|_| starts) {
            Some(len) if free(&spans, at, at + len) => {
                spans.push((at, at + len, Kind::Number));
                at += len;
            }
            _ => at += text[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    spans.sort_by_key(|span| span.0);
    spans
}

/// `text` with addresses, phone numbers, long digit runs and `names` (the
/// names in the messages' headers, and each word of one standing alone)
/// replaced by placeholders such as `[person-1]` and `[address-2]`,
/// numbered within their kind in the order met.
pub fn redact(text: &str, names: &[&str], links: Links) -> Redacted {
    let mut out = Redacted { text: String::new(), map: Vec::new(), people: 0 };
    let mut people: Vec<usize> = Vec::new();
    let mut at = 0;
    for (from, to, kind) in spans(text, names, links) {
        let was = &text[from..to];
        let seen = |m: &&(String, Kind, String)| match (kind, m.1) {
            (Kind::Person(..), _) => m.1 == kind,
            _ => m.1 == kind && m.2 == was,
        };
        let placeholder = match out.map.iter().find(seen) {
            Some((placeholder, ..)) => placeholder.clone(),
            None => {
                let nth = |kind: Kind| out.map.iter().filter(|m| m.1 == kind).count() + 1;
                let placeholder = match kind {
                    Kind::Link => format!("[link-{}]", nth(kind)),
                    Kind::Address => format!("[address-{}]", nth(kind)),
                    Kind::Number => format!("[number-{}]", nth(kind)),
                    Kind::Person(person, form) => {
                        if !people.contains(&person) {
                            people.push(person);
                        }
                        let n = people.iter().position(|p| *p == person).map_or(0, |i| i + 1);
                        // Letters run out after 26 words of one name; the
                        // form's number still tells them apart.
                        let letter =
                            u8::try_from(form).ok().filter(|f| (1..=26).contains(f)).map(|f| (b'a' + f - 1) as char);
                        match (form, letter) {
                            (0, _) => format!("[person-{n}]"),
                            (_, Some(letter)) => format!("[person-{n}{letter}]"),
                            (form, None) => format!("[person-{n}w{form}]"),
                        }
                    }
                };
                out.map.push((placeholder.clone(), kind, was.to_string()));
                placeholder
            }
        };
        out.text += &text[at..from];
        out.text += &placeholder;
        at = to;
    }
    out.text += &text[at..];
    out.people = people.len();
    out
}

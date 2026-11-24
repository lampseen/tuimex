//! Spans and the primitives every screen is built from, ported from
//! `design/tui.js` (`run`, `fit`, `leftRight`, `fill`, `m`, `wrap`,
//! `highlight`) and kept behaviourally identical, because the golden tests
//! compare their output byte for byte (ADR-002).
//!
//! The design counts code points, and its fixture is ASCII. Here a width is
//! what a terminal gives the text, so a wide character in a real subject
//! cannot push a column out of line.

use unicode_width::UnicodeWidthChar;

use crate::glyphs::ELLIPSIS;
use crate::palette::Tok;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub fg: Tok,
    pub bg: Option<Tok>,
    pub bold: bool,
    pub underline: bool,
}

pub type Line = Vec<Span>;

impl Span {
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn on(mut self, bg: Tok) -> Self {
        self.bg = Some(bg);
        self
    }
}

pub fn run(text: impl Into<String>, fg: Tok) -> Span {
    Span { text: text.into(), fg, bg: None, bold: false, underline: false }
}

/// Plain spaces with no background of their own, so `fill` reaches them.
pub fn gap(n: usize) -> Span {
    run(" ".repeat(n), Tok::Fg)
}

/// A control character has no cell: `paint` drops it.
fn cell_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

pub fn cells(line: &[Span]) -> usize {
    line.iter().flat_map(|s| s.text.chars()).map(cell_width).sum()
}

/// Pads or cuts to exactly `width` cells. A cut line ends in an ellipsis
/// styled like the text it replaces.
pub fn fit(line: &[Span], width: usize) -> Line {
    let n = cells(line);
    if n <= width {
        let mut out = line.to_vec();
        if n < width {
            out.push(gap(width - n));
        }
        return out;
    }
    if width == 0 {
        return Vec::new();
    }
    let keep = width - 1;
    let mut out: Line = Vec::new();
    let mut used = 0;
    for span in line {
        let len = cells(std::slice::from_ref(span));
        if used + len <= keep {
            out.push(span.clone());
            used += len;
            continue;
        }
        let mut text = String::new();
        for c in span.text.chars() {
            if used + cell_width(c) > keep {
                break;
            }
            text.push(c);
            used += cell_width(c);
        }
        if !text.is_empty() {
            out.push(Span { text, ..span.clone() });
        }
        break;
    }
    let last = out.last().cloned().unwrap_or_else(|| run("", Tok::Fg));
    out.push(Span { text: ELLIPSIS.to_string(), underline: false, ..last });
    // A wide character that would have straddled the cut leaves a cell over.
    if used + 1 < width {
        out.push(gap(width - used - 1));
    }
    out
}

/// Cuts the middle and keeps both ends, with an ellipsis between, when
/// `line` is wider than `width`: a question that is too wide still ends on
/// the last part of the host it names, which says whose it is.
pub fn fit_middle(line: &[Span], width: usize) -> Line {
    if cells(line) <= width || width == 0 {
        return line.to_vec();
    }
    let all: Vec<(char, &Span)> = line.iter().flat_map(|s| s.text.chars().map(move |c| (c, s))).collect();
    let head = (width - 1) / 2;
    let tail = all.len().saturating_sub(width - 1 - head);
    let piece = |(c, span): &(char, &Span)| Span { text: c.to_string(), ..(*span).clone() };
    let mut out: Line = all[..head.min(all.len())].iter().map(piece).collect();
    out.extend(all.get(head).map(|(_, span)| Span { text: ELLIPSIS.to_string(), ..(*span).clone() }));
    out.extend(all[tail.max(head)..].iter().map(piece));
    out
}

/// `left` from the left edge and `right` against the right one; `left` is
/// cut when they would meet.
pub fn left_right(left: &[Span], right: &[Span], width: usize) -> Line {
    let mut out = fit(left, width.saturating_sub(cells(right)));
    out.extend_from_slice(right);
    out
}

/// Gives every span without a background this one.
pub fn fill(line: Line, bg: Tok) -> Line {
    line.into_iter().map(|s| if s.bg.is_none() { s.on(bg) } else { s }).collect()
}

/// The design's markup: `{dim|text}`, `{fg.b|text}`, `{fg.u|text}`; anything
/// else is plain text. For wording written here. Text that came from a
/// message or a server is given its span directly, so a brace in it stays a
/// brace.
pub fn markup(src: &str) -> Line {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some((open, tag, body, end)) = next_tag(rest) {
        if open > 0 {
            out.push(run(&rest[..open], Tok::Fg));
        }
        let mut flags = tag.split('.');
        let mut span = run(body, flags.next().and_then(Tok::from_name).unwrap_or(Tok::Fg));
        for flag in flags {
            span.bold |= flag == "b";
            span.underline |= flag == "u";
        }
        out.push(span);
        rest = &rest[end..];
    }
    if !rest.is_empty() {
        out.push(run(rest, Tok::Fg));
    }
    out
}

/// The next `{tag|body}` in `src`, as the design's
/// `/\{([a-z0-9.]+)\|([^}]*)\}/` finds it: where it opens, its two parts,
/// and where it ends.
fn next_tag(src: &str) -> Option<(usize, &str, &str, usize)> {
    let mut from = 0;
    while let Some(open) = src[from..].find('{').map(|i| from + i) {
        let after = &src[open + 1..];
        let tag = after.bytes().take_while(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'.').count();
        if tag > 0 && after.as_bytes().get(tag) == Some(&b'|') {
            let body = open + 1 + tag + 1;
            if let Some(close) = src[body..].find('}') {
                return Some((open, &after[..tag], &src[body..body + close], body + close + 1));
            }
        }
        from = open + 1;
    }
    None
}

/// Greedy word wrap. A word is never split, so one longer than `width` has
/// a line to itself and is cut where the line is fitted.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let len = |s: &str| s.chars().map(cell_width).sum::<usize>();
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if line.is_empty() {
            line = word.to_string();
        } else if len(&line) + 1 + len(word) <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// `text` in `base`'s style, with every occurrence of `needle`, whatever
/// its case, bright, bold and underlined.
pub fn highlight(text: &str, needle: Option<&str>, base: Span) -> Line {
    let piece = |chars: &[char]| Span { text: chars.iter().collect(), ..base.clone() };
    // One character to one character, so a position in the folded text is
    // the same position in the text.
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let chars: Vec<char> = text.chars().collect();
    let needle: Vec<char> = needle.unwrap_or("").chars().map(fold).collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return vec![piece(&chars)];
    }
    let folded: Vec<char> = chars.iter().copied().map(fold).collect();
    let mut out = Vec::new();
    let (mut at, mut i) = (0, 0);
    while i + needle.len() <= chars.len() {
        if folded[i..i + needle.len()] != needle[..] {
            i += 1;
            continue;
        }
        if i > at {
            out.push(piece(&chars[at..i]));
        }
        let hit: String = chars[i..i + needle.len()].iter().collect();
        out.push(Span { underline: true, ..run(hit, Tok::Fg).bold() });
        i += needle.len();
        at = i;
    }
    if at < chars.len() {
        out.push(piece(&chars[at..]));
    }
    out
}

#[cfg(test)]
mod tests;

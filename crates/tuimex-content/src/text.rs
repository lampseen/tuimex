//! Text a message supplied, made safe to put on a terminal.

/// Multi-line text without anything that can drive a terminal or reorder
/// what is displayed. An escape sequence in a message is otherwise a way to
/// retitle the window or write to the clipboard, and a bidirectional
/// override makes a person see different words from the ones an agent reads
/// (ADR-017).
pub(crate) fn clean(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            matches!(c, '\n' | '\t')
                || !(c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
        })
        .collect()
}

/// `clean`, on one line: for a subject, a name, a link's text.
pub(crate) fn line(s: &str) -> String {
    clean(s).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Appends `item` unless it is already there. The lists this builds are the
/// few hosts or names one message mentions, read in order of appearance.
pub(crate) fn push_unique(list: &mut Vec<String>, item: &str) {
    if !list.iter().any(|have| have == item) {
        list.push(item.to_string());
    }
}

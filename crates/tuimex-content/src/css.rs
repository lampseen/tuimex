//! As much CSS as it takes to know what a browser would hide and what it
//! would fetch. Every reading here errs towards hiding more and reporting
//! more hosts: the other direction is the one a message profits from.

use std::collections::HashMap;

#[cfg(test)]
mod tests;

/// Name, value, `!important`; lower case, escapes decoded.
pub(crate) type Decl = (String, String, bool);

/// Comments removed, escapes decoded, lower case. Decoding first is what
/// makes `d\69splay:none` and `url(\68ttps://…)` read as a browser reads
/// them.
fn flatten(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
        } else if c == '\\' {
            let after = &rest[1..];
            let hex = after.bytes().take(6).take_while(u8::is_ascii_hexdigit).count();
            if hex > 0 {
                // The radix is checked by the count above; a value that is
                // no character becomes the replacement, as in a browser.
                let code = u32::from_str_radix(&after[..hex], 16).unwrap_or(0xFFFD);
                out.extend(char::from_u32(code).unwrap_or('\u{FFFD}').to_lowercase());
                let after = &after[hex..];
                rest = after.strip_prefix([' ', '\t', '\n']).unwrap_or(after);
            } else {
                let mut chars = after.chars();
                out.extend(chars.next().into_iter().flat_map(char::to_lowercase));
                rest = chars.as_str();
            }
        } else {
            out.extend(c.to_lowercase());
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// The declarations of a `style` attribute.
pub(crate) fn declarations(style: &str) -> Vec<Decl> {
    block(&flatten(style))
}

fn block(flat: &str) -> Vec<Decl> {
    flat.split(';')
        .filter_map(|decl| {
            let (name, value) = decl.split_once(':')?;
            let (value, important) = match value.rsplit_once('!') {
                Some((v, flag)) if flag.trim() == "important" => (v, true),
                _ => (value, false),
            };
            Some((name.trim().to_string(), value.trim().to_string(), important))
        })
        .collect()
}

/// The number a value starts with: `0` of `0px`, `1` of `1`. A value that
/// starts with none has none, which is all a failed parse means here.
pub(crate) fn number(value: &str) -> Option<f32> {
    let end = value.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))).unwrap_or(value.len());
    value[..end].parse().ok()
}

/// Whether an element with these declarations, in cascade order, is one a
/// browser would not display (ADR-009). The last declaration of a property
/// wins unless an earlier one is `!important`.
pub(crate) fn hides<'a>(decls: impl Iterator<Item = &'a Decl>) -> bool {
    let mut last: HashMap<&str, (&str, bool)> = HashMap::new();
    for (name, value, important) in decls {
        if *important || !last.get(name.as_str()).is_some_and(|(_, held)| *held) {
            last.insert(name, (value, *important));
        }
    }
    let get = |name: &str| last.get(name).map(|(value, _)| *value);
    let zero = |name: &&str| get(name).and_then(number) == Some(0.0);
    get("display") == Some("none")
        || matches!(get("visibility"), Some("hidden" | "collapse"))
        || get("color") == Some("transparent")
        || zero(&"opacity")
        || zero(&"font-size")
        // No size alone hides nothing: the text overflows the box.
        || (["width", "height", "max-width", "max-height"].iter().any(zero)
            && ["overflow", "overflow-x", "overflow-y"].iter().any(|o| matches!(get(o), Some("hidden" | "clip"))))
}

/// Everything in `css` that could be the address of something to fetch: the
/// argument of each `url(…)`, and every quoted string, which is how
/// `@import` and `image-set` name theirs.
pub(crate) fn urls(css: &str) -> Vec<String> {
    let flat = flatten(css);
    let mut found = Vec::new();
    let mut rest = flat.as_str();
    while let Some((_, after)) = rest.split_once("url(") {
        let inside = after.split(')').next().unwrap_or(after);
        found.push(inside.trim().trim_matches(['"', '\'']).to_string());
        rest = after;
    }
    for quote in ['"', '\''] {
        found.extend(flat.split(quote).skip(1).step_by(2).map(str::to_string));
    }
    found
}

/// Whether a `style` attribute could make a browser fetch something.
pub(crate) fn fetches(style: &str) -> bool {
    let flat = flatten(style);
    ["url(", "image-set(", "image(", "src(", "@import"].iter().any(|f| flat.contains(f))
}

/// The properties `hides` reads; a rule that sets none of them is not kept.
const WATCHED: [&str; 12] = [
    "display",
    "visibility",
    "color",
    "opacity",
    "font-size",
    "width",
    "height",
    "max-width",
    "max-height",
    "overflow",
    "overflow-x",
    "overflow-y",
];

/// Every element is matched against every rule, so a message could buy
/// quadratic work with a long stylesheet. Rules past this many are ignored.
const MAX_RULES: usize = 512;

#[derive(Debug, Default, PartialEq)]
struct Selector {
    tag: String,
    id: String,
    classes: Vec<String>,
}

/// The rules of a message's `<style>` elements that can hide an element.
#[derive(Default)]
pub(crate) struct Sheet {
    rules: Vec<(Selector, Vec<Decl>)>,
}

impl Sheet {
    pub(crate) fn add(&mut self, css: &str) {
        let flat = flatten(css);
        let mut rest = flat.trim_start();
        while !rest.is_empty() && self.rules.len() < MAX_RULES {
            if rest.starts_with('@') {
                // A rule inside `@media` hides on some screens only, and the
                // reader keeps what any screen would show.
                rest = after_at_rule(rest).trim_start();
                continue;
            }
            let Some((prelude, after)) = rest.split_once('{') else { break };
            let (body, after) = after.split_once('}').unwrap_or((after, ""));
            let decls: Vec<Decl> = block(body).into_iter().filter(|d| WATCHED.contains(&d.0.as_str())).collect();
            if !decls.is_empty() {
                self.rules.extend(prelude.split(',').filter_map(selector).map(|s| (s, decls.clone())));
            }
            rest = after.trim_start();
        }
    }

    /// The declarations that apply to an element, in source order.
    /// Specificity is not weighed; `id` and `classes` are lower case.
    pub(crate) fn matching<'a>(
        &'a self,
        tag: &'a str,
        id: &'a str,
        classes: &'a [String],
    ) -> impl Iterator<Item = &'a Decl> {
        let applies = move |s: &Selector| {
            (s.tag.is_empty() || s.tag == tag)
                && (s.id.is_empty() || s.id == id)
                && s.classes.iter().all(|c| classes.contains(c))
        };
        self.rules.iter().filter(move |(s, _)| applies(s)).flat_map(|(_, decls)| decls)
    }
}

/// Past an at-rule: its `;` if it is a statement, its braces if a block.
fn after_at_rule(rest: &str) -> &str {
    let Some(open) = rest.find(['{', ';']) else { return "" };
    if rest[open..].starts_with(';') {
        return &rest[open + 1..];
    }
    let mut depth = 0usize;
    for (i, c) in rest[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth <= 1 => return &rest[open + i + 1..],
            '}' => depth -= 1,
            _ => {}
        }
    }
    ""
}

/// Whether `text` is letters, digits, space and the marks in `also`.
fn plain(text: &str, also: &str) -> bool {
    text.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || also.contains(c))
}

/// The rules of a message's stylesheet that the browser copy keeps
/// (ADR-009): those that only lay out and colour. A rule stays when its
/// selector is tags, classes and ids, and of it the declarations of
/// `properties` whose values are words, numbers and colours and name
/// nothing to fetch. `@media` blocks are kept on the same terms and every
/// other at-rule goes. Nothing here can end the `<style>` it is written
/// into, hold an escape, or style a pseudo-element.
pub(crate) fn layout(css: &str, properties: &[&str]) -> String {
    let mut uncommented = String::with_capacity(css.len());
    let mut rest = css;
    while let Some((before, after)) = rest.split_once("/*") {
        uncommented.push_str(before);
        rest = after.split_once("*/").map_or("", |(_, tail)| tail);
    }
    uncommented.push_str(rest);
    let (mut out, mut rules) = (String::new(), 0);
    keep(&uncommented, properties, true, &mut out, &mut rules);
    out
}

/// Whether every function in a value is one that works a colour or a
/// length out: anything else called here is not kept, whatever it is.
fn only_measures(value: &str) -> bool {
    const FUNCTIONS: [&str; 7] = ["rgb", "rgba", "hsl", "hsla", "calc", "min", "max"];
    value.split('(').rev().skip(1).all(|before| {
        let name = before.rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).next().unwrap_or("");
        FUNCTIONS.contains(&name.to_ascii_lowercase().as_str())
    })
}

fn keep(css: &str, properties: &[&str], top: bool, out: &mut String, rules: &mut usize) {
    let mut rest = css.trim_start();
    while !rest.is_empty() && *rules < MAX_RULES {
        if rest.starts_with('@') {
            let after = after_at_rule(rest);
            let rule = rest[..rest.len() - after.len()].trim_end();
            let media = rule.strip_prefix("@media").and_then(|r| r.strip_suffix('}')).and_then(|r| r.split_once('{'));
            if let (true, Some((condition, inside))) = (top, media)
                && plain(condition, "():-,.")
            {
                let mut kept = String::new();
                keep(inside, properties, false, &mut kept, rules);
                if !kept.is_empty() {
                    out.push_str(&format!("@media {}{{{kept}}}", condition.trim()));
                }
            }
            rest = after.trim_start();
            continue;
        }
        let Some((prelude, after)) = rest.split_once('{') else { break };
        let (body, after) = after.split_once('}').unwrap_or((after, ""));
        rest = after.trim_start();
        let declared = |decl: &str| {
            let (name, value) = decl.split_once(':')?;
            let name = name.trim().to_ascii_lowercase();
            let safe = properties.contains(&name.as_str()) && plain(value, ".#,%()-+!'\"/") && only_measures(value);
            safe.then(|| format!("{name}:{}", value.trim()))
        };
        let kept: Vec<String> = body.split(';').filter_map(declared).collect();
        if plain(prelude, ".#,>+~-_*") && !prelude.trim().is_empty() && !kept.is_empty() {
            out.push_str(&format!("{}{{{}}}", prelude.trim(), kept.join(";")));
            *rules += 1;
        }
    }
}

/// The last compound of a selector, which is the element it styles; the
/// ancestors before it are not checked, so `.a .b` matches every `.b`. A
/// compound with a pseudo-class or an attribute test is not understood and
/// matches nothing.
fn selector(text: &str) -> Option<Selector> {
    let last = text.rsplit(|c: char| c.is_whitespace() || matches!(c, '>' | '+' | '~')).find(|s| !s.is_empty())?;
    if last.contains([':', '[', '*']) {
        return None;
    }
    let cut = last.find(['.', '#']).unwrap_or(last.len());
    let mut selector = Selector { tag: last[..cut].to_string(), ..Selector::default() };
    let mut rest = &last[cut..];
    while let Some(marker) = rest.chars().next() {
        let body = &rest[1..];
        let end = body.find(['.', '#']).unwrap_or(body.len());
        match marker {
            '.' => selector.classes.push(body[..end].to_string()),
            _ => selector.id = body[..end].to_string(),
        }
        rest = &body[end..];
    }
    Some(selector)
}

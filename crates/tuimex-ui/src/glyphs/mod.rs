//! Glyph tiers: the design's `GLYPHS` table. A tier swaps characters only,
//! and every glyph is one cell in both, so no layout asks which is active.

pub const CURSOR: char = '▌';
pub const MARK: char = '▪';
pub const FLAG: char = '★';
pub const FOLDED: char = '▸';
pub const UNFOLDED: char = '▾';
pub const OK: char = '✓';
pub const BAD: char = '✗';
pub const NOTICE: char = '▲';
pub const AGENT: char = '◇';
pub const BAR: char = '│';
pub const RULE: char = '─';
pub const ELLIPSIS: char = '…';
pub const DOT: char = '·';
pub const SHADE: char = '░';
pub const FEW: char = '∙';
pub const MANY: char = '⋮';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Glyphs {
    #[default]
    Unicode,
    Ascii,
}

impl Glyphs {
    /// Every character drawn passes through here. Views always write the
    /// Unicode tier.
    pub fn map(self, c: char) -> char {
        match self {
            Glyphs::Unicode => c,
            Glyphs::Ascii => ascii(c),
        }
    }
}

fn ascii(c: char) -> char {
    match c {
        '▌' => '>',
        '▪' => '+',
        '★' => '!',
        '▸' => '>',
        '▾' => 'v',
        '✓' => '+',
        '✗' => 'x',
        '▲' => '^',
        '◇' => '%',
        '│' => '|',
        '─' => '-',
        '…' => '~',
        '·' => '-',
        '░' => ':',
        '∙' => '.',
        '⋮' => '#',
        c => c,
    }
}

/// Unicode where the locale says the terminal decodes UTF-8, by the first
/// of `LC_ALL`, `LC_CTYPE` and `LANG` that is set, as the C library reads
/// them. The Linux console decodes it and still has no font for most of the
/// table, so it gets ASCII.
pub fn detect(locale: [Option<&str>; 3], term: Option<&str>) -> Glyphs {
    let named = locale.into_iter().flatten().find(|v| !v.is_empty()).unwrap_or("").to_ascii_lowercase();
    if (named.contains("utf-8") || named.contains("utf8")) && term != Some("linux") {
        Glyphs::Unicode
    } else {
        Glyphs::Ascii
    }
}

#[cfg(test)]
mod tests;

//! The one palette tuimex ships (ADR-003): the design's tokens in three
//! tiers. The values are `design/tui.js`'s `PALETTE`, and a test reads that
//! table to say so.

use ratatui::style::Color;

use crate::layout;

/// A colour role. What each is for is the design's `PALETTE.roles`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Bg,
    Sel,
    Fg,
    Dim,
    Faint,
    Acc,
    Ok,
    Warn,
    Err,
    Agent,
    Link,
    Head,
}

pub type Rgb = (u8, u8, u8);

impl Tok {
    pub const ALL: [Tok; 12] = [
        Tok::Bg,
        Tok::Sel,
        Tok::Fg,
        Tok::Dim,
        Tok::Faint,
        Tok::Acc,
        Tok::Ok,
        Tok::Warn,
        Tok::Err,
        Tok::Agent,
        Tok::Link,
        Tok::Head,
    ];

    /// The token's name in the design, in markup and in a palette file.
    pub fn name(self) -> &'static str {
        match self {
            Tok::Bg => "bg",
            Tok::Sel => "sel",
            Tok::Fg => "fg",
            Tok::Dim => "dim",
            Tok::Faint => "faint",
            Tok::Acc => "acc",
            Tok::Ok => "ok",
            Tok::Warn => "warn",
            Tok::Err => "err",
            Tok::Agent => "agent",
            Tok::Link => "link",
            Tok::Head => "head",
        }
    }

    pub fn from_name(name: &str) -> Option<Tok> {
        Tok::ALL.into_iter().find(|t| t.name() == name)
    }

    pub fn truecolor(self) -> Rgb {
        let v: u32 = match self {
            Tok::Bg => 0x121316,
            Tok::Sel => 0x2d2f38,
            Tok::Fg => 0xe6e1d3,
            Tok::Dim => 0x989ba3,
            Tok::Faint => 0x666972,
            Tok::Acc => 0xf5aa4d,
            Tok::Ok => 0xb4d1af,
            Tok::Warn => 0xdccb7e,
            Tok::Err => 0xe8877c,
            Tok::Agent => 0x82bdbf,
            Tok::Link => 0x8cb2ee,
            Tok::Head => 0xe2a47b,
        };
        ((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    fn c256(self) -> u8 {
        match self {
            Tok::Bg => 233,
            Tok::Sel => 237,
            Tok::Fg => 253,
            Tok::Dim => 246,
            Tok::Faint => 241,
            Tok::Acc => 215,
            Tok::Ok => 151,
            Tok::Warn => 186,
            Tok::Err => 210,
            Tok::Agent => 109,
            Tok::Link => 111,
            Tok::Head => 180,
        }
    }

    /// `None` is the design's `none`: sixteen colours have no shade that is
    /// reliably a little lighter than the background, so the selected row
    /// has no fill and the cursor glyph says which row it is.
    fn c16(self) -> Option<Color> {
        match self {
            Tok::Bg => Some(Color::Black),
            Tok::Sel => None,
            Tok::Fg => Some(Color::Reset),
            Tok::Dim | Tok::Faint => Some(Color::DarkGray),
            Tok::Acc => Some(Color::Magenta),
            Tok::Ok => Some(Color::Green),
            Tok::Warn => Some(Color::Yellow),
            Tok::Err => Some(Color::Red),
            Tok::Agent => Some(Color::Cyan),
            Tok::Link => Some(Color::LightBlue),
            Tok::Head => Some(Color::LightYellow),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    True,
    C256,
    C16,
}

/// Resolves tokens to terminal colours for one tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub tier: Tier,
    /// A palette file's colours by token, in `Tok::ALL`'s order, in place
    /// of the built-in truecolor values (ADR-003). The other tiers cannot
    /// show an arbitrary colour and keep the built-in ones.
    pub custom: Option<[Rgb; 12]>,
}

impl Palette {
    pub fn new(tier: Tier) -> Self {
        Palette { tier, custom: None }
    }

    /// The accents accounts take in turn, and the two colours of a row's
    /// words, its numbers and bright words and then the rest, when the
    /// accounts wear `in_use`: the design's own with the built-in palette,
    /// and worked out from a loaded one's values.
    pub fn account_order(&self) -> Vec<Tok> {
        match self.loaded() {
            Some(colours) => account_order(&colours),
            None => layout::ACCOUNTS_BUILTIN.to_vec(),
        }
    }

    pub fn words(&self, in_use: &[Tok]) -> (Tok, Tok) {
        match self.loaded().map(|colours| words_colour(&colours, in_use)) {
            None => (layout::WORDS_BUILTIN, layout::WORDS_BUILTIN),
            Some(Some(chosen)) => (chosen, chosen),
            Some(None) => (Tok::Fg, Tok::Dim),
        }
    }

    /// A palette file's colours, in the one tier that draws with them.
    fn loaded(&self) -> Option<[Rgb; 12]> {
        self.custom.filter(|_| self.tier == Tier::True)
    }

    /// `None` leaves the terminal's own colour in place.
    pub fn color(&self, t: Tok) -> Option<Color> {
        match self.tier {
            Tier::True => {
                let own = self.custom.and_then(|c| Tok::ALL.iter().position(|x| *x == t).map(|i| c[i]));
                let (r, g, b) = own.unwrap_or_else(|| t.truecolor());
                Some(Color::Rgb(r, g, b))
            }
            Tier::C256 => Some(Color::Indexed(t.c256())),
            Tier::C16 => t.c16(),
        }
    }
}

/// `#rrggbb`, the spelling the design and a palette file use.
pub fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub fn parse_hex(text: &str) -> Option<Rgb> {
    let digits = text.strip_prefix('#').filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()))?;
    // Six hex digits always parse; the check above is what refuses the sign
    // `from_str_radix` would accept.
    let v = u32::from_str_radix(digits, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// A colour in linear light.
fn linear((r, g, b): Rgb) -> [f64; 3] {
    [r, g, b].map(|v| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    })
}

fn times(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// The design's `SEPARATION`: what a viewer with normal, protan and
/// deutan vision sees of linear light (Vienot, Brettel and Mollon, 1999),
/// and the two matrices of OKLab.
const VIEWERS: [[[f64; 3]; 3]; 3] = [
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    [[0.11238, 0.88762, 0.0], [0.11238, 0.88762, 0.0], [0.00401, -0.00401, 1.0]],
    [[0.29275, 0.70725, 0.0], [0.29275, 0.70725, 0.0], [-0.02234, 0.02234, 1.0]],
];
const TO_CONE: [[f64; 3]; 3] = [
    [0.4122214708, 0.5363325363, 0.0514459929],
    [0.2119034982, 0.6806995451, 0.1073969566],
    [0.0883024619, 0.2817188376, 0.6299787005],
];
const TO_LAB: [[f64; 3]; 3] = [
    [0.2104542553, 0.793617785, -0.0040720468],
    [1.9779984951, -2.428592205, 0.4505937099],
    [0.0259040371, 0.7827717662, -0.808675766],
];

/// How far apart two colours are for the viewer who tells them apart
/// least: the distance in OKLab times 100, where about 2 is just
/// noticeable. The design's `separation`.
pub fn separation(a: Rgb, b: Rgb) -> f64 {
    let seen = |colour: Rgb, viewer: &[[f64; 3]; 3]| {
        let cone = times(&TO_CONE, times(viewer, linear(colour)).map(|v| v.max(0.0)));
        times(&TO_LAB, cone.map(f64::cbrt))
    };
    let apart = |viewer: &[[f64; 3]; 3]| {
        let (p, q) = (seen(a, viewer), seen(b, viewer));
        100.0 * ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    VIEWERS.iter().map(apart).fold(f64::INFINITY, f64::min)
}

/// WCAG contrast of two colours, 1 to 21: the design's `contrast`.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let luminance = |colour: Rgb| {
        let [r, g, b] = linear(colour);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn of(colours: &[Rgb; 12], t: Tok) -> Rgb {
    Tok::ALL.iter().position(|x| *x == t).map_or(t.truecolor(), |i| colours[i])
}

/// The smallest separation of `token` from each of `others`.
fn least(colours: &[Rgb; 12], token: Tok, others: &[Tok]) -> f64 {
    others.iter().map(|other| separation(of(colours, token), of(colours, *other))).fold(f64::INFINITY, f64::min)
}

/// The order accounts take colours in on a loaded palette, from its own
/// values: the design's `accountOrder`, by the rule `layout` states. It is
/// the same however many accounts there are.
pub fn account_order(colours: &[Rgb; 12]) -> Vec<Tok> {
    let mut order: Vec<Tok> = Vec::new();
    let (mut pool, mut last) = (layout::ACCOUNTS_POOL.to_vec(), layout::ACCOUNTS_LAST.to_vec());
    while !pool.is_empty() || !last.is_empty() {
        let score = |token: Tok| least(colours, token, &[&order[..], &layout::ACCOUNTS_BESIDE].concat());
        let best = |tokens: &[Tok]| {
            tokens.iter().copied().fold(None, |held: Option<Tok>, token| match held {
                Some(held) if score(token) <= score(held) + layout::ACCOUNTS_MARGIN => Some(held),
                _ => Some(token),
            })
        };
        let pick = match (best(&pool), best(&last)) {
            (Some(pick), Some(late))
                if score(pick) < layout::ACCOUNTS_FLOOR && score(late) > score(pick) + layout::ACCOUNTS_MARGIN =>
            {
                late
            }
            (Some(pick), _) | (None, Some(pick)) => pick,
            (None, None) => break,
        };
        order.push(pick);
        pool.retain(|token| *token != pick);
        last.retain(|token| *token != pick);
    }
    order
}

/// The colour of a row's words on a loaded palette, given the tokens the
/// configured accounts wear, or `None` when no accent would show: the
/// design's `wordsColour`.
pub fn words_colour(colours: &[Rgb; 12], in_use: &[Tok]) -> Option<Tok> {
    let beside = [&layout::WORDS_BESIDE[..], in_use].concat();
    let score = |token: Tok| least(colours, token, &beside);
    let shows = |token: &Tok| contrast(of(colours, *token), of(colours, Tok::Sel)) >= layout::WORDS_ON_SEL;
    let free = layout::WORDS_POOL.into_iter().filter(|token| !in_use.contains(token) && shows(token));
    let best = free.fold(None, |held: Option<Tok>, token| match held {
        Some(held) if score(token) <= score(held) => Some(held),
        _ => Some(token),
    })?;
    (score(best) >= layout::WORDS_FLOOR).then_some(best)
}

/// The tier a terminal can show, from what it advertises: `COLORTERM` of
/// `truecolor` or `24bit`, else a `TERM` naming 256 colours, else sixteen.
pub fn detect(colorterm: Option<&str>, term: Option<&str>) -> Tier {
    if matches!(colorterm, Some("truecolor" | "24bit")) {
        Tier::True
    } else if term.is_some_and(|t| t.contains("256color")) {
        Tier::C256
    } else {
        Tier::C16
    }
}

#[cfg(test)]
pub(crate) mod tests;

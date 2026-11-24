//! Palette files (ADR-003) and `tuimex palette import` (ADR-026): a
//! terminal theme the user already holds becomes a palette file in their
//! config directory, and `ui.palette` names the file to draw with. No
//! colour of any theme is in this repository.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tuimex_store::files;
use tuimex_ui::palette::{Rgb, Tok, hex, parse_hex};

/// The name the imported palette is written under, in the config directory.
const FILE: &str = "palette.toml";

/// Where a theme keeps a colour.
#[derive(Clone, Copy)]
enum Source {
    Background,
    Foreground,
    Selection,
    /// One of the sixteen terminal colours, by its number.
    Ansi(usize),
    /// The first of two of them that the theme has.
    AnsiOr(usize, usize),
}

/// ADR-026's table: the design's token, and the theme colour it is given.
/// Each token takes the colour of the name the design's sixteen-colour tier
/// gives it: `dim` bright black, which is where a theme puts the colour of
/// its comments, `acc` magenta, where one puts its pink, `link` bright blue,
/// or blue in a theme that names no bright colours, and `head` bright
/// yellow. `faint` has no counterpart in a theme and is not here.
const MAPPING: [(Tok, Source); 11] = [
    (Tok::Bg, Source::Background),
    (Tok::Fg, Source::Foreground),
    (Tok::Sel, Source::Selection),
    (Tok::Dim, Source::Ansi(8)),
    (Tok::Err, Source::Ansi(1)),
    (Tok::Ok, Source::Ansi(2)),
    (Tok::Warn, Source::Ansi(3)),
    (Tok::Acc, Source::Ansi(5)),
    (Tok::Agent, Source::Ansi(6)),
    (Tok::Link, Source::AnsiOr(12, 4)),
    (Tok::Head, Source::Ansi(11)),
];

#[derive(Debug, Default, PartialEq)]
struct Theme {
    background: Option<Rgb>,
    foreground: Option<Rgb>,
    selection: Option<Rgb>,
    ansi: [Option<Rgb>; 16],
}

/// A colour as either format writes one: `#rrggbb`, `rrggbb` or `0xrrggbb`.
fn colour(text: &str) -> Option<Rgb> {
    let digits = text.trim().trim_matches('"');
    let digits = digits.strip_prefix('#').or_else(|| digits.strip_prefix("0x")).unwrap_or(digits);
    parse_hex(&format!("#{digits}"))
}

/// Alacritty's format: TOML, with the colours under `[colors]`.
fn alacritty(text: &str) -> Option<Theme> {
    // A file that is not TOML is not an error here: it is Ghostty's.
    let table: toml::Table = toml::from_str(text).ok()?;
    let colors = table.get("colors")?.as_table()?;
    let at = |group: &str, key: &str| colors.get(group)?.as_table()?.get(key)?.as_str().and_then(colour);
    let mut theme = Theme {
        background: at("primary", "background"),
        foreground: at("primary", "foreground"),
        selection: at("selection", "background"),
        ..Theme::default()
    };
    let names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
    for (i, name) in names.into_iter().enumerate() {
        theme.ansi[i] = at("normal", name);
        theme.ansi[i + 8] = at("bright", name);
    }
    Some(theme)
}

/// Ghostty's format: `key = value` lines, the sixteen colours as
/// `palette = N=colour`.
fn ghostty(text: &str) -> Theme {
    let mut theme = Theme::default();
    for (key, value) in text.lines().filter_map(|line| line.split_once('=')) {
        match key.trim() {
            "background" => theme.background = colour(value),
            "foreground" => theme.foreground = colour(value),
            "selection-background" => theme.selection = colour(value),
            "palette" => {
                // A line that names no colour of the sixteen sets none.
                let Some((n, value)) = value.split_once('=') else { continue };
                if let Some(slot) = n.trim().parse::<usize>().ok().and_then(|n| theme.ansi.get_mut(n)) {
                    *slot = colour(value);
                }
            }
            _ => {}
        }
    }
    theme
}

/// The tokens a theme gives a colour to, by the table. `faint` is derived:
/// halfway between `dim` and the background, so that on any theme it is
/// dimmer than `dim` and still off the ground.
fn palette_of(theme: &Theme) -> Result<Vec<(Tok, Rgb)>> {
    let mut out = Vec::new();
    for (tok, source) in MAPPING {
        let found = match source {
            Source::Background => theme.background,
            Source::Foreground => theme.foreground,
            Source::Selection => theme.selection,
            Source::Ansi(n) => theme.ansi[n],
            Source::AnsiOr(n, or) => theme.ansi[n].or(theme.ansi[or]),
        };
        out.extend(found.map(|rgb| (tok, rgb)));
    }
    let of = |tok| out.iter().find(|(t, _)| *t == tok).map(|(_, rgb)| *rgb);
    if of(Tok::Bg).is_none() || of(Tok::Fg).is_none() {
        bail!("no background and foreground in it: not a Ghostty or an Alacritty theme");
    }
    if let (Some(dim), Some(bg)) = (of(Tok::Dim), of(Tok::Bg)) {
        let mid = |a: u8, b: u8| ((u16::from(a) + u16::from(b)) / 2) as u8;
        out.push((Tok::Faint, (mid(dim.0, bg.0), mid(dim.1, bg.1), mid(dim.2, bg.2))));
    }
    Ok(out)
}

/// What `import` did, for the command to say.
pub struct Imported {
    pub path: PathBuf,
    /// The tokens the theme had no colour for, which keep the built-in one.
    pub built_in: Vec<&'static str>,
}

/// Reads the theme at `theme` and writes its palette into `config_dir`.
/// A palette already there is left alone: it may be one the user edited.
pub fn import(theme: &Path, config_dir: &Path) -> Result<Imported> {
    let text = std::fs::read_to_string(theme).with_context(|| format!("reading {}", theme.display()))?;
    let parsed = alacritty(&text).unwrap_or_else(|| ghostty(&text));
    let palette = palette_of(&parsed).with_context(|| theme.display().to_string())?;
    let mut body = String::from("# Written by `tuimex palette import`. A token left out keeps the built-in colour.\n");
    for (tok, rgb) in &palette {
        body += &format!("{} = \"{}\"\n", tok.name(), hex(*rgb));
    }
    let path = config_dir.join(FILE);
    files::private_dir(config_dir).with_context(|| format!("creating {}", config_dir.display()))?;
    let mut file = files::create(&path)
        .with_context(|| format!("creating {}; if it exists, move it aside to import another theme", path.display()))?;
    file.write_all(body.as_bytes()).with_context(|| format!("writing {}", path.display()))?;
    let built_in =
        Tok::ALL.iter().filter(|t| !palette.iter().any(|(given, _)| given == *t)).map(|t| t.name()).collect();
    Ok(Imported { path, built_in })
}

/// A palette file: `token = "#rrggbb"` lines. The colours come back in
/// `Tok::ALL`'s order, the built-in one for a token the file leaves out.
pub fn load(path: &Path) -> Result<[Rgb; 12]> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading the palette {}", path.display()))?;
    parse(&text).with_context(|| format!("the palette {}", path.display()))
}

fn parse(text: &str) -> Result<[Rgb; 12]> {
    let given: BTreeMap<String, String> =
        toml::from_str(text).map_err(|_| anyhow::anyhow!("is not `token = \"#rrggbb\"` lines"))?;
    let mut out = Tok::ALL.map(Tok::truecolor);
    for (name, value) in given {
        let at = Tok::ALL.iter().position(|t| t.name() == name);
        let at = at.with_context(|| format!("`{name}` is not a token of the palette"))?;
        out[at] = parse_hex(&value).with_context(|| format!("`{name}` is not a #rrggbb colour"))?;
    }
    Ok(out)
}

/// The file `ui.palette` names. `~/` is the home directory, and a relative
/// path is relative to the config directory, never the current one
/// (ADR-011).
pub fn resolve(value: &str, home: &Path, config_dir: &Path) -> PathBuf {
    match value.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => config_dir.join(value),
    }
}

#[cfg(test)]
mod tests;

//! Which colours and glyphs to draw with: what the user asked for, and
//! otherwise what the terminal says it can show.

use tuimex_ui::glyphs::{self, Glyphs};
use tuimex_ui::paint::Theme;
use tuimex_ui::palette::{self, Palette, Tier};

use crate::cli;

/// `--colors` and `--glyphs` win. Without them the tier comes from
/// `COLORTERM` and `TERM`, and the glyphs from the locale. `env` is the
/// environment, passed in so that a test can supply one.
pub fn choose(colors: Option<cli::Colors>, glyphs: Option<cli::Glyphs>, env: impl Fn(&str) -> Option<String>) -> Theme {
    let term = env("TERM");
    let tier = match colors {
        Some(cli::Colors::True) => Tier::True,
        Some(cli::Colors::C256) => Tier::C256,
        Some(cli::Colors::C16) => Tier::C16,
        None => palette::detect(env("COLORTERM").as_deref(), term.as_deref()),
    };
    let glyphs = match glyphs {
        Some(cli::Glyphs::Ascii) => Glyphs::Ascii,
        // The design draws two tiers and has no table for a third.
        Some(cli::Glyphs::Unicode | cli::Glyphs::Nerd) => Glyphs::Unicode,
        None => {
            let locale = [env("LC_ALL"), env("LC_CTYPE"), env("LANG")];
            glyphs::detect(locale.each_ref().map(Option::as_deref), term.as_deref())
        }
    };
    Theme { palette: Palette::new(tier), glyphs }
}

#[cfg(test)]
mod tests;

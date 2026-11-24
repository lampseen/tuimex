use tuimex_ui::glyphs::Glyphs;
use tuimex_ui::palette::Tier;

use super::choose;
use crate::cli::{Colors, Glyphs as Asked};

fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
}

#[test]
fn the_environment_decides_when_no_flag_is_given() {
    let theme =
        choose(None, None, env(&[("COLORTERM", "truecolor"), ("TERM", "xterm-256color"), ("LANG", "en_GB.UTF-8")]));
    assert_eq!((theme.palette.tier, theme.glyphs), (Tier::True, Glyphs::Unicode));
    let theme = choose(None, None, env(&[("TERM", "xterm-256color"), ("LANG", "C")]));
    assert_eq!((theme.palette.tier, theme.glyphs), (Tier::C256, Glyphs::Ascii));
    let theme = choose(None, None, env(&[]));
    assert_eq!((theme.palette.tier, theme.glyphs), (Tier::C16, Glyphs::Ascii));
}

#[test]
fn a_flag_wins_over_the_environment() {
    let rich = [("COLORTERM", "truecolor"), ("LANG", "en_GB.UTF-8")];
    let theme = choose(Some(Colors::C16), Some(Asked::Ascii), env(&rich));
    assert_eq!((theme.palette.tier, theme.glyphs), (Tier::C16, Glyphs::Ascii));
    let theme = choose(Some(Colors::True), Some(Asked::Unicode), env(&[]));
    assert_eq!((theme.palette.tier, theme.glyphs), (Tier::True, Glyphs::Unicode));
    assert_eq!(choose(Some(Colors::C256), None, env(&rich)).palette.tier, Tier::C256);
}

#[test]
fn lc_all_wins_over_lang_as_it_does_for_the_c_library() {
    let theme = choose(None, None, env(&[("LC_ALL", "C"), ("LANG", "en_GB.UTF-8")]));
    assert_eq!(theme.glyphs, Glyphs::Ascii);
}

use std::path::{Path, PathBuf};

use tuimex_ui::palette::{Rgb, Tok, hex};

use super::{alacritty, ghostty, import, load, palette_of, parse, resolve};

/// A colour for slot `n` of an invented theme. The fixtures are computed
/// so that no theme's values, real or not, are written in the repository
/// (ADR-003).
fn invented(n: u8) -> Rgb {
    (n.wrapping_mul(37).wrapping_add(11), n.wrapping_mul(59).wrapping_add(5), n.wrapping_mul(83).wrapping_add(90))
}

const BACKGROUND: u8 = 100;
const FOREGROUND: u8 = 101;
const SELECTION: u8 = 102;

fn ghostty_theme() -> String {
    let mut text = format!(
        "# an invented theme\nbackground = {}\nforeground = {}\nselection-background = {}\ncursor-color = {}\n",
        hex(invented(BACKGROUND)),
        hex(invented(FOREGROUND)).trim_start_matches('#'),
        hex(invented(SELECTION)),
        hex(invented(103)),
    );
    for n in 0..16 {
        text += &format!("palette = {n}={}\n", hex(invented(n)));
    }
    text
}

fn alacritty_theme() -> String {
    let names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
    let group = |from: u8| -> String {
        names.iter().zip(from..).map(|(name, n)| format!("{name} = \"{}\"\n", hex(invented(n)))).collect()
    };
    format!(
        "[colors.primary]\nbackground = \"{}\"\nforeground = \"{}\"\n\n[colors.selection]\nbackground = \"{}\"\n\n\
         [colors.normal]\n{}\n[colors.bright]\n{}",
        hex(invented(BACKGROUND)),
        hex(invented(FOREGROUND)).replace('#', "0x"),
        hex(invented(SELECTION)),
        group(0),
        group(8),
    )
}

/// What ADR-026's table gives for the invented theme.
fn expected() -> Vec<(Tok, Rgb)> {
    let (dim, bg) = (invented(8), invented(BACKGROUND));
    let mid = |a: u8, b: u8| ((u16::from(a) + u16::from(b)) / 2) as u8;
    vec![
        (Tok::Bg, bg),
        (Tok::Fg, invented(FOREGROUND)),
        (Tok::Sel, invented(SELECTION)),
        (Tok::Dim, dim),
        (Tok::Err, invented(1)),
        (Tok::Ok, invented(2)),
        (Tok::Warn, invented(3)),
        (Tok::Acc, invented(5)),
        (Tok::Agent, invented(6)),
        (Tok::Link, invented(12)),
        (Tok::Head, invented(11)),
        (Tok::Faint, (mid(dim.0, bg.0), mid(dim.1, bg.1), mid(dim.2, bg.2))),
    ]
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-scratch/palette").join(test);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    tuimex_store::files::private_dir(&dir).unwrap();
    dir
}

fn theme_file(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("theme");
    tuimex_store::files::replace(&path, text.as_bytes()).unwrap();
    path
}

#[test]
fn a_ghostty_theme_maps_by_the_table() {
    assert_eq!(palette_of(&ghostty(&ghostty_theme())).unwrap(), expected());
}

#[test]
fn an_alacritty_theme_maps_by_the_table() {
    let theme = alacritty(&alacritty_theme()).expect("TOML with a colors table");
    assert_eq!(palette_of(&theme).unwrap(), expected());
    assert_eq!(theme, ghostty(&ghostty_theme()), "the two formats of one theme read alike");
}

#[test]
fn faint_lies_between_dim_and_the_background() {
    let palette = expected();
    let of = |tok| palette.iter().find(|(t, _)| *t == tok).unwrap().1;
    let (faint, dim, bg) = (of(Tok::Faint), of(Tok::Dim), of(Tok::Bg));
    for (f, d, b) in [(faint.0, dim.0, bg.0), (faint.1, dim.1, bg.1), (faint.2, dim.2, bg.2)] {
        assert!((d.min(b)..=d.max(b)).contains(&f));
    }
}

#[test]
fn a_colour_the_theme_lacks_is_left_built_in() {
    let dir = scratch("lacks");
    let without: String = ghostty_theme()
        .lines()
        .filter(|l| !l.starts_with("selection") && !l.starts_with("palette = 8="))
        .fold(String::new(), |mut text, line| {
            text += line;
            text + "\n"
        });
    let imported = import(&theme_file(&dir, &without), &dir.join("config")).unwrap();
    assert_eq!(imported.built_in, ["sel", "dim", "faint"]);
    let loaded = load(&imported.path).unwrap();
    for (i, tok) in Tok::ALL.into_iter().enumerate() {
        let given = expected().into_iter().find(|(t, _)| *t == tok).map(|(_, rgb)| rgb);
        let want = if imported.built_in.contains(&tok.name()) { tok.truecolor() } else { given.unwrap() };
        assert_eq!(loaded[i], want, "{}", tok.name());
    }
}

#[test]
fn a_theme_with_no_bright_blue_gives_link_its_blue() {
    let without: String =
        ghostty_theme().lines().filter(|l| !l.starts_with("palette = 12=")).map(|l| format!("{l}\n")).collect();
    let palette = palette_of(&ghostty(&without)).unwrap();
    assert_eq!(palette.iter().find(|(tok, _)| *tok == Tok::Link), Some(&(Tok::Link, invented(4))));
}

#[test]
fn an_imported_palette_loads_back_as_the_table_mapped_it() {
    for (name, theme) in [("ghostty", ghostty_theme()), ("alacritty", alacritty_theme())] {
        let dir = scratch(name);
        let imported = import(&theme_file(&dir, &theme), &dir.join("config")).unwrap();
        assert_eq!(imported.path, dir.join("config/palette.toml"));
        assert!(imported.built_in.is_empty());
        let loaded = load(&imported.path).unwrap();
        for (tok, rgb) in expected() {
            assert_eq!(loaded[Tok::ALL.iter().position(|t| *t == tok).unwrap()], rgb, "{name}: {}", tok.name());
        }
    }
}

#[test]
fn a_palette_already_there_is_not_written_over() {
    let dir = scratch("twice");
    let theme = theme_file(&dir, &ghostty_theme());
    let first = import(&theme, &dir.join("config")).unwrap();
    let before = std::fs::read(&first.path).unwrap();
    let again = format!("{:#}", import(&theme, &dir.join("config")).err().expect("the second import is refused"));
    assert!(again.contains("move it aside"), "{again}");
    assert_eq!(std::fs::read(&first.path).unwrap(), before);
}

#[test]
fn a_file_that_is_no_theme_is_refused_and_nothing_is_written() {
    let dir = scratch("no-theme");
    for text in ["", "just some words\n", "[colors.normal]\nred = \"x\"\n", "background = teal\nforeground = navy\n"] {
        let error = import(&theme_file(&dir, text), &dir.join("config")).err().expect("refused");
        assert!(format!("{error:#}").contains("not a Ghostty or an Alacritty theme"), "{error:#}");
    }
    assert!(!dir.join("config").exists());
    assert!(import(&dir.join("absent"), &dir.join("config")).is_err());
}

#[test]
fn a_palette_file_with_an_unknown_token_or_a_bad_colour_is_refused() {
    let colour = hex(invented(1));
    assert_eq!(parse("").unwrap(), Tok::ALL.map(Tok::truecolor));
    assert_eq!(parse(&format!("err = \"{colour}\"\n")).unwrap()[8], invented(1));
    for (text, says) in [
        (format!("surface = \"{colour}\"\n"), "`surface` is not a token"),
        ("acc = \"orange\"\n".to_string(), "`acc` is not a #rrggbb colour"),
        ("acc = 3\n".to_string(), "is not `token = \"#rrggbb\"` lines"),
        ("acc\n".to_string(), "is not `token = \"#rrggbb\"` lines"),
    ] {
        let error = format!("{:#}", parse(&text).unwrap_err());
        assert!(error.contains(says), "{text:?}: {error}");
    }
}

#[test]
fn ui_palette_is_under_home_or_the_config_directory_and_never_the_current_one() {
    let (home, config) = (Path::new("/home/robin"), Path::new("/home/robin/.config/tuimex"));
    assert_eq!(resolve("~/themes/p.toml", home, config), Path::new("/home/robin/themes/p.toml"));
    assert_eq!(resolve("palette.toml", home, config), config.join("palette.toml"));
    assert_eq!(resolve("/etc/p.toml", home, config), Path::new("/etc/p.toml"));
}

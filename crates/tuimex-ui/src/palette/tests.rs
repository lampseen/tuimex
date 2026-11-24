use std::path::Path;

use ratatui::style::Color;

use super::{Palette, Rgb, Tier, Tok, account_order, contrast, detect, hex, parse_hex, separation, words_colour};

/// The lines of one tier's table in `design/tui.js`, as `token -> value`.
fn design_tier(tier: &str) -> Vec<(String, String)> {
    let js = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/tui.js")).unwrap();
    let palette = js.split("const PALETTE = {").nth(1).unwrap().split("\n};").next().unwrap();
    let table = palette.split(&format!("  {tier}: {{")).nth(1).unwrap().split("  },").next().unwrap();
    let entry = |e: &str| {
        let (key, value) = e.split_once(':')?;
        Some((key.trim().to_string(), value.trim().trim_matches('\'').to_string()))
    };
    table.split(',').filter_map(entry).collect()
}

#[test]
fn every_tier_is_the_design_s_table() {
    let name16 = |c: Option<Color>| match c {
        None => "none",
        Some(Color::Reset) => "default",
        Some(Color::Black) => "black",
        Some(Color::DarkGray) => "brightblack",
        Some(Color::Magenta) => "magenta",
        Some(Color::Green) => "green",
        Some(Color::Yellow) => "yellow",
        Some(Color::Red) => "red",
        Some(Color::Cyan) => "cyan",
        Some(Color::LightBlue) => "brightblue",
        Some(Color::LightYellow) => "brightyellow",
        other => panic!("{other:?} is not one of the design's sixteen-colour names"),
    };
    for (tier, ours) in [
        ("truecolor", Tok::ALL.map(|t| (t.name().to_string(), hex(t.truecolor())))),
        ("c256", Tok::ALL.map(|t| (t.name().to_string(), t.c256().to_string()))),
        ("c16", Tok::ALL.map(|t| (t.name().to_string(), name16(t.c16()).to_string()))),
    ] {
        let mut design = design_tier(tier);
        let mut ours = ours.to_vec();
        design.sort();
        ours.sort();
        assert_eq!(ours, design, "{tier}");
    }
}

#[test]
fn a_token_is_found_by_its_name() {
    for t in Tok::ALL {
        assert_eq!(Tok::from_name(t.name()), Some(t));
    }
    assert_eq!(Tok::from_name("surface"), None);
}

#[test]
fn a_palette_file_replaces_truecolor_and_no_other_tier() {
    let mut custom = Tok::ALL.map(Tok::truecolor);
    custom[5] = (1, 2, 3);
    let with = |tier| Palette { tier, custom: Some(custom) };
    assert_eq!(with(Tier::True).color(Tok::Acc), Some(Color::Rgb(1, 2, 3)));
    assert_eq!(with(Tier::True).color(Tok::Fg), Palette::new(Tier::True).color(Tok::Fg));
    assert_eq!(with(Tier::C256).color(Tok::Acc), Palette::new(Tier::C256).color(Tok::Acc));
    assert_eq!(with(Tier::C16).color(Tok::Acc), Some(Color::Magenta));
}

#[test]
fn sixteen_colours_have_no_fill_for_the_selected_row() {
    assert_eq!(Palette::new(Tier::C16).color(Tok::Sel), None);
    assert_eq!(Palette::new(Tier::C16).color(Tok::Dim), Palette::new(Tier::C16).color(Tok::Faint));
}

#[test]
fn hex_round_trips_and_refuses_what_is_not_six_digits() {
    assert_eq!(parse_hex(&hex((0, 171, 255))), Some((0, 171, 255)));
    for bad in ["", "#", "#12345", "#1234567", "123456", "#12345g", "#+12345"] {
        assert_eq!(parse_hex(bad), None, "{bad}");
    }
}

#[test]
fn the_tier_comes_from_what_the_terminal_advertises() {
    assert_eq!(detect(Some("truecolor"), Some("xterm")), Tier::True);
    assert_eq!(detect(Some("24bit"), None), Tier::True);
    assert_eq!(detect(None, Some("tmux-256color")), Tier::C256);
    assert_eq!(detect(Some("yes"), Some("xterm-256color")), Tier::C256);
    assert_eq!(detect(None, Some("xterm")), Tier::C16);
    assert_eq!(detect(None, None), Tier::C16);
}

/// An invented palette, in `Tok::ALL`'s order, with some tokens changed.
/// Colours are written as numbers: no theme's values are in the
/// repository (ADR-003).
pub(crate) fn invented(changed: &[(Tok, Rgb)]) -> [Rgb; 12] {
    let mut colours = [
        (20, 22, 30),
        (50, 54, 70),
        (230, 230, 235),
        (150, 155, 170),
        (100, 104, 118),
        (240, 160, 60),
        (60, 200, 165),
        (235, 215, 110),
        (240, 110, 110),
        (110, 200, 210),
        (90, 130, 255),
        (255, 90, 120),
    ];
    for (token, colour) in changed {
        colours[Tok::ALL.iter().position(|t| t == token).unwrap()] = *colour;
    }
    colours
}

/// Cyan that reads as the dim text, and green that reads as the text.
pub(crate) const CYAN_AS_DIM: (Tok, Rgb) = (Tok::Agent, (160, 166, 180));
pub(crate) const GREEN_AS_TEXT: (Tok, Rgb) = (Tok::Ok, (226, 232, 228));
/// Greens, blues and pinks under which green is the best apart.
pub(crate) const GREEN_FIRST: [(Tok, Rgb); 3] =
    [(Tok::Ok, (130, 215, 120)), (Tok::Link, (110, 150, 255)), (Tok::Head, (255, 120, 190))];

/// The numbers are what `TUI.separation` and `TUI.contrast` of
/// `design/tui.js` give for each pair.
#[test]
fn separation_and_contrast_are_the_design_s_for_a_table_of_colour_pairs() {
    let pairs: [(Rgb, Rgb, f64, f64); 8] = [
        ((0, 0, 0), (255, 255, 255), 99.99999934735469, 21.0),
        ((255, 0, 0), (0, 255, 0), 22.403207547362502, 2.9139375476009137),
        ((120, 160, 250), (110, 200, 210), 11.903175519368705, 1.322634353902627),
        ((250, 150, 200), (150, 155, 170), 5.059370575446421, 1.3562826860629607),
        ((12, 200, 90), (200, 90, 12), 12.71592562279916, 1.9142275722976394),
        ((33, 33, 33), (34, 33, 33), 0.04821178762146521, 1.0025684039067335),
        ((240, 160, 60), (235, 215, 110), 10.46374284493052, 1.4775305756535964),
        ((5, 5, 250), (250, 5, 250), 15.806098909338537, 2.6849489456283093),
    ];
    for (a, b, apart, ratio) in pairs {
        assert!((separation(a, b) - apart).abs() < 1e-9, "{a:?} {b:?}: {} for {apart}", separation(a, b));
        assert!((separation(b, a) - apart).abs() < 1e-9, "either way round");
        assert!((contrast(a, b) - ratio).abs() < 1e-9, "{a:?} {b:?}: {} for {ratio}", contrast(a, b));
    }
    let agent_beside_link = separation(Tok::Link.truecolor(), Tok::Agent.truecolor());
    assert!((agent_beside_link - 7.553092848121874).abs() < 1e-9, "{agent_beside_link}");
}

/// Each order is what `TUI.accountOrder` gives the same palette.
#[test]
fn a_loaded_palette_orders_the_account_colours_by_the_design_s_rule() {
    use Tok::{Head, Link, Ok, Warn};
    let built_in = Tok::ALL.map(Tok::truecolor);
    assert_eq!(account_order(&built_in), [Link, Head, Ok, Warn], "the built-in values give the built-in order");
    assert_eq!(account_order(&invented(&[])), [Link, Head, Ok, Warn]);
    assert_eq!(account_order(&invented(&GREEN_FIRST)), [Ok, Link, Head, Warn]);
    assert_eq!(account_order(&invented(&[CYAN_AS_DIM, GREEN_AS_TEXT])), [Link, Head, Warn, Ok], "yellow takes a step");
    let loaded = |tier| Palette { tier, custom: Some(invented(&GREEN_FIRST)) };
    assert_eq!(loaded(Tier::True).account_order(), [Ok, Link, Head, Warn]);
    assert_eq!(loaded(Tier::C256).account_order(), [Link, Head, Ok, Warn], "a tier that cannot show it keeps its own");
}

/// Each answer is what `TUI.wordsColour` gives the same palette and the
/// first accounts of its order.
#[test]
fn the_words_take_the_free_accent_that_shows_or_fall_back_to_brightness() {
    let words = |changed: &[(Tok, Rgb)], accounts: usize| {
        let colours = invented(changed);
        words_colour(&colours, &account_order(&colours)[..accounts])
    };
    assert_eq!([1, 2, 3].map(|n| words(&[], n)), [Some(Tok::Agent); 3]);
    assert_eq!(words(&[CYAN_AS_DIM], 2), Some(Tok::Ok), "cyan that reads as dim is passed over for green");
    assert_eq!(words(&[CYAN_AS_DIM], 3), None, "which the third account wears");
    assert_eq!([1, 2, 3].map(|n| words(&[CYAN_AS_DIM, GREEN_AS_TEXT], n)), [None; 3], "nothing left would show");
    let dark = (Tok::Agent, (70, 130, 140));
    let colours = invented(&[dark]);
    assert!(contrast(colours[9], colours[1]) < 4.5 && contrast(invented(&[CYAN_AS_DIM])[9], colours[1]) >= 4.5);
    assert_eq!(words(&[dark], 2), Some(Tok::Ok), "cyan too dark for the selected row is not taken");

    let loaded = |changed: &[(Tok, Rgb)]| Palette { tier: Tier::True, custom: Some(invented(changed)) };
    assert_eq!(Palette::new(Tier::True).words(&[Tok::Link]), (Tok::Agent, Tok::Agent), "built in, they are cyan");
    assert_eq!(loaded(&[CYAN_AS_DIM]).words(&[Tok::Link, Tok::Head]), (Tok::Ok, Tok::Ok));
    assert_eq!(loaded(&[CYAN_AS_DIM, GREEN_AS_TEXT]).words(&[Tok::Link, Tok::Head]), (Tok::Fg, Tok::Dim));
}

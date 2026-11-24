use super::*;

fn hidden(style: &str) -> bool {
    hides(declarations(style).iter())
}

#[test]
fn a_style_attribute_hides_by_the_table() {
    for (style, want) in [
        ("display:none", true),
        ("DISPLAY : NONE", true),
        ("display:none !important", true),
        ("display:none! IMPORTANT ;color:red", true),
        ("color:red;display:none", true),
        ("d\\69splay:none", true),
        ("display:\\6e one", true),
        ("display:n\\one", true),
        ("dis/**/play:no/* x */ne", true),
        ("visibility:hidden", true),
        ("visibility:collapse", true),
        ("opacity:0", true),
        ("opacity:0.0", true),
        ("font-size:0", true),
        ("font-size:0px", true),
        ("color:transparent", true),
        ("max-height:0;overflow:hidden", true),
        ("width:0px;height:0px;overflow:hidden", true),
        ("display:none;display:block", false),
        ("display:none !important;display:block", true),
        ("display:block", false),
        ("height:0", false),
        ("opacity:0.5", false),
        ("font-size:10px", false),
        ("background:url(none)", false),
        ("", false),
    ] {
        assert_eq!(hidden(style), want, "{style:?}");
    }
}

#[test]
fn every_spelling_of_a_fetch_is_found() {
    for (css, want) in [
        ("background:url(https://example.net/a.png)", "https://example.net/a.png"),
        ("background:URL( 'HTTPS://EXAMPLE.NET/a.png' )", "https://example.net/a.png"),
        ("background:url(\\68ttps://example.net/a.png)", "https://example.net/a.png"),
        ("background:u\\72l(https://example.net/a.png)", "https://example.net/a.png"),
        ("@import 'https://example.net/a.css';", "https://example.net/a.css"),
        ("@IMPORT \"//example.net/a.css\";", "//example.net/a.css"),
        ("background:image-set(\"https://example.net/a.png\" 1x)", "https://example.net/a.png"),
        ("background:ur/**/l(https://example.net/a.png)", "https://example.net/a.png"),
    ] {
        assert!(urls(css).iter().any(|u| u == want), "{css}: {:?}", urls(css));
    }
    assert!(urls("color:red").is_empty());
}

fn sheet_hides(css: &str, tag: &str, id: &str, classes: &[&str]) -> bool {
    let mut sheet = Sheet::default();
    sheet.add(css);
    let classes: Vec<String> = classes.iter().map(|c| c.to_string()).collect();
    hides(sheet.matching(tag, id, &classes))
}

#[test]
fn a_stylesheet_hides_by_the_table() {
    // (css, tag, id, classes, hidden)
    let table: [(&str, &str, &str, &[&str], bool); 16] = [
        (".x{display:none}", "div", "", &["x"], true),
        (".x{display:none}", "div", "", &["y"], false),
        (".X { DISPLAY: NONE }", "div", "", &["x"], true),
        ("div.x.y{display:none}", "div", "", &["x", "y"], true),
        ("div.x.y{display:none}", "div", "", &["x"], false),
        ("span.x{display:none}", "div", "", &["x"], false),
        ("#pre{visibility:hidden}", "p", "pre", &[], true),
        ("p, .x , td{display:none}", "div", "", &["x"], true),
        ("table td > .x{display:none}", "div", "", &["x"], true),
        (".x{color:red} .x{display:none}", "div", "", &["x"], true),
        (".x{display:none} .x{display:block}", "div", "", &["x"], false),
        ("@import 'a.css'; .x{display:none}", "div", "", &["x"], true),
        ("@media (max-width:600px){.x{display:none} .y{color:red}} .z{display:none}", "div", "", &["x"], false),
        ("@media (max-width:600px){.x{display:none}} .z{display:none}", "div", "", &["z"], true),
        (".x:hover{display:none}", "div", "", &["x"], false),
        (".\\78{display:none}", "div", "", &["x"], true),
    ];
    for (css, tag, id, classes, want) in table {
        assert_eq!(sheet_hides(css, tag, id, classes), want, "{css}");
    }
}

#[test]
fn a_long_stylesheet_is_cut_off() {
    let mut sheet = Sheet::default();
    sheet.add(&".x{display:none}".repeat(10 * MAX_RULES));
    assert_eq!(sheet.rules.len(), MAX_RULES);
}

use super::*;

fn page(html: &str) -> Page {
    read(html, 72).unwrap()
}

#[test]
fn structure_survives_as_text() {
    let p = page(
        "<h1>Title</h1><p>One <em>two</em> <b>three</b>.</p><ul><li>a</li><li>b</li></ul>\
         <blockquote>quoted</blockquote><table><tr><td>left</td><td>right</td></tr></table>",
    );
    for want in ["# Title", "One two three.", "* a", "* b", "> quoted", "left", "right"] {
        assert!(p.text.contains(want), "{want:?} not in {:?}", p.text);
    }
    assert!(!p.text.contains('─'), "table borders are not drawn: {:?}", p.text);
}

#[test]
fn links_are_numbered_where_they_stand_with_their_own_text() {
    let p = page(
        "<p>See <a href='https://example.com/a'>the <b>first</b></a> and \
         <a href=' https://example.org/b '>https://example.com/b</a>.</p>\
         <table><tr><td><a href='ssh://example.net/'>in a cell</a></td></tr></table>\
         <a href='#top'>top</a> <a>bare</a> <a href='https://example.net/pixel'><img src='cid:x'></a>",
    );
    assert!(p.text.contains("the first [1]"), "{:?}", p.text);
    assert!(p.text.contains("https://example.com/b [2]"), "{:?}", p.text);
    assert!(p.text.contains("in a cell [3]"), "{:?}", p.text);
    let got: Vec<(usize, &str, &str)> = p.links.iter().map(|l| (l.n, l.url.as_str(), l.shown.as_str())).collect();
    assert_eq!(
        got,
        [
            (1, "https://example.com/a", "the first"),
            (2, "https://example.org/b", "https://example.com/b"),
            (3, "ssh://example.net/", "in a cell"),
        ]
    );
    assert!(p.text.contains("top") && p.text.contains("bare") && !p.text.contains("[4]"), "{:?}", p.text);
}

#[test]
fn a_link_wrapped_over_lines_keeps_its_text_whole() {
    let p = read("<a href='https://example.com/'>one two three four five six seven eight nine ten</a>", 20).unwrap();
    assert_eq!(p.links[0].shown, "one two three four five six seven eight nine ten");
}

#[test]
fn what_a_browser_hides_is_not_text() {
    for hidden in [
        "<div style='display:none'>SECRET</div>",
        "<div style='DISPLAY: NONE'>SECRET</div>",
        "<div style='d\\69splay:none'>SECRET</div>",
        "<span style='visibility:hidden'>SECRET</span>",
        "<p hidden>SECRET</p>",
        "<div style='max-height:0;overflow:hidden'>SECRET</div>",
        "<span style='font-size:0'>SECRET</span>",
        "<span style='opacity:0'>SECRET</span>",
        "<span style='color:transparent'>SECRET</span>",
        "<div style='display:none'><table><tr><td><b>SECRET</b></td></tr></table></div>",
        "<style>.pre{display:none}</style><div class='pre'>SECRET</div>",
        "<div class='PRE'>SECRET</div><style>.pre{display:none}</style>",
        "<style>#x{visibility:hidden}</style><div id='x'>SECRET</div>",
        "<!-- SECRET -->",
        "<script>SECRET</script>",
        "<style>SECRET{}</style>",
        "<title>SECRET</title>",
        "<template>SECRET</template>",
        "<datalist><option>SECRET</option></datalist>",
        "<dialog>SECRET</dialog>",
    ] {
        let p = page(&format!("<p>before</p>{hidden}<p>after</p>"));
        assert!(!p.text.contains("SECRET"), "{hidden} gave {:?}", p.text);
        assert!(p.text.contains("before") && p.text.contains("after"), "{hidden} gave {:?}", p.text);
    }
}

#[test]
fn what_a_browser_shows_is_text() {
    for shown in [
        "<div style='display:block'>SHOWN</div>",
        "<div style='display:none;display:block'>SHOWN</div>",
        "<div style='height:0'>SHOWN</div>",
        "<style>@media (max-width:600px){.wide{display:none}}</style><div class='wide'>SHOWN</div>",
        "<style>.other{display:none}</style><div class='wide'>SHOWN</div>",
    ] {
        assert!(page(shown).text.contains("SHOWN"), "{shown}");
    }
}

#[test]
fn every_way_of_loading_is_reported_and_nothing_else() {
    for (html, want) in [
        ("<img src='https://a.example.net/x.png'>", "a.example.net"),
        ("<img src=' https://a.example.net/x.png'>", "a.example.net"),
        ("<img src='HTTPS://A.EXAMPLE.NET/x.png'>", "a.example.net"),
        ("<img src='//a.example.net/x.png'>", "a.example.net"),
        ("<img src='ht\ntps://a.example.net/x.png'>", "a.example.net"),
        (
            "<img srcset='https://a.example.net/x.png 2x, https://b.example.net/y.png 3x'>",
            "a.example.net b.example.net",
        ),
        ("<table background='https://a.example.net/x.png'><tr><td>x</td></tr></table>", "a.example.net"),
        (
            "<video poster='https://a.example.net/x.png' src='https://b.example.net/v'></video>",
            "a.example.net b.example.net",
        ),
        ("<link rel='stylesheet' href='https://a.example.net/x.css'>", "a.example.net"),
        ("<style>@import 'https://a.example.net/x.css';</style>", "a.example.net"),
        ("<style>body{background:url(HTTPS://a.example.net/x.png)}</style>", "a.example.net"),
        ("<style>body{background:url(\\68ttps://a.example.net/x.png)}</style>", "a.example.net"),
        ("<p style='background:url(https://a.example.net/x.png)'>x</p>", "a.example.net"),
        ("<div style='display:none'><img src='https://a.example.net/x.png'></div>", "a.example.net"),
        ("<svg><image href='https://a.example.net/x.png'/></svg>", "a.example.net"),
        ("<iframe src='https://a.example.net/'></iframe>", "a.example.net"),
        ("<script src='https://a.example.net/x.js'></script>", "a.example.net"),
        ("<a href='https://a.example.net/'>a link is not a load</a>", ""),
        ("<img src='cid:x@example.com'><img src='data:image/png;base64,AAAA'><img src='/etc/hostname'>", ""),
    ] {
        assert_eq!(page(html).remote.join(" "), want, "{html}");
    }
}

#[test]
fn trackers_are_named() {
    for (html, want) in [
        ("<img src='https://t.example.net/o.gif' width='1' height='1'>", "t.example.net"),
        ("<img src='https://t.example.net/o.gif' style='width:1px;height:1px'>", "t.example.net"),
        ("<img src='https://t.example.net/o.gif' width='0' height='0'>", "t.example.net"),
        ("<img src='https://t.example.net/o.gif' style='display:none'>", "t.example.net"),
        ("<div style='display:none'><img src='https://t.example.net/o.gif'></div>", "t.example.net"),
        ("<img src='https://u123.ct.sendgrid.net/wf/open?x=1'>", "SendGrid"),
        ("<img src='https://notsendgrid.net/a.png'>", ""),
        ("<img src='https://t.example.net/banner.png' width='600' height='200'>", ""),
        ("<img src='https://t.example.net/banner.png'>", ""),
    ] {
        assert_eq!(page(html).trackers.join(" "), want, "{html}");
    }
}

#[test]
fn nothing_that_drives_a_terminal_reaches_the_text() {
    let p = page("<p>a&#27;[31mb&#x202e;c</p><a href='https://example.com/'>x&#7;y</a>");
    assert!(p.text.contains("a[31mbc"), "{:?}", p.text);
    assert_eq!(p.links[0].shown, "xy");
}

#[test]
fn a_page_nested_ten_thousand_deep_is_read() {
    let depth = 10_000;
    let html = format!("{}deep{}", "<div>".repeat(depth), "</div>".repeat(depth));
    assert!(read(&html, 72).is_ok());
}

#[test]
fn bold_however_nested_adds_no_asterisk_and_a_typed_one_stays() {
    for (html, want) in [
        ("<p>the <b><strong>parts</strong></b> of it</p>", "the parts of it"),
        ("<b><span style='font-weight:bold'><strong>A look</strong></span></b>", "A look"),
        ("<p><em>a</em> <i>b</i> <code>c</code> <kbd>d</kbd> <u>e</u> 1<sup>st</sup></p>", "a b c d e 1st"),
        ("<p>typed **so** and *so* and `so`</p>", "typed **so** and *so* and `so`"),
        ("<ul><li><b>item</b></li></ul>", "* item"),
    ] {
        assert_eq!(page(html).text, want, "{html}");
    }
}

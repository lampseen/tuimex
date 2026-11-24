use super::*;

/// The page's body, without the fixed head and tail.
fn clean(html: &str) -> String {
    let page = sanitise(html, &[], &[]);
    let body = page.split_once("<body>").unwrap().1;
    body.strip_suffix("</body></html>").unwrap().to_string()
}

#[test]
fn the_page_begins_with_the_policy() {
    let page = sanitise("<p>x</p>", &[], &[]);
    assert!(page.starts_with(
        "<!doctype html><html><head><meta http-equiv=\"Content-Security-Policy\" \
         content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\">"
    ));
}

#[test]
fn what_runs_submits_redirects_or_frames_is_removed() {
    for (html, want) in [
        ("<p>a</p><script>x()</script>", "<p>a</p>"),
        ("<p onclick=\"x()\">a</p>", "<p>a</p>"),
        ("<P ONCLICK=x()>a</P>", "<p>a</p>"),
        ("<form action=\"https://example.net/\"><input name=q>go</form>", "go"),
        ("<iframe src=\"https://example.net/\"></iframe>a", "a"),
        ("<meta http-equiv=\"refresh\" content=\"0;url=https://example.net/\">a", "a"),
        ("<base href=\"https://example.net/\">a", "a"),
        ("<link rel=\"stylesheet\" href=\"https://example.net/a.css\">a", "a"),
        ("<style>p{color:red}</style>a", "a"),
        ("<svg onload=\"x()\"><script>x()</script></svg>a", "a"),
        ("<math><mtext><script>x()</script></mtext></math>a", "a"),
        ("<a href=\"javascript:x()\">a</a>", "<a rel=\"noopener noreferrer\">a</a>"),
        ("<a href=\"jav&#x09;ascript:x()\">a</a>", "<a rel=\"noopener noreferrer\">a</a>"),
        ("<a href=\" data:text/html,x\">a</a>", "<a rel=\"noopener noreferrer\">a</a>"),
        ("<a href=\"cid:x@example.com\">a</a>", "<a rel=\"noopener noreferrer\">a</a>"),
        ("<a href=\"/local\">a</a>", "<a rel=\"noopener noreferrer\">a</a>"),
        (
            "<a href=\"https://example.com/\" target=\"_top\">a</a>",
            "<a href=\"https://example.com/\" rel=\"noopener noreferrer\">a</a>",
        ),
        (
            "<a href=\"mailto:ada@example.com\">a</a>",
            "<a href=\"mailto:ada@example.com\" rel=\"noopener noreferrer\">a</a>",
        ),
    ] {
        assert_eq!(clean(html), want, "{html}");
    }
}

#[test]
fn a_style_keeps_its_looks_and_loses_anything_it_would_fetch() {
    for (html, want) in [
        ("<p style=\"color:red;font-weight:bold\">a</p>", "<p style=\"color:red;font-weight:bold\">a</p>"),
        ("<p style=\"background:#fff\">a</p>", "<p style=\"background:#fff\">a</p>"),
        ("<p style=\"color:red;background:url(https://example.net/a.png)\">a</p>", "<p>a</p>"),
        ("<p style=\"background:URL( 'https://example.net/a.png' )\">a</p>", "<p>a</p>"),
        ("<p style=\"background:u\\72l(https://example.net/a.png)\">a</p>", "<p>a</p>"),
        ("<p style=\"background:url(\\68ttps://example.net/a.png)\">a</p>", "<p>a</p>"),
        ("<p style=\"background:image-set('https://example.net/a.png' 1x)\">a</p>", "<p>a</p>"),
        ("<p style=\"position:fixed;top:0;color:red\">a</p>", "<p style=\"color:red\">a</p>"),
        ("<p style=\"behavior:x;-moz-binding:y;color:red\">a</p>", "<p style=\"color:red\">a</p>"),
        ("<p style=\"d\\69splay:none\">a</p>", "<p style=\"display:none\">a</p>"),
    ] {
        assert_eq!(clean(html), want, "{html}");
    }
}

fn png() -> InlineImage {
    InlineImage { cid: "logo@example.com".into(), mime: "image/png".into(), data: b"\x89PNG".to_vec() }
}

#[test]
fn an_image_is_attached_inline_or_allowed_and_nothing_else() {
    let allowed = ["img.example.net".to_string()];
    let data = "data:image/png;base64,iVBORw==";
    for (src, want) in [
        ("cid:logo@example.com", Some(data)),
        ("CID:logo@example.com", Some(data)),
        ("cid:other@example.com", None),
        ("data:image/png;base64,AAAA", Some("data:image/png;base64,AAAA")),
        ("DATA:IMAGE/JPEG;BASE64,AAAA", Some("DATA:IMAGE/JPEG;BASE64,AAAA")),
        ("data:image/svg+xml;base64,AAAA", None),
        ("data:image/png,AAAA", None),
        ("data:text/html;base64,AAAA", None),
        ("https://img.example.net/a.png", Some("https://img.example.net/a.png")),
        (" https://img.example.net/a.png", Some("https://img.example.net/a.png")),
        ("http://img.example.net/a.png", None),
        ("//img.example.net/a.png", None),
        ("https://other.example.net/a.png", None),
        ("https://img.example.net@other.example.net/a.png", None),
        ("https://other.example.net/img.example.net/a.png", None),
        ("/etc/hostname", None),
        ("a.png", None),
        ("file:///etc/passwd", None),
    ] {
        let page = sanitise(&format!("<img src=\"{src}\" alt=\"x\">"), &[png()], &allowed);
        let want = match want {
            Some(src) => format!("<body><img src=\"{src}\" alt=\"x\"></body>"),
            None => "<body><img alt=\"x\"></body>".to_string(),
        };
        assert!(page.contains(&want), "{src} gave {page}");
    }
}

#[test]
fn only_an_image_element_takes_a_source() {
    let page =
        sanitise("<p src=\"cid:logo@example.com\">a</p><audio src=\"cid:logo@example.com\"></audio>", &[png()], &[]);
    assert!(!page.contains("src="), "{page}");
}

#[test]
fn an_attachment_that_is_not_a_picture_is_not_inlined() {
    let script = InlineImage { cid: "logo@example.com".into(), mime: "image/svg+xml".into(), data: b"<svg/>".to_vec() };
    let page = sanitise("<img src=\"cid:logo@example.com\">", &[script], &[]);
    assert!(page.contains("<body><img></body>"), "{page}");
}

#[test]
fn the_policy_names_only_hosts_that_are_hosts() {
    let hosts = ["img.example.net", "x; script-src *", "'unsafe-inline'", "a b", "", "*"].map(String::from);
    let page = sanitise("<p>x</p>", &[], &hosts);
    assert!(
        page.contains(
            "content=\"default-src 'none'; img-src data: https://img.example.net; style-src 'unsafe-inline'\""
        ),
        "{page}"
    );
}

/// The page's own stylesheet: the rules it kept of the message's, then
/// its own.
fn sheet(html: &str) -> String {
    let page = sanitise(html, &[], &[]);
    let head = page.split_once("<meta charset=\"utf-8\"><style>").unwrap().1;
    let (sheet, body) = head.split_once("</style></head><body>").unwrap();
    assert!(!body.contains("<style"), "{page}");
    sheet.split_once(PLACEHOLDER).map_or(sheet, |(kept, _)| kept).to_string()
}

#[test]
fn a_stylesheet_keeps_the_rules_that_lay_a_message_out_and_loses_what_fetches_runs_or_covers() {
    for (css, want) in [
        (".col{width:50%;float:left} p{color:red}", ".col{width:50%;float:left}p{color:red}"),
        (".Row > td.Cell, #sum{padding:4px 8px!important}", ".Row > td.Cell, #sum{padding:4px 8px!important}"),
        ("@media (max-width:600px){.col{width:100%}}", "@media (max-width:600px){.col{width:100%}}"),
        ("@media screen{@media print{p{color:red}}p{color:blue}}", "@media screen{p{color:blue}}"),
        ("p{color:rgb(0,0,0);width:calc(100% - 20px)}", "p{color:rgb(0,0,0);width:calc(100% - 20px)}"),
        ("/* a */p{/* b */color:red}", "p{color:red}"),
        ("@import url(https://example.net/a.css);p{color:red}", "p{color:red}"),
        ("@import 'https://example.net/a.css';", ""),
        ("@font-face{font-family:x;src:url(https://example.net/f.woff)}", ""),
        ("@keyframes spin{from{color:red}to{color:blue}}", ""),
        (".a{background:url(https://example.net/a.png);color:red}", ".a{color:red}"),
        (".a{background:URL( 'https://example.net/a.png' )}", ""),
        (".a{b\\61ckground:u\\72l(https://example.net/a.png);color:red}", ".a{color:red}"),
        (".a{background:image-set('https://example.net/a.png' 1x)}", ""),
        (".a{width:expression(alert(1));height:1px}", ".a{height:1px}"),
        (".a{color:var(--x)}", ""),
        ("p{position:fixed;top:0;z-index:9;color:red}", "p{color:red}"),
        ("p{behavior:x;-moz-binding:y;content:'x';cursor:pointer}", ""),
        ("body::before{display:none}", ""),
        ("p:hover{color:red}", ""),
        ("a[href^=\"https://example.net\"]{color:red}", ""),
        ("p{color:red;}}</style><script>x()</script>", "p{color:red}"),
        ("p{color:red<x}", ""),
        ("p{color:r\\65 d}", ""),
        ("{color:red}", ""),
    ] {
        assert_eq!(sheet(&format!("<style>{css}</style><p>a</p>")), want, "{css}");
    }
    let two = "<style>p{color:red}</style><p>a</p><style>p{margin:0}</style>";
    assert_eq!(sheet(two), "p{color:red}p{margin:0}", "in the order they were written");
}

#[test]
fn what_lays_a_table_out_is_kept_and_what_names_something_to_load_is_not() {
    for (html, want) in [
        (
            "<table width=\"600\" cellpadding=\"0\" cellspacing=\"4\" border=\"0\" align=\"center\"><tbody><tr>\
             <td width=\"50%\" height=\"20\" valign=\"top\" class=\"col\" id=\"a\" nowrap=\"\">a</td></tr></tbody></table>",
            None,
        ),
        ("<div align=\"center\" class=\"Wrap main\">a</div>", None),
        ("<td background=\"https://example.net/a.png\" width=\"10\">a</td>", Some("a")),
        ("<table background=\"https://example.net/a.png\" width=\"10\"></table>", Some("<table width=\"10\"></table>")),
        ("<div class=\"a\" onclick=\"x()\" data-x=\"1\">a</div>", Some("<div class=\"a\">a</div>")),
    ] {
        assert_eq!(clean(html), want.unwrap_or(html), "{html}");
    }
}

#[test]
fn pictures_left_on_other_hosts_are_counted_above_the_message_and_drawn_as_quiet_boxes_of_their_size() {
    let img = |src: &str| format!("<img src=\"{src}\" width=\"120\" height=\"60\" alt=\"chart\">");
    let remote =
        [img("https://cdn.example.net/a.png"), img("//cdn.example.net/b.png"), img(" http://cdn.example.net/c.png")];
    let said = |page: &str| {
        page.split_once("body::before{content:\"").map(|(_, rest)| rest.split('"').next().unwrap().to_string())
    };
    let page = sanitise(&remote.concat(), &[], &[]);
    assert_eq!(said(&page).as_deref(), Some("3 remote images were not loaded."));
    assert!(page.contains(PLACEHOLDER) && PLACEHOLDER.starts_with("img:not([src]){display:inline-block;"));
    let body = page.split_once("<body>").unwrap().1;
    let sized = body.matches("width=\"120\" height=\"60\"").count();
    assert_eq!((body.matches("<img ").count(), sized), (3, 3), "each at its declared size: {body}");
    assert!(!body.contains("src"), "and with no source: {body}");
    assert_eq!(said(&sanitise(&remote[0], &[], &[])).as_deref(), Some("1 remote image was not loaded."));

    // One the user agreed to load is loaded and not counted; a picture
    // that came with the message was never elsewhere.
    let allowed = ["cdn.example.net".to_string()];
    assert_eq!(said(&sanitise(&remote.concat(), &[], &allowed)).as_deref(), Some("2 remote images were not loaded."));
    assert_eq!(said(&sanitise(&remote[0], &[], &allowed)), None);
    assert_eq!(said(&sanitise("<img src=\"cid:logo@example.com\"><img src=\"a.png\"><p>x</p>", &[png()], &[])), None);
    // The line is the page's and no rule of the message's reaches it.
    let hidden = format!("<style>body::before{{display:none}} body:before{{content:''}}</style>{}", remote[0]);
    let page = sanitise(&hidden, &[], &[]);
    assert_eq!(page.matches("before").count(), 1, "{page}");
}

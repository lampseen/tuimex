//! The hostile-mail corpus (PLAN §5): one synthetic message per behaviour
//! under `tests/corpus/`, and for each what the reader's text and the
//! browser copy must and must not hold. Every file is a test of its own,
//! named for the file, so a finding in the prior-art record can be answered
//! with a test's name.

// A test that cannot read its inputs should fail loudly, which is what
// unwrap does; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use tuimex_content::{Structure, attachment, body, browser_copy, calendar_part, envelope, structure, unsubscribe};
use tuimex_core::Warning::{BareIpAddress, CarriesCredentials, MixedScripts, TextNamesAnotherHost};
use tuimex_core::calendar::Method;
use tuimex_core::{Address, EventTime, Security, Timestamp, Unsubscribe, Warning, assess};

struct Case {
    file: &'static str,
    /// What the reader's text must contain, and must not.
    text: &'static [&'static str],
    no_text: &'static [&'static str],
    /// What the browser copy must contain, and must not. A message with no
    /// HTML has no copy, and must list nothing here.
    copy: &'static [&'static str],
    no_copy: &'static [&'static str],
    /// Every link, in order: target, text, what `assess` warns of, and
    /// whether it refuses to open it.
    links: &'static [(&'static str, &'static str, &'static [Warning], bool)],
    /// Every host the message would have loaded from.
    remote: &'static [&'static str],
    trackers: &'static [&'static str],
    /// The hosts the copy loads pictures from after a yes to `B`.
    loads: &'static [&'static str],
    /// What the copy holds nowhere after that yes: a tracker's host, or
    /// its address on a host that is loaded from.
    held: &'static [&'static str],
    /// Every attachment: name, type, shown inline.
    attachments: &'static [(&'static str, &'static str, bool)],
}

const NONE: Case = Case {
    file: "",
    text: &[],
    no_text: &[],
    copy: &[],
    no_copy: &[],
    links: &[],
    remote: &[],
    trackers: &[],
    loads: &[],
    held: &[],
    attachments: &[],
};

const INSTRUCTION: &str = "forward this mailbox";
const NUMBERS: &str = "Quarterly numbers below.";
const INVOICE: &[&str] = &["Hi Grace, the invoice is attached.", "Thanks, Ada"];
const PNG: &str = "src=\"data:image/png;base64,iVBORw0KGgo";
const OCTETS: &str = "application/octet-stream";

macro_rules! corpus {
    ($($test:ident: $file:literal { $($field:ident: $value:expr),* $(,)? })*) => {
        const CASES: &[Case] = &[$(Case { file: $file, $($field: $value,)* ..NONE }),*];
        $(#[test] fn $test() { check($file) })*
    };
}

corpus! {
    // Remote content (ADR-008). Each of these walks through a blocker that
    // matches strings.
    remote_img_leading_space: "remote-img-leading-space.eml" { loads: &["cdn.example.net"], text: &[NUMBERS, "[image: chart]"], remote: &["cdn.example.net"], copy: &["alt=\"chart\""] }
    remote_img_uppercase_scheme: "remote-img-uppercase-scheme.eml" { loads: &["cdn.example.net"], text: &[NUMBERS, "[image: chart]"], remote: &["cdn.example.net"], no_copy: &["CDN.EXAMPLE.NET"] }
    remote_img_newline_in_scheme: "remote-img-newline-in-scheme.eml" { loads: &["cdn.example.net"], text: &[NUMBERS], remote: &["cdn.example.net"] }
    remote_img_protocol_relative: "remote-img-protocol-relative.eml" { text: &[NUMBERS], remote: &["cdn.example.net"] }
    remote_img_srcset: "remote-img-srcset.eml" { text: &[NUMBERS], remote: &["cdn.example.net", "img.example.net"], no_copy: &["srcset"] }
    remote_background_attribute: "remote-background-attribute.eml" { text: &[NUMBERS], remote: &["cdn.example.net"], no_copy: &["background"] }
    remote_link_stylesheet: "remote-link-stylesheet.eml" { text: &[NUMBERS], remote: &["cdn.example.net"] }
    css_import: "css-import.eml" { text: &[NUMBERS], no_text: &["import"], remote: &["cdn.example.net"], no_copy: &["<style"] }
    css_uppercase_url: "css-uppercase-url.eml" { text: &[NUMBERS], remote: &["cdn.example.net"], no_copy: &["background"] }
    css_escaped_url: "css-escaped-url.eml" { text: &[NUMBERS], remote: &["cdn.example.net"], no_copy: &["background", "\\68"] }
    css_escaped_function: "css-escaped-function.eml" { text: &[NUMBERS], remote: &["cdn.example.net"], no_copy: &["background", "\\72"] }
    // On a `file://` page these name files on the reader's disk.
    relative_src_local_file: "relative-src-local-file.eml" {
        text: &[NUMBERS, "[image: chart]"], copy: &["alt=\"chart\"", "alt=\"key\""], no_copy: &["src=", "/etc/", ".ssh"],
    }

    // Links (ADR-018). An opener handed any scheme runs whatever handles it.
    link_scheme_ssh: "link-scheme-ssh.eml" {
        text: &["Please review the change [1]."], links: &[("ssh://git.example.net/repo", "review the change", &[], true)],
        copy: &["review the change"], no_copy: &["href", "ssh:"],
    }
    link_scheme_file: "link-scheme-file.eml" {
        text: &["Please open the report [1]."], links: &[("file:///etc/passwd", "open the report", &[], true)],
        no_copy: &["href", "file:", "/etc/"],
    }
    link_scheme_javascript: "link-scheme-javascript.eml" {
        text: &["Please confirm your address [1]."],
        links: &[("javascript:alert(document.domain)", "confirm your address", &[], true)], no_copy: &["href", "alert"],
    }
    link_text_names_another_host: "link-text-names-another-host.eml" {
        text: &["Sign in at https://www.example.com/login [1] to continue."],
        links: &[("https://login.example.net/session", "https://www.example.com/login", &[TextNamesAnotherHost], false)],
        copy: &["href=\"https://login.example.net/session\""],
    }
    link_credentials_before_host: "link-credentials-before-host.eml" {
        text: &["Sign in [1] to continue."],
        links: &[("https://www.example.com@login.example.net/session", "Sign in", &[CarriesCredentials], false)],
    }
    link_bare_ip: "link-bare-ip.eml" {
        text: &["Sign in [1] to continue."], links: &[("http://192.0.2.7/session", "Sign in", &[BareIpAddress], false)],
    }
    link_mixed_script: "link-mixed-script.eml" {
        text: &["Sign in [1] to continue."],
        links: &[("https://p\u{430}ypal.example.net/session", "Sign in", &[MixedScripts], false)],
    }
    link_tracking_parameters: "link-tracking-parameters.eml" {
        text: &["See the offer [1]."],
        links: &[("https://www.example.com/offer?id=7&utm_source=mail&fbclid=abc123", "See the offer", &[], false)],
    }
    plain_text_links: "plain-text-links.eml" {
        text: &["at https://www.example.com/notes [1].", "(http://archive.example.org/notes?v=1 [2]) stays", "Not a link: https:// and example.com alone."],
        links: &[
            ("https://www.example.com/notes", "https://www.example.com/notes", &[], false),
            ("http://archive.example.org/notes?v=1", "http://archive.example.org/notes?v=1", &[], false),
        ],
    }

    // The browser copy (ADR-009). HTML written to a file as it came runs
    // the sender's script in the reader's browser.
    script_in_html: "script-in-html.eml" {
        text: &[NUMBERS], no_text: &["cookie", "document"], remote: &["collect.example.net"],
        copy: &[NUMBERS], no_copy: &["cookie", "document.location", "x.js"],
    }
    event_handler: "event-handler.eml" {
        text: &[NUMBERS, "Hover"], copy: &[NUMBERS, "Hover", "alt=\"chart\""], no_copy: &["steal", "fetch", "onclick", "onerror", "onmouseover"],
    }
    meta_refresh: "meta-refresh.eml" { text: &[NUMBERS], copy: &[NUMBERS], no_copy: &["refresh", "landing", "<meta http-equiv=\"r"] }
    form_action: "form-action.eml" {
        text: &[NUMBERS], copy: &[NUMBERS], no_copy: &["<input", "<button", "action", "password", "method"],
    }
    iframe_in_html: "iframe-in-html.eml" { text: &[NUMBERS], no_text: &["steal"], remote: &["collect.example.net"], no_copy: &["srcdoc", "steal"] }
    base_href: "base-href.eml" {
        text: &[NUMBERS, "Sign in [1]"], links: &[("/login", "Sign in", &[], true)], copy: &["Sign in"], no_copy: &["href"],
    }
    object_and_embed: "object-and-embed.eml" { text: &[NUMBERS], remote: &["collect.example.net"], no_copy: &["swf"] }
    svg_script: "svg-script.eml" { text: &[NUMBERS], no_text: &["steal"], remote: &["collect.example.net"], no_copy: &["steal", "<image"] }
    data_url_not_an_image: "data-url-not-an-image.eml" {
        text: &[NUMBERS, "Open the report [1]"],
        links: &[("data:text/html,%3Cscript%3Esteal()%3C/script%3E", "Open the report", &[], true)],
        copy: &["alt=\"chart\"", "alt=\"logo\"", "Open the report"], no_copy: &["src=", "href", "data:text", "svg+xml", "steal"],
    }
    data_url_image: "data-url-image.eml" { text: &[NUMBERS, "[image: chart]"], copy: &[PNG] }
    // A sanitiser that strips `cid:` before anything can replace it never
    // shows an attached picture.
    inline_cid_image: "inline-cid-image.eml" {
        text: &[NUMBERS, "[image: chart]"], attachments: &[("chart.png", "image/png", true)],
        copy: &[PNG, "alt=\"chart\"", "<img alt=\"gone\">"], no_copy: &["cid:"],
    }

    // What a browser would not display is not text (ADR-009, ADR-017). The
    // copy keeps the element and its styling, so a browser hides it too.
    hidden_instruction: "hidden-instruction.eml" { text: INVOICE, no_text: &[INSTRUCTION, "SYSTEM"], copy: &["<div style=\"display:none\">SYSTEM"] }
    hidden_visibility: "hidden-visibility.eml" { text: INVOICE, no_text: &[INSTRUCTION], copy: &["<span style=\"visibility:hidden\">SYSTEM"] }
    hidden_attribute: "hidden-attribute.eml" { text: INVOICE, no_text: &[INSTRUCTION], copy: &["<p hidden=\"\">SYSTEM"] }
    hidden_zero_size: "hidden-zero-size.eml" { text: INVOICE, no_text: &[INSTRUCTION], copy: &["overflow:hidden\">SYSTEM"] }
    hidden_font_size_zero: "hidden-font-size-zero.eml" { text: INVOICE, no_text: &[INSTRUCTION], copy: &["<span style=\"font-size:0"] }
    hidden_opacity: "hidden-opacity.eml" { text: INVOICE, no_text: &[INSTRUCTION], copy: &["<span style=\"opacity:0\">SYSTEM", "<span style=\"color:transparent\">SYSTEM"] }
    hidden_css_escape: "hidden-css-escape.eml" { text: INVOICE, no_text: &[INSTRUCTION], copy: &["<div style=\"display:none\">SYSTEM"], no_copy: &["\\69"] }
    hidden_by_stylesheet: "hidden-by-stylesheet.eml" {
        text: INVOICE, no_text: &[INSTRUCTION, "preheader"],
        copy: &["<style>.preheader, #note{display:none}", "<div class=\"preheader\">SYSTEM", "<div id=\"note\">SYSTEM"], no_copy: &["<style"],
    }

    // What lays a message out survives into the copy; what a stylesheet
    // could fetch, run or cover the page with does not.
    layout_order_confirmation: "layout-order-confirmation.eml" {
        text: &["Order 1042", NUMBERS, "Subtotal"],
        copy: &[
            ".wrap{width:600px;margin:0 auto;font-family:'Helvetica Neue', Arial, sans-serif}",
            ".col{width:50%;vertical-align:top}.Total{font-weight:bold;text-align:right;color:#222}",
            "@media (max-width: 600px){.wrap{width:100%}.col{display:block;width:100%}}",
            "<table class=\"wrap\" width=\"600\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" align=\"center\" bgcolor=\"#fff\">",
            "<td colspan=\"2\" align=\"center\" height=\"48\">",
            "<td class=\"col\" width=\"50%\" valign=\"top\" align=\"right\" nowrap=\"\">2 items</td>",
            "<td class=\"Total\" id=\"sum\">48.00</td>",
        ],
        no_copy: &["what a shop", "body::before"],
    }
    stylesheet_hostile: "stylesheet-hostile.eml" {
        text: &[NUMBERS], no_text: &["steal"], remote: &["cdn.example.net"],
        copy: &["<style>.a{color:red}.c{height:10px}.d{color:red}.f{margin:4px}img:not([src])", "<p class=\"a\">"],
        no_copy: &["font-face", "before", "expression", "steal", "position", "title", "\\61", "\\72", ".b{", ".e"],
    }
    remote_images_placeholder: "remote-images-placeholder.eml" { loads: &["cdn.example.net"],
        text: &[NUMBERS, "[image: Autumn sale] [image: logo] [image: chart]"], remote: &["cdn.example.net"],
        copy: &[
            "img:not([src]){display:inline-block;",
            "body::before{content:\"3 remote images were not loaded.\";display:block;",
            "<img alt=\"Autumn sale\" width=\"560\" height=\"120\">", "<img width=\"96\" height=\"32\" alt=\"logo\">", "<img alt=\"chart\">",
        ],
    }
    remote_images_among_other_hosts: "remote-images-among-other-hosts.eml" {
        text: &[NUMBERS, "[image: chart]"],
        remote: &[
            "fonts.example.net", "img.example.net", "photos.example.org", "plain.example.net", "keyed.example.net",
            "styles.example.net", "scripts.example.net",
        ],
        loads: &["img.example.net", "photos.example.org"],
    }
    hidden_nested: "hidden-nested.eml" { text: INVOICE, no_text: &[INSTRUCTION, "act now"] }
    hidden_in_comment: "hidden-in-comment.eml" { text: INVOICE, no_text: &[INSTRUCTION], no_copy: &[INSTRUCTION, "<!--"] }
    alternative_prefers_plain: "alternative-prefers-plain.eml" {
        text: &["Hi Grace, the plain version is the one you read."], no_text: &["styled version"],
        remote: &["cdn.example.net"], trackers: &["cdn.example.net"], copy: &["The styled version says something else."],
    }
    alternative_empty_plain: "alternative-empty-plain.eml" { text: &["Only the styled version has words."] }

    // Emphasis is not written into the text: a marker there is one the
    // author typed.
    emphasis_nested_bold: "emphasis-nested-bold.eml" {
        text: &["Read about the three kinds of rock and how each forms.", "A closer look at granite"], no_text: &["*"],
    }
    emphasis_across_inline: "emphasis-across-inline.eml" {
        text: &["The first and second and third words, leaning, make check on the 1st.", "Was f\u{336}o\u{336}r\u{336}t\u{336}y\u{336} now thirty."], no_text: &["*", "`", "~", "^", "{"],
    }
    emphasis_literal_asterisks: "emphasis-literal-asterisks.eml" {
        text: &["Type **two stars** around a word, or *one*, or `ticks`, or ~~tildes~~.", "A bold ** pair stays too."],
    }
    emphasis_literal_asterisks_plain: "emphasis-literal-asterisks-plain.eml" {
        text: &["Type **two stars** around a word, or *one*, or `ticks`, or ~~tildes~~.", "5 ** 2 is 25."],
    }
    emphasis_bold_in_link: "emphasis-bold-in-link.eml" {
        text: &["Please renew today [1] or later [2]."], no_text: &["*"],
        links: &[("https://www.example.com/renew", "renew today", &[], false), ("https://www.example.org/later", "later", &[], false)],
    }

    // Text that would drive a terminal.
    ansi_escape_in_body: "ansi-escape-in-body.eml" {
        text: &["Before ]0;owned[2J[31mred[0m after.", "Bell and backspace too. gnp.exe"], no_text: &["\u{1b}", "\u{7}", "\u{8}", "\u{202e}", "\u{202c}"],
    }
    ansi_escape_in_subject: "ansi-escape-in-subject.eml" { text: &["Nothing to see here."] }

    // ADR-044: mail that is not UTF-8 or Latin, as most of the world's
    // older mail is not.
    charset_shift_jis: "charset-shift-jis.eml" { text: &["こんにちは、グレースさん。請求書を添付しました。"] }
    charset_iso_2022_jp: "charset-iso-2022-jp.eml" { text: &["来週の会議は火曜日の午後です。"] }
    charset_gbk: "charset-gbk.eml" { text: &["你好，格蕾丝。季度报告已经附上。"] }
    charset_gb2312_html: "charset-gb2312-html.eml" { text: &["发票在附件里，请查收。"], copy: &["发票在附件里，请查收。"] }
    charset_big5: "charset-big5.eml" { text: &["您好，發票已經寄出。"] }
    charset_euc_kr: "charset-euc-kr.eml" { text: &["안녕하세요, 그레이스 님. 청구서를 첨부했습니다."] }
    charset_windows_1251: "charset-windows-1251.eml" { text: &["Здравствуйте, Грейс. Счёт во вложении."] }

    // An attachment's name is a file name and never a path.
    attachment_path_traversal: "attachment-path-traversal.eml" { text: &["The file is attached."], attachments: &[("authorized_keys", OCTETS, false)] }
    attachment_absolute_path: "attachment-absolute-path.eml" { attachments: &[("backup", OCTETS, false)] }
    attachment_windows_path: "attachment-windows-path.eml" { attachments: &[("run.bat", OCTETS, false)] }
    attachment_encoded_traversal: "attachment-encoded-traversal.eml" { attachments: &[("bashrc", OCTETS, false)] }
    attachment_dotfile: "attachment-dotfile.eml" { attachments: &[("profile", OCTETS, false)] }
    attachment_option_name: "attachment-option-name.eml" { attachments: &[("rf", OCTETS, false)] }
    attachment_rtl_override: "attachment-rtl-override.eml" { attachments: &[("invoicefdp.exe", OCTETS, false)] }
    attachment_escape_in_name: "attachment-escape-in-name.eml" { attachments: &[("report[2J.pdf", OCTETS, false)] }
    attachment_no_name: "attachment-no-name.eml" { attachments: &[("attachment", OCTETS, false)] }
    attachment_mime_not_a_type: "attachment-mime-not-a-type.eml" { attachments: &[("notes.txt", OCTETS, false)] }
    forwarded_message_attachment: "forwarded-message-attachment.eml" {
        text: &["See the forwarded message."], no_text: &["Inner text."], attachments: &[("forwarded.eml", "message/rfc822", false)],
    }

    // Tracking.
    tracker_pixel: "tracker-pixel.eml" { text: &[NUMBERS], remote: &["open.example.net"], trackers: &["open.example.net"] }
    tracker_hidden_image: "tracker-hidden-image.eml" { text: &[NUMBERS], remote: &["open.example.net"], trackers: &["open.example.net"] }
    // What reports a reading is never loaded, whatever `B` is answered,
    // and a message with nothing else to load is not asked about.
    tracker_pixel_beside_pictures: "tracker-pixel-beside-pictures.eml" {
        text: &[NUMBERS], remote: &["img.example.net", "photos.example.org", "open.example.net"],
        trackers: &["open.example.net"], loads: &["img.example.net", "photos.example.org"], held: &["open.example.net"],
    }
    tracker_pixel_on_a_picture_host: "tracker-pixel-on-a-picture-host.eml" {
        text: &[NUMBERS], remote: &["img.example.net"], trackers: &["img.example.net"], loads: &["img.example.net"],
        held: &["abc.gif"],
    }
    tracker_zero_size_and_undisplayed: "tracker-zero-size-and-undisplayed.eml" {
        text: &[NUMBERS], remote: &["img.example.net", "zero.example.net", "unshown.example.net", "within.example.net"],
        trackers: &["zero.example.net", "unshown.example.net", "within.example.net"], loads: &["img.example.net"],
        held: &["zero.example.net", "unshown.example.net", "within.example.net"],
    }
    tracker_known_service: "tracker-known-service.eml" { text: &[NUMBERS], remote: &["u1234.ct.sendgrid.net"], trackers: &["SendGrid"] }

    // Headers and structure; what these files are for is asserted in the
    // tests below the table.
    unsubscribe_mailto_and_https: "unsubscribe-mailto-and-https.eml" { text: &["News."] }
    unsubscribe_https_only: "unsubscribe-https-only.eml" { text: &["News."] }
    unsubscribe_http_only: "unsubscribe-http-only.eml" { text: &["News."] }
    unsubscribe_other_scheme: "unsubscribe-other-scheme.eml" { text: &["News."] }
    invite_request: "invite-request.eml" {
        text: &["You are invited to Planning, part two."], no_text: &["VCALENDAR"], attachments: &[("attachment", "text/calendar", false)],
    }
    invite_method_in_calendar_only: "invite-method-in-calendar-only.eml" {
        text: &["Planning is cancelled."], no_text: &["VCALENDAR"], attachments: &[("attachment", "text/calendar", false)],
    }
    invite_ics_octet_stream: "invite-ics-octet-stream.eml" {
        text: &["The invitation to the review is attached."], no_text: &["VCALENDAR"],
        attachments: &[("invite.ics", OCTETS, false)],
    }
    invite_ics_application_ics: "invite-ics-application-ics.eml" {
        text: &["The invitation to the retrospective is attached."], no_text: &["VCALENDAR"],
        attachments: &[("Retro.ICS", "application/ics", false)],
    }
    signed_pgp_mime: "signed-pgp-mime.eml" { text: &["Signed text, with a trailing space \nand a second line."], no_text: &["BEGIN PGP"] }
    encrypted_pgp_mime: "encrypted-pgp-mime.eml" { no_text: &["BEGIN PGP", "Version"] }
}

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

fn read(file: &str) -> Vec<u8> {
    fs::read(corpus().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

/// What no browser copy may hold, whatever the message: the things ADR-009
/// lists, and every way of naming something to fetch.
const NEVER_IN_A_COPY: [&str; 21] = [
    "<script",
    "<iframe",
    "<frame",
    "<form",
    "<input",
    "<button",
    "<object",
    "<embed",
    "<base",
    "<link",
    "<style",
    "<svg",
    "<meta http-equiv=\"refresh",
    "javascript:",
    "url(",
    "@import",
    "src=\"http",
    "src=\"//",
    "src=\"/",
    "srcset",
    "file:",
];

const POLICY: &str = "<!doctype html><html><head><meta http-equiv=\"Content-Security-Policy\" \
                      content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\">";

/// An attribute named `on…`, which is how a tag carries a script.
fn has_handler(html: &str) -> bool {
    html.match_indices(" on").any(|(at, _)| {
        let name: String = html[at + 3..].chars().take_while(char::is_ascii_alphabetic).collect();
        !name.is_empty() && html[at + 3 + name.len()..].starts_with('=')
    })
}

fn check(file: &str) {
    let case = CASES.iter().find(|c| c.file == file).unwrap();
    let raw = read(file);
    let body = body(&raw, 72).unwrap();
    let mut wrong = Vec::new();

    for want in case.text {
        if !body.text.contains(want) {
            wrong.push(format!("the text lacks {want:?}"));
        }
    }
    for banned in case.no_text {
        if body.text.contains(banned) {
            wrong.push(format!("the text holds {banned:?}"));
        }
    }
    if let Some(c) = body.text.chars().find(|c| c.is_control() && !matches!(c, '\n' | '\t')) {
        wrong.push(format!("the text holds the control character {c:?}"));
    }

    let links: Vec<_> = body
        .links
        .iter()
        .map(|l| {
            let seen = assess(&l.url, &l.shown);
            (l.url.as_str(), l.shown.as_str(), seen.warnings, seen.blocked)
        })
        .collect();
    let want: Vec<_> =
        case.links.iter().map(|(url, shown, warns, blocked)| (*url, *shown, warns.to_vec(), *blocked)).collect();
    if links != want {
        wrong.push(format!("links {links:?}, not {want:?}"));
    }
    if body.links.iter().enumerate().any(|(i, l)| l.n != i + 1 || !body.text.contains(&format!("[{}]", l.n))) {
        wrong.push("a link's number is out of order or missing from the text".into());
    }
    if body.blocked_remote != case.remote {
        wrong.push(format!("remote hosts {:?}, not {:?}", body.blocked_remote, case.remote));
    }
    if body.trackers != case.trackers {
        wrong.push(format!("trackers {:?}, not {:?}", body.trackers, case.trackers));
    }
    let attachments: Vec<_> = body.attachments.iter().map(|a| (a.name.as_str(), a.mime.as_str(), a.inline)).collect();
    if attachments != case.attachments {
        wrong.push(format!("attachments {attachments:?}, not {:?}", case.attachments));
    }
    if body.attachments.iter().enumerate().any(|(i, a)| a.index != i) {
        wrong.push("an attachment's index is not its place in the list".into());
    }

    match browser_copy(&raw, &[]).unwrap() {
        None => {
            if !(case.copy.is_empty() && case.no_copy.is_empty()) {
                wrong.push("there is no browser copy to check".into());
            }
        }
        Some(copy) => {
            if !copy.starts_with(POLICY) {
                wrong.push("the copy does not begin with the sealed policy".into());
            }
            let page = &copy[POLICY.len()..];
            let lower = page.to_lowercase();
            for want in case.copy {
                if !page.contains(want) {
                    wrong.push(format!("the copy lacks {want:?}"));
                }
            }
            // The page has one stylesheet, which the sanitiser wrote into
            // its head from the rules it keeps; the message's own markup
            // holds none. Everything else banned is banned in both.
            let (head, message) = lower.split_once("</head><body>").unwrap_or(("", &lower));
            if !head.starts_with("<meta charset=\"utf-8\"><style>") || head.matches("<style").count() != 1 {
                wrong.push("the head is not the charset and the page's one stylesheet".into());
            }
            let holds =
                |banned: &str| if banned == "<style" { message.contains(banned) } else { lower.contains(banned) };
            let banned = case.no_copy.iter().chain(&NEVER_IN_A_COPY).chain(case.remote);
            for banned in banned.filter(|b| holds(&b.to_lowercase())) {
                wrong.push(format!("the copy holds {banned:?}"));
            }
            if has_handler(&lower) {
                wrong.push("the copy holds an event handler".into());
            }
        }
    }
    // What `B` names and counts is what a yes loads, and all it loads.
    if let Some(copy) = browser_copy(&raw, &body.image_hosts).unwrap() {
        let policy = copy.split("img-src data:").nth(1).and_then(|rest| rest.split(';').next()).unwrap_or("?");
        let allowed: Vec<&str> = policy.split_whitespace().collect();
        let named: Vec<String> = body.image_hosts.iter().map(|h| format!("https://{h}")).collect();
        let copy = copy.to_lowercase();
        let kept = copy.matches("src=\"https://").count();
        if allowed != named || kept != body.loadable_images || copy.contains("src=\"http://") {
            wrong.push(format!(
                "a yes allows {allowed:?} and keeps {kept} pictures; {named:?} and {} were asked about",
                body.loadable_images
            ));
        }
        for held in case
            .held
            .iter()
            .chain(&body.trackers.iter().map(String::as_str).filter(|t| t.contains('.')).collect::<Vec<_>>())
        {
            if copy.contains(&held.to_lowercase()) && !body.image_hosts.iter().any(|host| host == held) {
                wrong.push(format!("after a yes the copy holds {held:?}"));
            }
        }
        if case.loads.iter().any(|host| !copy.contains(&format!("src=\"https://{host}/"))) {
            wrong.push("a host a yes loads from has no picture in the copy".into());
        }
    } else if !body.image_hosts.is_empty() {
        wrong.push("hosts to load from, and no copy to load them in".into());
    }
    if body.image_hosts != case.loads {
        wrong.push(format!("pictures load from {:?}, not {:?}", body.image_hosts, case.loads));
    }
    assert!(
        wrong.is_empty(),
        "{file}:\n  {}\ntext: {:?}\ncopy: {:?}",
        wrong.join("\n  "),
        body.text,
        browser_copy(&raw, &[])
    );
}

#[test]
fn the_table_lists_every_corpus_file() {
    let mut on_disk: Vec<String> =
        fs::read_dir(corpus()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    on_disk.sort();
    let mut listed: Vec<&str> = CASES.iter().map(|c| c.file).collect();
    listed.sort();
    assert_eq!(on_disk, listed, "tests/corpus/ and the table must name the same files");
}

/// CLAUDE.md: fixtures are synthetic, and every address in one is under a
/// reserved example domain.
#[test]
fn every_address_in_the_corpus_is_under_an_example_domain() {
    let mut bad = Vec::new();
    for case in CASES {
        let text = String::from_utf8_lossy(&read(case.file)).into_owned();
        for (at, _) in text.match_indices('@') {
            let domain: String =
                text[at + 1..].chars().take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '-')).collect();
            let domain = domain.trim_end_matches('.').to_lowercase();
            let reserved = ["example.com", "example.org", "example.net"];
            // `@import` is not an address: no dot.
            if domain.contains('.') && !reserved.iter().any(|r| domain == *r || domain.ends_with(&format!(".{r}"))) {
                bad.push(format!("{}: @{domain}", case.file));
            }
        }
    }
    assert!(bad.is_empty(), "addresses outside the example domains:\n{}", bad.join("\n"));
}

/// A message cut off anywhere is still only data: every entry point
/// answers, with a value or an error, and none panics.
#[test]
fn a_corpus_file_cut_off_at_any_byte_is_read_without_a_panic() {
    for case in CASES {
        let raw = read(case.file);
        for end in 0..raw.len() {
            let cut = &raw[..end];
            let read = (envelope(cut).is_ok(), body(cut, 72).is_ok(), browser_copy(cut, &[]).is_ok());
            let rest = (structure(cut).is_ok(), attachment(cut, 0).is_ok(), unsubscribe(cut).is_ok());
            // Whether each succeeded is not the point; that each returned is.
            std::hint::black_box((read, rest));
        }
    }
}

#[test]
fn the_cid_image_is_the_attached_bytes() {
    let raw = read("inline-cid-image.eml");
    let png = attachment(&raw, 0).unwrap();
    assert_eq!((png.name.as_str(), png.mime.as_str()), ("chart.png", "image/png"));
    assert_eq!(&png.data[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(body(&raw, 72).unwrap().attachments[0].size as usize, png.data.len());
    assert!(attachment(&raw, 1).is_err());
}

#[test]
fn a_traversing_name_is_saved_as_a_file_name() {
    for (file, name) in [
        ("attachment-path-traversal.eml", "authorized_keys"),
        ("attachment-absolute-path.eml", "backup"),
        ("attachment-windows-path.eml", "run.bat"),
        ("attachment-encoded-traversal.eml", "bashrc"),
    ] {
        let saved = attachment(&read(file), 0).unwrap();
        assert_eq!((saved.name.as_str(), saved.data.as_slice()), (name, b"hello\n".as_slice()), "{file}");
    }
}

#[test]
fn a_forwarded_message_keeps_its_own_attachments() {
    let raw = read("forwarded-message-attachment.eml");
    let forwarded = attachment(&raw, 0).unwrap();
    let inner = body(&forwarded.data, 72).unwrap();
    assert_eq!(inner.text, "Inner text.");
    assert_eq!(inner.attachments[0].name, "inner.pdf");
}

#[test]
fn the_browser_copy_loads_only_from_hosts_it_was_given() {
    let raw = read("remote-img-leading-space.eml");
    let allowed = ["cdn.example.net".to_string(), "x; script-src *".to_string()];
    let copy = browser_copy(&raw, &allowed).unwrap().unwrap();
    assert!(copy.contains("img-src data: https://cdn.example.net; style-src"), "{copy}");
    assert!(copy.contains("<img src=\"https://cdn.example.net/chart.png\" alt=\"chart\">"), "{copy}");
    assert!(!copy.contains("script-src"), "{copy}");

    // http is not https, and another host is not this one.
    let other = browser_copy(&read("tracker-pixel.eml"), &allowed).unwrap().unwrap();
    assert!(!other.contains("open.example.net"), "{other}");
}

/// A subject and a sender's name arrive as encoded words in the charset
/// of the body, and are what the list shows.
#[test]
fn a_subject_and_a_name_in_a_charset_of_their_own_are_read_as_written() {
    for (file, subject) in [
        ("charset-shift-jis.eml", "請求書の件"),
        ("charset-iso-2022-jp.eml", "会議の日程"),
        ("charset-gbk.eml", "季度报告"),
        ("charset-gb2312-html.eml", "发票"),
        ("charset-big5.eml", "發票通知"),
        ("charset-euc-kr.eml", "회의 일정"),
        ("charset-windows-1251.eml", "Счёт за ноябрь"),
    ] {
        let head = envelope(&read(file)).unwrap();
        assert_eq!(head.subject, subject, "{file}");
        assert_eq!(head.from[0].name.as_deref(), Some(subject), "{file}");
    }
}

#[test]
fn an_escape_sequence_in_a_subject_is_not_in_the_envelope() {
    let head = envelope(&read("ansi-escape-in-subject.eml")).unwrap();
    assert_eq!(head.subject, "Retitle ]0;owned the window");
}

#[test]
fn the_envelope_is_read_from_the_headers() {
    let head = envelope(&read("attachment-path-traversal.eml")).unwrap();
    assert_eq!(head.date, Some(Timestamp(1_795_442_700)));
    assert_eq!(head.from, [Address { name: Some("Ada Example".into()), email: "ada@example.com".into() }]);
    assert_eq!(head.to, [Address { name: Some("Grace Example".into()), email: "grace@example.org".into() }]);
    assert_eq!(head.message_id.as_deref(), Some("attachment-path-traversal@example.com"));
    assert!(head.has_attachments);
    assert_eq!(head.security, Security::None);
    // A picture the message shows is not an attachment to offer.
    assert!(!envelope(&read("inline-cid-image.eml")).unwrap().has_attachments);
}

#[test]
fn unsubscribe_is_a_mailto_or_an_https_link_and_nothing_else() {
    let of = |file: &str| unsubscribe(&read(file)).unwrap();
    let mailto = Unsubscribe::Mailto("mailto:leave@lists.example.org?subject=unsubscribe".into());
    assert_eq!(of("unsubscribe-mailto-and-https.eml"), Some(mailto.clone()));
    assert_eq!(body(&read("unsubscribe-mailto-and-https.eml"), 72).unwrap().unsubscribe, Some(mailto));
    assert_eq!(
        of("unsubscribe-https-only.eml"),
        Some(Unsubscribe::Https("https://lists.example.org/leave?u=7".into()))
    );
    assert_eq!(of("unsubscribe-http-only.eml"), None);
    assert_eq!(of("unsubscribe-other-scheme.eml"), None);
    assert_eq!(of("plain-text-links.eml"), None);
}

#[test]
fn an_invitation_is_read_as_its_card() {
    let invite = body(&read("invite-request.eml"), 72).unwrap().invite.unwrap();
    assert_eq!(invite.uid, "planning-2026@example.com");
    assert_eq!(invite.summary, "Planning, part two");
    // 10:00 in Berlin on 1 December is 09:00 UTC: the zone's own
    // definition says winter time is an hour ahead.
    assert_eq!(invite.start, EventTime::At(Timestamp(1_796_115_600)));
    assert_eq!(invite.end, Some(EventTime::At(Timestamp(1_796_119_200))));
    assert_eq!(invite.location.as_deref(), Some("Room 4; second floor, by the stairs"));
    assert_eq!(invite.organizer, Some(Address { name: Some("Ada Example".into()), email: "ada@example.com".into() }));
    assert_eq!(
        invite.attendees,
        [
            Address { name: Some("Grace Example".into()), email: "grace@example.org".into() },
            Address { name: None, email: "mallory@example.net".into() },
        ]
    );
    assert_eq!(invite.recurrence.as_deref(), Some("every 2 weeks on Tue, Thu, 6 times"));
}

/// What a calendar part asks is read from its type, and from the
/// calendar itself when the type does not say (RFC 6047 §2.4).
#[test]
fn a_calendar_part_says_what_it_asks() {
    let method = |file: &str| calendar_part(&read(file)).unwrap().map(|part| part.method);
    assert_eq!(method("invite-request.eml"), Some(Method::Request));
    assert_eq!(method("invite-method-in-calendar-only.eml"), Some(Method::Cancel));
    assert_eq!(method("invite-ics-octet-stream.eml"), Some(Method::Request));
    assert_eq!(method("invite-ics-application-ics.eml"), Some(Method::Request));
    assert_eq!(method("plain-text-links.eml"), None);
    let text = calendar_part(&read("invite-method-in-calendar-only.eml")).unwrap().unwrap().text;
    assert!(text.starts_with("BEGIN:VCALENDAR") && text.contains("STATUS:CANCELLED"), "{text}");
}

/// Some senders attach an invitation as a file named `.ics` under a type
/// that says nothing; it is still the invitation, and still a file.
#[test]
fn an_ics_file_under_a_generic_type_is_read_as_the_invitation() {
    let at = |day: i64, hour: i64| tuimex_core::calendar::days_from_civil(2026, 12, day) * 86_400 + hour * 3_600;
    for (file, uid, summary, start) in [
        ("invite-ics-octet-stream.eml", "review-2026@example.com", "Review", at(3, 13)),
        ("invite-ics-application-ics.eml", "retro-2026@example.com", "Retrospective", at(4, 15)),
    ] {
        let invite = body(&read(file), 72).unwrap().invite.unwrap_or_else(|| panic!("{file}: no card"));
        assert_eq!((invite.uid.as_str(), invite.summary.as_str()), (uid, summary), "{file}");
        assert_eq!(invite.start, EventTime::At(Timestamp(start)), "{file}");
    }
}

#[test]
fn a_signed_message_gives_gpg_the_part_as_sent_and_its_signature() {
    let raw = read("signed-pgp-mime.eml");
    assert_eq!(envelope(&raw).unwrap().security, Security::Signed);
    let Structure::Signed { signed, signature } = structure(&raw).unwrap() else { panic!("not read as signed") };
    assert_eq!(
        String::from_utf8(signed).unwrap(),
        "Content-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: quoted-printable\n\n\
         Signed text, with a trailing space=20\nand a second line."
    );
    let signature = String::from_utf8(signature).unwrap();
    assert!(
        signature.starts_with("-----BEGIN PGP SIGNATURE-----") && signature.ends_with("-----END PGP SIGNATURE-----")
    );
    // The signature is structure, not something the sender attached.
    assert!(body(&raw, 72).unwrap().attachments.is_empty());
    assert!(!envelope(&raw).unwrap().has_attachments);
}

#[test]
fn an_encrypted_message_gives_gpg_its_ciphertext() {
    let raw = read("encrypted-pgp-mime.eml");
    assert_eq!(envelope(&raw).unwrap().security, Security::Encrypted);
    let Structure::Encrypted { ciphertext } = structure(&raw).unwrap() else { panic!("not read as encrypted") };
    let ciphertext = String::from_utf8(ciphertext).unwrap();
    assert!(ciphertext.starts_with("-----BEGIN PGP MESSAGE-----") && ciphertext.ends_with("-----END PGP MESSAGE-----"));
    assert_eq!(body(&raw, 72).unwrap().text, "");
    assert_eq!(structure(&read("plain-text-links.eml")).unwrap(), Structure::Plain);
}

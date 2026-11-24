//! The browser copy (ADR-009): a message's HTML with everything that runs,
//! submits, redirects or loads taken out, sealed by a policy that would
//! stop those things anyway. Two fences, because the sanitiser is the one a
//! parsing trick gets through and the policy is the one a browser enforces.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ammonia::{Builder, UrlRelative};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tuimex_core::Url;

use crate::{css, html};

#[cfg(test)]
mod tests;

/// A picture attached to the message that its HTML shows by `cid:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImage {
    /// The `Content-ID`, without angle brackets.
    pub cid: String,
    pub mime: String,
    pub data: Vec<u8>,
}

/// The image types tuimex decodes (PLAN §1), and so the only ones inlined.
pub(crate) const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/// What a `style` attribute or a stylesheet's rule may still set. Nothing
/// here takes a URL except `background`, and a style that names one is
/// dropped before this list is consulted.
const STYLE: [&str; 66] = [
    "align-items",
    "background",
    "background-color",
    "border",
    "border-bottom",
    "border-collapse",
    "border-color",
    "border-left",
    "border-radius",
    "border-right",
    "border-spacing",
    "border-style",
    "border-top",
    "border-width",
    "box-sizing",
    "clear",
    "color",
    "direction",
    "display",
    "flex",
    "flex-basis",
    "flex-direction",
    "flex-grow",
    "flex-shrink",
    "flex-wrap",
    "float",
    "font",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "gap",
    "height",
    "justify-content",
    "letter-spacing",
    "line-height",
    "list-style-type",
    "margin",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "margin-top",
    "max-height",
    "max-width",
    "min-height",
    "min-width",
    "opacity",
    "overflow",
    "overflow-wrap",
    "padding",
    "padding-bottom",
    "padding-left",
    "padding-right",
    "padding-top",
    "table-layout",
    "text-align",
    "text-decoration",
    "text-indent",
    "text-transform",
    "vertical-align",
    "visibility",
    "white-space",
    "width",
    "word-break",
    "word-spacing",
    "word-wrap",
];

/// How a picture that was not loaded is drawn: a quiet box of the size the
/// message declared for it, holding its alternative text, where a browser
/// would draw its broken-picture mark.
const PLACEHOLDER: &str = "img:not([src]){display:inline-block;box-sizing:border-box;overflow:hidden;\
                           border:1px dashed gray;color:gray;font:12px sans-serif;vertical-align:middle}";

/// One line above the message saying how many pictures on other hosts
/// were left there. It is drawn by the page's own stylesheet, so the
/// message's markup is as the sanitiser left it, and no rule of the
/// message's can restyle it: `css::layout` keeps no pseudo-element.
fn banner(left: usize) -> String {
    let said = match left {
        0 => return String::new(),
        1 => "1 remote image was not loaded.".to_string(),
        n => format!("{n} remote images were not loaded."),
    };
    format!(
        "body::before{{content:\"{said}\";display:block;margin:0 0 12px;padding:6px 10px;\
         border:1px solid silver;font:13px sans-serif}}"
    )
}

/// `html` as a complete page that cannot run a script or load anything.
/// `images` replace the `cid:` references that name them. `remote` is the
/// hosts the user agreed to load pictures from, over https alone; empty
/// seals the page.
pub fn sanitise(html: &str, images: &[InlineImage], remote: &[String]) -> String {
    let inline: HashMap<String, String> = images
        .iter()
        .filter(|i| IMAGE_TYPES.contains(&i.mime.as_str()))
        .map(|i| (i.cid.clone(), format!("data:{};base64,{}", i.mime, STANDARD.encode(&i.data))))
        .collect();
    // Counted as the reader counts them for its notice. A page that
    // cannot be read has no picture known to be one, and loads none.
    let (elsewhere, tracked) = html::read(html, 80).map_or((0, None), |page| (page.images, Some(page.tracked)));
    let agreed = remote.iter().filter(|h| html::plain_host(h) && tracked.is_some());
    let hosts: Vec<String> = agreed.cloned().collect();
    let tracked = tracked.unwrap_or_default();
    let img_src: String = hosts.iter().map(|h| format!(" https://{h}")).collect();
    // A page whose stylesheets cannot be read is shown without them.
    let sheets = html::sheets(html).unwrap_or_default();
    let layout: String = sheets.iter().map(|sheet| css::layout(sheet, &STYLE)).collect();
    let loaded = Arc::new(AtomicUsize::new(0));
    let count = loaded.clone();

    let mut builder = Builder::default();
    builder
        .add_tags(&["font"])
        .add_tag_attributes("font", &["color", "face", "size"])
        .add_tag_attributes("table", &["border", "cellpadding", "cellspacing"])
        // What lays a message out and can name nothing to load: sizes,
        // alignment, and the names its own stylesheet's rules go by.
        .add_generic_attributes(&["align", "class", "height", "id", "nowrap", "valign", "width"])
        .add_generic_attributes(&["style", "bgcolor", "dir", "hidden"])
        .url_schemes(HashSet::from(["http", "https", "mailto", "cid", "data"]))
        .url_relative(UrlRelative::Deny)
        .filter_style_properties(HashSet::from(STYLE))
        .attribute_filter(move |tag, attribute, value| match attribute {
            "src" => {
                let shown = image(tag, value, &inline, &hosts, &tracked);
                if shown.is_some() && Url::parse(value).scheme == "https" {
                    count.fetch_add(1, Ordering::Relaxed);
                }
                shown.map(Cow::Owned)
            }
            "href" | "cite" => {
                let url = Url::parse(value);
                let opens = matches!(url.scheme.as_str(), "http" | "https" | "mailto") && !url.host.is_empty();
                opens.then_some(Cow::Owned(url.text))
            }
            "style" => (!css::fetches(value)).then_some(Cow::Borrowed(value)),
            _ => Some(Cow::Borrowed(value)),
        });
    let body = builder.clean(html).to_string();
    // The page's own rules come last, so that they hold over the message's.
    let banner = banner(elsewhere.saturating_sub(loaded.load(Ordering::Relaxed)));
    format!(
        "<!doctype html><html><head><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         img-src data:{img_src}; style-src 'unsafe-inline'\"><meta charset=\"utf-8\">\
         <style>{layout}{PLACEHOLDER}{banner}</style></head><body>{body}</body></html>"
    )
}

/// What an `<img>` may show: an attached picture, a picture carried in the
/// URL itself, or one from a host the user allowed that is not at an
/// address the page uses to report its reading: a host is allowed whole,
/// so a pixel on a host that also serves a picture is kept out here.
/// Everything else,
/// including a relative path that a `file://` page would resolve against
/// the disk, is dropped.
fn image(
    tag: &str,
    src: &str,
    inline: &HashMap<String, String>,
    hosts: &[String],
    tracked: &[String],
) -> Option<String> {
    let url = Url::parse(src);
    match url.scheme.as_str() {
        _ if tag != "img" => None,
        "cid" => inline.get(url.text.get(4..)?).cloned(),
        "data" => {
            let lower = url.text.to_ascii_lowercase();
            IMAGE_TYPES
                .iter()
                .any(|t| lower[5..].strip_prefix(t).is_some_and(|rest| rest.starts_with(";base64,")))
                .then_some(url.text)
        }
        _ if html::loadable(&url) && hosts.contains(&url.host) && !tracked.contains(&url.text) => Some(url.text),
        _ => None,
    }
}

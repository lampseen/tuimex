//! HTML read as text (ADR-009): what a browser would display and nothing
//! it would hide, with the links numbered and every host the page wanted to
//! load from written down instead of contacted (ADR-008).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use html2text::render::{TaggedLine, TextDecorator};
use html2text::{Element, Handle, RcDom, config};
use tuimex_core::{Link, Url};

use crate::css::{self, Sheet};
use crate::text::{clean, line, push_unique};
use crate::{Error, trackers};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct Page {
    pub text: String,
    pub links: Vec<Link>,
    pub remote: Vec<String>,
    /// How many `<img>` elements named a picture on another host.
    pub images: usize,
    /// The hosts of the pictures the browser copy may load after a yes,
    /// and how many pictures: those `loadable` passes that are not
    /// `tracked`.
    pub image_hosts: Vec<String>,
    pub loadable: usize,
    /// The address of every picture that is there to report and not to
    /// be seen: on a tracking service's host, hidden, or a pixel across.
    /// The browser copy loads none of them, whatever is answered.
    pub tracked: Vec<String>,
    pub trackers: Vec<String>,
}

pub(crate) fn read(html: &str, width: usize) -> Result<Page, Error> {
    let marks = Marks::default();
    let config = config::with_decorator(marks.clone()).link_footnotes(true).no_table_borders().allow_width_overflow();
    let dom = config.parse_html(html.as_bytes()).map_err(unreadable)?;
    let mut scan = Scan::default();
    scan.styles(&dom)?;
    scan.prune(&dom);
    let tree = config.dom_to_render_tree(&dom).map_err(unreadable)?;
    let lines = config.render_to_lines(tree, width.max(MIN_WIDTH)).map_err(unreadable)?;

    let urls = marks.urls.take();
    if marks.opened.get() != urls.len() {
        // The numbers in the text are the renderer's count and the text of
        // each link is found by ours. If they ever differ, a link's text
        // would be reported against another link's target.
        return Err(Error::Html("the renderer numbered the links differently".into()));
    }
    let mut shown = vec![String::new(); urls.len()];
    let mut text = String::new();
    for line in &lines {
        let start = text.len();
        for span in line.tagged_strings() {
            text.push_str(&span.s);
            for &n in &span.tag {
                if let Some(label) = shown.get_mut(n.wrapping_sub(1)) {
                    label.push_str(&span.s);
                }
            }
        }
        text.truncate(text[start..].trim_end().len() + start);
        text.push('\n');
        shown.iter_mut().for_each(|label| label.push(' '));
    }
    let links = urls.iter().zip(&shown).enumerate();
    let links =
        links.map(|(i, (url, shown))| Link { n: i + 1, url: Url::parse(url).text, shown: line(shown) }).collect();
    // An address that reports from one element is not loaded for another.
    let pictures: Vec<&(String, String)> = scan.pictures.iter().filter(|p| !scan.tracked.contains(&p.0)).collect();
    let mut image_hosts = Vec::new();
    pictures.iter().for_each(|picture| push_unique(&mut image_hosts, &picture.1));
    Ok(Page {
        text: clean(text.trim_end()),
        links,
        remote: scan.remote,
        images: scan.images,
        image_hosts,
        loadable: pictures.len(),
        tracked: scan.tracked,
        trackers: scan.trackers,
    })
}

/// Narrower than this the renderer has no room for a list marker and a
/// word, and a caller asking for less has no screen to show it on.
const MIN_WIDTH: usize = 20;

fn unreadable(e: html2text::Error) -> Error {
    Error::Html(e.to_string())
}

/// Elements a browser's own stylesheet does not display, wherever they
/// stand, and that the renderer would otherwise print.
const UNSHOWN: [&str; 5] = ["title", "datalist", "noembed", "noframes", "param"];

/// What the walk over the page collects before anything is rendered.
#[derive(Default)]
struct Scan {
    sheet: Sheet,
    remote: Vec<String>,
    images: usize,
    /// The address and the host of each picture `loadable` passes that
    /// is there to be seen.
    pictures: Vec<(String, String)>,
    tracked: Vec<String>,
    trackers: Vec<String>,
}

/// Whether `host` is nothing but a host. One is written into the browser
/// copy's policy as text, where a `;` would start a directive of the
/// sender's.
pub(crate) fn plain_host(host: &str) -> bool {
    !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
}

/// Whether the browser copy may load a picture at `url` once its host is
/// agreed to: https alone and no credentials. The question `B` asks and
/// the sanitiser both go by this and by `Page::tracked`, so what is named
/// is what loads.
pub(crate) fn loadable(url: &Url) -> bool {
    url.scheme == "https" && !url.credentials && plain_host(&url.host)
}

/// The text of every `<style>` element, in the order they are written.
fn style_texts(dom: &RcDom) -> Result<Vec<String>, Error> {
    let (mut stack, mut texts) = (vec![dom.document.clone()], Vec::new());
    while let Some(node) = stack.pop() {
        stack.extend(node.children.borrow().iter().rev().cloned());
        let Element { name, attrs, .. } = &node.data else { continue };
        if &*name.local != "style" {
            continue;
        }
        // Without attributes the element serialises as exactly its
        // tags around its text, which is the only way this DOM gives
        // the text up. The renderer never looks at a `<style>`.
        attrs.borrow_mut().clear();
        let mut out = Vec::new();
        node.serialize(&mut out).map_err(|e| Error::Html(e.to_string()))?;
        let css = String::from_utf8_lossy(&out);
        texts.push(css.strip_prefix("<style>").and_then(|c| c.strip_suffix("</style>")).unwrap_or(&css).to_string());
    }
    Ok(texts)
}

/// The stylesheets a page carries, for the browser copy to keep what is
/// safe of them.
pub(crate) fn sheets(html: &str) -> Result<Vec<String>, Error> {
    style_texts(&config::plain().parse_html(html.as_bytes()).map_err(unreadable)?)
}

impl Scan {
    /// Reads every `<style>` first, wherever it sits: a rule at the end of
    /// the page hides an element at the top.
    fn styles(&mut self, dom: &RcDom) -> Result<(), Error> {
        for css in style_texts(dom)? {
            self.sheet.add(&css);
            css::urls(&css).iter().for_each(|u| self.wants(u));
        }
        Ok(())
    }

    /// Detaches every element a browser would not display. A hidden
    /// subtree is still walked, because what it would have fetched is
    /// reported all the same. A stack, not recursion: the depth of a page
    /// is its author's to choose.
    fn prune(&mut self, dom: &RcDom) {
        let mut stack = vec![(dom.document.clone(), false)];
        while let Some((node, hidden)) = stack.pop() {
            let mut kept = Vec::new();
            for child in node.children.take() {
                let gone = self.visit(&child, hidden) || hidden;
                if !gone {
                    kept.push(child.clone());
                }
                stack.push((child, gone));
            }
            node.children.replace(kept);
        }
    }

    /// Notes what one element would load, and says whether it is hidden.
    fn visit(&mut self, node: &Handle, inside_hidden: bool) -> bool {
        let Element { name, attrs, .. } = &node.data else { return false };
        let tag: &str = &name.local;
        if tag == "a" {
            // A link to a place on the same page goes nowhere in a reader.
            let nowhere = |href: &str| href.trim().is_empty() || href.trim_start().starts_with('#');
            attrs.borrow_mut().retain(|a| &*a.name.local != "href" || !nowhere(&a.value));
        }
        let attrs = attrs.borrow();
        let get = |name: &str| attrs.iter().find(|a| &*a.name.local == name).map(|a| &*a.value);
        let style = get("style").map(css::declarations).unwrap_or_default();
        let id = get("id").unwrap_or("").to_lowercase();
        let classes: Vec<String> = get("class").unwrap_or("").split_whitespace().map(str::to_lowercase).collect();
        let hidden = get("hidden").is_some()
            || UNSHOWN.contains(&tag)
            || (tag == "dialog" && get("open").is_none())
            || css::hides(self.sheet.matching(tag, &id, &classes).chain(&style));

        for a in attrs.iter() {
            match &*a.name.local {
                "src" | "background" | "poster" | "data" | "lowsrc" | "dynsrc" => self.wants(&a.value),
                "href" if !matches!(tag, "a" | "area" | "base") => self.wants(&a.value),
                "srcset" => a.value.split(',').filter_map(|c| c.split_whitespace().next()).for_each(|u| self.wants(u)),
                "style" => css::urls(&a.value).iter().for_each(|u| self.wants(u)),
                _ => {}
            }
        }
        let image = if tag == "img" { get("src").map(Url::parse).filter(Url::is_remote) } else { None };
        if let Some(image) = image {
            self.images += 1;
            let css_size = |side: &str| style.iter().rev().find(|d| d.0 == side).map(|d| d.1.as_str());
            let tiny = |side: &str| get(side).or(css_size(side)).and_then(css::number).is_some_and(|n| n <= 1.0);
            // An image nobody can see has one use.
            let unseen = hidden || inside_hidden || (tiny("width") && tiny("height"));
            let service = trackers::service(&image.host);
            match service {
                Some(service) => push_unique(&mut self.trackers, service),
                None if unseen => push_unique(&mut self.trackers, &image.host),
                None => {}
            }
            if service.is_some() || unseen {
                push_unique(&mut self.tracked, &image.text);
            } else if loadable(&image) {
                self.pictures.push((image.text, image.host));
            }
        }
        hidden
    }

    fn wants(&mut self, url: &str) {
        let url = Url::parse(url);
        if url.is_remote() {
            push_unique(&mut self.remote, &url.host);
        }
    }
}

/// How the renderer is told to mark things up. Links are numbered by the
/// renderer's own footnote counter; this decorator tags each link's text
/// with the same number, so the text can be read back per link, and takes
/// the targets from `finalise`. The state is shared because every table
/// cell renders through a clone.
#[derive(Clone, Default)]
struct Marks {
    opened: Rc<Cell<usize>>,
    urls: Rc<RefCell<Vec<String>>>,
}

impl TextDecorator for Marks {
    /// The number of the link the text belongs to, or 0.
    type Annotation = usize;

    fn decorate_link_start(&mut self, _url: &str) -> (String, usize) {
        self.opened.set(self.opened.get() + 1);
        (String::new(), self.opened.get())
    }
    fn decorate_link_end(&mut self) -> String {
        " ".into()
    }
    // Emphasis has no marker: the text is the author's words and nothing
    // an author did not type, so a `**` in it is one they wrote (ADR-037
    // marks structure at the start of a line and nowhere else).
    fn decorate_em_start(&self) -> (String, usize) {
        (String::new(), 0)
    }
    fn decorate_em_end(&self) -> String {
        String::new()
    }
    fn decorate_strong_start(&self) -> (String, usize) {
        (String::new(), 0)
    }
    fn decorate_strong_end(&self) -> String {
        String::new()
    }
    fn decorate_strikeout_start(&self) -> (String, usize) {
        (String::new(), 0)
    }
    fn decorate_strikeout_end(&self) -> String {
        String::new()
    }
    fn decorate_code_start(&self) -> (String, usize) {
        (String::new(), 0)
    }
    fn decorate_code_end(&self) -> String {
        String::new()
    }
    fn decorate_superscript_start(&self) -> (String, usize) {
        (String::new(), 0)
    }
    fn decorate_superscript_end(&self) -> String {
        String::new()
    }
    fn decorate_preformat_first(&self) -> usize {
        0
    }
    fn decorate_preformat_cont(&self) -> usize {
        0
    }
    fn decorate_image(&mut self, _src: &str, title: &str) -> (String, usize) {
        (format!("[image: {title}]"), 0)
    }
    fn header_prefix(&self, level: usize) -> String {
        "#".repeat(level) + " "
    }
    fn quote_prefix(&self) -> String {
        "> ".into()
    }
    fn unordered_item_prefix(&self) -> String {
        "* ".into()
    }
    fn ordered_item_prefix(&self, i: i64) -> String {
        format!("{i}. ")
    }
    fn make_subblock_decorator(&self) -> Self {
        self.clone()
    }
    fn finalise(&mut self, urls: Vec<String>) -> Vec<TaggedLine<usize>> {
        self.urls.replace(urls);
        Vec::new()
    }
}

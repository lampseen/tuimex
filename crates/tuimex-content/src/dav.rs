//! A CalDAV server's answer to plain data (ADR-062): for each thing the
//! server lists, where it is, the tag that changes when it does, and the
//! calendar text when that was asked for. The XML is read by `roxmltree`,
//! which refuses a document type declaration, so no entity is ever
//! expanded.

use crate::Error;
use crate::text::line;

#[cfg(test)]
mod tests;

const DAV: &str = "DAV:";
const CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
const CALENDAR_SERVER: &str = "http://calendarserver.org/ns/";

/// More elements than any answer tuimex asks for holds; a larger one is
/// refused whole, not read in part.
const NODES: u32 = 2_000_000;

/// One `response` of a multistatus answer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DavItem {
    /// The address the server gave, as written: a path, or a whole URL.
    pub href: String,
    /// Changes whenever this one thing does.
    pub etag: Option<String>,
    /// Changes whenever anything in this collection does.
    pub ctag: Option<String>,
    pub name: Option<String>,
    /// A collection that holds events.
    pub is_calendar: bool,
    /// The calendar text, when the request asked for it.
    pub calendar: Option<String>,
    /// The server says it is no longer there.
    pub gone: bool,
}

fn child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    namespace: &str,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|c| c.has_tag_name((namespace, name)))
}

/// An element's text on one line, cleaned; `None` when there is none.
fn text(node: roxmltree::Node<'_, '_>) -> Option<String> {
    node.text().map(line).filter(|t| !t.is_empty())
}

/// The items of a `multistatus`. Only properties the server answered with
/// success are read; one it refused is as if it were absent.
pub fn multistatus(xml: &str) -> Result<Vec<DavItem>, Error> {
    let options = roxmltree::ParsingOptions { nodes_limit: NODES, ..roxmltree::ParsingOptions::default() };
    let document = roxmltree::Document::parse_with_options(xml, options).map_err(|e| Error::Dav(e.to_string()))?;
    let root = document.root_element();
    if !root.has_tag_name((DAV, "multistatus")) {
        return Err(Error::Dav("it is not a multistatus".into()));
    }
    let succeeded = |status: Option<&str>| {
        status.is_none_or(|s| s.split_whitespace().nth(1).is_some_and(|code| code.starts_with('2')))
    };

    let mut items = Vec::new();
    for response in root.children().filter(|c| c.has_tag_name((DAV, "response"))) {
        let Some(href) = child(response, DAV, "href").and_then(text) else { continue };
        let status = child(response, DAV, "status").and_then(|s| s.text());
        let mut item = DavItem {
            href,
            gone: status.is_some_and(|s| s.split_whitespace().nth(1) == Some("404")),
            ..DavItem::default()
        };
        for propstat in response.children().filter(|c| c.has_tag_name((DAV, "propstat"))) {
            if !succeeded(child(propstat, DAV, "status").and_then(|s| s.text())) {
                continue;
            }
            let Some(prop) = child(propstat, DAV, "prop") else { continue };
            item.etag = item.etag.or(child(prop, DAV, "getetag").and_then(text));
            item.ctag = item.ctag.or(child(prop, CALENDAR_SERVER, "getctag").and_then(text));
            item.name = item.name.or(child(prop, DAV, "displayname").and_then(text));
            item.is_calendar |=
                child(prop, DAV, "resourcetype").is_some_and(|kind| child(kind, CALDAV, "calendar").is_some());
            // Calendar text keeps its lines: it is parsed, not shown.
            item.calendar =
                item.calendar.or(child(prop, CALDAV, "calendar-data").and_then(|d| d.text()).map(str::to_string));
        }
        items.push(item);
    }
    Ok(items)
}

//! XML parsing as Python's ElementTree (expat with defusedxml) sees it:
//! namespaces resolved, line endings normalised, entity references expanded,
//! and a document type declaring entities refused.
//!
//! Small parts are read into an [`Element`] tree. Worksheets and shared
//! strings are streamed: only the subtrees openpyxl acts on (a row, a shared
//! string) are built, the way it handles `iterparse` end events.

use std::io::BufRead;

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use quick_xml::XmlVersion;

use crate::XlsxError;

/// The SpreadsheetML main namespace (transitional).
pub const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
/// The relationships namespace used by `r:id` attributes (transitional).
pub const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// An attribute with its resolved namespace (unprefixed attributes have none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// Namespace URI, if the attribute is prefixed.
    pub ns: Option<String>,
    /// Local name.
    pub local: String,
    /// Normalised value.
    pub value: String,
}

/// An element as ElementTree builds it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// Namespace URI, if any.
    pub ns: Option<String>,
    /// Local name.
    pub local: String,
    /// Attributes in document order.
    pub attrs: Vec<Attr>,
    /// Text before the first child element (`Element.text`).
    pub text: String,
    /// Child elements.
    pub children: Vec<Element>,
    /// True if the element was written self-closing (`<c r="A1"/>`).
    pub self_closing: bool,
}

impl Element {
    /// Value of an unprefixed attribute (`element.get(name)`).
    pub fn get(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.ns.is_none() && a.local == name)
            .map(|a| a.value.as_str())
    }

    /// Value of a namespaced attribute (`element.get("{ns}name")`).
    pub fn get_ns(&self, ns: &str, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.ns.as_deref() == Some(ns) && a.local == name)
            .map(|a| a.value.as_str())
    }

    /// True if this element is `{MAIN_NS}local`.
    pub fn is_main(&self, local: &str) -> bool {
        self.local == local && self.ns.as_deref() == Some(MAIN_NS)
    }

    /// First child that is `{MAIN_NS}local` (`element.find(tag)`).
    pub fn find_main(&self, local: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.is_main(local))
    }

    /// Children with this local name, in any namespace (openpyxl's descriptors match local names).
    pub fn children_named<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.local == local)
    }

    /// The last child with this local name: a single-valued descriptor keeps the last one.
    pub fn last_named(&self, local: &str) -> Option<&Element> {
        self.children.iter().rfind(|c| c.local == local)
    }
}

/// Plain text of an openpyxl `Text` (`<si>`, `<is>`, a comment's `<text>`):
/// the last direct `<t>`, then the last `<t>` of each `<r>` run. Phonetic
/// runs (`<rPh>`) are left out.
pub fn text_content(el: &Element) -> String {
    let mut out = String::new();
    if let Some(t) = el.last_named("t") {
        out.push_str(&t.text);
    }
    for run in el.children_named("r") {
        if let Some(t) = run.last_named("t") {
            out.push_str(&t.text);
        }
    }
    out
}

/// A streaming parser over one XML part.
pub struct Parser<R: BufRead> {
    reader: NsReader<R>,
    buf: Vec<u8>,
}

/// One event of interest from [`Parser::next_node`].
pub enum Node {
    /// An element opened; call [`Parser::subtree`] to read it, or keep
    /// reading to descend into it.
    Open(Element),
    /// The document ended.
    End,
}

fn parse_error(e: impl std::fmt::Display) -> XlsxError {
    XlsxError::new("ParseError", e.to_string())
}

/// A reader error: a failed read of the zip part (`BadZipFile`, as Python's
/// `zipfile` reports a part larger than declared or a bad CRC), else malformed XML.
fn read_error(e: quick_xml::Error) -> XlsxError {
    match e {
        quick_xml::Error::Io(io) => XlsxError::new("BadZipFile", io.to_string()),
        other => parse_error(other),
    }
}

impl<R: BufRead> Parser<R> {
    /// Start parsing `source`.
    pub fn new(source: R) -> Self {
        let mut reader = NsReader::from_reader(source);
        let config = reader.config_mut();
        config.trim_text(false);
        config.check_end_names = true;
        config.expand_empty_elements = false;
        Parser {
            reader,
            buf: Vec::new(),
        }
    }

    fn element(&self, start: &BytesStart<'_>, self_closing: bool) -> Result<Element, XlsxError> {
        let (ns, local) = self.reader.resolver().resolve_element(start.name());
        let mut attrs = Vec::new();
        for attr in start.attributes() {
            let attr = attr.map_err(parse_error)?;
            let key = attr.key;
            if key.as_ref() == "xmlns" || key.as_ref().starts_with("xmlns:") {
                continue;
            }
            let (ans, alocal) = self.reader.resolver().resolve_attribute(key);
            let value = attr
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(parse_error)?;
            attrs.push(Attr {
                ns: resolved(ans)?,
                local: alocal.as_ref().to_string(),
                value: legal(value.into_owned())?,
            });
        }
        Ok(Element {
            ns: resolved(ns)?,
            local: local.as_ref().to_string(),
            attrs,
            text: String::new(),
            children: Vec::new(),
            self_closing,
        })
    }

    /// The next element start anywhere in the document, skipping text and
    /// end tags; [`Node::End`] at the end of the document.
    pub fn next_node(&mut self) -> Result<Node, XlsxError> {
        loop {
            self.buf.clear();
            let event = self
                .reader
                .read_event_into(&mut self.buf)
                .map_err(read_error)?;
            match event {
                Event::Start(e) => {
                    let start = e.into_owned();
                    return self.element(&start, false).map(Node::Open);
                }
                Event::Empty(e) => {
                    let start = e.into_owned();
                    return self.element(&start, true).map(Node::Open);
                }
                Event::DocType(d) => check_doctype(&d)?,
                Event::GeneralRef(r) => {
                    resolve_ref(&r)?;
                }
                Event::Text(t) => {
                    legal(t.xml10_content().into_owned())?;
                }
                Event::CData(c) => {
                    legal(c.into_inner().into_owned())?;
                }
                Event::Eof => return Ok(Node::End),
                _ => {}
            }
        }
    }

    /// Read the rest of an element opened by [`Parser::next_node`], with its
    /// text and children, consuming its end tag.
    pub fn subtree(&mut self, mut el: Element) -> Result<Element, XlsxError> {
        if el.self_closing {
            return Ok(el);
        }
        loop {
            self.buf.clear();
            let event = self
                .reader
                .read_event_into(&mut self.buf)
                .map_err(read_error)?;
            match event {
                Event::Start(e) => {
                    let start = e.into_owned();
                    let child = self.element(&start, false)?;
                    let child = self.subtree(child)?;
                    el.children.push(child);
                }
                Event::Empty(e) => {
                    let start = e.into_owned();
                    let child = self.element(&start, true)?;
                    el.children.push(child);
                }
                Event::Text(t) => {
                    let text = legal(t.xml10_content().into_owned())?;
                    if el.children.is_empty() {
                        el.text.push_str(&text);
                    }
                }
                Event::CData(c) => {
                    let raw = legal(c.into_inner().into_owned())?;
                    if el.children.is_empty() {
                        el.text
                            .push_str(&raw.replace("\r\n", "\n").replace('\r', "\n"));
                    }
                }
                Event::GeneralRef(r) => {
                    let text = resolve_ref(&r)?;
                    if el.children.is_empty() {
                        el.text.push_str(&text);
                    }
                }
                Event::End(_) => return Ok(el),
                Event::Eof => return Err(parse_error("no element found")),
                _ => {}
            }
        }
    }
}

impl<R: BufRead> Parser<R> {
    /// Read the rest of a `<row>` opened by [`Parser::next_node`], dropping
    /// the cells the Python reader removed before parsing (design decision
    /// 0014): a self-closing `c` with an `r` attribute whose next sibling is
    /// another `c` with an `r` attribute, or the end of the row. A sheet
    /// formatted across whole rows holds millions of them.
    pub fn row(&mut self, mut row: Element) -> Result<Element, XlsxError> {
        if row.self_closing {
            return Ok(row);
        }
        let mut pending: Option<BytesStart<'static>> = None;
        loop {
            self.buf.clear();
            let event = self
                .reader
                .read_event_into(&mut self.buf)
                .map_err(read_error)?
                .into_owned();
            let next_has_ref = match &event {
                Event::Start(e) | Event::Empty(e) => Some(is_referenced_cell(e)),
                Event::End(_) => Some(true),
                _ => None,
            };
            if let (Some(keep_pending), Some(start)) = (next_has_ref.map(|r| !r), pending.as_ref())
            {
                if keep_pending {
                    let el = self.element(start, true)?;
                    row.children.push(el);
                }
                pending = None;
            }
            match event {
                Event::Start(e) => {
                    let start = e.into_owned();
                    let child = self.element(&start, false)?;
                    let child = self.subtree(child)?;
                    row.children.push(child);
                }
                Event::Empty(e) => {
                    let start = e.into_owned();
                    if is_referenced_cell(&start) {
                        pending = Some(start);
                    } else {
                        let child = self.element(&start, true)?;
                        row.children.push(child);
                    }
                }
                Event::GeneralRef(r) => {
                    resolve_ref(&r)?;
                }
                Event::Text(t) => {
                    legal(t.xml10_content().into_owned())?;
                }
                Event::End(_) => return Ok(row),
                Event::Eof => return Err(parse_error("no element found")),
                _ => {}
            }
        }
    }
}

/// True for a `c` element (any prefix) with an unprefixed `r` attribute.
fn is_referenced_cell(start: &BytesStart<'_>) -> bool {
    start.local_name().as_ref() == "c"
        && start.attributes().flatten().any(|a| a.key.as_ref() == "r")
}

fn resolved(ns: ResolveResult<'_>) -> Result<Option<String>, XlsxError> {
    match ns {
        ResolveResult::Bound(n) => Ok(Some(n.as_ref().to_string())),
        ResolveResult::Unbound => Ok(None),
        ResolveResult::Unknown(p) => Err(parse_error(format!("unbound prefix: {p}"))),
    }
}

/// defusedxml refuses entity declarations; a plain document type is accepted.
fn check_doctype(doctype: &quick_xml::events::BytesText<'_>) -> Result<(), XlsxError> {
    if doctype.contains("<!ENTITY") {
        return Err(XlsxError::new(
            "EntitiesForbidden",
            "entity declarations are refused",
        ));
    }
    Ok(())
}

/// expat refuses characters XML 1.0 does not allow (controls other than
/// tab, LF and CR, and U+FFFE/U+FFFF), written literally or as a reference.
fn legal(text: String) -> Result<String, XlsxError> {
    let bad = text.chars().any(|c| {
        (c < ' ' && !matches!(c, '\t' | '\n' | '\r')) || matches!(c, '\u{fffe}' | '\u{ffff}')
    });
    if bad {
        return Err(parse_error("not well-formed (invalid token)"));
    }
    Ok(text)
}

/// Expand a predefined entity or character reference; anything else is undefined.
fn resolve_ref(r: &quick_xml::events::BytesRef<'_>) -> Result<String, XlsxError> {
    if let Some(c) = r.resolve_char_ref().map_err(parse_error)? {
        return legal(c.to_string());
    }
    let text = match r.as_ref() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        other => return Err(parse_error(format!("undefined entity &{other};"))),
    };
    Ok(text.to_string())
}

/// Parse a whole small part into its root element.
pub fn parse_tree(data: &[u8]) -> Result<Element, XlsxError> {
    let mut parser = Parser::new(data);
    match parser.next_node()? {
        Node::Open(root) => parser.subtree(root),
        Node::End => Err(parse_error("no element found")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_matches_elementtree() {
        let xml = b"<?xml version=\"1.0\"?>\r\n<a xmlns=\"urn:x\" xmlns:p=\"urn:p\" p:k=\"1\" v=\"x&#10;y\r\nz\">one\r\ntwo&amp;<![CDATA[<c>]]><!-- c -->more<b/>tail</a>";
        let root = parse_tree(xml).unwrap();
        assert_eq!(root.ns.as_deref(), Some("urn:x"));
        assert_eq!(root.local, "a");
        assert_eq!(root.text, "one\ntwo&<c>more");
        assert_eq!(root.get_ns("urn:p", "k"), Some("1"));
        assert_eq!(root.get("v"), Some("x\ny z"));
        assert_eq!(root.children.len(), 1);
        assert!(root.children[0].self_closing);
    }

    #[test]
    fn entities_are_refused_or_undefined() {
        let doctype = b"<!DOCTYPE a [<!ENTITY e \"x\">]><a>&e;</a>";
        assert_eq!(parse_tree(doctype).unwrap_err().kind, "EntitiesForbidden");
        assert_eq!(parse_tree(b"<a>&e;</a>").unwrap_err().kind, "ParseError");
        assert!(parse_tree(b"<!DOCTYPE a><a>x</a>").is_ok());
        assert_eq!(parse_tree(b"<a><b></a>").unwrap_err().kind, "ParseError");
        assert_eq!(parse_tree(b"").unwrap_err().kind, "ParseError");
        assert_eq!(parse_tree(b"<a>x&#1;</a>").unwrap_err().kind, "ParseError");
        assert_eq!(parse_tree(b"<a>x\x01</a>").unwrap_err().kind, "ParseError");
        assert_eq!(
            parse_tree(b"<a v=\"&#2;\"/>").unwrap_err().kind,
            "ParseError"
        );
        assert!(parse_tree(b"<a>\t&#13;&#10;</a>").is_ok());
    }

    #[test]
    fn text_content_like_openpyxl() {
        let si = parse_tree(
            b"<si xmlns=\"urn:x\"><t>a</t><t>b</t><r><t>c</t></r><rPh><t>no</t></rPh><r><rPr/><t>d</t></r></si>",
        )
        .unwrap();
        assert_eq!(text_content(&si), "bcd");
        let el = parse_tree(b"<x xmlns=\"urn:m\"><v>1</v><f>2</f></x>").unwrap();
        assert_eq!(el.find_main("v"), None);
        assert_eq!(el.last_named("f").map(|f| f.text.as_str()), Some("2"));
    }
}

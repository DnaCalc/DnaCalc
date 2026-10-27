//! Lazy XML readers over a part's decompressed bytes: a pull reader that yields one event at a
//! time, so a part is tokenized only as far as its consumer reads it.
//!
//! A document type declaration is refused wherever it appears, so no entity a document declares,
//! internal or external, is ever expanded and nothing outside the part is ever fetched. Only the
//! five predefined entities and character references are replaced; any other entity reference is
//! refused. Namespaces are resolved; nesting depth and the attributes of one element are bounded.

use quick_xml::NsReader;
use quick_xml::escape::EscapeError;
use quick_xml::events::Event;
use quick_xml::name::{QName, ResolveResult};
use std::borrow::Cow;
use std::fmt;
use std::ops::Deref;

/// Bounds on one XML part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XmlLimits {
    /// The deepest element nesting accepted; the document element is at depth 1.
    pub max_depth: u32,
    /// The most attributes one element may carry, namespace declarations included.
    pub max_attributes: u32,
}

impl Default for XmlLimits {
    fn default() -> Self {
        XmlLimits {
            max_depth: 256,
            max_attributes: 128,
        }
    }
}

/// Why an XML part was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XmlError {
    /// The part holds a document type declaration (`<!DOCTYPE`), which could declare entities.
    DocumentTypeDeclaration,
    /// The part refers to an entity that is neither predefined nor a character reference.
    UndefinedEntity { name: String },
    /// Elements nest deeper than [`XmlLimits::max_depth`].
    DepthLimit { limit: u32 },
    /// An element carries more attributes than [`XmlLimits::max_attributes`].
    AttributeLimit { limit: u32 },
    /// A name uses a namespace prefix that no enclosing element declares.
    UnboundPrefix { prefix: String },
    /// The part is not encoded as UTF-8.
    UnsupportedEncoding { encoding: String },
    /// The part is not well-formed XML.
    Malformed { position: u64, detail: String },
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DocumentTypeDeclaration => {
                f.write_str("a document type declaration, which is refused")
            }
            Self::UndefinedEntity { name } => write!(f, "the undefined entity &{name};"),
            Self::DepthLimit { limit } => write!(f, "elements nest deeper than {limit}"),
            Self::AttributeLimit { limit } => {
                write!(f, "an element carries more than {limit} attributes")
            }
            Self::UnboundPrefix { prefix } => {
                write!(f, "the namespace prefix {prefix:?} is not declared")
            }
            Self::UnsupportedEncoding { encoding } => {
                write!(f, "the encoding {encoding:?}, where UTF-8 is required")
            }
            Self::Malformed { position, detail } => {
                write!(f, "malformed XML at byte {position}: {detail}")
            }
        }
    }
}

impl std::error::Error for XmlError {}

/// A namespace URI or a local name as the reader gives it. One of the names the workbook parts
/// use most (the SpreadsheetML and relationship namespaces, `c`, `v`, `f`, `row` and the like)
/// is shared from a fixed table, so reading it allocates nothing (dc-dxm: the allocator was a
/// third of a native open); any other name is owned. It compares and prints as its text.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct XmlStr(Cow<'static, str>);

impl XmlStr {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The namespace URI `bytes` spells: from the table when it is one of the common ones, else
    /// checked as UTF-8 and owned.
    fn namespace(bytes: &[u8], position: u64) -> Result<XmlStr, XmlError> {
        if let Some(name) = COMMON_NAMESPACES.iter().find(|name| name.as_bytes() == bytes) {
            return Ok(XmlStr(Cow::Borrowed(name)));
        }
        utf8(bytes, position).map(|text| XmlStr(Cow::Owned(text)))
    }

    /// The local name `bytes` spells: shared when it is one of the common ones, else checked as
    /// UTF-8 and owned.
    fn local(bytes: &[u8], position: u64) -> Result<XmlStr, XmlError> {
        match common_local(bytes) {
            Some(name) => Ok(XmlStr(Cow::Borrowed(name))),
            None => utf8(bytes, position).map(|text| XmlStr(Cow::Owned(text))),
        }
    }
}

impl Deref for XmlStr {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for XmlStr {
    fn eq(&self, other: &str) -> bool {
        *self.0 == *other
    }
}

impl PartialEq<&str> for XmlStr {
    fn eq(&self, other: &&str) -> bool {
        *self.0 == **other
    }
}

impl fmt::Debug for XmlStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for XmlStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

impl From<XmlStr> for String {
    fn from(name: XmlStr) -> String {
        name.0.into_owned()
    }
}

impl From<&str> for XmlStr {
    fn from(name: &str) -> XmlStr {
        XmlStr(Cow::Owned(name.to_owned()))
    }
}

/// The namespaces the workbook parts use most, shared rather than allocated per name.
const COMMON_NAMESPACES: &[&str] = &[
    "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "http://schemas.openxmlformats.org/package/2006/relationships",
    "http://schemas.openxmlformats.org/package/2006/content-types",
    "http://schemas.openxmlformats.org/markup-compatibility/2006",
    "http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac",
    "http://schemas.microsoft.com/office/spreadsheetml/2014/revision",
];

/// The local name `bytes` spells when it is one the workbook parts use most, shared rather than
/// allocated per name.
fn common_local(bytes: &[u8]) -> Option<&'static str> {
    Some(match bytes {
        b"c" => "c",
        b"v" => "v",
        b"f" => "f",
        b"r" => "r",
        b"s" => "s",
        b"t" => "t",
        b"row" => "row",
        b"si" => "si",
        b"ref" => "ref",
        b"spans" => "spans",
        b"ht" => "ht",
        b"customHeight" => "customHeight",
        b"dyDescent" => "dyDescent",
        b"is" => "is",
        b"cm" => "cm",
        b"vm" => "vm",
        b"ca" => "ca",
        b"aca" => "aca",
        b"hidden" => "hidden",
        b"customFormat" => "customFormat",
        b"sst" => "sst",
        b"count" => "count",
        b"uniqueCount" => "uniqueCount",
        b"rPr" => "rPr",
        b"xf" => "xf",
        b"numFmtId" => "numFmtId",
        b"fontId" => "fontId",
        b"fillId" => "fillId",
        b"borderId" => "borderId",
        b"xfId" => "xfId",
        b"applyNumberFormat" => "applyNumberFormat",
        b"applyFont" => "applyFont",
        b"applyFill" => "applyFill",
        b"applyBorder" => "applyBorder",
        b"applyAlignment" => "applyAlignment",
        b"applyProtection" => "applyProtection",
        b"alignment" => "alignment",
        b"col" => "col",
        b"min" => "min",
        b"max" => "max",
        b"width" => "width",
        b"style" => "style",
        b"id" => "id",
        _ => return None,
    })
}

/// An element or attribute name: its namespace URI, if it has one, and its local name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlName {
    pub namespace: Option<XmlStr>,
    pub local: XmlStr,
}

impl XmlName {
    /// Whether the name is `local` in the namespace `namespace`.
    pub fn is(&self, namespace: &str, local: &str) -> bool {
        self.namespace.as_deref() == Some(namespace) && self.local == local
    }
}

/// One attribute, its value with entity and character references replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlAttribute {
    pub name: XmlName,
    pub value: String,
}

/// A start tag. An empty element (`<a/>`) is a start tag followed by its end tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlElement {
    pub name: XmlName,
    /// The element's attributes in document order, without its namespace declarations.
    pub attributes: Vec<XmlAttribute>,
}

impl XmlElement {
    /// The value of the attribute without a namespace named `local`.
    pub fn attribute(&self, local: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name.namespace.is_none() && attribute.name.local == local)
            .map(|attribute| attribute.value.as_str())
    }
}

/// One event of a part, in document order. Comments and processing instructions are skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XmlEvent {
    Start(XmlElement),
    End(XmlName),
    /// Character data inside the document element, CDATA sections included.
    Text(String),
}

/// A part's decompressed bytes, ready to be read as XML.
pub struct XmlPart {
    name: String,
    bytes: Vec<u8>,
    limits: XmlLimits,
}

impl XmlPart {
    pub(crate) fn new(name: String, bytes: Vec<u8>, limits: XmlLimits) -> Self {
        XmlPart {
            name,
            bytes,
            limits,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The part of this name and limits with other bytes: what a save makes of it.
    pub(crate) fn with_bytes(&self, bytes: Vec<u8>) -> XmlPart {
        XmlPart::new(self.name.clone(), bytes, self.limits)
    }

    /// A reader from the start of the part.
    pub fn reader(&self) -> XmlReader<'_> {
        XmlReader::new(&self.bytes, self.limits)
    }
}

/// The UTF-8 encoding of U+FEFF, which a part may start with as a byte order mark.
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// A pull reader over one part.
pub struct XmlReader<'a> {
    inner: NsReader<&'a [u8]>,
    /// The offset in the part of the first byte the inner reader reads: 3 past a UTF-8 byte order
    /// mark, which the reader drops itself, else 0. Every position the inner reader gives is
    /// relative to that byte.
    origin: usize,
    limits: XmlLimits,
    depth: u32,
    started: bool,
    root_closed: bool,
    failure: Option<XmlError>,
    done: bool,
    /// The byte range of the source that the last event returned was read from. An empty
    /// element's start tag spans the whole element, and its end tag spans nothing.
    span: (usize, usize),
    /// The namespace an element name without a prefix resolves to, once resolved (dc-dxm): it
    /// stands until a start tag that may declare a namespace, or the end of an element whose
    /// start tag might have, changes the bindings in scope.
    default_namespace: Option<Option<XmlStr>>,
    /// The depths of the open elements whose start tags may have declared a namespace.
    declaring: Vec<u32>,
}

fn utf8(bytes: &[u8], position: u64) -> Result<String, XmlError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| XmlError::Malformed {
        position,
        detail: "a name or text that is not UTF-8".to_owned(),
    })
}

fn namespace(resolved: ResolveResult<'_>, position: u64) -> Result<Option<XmlStr>, XmlError> {
    match resolved {
        ResolveResult::Unbound => Ok(None),
        ResolveResult::Bound(namespace) => {
            XmlStr::namespace(namespace.as_ref(), position).map(Some)
        }
        ResolveResult::Unknown(prefix) => Err(XmlError::UnboundPrefix {
            prefix: String::from_utf8_lossy(&prefix).into_owned(),
        }),
    }
}

impl<'a> XmlReader<'a> {
    pub(crate) fn new(bytes: &'a [u8], limits: XmlLimits) -> Self {
        // quick-xml drops a leading UTF-8 byte order mark without counting it in its positions,
        // so the mark is dropped here and its length added to every position instead.
        let origin = if bytes.starts_with(&UTF8_BOM) {
            UTF8_BOM.len()
        } else {
            0
        };
        let rest = &bytes[origin..];
        let mut inner = NsReader::from_reader(rest);
        let config = inner.config_mut();
        config.expand_empty_elements = true;
        config.check_end_names = true;
        config.check_comments = true;
        let failure = if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            Some(XmlError::UnsupportedEncoding {
                encoding: "UTF-16".to_owned(),
            })
        } else if rest.starts_with(&UTF8_BOM) {
            // A second U+FEFF before the document is a character outside the document element,
            // which the inner reader would drop as a byte order mark without counting it.
            Some(XmlError::Malformed {
                position: origin as u64,
                detail: "a second byte order mark".to_owned(),
            })
        } else {
            None
        };
        XmlReader {
            inner,
            origin,
            limits,
            depth: 0,
            started: false,
            root_closed: false,
            failure,
            done: false,
            span: (0, 0),
            default_namespace: None,
            declaring: Vec::new(),
        }
    }

    /// The byte range of the part that the last event returned was read from: a start tag's
    /// `<` to its `>`, an end tag's `</` to its `>`, a text's first byte to the next markup. The
    /// start tag of an empty element (`<a/>`) spans the whole element and its end tag spans no
    /// byte. Comments and processing instructions the reader skipped before an event lie before
    /// its range.
    pub(crate) fn span(&self) -> (usize, usize) {
        self.span
    }

    fn error(&self, error: quick_xml::Error) -> XmlError {
        match error {
            quick_xml::Error::Syntax(quick_xml::errors::SyntaxError::UnclosedDoctype) => {
                XmlError::DocumentTypeDeclaration
            }
            quick_xml::Error::Escape(EscapeError::UnrecognizedEntity(_, name)) => {
                XmlError::UndefinedEntity { name }
            }
            other => XmlError::Malformed {
                position: self.inner.error_position() + self.origin as u64,
                detail: other.to_string(),
            },
        }
    }

    /// The offset in the part of the inner reader's position.
    fn position(&self) -> u64 {
        self.inner.buffer_position() + self.origin as u64
    }

    fn malformed(&self, detail: &str) -> XmlError {
        XmlError::Malformed {
            position: self.position(),
            detail: detail.to_owned(),
        }
    }

    fn element_name(&mut self, name: QName<'_>) -> Result<XmlName, XmlError> {
        let position = self.position();
        let (local, prefix) = name.decompose();
        let namespace = match (prefix, &self.default_namespace) {
            // Kept from the last name without a prefix: no resolution.
            (None, Some(held)) => held.clone(),
            (None, None) => {
                let resolved = namespace(self.inner.resolve_element(name).0, position)?;
                self.default_namespace = Some(resolved.clone());
                resolved
            }
            (Some(_), _) => namespace(self.inner.resolve_element(name).0, position)?,
        };
        Ok(XmlName {
            namespace,
            local: XmlStr::local(local.as_ref(), position)?,
        })
    }

    /// The next event, `None` after the document element has closed and the input has ended, or
    /// the reason the part is refused. After an error every further call returns it again.
    pub fn next_event(&mut self) -> Result<Option<XmlEvent>, XmlError> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let result = self.advance();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn advance(&mut self) -> Result<Option<XmlEvent>, XmlError> {
        loop {
            if self.done {
                return Ok(None);
            }
            let start = self.position() as usize;
            let event = match self.inner.read_event() {
                Ok(event) => event,
                Err(error) => return Err(self.error(error)),
            };
            self.span = (start, self.position() as usize);
            let first = !self.started;
            self.started = true;
            match event {
                Event::Decl(declaration) => {
                    if !first {
                        return Err(self.malformed("an XML declaration after the document start"));
                    }
                    if let Some(encoding) = declaration.encoding() {
                        let encoding = encoding.map_err(|error| self.error(error.into()))?;
                        if !encoding.eq_ignore_ascii_case(b"utf-8") {
                            return Err(XmlError::UnsupportedEncoding {
                                encoding: String::from_utf8_lossy(&encoding).into_owned(),
                            });
                        }
                    }
                }
                Event::DocType(_) => return Err(XmlError::DocumentTypeDeclaration),
                Event::Start(start) => {
                    if self.root_closed {
                        return Err(self.malformed("a second document element"));
                    }
                    self.depth += 1;
                    if self.depth > self.limits.max_depth {
                        return Err(XmlError::DepthLimit {
                            limit: self.limits.max_depth,
                        });
                    }
                    // A start tag that may declare a namespace (any `xmlns` in its attributes)
                    // ends what the reader knows of the default namespace.
                    let raw = start.attributes_raw();
                    let declares = raw
                        .iter()
                        .enumerate()
                        .any(|(at, byte)| *byte == b'x' && raw[at..].starts_with(b"xmlns"));
                    if declares {
                        self.default_namespace = None;
                        self.declaring.push(self.depth);
                    }
                    let name = self.element_name(start.name())?;
                    let mut attributes = Vec::new();
                    let mut count = 0u32;
                    for attribute in start.attributes() {
                        let attribute = attribute.map_err(|error| self.error(error.into()))?;
                        count += 1;
                        if count > self.limits.max_attributes {
                            return Err(XmlError::AttributeLimit {
                                limit: self.limits.max_attributes,
                            });
                        }
                        if attribute.key.as_namespace_binding().is_some() {
                            continue;
                        }
                        let position = self.position();
                        let (resolved, local) = self.inner.resolve_attribute(attribute.key);
                        let name = XmlName {
                            namespace: namespace(resolved, position)?,
                            local: XmlStr::local(local.as_ref(), position)?,
                        };
                        let value = attribute
                            .unescape_value()
                            .map_err(|error| self.error(error))?
                            .into_owned();
                        attributes.push(XmlAttribute { name, value });
                    }
                    return Ok(Some(XmlEvent::Start(XmlElement { name, attributes })));
                }
                Event::End(end) => {
                    let name = self.element_name(end.name())?;
                    if self.declaring.last() == Some(&self.depth) {
                        self.declaring.pop();
                        self.default_namespace = None;
                    }
                    self.depth -= 1;
                    if self.depth == 0 {
                        self.root_closed = true;
                    }
                    return Ok(Some(XmlEvent::End(name)));
                }
                Event::Text(text) => {
                    let text = text.unescape().map_err(|error| self.error(error))?;
                    if self.depth > 0 {
                        return Ok(Some(XmlEvent::Text(text.into_owned())));
                    }
                    if !text.trim().is_empty() {
                        return Err(self.malformed("text outside the document element"));
                    }
                }
                Event::CData(data) => {
                    if self.depth == 0 {
                        return Err(self.malformed("a CDATA section outside the document element"));
                    }
                    let position = self.position();
                    return Ok(Some(XmlEvent::Text(utf8(&data, position)?)));
                }
                Event::Empty(_) => {
                    return Err(self.malformed("an empty element the reader did not expand"));
                }
                Event::Comment(_) | Event::PI(_) => {}
                Event::Eof => {
                    if !self.root_closed {
                        return Err(self.malformed(if self.depth == 0 {
                            "the part has no document element"
                        } else {
                            "the part ends inside an element"
                        }));
                    }
                    self.done = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each event's kind and the bytes of `text` its span gives.
    fn spanned(text: &[u8]) -> Vec<(String, String)> {
        let mut reader = XmlReader::new(text, XmlLimits::default());
        let mut seen = Vec::new();
        while let Some(event) = reader.next_event().expect("well formed") {
            let (start, end) = reader.span();
            let kind = match event {
                XmlEvent::Start(element) => format!("start {}", element.name.local),
                XmlEvent::End(name) => format!("end {}", name.local),
                XmlEvent::Text(text) => format!("text {text:?}"),
            };
            seen.push((
                kind,
                String::from_utf8_lossy(&text[start..end]).into_owned(),
            ));
        }
        seen
    }

    /// Each event's span is the bytes it was read from, so a writer can copy a part's source
    /// bytes around the elements it replaces; a part that starts with a UTF-8 byte order mark
    /// gives the same bytes, its spans counted from the part's first byte.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn spans() {
        let text = br#"<?xml version="1.0"?>
<a xmlns="urn:x"><!-- c --><b k="1"/> t<c>u</c></a>"#;
        let expected = [
            ("start a", r#"<a xmlns="urn:x">"#),
            ("start b", r#"<b k="1"/>"#),
            ("end b", ""),
            ("text \" t\"", " t"),
            ("start c", "<c>"),
            ("text \"u\"", "u"),
            ("end c", "</c>"),
            ("end a", "</a>"),
        ];
        let expected: Vec<(String, String)> = expected
            .iter()
            .map(|(kind, bytes)| ((*kind).to_owned(), (*bytes).to_owned()))
            .collect();
        assert_eq!(spanned(text), expected);
        let marked = [&UTF8_BOM[..], &text[..]].concat();
        assert_eq!(spanned(&marked), expected, "after a byte order mark");
        let mut reader = XmlReader::new(&marked, XmlLimits::default());
        reader.next_event().expect("the document element");
        assert_eq!(
            reader.span(),
            (25, 42),
            "counted from the part's first byte"
        );
    }

    /// dc-dxm: the default namespace an element name resolves to follows the declarations in
    /// scope, through a nested redeclaration, an undeclaration and the ends that close them,
    /// whatever the reader keeps of it between names.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_dxm__default_namespaces_follow_their_scope() {
        let text = br#"<a xmlns="urn:one"><b/><c xmlns="urn:two"><d/></c><e/><f xmlns=""><g/></f><x:h xmlns:x="urn:three"><i/></x:h><j/></a>"#;
        let mut reader = XmlReader::new(text, XmlLimits::default());
        let mut seen = Vec::new();
        while let Some(event) = reader.next_event().expect("well formed") {
            let name = match event {
                XmlEvent::Start(element) => element.name,
                XmlEvent::End(name) => name,
                XmlEvent::Text(_) => continue,
            };
            seen.push(format!(
                "{}{}",
                name.namespace.as_deref().map(|n| format!("{{{n}}}")).unwrap_or_default(),
                name.local
            ));
        }
        let expected = [
            "{urn:one}a", "{urn:one}b", "{urn:one}b", "{urn:two}c", "{urn:two}d", "{urn:two}d",
            "{urn:two}c", "{urn:one}e", "{urn:one}e", "f", "g", "g", "f", "{urn:three}h",
            "{urn:one}i", "{urn:one}i", "{urn:three}h", "{urn:one}j", "{urn:one}j", "{urn:one}a",
        ];
        assert_eq!(seen, expected);
    }

    /// A second byte order mark is refused, not dropped.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn second_byte_order_mark() {
        let text = [&UTF8_BOM[..], &UTF8_BOM[..], b"<a/>"].concat();
        let mut reader = XmlReader::new(&text, XmlLimits::default());
        assert_eq!(
            reader.next_event(),
            Err(XmlError::Malformed {
                position: 3,
                detail: "a second byte order mark".to_owned()
            })
        );
    }
}

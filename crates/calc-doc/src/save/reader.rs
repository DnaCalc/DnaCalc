//! A pull over one part's XML events with the bytes each was read from, turning a failure into a
//! [`SaveRefusal`] that names the part.

use super::SaveRefusal;
use crate::load::MAIN_NAMESPACE;
use crate::xml::{XmlElement, XmlEvent, XmlPart, XmlReader};

pub(super) struct Reader<'a> {
    reader: XmlReader<'a>,
    part: &'a str,
}

/// One event and the byte range of the part it was read from.
pub(super) type Spanned = (XmlEvent, (usize, usize));

impl<'a> Reader<'a> {
    pub(super) fn new(xml: &'a XmlPart) -> Self {
        Reader {
            reader: xml.reader(),
            part: xml.name(),
        }
    }

    /// A refusal of the part: it is not shaped as this writer rewrites it.
    pub(super) fn invalid(&self, detail: impl Into<String>) -> SaveRefusal {
        SaveRefusal::InvalidPart {
            part: self.part.to_owned(),
            detail: detail.into(),
        }
    }

    pub(super) fn next(&mut self) -> Result<Option<Spanned>, SaveRefusal> {
        match self.reader.next_event() {
            Ok(Some(event)) => Ok(Some((event, self.reader.span()))),
            Ok(None) => Ok(None),
            Err(error) => Err(self.invalid(error.to_string())),
        }
    }

    /// The next event, refused when the part ends first.
    pub(super) fn expect(&mut self) -> Result<Spanned, SaveRefusal> {
        self.next()?
            .ok_or_else(|| self.invalid("the part ends inside an element"))
    }

    /// Reads the document element, which must be `local` in the SpreadsheetML namespace.
    pub(super) fn root(
        &mut self,
        local: &str,
    ) -> Result<(XmlElement, (usize, usize)), SaveRefusal> {
        match self.next()? {
            Some((XmlEvent::Start(root), span)) if root.name.is(MAIN_NAMESPACE, local) => {
                Ok((root, span))
            }
            _ => Err(self.invalid(format!(
                "the document element is not a SpreadsheetML {local}"
            ))),
        }
    }

    /// Reads to the end of the element whose start tag was just read; returns the offset just
    /// past it.
    pub(super) fn element_end(&mut self) -> Result<usize, SaveRefusal> {
        let mut depth = 1u32;
        loop {
            match self.expect()? {
                (XmlEvent::Start(_), _) => depth += 1,
                (XmlEvent::End(_), (_, end)) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(end);
                    }
                }
                (XmlEvent::Text(_), _) => {}
            }
        }
    }

    /// Checks that nothing follows the document element.
    pub(super) fn end(&mut self) -> Result<(), SaveRefusal> {
        match self.next()? {
            None => Ok(()),
            Some(_) => Err(self.invalid("content after the document element")),
        }
    }
}

/// Standalone source fragments retain the namespace declarations inherited from ancestors.
pub(crate) fn element_sources(
    xml: &XmlPart,
    mut matches: impl FnMut(&XmlElement) -> bool,
) -> Result<Vec<Vec<u8>>, SaveRefusal> {
    element_sources_in(xml, None, |e| matches(e))
}

/// Restrict source records to a standard worksheet path when the same element name can
/// appear in retained extension content.
pub(crate) fn element_sources_in(
    xml: &XmlPart,
    path: Option<&[&str]>,
    mut matches: impl FnMut(&XmlElement) -> bool,
) -> Result<Vec<Vec<u8>>, SaveRefusal> {
    use super::text::RawTag;
    let mut reader = Reader::new(xml);
    let mut parents: Vec<Vec<u8>> = Vec::new();
    let mut names: Vec<crate::xml::XmlName> = Vec::new();
    let mut out = Vec::new();
    while let Some((event, span)) = reader.next()? {
        match event {
            XmlEvent::Start(e) => {
                let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                let opening = parents.last().map_or_else(
                    || tag.rebuilt(&[], tag.is_empty()),
                    |p| tag.with_inherited_namespaces(&RawTag::parse(p)),
                );
                let in_path = path.is_none_or(|p| {
                    names.len() == p.len()
                        && names
                            .iter()
                            .zip(p)
                            .all(|(name, local)| name.is(MAIN_NAMESPACE, local))
                });
                if in_path && matches(&e) {
                    let end = reader.element_end()?;
                    let mut bytes = opening;
                    bytes.extend_from_slice(&xml.bytes()[span.1..end]);
                    out.push(bytes);
                } else {
                    parents.push(opening);
                    names.push(e.name);
                }
            }
            XmlEvent::End(_) => {
                parents.pop();
                names.pop();
            }
            _ => {}
        }
    }
    Ok(out)
}

pub(crate) fn without_revision_ids(xml: &XmlPart) -> Result<Vec<u8>, SaveRefusal> {
    use super::text::{RawTag, Splice};
    let mut reader = Reader::new(xml);
    let mut splice = Splice::new(xml.bytes());
    while let Some((event, span)) = reader.next()? {
        if let XmlEvent::Start(e) = event {
            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
            let raw = tag
                .attribute_names()
                .filter(|n| *n != b"xmlns" && !n.starts_with(b"xmlns:"))
                .collect::<Vec<_>>();
            let changes = e
                .attributes
                .iter()
                .zip(raw)
                .filter(|(a, _)| {
                    a.name.is(
                        "http://schemas.microsoft.com/office/spreadsheetml/2014/revision",
                        "uid",
                    )
                })
                .map(|(_, n)| (String::from_utf8_lossy(n).into_owned(), None))
                .collect::<Vec<(String, Option<String>)>>();
            if !changes.is_empty() {
                let changes = changes
                    .iter()
                    .map(|(n, v)| (n.as_str(), v.as_deref()))
                    .collect::<Vec<_>>();
                splice.replace(span, &tag.rebuilt(&changes, tag.is_empty()));
            }
        }
    }
    Ok(splice.finish())
}

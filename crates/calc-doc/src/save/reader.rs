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

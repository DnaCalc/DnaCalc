//! The shared strings table a save writes new text into: every item the part holds keeps its
//! index, and text no plain item holds is appended.

use super::SaveRefusal;
use super::reader::Reader;
use super::text::{RawTag, Splice, needs_preserve, xstring};
use crate::load::MAIN_NAMESPACE;
use crate::load::sheet::collapsed;
use crate::load::strings::decode;
use crate::opc::Package;
use crate::xml::{XmlEvent, XmlPart};
use std::collections::BTreeMap;

/// The content type of a shared strings part.
pub(super) const CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";

pub(super) struct SharedStrings {
    /// The workbook's shared strings part, if it has one.
    part: Option<String>,
    /// The number of items (`<si>`) the part holds.
    held: u32,
    /// The text of each item that is one `<t>` and nothing else, with its first index. Text equal
    /// to a rich or phonetic item is not written as that item, whose runs would format it.
    plain: BTreeMap<Vec<u16>, u32>,
    /// The text appended, in index order after the held items.
    appended: Vec<Vec<u16>>,
    appended_index: BTreeMap<Vec<u16>, u32>,
    /// References to the table the rewritten cells added and removed.
    pub(super) added: u64,
    pub(super) removed: u64,
}

pub(super) fn read_part(package: &Package, part: &str) -> Result<XmlPart, SaveRefusal> {
    package.read_xml(part).map_err(|error| SaveRefusal::Part {
        part: part.to_owned(),
        error,
    })
}

pub(crate) fn decorated_indices(
    xml: &XmlPart,
) -> Result<std::collections::BTreeSet<u32>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    reader.root("sst")?;
    let mut decorated = std::collections::BTreeSet::new();
    let mut index = 0;
    while let Some((event, _)) = reader.next()? {
        if let XmlEvent::Start(e) = event {
            if e.name.is(MAIN_NAMESPACE, "si") {
                if plain_item(&mut reader)?.is_none() {
                    decorated.insert(index);
                }
                index += 1;
            } else {
                reader.element_end()?;
            }
        }
    }
    Ok(decorated)
}

impl SharedStrings {
    /// The table of the workbook's shared strings part `part`, or an empty one without a part.
    pub(super) fn read(package: &Package, part: Option<&str>) -> Result<Self, SaveRefusal> {
        let mut table = SharedStrings {
            part: part.map(str::to_owned),
            held: 0,
            plain: BTreeMap::new(),
            appended: Vec::new(),
            appended_index: BTreeMap::new(),
            added: 0,
            removed: 0,
        };
        let Some(part) = part else {
            return Ok(table);
        };
        let xml = read_part(package, part)?;
        let mut reader = Reader::new(&xml);
        reader.root("sst")?;
        loop {
            match reader.expect()? {
                (XmlEvent::Start(element), _) if element.name.is(MAIN_NAMESPACE, "si") => {
                    let index = table.held;
                    table.held = table.held.checked_add(1).ok_or_else(|| {
                        reader.invalid("more shared strings than an index counts")
                    })?;
                    if let Some(text) = plain_item(&mut reader)? {
                        table.plain.entry(text).or_insert(index);
                    }
                }
                (XmlEvent::Start(_), _) => {
                    reader.element_end()?;
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), _) => break,
            }
        }
        reader.end()?;
        Ok(table)
    }

    pub(super) fn part(&self) -> Option<&str> {
        self.part.as_deref()
    }

    /// The index of `text`: a plain item's that holds it, else an appended one's.
    pub(super) fn index(&mut self, text: &[u16]) -> u32 {
        if let Some(&index) = self.plain.get(text) {
            return index;
        }
        if let Some(&index) = self.appended_index.get(text) {
            return index;
        }
        let index = self.held + self.appended.len() as u32;
        self.appended.push(text.to_vec());
        self.appended_index.insert(text.to_vec(), index);
        index
    }

    pub(super) fn appended(&self) -> usize {
        self.appended.len()
    }

    /// Whether the part must be rewritten: text was appended, or the references it counts changed.
    pub(super) fn changed(&self) -> bool {
        !self.appended.is_empty() || self.added != self.removed
    }

    fn items(&self, prefix: &[u8], out: &mut Vec<u8>) {
        for text in &self.appended {
            out.push(b'<');
            out.extend_from_slice(prefix);
            out.extend_from_slice(b"si><");
            out.extend_from_slice(prefix);
            out.push(b't');
            if needs_preserve(text) {
                out.extend_from_slice(br#" xml:space="preserve""#);
            }
            out.push(b'>');
            xstring(text, out);
            out.extend_from_slice(b"</");
            out.extend_from_slice(prefix);
            out.extend_from_slice(b"t></");
            out.extend_from_slice(prefix);
            out.extend_from_slice(b"si>");
        }
    }

    /// The part rewritten: every item kept as the part wrote it, the new text appended after the
    /// last item (before the table's `extLst`, when it has one), and the `count` and `uniqueCount`
    /// the part declares, when it declares them, brought up to date.
    pub(super) fn rewrite(&self, xml: &XmlPart) -> Result<Vec<u8>, SaveRefusal> {
        let mut reader = Reader::new(xml);
        let (root, span) = reader.root("sst")?;
        let mut splice = Splice::new(xml.bytes());
        let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
        let count = match root.attribute("count") {
            None => None,
            Some(text) => {
                let old: u64 = collapsed(text)
                    .parse()
                    .map_err(|_| reader.invalid("an sst count that is not a number"))?;
                Some((old + self.added).saturating_sub(self.removed).to_string())
            }
        };
        let unique = root
            .attribute("uniqueCount")
            .map(|_| (u64::from(self.held) + self.appended.len() as u64).to_string());
        let mut changes = Vec::new();
        if let Some(count) = &count {
            changes.push(("count", Some(count.as_str())));
        }
        if let Some(unique) = &unique {
            changes.push(("uniqueCount", Some(unique.as_str())));
        }
        let prefix = tag.prefix().to_vec();
        let mut items = Vec::new();
        self.items(&prefix, &mut items);
        if tag.is_empty() {
            let mut bytes = tag.rebuilt(&changes, false);
            bytes.extend_from_slice(&items);
            bytes.extend_from_slice(b"</");
            bytes.extend_from_slice(&prefix);
            bytes.extend_from_slice(b"sst>");
            splice.replace(span, &bytes);
            reader.expect()?;
        } else {
            splice.replace(span, &tag.rebuilt(&changes, false));
            // The table holds its items and then an `extLst` (CT_Sst), so the new items go before
            // the first element that is not an item, and an item after such an element, whose
            // index the new items would move, is refused.
            let mut placed = false;
            loop {
                match reader.expect()? {
                    (XmlEvent::Start(element), _) if element.name.is(MAIN_NAMESPACE, "si") => {
                        if placed {
                            return Err(reader
                                .invalid("a shared string item after an element that is not one"));
                        }
                        reader.element_end()?;
                    }
                    (XmlEvent::Start(_), (start, _)) => {
                        if !placed {
                            splice.keep_to(start);
                            splice.put(&items);
                            placed = true;
                        }
                        reader.element_end()?;
                    }
                    (XmlEvent::Text(_), _) => {}
                    (XmlEvent::End(_), (start, _)) => {
                        if !placed {
                            splice.keep_to(start);
                            splice.put(&items);
                        }
                        break;
                    }
                }
            }
        }
        reader.end()?;
        Ok(splice.finish())
    }

    /// A new shared strings part holding the appended text, for a workbook that had none.
    pub(super) fn new_part(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
        out.extend_from_slice(b"\r\n<sst xmlns=\"");
        out.extend_from_slice(MAIN_NAMESPACE.as_bytes());
        out.extend_from_slice(
            format!(
                "\" count=\"{}\" uniqueCount=\"{}\">",
                self.added,
                self.appended.len()
            )
            .as_bytes(),
        );
        self.items(b"", &mut out);
        out.extend_from_slice(b"</sst>");
        out
    }
}

/// Reads one `<si>` whose start tag was just read: its text when it is exactly one `<t>` element,
/// `None` when it holds runs, phonetic text or anything else.
fn plain_item(reader: &mut Reader<'_>) -> Result<Option<Vec<u16>>, SaveRefusal> {
    let mut text = None;
    let mut plain = true;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(element), _) => {
                if plain && text.is_none() && element.name.is(MAIN_NAMESPACE, "t") {
                    let mut content = String::new();
                    loop {
                        match reader.expect()? {
                            (XmlEvent::Text(more), _) => content.push_str(&more),
                            (XmlEvent::End(_), _) => break,
                            (XmlEvent::Start(_), _) => {
                                return Err(reader.invalid("an element inside a t element"));
                            }
                        }
                    }
                    text = Some(decode(&content));
                } else {
                    plain = false;
                    reader.element_end()?;
                }
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), _) => return Ok(if plain { text } else { None }),
        }
    }
}

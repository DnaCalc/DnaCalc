//! Write a worksheet's authored merged areas: the `mergeCells` element replaced (or added after
//! the elements ECMA-376 puts before it) and dropped when no area remains (dc-oc78o).
use super::{
    SaveRefusal,
    reader::Reader,
    text::{RawTag, Splice, a1},
};
use crate::{
    load::MAIN_NAMESPACE,
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::value::GridArea;

/// Elements that follow `mergeCells` in the worksheet schema's sequence.
const AFTER: &[&str] = &[
    "phoneticPr",
    "conditionalFormatting",
    "dataValidations",
    "hyperlinks",
    "printOptions",
    "pageMargins",
    "pageSetup",
    "headerFooter",
    "rowBreaks",
    "colBreaks",
    "customProperties",
    "cellWatches",
    "ignoredErrors",
    "smartTags",
    "drawing",
    "legacyDrawing",
    "legacyDrawingHF",
    "drawingHF",
    "picture",
    "oleObjects",
    "controls",
    "webPublishItems",
    "tableParts",
    "extLst",
];

pub(super) fn rewrite(xml: &XmlPart, areas: &[GridArea]) -> Result<Vec<u8>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (_, root_span) = reader.root("worksheet")?;
    let root = RawTag::parse(&xml.bytes()[root_span.0..root_span.1]);
    let prefix = String::from_utf8_lossy(root.prefix()).into_owned();
    let mut replacement = Vec::new();
    if !areas.is_empty() {
        replacement.extend_from_slice(
            format!("<{prefix}mergeCells count=\"{}\">", areas.len()).as_bytes(),
        );
        for area in areas {
            let (first, last) = (area.first(), area.last());
            replacement.extend_from_slice(
                format!(
                    "<{prefix}mergeCell ref=\"{}:{}\"/>",
                    a1(first.row(), first.column()),
                    a1(last.row(), last.column())
                )
                .as_bytes(),
            );
        }
        replacement.extend_from_slice(format!("</{prefix}mergeCells>").as_bytes());
    }
    if root.is_empty() {
        let mut body = root.rebuilt(&[], false);
        body.extend_from_slice(&replacement);
        body.extend_from_slice(format!("</{prefix}worksheet>").as_bytes());
        let mut splice = Splice::new(xml.bytes());
        splice.replace(root_span, &body);
        return Ok(splice.finish());
    }
    let mut splice = Splice::new(xml.bytes());
    let mut inserted = false;
    while let Some((event, span)) = reader.next()? {
        match event {
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE, "mergeCells") => {
                let end = reader.element_end()?;
                splice.replace(
                    (span.0, end),
                    if inserted {
                        &[]
                    } else {
                        inserted = true;
                        &replacement
                    },
                );
            }
            XmlEvent::Start(e)
                if !inserted
                    && e.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
                    && AFTER.contains(&e.name.local.as_ref()) =>
            {
                splice.replace((span.0, span.0), &replacement);
                inserted = true;
                reader.element_end()?;
            }
            XmlEvent::End(e) if !inserted && e.is(MAIN_NAMESPACE, "worksheet") => {
                splice.replace((span.0, span.0), &replacement);
                inserted = true;
            }
            XmlEvent::Start(_) => {
                reader.element_end()?;
            }
            _ => {}
        }
    }
    Ok(splice.finish())
}

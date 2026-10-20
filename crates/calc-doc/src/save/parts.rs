//! The workbook-level parts an authored save changes, each rewritten from its own bytes: the
//! workbook part's `calcPr`, the workbook part's relationships, and `[Content_Types].xml`.

use super::SaveRefusal;
use super::reader::Reader;
use super::text::{RawTag, Splice, attribute_value};
use crate::load::MAIN_NAMESPACE;
use crate::opc::{CONTENT_TYPES_NAMESPACE, RELATIONSHIPS_NAMESPACE};
use crate::xml::{XmlEvent, XmlPart};

/// The elements CT_Workbook places after `calcPr`, before which a missing `calcPr` is inserted.
const AFTER_CALC_PR: [&str; 9] = [
    "oleSize",
    "customWorkbookViews",
    "pivotCaches",
    "smartTagPr",
    "smartTagTypes",
    "webPublishing",
    "fileRecoveryPr",
    "webPublishObjects",
    "extLst",
];

/// The workbook part with `fullCalcOnLoad="1"` on its `calcPr`, which asks the application that
/// opens the package to recalculate every formula (whether Excel does is measured by infra-m1.3's
/// round trip): set on the `calcPr` the part holds, its other attributes kept, or a new `calcPr`
/// at the place the schema gives it. Refused when the part holds two.
pub(super) fn full_calc_on_load(xml: &XmlPart) -> Result<Vec<u8>, SaveRefusal> {
    let source = xml.bytes();
    let mut reader = Reader::new(xml);
    let (_, root) = reader.root("workbook")?;
    let prefix = RawTag::parse(&source[root.0..root.1]).prefix().to_vec();
    let mut calc_pr = None;
    let mut insert_at = None;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(element), span) => {
                if element.name.is(MAIN_NAMESPACE, "calcPr") {
                    if calc_pr.is_some() {
                        return Err(reader.invalid("a workbook with two calcPr elements"));
                    }
                    calc_pr = Some(span);
                } else if insert_at.is_none()
                    && element.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
                    && AFTER_CALC_PR.contains(&element.name.local.as_str())
                {
                    insert_at = Some(span.0);
                }
                reader.element_end()?;
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), (start, _)) => {
                insert_at.get_or_insert(start);
                break;
            }
        }
    }
    reader.end()?;
    let mut splice = Splice::new(source);
    match calc_pr {
        Some(span) => {
            let tag = RawTag::parse(&source[span.0..span.1]);
            let empty = tag.is_empty();
            splice.replace(span, &tag.rebuilt(&[("fullCalcOnLoad", Some("1"))], empty));
        }
        None => {
            let at = insert_at.expect("set when the document element ends");
            let mut element = b"<".to_vec();
            element.extend_from_slice(&prefix);
            element.extend_from_slice(br#"calcPr fullCalcOnLoad="1"/>"#);
            splice.replace((at, at), &element);
        }
    }
    Ok(splice.finish())
}

/// A relationship to add.
pub(super) struct NewRelationship<'a> {
    pub(super) id: &'a str,
    pub(super) kind: &'a str,
    pub(super) target: &'a str,
}

/// A relationships part without its relationships of type `remove`, and with `add` appended.
pub(super) fn relationships(
    xml: &XmlPart,
    remove: Option<&str>,
    add: Option<&NewRelationship<'_>>,
) -> Result<Vec<u8>, SaveRefusal> {
    let source = xml.bytes();
    let mut reader = Reader::new(xml);
    let root = match reader.next()? {
        Some((XmlEvent::Start(root), span))
            if root.name.is(RELATIONSHIPS_NAMESPACE, "Relationships") =>
        {
            span
        }
        _ => return Err(reader.invalid("the document element is not Relationships")),
    };
    let root_tag = RawTag::parse(&source[root.0..root.1]);
    let prefix = root_tag.prefix().to_vec();
    let mut splice = Splice::new(source);
    let mut added = Vec::new();
    if let Some(add) = add {
        added.push(b'<');
        added.extend_from_slice(&prefix);
        added.extend_from_slice(b"Relationship Id=");
        attribute_value(add.id, &mut added);
        added.extend_from_slice(b" Type=");
        attribute_value(add.kind, &mut added);
        added.extend_from_slice(b" Target=");
        attribute_value(add.target, &mut added);
        added.extend_from_slice(b"/>");
    }
    if root_tag.is_empty() {
        reader.expect()?;
        if add.is_some() {
            let mut bytes = root_tag.rebuilt(&[], false);
            bytes.extend_from_slice(&added);
            bytes.extend_from_slice(b"</");
            bytes.extend_from_slice(&prefix);
            bytes.extend_from_slice(b"Relationships>");
            splice.replace(root, &bytes);
        }
    } else {
        loop {
            match reader.expect()? {
                (XmlEvent::Start(element), span) => {
                    let end = reader.element_end()?;
                    if remove.is_some() && element.attribute("Type") == remove {
                        splice.keep_to(span.0);
                        splice.skip_to(end);
                    }
                }
                (XmlEvent::Text(_), _) => {}
                (XmlEvent::End(_), (start, _)) => {
                    splice.keep_to(start);
                    splice.put(&added);
                    break;
                }
            }
        }
    }
    reader.end()?;
    Ok(splice.finish())
}

/// `[Content_Types].xml` without the `Override` of the part `remove` (a ZIP item name, compared
/// ignoring ASCII case), and with an `Override` giving the part `add.0` the content type `add.1`.
pub(super) fn content_types(
    xml: &XmlPart,
    remove: Option<&str>,
    add: Option<(&str, &str)>,
) -> Result<Vec<u8>, SaveRefusal> {
    let source = xml.bytes();
    let mut reader = Reader::new(xml);
    let root = match reader.next()? {
        Some((XmlEvent::Start(root), span)) if root.name.is(CONTENT_TYPES_NAMESPACE, "Types") => {
            span
        }
        _ => return Err(reader.invalid("the document element is not Types")),
    };
    let root_tag = RawTag::parse(&source[root.0..root.1]);
    let prefix = root_tag.prefix().to_vec();
    let mut added = Vec::new();
    if let Some((part, content_type)) = add {
        added.push(b'<');
        added.extend_from_slice(&prefix);
        added.extend_from_slice(b"Override PartName=");
        attribute_value(&format!("/{part}"), &mut added);
        added.extend_from_slice(b" ContentType=");
        attribute_value(content_type, &mut added);
        added.extend_from_slice(b"/>");
    }
    let mut splice = Splice::new(source);
    if root_tag.is_empty() {
        return Err(reader.invalid("a Types element without content types"));
    }
    loop {
        match reader.expect()? {
            (XmlEvent::Start(element), span) => {
                let end = reader.element_end()?;
                let named = element
                    .attribute("PartName")
                    .and_then(|name| name.strip_prefix('/'));
                if element.name.is(CONTENT_TYPES_NAMESPACE, "Override")
                    && let (Some(name), Some(remove)) = (named, remove)
                    && name.eq_ignore_ascii_case(remove)
                {
                    splice.keep_to(span.0);
                    splice.skip_to(end);
                }
            }
            (XmlEvent::Text(_), _) => {}
            (XmlEvent::End(_), (start, _)) => {
                splice.keep_to(start);
                splice.put(&added);
                break;
            }
        }
    }
    reader.end()?;
    Ok(splice.finish())
}

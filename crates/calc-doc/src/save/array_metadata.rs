//! The cell metadata an authored dynamic-array formula's cell names (`cm`): a `cellMetadata`
//! record of the workbook's metadata part whose type is `XLDAPR` with
//! `dynamicArrayProperties fDynamic="1" fCollapsed="0"`, the record Excel writes for a dynamic
//! array. A package whose metadata part holds such a record keeps the part as it is and the
//! formula names that record; one whose part holds none gets the record appended (a metadata type,
//! a `futureMetadata` record and a `cellMetadata` record, each after the part's own); one without
//! a metadata part gets one, with the part, its workbook relationship and its content type, as
//! Excel writes it.

use super::SaveRefusal;
use crate::load::metadata::{self, CellMeta};
use crate::opc::Package;

pub(super) const RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata";
pub(super) const CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml";

const METADATA_TYPE: &str = r#"<metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1" cellMeta="1"/>"#;
const FUTURE_RECORD: &str = r#"<bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk>"#;
const DYNAMIC_ARRAY_NAMESPACE: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray";

/// The `cm` an authored dynamic array's cell takes, and how the package changes for it.
pub(super) enum ArrayMetadata {
    /// The metadata part already holds the record: `cm` is its index.
    Present { cm: u32 },
    /// The metadata part at `part` is rewritten with the record appended at index `cm`.
    Rewrite {
        cm: u32,
        part: String,
        content: Vec<u8>,
    },
    /// The package has no metadata part: one is added (cm 1).
    Create { content: Vec<u8> },
}

/// The metadata part Excel writes for a workbook whose only cell metadata is one dynamic array.
pub(super) fn new_part() -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xda="{DYNAMIC_ARRAY_NAMESPACE}"><metadataTypes count="1">{METADATA_TYPE}</metadataTypes><futureMetadata name="XLDAPR" count="1">{FUTURE_RECORD}</futureMetadata><cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>"#
    )
    .into_bytes()
}

fn invalid(part: &str, detail: &str) -> SaveRefusal {
    SaveRefusal::InvalidPart {
        part: part.to_owned(),
        detail: detail.to_owned(),
    }
}

/// Sets the `count` attribute of the start tag that begins at `at` in `text` to `count`, or
/// adds it when the tag has none.
fn set_count(text: &mut String, at: usize, count: usize) {
    let end = at + text[at..].find('>').expect("a start tag ends");
    let tag = &text[at..end];
    match tag.find(" count=\"") {
        Some(offset) => {
            let value_start = at + offset + " count=\"".len();
            let value_end = value_start + text[value_start..].find('"').expect("a quoted value");
            text.replace_range(value_start..value_end, &count.to_string());
        }
        None => {
            let insert = if tag.ends_with('/') { end - 1 } else { end };
            text.insert_str(insert, &format!(" count=\"{count}\""));
        }
    }
}

/// The start of the first `<name` start tag (followed by a space, `>` or `/`) in `text`.
fn start_tag(text: &str, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(found) = text[from..].find(&open) {
        let at = from + found;
        match text.as_bytes().get(at + open.len()) {
            Some(b' ' | b'>' | b'/' | b'\t' | b'\r' | b'\n') => return Some(at),
            _ => from = at + open.len(),
        }
    }
    None
}

/// Counts the `<child` start tags inside the element whose start tag begins at `at`.
fn children(text: &str, at: usize, name: &str, child: &str) -> usize {
    let close = format!("</{name}>");
    let body_start = at + text[at..].find('>').expect("a start tag ends") + 1;
    if text[..body_start].ends_with("/>") {
        return 0;
    }
    let body_end = body_start + text[body_start..].find(&close).unwrap_or(0);
    let body = &text[body_start..body_end];
    let mut count = 0;
    let mut from = 0;
    while let Some(found) = start_tag(&body[from..], child) {
        count += 1;
        from += found + child.len() + 1;
    }
    count
}

/// Appends `inner` as the last child of the element whose start tag begins at `at` (expanding an
/// empty element).
fn append(text: &mut String, at: usize, name: &str, inner: &str) {
    let tag_end = at + text[at..].find('>').expect("a start tag ends");
    if text[..tag_end].ends_with('/') {
        text.replace_range(tag_end - 1..=tag_end, &format!(">{inner}</{name}>"));
        return;
    }
    let close = format!("</{name}>");
    let end = tag_end + text[tag_end..].find(&close).expect("the element closes");
    text.insert_str(end, inner);
}

/// The `cm` for an authored dynamic array in this package, whose workbook part is `workbook`.
pub(super) fn array_metadata(
    package: &Package,
    workbook: &str,
) -> Result<ArrayMetadata, SaveRefusal> {
    let refusal = |error: crate::load::LoadError| invalid(workbook, &error.to_string());
    let Some(part) = metadata::part(package, workbook) else {
        return Ok(ArrayMetadata::Create {
            content: new_part(),
        });
    };
    let read = metadata::read(package, workbook).map_err(refusal)?;
    for index in 1..=read.cell_count() {
        if read.cell(index)
            == (CellMeta::DynamicArray {
                dynamic: true,
                collapsed: false,
            })
        {
            return Ok(ArrayMetadata::Present { cm: index });
        }
    }
    let bytes = package
        .read_part(&part)
        .map_err(|error| SaveRefusal::Part {
            part: part.clone(),
            error,
        })?;
    let mut text = String::from_utf8(bytes)
        .map_err(|_| invalid(&part, "a metadata part that is not UTF-8"))?;
    let Some(root) = start_tag(&text, "metadata") else {
        return Err(invalid(
            &part,
            "a metadata part whose elements carry a namespace prefix, which this writer does not extend",
        ));
    };
    // The metadata type, last among the types.
    let types = match start_tag(&text, "metadataTypes") {
        Some(at) => at,
        None => {
            let insert = root + text[root..].find('>').expect("a start tag ends") + 1;
            text.insert_str(insert, r#"<metadataTypes count="0"></metadataTypes>"#);
            insert
        }
    };
    let type_index = children(&text, types, "metadataTypes", "metadataType") + 1;
    append(&mut text, types, "metadataTypes", METADATA_TYPE);
    set_count(&mut text, types, type_index);
    // The future record: in an XLDAPR block, a new one before the cell or value metadata.
    let record = match text.find(r#"<futureMetadata name="XLDAPR""#) {
        Some(at) => {
            let index = children(&text, at, "futureMetadata", "bk");
            append(&mut text, at, "futureMetadata", &future_record());
            set_count(&mut text, at, index + 1);
            index
        }
        None => {
            let block = format!(
                r#"<futureMetadata name="XLDAPR" count="1">{}</futureMetadata>"#,
                future_record()
            );
            let at = start_tag(&text, "cellMetadata")
                .or_else(|| start_tag(&text, "valueMetadata"))
                .or_else(|| start_tag(&text, "extLst"))
                .or_else(|| text.rfind("</metadata>"))
                .ok_or_else(|| invalid(&part, "a metadata part without its end tag"))?;
            text.insert_str(at, &block);
            0
        }
    };
    // The cell metadata record.
    let cell_record = format!(r#"<bk><rc t="{type_index}" v="{record}"/></bk>"#);
    let cm = match start_tag(&text, "cellMetadata") {
        Some(at) => {
            let index = children(&text, at, "cellMetadata", "bk") + 1;
            append(&mut text, at, "cellMetadata", &cell_record);
            set_count(&mut text, at, index);
            index
        }
        None => {
            let at = start_tag(&text, "valueMetadata")
                .or_else(|| start_tag(&text, "extLst"))
                .or_else(|| text.rfind("</metadata>"))
                .ok_or_else(|| invalid(&part, "a metadata part without its end tag"))?;
            text.insert_str(
                at,
                &format!(r#"<cellMetadata count="1">{cell_record}</cellMetadata>"#),
            );
            1
        }
    };
    Ok(ArrayMetadata::Rewrite {
        cm: u32::try_from(cm).unwrap_or(u32::MAX),
        part,
        content: text.into_bytes(),
    })
}

/// The future record, declaring the dynamic-array namespace itself so a part that does not
/// declare it stays well formed.
fn future_record() -> String {
    FUTURE_RECORD.replace(
        "<xda:dynamicArrayProperties ",
        &format!(r#"<xda:dynamicArrayProperties xmlns:xda="{DYNAMIC_ARRAY_NAMESPACE}" "#),
    )
}

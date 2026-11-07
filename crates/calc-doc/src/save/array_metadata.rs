//! The cell and value metadata a save writes (`xl/metadata.xml` and the rich value parts it
//! leads to).
//!
//! - The cell metadata an authored dynamic-array formula's cell names (`cm`): a `cellMetadata`
//!   record of the workbook's metadata part whose type is `XLDAPR` with
//!   `dynamicArrayProperties fDynamic="1" fCollapsed="0"`, the record Excel writes for a dynamic
//!   array. A package whose metadata part holds such a record keeps the part as it is and the
//!   formula names that record; one whose part holds none gets the record appended (a metadata
//!   type, a `futureMetadata` record and a `cellMetadata` record, each after the part's own); one
//!   without a metadata part gets one, with the part, its workbook relationship and its content
//!   type, as Excel writes it.
//! - The value metadata a formula cell whose value is a newer error names (`vm`), as Excel writes
//!   it (calc-doc's `tests/fixtures/cdoc_m1_2/d1.saved.xlsx`, saved by Excel 16.0 build 20430,
//!   holds #SPILL! and #CALC! this way): a `valueMetadata` record of type `XLRICHVALUE` whose
//!   `futureMetadata` record's `xlrd:rvb i` names a rich value of `rdrichvalue.xml`, whose
//!   structure in `rdrichvaluestructure.xml` is an `_error` with the key `errorType`, the error's
//!   `ERROR.TYPE` minus 1. Excel also writes a `subType` (and, for #SPILL!, the offsets of the
//!   cell that blocks the spill); the engine does not report them, so the structure written here
//!   holds `errorType` alone. Records are appended to the parts a package holds, and the parts
//!   (with their relationships and content types) are added to a package without them, with
//!   `rdRichValueTypes.xml` as Excel writes it.

use super::SaveRefusal;
use super::author::new_part_name;
use crate::load::metadata::{self, CellMeta};
use crate::opc::Package;
use calc_doc_model::value::{ImageValue, WorksheetErrorCode};
use std::collections::{BTreeMap, BTreeSet};

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

/// Where an element goes among the metadata part's own children (CT_Metadata: metadataTypes,
/// metadataStrings, mdxMetadata, futureMetadata..., cellMetadata, valueMetadata, extLst): just
/// after the last of `before` the part holds, the elements that precede it, latest first; just
/// after the root's start tag when it holds none. A search for the first `<extLst` would find one
/// inside a futureMetadata record (the dynamic-array properties are held in one), and an element
/// put there makes the part one the loader refuses (dc-1yp).
fn after_last(text: &str, root: usize, before: &[&str]) -> usize {
    for name in before {
        let close = format!("</{name}>");
        if let Some(at) = text.rfind(&close) {
            return at + close.len();
        }
        if let Some(at) = start_tag(text, name) {
            // An empty element, `<name .../>`.
            return at + text[at..].find('>').expect("a start tag ends") + 1;
        }
    }
    root + text[root..].find('>').expect("a start tag ends") + 1
}

/// The elements that precede a futureMetadata or a cellMetadata block, latest first.
const BEFORE_CELL_METADATA: [&str; 4] = [
    "futureMetadata",
    "mdxMetadata",
    "metadataStrings",
    "metadataTypes",
];
/// The elements that precede a valueMetadata block, latest first.
const BEFORE_VALUE_METADATA: [&str; 5] = [
    "cellMetadata",
    "futureMetadata",
    "mdxMetadata",
    "metadataStrings",
    "metadataTypes",
];

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
            let at = after_last(&text, root, &BEFORE_CELL_METADATA);
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
            let at = after_last(&text, root, &BEFORE_CELL_METADATA);
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

const RICH_VALUE_TYPE: &str = r#"<metadataType name="XLRICHVALUE" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1"/>"#;
const RICH_DATA_NAMESPACE: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
const ERROR_STRUCTURE: &str = r#"<s t="_error"><k n="errorType" t="i"/></s>"#;

const WEB_IMAGE_RELATIONSHIP: &str =
    "http://schemas.microsoft.com/office/2020/07/relationships/rdRichValueWebImage";
const WEB_IMAGE_CONTENT_TYPE: &str = "application/vnd.ms-excel.rdrichvaluewebimage+xml";
const WEB_IMAGE_NAMESPACE: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2020/richdatawebimage";
const RELATIONSHIPS_NAMESPACE: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_RELATIONSHIPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const RELATIONSHIPS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.relationships+xml";

const RICH_VALUE_RELATIONSHIP: &str =
    "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValue";
const RICH_VALUE_CONTENT_TYPE: &str = "application/vnd.ms-excel.rdrichvalue+xml";
const STRUCTURE_RELATIONSHIP: &str =
    "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueStructure";
const STRUCTURE_CONTENT_TYPE: &str = "application/vnd.ms-excel.rdrichvaluestructure+xml";
const TYPES_RELATIONSHIP: &str =
    "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueTypes";
const TYPES_CONTENT_TYPE: &str = "application/vnd.ms-excel.rdrichvaluetypes+xml";

/// `rdRichValueTypes.xml` as Excel 16.0 build 20430 writes it for a workbook whose rich values are
/// errors (`d1.saved.xlsx`).
const TYPES_PART: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<rvTypesInfo xmlns="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata2" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="x" xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><global><keyFlags><key name="_Self"><flag name="ExcludeFromFile" value="1"/><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_DisplayString"><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_Flags"><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_Format"><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_SubLabel"><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_Attribution"><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_Icon"><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_Display"><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_CanonicalPropertyNames"><flag name="ExcludeFromCalcComparison" value="1"/></key><key name="_ClassificationId"><flag name="ExcludeFromCalcComparison" value="1"/></key></keyFlags></global></rvTypesInfo>"#;

/// The `errorType` of an `_error` rich value for a newer error (`ERROR.TYPE` minus 1, the inverse
/// of the loader's `error_of_type`); `None` for an error Excel holds without value metadata, and
/// for #BUSY!, whose `errorType` is not known here.
pub(super) fn rich_error_type(code: WorksheetErrorCode) -> Option<u32> {
    use WorksheetErrorCode::*;
    Some(match code {
        GettingData => 7,
        Spill => 8,
        Connect => 9,
        Blocked => 10,
        Field => 12,
        Calc => 13,
        Null | Div0 | Value | Ref | Name | Num | NA | Busy => return None,
    })
}

/// A part a save adds for its metadata, related from the workbook part.
pub(super) struct NewPart {
    pub(super) name: String,
    pub(super) target: String,
    pub(super) content: Vec<u8>,
    pub(super) content_type: &'static str,
    pub(super) relationship: &'static str,
}

/// The `cm` and `vm` indexes a save's cells name, and the parts it rewrites or adds for them.
#[derive(Default)]
pub(super) struct MetadataPlan {
    pub(super) cm: Option<u32>,
    /// The `vm` of each newer error's record, by the error's `errorType`.
    pub(super) vm: BTreeMap<u32, u32>,
    /// The `vm` of each picture's record.
    pub(super) image_vm: Vec<(ImageValue, u32)>,
    pub(super) rewritten: Vec<(String, Vec<u8>)>,
    pub(super) added: Vec<NewPart>,
    /// Parts no workbook relationship names (a media part, a relationships part).
    pub(super) unrelated: Vec<(String, Vec<u8>, &'static str)>,
}

/// The 1-based index of the first `<child` start tag inside the element `parent` (whose start tag
/// is the first `<parent` of `text`) for which `matches` holds of the tag's text.
fn position_of(
    text: &str,
    parent: &str,
    child: &str,
    matches: impl Fn(&str) -> bool,
) -> Option<usize> {
    let open = start_tag(text, parent)?;
    let close = format!("</{parent}>");
    let body_end = open + text[open..].find(&close)?;
    let mut from = open + parent.len() + 1;
    let mut index = 0;
    while let Some(found) = start_tag(&text[from..body_end], child) {
        let at = from + found;
        index += 1;
        let end = at + text[at..].find('>').expect("a start tag ends");
        if matches(&text[at..end]) {
            return Some(index);
        }
        from = at + child.len() + 1;
    }
    None
}

/// The metadata a save needs: a `cm` for authored dynamic arrays when `dynamic`, and a `vm` for
/// each error of `errors` that Excel holds with value metadata.
pub(super) fn plan(
    package: &Package,
    workbook: &str,
    dynamic: bool,
    errors: &[WorksheetErrorCode],
    images: &[ImageValue],
) -> Result<MetadataPlan, SaveRefusal> {
    let errors: BTreeSet<u32> = errors
        .iter()
        .filter_map(|code| rich_error_type(*code))
        .collect();
    let mut out = MetadataPlan::default();
    if !dynamic && errors.is_empty() && images.is_empty() {
        return Ok(out);
    }
    // The metadata part, with the dynamic-array record when one is needed.
    let existing = metadata::part(package, workbook);
    let (part, mut text) = if dynamic {
        match array_metadata(package, workbook)? {
            ArrayMetadata::Present { cm } => {
                out.cm = Some(cm);
                let part = existing.expect("a present record lies in a metadata part");
                let text = read_text(package, &part)?;
                (Some(part), text)
            }
            ArrayMetadata::Rewrite { cm, part, content } => {
                out.cm = Some(cm);
                let text = String::from_utf8(content).expect("written as UTF-8");
                (Some(part), text)
            }
            ArrayMetadata::Create { content } => {
                out.cm = Some(1);
                (None, String::from_utf8(content).expect("written as UTF-8"))
            }
        }
    } else {
        match existing {
            Some(part) => {
                let text = read_text(package, &part)?;
                (Some(part), text)
            }
            None => (
                None,
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"></metadata>"#
                    .to_owned(),
            ),
        }
    };
    let part_name = part.clone().unwrap_or_else(|| "xl/metadata.xml".to_owned());
    if !errors.is_empty() || !images.is_empty() {
        add_rich_values(package, workbook, &part_name, &mut text, &errors, images, &mut out)?;
    }
    match part {
        Some(part) => {
            let unchanged = package
                .read_part(&part)
                .is_ok_and(|bytes| bytes == text.as_bytes());
            if !unchanged {
                out.rewritten.push((part, text.into_bytes()));
            }
        }
        None => {
            let (name, target) = new_part_name(package, workbook, "metadata");
            out.added.insert(
                0,
                NewPart {
                    name,
                    target,
                    content: text.into_bytes(),
                    content_type: CONTENT_TYPE,
                    relationship: RELATIONSHIP,
                },
            );
        }
    }
    Ok(out)
}

/// Appends to the metadata part's `text` (named `part`) and to the rich value parts one value
/// metadata record for each of `errors` and `images`, recording each one's `vm` in `out`.
fn add_rich_values(
    package: &Package,
    workbook: &str,
    part: &str,
    text: &mut String,
    errors: &BTreeSet<u32>,
    images: &[ImageValue],
    out: &mut MetadataPlan,
) -> Result<(), SaveRefusal> {
    // The rich value parts, read or new.
    let rich_values = metadata::related(package, workbook, "/rdRichValue");
    let structures = metadata::related(package, workbook, "/rdRichValueStructure");
    let (mut values_text, mut structures_text) = match (&rich_values, &structures) {
        (Some(values), Some(structures)) => {
            (read_text(package, values)?, read_text(package, structures)?)
        }
        (None, None) => (
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<rvData xmlns=\"{RICH_DATA_NAMESPACE}\" count=\"0\"></rvData>"
            ),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<rvStructures xmlns=\"{RICH_DATA_NAMESPACE}\" count=\"0\"></rvStructures>"
            ),
        ),
        _ => {
            return Err(invalid(
                workbook,
                "a workbook with one of the rich value parts and not the other",
            ));
        }
    };
    let Some(rv_root) = start_tag(&values_text, "rvData") else {
        return Err(invalid(
            rich_values.as_deref().unwrap_or(workbook),
            "a rich value part whose elements carry a namespace prefix, which this writer does not extend",
        ));
    };
    let Some(s_root) = start_tag(&structures_text, "rvStructures") else {
        return Err(invalid(
            structures.as_deref().unwrap_or(workbook),
            "a rich value structure part whose elements carry a namespace prefix, which this writer does not extend",
        ));
    };
    // A structure, found or appended (0-based `s`): an `_error` holding errorType alone, and a
    // `_webimage` of each shape the pictures take.
    let structure_index = |structures_text: &mut String, wanted: &str| {
        match position_of(structures_text, "rvStructures", "s", |_| true)
            .and_then(|_| structures_text.find(wanted))
        {
            Some(at) => {
                let mut index = 0;
                let mut from = s_root + "<rvStructures".len();
                while let Some(found) = start_tag(&structures_text[from..at], "s") {
                    index += 1;
                    from += found + 2;
                }
                index
            }
            None => {
                let index = children(structures_text, s_root, "rvStructures", "s");
                append(structures_text, s_root, "rvStructures", wanted);
                set_count(structures_text, s_root, index + 1);
                index
            }
        }
    };
    // The metadata type XLRICHVALUE, found or appended (1-based `t`).
    let Some(root) = start_tag(text, "metadata") else {
        return Err(invalid(
            part,
            "a metadata part whose elements carry a namespace prefix, which this writer does not extend",
        ));
    };
    let types = match start_tag(text, "metadataTypes") {
        Some(at) => at,
        None => {
            let insert = root + text[root..].find('>').expect("a start tag ends") + 1;
            text.insert_str(insert, r#"<metadataTypes count="0"></metadataTypes>"#);
            insert
        }
    };
    let type_index = match position_of(text, "metadataTypes", "metadataType", |tag| {
        tag.contains(r#"name="XLRICHVALUE""#)
    }) {
        Some(index) => index,
        None => {
            let index = children(text, types, "metadataTypes", "metadataType") + 1;
            append(text, types, "metadataTypes", RICH_VALUE_TYPE);
            set_count(text, types, index);
            index
        }
    };
    // Every record: one rich value, in the order errors then pictures.
    let mut records: Vec<(String, Option<usize>, Option<u32>)> = Vec::new();
    if !errors.is_empty() {
        let structure = structure_index(&mut structures_text, ERROR_STRUCTURE);
        for &error_type in errors {
            records.push((
                format!(r#"<rv s="{structure}"><v>{error_type}</v></rv>"#),
                None,
                Some(error_type),
            ));
        }
    }
    let first_image = web_image_count(package, workbook)?;
    for (index, image) in images.iter().enumerate() {
        let (shape, values) = web_image_value(image, first_image + index);
        let structure = structure_index(&mut structures_text, &shape);
        records.push((
            format!(r#"<rv s="{structure}">{values}</rv>"#),
            Some(index),
            None,
        ));
    }
    if !images.is_empty() {
        add_web_images(package, workbook, images, out)?;
    }
    for (rich_value, image, error_type) in records {
        let rv = children(&values_text, rv_root, "rvData", "rv");
        append(&mut values_text, rv_root, "rvData", &rich_value);
        set_count(&mut values_text, rv_root, rv + 1);
        let record = format!(
            r#"<bk><extLst><ext uri="{{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}}"><xlrd:rvb xmlns:xlrd="{RICH_DATA_NAMESPACE}" i="{rv}"/></ext></extLst></bk>"#
        );
        let future = match text.find(r#"<futureMetadata name="XLRICHVALUE""#) {
            Some(at) => {
                let index = children(text, at, "futureMetadata", "bk");
                append(text, at, "futureMetadata", &record);
                set_count(text, at, index + 1);
                index
            }
            None => {
                let block = format!(
                    r#"<futureMetadata name="XLRICHVALUE" count="1">{record}</futureMetadata>"#
                );
                let at = after_last(text, root, &BEFORE_CELL_METADATA);
                text.insert_str(at, &block);
                0
            }
        };
        let value_record = format!(r#"<bk><rc t="{type_index}" v="{future}"/></bk>"#);
        let vm = match start_tag(text, "valueMetadata") {
            Some(at) => {
                let index = children(text, at, "valueMetadata", "bk") + 1;
                append(text, at, "valueMetadata", &value_record);
                set_count(text, at, index);
                index
            }
            None => {
                // valueMetadata goes after the cell metadata and before the part's own extLst.
                let at = after_last(text, root, &BEFORE_VALUE_METADATA);
                text.insert_str(
                    at,
                    &format!(r#"<valueMetadata count="1">{value_record}</valueMetadata>"#),
                );
                1
            }
        };
        let vm = u32::try_from(vm).unwrap_or(u32::MAX);
        if let Some(error_type) = error_type {
            out.vm.insert(error_type, vm);
        }
        if let Some(index) = image {
            out.image_vm.push((images[index].clone(), vm));
        }
    }
    match (rich_values, structures) {
        (Some(values), Some(structures)) => {
            out.rewritten.push((values, values_text.into_bytes()));
            out.rewritten
                .push((structures, structures_text.into_bytes()));
        }
        _ => {
            for (stem, content, content_type, relationship) in [
                (
                    "richData/rdrichvalue",
                    values_text.into_bytes(),
                    RICH_VALUE_CONTENT_TYPE,
                    RICH_VALUE_RELATIONSHIP,
                ),
                (
                    "richData/rdrichvaluestructure",
                    structures_text.into_bytes(),
                    STRUCTURE_CONTENT_TYPE,
                    STRUCTURE_RELATIONSHIP,
                ),
            ] {
                let (name, target) = new_part_name(package, workbook, stem);
                out.added.push(NewPart {
                    name,
                    target,
                    content,
                    content_type,
                    relationship,
                });
            }
            if metadata::related(package, workbook, "/rdRichValueTypes").is_none() {
                let (name, target) = new_part_name(package, workbook, "richData/rdRichValueTypes");
                out.added.push(NewPart {
                    name,
                    target,
                    content: TYPES_PART.as_bytes().to_vec(),
                    content_type: TYPES_CONTENT_TYPE,
                    relationship: TYPES_RELATIONSHIP,
                });
            }
        }
    }
    Ok(())
}

/// How many web images the package already holds.
fn web_image_count(package: &Package, workbook: &str) -> Result<usize, SaveRefusal> {
    match metadata::related(package, workbook, "/rdRichValueWebImage") {
        Some(part) => Ok(read_text(package, &part)?.matches("<webImageSrd>").count()),
        None => Ok(0),
    }
}

/// A picture's `_webimage` structure and `<v>` values, as Excel 16.0.20527 writes them: the keys
/// WebImageIdentifier, CalcOrigin (1), ComputedImage (0), then ImageHeight, ImageSizing,
/// ImageWidth and Text as the call gave them.
fn web_image_value(image: &ImageValue, identifier: usize) -> (String, String) {
    let mut keys = String::from(
        r#"<k n="WebImageIdentifier" t="i"/><k n="CalcOrigin" t="i"/><k n="ComputedImage" t="b"/>"#,
    );
    let mut values = format!("<v>{identifier}</v><v>1</v><v>0</v>");
    if let Some(height) = image.height {
        keys.push_str(r#"<k n="ImageHeight"/>"#);
        values.push_str(&format!("<v>{height}</v>"));
    }
    keys.push_str(r#"<k n="ImageSizing" t="i"/>"#);
    values.push_str(&format!("<v>{}</v>", image.sizing));
    if let Some(width) = image.width {
        keys.push_str(r#"<k n="ImageWidth"/>"#);
        values.push_str(&format!("<v>{width}</v>"));
    }
    if let Some(text) = &image.alt_text {
        keys.push_str(r#"<k n="Text" t="s"/>"#);
        values.push_str(&format!("<v>{}</v>", xml_escaped(text)));
    }
    (format!(r#"<s t="_webimage">{keys}</s>"#), values)
}

fn xml_escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Appends `images` to the web image part and its relationships, with one media part each.
fn add_web_images(
    package: &Package,
    workbook: &str,
    images: &[ImageValue],
    out: &mut MetadataPlan,
) -> Result<(), SaveRefusal> {
    let existing = metadata::related(package, workbook, "/rdRichValueWebImage");
    let mut part_text = match &existing {
        Some(part) => read_text(package, part)?,
        None => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<webImagesSrd xmlns=\"{WEB_IMAGE_NAMESPACE}\" xmlns:r=\"{OFFICE_RELATIONSHIPS}\"></webImagesSrd>"
        ),
    };
    let (name, target) = match &existing {
        Some(part) => (part.clone(), String::new()),
        None => new_part_name(package, workbook, "richData/rdRichValueWebImage"),
    };
    let (folder, file) = name.rsplit_once('/').unwrap_or(("", &name));
    let rels_name = format!("{folder}/_rels/{file}.rels");
    let held = package.relationships_of(Some(&name));
    let mut rels_text = match &held {
        Some(_) => read_text(package, &rels_name)?,
        None => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"{RELATIONSHIPS_NAMESPACE}\"></Relationships>"
        ),
    };
    let used = held.map_or(0, |held| {
        held.items()
            .iter()
            .filter_map(|item| item.id.strip_prefix("rId")?.parse::<usize>().ok())
            .max()
            .unwrap_or(0)
    });
    let mut entries = String::new();
    let mut relationships = String::new();
    let parent = workbook.rsplit_once('/').map_or("", |(parent, _)| parent);
    let media_stem = if parent.is_empty() { "media/image" } else { "/media/image" };
    let mut media_number = package
        .parts()
        .iter()
        .filter(|part| part.name().contains("media/image"))
        .count();
    for (index, image) in images.iter().enumerate() {
        let address = used + index * 2 + 1;
        let blip = address + 1;
        entries.push_str(&format!(
            r#"<webImageSrd><address r:id="rId{address}"/><blip r:id="rId{blip}"/></webImageSrd>"#
        ));
        media_number += 1;
        let extension = image.format.extension();
        let media = format!("{parent}{media_stem}{media_number}.{extension}");
        relationships.push_str(&format!(
            r#"<Relationship Id="rId{address}" Type="{OFFICE_RELATIONSHIPS}/hyperlink" Target="{}" TargetMode="External"/><Relationship Id="rId{blip}" Type="{OFFICE_RELATIONSHIPS}/image" Target="../media/image{media_number}.{extension}"/>"#,
            xml_escaped(&image.source)
        ));
        out.unrelated
            .push((media, image.bytes.to_vec(), image.format.content_type()));
    }
    let close = "</webImagesSrd>";
    let at = part_text.rfind(close).ok_or_else(|| invalid(&name, "a web image part without its end tag"))?;
    part_text.insert_str(at, &entries);
    let close = "</Relationships>";
    let at = rels_text.rfind(close).ok_or_else(|| invalid(&rels_name, "a relationships part without its end tag"))?;
    rels_text.insert_str(at, &relationships);
    match existing {
        Some(part) => out.rewritten.push((part, part_text.into_bytes())),
        None => out.added.push(NewPart {
            name,
            target,
            content: part_text.into_bytes(),
            content_type: WEB_IMAGE_CONTENT_TYPE,
            relationship: WEB_IMAGE_RELATIONSHIP,
        }),
    }
    if held.is_some() {
        out.rewritten.push((rels_name, rels_text.into_bytes()));
    } else {
        out.unrelated
            .push((rels_name, rels_text.into_bytes(), RELATIONSHIPS_CONTENT_TYPE));
    }
    Ok(())
}

fn read_text(package: &Package, part: &str) -> Result<String, SaveRefusal> {
    let bytes = package.read_part(part).map_err(|error| SaveRefusal::Part {
        part: part.to_owned(),
        error,
    })?;
    String::from_utf8(bytes).map_err(|_| invalid(part, "a part that is not UTF-8"))
}

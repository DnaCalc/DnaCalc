//! The workbook's cell and value metadata (`xl/metadata.xml`, the part of the workbook part's
//! `sheetMetadata` relationship) and the rich values it leads to (`xl/richData/`), read so that a
//! cell's `cm` and `vm` attributes can be resolved.
//!
//! - `cm="n"` names the n-th `cellMetadata` record. Its `<rc t v>` names a metadata type (1-based
//!   `t` into `metadataTypes`) and a record of that type's `futureMetadata` block (0-based `v`).
//!   Excel writes it on the anchor of a dynamic-array formula: the type is `XLDAPR`, whose record
//!   carries `xda:dynamicArrayProperties` with `fDynamic` and `fCollapsed`.
//! - `vm="n"` names the n-th `valueMetadata` record, bound to the cell's value. Of type
//!   `XLRICHVALUE`, its future record's `xlrd:rvb i` names a rich value in `rdrichvalue.xml`,
//!   whose `s` names a structure in `rdrichvaluestructure.xml`. A structure of type `_error`
//!   carries the error the cell really holds in its `errorType` key ([MS-XLSX] 2.3.6.1.3); the
//!   cell's `<v>` holds only the `#VALUE!` fallback Excel writes for readers that do not follow
//!   `vm`. Every other structure is a rich value the model does not represent.
//!
//! A part that is absent leaves each lookup to say what it lacks; a part that is present but not
//! acceptable XML is refused like any other part read.

use super::{Events, LoadError, read_xml};
use crate::opc::{Package, TargetMode};
use crate::xml::XmlEvent;
use calc_doc_model::value::{ImageFormat, ImageValue, WorksheetErrorCode};
use std::collections::BTreeMap;
use std::sync::Arc;

/// What a `cm` index resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CellMeta {
    /// Dynamic-array properties: the cell anchors a dynamic-array formula when `dynamic`.
    DynamicArray { dynamic: bool, collapsed: bool },
    /// A record of another metadata type, which this reader does not interpret.
    Other { type_name: String },
    /// The chain could not be followed; the reason names the broken link.
    Unresolved { reason: String },
}

/// What a `vm` index resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ValueMeta {
    /// An `_error` rich value whose `errorType` names this worksheet error.
    Error(WorksheetErrorCode),
    /// An `_error` rich value whose `errorType` names no worksheet error the model holds.
    UnknownErrorType { error_type: u32 },
    /// An `mdxMetadata` record: a CUBE function's value, whose `<v>` is the value shown.
    Mdx,
    /// A `_webimage` rich value: IMAGE's picture, with the media part's bytes.
    Image(ImageValue),
    /// A rich value of another structure (a linked data type, a picture placed in a cell).
    OtherRichValue { structure: String },
    /// The chain could not be followed; the reason names the broken link.
    Unresolved { reason: String },
}

/// The resolved `cm` and `vm` indexes of a workbook.
#[derive(Debug, Default)]
pub(crate) struct Metadata {
    cell: Vec<CellMeta>,
    value: Vec<ValueMeta>,
    /// Why no index could be built (no metadata part), so each lookup can say what it lacks.
    unavailable: Option<String>,
}

impl Metadata {
    pub(crate) fn cell(&self, index: u32) -> CellMeta {
        index
            .checked_sub(1)
            .and_then(|at| self.cell.get(at as usize))
            .cloned()
            .unwrap_or_else(|| CellMeta::Unresolved {
                reason: self.unavailable.clone().unwrap_or_else(|| {
                    format!(
                        "cellMetadata has no record {index} ({} records)",
                        self.cell.len()
                    )
                }),
            })
    }

    /// The number of `cellMetadata` records.
    pub(crate) fn cell_count(&self) -> u32 {
        u32::try_from(self.cell.len()).unwrap_or(u32::MAX)
    }

    pub(crate) fn value(&self, index: u32) -> ValueMeta {
        index
            .checked_sub(1)
            .and_then(|at| self.value.get(at as usize))
            .cloned()
            .unwrap_or_else(|| ValueMeta::Unresolved {
                reason: self.unavailable.clone().unwrap_or_else(|| {
                    format!(
                        "valueMetadata has no record {index} ({} records)",
                        self.value.len()
                    )
                }),
            })
    }
}

/// One record of a `futureMetadata` block.
#[derive(Clone, Debug, Default)]
struct FutureRecord {
    /// `<xlrd:rvb i>`, when present.
    rich_value: Option<u32>,
    /// `<xda:dynamicArrayProperties fDynamic fCollapsed>`, when present.
    dynamic_array: Option<(bool, bool)>,
}

/// The parts of `metadata.xml` the lookups need.
#[derive(Debug, Default)]
struct ParsedMetadata {
    /// `metadataTypes` names in document order (1-based `rc t`).
    type_names: Vec<String>,
    /// Each `futureMetadata` block's records, by its `name`.
    future: BTreeMap<String, Vec<FutureRecord>>,
    /// `cellMetadata` records' `<rc t v>`, in document order (1-based `cm`).
    cell: Vec<Option<(u32, u32)>>,
    /// `valueMetadata` records' `<rc t v>`, in document order (1-based `vm`).
    value: Vec<Option<(u32, u32)>>,
}

/// A rich value: its 0-based structure index and its `<v>` texts.
#[derive(Debug, Default)]
struct RichValue {
    structure: u32,
    values: Vec<String>,
}

/// A rich value structure: its type and its key names in order.
#[derive(Debug, Default)]
struct Structure {
    kind: String,
    keys: Vec<String>,
}

fn is_true(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true"))
}

fn number(value: Option<&str>) -> Option<u32> {
    value.and_then(|text| text.trim().parse().ok())
}

/// The part a relationship of the workbook part whose type ends with `suffix` targets.
pub(crate) fn related(package: &Package, workbook: &str, suffix: &str) -> Option<String> {
    package
        .relationships_of(Some(workbook))?
        .items()
        .iter()
        .find(|item| item.kind.ends_with(suffix) && item.mode == TargetMode::Internal)
        .and_then(|item| item.resolved.clone())
        .filter(|part| package.part(part).is_some())
}

fn parse_metadata(package: &Package, part: &str) -> Result<ParsedMetadata, LoadError> {
    #[derive(Clone, Copy, PartialEq)]
    enum Section {
        None,
        Future,
        Cell,
        Value,
    }
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let mut parsed = ParsedMetadata::default();
    let mut section = Section::None;
    let mut future_name = String::new();
    let mut record: Option<FutureRecord> = None;
    let mut rc: Option<Option<(u32, u32)>> = None;
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(element) => match element.name.local.as_str() {
                "metadataType" => parsed
                    .type_names
                    .push(element.attribute("name").unwrap_or_default().to_owned()),
                "futureMetadata" => {
                    section = Section::Future;
                    future_name = element.attribute("name").unwrap_or_default().to_owned();
                    parsed.future.entry(future_name.clone()).or_default();
                }
                "cellMetadata" => section = Section::Cell,
                "valueMetadata" => section = Section::Value,
                "bk" => match section {
                    Section::Future => record = Some(FutureRecord::default()),
                    Section::Cell | Section::Value => rc = Some(None),
                    Section::None => {}
                },
                "rvb" => {
                    if let Some(record) = record.as_mut() {
                        record.rich_value = number(element.attribute("i"));
                    }
                }
                "dynamicArrayProperties" => {
                    if let Some(record) = record.as_mut() {
                        record.dynamic_array = Some((
                            is_true(element.attribute("fDynamic")),
                            is_true(element.attribute("fCollapsed")),
                        ));
                    }
                }
                "rc" => {
                    if let Some(rc) = rc.as_mut() {
                        *rc = number(element.attribute("t")).zip(number(element.attribute("v")));
                    }
                }
                _ => {}
            },
            XmlEvent::End(name) => match name.local.as_str() {
                "futureMetadata" | "cellMetadata" | "valueMetadata" => section = Section::None,
                "bk" => match section {
                    Section::Future => {
                        if let Some(record) = record.take() {
                            parsed
                                .future
                                .entry(future_name.clone())
                                .or_default()
                                .push(record);
                        }
                    }
                    Section::Cell => parsed.cell.push(rc.take().flatten()),
                    Section::Value => parsed.value.push(rc.take().flatten()),
                    Section::None => {}
                },
                _ => {}
            },
            XmlEvent::Text(_) => {}
        }
    }
    Ok(parsed)
}

fn parse_rich_values(package: &Package, part: &str) -> Result<Vec<RichValue>, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let mut values = Vec::new();
    let mut current: Option<RichValue> = None;
    let mut in_value = false;
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(element) if element.name.local == "rv" => {
                current = Some(RichValue {
                    structure: number(element.attribute("s")).unwrap_or(u32::MAX),
                    values: Vec::new(),
                });
            }
            XmlEvent::Start(element) if element.name.local == "v" => {
                if let Some(current) = current.as_mut() {
                    current.values.push(String::new());
                    in_value = true;
                }
            }
            XmlEvent::Start(_) => {}
            XmlEvent::End(name) if name.local == "rv" => values.extend(current.take()),
            XmlEvent::End(name) if name.local == "v" => in_value = false,
            XmlEvent::End(_) => {}
            XmlEvent::Text(text) => {
                if in_value && let Some(last) = current.as_mut().and_then(|v| v.values.last_mut()) {
                    last.push_str(&text);
                }
            }
        }
    }
    Ok(values)
}

/// One `webImageSrd` of `rdRichValueWebImage.xml`, resolved through the part's relationships.
#[derive(Debug)]
struct WebImage {
    address: String,
    format: ImageFormat,
    bytes: Arc<[u8]>,
}

/// The web images in document order; `None` for one whose address or picture does not resolve.
fn parse_web_images(package: &Package, part: &str) -> Result<Vec<Option<WebImage>>, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let mut ids: Vec<(Option<String>, Option<String>)> = Vec::new();
    while let Some(event) = events.next()? {
        if let XmlEvent::Start(element) = event {
            let id = super::relationship_attribute(&element, "id").map(str::to_owned);
            match element.name.local.as_str() {
                "webImageSrd" => ids.push((None, None)),
                "address" => {
                    if let Some(last) = ids.last_mut() {
                        last.0 = id;
                    }
                }
                "blip" => {
                    if let Some(last) = ids.last_mut() {
                        last.1 = id;
                    }
                }
                _ => {}
            }
        }
    }
    let relationships = package.relationships_of(Some(part));
    let find = |id: &str| relationships?.items().iter().find(|item| item.id == id);
    Ok(ids
        .into_iter()
        .map(|(address, blip)| {
            let address = find(address.as_deref()?)?.target.clone();
            let media = find(blip.as_deref()?)?.resolved.clone()?;
            let bytes = package.read_part(&media).ok()?;
            Some(WebImage {
                address,
                format: ImageFormat::sniff(&bytes)?,
                bytes: bytes.into(),
            })
        })
        .collect())
}

/// A `_webimage` rich value as the picture it names.
fn web_image(
    value: &RichValue,
    structure: &Structure,
    images: Option<&[Option<WebImage>]>,
) -> ValueMeta {
    let unresolved = |reason: &str| ValueMeta::Unresolved {
        reason: reason.to_owned(),
    };
    let key = |name: &str| {
        let at = structure.keys.iter().position(|key| key == name)?;
        value.values.get(at).map(String::as_str)
    };
    let Some(index) = key("WebImageIdentifier").and_then(|text| text.trim().parse::<usize>().ok())
    else {
        return unresolved("_webimage rich value has no WebImageIdentifier");
    };
    let Some(Some(image)) = images.and_then(|images| images.get(index)) else {
        return unresolved("_webimage rich value names no resolvable web image");
    };
    let size = |name: &str| key(name).and_then(|text| text.trim().parse::<f64>().ok());
    ValueMeta::Image(ImageValue {
        source: image.address.clone(),
        alt_text: key("Text").map(str::to_owned),
        sizing: key("ImageSizing")
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0),
        height: size("ImageHeight"),
        width: size("ImageWidth"),
        format: image.format,
        bytes: image.bytes.clone(),
    })
}

fn parse_structures(package: &Package, part: &str) -> Result<Vec<Structure>, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let mut structures = Vec::new();
    let mut current: Option<Structure> = None;
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(element) if element.name.local == "s" => {
                current = Some(Structure {
                    kind: element.attribute("t").unwrap_or_default().to_owned(),
                    keys: Vec::new(),
                });
            }
            XmlEvent::Start(element) if element.name.local == "k" => {
                if let Some(current) = current.as_mut() {
                    current
                        .keys
                        .push(element.attribute("n").unwrap_or_default().to_owned());
                }
            }
            XmlEvent::End(name) if name.local == "s" => structures.extend(current.take()),
            _ => {}
        }
    }
    Ok(structures)
}

/// The worksheet error of a rich value `_error` structure's `errorType`, which is `ERROR.TYPE`
/// minus 1: 0 `#NULL!` to 6 `#N/A`, 7 `#GETTING_DATA`, 8 `#SPILL!`, 9 `#CONNECT!`, 10 `#BLOCKED!`,
/// 11 `#UNKNOWN!` (which the model does not hold), 12 `#FIELD!` and 13 `#CALC!`.
pub(crate) fn error_of_type(error_type: u32) -> Option<WorksheetErrorCode> {
    use WorksheetErrorCode::*;
    Some(match error_type {
        0 => Null,
        1 => Div0,
        2 => Value,
        3 => Ref,
        4 => Name,
        5 => Num,
        6 => NA,
        7 => GettingData,
        8 => Spill,
        9 => Connect,
        10 => Blocked,
        12 => Field,
        13 => Calc,
        _ => return None,
    })
}

fn resolve_value(
    rc: Option<(u32, u32)>,
    parsed: &ParsedMetadata,
    rich_values: Option<&[RichValue]>,
    structures: Option<&[Structure]>,
    web_images: Option<&[Option<WebImage>]>,
) -> ValueMeta {
    let unresolved = |reason: String| ValueMeta::Unresolved { reason };
    let Some((type_index, future_index)) = rc else {
        return unresolved("valueMetadata record has no <rc t v>".to_owned());
    };
    let Some(type_name) = type_index
        .checked_sub(1)
        .and_then(|at| parsed.type_names.get(at as usize))
    else {
        return unresolved(format!(
            "valueMetadata rc t={type_index} names no metadataType"
        ));
    };
    if type_name == "XLMDX" {
        return ValueMeta::Mdx;
    }
    if type_name != "XLRICHVALUE" {
        return unresolved(format!(
            "valueMetadata record is of metadata type {type_name}, not XLRICHVALUE"
        ));
    }
    let Some(record) = parsed
        .future
        .get("XLRICHVALUE")
        .and_then(|records| records.get(future_index as usize))
    else {
        return unresolved(format!(
            "valueMetadata rc v={future_index} names no futureMetadata XLRICHVALUE record"
        ));
    };
    let Some(rich_value) = record.rich_value else {
        return unresolved("futureMetadata XLRICHVALUE record has no <xlrd:rvb i>".to_owned());
    };
    let Some(rich_values) = rich_values else {
        return unresolved("the workbook has no rdRichValue part".to_owned());
    };
    let Some(value) = rich_values.get(rich_value as usize) else {
        return unresolved(format!(
            "rvb i={rich_value} is outside rdrichvalue.xml ({} values)",
            rich_values.len()
        ));
    };
    let Some(structures) = structures else {
        return unresolved("the workbook has no rdRichValueStructure part".to_owned());
    };
    let Some(structure) = structures.get(value.structure as usize) else {
        return unresolved(format!(
            "rv s={} is outside rdrichvaluestructure.xml ({} structures)",
            value.structure,
            structures.len()
        ));
    };
    if structure.kind == "_webimage" {
        return web_image(value, structure, web_images);
    }
    if structure.kind != "_error" {
        return ValueMeta::OtherRichValue {
            structure: structure.kind.clone(),
        };
    }
    let Some(position) = structure.keys.iter().position(|key| key == "errorType") else {
        return unresolved("_error structure has no errorType key".to_owned());
    };
    let Some(error_type) = value
        .values
        .get(position)
        .and_then(|text| text.trim().parse::<u32>().ok())
    else {
        return unresolved(format!(
            "_error rich value has no integer errorType at key position {position}"
        ));
    };
    match error_of_type(error_type) {
        Some(code) => ValueMeta::Error(code),
        None => ValueMeta::UnknownErrorType { error_type },
    }
}

fn resolve_cell(rc: Option<(u32, u32)>, parsed: &ParsedMetadata) -> CellMeta {
    let unresolved = |reason: String| CellMeta::Unresolved { reason };
    let Some((type_index, future_index)) = rc else {
        return unresolved("cellMetadata record has no <rc t v>".to_owned());
    };
    let Some(type_name) = type_index
        .checked_sub(1)
        .and_then(|at| parsed.type_names.get(at as usize))
    else {
        return unresolved(format!(
            "cellMetadata rc t={type_index} names no metadataType"
        ));
    };
    if type_name != "XLDAPR" {
        return CellMeta::Other {
            type_name: type_name.clone(),
        };
    }
    match parsed
        .future
        .get("XLDAPR")
        .and_then(|records| records.get(future_index as usize))
        .and_then(|record| record.dynamic_array)
    {
        Some((dynamic, collapsed)) => CellMeta::DynamicArray { dynamic, collapsed },
        None => unresolved(format!(
            "cellMetadata rc v={future_index} names no XLDAPR record with dynamicArrayProperties"
        )),
    }
}

/// The workbook's metadata part, when its relationships name one that the package holds.
pub(crate) fn part(package: &Package, workbook: &str) -> Option<String> {
    related(package, workbook, "/sheetMetadata")
}

/// Reads the workbook's metadata and rich value parts, when it has them.
pub(crate) fn read(package: &Package, workbook: &str) -> Result<Metadata, LoadError> {
    let Some(part) = related(package, workbook, "/sheetMetadata") else {
        return Ok(Metadata {
            unavailable: Some("the workbook has no sheetMetadata part".to_owned()),
            ..Metadata::default()
        });
    };
    let parsed = parse_metadata(package, &part)?;
    let rich_values = match related(package, workbook, "/rdRichValue") {
        Some(part) => Some(parse_rich_values(package, &part)?),
        None => None,
    };
    let structures = match related(package, workbook, "/rdRichValueStructure") {
        Some(part) => Some(parse_structures(package, &part)?),
        None => None,
    };
    let web_images = match related(package, workbook, "/rdRichValueWebImage") {
        Some(part) => Some(parse_web_images(package, &part)?),
        None => None,
    };
    Ok(Metadata {
        cell: parsed
            .cell
            .iter()
            .map(|rc| resolve_cell(*rc, &parsed))
            .collect(),
        value: parsed
            .value
            .iter()
            .map(|rc| {
                resolve_value(
                    *rc,
                    &parsed,
                    rich_values.as_deref(),
                    structures.as_deref(),
                    web_images.as_deref(),
                )
            })
            .collect(),
        unavailable: None,
    })
}

//! Excel binary workbooks (.xlsb, MS-XLSB; dc-5sr): a package whose workbook, worksheets, macro
//! sheets (dc-xuw), styles, shared strings, tables, external links, comments and metadata are
//! parts of binary records
//! instead of SpreadsheetML. [`crate::open`] reads such a package by converting it, before
//! anything else reads it, into the SpreadsheetML package it stands for: each binary part becomes
//! the XML part of the same content (`xl/workbook.bin` becomes `xl/workbook.xml`), with its
//! relationships and content types; every formula, stored as parsed tokens, is printed back as the
//! text Excel writes for it ([`formula`]). Parts that are the same in both forms (themes,
//! drawings, VML, images, printer settings, a VBA project, custom XML, document properties) are
//! carried over as they are; the binary index of each sheet, the calculation chain and the
//! volatile dependencies, which only speed up Excel's reading, are left out. The loader then reads
//! the converted package as it reads any, so the model of an .xlsb is the model of the .xlsx Excel
//! saves for it, and a save writes an .xlsx (an .xlsm for a workbook with a VBA project or a macro
//! sheet).
//!
//! A part of a kind this conversion does not read (a pivot table or cache, a query table, a
//! connection list, a slicer) refuses the workbook with a typed reason
//! ([`crate::OpenError::BinaryWorkbook`]), as does a record of a read kind or a formula token it
//! cannot read. The records a converted part leaves out are listed with it ([`sheet`],
//! [`styles`], [`parts`]); of them the loader reads only a data bar's Excel 2010 properties
//! (its lengths and fill), so a data bar converts with SpreadsheetML 2006's.

mod commands;
mod formula;
mod parts;
mod records;
mod sheet;
mod styles;
mod workbook;

use crate::OpenError;
use crate::opc::{Package, TargetMode};
use crate::save::text::{attribute_value, xstring};
use std::collections::BTreeMap;

pub(crate) use records::Malformed;

/// The content type of an Excel binary workbook's workbook part.
const BINARY_MAIN: &str = "application/vnd.ms-excel.sheet.binary.macroEnabled.main";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub(crate) const MAIN_NAMESPACE: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub(crate) const RELATIONSHIPS_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The workbook part of an Excel binary workbook, when the package is one.
pub(crate) fn binary_workbook(package: &Package) -> Option<String> {
    let relationships = package.relationships_of(None)?;
    relationships
        .items()
        .iter()
        .filter(|item| item.kind == OFFICE_DOCUMENT && item.mode == TargetMode::Internal)
        .filter_map(|item| item.resolved.clone())
        .find(|part| {
            package
                .part(part)
                .and_then(|p| p.content_type())
                .is_some_and(|ct| ct == BINARY_MAIN)
        })
}

/// Why a binary workbook was not converted: the part and what in it.
pub(crate) fn refusal(part: &str, detail: impl Into<String>) -> OpenError {
    OpenError::BinaryWorkbook {
        part: part.to_owned(),
        detail: detail.into(),
    }
}

/// What a binary part becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Workbook,
    Worksheet,
    /// An Excel 4.0 macro sheet (dc-xuw), or an international one.
    MacroSheet {
        international: bool,
    },
    Styles,
    SharedStrings,
    Table,
    ExternalLink,
    Comments,
    Metadata,
    /// Left out: it only speeds up Excel's reading.
    Dropped,
    /// Carried over as it is.
    Kept,
}

fn kind_of(content_type: &str) -> Result<Kind, &str> {
    Ok(match content_type {
        BINARY_MAIN => Kind::Workbook,
        "application/vnd.ms-excel.worksheet" => Kind::Worksheet,
        "application/vnd.ms-excel.macrosheet" => Kind::MacroSheet {
            international: false,
        },
        "application/vnd.ms-excel.intlmacrosheet" => Kind::MacroSheet {
            international: true,
        },
        "application/vnd.ms-excel.styles" => Kind::Styles,
        "application/vnd.ms-excel.sharedStrings" => Kind::SharedStrings,
        "application/vnd.ms-excel.table" => Kind::Table,
        "application/vnd.ms-excel.externalLink" => Kind::ExternalLink,
        "application/vnd.ms-excel.comments" => Kind::Comments,
        "application/vnd.ms-excel.sheetMetadata" => Kind::Metadata,
        "application/vnd.ms-excel.binIndexWs"
        | "application/vnd.ms-excel.binIndexMs"
        | "application/vnd.ms-excel.calcChain"
        | "application/vnd.ms-excel.volatileDependencies" => Kind::Dropped,
        other if other.starts_with("application/vnd.ms-excel.") && !other.ends_with("+xml") => {
            return Err(other);
        }
        _ => Kind::Kept,
    })
}

/// The SpreadsheetML content type of a converted part.
fn xml_content_type(kind: Kind, macros: bool) -> &'static str {
    match kind {
        Kind::Workbook if macros => "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
        Kind::Workbook => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
        }
        Kind::Worksheet => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"
        }
        Kind::MacroSheet {
            international: false,
        } => "application/vnd.ms-excel.macrosheet+xml",
        Kind::MacroSheet {
            international: true,
        } => "application/vnd.ms-excel.intlmacrosheet+xml",
        Kind::Styles => "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        Kind::SharedStrings => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"
        }
        Kind::Table => "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml",
        Kind::ExternalLink => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml"
        }
        Kind::Comments => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"
        }
        Kind::Metadata => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"
        }
        Kind::Dropped | Kind::Kept => unreachable!("not converted"),
    }
}

/// A part's name with `.bin` turned into `.xml`.
fn xml_name(name: &str) -> String {
    match name.strip_suffix(".bin") {
        Some(stem) => format!("{stem}.xml"),
        None => format!("{name}.xml"),
    }
}

/// A small XML writer.
pub(crate) struct Xml {
    pub(crate) out: Vec<u8>,
}

impl Xml {
    pub(crate) fn new() -> Self {
        let mut out = Vec::with_capacity(4096);
        out.extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
        Xml { out }
    }

    fn tag(&mut self, name: &str, attributes: &[(&str, String)], empty: bool) {
        self.out.push(b'<');
        self.out.extend_from_slice(name.as_bytes());
        for (key, value) in attributes {
            self.out.push(b' ');
            self.out.extend_from_slice(key.as_bytes());
            self.out.push(b'=');
            // A control character, which no XML attribute holds, as Excel writes it: `_x0013_`.
            if value.chars().any(|c| c < ' ') {
                let mut escaped = String::with_capacity(value.len());
                for c in value.chars() {
                    if c < ' ' {
                        escaped.push_str(&format!("_x{:04X}_", c as u32));
                    } else {
                        escaped.push(c);
                    }
                }
                attribute_value(&escaped, &mut self.out);
            } else {
                attribute_value(value, &mut self.out);
            }
        }
        self.out
            .extend_from_slice(if empty { b"/>" as &[u8] } else { b">" });
    }

    pub(crate) fn open(&mut self, name: &str, attributes: &[(&str, String)]) {
        self.tag(name, attributes, false);
    }

    pub(crate) fn empty(&mut self, name: &str, attributes: &[(&str, String)]) {
        self.tag(name, attributes, true);
    }

    pub(crate) fn close(&mut self, name: &str) {
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(name.as_bytes());
        self.out.push(b'>');
    }

    /// Element content of type ST_Xstring.
    pub(crate) fn text(&mut self, units: &[u16]) {
        xstring(units, &mut self.out);
    }

    pub(crate) fn text_str(&mut self, text: &str) {
        let units: Vec<u16> = text.encode_utf16().collect();
        xstring(&units, &mut self.out);
    }

    /// `<name attributes>text</name>`.
    pub(crate) fn element(&mut self, name: &str, attributes: &[(&str, String)], text: &str) {
        self.open(name, attributes);
        self.text_str(text);
        self.close(name);
    }
}

/// The relationships part of `part`.
fn relationships_part(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// The SpreadsheetML package an Excel binary workbook whose workbook part is `workbook` stands
/// for, as the bytes of a ZIP archive.
pub(crate) fn convert(package: &Package, workbook: &str) -> Result<Vec<u8>, OpenError> {
    // What each part becomes.
    let mut kinds: BTreeMap<String, Kind> = BTreeMap::new();
    for part in package.parts() {
        let name = part.name();
        if name.ends_with(".rels") {
            continue;
        }
        let kind = match part.content_type() {
            Some(content_type) => kind_of(content_type)
                .map_err(|other| refusal(name, format!("a part of the content type {other}")))?,
            None => Kind::Kept,
        };
        kinds.insert(name.to_owned(), kind);
    }
    let read = |name: &str| -> Result<Vec<u8>, OpenError> {
        package.read_part(name).map_err(|error| OpenError::Part {
            part: name.to_owned(),
            error,
        })
    };
    // A workbook with a VBA project or a macro sheet is a macro-enabled one.
    let macros = kinds.iter().any(|(name, kind)| {
        matches!(kind, Kind::MacroSheet { .. })
            || package
                .part(name)
                .and_then(|p| p.content_type())
                .is_some_and(|ct| ct == "application/vnd.ms-office.vbaProject")
    });

    // The workbook first: its sheets, names and external references are what formulas name.
    let book = workbook::read(package, workbook, &read)?;
    // The converted parts, by their new names.
    let mut written: BTreeMap<String, (Vec<u8>, &'static str)> = BTreeMap::new();
    let shared = parts::Shared::new(package, workbook, &book, &read)?;
    written.insert(
        xml_name(workbook),
        (
            workbook::write(&book, &shared)?,
            xml_content_type(Kind::Workbook, macros),
        ),
    );
    for (name, kind) in &kinds {
        let content = match kind {
            Kind::Workbook | Kind::Dropped | Kind::Kept => continue,
            Kind::Worksheet => {
                let tab = book
                    .sheets
                    .iter()
                    .position(|sheet| sheet.part.as_deref() == Some(name.as_str()));
                sheet::convert(name, &read(name)?, tab, &shared, false)?
            }
            Kind::MacroSheet { .. } => {
                let tab = book
                    .sheets
                    .iter()
                    .position(|sheet| sheet.part.as_deref() == Some(name.as_str()));
                sheet::convert(name, &read(name)?, tab, &shared, true)?
            }
            Kind::Styles => styles::convert(name, &read(name)?)?,
            Kind::SharedStrings => parts::shared_strings(name, &read(name)?, &shared)?,
            Kind::Table => parts::table(name, &read(name)?, &shared)?,
            Kind::ExternalLink => parts::external_link(name, &read(name)?)?,
            Kind::Comments => parts::comments(name, &read(name)?, &shared)?,
            Kind::Metadata => parts::metadata(name, &read(name)?)?,
        };
        written.insert(xml_name(name), (content, xml_content_type(*kind, macros)));
    }

    // The parts carried over, the relationships rewritten.
    let renamed = |target: &str| -> Option<String> {
        // The new name of the part `target` resolves to, `None` for one left out.
        match kinds.get(target) {
            Some(Kind::Dropped) => None,
            Some(Kind::Kept) | None => Some(target.to_owned()),
            Some(_) => Some(xml_name(target)),
        }
    };
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut content_types: BTreeMap<String, String> = BTreeMap::new();
    for (name, (content, content_type)) in written {
        content_types.insert(name.clone(), (*content_type).to_owned());
        entries.push((name, content));
    }
    for (name, kind) in &kinds {
        if *kind != Kind::Kept {
            continue;
        }
        let content_type = package
            .part(name)
            .and_then(|p| p.content_type())
            .unwrap_or("application/octet-stream");
        content_types.insert(name.clone(), content_type.to_owned());
        entries.push((name.clone(), read(name)?));
    }
    for relationships in package.relationships() {
        let source = relationships.source();
        if let Some(source) = source
            && renamed(source).is_none()
        {
            continue;
        }
        let new_part = match source {
            Some(source) => relationships_part(&renamed(source).expect("kept above")),
            None => "_rels/.rels".to_owned(),
        };
        let mut xml = Xml::new();
        xml.open(
            "Relationships",
            &[(
                "xmlns",
                "http://schemas.openxmlformats.org/package/2006/relationships".to_owned(),
            )],
        );
        for item in relationships.items() {
            let mut target = item.target.clone();
            if item.mode == TargetMode::Internal
                && let Some(resolved) = &item.resolved
            {
                match renamed(resolved) {
                    None => continue,
                    Some(new) if &new != resolved => {
                        if let Some(stem) = target.strip_suffix(".bin") {
                            target = format!("{stem}.xml");
                        }
                    }
                    Some(_) => {}
                }
            }
            let mut attributes = vec![
                ("Id", item.id.clone()),
                ("Type", item.kind.clone()),
                ("Target", target),
            ];
            if item.mode == TargetMode::External {
                attributes.push(("TargetMode", "External".to_owned()));
            }
            xml.empty("Relationship", &attributes);
        }
        xml.close("Relationships");
        content_types.insert(
            new_part.clone(),
            "application/vnd.openxmlformats-package.relationships+xml".to_owned(),
        );
        entries.push((new_part, xml.out));
    }
    // Content types: a default for rels and xml, an override for every other part.
    let mut types = Xml::new();
    types.open(
        "Types",
        &[(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/content-types".to_owned(),
        )],
    );
    types.empty(
        "Default",
        &[
            ("Extension", "rels".to_owned()),
            (
                "ContentType",
                "application/vnd.openxmlformats-package.relationships+xml".to_owned(),
            ),
        ],
    );
    types.empty(
        "Default",
        &[
            ("Extension", "xml".to_owned()),
            ("ContentType", "application/xml".to_owned()),
        ],
    );
    for (name, content_type) in &content_types {
        let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let defaulted = match extension.as_str() {
            "rels" => content_type == "application/vnd.openxmlformats-package.relationships+xml",
            "xml" => content_type == "application/xml",
            _ => false,
        };
        if !defaulted {
            types.empty(
                "Override",
                &[
                    ("PartName", format!("/{name}")),
                    ("ContentType", content_type.clone()),
                ],
            );
        }
    }
    types.close("Types");
    let mut all: Vec<(&str, &[u8])> = vec![("[Content_Types].xml", types.out.as_slice())];
    all.extend(entries.iter().map(|(n, c)| (n.as_str(), c.as_slice())));
    crate::save::new_archive(&all).ok_or_else(|| {
        refusal(
            workbook,
            "the converted package is too large for a ZIP archive",
        )
    })
}

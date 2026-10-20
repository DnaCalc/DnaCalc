//! Loading a workbook: its sheets, from the workbook part and its relationships, and each sheet's
//! cells, into a calc-doc-model [`ModelRoot`] at the document's first revision.
//!
//! The model holds each occupied cell's authored input: a number, a text (shared or inline), a
//! logical, an error or a formula's text. Everything else a cell record carries is kept beside the
//! model, per sheet, in a [`LoadedSheet`]: the cell's style index, and for a formula its shared
//! formula group and its cached result, which is [`FileCached`] with the identity of the sheet's
//! part and never part of the input. Only cells that hold a value or a formula are materialized
//! in the model (a styled cell without either has only its style index beside it), and nothing
//! is allocated for the extent the cells span.
//!
//! Of the workbook part's other elements, the loaded workbook carries the date system its
//! `workbookPr` names ([`DateSystem`]) and the number of `definedName` elements it holds, which
//! are counted and not loaded. `calcPr` and every other element of the workbook part are not
//! read.
//!
//! A loaded workbook keeps the package it was loaded from, so [`LoadedWorkbook::snapshot`] can
//! give a save any revision of its model together with the records the save rewrites from.

pub(crate) mod address;
mod error;
pub(crate) mod sheet;
mod shift;
pub(crate) mod strings;

pub use error::{CellError, LoadError};

use crate::OpenedDocument;
use crate::error::PartReadError;
use crate::opc::{Package, TargetMode};
use crate::save::DocumentSnapshot;
use crate::xml::{XmlElement, XmlEvent, XmlPart, XmlReader};
use calc_doc_model::value::{CellKey, ExcelText, GridArea, IdAllocator, SheetId};
use calc_doc_model::{
    AuthoredRevision, FileCached, ModelRoot, SheetEntry, SheetKey, SheetName, SheetVisibility,
    SourcePart,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The longest formula text loaded, in UTF-16 code units of the text a cell's `<f>` element holds
/// with its `_xHHHH_` escapes decoded (and, for a shared formula's member, after the master's text
/// is moved to it). Microsoft documents a formula-contents limit of 8,192 characters; whether
/// Excel counts the leading `=` or counts UTF-16 code units rather than characters has not been
/// measured here.
pub const FORMULA_MAX_UTF16_CODE_UNITS: usize = 8_192;

pub(crate) const MAIN_NAMESPACE: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIP_ATTRIBUTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
pub(crate) const SHARED_STRINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
const STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";

/// The content types of a workbook part: a workbook, a macro-enabled workbook, and their
/// templates.
const WORKBOOK_CONTENT_TYPES: [&str; 4] = [
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
    "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml",
    "application/vnd.ms-excel.template.macroEnabled.main+xml",
];

/// One sheet's records beside the model: its part, and the style and formula records of its
/// occupied cells.
#[derive(Debug)]
pub struct LoadedSheet {
    id: SheetId,
    relationship: String,
    part: Arc<SourcePart>,
    styles: BTreeMap<(u32, u32), u32>,
    formulas: BTreeMap<(u32, u32), FormulaRecord>,
}

impl LoadedSheet {
    /// The sheet's ID in the model.
    pub fn id(&self) -> SheetId {
        self.id
    }

    /// The ID of the workbook part's relationship to the sheet's part (its `r:id`).
    pub fn relationship(&self) -> &str {
        &self.relationship
    }

    /// The sheet's part, which every cached value of the sheet names as its source.
    pub fn part(&self) -> &Arc<SourcePart> {
        &self.part
    }

    /// The style (cell format) index of the cell at `row` and `column`: its `s` attribute, 0 when
    /// it has none or the sheet holds no record of it.
    pub fn style(&self, row: u32, column: u32) -> u32 {
        self.styles.get(&(row, column)).copied().unwrap_or(0)
    }

    /// The cells whose style index is not 0, as (row, column, style), in row-major order. A
    /// styled cell without a value is here and not in the model.
    pub fn styled_cells(&self) -> impl Iterator<Item = (u32, u32, u32)> + '_ {
        self.styles
            .iter()
            .map(|(&(row, column), &style)| (row, column, style))
    }

    /// The formula record of the cell at `row` and `column`.
    pub fn formula(&self, row: u32, column: u32) -> Option<&FormulaRecord> {
        self.formulas.get(&(row, column))
    }

    /// Every formula record, as (row, column, record), in row-major order.
    pub fn formulas(&self) -> impl Iterator<Item = (u32, u32, &FormulaRecord)> + '_ {
        self.formulas
            .iter()
            .map(|(&(row, column), record)| (row, column, record))
    }
}

/// What a sheet's record of a formula cell holds beside the formula's text, which is the cell's
/// input in the model.
#[derive(Debug, PartialEq, Eq)]
pub struct FormulaRecord {
    shared: Option<SharedFormula>,
    cache: Option<FileCached>,
}

impl FormulaRecord {
    /// The shared formula group the cell belongs to, as the file wrote it.
    pub fn shared(&self) -> Option<&SharedFormula> {
        self.shared.as_ref()
    }

    /// The result the file stored for the formula, `None` when it stored none.
    pub fn cache(&self) -> Option<&FileCached> {
        self.cache.as_ref()
    }
}

/// A shared formula group as the file wrote it: its index (`si`), the master cell that holds its
/// text, and the area (`ref`) its members lie in. Each member's input is the master's text with
/// its relative references moved to the member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharedFormula {
    index: u32,
    master: CellKey,
    area: GridArea,
}

impl SharedFormula {
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn master(&self) -> CellKey {
        self.master
    }

    pub fn area(&self) -> GridArea {
        self.area
    }
}

/// The date system a workbook's serial date numbers count in, from the `date1904` attribute of
/// its workbook part's `workbookPr` element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateSystem {
    /// Serial number 1 is 1 January 1900: `date1904` absent, `0` or `false`.
    Date1900,
    /// Serial number 0 is 1 January 1904: `date1904` is `1` or `true`.
    Date1904,
}

/// A loaded workbook: the model root of its sheets and their cells at the document's first
/// revision, each sheet's records beside it, and the package they were read from.
#[derive(Debug)]
pub struct LoadedWorkbook {
    package: Arc<Package>,
    model: Arc<ModelRoot>,
    workbook_part: String,
    sheets: Arc<[LoadedSheet]>,
    cell_formats: u32,
    date_system: DateSystem,
    defined_names_not_loaded: usize,
}

impl LoadedWorkbook {
    pub fn model(&self) -> &Arc<ModelRoot> {
        &self.model
    }

    /// The workbook part the package's relationships name.
    pub fn workbook_part(&self) -> &str {
        &self.workbook_part
    }

    /// Each sheet's records, in workbook order, as the model's sheets are.
    pub fn sheets(&self) -> &[LoadedSheet] {
        &self.sheets
    }

    pub fn sheet(&self, id: SheetId) -> Option<&LoadedSheet> {
        self.sheets.iter().find(|sheet| sheet.id == id)
    }

    /// The number of cell formats (`<xf>` elements of `<cellXfs>`) the styles part holds; 0
    /// without a styles part.
    pub fn cell_formats(&self) -> u32 {
        self.cell_formats
    }

    /// The date system the workbook part names.
    pub fn date_system(&self) -> DateSystem {
        self.date_system
    }

    /// The number of defined names (`definedName` elements) the workbook part holds, none of
    /// which is loaded.
    pub fn defined_names_not_loaded(&self) -> usize {
        self.defined_names_not_loaded
    }

    /// An immutable view of the document at `model`, a revision of this workbook's model, which a
    /// save is prepared from. The caller keeps `model`; a save reads it and never changes it.
    /// [`crate::prepare_save`] writes the cells in which `model` differs from the loaded revision
    /// and refuses a model that is not of the lineage of the model this load made
    /// ([`calc_doc_model::ModelRoot::same_lineage`]), such as one another load made.
    pub fn snapshot(&self, model: &Arc<ModelRoot>) -> DocumentSnapshot {
        DocumentSnapshot::edited(
            Arc::clone(&self.package),
            self.workbook_part.clone(),
            Arc::clone(&self.sheets),
            Arc::clone(&self.model),
            Arc::clone(model),
        )
    }
}

/// A pull over one part's XML events that turns refusals into [`LoadError`]s naming the part.
pub(crate) struct Events<'a> {
    reader: XmlReader<'a>,
    part: &'a str,
}

impl<'a> Events<'a> {
    pub(crate) fn new(xml: &'a XmlPart) -> Self {
        Events {
            reader: xml.reader(),
            part: xml.name(),
        }
    }

    pub(crate) fn part(&self) -> &'a str {
        self.part
    }

    pub(crate) fn invalid(&self, detail: &'static str) -> LoadError {
        LoadError::InvalidPart {
            part: self.part.to_owned(),
            detail,
        }
    }

    pub(crate) fn next(&mut self) -> Result<Option<XmlEvent>, LoadError> {
        self.reader.next_event().map_err(|error| LoadError::Xml {
            part: self.part.to_owned(),
            error,
        })
    }

    /// Reads the document element, refused with `detail` unless it is `local` in `namespace`.
    pub(crate) fn root(
        &mut self,
        namespace: &str,
        local: &str,
        detail: &'static str,
    ) -> Result<XmlElement, LoadError> {
        match self.next()? {
            Some(XmlEvent::Start(root)) if root.name.is(namespace, local) => Ok(root),
            _ => Err(self.invalid(detail)),
        }
    }

    /// Reads past the end of the element whose start tag was just read.
    pub(crate) fn skip(&mut self) -> Result<(), LoadError> {
        let mut depth = 1u32;
        while depth > 0 {
            match self.next()? {
                Some(XmlEvent::Start(_)) => depth += 1,
                Some(XmlEvent::End(_)) => depth -= 1,
                Some(XmlEvent::Text(_)) => {}
                None => return Err(self.invalid("the part ends inside an element")),
            }
        }
        Ok(())
    }

    /// The text of the element whose start tag was just read, which holds no element.
    pub(crate) fn text(&mut self) -> Result<String, LoadError> {
        let mut text = String::new();
        loop {
            match self.next()? {
                Some(XmlEvent::Text(more)) => text.push_str(&more),
                Some(XmlEvent::End(_)) => return Ok(text),
                Some(XmlEvent::Start(_)) => {
                    return Err(self.invalid("an element inside an element that holds text"));
                }
                None => return Err(self.invalid("the part ends inside an element")),
            }
        }
    }

    /// Checks that nothing follows the document element.
    pub(crate) fn end(&mut self) -> Result<(), LoadError> {
        match self.next()? {
            None => Ok(()),
            Some(_) => Err(self.invalid("content after the document element")),
        }
    }
}

pub(crate) fn read_xml(package: &Package, part: &str) -> Result<XmlPart, LoadError> {
    package.read_xml(part).map_err(|error| match error {
        PartReadError::NotAPart { name } => LoadError::MissingPart { part: name },
        error => LoadError::Part {
            part: part.to_owned(),
            error,
        },
    })
}

/// The value of the attribute `local` in the relationships namespace (`r:id`).
fn relationship_attribute<'e>(element: &'e XmlElement, local: &str) -> Option<&'e str> {
    element
        .attributes
        .iter()
        .find(|attribute| attribute.name.is(RELATIONSHIP_ATTRIBUTES, local))
        .map(|attribute| attribute.value.as_str())
}

/// One `<sheet>` of the workbook part, before its cells are read.
struct DeclaredSheet {
    name: SheetName,
    key: SheetKey,
    visibility: SheetVisibility,
    relationship: String,
    part: String,
}

/// The type of the workbook part's relationship `id` and the part it targets.
fn target(package: &Package, workbook: &str, id: &str) -> Result<(String, String), LoadError> {
    let invalid = |detail| LoadError::InvalidPart {
        part: workbook.to_owned(),
        detail,
    };
    let relationship = package
        .relationships_of(Some(workbook))
        .and_then(|relationships| relationships.items().iter().find(|item| item.id == id))
        .ok_or(invalid(
            "a sheet's r:id names no relationship of the workbook part",
        ))?;
    if relationship.mode == TargetMode::External {
        return Err(invalid(
            "a sheet's relationship targets a resource outside the package",
        ));
    }
    let part = relationship
        .resolved
        .clone()
        .ok_or(LoadError::MissingPart {
            part: relationship.target.clone(),
        })?;
    Ok((relationship.kind.clone(), part))
}

/// What is read from the workbook part.
struct WorkbookPart {
    /// The sheets it declares, in workbook order.
    sheets: Vec<DeclaredSheet>,
    date_system: DateSystem,
    /// The number of its `definedName` elements.
    defined_names: usize,
}

/// Reads the workbook part: its sheets, its date system and the number of its defined names.
fn read_workbook(package: &Package, workbook: &str) -> Result<WorkbookPart, LoadError> {
    let xml = read_xml(package, workbook)?;
    let mut events = Events::new(&xml);
    events.root(
        MAIN_NAMESPACE,
        "workbook",
        "the document element is not a SpreadsheetML workbook",
    )?;
    let mut declared = Vec::new();
    let mut names: BTreeSet<Vec<u16>> = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut date_system = None;
    let mut defined_names = 0;
    loop {
        let element = match events.next()? {
            Some(XmlEvent::Start(element)) => element,
            Some(XmlEvent::Text(text)) if text.trim().is_empty() => continue,
            Some(XmlEvent::Text(_)) => return Err(events.invalid("workbook holds text")),
            Some(XmlEvent::End(_)) => break,
            None => return Err(events.invalid("the part ends inside workbook")),
        };
        if element.name.is(MAIN_NAMESPACE, "workbookPr") {
            events.skip()?;
            if date_system.is_some() {
                return Err(events.invalid("a workbook with two workbookPr elements"));
            }
            // An xsd:boolean, whose lexical forms are 0, 1, false and true.
            date_system = Some(match element.attribute("date1904") {
                None | Some("0" | "false") => DateSystem::Date1900,
                Some("1" | "true") => DateSystem::Date1904,
                Some(_) => {
                    return Err(events.invalid("a workbookPr whose date1904 is not a boolean"));
                }
            });
            continue;
        }
        if element.name.is(MAIN_NAMESPACE, "definedNames") {
            loop {
                match events.next()? {
                    Some(XmlEvent::Start(name)) if name.name.is(MAIN_NAMESPACE, "definedName") => {
                        defined_names += 1;
                        events.skip()?;
                    }
                    Some(XmlEvent::Start(_)) => {
                        return Err(
                            events.invalid("an element other than definedName in definedNames")
                        );
                    }
                    Some(XmlEvent::Text(text)) if text.trim().is_empty() => {}
                    Some(XmlEvent::Text(_)) => {
                        return Err(events.invalid("definedNames holds text"));
                    }
                    Some(XmlEvent::End(_)) => break,
                    None => return Err(events.invalid("the part ends inside definedNames")),
                }
            }
            continue;
        }
        if !element.name.is(MAIN_NAMESPACE, "sheets") {
            events.skip()?;
            continue;
        }
        loop {
            let sheet = match events.next()? {
                Some(XmlEvent::Start(sheet)) if sheet.name.is(MAIN_NAMESPACE, "sheet") => sheet,
                Some(XmlEvent::Start(_)) => {
                    return Err(events.invalid("an element other than sheet in sheets"));
                }
                Some(XmlEvent::Text(text)) if text.trim().is_empty() => continue,
                Some(XmlEvent::Text(_)) => return Err(events.invalid("sheets holds text")),
                Some(XmlEvent::End(_)) => break,
                None => return Err(events.invalid("the part ends inside sheets")),
            };
            events.skip()?;
            let text = sheet
                .attribute("name")
                .ok_or(events.invalid("a sheet without a name"))?;
            let units = strings::decode(text);
            let name = SheetName::new(ExcelText::from_utf16_code_units(units.clone())).map_err(
                |error| LoadError::InvalidSheetName {
                    name: String::from_utf16_lossy(&units),
                    error,
                },
            )?;
            let folded: Vec<u16> = units
                .iter()
                .map(|&unit| match u8::try_from(unit) {
                    Ok(byte) => u16::from(byte.to_ascii_uppercase()),
                    Err(_) => unit,
                })
                .collect();
            if !names.insert(folded) {
                return Err(LoadError::DuplicateSheetName {
                    name: String::from_utf16_lossy(&units),
                });
            }
            let key: u32 = sheet
                .attribute("sheetId")
                .and_then(|text| text.parse().ok())
                .ok_or(events.invalid("a sheet without a sheetId that is a number"))?;
            if !keys.insert(key) {
                return Err(LoadError::DuplicateSheetKey { key });
            }
            let visibility = match sheet.attribute("state") {
                None | Some("visible") => SheetVisibility::Visible,
                Some("hidden") => SheetVisibility::Hidden,
                Some("veryHidden") => SheetVisibility::VeryHidden,
                Some(_) => {
                    return Err(
                        events.invalid("a sheet state other than visible, hidden or veryHidden")
                    );
                }
            };
            let relationship = relationship_attribute(&sheet, "id")
                .ok_or(events.invalid("a sheet without an r:id"))?
                .to_owned();
            let (kind, part) = target(package, workbook, &relationship)?;
            if kind != WORKSHEET {
                return Err(LoadError::UnsupportedSheet {
                    name: String::from_utf16_lossy(&units),
                    kind,
                });
            }
            declared.push(DeclaredSheet {
                name,
                key: SheetKey::new(key),
                visibility,
                relationship,
                part,
            });
        }
    }
    events.end()?;
    Ok(WorkbookPart {
        sheets: declared,
        date_system: date_system.unwrap_or(DateSystem::Date1900),
        defined_names,
    })
}

/// The number of cell formats in the styles part.
fn cell_formats(package: &Package, part: &str) -> Result<u32, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    events.root(
        MAIN_NAMESPACE,
        "styleSheet",
        "the document element is not a SpreadsheetML styleSheet",
    )?;
    let mut count = 0u32;
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "cellXfs") => loop {
                match events.next()? {
                    Some(XmlEvent::Start(xf)) => {
                        if xf.name.is(MAIN_NAMESPACE, "xf") {
                            count += 1;
                        }
                        events.skip()?;
                    }
                    Some(XmlEvent::Text(_)) => {}
                    Some(XmlEvent::End(_)) => break,
                    None => return Err(events.invalid("the part ends inside cellXfs")),
                }
            },
            Some(XmlEvent::Start(_)) => events.skip()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => break,
            None => return Err(events.invalid("the part ends inside styleSheet")),
        }
    }
    events.end()?;
    Ok(count)
}

/// The single part the workbook part's relationships give of type `kind`, if any.
pub(crate) fn workbook_part_of(
    package: &Package,
    workbook: &str,
    kind: &str,
) -> Result<Option<String>, LoadError> {
    let invalid = |detail| LoadError::InvalidPart {
        part: workbook.to_owned(),
        detail,
    };
    let Some(relationships) = package.relationships_of(Some(workbook)) else {
        return Ok(None);
    };
    let mut found = relationships
        .items()
        .iter()
        .filter(|item| item.kind == kind);
    let Some(relationship) = found.next() else {
        return Ok(None);
    };
    if found.next().is_some() {
        return Err(invalid(
            "two relationships of one kind that a workbook has once",
        ));
    }
    if relationship.mode == TargetMode::External {
        return Err(invalid(
            "a shared strings or styles relationship targets a resource outside the package",
        ));
    }
    relationship
        .resolved
        .clone()
        .map(Some)
        .ok_or(LoadError::MissingPart {
            part: relationship.target.clone(),
        })
}

/// Loads the workbook of an opened package: its sheets, in workbook order, each with the ID
/// `sheet_ids` issues it, and their cells, into a model root at [`AuthoredRevision::INITIAL`].
/// Refused, with every ID it issued released, when a part it reads is missing, not acceptable XML
/// or not shaped as SpreadsheetML gives it (two `workbookPr` elements or a `date1904` that is not
/// a boolean included), a sheet is not a worksheet or has a name or `sheetId` that is refused, or
/// a cell is refused ([`LoadError`]).
pub fn load(
    document: &OpenedDocument,
    sheet_ids: &mut IdAllocator<SheetId>,
) -> Result<LoadedWorkbook, LoadError> {
    let package = document.package();
    let workbook = {
        let detail = |detail| LoadError::Workbook { detail };
        let mut documents = package
            .relationships_of(None)
            .map(|relationships| relationships.items())
            .unwrap_or_default()
            .iter()
            .filter(|item| item.kind == OFFICE_DOCUMENT);
        let relationship = documents
            .next()
            .ok_or(detail("the package has no officeDocument relationship"))?;
        if documents.next().is_some() {
            return Err(detail("the package has two officeDocument relationships"));
        }
        if relationship.mode == TargetMode::External {
            return Err(detail(
                "the officeDocument relationship targets a resource outside the package",
            ));
        }
        relationship
            .resolved
            .clone()
            .ok_or(LoadError::MissingPart {
                part: relationship.target.clone(),
            })?
    };
    let content_type = package
        .part(&workbook)
        .ok_or(LoadError::MissingPart {
            part: workbook.clone(),
        })?
        .content_type();
    if !content_type.is_some_and(|content_type| WORKBOOK_CONTENT_TYPES.contains(&content_type)) {
        return Err(LoadError::NotASpreadsheet {
            part: workbook,
            content_type: content_type.map(str::to_owned),
        });
    }
    let WorkbookPart {
        sheets: declared,
        date_system,
        defined_names,
    } = read_workbook(package, &workbook)?;
    let shared_strings = match workbook_part_of(package, &workbook, SHARED_STRINGS)? {
        Some(part) => strings::shared_strings(package, &part)?,
        None => Vec::new(),
    };
    let cell_formats = match workbook_part_of(package, &workbook, STYLES)? {
        Some(part) => cell_formats(package, &part)?,
        None => 0,
    };

    let mut issued = Vec::with_capacity(declared.len());
    let release = |ids: &mut IdAllocator<SheetId>, issued: &[SheetId]| {
        for id in issued {
            ids.release(*id).expect("an ID this load issued is live");
        }
    };
    for _ in &declared {
        match sheet_ids.issue() {
            Ok(id) => issued.push(id),
            Err(error) => {
                release(sheet_ids, &issued);
                return Err(LoadError::SheetIds(error));
            }
        }
    }
    let context = sheet::Context {
        shared_strings: &shared_strings,
        cell_formats,
    };
    let mut cells = Vec::new();
    let mut sheets = Vec::with_capacity(declared.len());
    for (declared, &id) in declared.iter().zip(&issued) {
        match sheet::load_sheet(package, &context, id, &declared.part) {
            Ok(loaded) => {
                cells.extend(loaded.inputs);
                sheets.push(LoadedSheet {
                    id,
                    relationship: declared.relationship.clone(),
                    part: loaded.part,
                    styles: loaded.styles,
                    formulas: loaded.formulas,
                });
            }
            Err(error) => {
                release(sheet_ids, &issued);
                return Err(error);
            }
        }
    }
    let entries = declared
        .into_iter()
        .zip(&issued)
        .map(|(declared, &id)| {
            SheetEntry::new(id, declared.key, declared.name, declared.visibility)
        })
        .collect();
    let model = ModelRoot::with_cells(AuthoredRevision::INITIAL, entries, cells)
        .expect("the IDs are distinct, the keys were checked and each cell lies on its own sheet");
    Ok(LoadedWorkbook {
        package: Arc::clone(document.package_arc()),
        model,
        workbook_part: workbook,
        sheets: sheets.into(),
        cell_formats,
        date_system,
        defined_names_not_loaded: defined_names,
    })
}

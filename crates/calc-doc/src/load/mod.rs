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
//! `workbookPr` names ([`DateSystem`]), the calculation mode `calcPr`'s `calcMode` names
//! ([`CalcMode`]), and its `definedName` elements: a block's locator becomes the block's anchor,
//! and every other defined name is counted and not loaded. `calcPr`'s other attributes and every
//! other element of the workbook part are not read.
//!
//! The package's DNA view manifest, when it holds one, is read into the model's skin layout
//! ([`manifest`]): its blocks, each anchored where its locator refers, and its placements in the
//! four views. A manifest that is not read, and a locator that does not resolve, is a
//! [`LayoutWarning`] of the loaded workbook, never a refusal: the cells load as they would
//! without it, and the manifest's parts stay in the package as they are.
//!
//! A loaded workbook keeps the package it was loaded from, so [`LoadedWorkbook::snapshot`] can
//! give a save any revision of its model together with the records the save rewrites from.

pub(crate) mod address;
mod chart;
pub(crate) mod conditional;
mod error;
pub(crate) mod geometry;
pub(crate) mod manifest;
pub(crate) mod metadata;
pub(crate) mod sheet;
mod shift;
pub(crate) mod strings;
mod styles;
pub(crate) mod tables;
mod theme;
pub(crate) mod validation;

pub use conditional::{
    ColorScale, ConditionalFormat, ConditionalRule, DataBar, IconSet, Threshold,
};
pub use chart::{
    Chart, ChartAnchor, ChartData, ChartPlot, ChartSeries, DrawingPoint, SheetCharts,
};
pub use error::{CellError, LoadError};
pub use validation::Validation;
pub use geometry::{ColumnSpan, RowFormat, SheetGeometry};
pub use manifest::LayoutWarning;

use crate::OpenedDocument;
use crate::error::PartReadError;
use crate::opc::{Package, TargetMode};
use crate::save::{DocumentSnapshot, SaveRefusal};
use crate::xml::{XmlElement, XmlEvent, XmlPart, XmlReader};
use calc_doc_model::value::{CellKey, ExcelText, GridArea, IdAllocator, SheetId};
use calc_doc_model::{
    AuthoredRevision, Definitions, FileCached, IterationSettings, LocatorName, ModelRoot,
    NameScope, RowHidden, SheetEntry, SheetKey, SheetName, SheetVisibility, SkinLayoutEdit,
    SourcePart, StyleTable, WorkbookOp,
};
use manifest::{DefinedName, ManifestState};
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
/// An Excel 4.0 macro sheet, and an international one (dc-xuw).
const MACRO_SHEET: &str = "http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet";
const INTERNATIONAL_MACRO_SHEET: &str =
    "http://schemas.microsoft.com/office/2006/relationships/xlIntlMacrosheet";
pub(crate) const SHARED_STRINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
const STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";

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
    value_caches: BTreeMap<(u32, u32), FileCached>,
    geometry: SheetGeometry,
    conditional_formats: Arc<[ConditionalFormat]>,
    macro_sheet: bool,
    validations: Arc<[Validation]>,
    charts: Arc<SheetCharts>,
}

impl LoadedSheet {
    /// Whether the sheet is an Excel 4.0 macro sheet (dc-xuw): its formula cells are unloaded
    /// ([`calc_doc_model::UnloadedCell::MacroSheet`]) and a save that would change its part is
    /// refused ([`crate::SaveRefusal::MacroSheetChanged`]).
    pub fn is_macro_sheet(&self) -> bool {
        self.macro_sheet
    }

    /// The sheet's data validations as the file states them, in document order (dc-6yw).
    pub fn validations(&self) -> &[Validation] {
        &self.validations
    }

    /// The sheet's charts as the file states them, and how many could not be read (dc-6yw).
    pub fn charts(&self) -> &SheetCharts {
        &self.charts
    }

    /// The sheet's conditional formats as the file states them, in document order (dc-6yw).
    pub fn conditional_formats(&self) -> &[ConditionalFormat] {
        &self.conditional_formats
    }

    /// The sheet's row heights, column widths and hidden rows and columns as the file states
    /// them (dc-09v).
    pub fn geometry(&self) -> &SheetGeometry {
        &self.geometry
    }

    /// What the file cached for the cell at `row` and `column` when the model holds no input for
    /// it: a cell an array formula fills beside its anchor, or an unloaded cell
    /// ([`calc_doc_model::Definitions`]). Nothing here calculated it.
    pub fn value_cache(&self, row: u32, column: u32) -> Option<&FileCached> {
        self.value_caches.get(&(row, column))
    }

    /// Every such cache, as (row, column, cache), in row-major order.
    pub fn value_caches(&self) -> impl Iterator<Item = (u32, u32, &FileCached)> + '_ {
        self.value_caches
            .iter()
            .map(|(&(row, column), cache)| (row, column, cache))
    }

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

/// How a workbook asks to be recalculated, from the `calcMode` attribute of its workbook part's
/// `calcPr` element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CalcMode {
    /// After every change: `calcMode` absent or `auto`.
    #[default]
    Automatic,
    /// After every change, except what-if data tables: `autoNoTable`.
    AutomaticExceptTables,
    /// Only when asked: `manual`.
    Manual,
}

/// A loaded workbook: the model root of its sheets, their cells and its skin layout at the
/// document's first revision, each sheet's records beside it, and the package they were read
/// from.
#[derive(Debug)]
pub struct LoadedWorkbook {
    package: Arc<Package>,
    model: Arc<ModelRoot>,
    workbook_part: String,
    sheets: Arc<[LoadedSheet]>,
    cell_formats: u32,
    styles: Arc<StyleTable>,
    theme_colors: Arc<[String]>,
    date_system: DateSystem,
    calc_mode: CalcMode,
    defined_names_not_loaded: usize,
    manifest: ManifestState,
    layout_warnings: Vec<LayoutWarning>,
    /// The workbook-scoped defined names no layout save removes.
    fixed_names: Vec<String>,
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

    /// The style table the styles part declares; empty without a styles part. A cell's style
    /// index ([`LoadedSheet::style`]) names one of its cell formats.
    pub fn styles(&self) -> &Arc<StyleTable> {
        &self.styles
    }

    /// The theme part's twelve colours as `RRGGBB`, in its colour scheme's order (dk1, lt1, dk2,
    /// lt2, accent1 to accent6, hlink, folHlink); empty without a theme part (dc-6yw).
    pub fn theme_colors(&self) -> &[String] {
        &self.theme_colors
    }

    /// The date system the workbook part names.
    pub fn date_system(&self) -> DateSystem {
        self.date_system
    }

    /// The calculation mode the workbook part's `calcPr` names.
    pub fn calc_mode(&self) -> CalcMode {
        self.calc_mode
    }

    /// The number of defined names (`definedName` elements) the workbook part holds that are not
    /// a block's locator: every one but the locators that became a block's anchor, so all of them
    /// for a workbook whose DNA view manifest is absent or was not read. Each of these is in the
    /// model's definitions ([`calc_doc_model::Definitions::names`]) unless its `localSheetId`
    /// names no sheet.
    pub fn defined_names_not_loaded(&self) -> usize {
        self.defined_names_not_loaded
    }

    /// The DNA view manifest's item part, whether or not it was read; `None` when the package
    /// holds none. Of several, the first, which [`LoadedWorkbook::layout_warnings`] reports.
    pub fn manifest_part(&self) -> Option<&str> {
        match &self.manifest {
            ManifestState::Absent => None,
            ManifestState::Read { part } | ManifestState::Unread { part } => Some(part),
        }
    }

    /// Why the model's layout lacks what the package holds: a manifest that was not read, or a
    /// block whose locator did not resolve. Empty when the layout is all the package holds.
    pub fn layout_warnings(&self) -> &[LayoutWarning] {
        &self.layout_warnings
    }

    /// Whether a save of this workbook can write the skin layout edits among `operations`,
    /// judged before they are applied, so a caller can refuse a layout edit before it joins cell
    /// edits in one revision: the refusal [`crate::prepare_save`] gives a revision that holds them.
    /// [`SaveRefusal::LayoutNotWritable`] when `operations` hold a layout edit and the package's
    /// DNA view manifest was not read ([`LoadedWorkbook::layout_warnings`]), and
    /// [`SaveRefusal::LocatorNameTaken`] when a block they create would get the locator name of a
    /// workbook-scoped defined name that a save keeps (every one but the hidden locators of the
    /// loaded blocks). `Ok` for operations without a layout edit.
    ///
    /// The judgement is of each edit, not of the revision: edits that leave the layout as loaded
    /// (a block created and removed again) are judged as edits, though a save of that revision
    /// writes no layout. [`crate::prepare_save`] judges the revision and stays the last check; it
    /// also refuses for reasons this does not judge, such as a workbook part it cannot rewrite.
    pub fn check_layout_save(&self, operations: &[WorkbookOp]) -> Result<(), SaveRefusal> {
        let mut edits = operations
            .iter()
            .filter_map(|operation| match operation {
                WorkbookOp::EditSkinLayout(edit) => Some(edit),
                _ => None,
            })
            .peekable();
        if edits.peek().is_none() {
            return Ok(());
        }
        if let ManifestState::Unread { part } = &self.manifest {
            return Err(SaveRefusal::LayoutNotWritable { part: part.clone() });
        }
        for edit in edits {
            if let SkinLayoutEdit::CreateBlock { block, .. } = edit {
                let name = LocatorName::for_block(*block);
                if self.fixed_names.iter().any(|fixed| name.is(fixed)) {
                    let part = self
                        .package
                        .part(&self.workbook_part)
                        .map_or(self.workbook_part.as_str(), |part| part.name());
                    return Err(SaveRefusal::LocatorNameTaken {
                        part: part.to_owned(),
                        name: name.as_str().to_owned(),
                    });
                }
            }
        }
        Ok(())
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
            self.manifest.clone(),
            Arc::clone(&self.model),
            Arc::clone(model),
            None,
            Arc::default(),
        )
    }

    /// [`LoadedWorkbook::snapshot`] with the values a caller calculated for `model`'s formula
    /// cells, which a save of a revision that differs from the loaded one writes as their cached
    /// values (`<v>`, with `t` for a text, a logical or an error). A newer error (#SPILL!, #CALC!
    /// and the others but #BUSY!) is written as Excel writes it: `t="e"`, the `#VALUE!` fallback
    /// as `<v>`, and value metadata (`vm`) naming an `_error` rich value with its `errorType`. A
    /// formula cell without a value here (one the caller refused), and one whose value is #BUSY!,
    /// is written without a cached value. Excel calculates an ordinary formula without one when
    /// it opens the package, but not an array formula, so such a save sets `fullCalcOnLoad` when
    /// it writes an array formula without a value, and when the workbook holds a data table,
    /// whose cached results it keeps (tests/fixtures/d1/excel-open.json). A save of the loaded
    /// revision itself copies the package as opened, values or not.
    /// [`crate::prepare_save`] refuses a value for a cell that holds no formula
    /// ([`SaveRefusal::ValueWithoutFormula`]).
    pub fn snapshot_with_values(
        &self,
        model: &Arc<ModelRoot>,
        values: BTreeMap<CellKey, calc_doc_model::CachedValue>,
    ) -> DocumentSnapshot {
        self.snapshot_with_results(model, values, BTreeMap::new())
    }

    /// [`LoadedWorkbook::snapshot_with_values`] with the area each array formula's result fills,
    /// by anchor (`spills`): the anchor alone for a result that fills nothing beside it (a single
    /// value, or #SPILL!), the spill range of a dynamic array's result, or a legacy array's fixed
    /// area. An array formula's anchor is written with that area as its `ref`, and `values` may
    /// then hold a value for each cell of the area beside the anchor, which is written as Excel
    /// writes a spilled cell: the value alone, no formula. The spill of an anchor the save does
    /// not write as an array formula (a formula Excel saved as an ordinary one) is not written:
    /// its anchor is left without a cached value, for Excel to calculate on open, and its cells
    /// beside the anchor are not written.
    pub fn snapshot_with_results(
        &self,
        model: &Arc<ModelRoot>,
        values: BTreeMap<CellKey, calc_doc_model::CachedValue>,
        spills: BTreeMap<CellKey, calc_doc_model::value::GridArea>,
    ) -> DocumentSnapshot {
        DocumentSnapshot::edited(
            Arc::clone(&self.package),
            self.workbook_part.clone(),
            Arc::clone(&self.sheets),
            self.manifest.clone(),
            Arc::clone(&self.model),
            Arc::clone(model),
            Some(Arc::new(values)),
            Arc::new(spills),
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
                // The first run of text is taken as it is (dc-dxm), without copying it.
                Some(XmlEvent::Text(more)) if text.is_empty() => text = more,
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
    /// An Excel 4.0 macro sheet rather than a worksheet.
    macro_sheet: bool,
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
    calc_mode: CalcMode,
    /// `calcPr`'s `iterate`, `iterateCount` and `iterateDelta`.
    iteration: IterationSettings,
    /// Its `definedName` elements, in order.
    defined_names: Vec<DefinedName>,
}

/// The text of the `definedName` element whose start tag was just read, with its `_xHHHH_`
/// escapes decoded; the text of an element inside it, which the schema does not give it, is not
/// part of it.
fn defined_name_text(events: &mut Events<'_>) -> Result<String, LoadError> {
    let mut text = String::new();
    loop {
        match events.next()? {
            Some(XmlEvent::Text(more)) => text.push_str(&more),
            Some(XmlEvent::Start(_)) => events.skip()?,
            Some(XmlEvent::End(_)) => return Ok(String::from_utf16_lossy(&strings::decode(&text))),
            None => return Err(events.invalid("the part ends inside definedName")),
        }
    }
}

/// Reads the workbook part: its sheets, its date system and its defined names.
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
    let mut calc_mode = CalcMode::Automatic;
    let mut iteration = IterationSettings::EXCEL_DEFAULT;
    let mut defined_names = Vec::new();
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
        if element.name.is(MAIN_NAMESPACE, "calcPr") {
            events.skip()?;
            calc_mode = match element.attribute("calcMode").map(str::trim) {
                None | Some("auto") => CalcMode::Automatic,
                Some("autoNoTable") => CalcMode::AutomaticExceptTables,
                Some("manual") => CalcMode::Manual,
                Some(_) => return Err(events.invalid("a calcPr whose calcMode is not a mode")),
            };
            // L3 (dc-m3u): the iterative-calculation settings, Excel's defaults where absent.
            let defaults = IterationSettings::EXCEL_DEFAULT;
            let enabled = match element.attribute("iterate").map(str::trim) {
                None | Some("0" | "false") => false,
                Some("1" | "true") => true,
                Some(_) => return Err(events.invalid("a calcPr whose iterate is not a boolean")),
            };
            let count = match element.attribute("iterateCount").map(str::trim) {
                None => defaults.max_iterations(),
                Some(text) => text.parse::<u32>().map_err(|_| {
                    events.invalid("a calcPr whose iterateCount is not an unsigned integer")
                })?,
            };
            let delta = match element.attribute("iterateDelta").map(str::trim) {
                None => defaults.max_change(),
                Some(text) => text
                    .parse::<f64>()
                    .ok()
                    .filter(|delta| delta.is_finite())
                    .ok_or_else(|| events.invalid("a calcPr whose iterateDelta is not a number"))?,
            };
            iteration = IterationSettings::new(enabled, count, delta);
            continue;
        }
        if element.name.is(MAIN_NAMESPACE, "definedNames") {
            loop {
                match events.next()? {
                    Some(XmlEvent::Start(name)) if name.name.is(MAIN_NAMESPACE, "definedName") => {
                        defined_names.push(DefinedName {
                            name: name.attribute("name").unwrap_or_default().to_owned(),
                            sheet_scoped: name.attribute("localSheetId").is_some(),
                            hidden: manifest::is_true(name.attribute("hidden")),
                            local_sheet: name
                                .attribute("localSheetId")
                                .and_then(|text| sheet::collapsed(text).parse().ok()),
                            comment: name.attribute("comment").map(str::to_owned),
                            function: manifest::is_true(name.attribute("function")),
                            vb_procedure: manifest::is_true(name.attribute("vbProcedure")),
                            xlm: manifest::is_true(name.attribute("xlm")),
                            text: defined_name_text(&mut events)?,
                        });
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
            let macro_sheet = kind == MACRO_SHEET || kind == INTERNATIONAL_MACRO_SHEET;
            if kind != WORKSHEET && !macro_sheet {
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
                macro_sheet,
            });
        }
    }
    events.end()?;
    Ok(WorkbookPart {
        sheets: declared,
        date_system: date_system.unwrap_or(DateSystem::Date1900),
        calc_mode,
        iteration,
        defined_names,
    })
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
    let macro_enabled =
        content_type.is_some_and(|content_type| content_type.contains("macroEnabled"));
    let WorkbookPart {
        sheets: declared,
        date_system,
        calc_mode,
        iteration,
        defined_names,
    } = read_workbook(package, &workbook)?;
    let shared_strings = match workbook_part_of(package, &workbook, SHARED_STRINGS)? {
        Some(part) => strings::shared_strings(package, &part)?,
        None => Vec::new(),
    };
    let styles = match workbook_part_of(package, &workbook, STYLES)? {
        Some(part) => styles::read_styles(package, &part)?,
        None => StyleTable::default(),
    };
    let cell_formats = u32::try_from(styles.cell_formats.len()).unwrap_or(u32::MAX);
    let theme_colors = match workbook_part_of(package, &workbook, THEME)? {
        Some(part) => theme::read_theme(package, &part)?,
        None => Vec::new(),
    };
    let metadata = metadata::read(package, &workbook)?;

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
        metadata: &metadata,
    };
    let mut cells = Vec::new();
    let mut sheets = Vec::with_capacity(declared.len());
    let (mut arrays, mut unloaded, mut tables) = (Vec::new(), Vec::new(), Vec::new());
    let mut data_tables = Vec::new();
    let mut hidden_rows: Vec<(SheetId, u32, Option<(u32, u32, u32, u32)>)> = Vec::new();
    for (declared, &id) in declared.iter().zip(&issued) {
        let loaded = sheet::load_sheet(
            package,
            &context,
            id,
            &declared.part,
            declared.macro_sheet,
        )
            .and_then(|loaded| Ok((loaded, tables::read_tables(package, &declared.part, id)?)));
        match loaded {
            Ok((loaded, sheet_tables)) => {
                cells.extend(loaded.inputs);
                arrays.extend(loaded.arrays);
                data_tables.extend(loaded.data_tables.iter().copied());
                hidden_rows.extend(
                    loaded
                        .hidden_rows
                        .iter()
                        .map(|row| (id, *row, loaded.auto_filter)),
                );
                unloaded.extend(loaded.unloaded);
                tables.extend(sheet_tables);
                sheets.push(LoadedSheet {
                    id,
                    relationship: declared.relationship.clone(),
                    part: loaded.part,
                    styles: loaded.styles,
                    formulas: loaded.formulas,
                    value_caches: loaded.value_caches,
                    geometry: loaded.geometry,
                    conditional_formats: loaded.conditional_formats.into(),
                    macro_sheet: declared.macro_sheet,
                    validations: loaded.validations.into(),
                    charts: Arc::new(chart::read_charts(package, &declared.part)),
                });
            }
            Err(error) => {
                release(sheet_ids, &issued);
                return Err(error);
            }
        }
    }
    let named: Vec<(SheetId, &SheetName)> = declared
        .iter()
        .zip(&issued)
        .map(|(declared, &id)| (id, &declared.name))
        .collect();
    let layout = manifest::load_layout(package, &named, &defined_names);
    // Every defined name but a block's locator, which the layout holds, and one whose
    // localSheetId names no sheet.
    let is_locator = |name: &DefinedName| {
        !name.sheet_scoped
            && layout
                .layout
                .blocks()
                .iter()
                .any(|block| block.anchor().is_some() && block.locator().is(&name.name))
    };
    let names = defined_names
        .iter()
        .filter(|name| !is_locator(name))
        .filter_map(|name| {
            let scope = match name.local_sheet {
                None if !name.sheet_scoped => NameScope::Workbook,
                None => return None,
                Some(position) => NameScope::Sheet(*issued.get(position as usize)?),
            };
            Some(calc_doc_model::DefinedName {
                name: name.name.clone(),
                scope,
                formula: name.text.clone(),
                hidden: name.hidden,
                comment: name.comment.clone(),
                function: name.function,
                vb_procedure: name.vb_procedure,
                xlm: name.xlm,
            })
        })
        .collect();
    // A hidden row below the header of the sheet's filter range or of a table's (whose filter the
    // table part holds) is taken as hidden by the filter; any other hidden row as hidden by hand.
    let hidden_rows: Vec<(SheetId, u32, RowHidden)> = hidden_rows
        .into_iter()
        .map(|(sheet, row, auto_filter)| {
            let in_filter = auto_filter
                .is_some_and(|(top, _, bottom, _)| row > top && row <= bottom)
                || tables.iter().any(|table: &calc_doc_model::Table| {
                    table.sheet == sheet
                        && row > table.area.first().row()
                        && row <= table.area.last().row()
                });
            let why = if in_filter {
                RowHidden::Filtered
            } else {
                RowHidden::Manual
            };
            (sheet, row, why)
        })
        .collect();
    let definitions = Definitions::new(arrays, names, tables, unloaded)
        .with_hidden_rows(hidden_rows)
        .with_macro_enabled(macro_enabled)
        .with_iteration(iteration)
        .with_data_tables(data_tables)
        .with_workbook_name(package.source_name().map(str::to_owned));
    let entries = declared
        .into_iter()
        .zip(&issued)
        .map(|(declared, &id)| {
            SheetEntry::new(id, declared.key, declared.name, declared.visibility)
        })
        .collect();
    let model = ModelRoot::with_definitions(
        AuthoredRevision::INITIAL,
        entries,
        cells,
        layout.layout,
        definitions,
    )
    .expect(
        "the IDs are distinct, the keys were checked, each cell, anchor and definition lies on a sheet of the load, and each array anchor holds its formula",
    );
    Ok(LoadedWorkbook {
        package: Arc::clone(document.package_arc()),
        model,
        workbook_part: workbook,
        sheets: sheets.into(),
        cell_formats,
        styles: Arc::new(styles),
        theme_colors: theme_colors.into(),
        date_system,
        calc_mode,
        defined_names_not_loaded: defined_names.len() - layout.locators_resolved,
        manifest: layout.state,
        layout_warnings: layout.warnings,
        fixed_names: layout.fixed_names,
    })
}

//! What an authored save changes in the package: the parts it rewrites, removes and adds, all
//! serialized here, before any byte is written: for changed cells, the worksheets, the shared
//! strings, the calculation chain and `calcPr`; for a changed skin layout, the view manifest and
//! the workbook part's locator names.

use super::array_metadata;
use super::diff::{SheetChanges, sheet_changes};
use super::file_form::{FileForm, file_form_with_links};
use super::manifest::{self, NewManifest};
use super::parts::{self, NameChanges, NewRelationship};
use super::reader::Reader;
use super::sheet::{self, Job};
use super::strings::{self, SharedStrings, read_part};
use super::structural::{self, SheetMoves};
use super::{Edited, SaveRefusal};
use crate::edit::{FormulaCheck, check_formula};
use crate::load::address::column_letters;
use crate::load::manifest::{CUSTOM_XML, ManifestState, is_locator_form};
use crate::load::sheet::collapsed;
use crate::load::{self, LoadError, SHARED_STRINGS};
use crate::opc::{Package, TargetMode};
use crate::xml::XmlElement;
use calc_doc_model::value::{CellKey, GridArea, WorksheetErrorCode};
use calc_doc_model::{CachedValue, CellInput, DefinedName, NameScope, SheetName};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const CALC_CHAIN: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain";
const TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";

/// The parts an authored save writes differently from the source.
#[derive(Default)]
pub(super) struct Changes {
    /// The new content of each part rewritten, by part name.
    pub(super) rewritten: BTreeMap<String, Vec<u8>>,
    /// The new content of `[Content_Types].xml`, when it changes.
    pub(super) content_types: Option<Vec<u8>>,
    /// The parts left out.
    pub(super) removed: Vec<String>,
    /// The parts added, with their content.
    pub(super) added: Vec<(String, Vec<u8>)>,
    /// The formula cells the edit did not change whose cached result the save leaves out.
    pub(super) caches_dropped: usize,
    /// The worksheet parts rewritten that hold no changed cell, only to leave out cached results.
    pub(super) sheets_rewritten_for_caches: usize,
    /// The formula cells written with a caller's value.
    pub(super) values_written: usize,
    /// The formula cells of a save with values written without a cached value.
    pub(super) values_left_to_excel: usize,
}

fn load_refusal(part: &str, error: LoadError) -> SaveRefusal {
    SaveRefusal::InvalidPart {
        part: part.to_owned(),
        detail: error.to_string(),
    }
}

/// The rows of a table's header and totals rows, as (first, last) ranges, from its root element:
/// `headerRowCount` rows (1 when absent) from the top of its area and `totalsRowCount` rows (0 when
/// absent) up from the bottom.
fn header_and_totals(
    reader: &Reader<'_>,
    table: &XmlElement,
    top: u32,
    bottom: u32,
) -> Result<[Option<(u32, u32)>; 2], SaveRefusal> {
    let count = |name: &str, absent: u32| match table.attribute(name) {
        None => Ok(absent),
        Some(text) => collapsed(text)
            .parse::<u32>()
            .map_err(|_| reader.invalid(format!("a table whose {name} is not a count"))),
    };
    let header = count("headerRowCount", 1)?;
    let totals = count("totalsRowCount", 0)?;
    let header = (header > 0).then(|| (top, top.saturating_add(header - 1).min(bottom)));
    let totals = (totals > 0).then(|| (bottom.saturating_sub(totals - 1).max(top), bottom));
    Ok([header, totals])
}

/// Refuses a change to a cell in the header row or the totals row of a table (ListObject) of the
/// sheet's part: the table part records its header row's names and its totals row's functions,
/// which an edit of those cells would contradict; authoring tables is not built (no bead owns it
/// yet). A cell of
/// a table's data rows is written like any other cell; the table part does not record it.
fn check_tables(
    package: &Package,
    sheet_part: &str,
    changes: &SheetChanges,
    moves: Option<&SheetMoves>,
) -> Result<(), SaveRefusal> {
    let Some(relationships) = package.relationships_of(Some(sheet_part)) else {
        return Ok(());
    };
    for relationship in relationships.items() {
        if relationship.kind != TABLE || relationship.mode != TargetMode::Internal {
            continue;
        }
        let table = relationship
            .resolved
            .clone()
            .unwrap_or_else(|| relationship.target.clone());
        let xml = read_part(package, &table)?;
        let mut reader = Reader::new(&xml);
        let (root, _) = reader.root("table")?;
        let area = root
            .attribute("ref")
            .and_then(load::address::area)
            .ok_or_else(|| reader.invalid("a table without a ref that is an area"))?;
        // On a sheet whose rows or columns were inserted or deleted, the table's area moved with
        // them; the changes are by position after the edits.
        let area = match moves {
            Some(moves) => match moves.area(area) {
                Some(area) => area,
                None => continue,
            },
            None => area,
        };
        let (top, left, bottom, right) = area;
        let rows = header_and_totals(&reader, &root, top, bottom)?;
        if let Some(&(row, column)) = changes.keys().find(|(row, column)| {
            (left..=right).contains(column)
                && rows
                    .iter()
                    .flatten()
                    .any(|(first, last)| (*first..=*last).contains(row))
        }) {
            return Err(SaveRefusal::EditInTable {
                part: sheet_part.to_owned(),
                table,
                row,
                column,
            });
        }
    }
    Ok(())
}

/// Refuses a formula the revision sets that a save could not write, as [`crate::prepare_edit`]
/// refuses it: a revision made another way, such as by [`calc_doc_model::ModelRoot::apply`], is
/// checked here. Only the changed cells are checked; every other formula is written as the file
/// held it, or, for a member of a shared group written as an individual formula, as the loader
/// moved it from the group's text.
fn check_formulas(
    sheet_part: &str,
    changes: &SheetChanges,
    pivot_outputs: &BTreeSet<(u32, u32)>,
) -> Result<(), SaveRefusal> {
    for (&(row, column), input) in changes {
        // A pivot output cell holds #SPILL! when the pivot took one cell for lack of room; Excel's file
        // for it is `#VALUE!` with value metadata, which the cell writer gives it.
        if let Some(CellInput::Error(code)) = input
            && !crate::edit::error_constant_writable(*code)
            && !(pivot_outputs.contains(&(row, column))
                && array_metadata::rich_error_type(*code).is_some())
        {
            return Err(SaveRefusal::ErrorConstant {
                part: sheet_part.to_owned(),
                row,
                column,
                code: *code,
            });
        }
        let Some(CellInput::Formula(formula)) = input else {
            continue;
        };
        check_formula(formula).map_err(|check| {
            let part = sheet_part.to_owned();
            match check {
                FormulaCheck::TooLong(units) => SaveRefusal::FormulaTooLong {
                    part,
                    row,
                    column,
                    units,
                },
                FormulaCheck::Syntax(message) => SaveRefusal::FormulaSyntax {
                    part,
                    row,
                    column,
                    message,
                },
            }
        })?;
    }
    Ok(())
}

/// The file form of each formula `changes` set, by (row, column): its text with Excel's prefixes
/// and whether it is written as a dynamic array.
fn file_forms(
    sheet_part: &str,
    changes: &SheetChanges,
    names: &[DefinedName],
    links: &[(u32, String)],
) -> Result<BTreeMap<(u32, u32), FileForm>, SaveRefusal> {
    let mut forms = BTreeMap::new();
    for (&(row, column), input) in changes {
        let Some(CellInput::Formula(formula)) = input else {
            continue;
        };
        let rendered = calc_doc_model::FormulaText::with_form(formula.rendered(), formula.form())
            .expect("an admitted formula has nonempty source");
        let form = file_form_with_links(&rendered, names, links).map_err(|error| {
            SaveRefusal::FormulaFileForm {
                part: sheet_part.to_owned(),
                row,
                column,
                error,
            }
        })?;
        forms.insert((row, column), form);
    }
    Ok(forms)
}

/// A part name beside the workbook part, `folder/<stem>.xml` or, when the package holds a part of
/// that name, `folder/<stem><n>.xml`, and the relationship target that names it from the workbook
/// part.
pub(super) fn new_part_name(
    package: &Package,
    workbook_part: &str,
    stem: &str,
) -> (String, String) {
    let name = available_part_name(package, workbook_part, stem, "xml", &[]);
    let parent = workbook_part.rsplit_once('/').map(|(p, _)| p);
    let target = parent
        .and_then(|p| name.strip_prefix(&format!("{p}/")))
        .unwrap_or(&name)
        .to_owned();
    (name, target)
}
pub(crate) fn available_part_name(
    package: &Package,
    workbook_part: &str,
    stem: &str,
    extension: &str,
    reserved: &[String],
) -> String {
    let folder = workbook_part.rsplit_once('/').map(|(folder, _)| folder);
    let mut suffix = String::new();
    for n in 1u32.. {
        let target = format!("{stem}{suffix}.{extension}");
        let name = match folder {
            Some(folder) => format!("{folder}/{target}"),
            None => target.clone(),
        };
        let overridden = package.content_types().overrides().iter().any(|(part, _)| {
            part.strip_prefix('/')
                .is_some_and(|part| part.eq_ignore_ascii_case(&name))
        });
        if package.part(&name).is_none()
            && !overridden
            && !reserved.iter().any(|n| n.eq_ignore_ascii_case(&name))
        {
            return name;
        }
        suffix = (n + 1).to_string();
    }
    unreachable!("a free name is found before the counter runs out")
}

/// The view manifest's part a save writes for a changed layout.
enum ManifestChange {
    /// The manifest the package holds, at this part, with new content.
    Rewrite { part: String, content: Vec<u8> },
    /// A manifest for a package that holds none.
    Create(NewManifest),
}

/// What a changed skin layout writes: the workbook part's locator names and the manifest.
struct LayoutChanges {
    names: NameChanges,
    manifest: ManifestChange,
}

/// A locator's formula text for `anchor` on the sheet `sheet`: `'<sheet>'!$A$1`, or
/// `'<sheet>'!$A$1:$C$4` for a range, the name quoted with each apostrophe doubled.
fn locator_text(sheet: &SheetName, anchor: GridArea) -> Vec<u16> {
    let apostrophe = u16::from(b'\'');
    let mut units = vec![apostrophe];
    for &unit in sheet.text().utf16_code_units() {
        units.push(unit);
        if unit == apostrophe {
            units.push(unit);
        }
    }
    let cell = |at: CellKey| format!("${}${}", column_letters(at.column()), at.row());
    let (first, last) = (anchor.first(), anchor.last());
    let mut reference = format!("'!{}", cell(first));
    if first != last {
        reference.push(':');
        reference.push_str(&cell(last));
    }
    units.extend(reference.encode_utf16());
    units
}

/// What the save writes for the revision's skin layout; `None` when it is the loaded one. A block
/// the revision removed, or re-created with another anchor or kind, takes its locator out of the
/// workbook part when the part holds it as a locator (a hidden, workbook-scoped name of a
/// locator's form); a block it created puts one in. Refused when the layout changed and the
/// package's manifest was not read.
fn layout_changes(package: &Package, edit: &Edited) -> Result<Option<LayoutChanges>, SaveRefusal> {
    let (base, model) = (edit.base.layout(), edit.model.layout());
    if Arc::ptr_eq(base, model) || base == model {
        return Ok(None);
    }
    let manifest = match &edit.manifest {
        ManifestState::Unread { part } => {
            return Err(SaveRefusal::LayoutNotWritable { part: part.clone() });
        }
        ManifestState::Read { part } => ManifestChange::Rewrite {
            part: part.clone(),
            content: manifest::rewritten(package, part, model)?,
        },
        ManifestState::Absent => ManifestChange::Create(manifest::new_parts(
            manifest::free_number(package),
            manifest::fresh(model),
        )),
    };
    let mut names = NameChanges::default();
    for block in base.blocks() {
        // Only a name of a locator's form is ever a locator: a block whose manifest names any
        // other name (a user's) leaves that name as the source wrote it.
        if model.block(block.id()) != Some(block) && is_locator_form(block.locator().as_str()) {
            names.removed.push(block.locator().as_str().to_owned());
        }
    }
    for block in model.blocks() {
        if base.block(block.id()) != Some(block) {
            let Some(anchor) = block.anchor() else {
                names.added.push((
                    block.locator().as_str().to_owned(),
                    "#REF!".encode_utf16().collect(),
                ));
                continue;
            };
            let sheet = edit
                .model
                .sheet_by_id(anchor.sheet())
                .expect("a created block's anchor lies on a sheet of the model");
            names.added.push((
                block.locator().as_str().to_owned(),
                locator_text(sheet.name(), anchor),
            ));
        }
    }
    Ok(Some(LayoutChanges { names, manifest }))
}

/// The target that names `to` from a relationship of the part `from`, both ZIP item names.
pub(super) fn relative_target(from: &str, to: &str) -> String {
    format!("{}{to}", "../".repeat(from.matches('/').count()))
}

/// The changes an authored save makes to the package for `edit`; none when the edited model's
/// cells and skin layout are the loaded ones and no cache-state write was requested.
pub(super) fn plan(
    package: &Package,
    edit: &Edited,
    views: Option<&super::SaveViews>,
) -> Result<Changes, SaveRefusal> {
    let Edited {
        workbook_part,
        sheets,
        base,
        model,
        values,
        spills,
        structural,
        cache_state,
        ..
    } = edit;
    if !model.same_lineage(base) {
        return Err(SaveRefusal::ForeignModel);
    }
    if let Some(views) = views {
        super::panes::validate(views, model)?;
    }
    let sheets_changed = model.sheets() != base.sheets();
    let original_sheets = sheets;
    let expanded = super::sheets::expanded(package, workbook_part, sheets, model);
    let sheets = &expanded;
    // Rows or columns inserted or deleted: the package's parts moved with them first, and each
    // moved sheet's changes found against the cells its cells moved from.
    // A macro sheet's formulas are not read, so references they hold cannot be moved: a
    // structural edit of a workbook with a macro sheet is refused (dc-xuw).
    if structural.is_some()
        && let Some(sheet) = sheets.iter().find(|sheet| sheet.is_macro_sheet())
    {
        return Err(SaveRefusal::MacroSheetChanged {
            part: sheet.part().name().to_owned(),
        });
    }
    if structural.is_some()
        && let Some(sheet) = sheets.iter().find(|sheet| sheet.preserved_kind().is_some())
    {
        return Err(SaveRefusal::PreservedSheetChanged {
            part: sheet.part().name().to_owned(),
            kind: sheet.preserved_kind().unwrap_or_default(),
        });
    }
    let mut moved = match structural {
        Some(structural) => Some(structural::moved(
            package,
            original_sheets,
            base,
            structural,
        )?),
        None => None,
    };
    let moves: Vec<Option<SheetMoves>> = (0..sheets.len())
        .map(|index| {
            moved
                .as_mut()
                .and_then(|moved| moved.moves.get_mut(index).and_then(Option::take))
        })
        .collect();
    let mut changes: Vec<SheetChanges> = sheets
        .iter()
        .zip(&moves)
        .map(|(sheet, moves)| match moves {
            Some(moves) => structural::sheet_changes(base, model, sheet.id(), moves),
            None => sheet_changes(base, model, sheet.id()),
        })
        .collect();
    // Fresh pivot outputs come from the engine's pinned result view, never model cell inputs.
    let mut pivot_output_cells: Vec<BTreeSet<(u32, u32)>> = vec![BTreeSet::new(); sheets.len()];
    for (at, value) in edit.pivot_outputs.iter() {
        let index = sheets
            .iter()
            .position(|sheet| sheet.id() == at.sheet())
            .ok_or_else(|| super::pivot::missing_output("The output worksheet is unavailable"))?;
        let input = match value.core_ref() {
            calc_doc_model::value::CoreRef::Number(n) => Some(CellInput::Number(
                calc_doc_model::FiniteNumber::new(n)
                    .ok_or_else(|| super::pivot::missing_output("A pivot number is not finite"))?,
            )),
            calc_doc_model::value::CoreRef::Text(t) => Some(CellInput::Text(
                calc_doc_model::CellText::new(t.clone()).ok_or_else(|| {
                    super::pivot::missing_output("A pivot label exceeds the cell text limit")
                })?,
            )),
            calc_doc_model::value::CoreRef::Logical(b) => Some(CellInput::Logical(b)),
            calc_doc_model::value::CoreRef::Error(e) => Some(CellInput::Error(e)),
            calc_doc_model::value::CoreRef::Empty => None,
            _ => {
                return Err(super::pivot::missing_output(
                    "A pivot output is not a scalar cell value",
                ));
            }
        };
        changes[index].insert((at.row(), at.column()), input);
        pivot_output_cells[index].insert((at.row(), at.column()));
    }
    // (The empty pivot, a frame with no field, has no output.)
    for table in model.definitions().pivots().tables().filter(|t| {
        !(t.row_fields.is_empty()
            && t.column_fields.is_empty()
            && t.page_fields.is_empty()
            && t.data_fields.is_empty())
    }) {
        if model
            .definitions()
            .pivots()
            .caches
            .caches
            .get(&table.cache_id)
            .is_some_and(|c| c.snapshot.is_some())
        {
            if (table.area.first().row()..=table.area.last().row()).any(|row| {
                (table.area.first().column()..=table.area.last().column()).any(|column| {
                    !edit.pivot_outputs.contains_key(
                        &calc_doc_model::value::CellKey::new(table.area.sheet(), row, column)
                            .expect("pivot bounds"),
                    )
                })
            }) {
                return Err(super::pivot::missing_output(
                    "Refreshed pivot output is missing from the pinned engine view",
                ));
            }
        }
    }
    // The cell formats edits gave each sheet's cells (dc-09v), by (row, column).
    let formats: Vec<BTreeMap<(u32, u32), u32>> = sheets
        .iter()
        .map(|sheet| {
            model
                .definitions()
                .cell_formats()
                .filter(|(at, _)| at.sheet() == sheet.id())
                .map(|(at, format)| ((at.row(), at.column()), format))
                .collect()
        })
        .collect();
    let cells_changed = cache_state.is_some()
        || !changes.iter().all(SheetChanges::is_empty)
        || moved.is_some()
        || formats.iter().any(|formats| !formats.is_empty())
        || model.definitions().styles().runs.iter().next().is_some();
    let mut layout = layout_changes(package, edit)?;
    let mut out = Changes::default();
    // A defined name's formula an edit changed (rows or columns inserted or deleted, cells moved).
    let names_changed = base.definitions().names() != model.definitions().names();
    let validation_changed = base
        .definitions()
        .all_validations()
        .ne(model.definitions().all_validations());
    let page_setup_changed = base
        .definitions()
        .page_setups()
        .ne(model.definitions().page_setups());
    let charts_changed = base
        .definitions()
        .all_charts()
        .ne(model.definitions().all_charts());
    let pivots_changed = base.definitions().pivots() != model.definitions().pivots();
    let links_changed = base
        .definitions()
        .all_review_links()
        .ne(model.definitions().all_review_links());
    let review_changed = base
        .definitions()
        .all_review_notes()
        .ne(model.definitions().all_review_notes());
    let conditional_changed = base
        .definitions()
        .all_conditional_rules()
        .ne(model.definitions().all_conditional_rules());
    let tables_changed = base.definitions().tables() != model.definitions().tables();
    let filters_changed = base
        .definitions()
        .auto_filters()
        .ne(model.definitions().auto_filters());
    let visibility_changed = base
        .definitions()
        .hidden_rows_all()
        .ne(model.definitions().hidden_rows_all());
    let panes_changed = base
        .definitions()
        .pane_layouts()
        .ne(model.definitions().pane_layouts());
    let dimensions_changed =
        base.definitions().axis_layouts() != model.definitions().axis_layouts();
    // The iterative-calculation settings an edit changed (dc-y2o.2), written to the `calcPr`.
    let iteration = Some(model.definitions().iteration())
        .filter(|settings| *settings != base.definitions().iteration());
    if base.definitions().styles() == model.definitions().styles()
        && !cells_changed
        && layout.is_none()
        && !names_changed
        && !conditional_changed
        && !review_changed
        && !charts_changed
        && !pivots_changed
        && !page_setup_changed
        && !links_changed
        && !validation_changed
        && views.is_none()
        && edit.custom_xml.is_empty()
        && !panes_changed
        && !dimensions_changed
        && !tables_changed
        && !filters_changed
        && !visibility_changed
        && iteration.is_none()
        && !sheets_changed
    {
        return Ok(out);
    }
    let mut chain = None;
    let mut new_strings = None;
    let mut metadata = array_metadata::MetadataPlan::default();
    // Whether an array formula is written without a value, which Excel does not calculate when it
    // opens the package (measured, tests/fixtures/d1/excel-open.json: array-no-value).
    let mut array_without_value = false;
    if cells_changed {
        let mut forms = Vec::with_capacity(sheets.len());
        for (index, ((sheet, changes), moves)) in
            sheets.iter().zip(&changes).zip(&moves).enumerate()
        {
            if !changes.is_empty() {
                check_formulas(sheet.part().name(), changes, &pivot_output_cells[index])?;
                if !tables_changed {
                    check_tables(package, sheet.part().name(), changes, moves.as_ref())?;
                }
            }
            forms.push(file_forms(
                sheet.part().name(),
                changes,
                model.definitions().names(),
                &model.definitions().external_file_names(),
            )?);
        }
        // The spills the save writes, by sheet: each array formula's area, by anchor, as
        // (top, left, bottom, right). An anchor written as an array formula (one of the model's
        // array formulas, or an authored dynamic-array formula) takes its area as its `ref`; the
        // spill of any other anchor is not written, and neither is that anchor's value.
        let mut sheet_spills: Vec<BTreeMap<(u32, u32), (u32, u32, u32, u32)>> =
            vec![BTreeMap::new(); sheets.len()];
        let mut unwritten: BTreeSet<CellKey> = BTreeSet::new();
        for (&anchor, area) in spills.iter() {
            let Some(position) = sheets.iter().position(|sheet| sheet.id() == anchor.sheet())
            else {
                return Err(SaveRefusal::ForeignModel);
            };
            let array = model.definitions().array(anchor).is_some()
                || matches!(model.cell(anchor),Some(CellInput::Formula(text)) if text.form() == calc_doc_model::FormulaForm::Dynamic)
                || forms[position]
                    .get(&(anchor.row(), anchor.column()))
                    .is_some_and(|form| form.dynamic);
            let (first, last) = (area.first(), area.last());
            if area.sheet() != anchor.sheet() || first != anchor {
                return Err(SaveRefusal::ForeignModel);
            }
            if array {
                sheet_spills[position].insert(
                    (anchor.row(), anchor.column()),
                    (first.row(), first.column(), last.row(), last.column()),
                );
            } else if area.cell_count() > 1 {
                unwritten.insert(anchor);
            }
        }
        // The caller's values, by sheet and (row, column). A value for a cell without a formula
        // is refused unless the cell lies in a written spill beside its anchor.
        let mut sheet_values: Vec<Option<BTreeMap<(u32, u32), CachedValue>>> =
            vec![values.as_ref().map(|_| BTreeMap::new()); sheets.len()];
        let mut errors: Vec<WorksheetErrorCode> = Vec::new();
        let mut images: Vec<calc_doc_model::value::ImageValue> = Vec::new();
        // A pivot output cell that holds a newer error is written with value metadata too.
        for (changes, cells) in changes.iter().zip(&pivot_output_cells) {
            for at in cells {
                if let Some(Some(CellInput::Error(code))) = changes.get(at)
                    && array_metadata::rich_error_type(*code).is_some()
                    && !errors.contains(code)
                {
                    errors.push(*code);
                }
            }
        }
        if let Some(values) = values {
            for (&at, value) in values.iter() {
                let position = sheets.iter().position(|sheet| sheet.id() == at.sheet());
                let Some(position) = position else {
                    return Err(SaveRefusal::ForeignModel);
                };
                let in_spill = || {
                    model.cell(at).is_none()
                        && sheet_spills[position].iter().any(
                            |(&anchor, &(top, left, bottom, right))| {
                                anchor != (at.row(), at.column())
                                    && (top..=bottom).contains(&at.row())
                                    && (left..=right).contains(&at.column())
                            },
                        )
                };
                if !matches!(model.cell(at), Some(CellInput::Formula(_)))
                    && !in_spill()
                    && !(model.cell(at).is_none()
                        && model.definitions().data_table_containing(at).is_some())
                {
                    return Err(SaveRefusal::ValueWithoutFormula {
                        part: sheets[position].part().name().to_owned(),
                        row: at.row(),
                        column: at.column(),
                    });
                }
                if unwritten.contains(&at) {
                    continue;
                }
                if let CachedValue::Error(code) = value
                    && !errors.contains(code)
                {
                    errors.push(*code);
                }
                if let CachedValue::Image(image) = value
                    && !images.contains(image)
                {
                    images.push(image.clone());
                }
                if let Some(map) = sheet_values[position].as_mut() {
                    map.insert((at.row(), at.column()), value.clone());
                }
            }
        }
        // The cell metadata an authored dynamic array names, and the value metadata of each
        // newer error a value holds, found or made in the metadata part.
        // A changed formula is written as a dynamic array when its file form is one or the model
        // anchors a dynamic array at it, unless the model anchors a legacy array there.
        let dynamic = sheets.iter().zip(&forms).any(|(sheet, forms)| {
            forms.iter().any(|(&(row, column), _)| {
                let array = CellKey::new(sheet.id(), row, column)
                    .ok()
                    .and_then(|at| model.definitions().array(at));
                match array.map(|array| array.kind()) {
                    Some(calc_doc_model::ArrayKind::Legacy) => false,
                    Some(calc_doc_model::ArrayKind::Dynamic { .. }) => true,
                    None => CellKey::new(sheet.id(),row,column).ok().is_some_and(|at|
                        matches!(model.cell(at),Some(CellInput::Formula(text)) if text.form() == calc_doc_model::FormulaForm::Dynamic)),
                }
            })
        });
        let written = |position: usize, at: (u32, u32)| {
            sheet_values[position]
                .as_ref()
                .and_then(|values| values.get(&at))
                .is_some_and(|value| {
                    !matches!(value, CachedValue::Error(code)
                        if !crate::edit::error_constant_writable(*code)
                            && array_metadata::rich_error_type(*code).is_none())
                })
        };
        array_without_value = model.definitions().arrays().any(|array| {
            let anchor = array.anchor();
            sheets
                .iter()
                .position(|sheet| sheet.id() == anchor.sheet())
                .is_some_and(|position| !written(position, (anchor.row(), anchor.column())))
        });
        metadata = array_metadata::plan(package, workbook_part, dynamic, &errors, &images)?;
        for (part, content) in std::mem::take(&mut metadata.rewritten) {
            out.rewritten.insert(part, content);
        }
        let dynamic_cm = metadata.cm;
        // The areas the loaded array formulas of each sheet fill, whose cells beside the anchor
        // hold the arrays' cached results (or, for an array whose anchor changed, belong to no
        // formula any more).
        // On a moved sheet, each area where its cells moved, and none for an array whose anchor
        // was deleted.
        let cleared: Vec<Vec<(u32, u32, u32, u32)>> = sheets
            .iter()
            .zip(&moves)
            .map(|(sheet, moves)| {
                base.definitions()
                    .arrays()
                    .filter(|array| array.anchor().sheet() == sheet.id())
                    .filter_map(|array| {
                        let (first, last) = (array.area().first(), array.area().last());
                        let area = (first.row(), first.column(), last.row(), last.column());
                        match moves {
                            Some(moves) => {
                                moves.forward((area.0, area.1))?;
                                moves.area(area)
                            }
                            None => Some(area),
                        }
                    })
                    .filter(|(top, left, bottom, right)| (top, left) != (bottom, right))
                    .collect()
            })
            .collect();
        // A sheet is rewritten when one of its cells changed, one of its formulas has a cached
        // result, or an array formula fills cells of it: a save after an edit no longer trusts
        // a cached result.
        let rewritten: Vec<bool> = sheets
            .iter()
            .enumerate()
            .zip(&changes)
            .zip(&cleared)
            .map(|(((index, sheet), changes), cleared)| {
                let moved_part = moved
                    .as_ref()
                    .is_some_and(|moved| moved.sheets.get(index).is_some_and(Option::is_some));
                let formulas = model
                    .sheet_cells(sheet.id())
                    .into_iter()
                    .flatten()
                    .any(|(_, input)| matches!(input, CellInput::Formula(_)));
                !changes.is_empty()
                    || !formats[index].is_empty()
                    || model
                        .definitions()
                        .styles()
                        .runs
                        .iter()
                        .any(|r| r.area.sheet() == sheet.id())
                    || moved_part
                    || moves[index].is_some()
                    || !cleared.is_empty()
                    || sheet
                        .formulas()
                        .any(|(_, _, record)| record.cache().is_some())
                    || (values.is_some() && formulas)
            })
            .collect();
        for (index, sheet) in sheets.iter().enumerate() {
            let name = sheet.part().name();
            let shared = sheets.iter().enumerate().any(|(other, loaded)| {
                other != index && loaded.part().name().eq_ignore_ascii_case(name)
            });
            if rewritten[index] && shared {
                return Err(SaveRefusal::SharedSheetPart {
                    part: name.to_owned(),
                });
            }
            if rewritten[index] && sheet.is_macro_sheet() {
                return Err(SaveRefusal::MacroSheetChanged {
                    part: name.to_owned(),
                });
            }
            if rewritten[index]
                && let Some(kind) = sheet.preserved_kind()
            {
                return Err(SaveRefusal::PreservedSheetChanged {
                    part: name.to_owned(),
                    kind,
                });
            }
        }
        let strings_part = load::workbook_part_of(package, workbook_part, SHARED_STRINGS)
            .map_err(|error| load_refusal(workbook_part, error))?;
        let mut strings = SharedStrings::read(package, strings_part.as_deref())?;
        if let Some(moved) = &moved {
            strings.removed += moved.strings_removed;
        }
        let no_groups = BTreeSet::new();
        let dirty_caches: Vec<BTreeSet<(u32, u32)>> = sheets
            .iter()
            .map(|sheet| {
                cache_state
                    .iter()
                    .flat_map(|state| state.dirty.iter())
                    .filter(|at| at.sheet() == sheet.id())
                    .map(|at| (at.row(), at.column()))
                    .collect()
            })
            .collect();
        for (index, ((((((sheet, changes), rewrite), cleared), forms), sheet_values), spills)) in
            sheets
                .iter()
                .zip(&changes)
                .zip(&rewritten)
                .zip(&cleared)
                .zip(&forms)
                .zip(&sheet_values)
                .zip(&sheet_spills)
                .enumerate()
        {
            if !rewrite {
                continue;
            }
            let name = sheet.part().name();
            let moved_xml = moved
                .as_mut()
                .and_then(|moved| moved.sheets.get_mut(index).and_then(Option::take));
            let xml = match moved_xml {
                Some(xml) => xml,
                None => super::sheets::read_sheet(package, name)?,
            };
            let style_plan =
                super::styles::sheet_plan(&xml, sheet.id(), &model.definitions().styles().runs)?;
            let job = Job {
                style_plan: style_plan.as_ref(),
                changes,
                cleared,
                forms,
                dynamic_cm,
                values: sheet_values.as_ref(),
                dirty_caches: &dirty_caches[index],
                cache_state: cache_state.is_some(),
                spills,
                error_vm: &metadata.vm,
                image_vm: &metadata.image_vm,
                loaded: sheet,
                model,
                moves: moves[index].as_ref(),
                cut_groups: moved.as_ref().map_or(&no_groups, |moved| {
                    moved.unshared.get(index).unwrap_or(&no_groups)
                }),
                formats: &formats[index],
            };
            let rewritten = sheet::rewrite(&xml, &job, &mut strings)?;
            out.rewritten.insert(name.to_owned(), rewritten.bytes);
            out.caches_dropped += rewritten.caches_dropped;
            out.values_written += rewritten.values_written;
            out.values_left_to_excel += rewritten.values_left_to_excel;
            if changes.is_empty() && formats[index].is_empty() {
                out.sheets_rewritten_for_caches += 1;
            }
        }

        // The calculation chain lists formula cells; when the set of them changed, the chain no
        // longer lists exactly the workbook's formulas, so it is left out, with its relationship
        // and its content type override, rather than written stale. What Excel does with a stale
        // or a missing chain is measured (tests/fixtures/d1/excel-open.json), not here.
        let formulas_changed = sheets.iter().zip(&changes).any(|(sheet, changes)| {
            changes.iter().any(|(&(row, column), input)| {
                let key =
                    CellKey::new(sheet.id(), row, column).expect("a changed cell is in the grid");
                let was = matches!(base.cell(key), Some(CellInput::Formula(_)));
                let is = matches!(input, Some(CellInput::Formula(_)));
                was != is
            })
        });
        // The chain also marks array formulas' anchors, so it goes when the arrays changed too.
        let arrays_changed = base.definitions().arrays().ne(model.definitions().arrays());
        if formulas_changed || arrays_changed || moved.is_some() {
            chain = load::workbook_part_of(package, workbook_part, CALC_CHAIN)
                .map_err(|error| load_refusal(workbook_part, error))?;
        }

        if let Some(part) = strings.part()
            && strings.changed()
        {
            let xml = read_part(package, part)?;
            out.rewritten
                .insert(part.to_owned(), strings.rewrite(&xml)?);
        }
        new_strings = (strings.part().is_none() && strings.appended() > 0)
            .then(|| new_part_name(package, workbook_part, "sharedStrings"));
        if let Some((name, _)) = &new_strings {
            out.added.push((name.clone(), strings.new_part()));
        }
    }

    // fullCalcOnLoad: a save writes every formula cell's value or leaves it without one. Excel
    // calculates an ordinary formula without a value when it opens the package, with the
    // calculation chain or without it, but not an array formula without one (measured,
    // tests/fixtures/d1/excel-open.json: no-value, no-value-no-chain, no-values-no-chain and
    // array-no-value). So a save asks for a full calculation only when it writes an array formula
    // without a value, or when the workbook keeps cached results it did not recalculate, a data
    // table's; a save without the caller's values asks for one only for the same reasons.
    let data_tables = model.definitions().unloaded_cells().any(|(at, why)| {
        matches!(why, calc_doc_model::UnloadedCell::DataTable)
            && !values
                .as_ref()
                .is_some_and(|values| values.contains_key(&at))
    });
    let full_calc = cells_changed && (data_tables || array_without_value);
    let mut names = layout
        .as_mut()
        .map(|layout| std::mem::take(&mut layout.names))
        .unwrap_or_default();
    // Each defined name whose formula the edits rewrote (rows or columns inserted or deleted,
    // cells moved by a cut, dc-gf0), and the other parts the edits' passes moved.
    if names_changed {
        let base_names: Vec<_> = base.definitions().name_table().nodes().collect();
        let mut by_key =
            std::collections::HashMap::<calc_doc_model::value::NameKey, Vec<usize>>::new();
        let mut by_id = std::collections::HashMap::new();
        let mut targets = Vec::new();
        for (index, node) in base_names.iter().enumerate() {
            let group = by_key.entry(node.key().clone()).or_default();
            let occurrence = group.len();
            group.push(index);
            by_id.insert(node.origin(), index);
            let sheet = match node.definition().scope {
                NameScope::Workbook => Some(None),
                NameScope::Sheet(id) => model
                    .sheets()
                    .iter()
                    .position(|sheet| sheet.id() == id)
                    .map(|index| Some(index as u32)),
            };
            targets.push(sheet.map(|sheet| parts::NameTarget {
                name: node.definition().name.clone(),
                sheet,
                occurrence,
            }));
        }
        let mut used = vec![false; base_names.len()];
        let mut order = Vec::new();
        for node in model.definitions().name_table().nodes() {
            let now = node.definition();
            let loaded = by_id
                .get(&node.origin())
                .copied()
                .filter(|index| !used[*index]);
            let sheet = match now.scope {
                NameScope::Workbook => None,
                NameScope::Sheet(id) => {
                    match model.sheets().iter().position(|entry| entry.id() == id) {
                        Some(position) => Some(position as u32),
                        None => continue,
                    }
                }
            };
            if let Some(index) = loaded {
                used[index] = true;
                let before = base_names[index].definition();
                let Some(target) = targets[index].clone() else {
                    continue;
                };
                order.push(parts::NamePlacement::Source(target.clone()));
                if before == now {
                    continue;
                }
                let attributes_changed = before.name != now.name
                    || before.scope != now.scope
                    || before.hidden != now.hidden
                    || before.comment != now.comment
                    || before.function != now.function
                    || before.vb_procedure != now.vb_procedure
                    || before.xlm != now.xlm;
                names.replaced.push(parts::NameWrite {
                    target,
                    sheet,
                    definition: now.clone(),
                    attributes_changed,
                });
            } else {
                order.push(parts::NamePlacement::Defined(names.defined.len()));
                names.defined.push(parts::NameWrite {
                    sheet,
                    target: parts::NameTarget {
                        name: now.name.clone(),
                        sheet,
                        occurrence: 0,
                    },
                    definition: now.clone(),
                    attributes_changed: true,
                });
            }
        }
        names.order = Some(order);
        names.record_removed.extend(
            targets
                .into_iter()
                .enumerate()
                .filter_map(|(index, target)| (!used[index]).then_some(target).flatten()),
        );
    }
    if let Some(moved) = moved.as_mut() {
        for (part, content) in std::mem::take(&mut moved.parts) {
            out.rewritten.insert(part, content);
        }
    }
    if full_calc
        || cache_state.is_some()
        || iteration.is_some()
        || !names.is_empty()
        || sheets_changed
    {
        let original = read_part(package, workbook_part)?;
        let workbook = if sheets_changed {
            original.with_bytes(super::sheets::workbook(&original, base, model, sheets)?)
        } else {
            original
        };
        out.rewritten.insert(
            workbook_part.clone(),
            parts::workbook(
                &workbook,
                full_calc,
                iteration,
                &names,
                cache_state.as_ref(),
            )?,
        );
    }

    // Relationships the workbook part gains, as (type, target), and content type overrides the
    // package gains, as (part, content type).
    let mut relationships_added: Vec<(&str, String)> = Vec::new();
    let mut overrides: Vec<(String, &str)> = Vec::new();
    if base.definitions().styles() != model.definitions().styles() {
        let source = load::workbook_part_of(package, workbook_part, super::styles::RELATIONSHIP)
            .map_err(|error| load_refusal(workbook_part, error))?;
        if let Some(name) = source {
            let xml = read_part(package, &name)?;
            out.rewritten.insert(
                name,
                super::styles::rewrite(
                    &xml,
                    base.definitions().styles(),
                    model.definitions().styles(),
                )?,
            );
        } else {
            let (name, target) = new_part_name(package, workbook_part, "styles");
            let xml = crate::xml::XmlPart::new(
                name.clone(),
                format!(
                    "<styleSheet xmlns=\"{}\"></styleSheet>",
                    load::MAIN_NAMESPACE
                )
                .into_bytes(),
                Default::default(),
            );
            let bytes = super::styles::rewrite(
                &xml,
                base.definitions().styles(),
                model.definitions().styles(),
            )?;
            relationships_added.push((super::styles::RELATIONSHIP, target));
            overrides.push((name.clone(), super::styles::CONTENT_TYPE));
            out.added.push((name, bytes));
        }
    }
    if let Some((name, target)) = &new_strings {
        relationships_added.push((SHARED_STRINGS, target.clone()));
        overrides.push((name.clone(), strings::CONTENT_TYPE));
    }
    for part in metadata.added {
        relationships_added.push((part.relationship, part.target));
        overrides.push((part.name.clone(), part.content_type));
        out.added.push((part.name, part.content));
    }
    for (name, content, content_type) in metadata.unrelated {
        overrides.push((name.clone(), content_type));
        out.added.push((name, content));
    }
    match layout.map(|layout| layout.manifest) {
        Some(ManifestChange::Rewrite { part, content }) => {
            out.rewritten.insert(part, content);
        }
        Some(ManifestChange::Create(parts)) => {
            let defaults = package.content_types().defaults();
            let has_default = |extension: &str, content_type: &str| {
                defaults
                    .iter()
                    .any(|(e, t)| e.eq_ignore_ascii_case(extension) && t == content_type)
            };
            relationships_added.push((CUSTOM_XML, relative_target(workbook_part, &parts.item.0)));
            if !has_default("xml", manifest::ITEM_CONTENT_TYPE) {
                overrides.push((parts.item.0.clone(), manifest::ITEM_CONTENT_TYPE));
            }
            overrides.push((parts.props.0.clone(), manifest::PROPS_CONTENT_TYPE));
            if !has_default("rels", manifest::RELATIONSHIPS_CONTENT_TYPE) {
                overrides.push((
                    parts.relationships.0.clone(),
                    manifest::RELATIONSHIPS_CONTENT_TYPE,
                ));
            }
            out.added
                .extend([parts.item, parts.props, parts.relationships]);
        }
        None => {}
    }
    // Custom XML items an application keeps: the content of the package's own item of the
    // namespace, or a new item with its item properties and relationship.
    let mut used_numbers: Vec<u32> = out
        .added
        .iter()
        .filter_map(|(name, _)| {
            name.strip_prefix("customXml/item")
                .and_then(|rest| rest.strip_suffix(".xml"))
                .and_then(|n| n.parse().ok())
        })
        .collect();
    for item in &edit.custom_xml {
        if let Some(part) = crate::custom_xml::parts_in(package, &item.namespace)
            .into_iter()
            .next()
        {
            out.rewritten.insert(part, item.content.clone());
            continue;
        }
        let n = manifest::free_number_beyond(package, &used_numbers);
        used_numbers.push(n);
        let parts = manifest::new_parts_for(
            n,
            item.content.clone(),
            &item.namespace,
            &crate::custom_xml::item_id(&item.namespace),
        );
        let defaults = package.content_types().defaults();
        let has_default = |extension: &str, content_type: &str| {
            defaults
                .iter()
                .any(|(e, t)| e.eq_ignore_ascii_case(extension) && t == content_type)
        };
        relationships_added.push((CUSTOM_XML, relative_target(workbook_part, &parts.item.0)));
        if !has_default("xml", manifest::ITEM_CONTENT_TYPE) {
            overrides.push((parts.item.0.clone(), manifest::ITEM_CONTENT_TYPE));
        }
        overrides.push((parts.props.0.clone(), manifest::PROPS_CONTENT_TYPE));
        if !has_default("rels", manifest::RELATIONSHIPS_CONTENT_TYPE) {
            overrides.push((
                parts.relationships.0.clone(),
                manifest::RELATIONSHIPS_CONTENT_TYPE,
            ));
        }
        out.added
            .extend([parts.item, parts.props, parts.relationships]);
    }

    if chain.is_some() || !relationships_added.is_empty() {
        let relationships = package
            .relationships_of(Some(workbook_part))
            .ok_or_else(|| SaveRefusal::InvalidPart {
                part: workbook_part.clone(),
                detail: "the workbook part has no relationships part".to_owned(),
            })?;
        let mut ids: Vec<String> = Vec::new();
        for _ in &relationships_added {
            let id = (1u32..)
                .map(|n| format!("rId{n}"))
                .find(|id| {
                    relationships.items().iter().all(|item| &item.id != id) && !ids.contains(id)
                })
                .expect("a free relationship ID");
            ids.push(id);
        }
        let add: Vec<NewRelationship<'_>> = relationships_added
            .iter()
            .zip(&ids)
            .map(|((kind, target), id)| NewRelationship { id, kind, target })
            .collect();
        let xml = read_part(package, relationships.part())?;
        let remove = chain.is_some().then_some(CALC_CHAIN);
        out.rewritten.insert(
            relationships.part().to_owned(),
            parts::relationships(&xml, remove, &add)?,
        );
    }
    if chain.is_some() || !overrides.is_empty() {
        let xml = package
            .content_types_xml()
            .map_err(|error| SaveRefusal::Part {
                part: crate::opc::CONTENT_TYPES_ENTRY.to_owned(),
                error,
            })?;
        let add: Vec<(&str, &str)> = overrides
            .iter()
            .map(|(part, content_type)| (part.as_str(), *content_type))
            .collect();
        out.content_types = Some(parts::content_types(&xml, chain.as_deref(), &add)?);
    }
    if let Some(chain) = chain
        && package.part(&chain).is_some()
    {
        out.removed.push(chain);
    }
    if dimensions_changed
        || visibility_changed
        || model.definitions().styles().runs.iter().next().is_some()
    {
        for sheet in sheets.iter() {
            let changed = base
                .definitions()
                .hidden_rows(sheet.id(), 1, 1048576)
                .ne(model.definitions().hidden_rows(sheet.id(), 1, 1048576))
                || model
                    .definitions()
                    .styles()
                    .runs
                    .iter()
                    .any(|r| r.area.sheet() == sheet.id())
                || [calc_doc_model::Axis::Rows, calc_doc_model::Axis::Columns]
                    .into_iter()
                    .any(|axis| {
                        base.definitions()
                            .axis_layouts()
                            .bands(sheet.id(), axis)
                            .ne(model.definitions().axis_layouts().bands(sheet.id(), axis))
                    });
            let outline_settings_changed =
                [calc_doc_model::Axis::Rows, calc_doc_model::Axis::Columns]
                    .into_iter()
                    .any(|axis| {
                        base.definitions()
                            .axis_layouts()
                            .summary_after(sheet.id(), axis)
                            != model
                                .definitions()
                                .axis_layouts()
                                .summary_after(sheet.id(), axis)
                    });
            if !changed && !outline_settings_changed {
                continue;
            }
            if sheet.geometry().outline_unsupported {
                return Err(SaveRefusal::OutlineNotModelled {
                    part: sheet.part().name().to_owned(),
                    owner: "dc-971p.39",
                });
            }
            if sheets.iter().any(|other| {
                other.id() != sheet.id()
                    && other
                        .part()
                        .name()
                        .eq_ignore_ascii_case(sheet.part().name())
            }) {
                return Err(SaveRefusal::SharedSheetPart {
                    part: sheet.part().name().to_owned(),
                });
            }
            if sheet.is_macro_sheet() {
                return Err(SaveRefusal::MacroSheetChanged {
                    part: sheet.part().name().to_owned(),
                });
            }
            if let Some(kind) = sheet.preserved_kind() {
                return Err(SaveRefusal::PreservedSheetChanged {
                    part: sheet.part().name().to_owned(),
                    kind,
                });
            }
            let original = super::sheets::read_sheet(package, sheet.part().name())?;
            let current = out
                .rewritten
                .remove(sheet.part().name())
                .map(|bytes| original.with_bytes(bytes));
            let xml = current.as_ref().unwrap_or(&original);
            let style_plan =
                super::styles::sheet_plan(xml, sheet.id(), &model.definitions().styles().runs)?;
            let bytes = super::geometry::rewrite(
                xml,
                sheet.id(),
                &model.definitions().visible_axis_layouts(),
                style_plan.as_ref(),
            )?;
            let bytes = super::outline::rewrite(
                &xml.with_bytes(bytes),
                sheet.id(),
                model.definitions().axis_layouts(),
            )?;
            out.rewritten.insert(sheet.part().name().to_owned(), bytes);
        }
    }
    if filters_changed || dimensions_changed {
        for sheet in sheets.iter().filter(|s| {
            !model
                .definitions()
                .auto_filter(s.id())
                .is_some_and(|f| f.unsupported)
                && (base.definitions().auto_filter(s.id())
                    != model.definitions().auto_filter(s.id())
                    || (model.definitions().auto_filter(s.id()).is_some()
                        && base.definitions().axis_layouts() != model.definitions().axis_layouts()))
        }) {
            let original = super::sheets::read_sheet(package, sheet.part().name())?;
            let current = out
                .rewritten
                .remove(sheet.part().name())
                .map(|bytes| original.with_bytes(bytes));
            let xml = current.as_ref().unwrap_or(&original);
            let bytes = super::filter::rewrite(
                xml,
                model.definitions().auto_filter(sheet.id()),
                model.definitions(),
            )?;
            out.rewritten.insert(sheet.part().name().to_owned(), bytes);
        }
    }
    if panes_changed || views.is_some() {
        for sheet in sheets
            .iter()
            .filter(|s| model.sheet_by_id(s.id()).is_some())
        {
            let changed = base.definitions().pane_layout(sheet.id())
                != model.definitions().pane_layout(sheet.id());
            let override_view = views.and_then(|v| v.sheets.get(&sheet.id())).copied();
            let selected = views
                .and_then(|v| v.active_sheet)
                .map(|active| active == sheet.id());
            if !changed && override_view.is_none() && selected.is_none() {
                continue;
            }
            if (sheet.is_macro_sheet() || sheet.preserved_kind().is_some())
                && !changed
                && override_view.is_none()
            {
                continue;
            }
            if sheets.iter().any(|other| {
                other.id() != sheet.id()
                    && other
                        .part()
                        .name()
                        .eq_ignore_ascii_case(sheet.part().name())
            }) {
                return Err(SaveRefusal::SharedSheetPart {
                    part: sheet.part().name().to_owned(),
                });
            }
            if sheet.is_macro_sheet() {
                return Err(SaveRefusal::MacroSheetChanged {
                    part: sheet.part().name().to_owned(),
                });
            }
            if let Some(kind) = sheet.preserved_kind() {
                return Err(SaveRefusal::PreservedSheetChanged {
                    part: sheet.part().name().to_owned(),
                    kind,
                });
            }

            let original = super::sheets::read_sheet(package, sheet.part().name())?;
            let current = out
                .rewritten
                .remove(sheet.part().name())
                .map(|bytes| original.with_bytes(bytes));
            let xml = current.as_ref().unwrap_or(&original);
            let view = if let Some(view) = override_view {
                Some(view)
            } else if changed {
                Some(
                    crate::load::sheet::view_from_xml(xml)
                        .map_err(|e| SaveRefusal::WindowView {
                            detail: e.to_string(),
                            owner: "dc-971p.16",
                        })?
                        .with_pane_layout(model.definitions().pane_layout(sheet.id())),
                )
            } else {
                None
            };
            let bytes = super::panes::rewrite(xml, view, selected)?;
            out.rewritten.insert(sheet.part().name().to_owned(), bytes);
        }
        if let Some(active) = views.and_then(|v| v.active_sheet) {
            let original = super::strings::read_part(package, workbook_part)?;
            let current = out
                .rewritten
                .remove(workbook_part)
                .map(|bytes| original.with_bytes(bytes));
            let xml = current.as_ref().unwrap_or(&original);
            let index = model
                .sheets()
                .iter()
                .position(|s| s.id() == active)
                .expect("validated sheet");
            out.rewritten.insert(
                workbook_part.to_owned(),
                super::panes::workbook(xml, index)?,
            );
        }
    }
    if conditional_changed {
        for sheet in sheets.iter().filter(|s| {
            base.definitions().conditional_rules(s.id())
                != model.definitions().conditional_rules(s.id())
        }) {
            let original = super::sheets::read_sheet(package, sheet.part().name())?;
            let current = out
                .rewritten
                .remove(sheet.part().name())
                .map(|b| original.with_bytes(b));
            let bytes = super::conditional::rewrite(
                current.as_ref().unwrap_or(&original),
                model.definitions().conditional_rules(sheet.id()),
            )?;
            out.rewritten.insert(sheet.part().name().to_owned(), bytes);
        }
    }
    if validation_changed {
        for sheet in sheets.iter().filter(|s| {
            base.definitions().validations(s.id()) != model.definitions().validations(s.id())
        }) {
            let original = super::sheets::read_sheet(package, sheet.part().name())?;
            let current = out
                .rewritten
                .remove(sheet.part().name())
                .map(|b| original.with_bytes(b));
            let bytes = super::validation::rewrite(
                current.as_ref().unwrap_or(&original),
                model.definitions().validations(sheet.id()),
            )?;
            out.rewritten.insert(sheet.part().name().to_owned(), bytes);
        }
    }
    if tables_changed || dimensions_changed {
        super::tables::finish(package, workbook_part, &sheets, base, model, &mut out)?;
    }
    if links_changed || structural.is_some() {
        for sheet in sheets.iter() {
            // Pure physical axis movement already preserves every original tag through the
            // structural writer. Only a different authored placement needs the sort rewrite.
            let mut natural = base.definitions().review_links(sheet.id()).to_vec();
            for edit in structural.iter().flat_map(|s| s.edits()) {
                natural = natural
                    .into_iter()
                    .filter_map(|mut l| {
                        l.area = calc_doc_model::moved_area(*edit, l.area)?;
                        Some(l)
                    })
                    .collect();
            }
            if natural == model.definitions().review_links(sheet.id()) {
                continue;
            }
            let original = super::sheets::read_sheet(package, sheet.part().name())?;
            let current = out
                .rewritten
                .remove(sheet.part().name())
                .map(|b| original.with_bytes(b));
            let bytes = super::review::sorted_links(
                &original,
                current.as_ref().unwrap_or(&original),
                base.definitions().review_links(sheet.id()),
                model.definitions().review_links(sheet.id()),
            )?;
            out.rewritten.insert(sheet.part().name().to_owned(), bytes);
        }
    }
    if page_setup_changed {
        for sheet in sheets
            .iter()
            .filter(|s| model.sheet_by_id(s.id()).is_some())
        {
            let before = base.definitions().page_setup(sheet.id());
            let after = model.definitions().page_setup(sheet.id());
            if before == after {
                continue;
            }
            if sheet.is_macro_sheet() {
                return Err(SaveRefusal::MacroSheetChanged {
                    part: sheet.part().name().into(),
                });
            }
            if let Some(kind) = sheet.preserved_kind() {
                return Err(SaveRefusal::PreservedSheetChanged {
                    part: sheet.part().name().into(),
                    kind,
                });
            }
            if sheets.iter().any(|s| {
                s.id() != sheet.id() && s.part().name().eq_ignore_ascii_case(sheet.part().name())
            }) {
                return Err(SaveRefusal::SharedSheetPart {
                    part: sheet.part().name().into(),
                });
            }
            let original = super::sheets::read_sheet(package, sheet.part().name())?;
            let current = out
                .rewritten
                .remove(sheet.part().name())
                .map(|b| original.with_bytes(b));
            let current = current.as_ref().unwrap_or(&original);
            let natural =
                crate::load::page_setup::read(current).map_err(|e| SaveRefusal::InvalidPart {
                    part: sheet.part().name().into(),
                    detail: e.to_string(),
                })?;
            out.rewritten.insert(
                sheet.part().name().into(),
                super::page_setup::rewrite(current, &natural, &after)?,
            );
        }
    }
    if pivots_changed {
        super::pivot::finish(
            package,
            base.definitions().pivots(),
            model.definitions().pivots(),
            &mut out,
            workbook_part,
            sheets,
        )?;
    }
    if charts_changed {
        super::chart::finish(package, base, model, &mut out, &sheets, structural.as_ref())?;
    }
    if review_changed {
        super::review::finish(
            package,
            workbook_part,
            &sheets,
            base,
            model,
            &moves,
            &mut out,
        )?;
    }
    if sheets_changed {
        super::sheets::finish(package, workbook_part, base, model, sheets, &mut out)?;
    }
    Ok(out)
}

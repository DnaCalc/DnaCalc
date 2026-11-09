//! Copy and paste, fill, and cut and paste (dc-09v): the cell edits a copy or a move of one area
//! into another makes, as Excel's copy and paste writes them.
//!
//! A copy:
//! - copies a value as it is, and a formula relocated, its relative references keeping their
//!   offsets and its absolute ones staying ([`calc_engine::relocate_formula`]); an empty source
//!   cell clears its target;
//! - copies a legacy (Ctrl+Shift+Enter) array formula whole: its formula relocated, authored as
//!   an array over the same shape at the target; a source that holds only part of one is refused,
//!   as Excel refuses to change part of an array;
//! - copies a cell a dynamic array spilled into as its value when the array's anchor is not
//!   copied with it (the anchor copied with it spills again at the target);
//! - fills a target that is a whole multiple of the source's shape by repeating the source (a fill
//!   down or right is the copy of its first row or column over the rest); a single target cell
//!   takes the source's shape;
//! - copies each cell's format (its cell format, the `s` of the file) with it.
//!
//! A cut (a move) puts the source's cells and their formats at the target, on the same sheet or
//! another, and empties the source, whose cells return to the default format. Every formula of
//! the workbook (the moved ones included) is adjusted as Excel's move adjusts it
//! ([`calc_engine::move_references`], measured in Excel: calc-host's
//! tests/fixtures/dc_gf0/cut-excel.json): a reference to moved cells follows them, one to cells
//! the paste writes over is `#REF!`, and a formula moved to another sheet keeps reading the cells
//! it read. Each defined name whose reference is absolute follows the moved cells too
//! ([`calc_engine::move_name`]), and scoped graph watches rebind the formulas that use it, so
//! they are bound to its new formula; a legacy array moves whole (dc-09v, dc-gf0).
//!
//! Every edit is read from the revision before the copy, so a target that overlaps its source
//! copies what the source held, and the edits commit as one transaction and one undo step.
//!
//! Left as Excel does differently, for now: a sheet-scoped name used without its sheet in a
//! formula that moves to another sheet is not given its sheet.

use crate::command::CellEdit;
use crate::state::HostState;
use calc_doc_model::value::{CalcValue, CellKey, CoreRef, GridArea, fold_workbook_name};
use calc_doc_model::{ArrayFormula, ArrayKind, CellInput, CellText, FiniteNumber};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

/// Why a copy made no edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyRefusal {
    Metadata(&'static str),
    /// The source and the target are on sheets the workbook does not hold.
    UnknownSheet,
    /// The target is not a whole multiple of the source's shape (Excel: "the copy area and the
    /// paste area are not the same size"); for a cut, not the source's shape.
    Shape {
        source: (u32, u32),
        target: (u32, u32),
    },
    /// The target reaches past the grid.
    PastTheGrid,
    /// The copy would make more edits than a copy takes at once.
    TooLarge {
        cells: u64,
    },
    /// A source formula's text does not parse, so it cannot be relocated.
    Unrelocatable {
        at: CellKey,
    },
    /// The source holds part of a legacy array formula, which is copied or moved only whole.
    PartOfArray {
        anchor: CellKey,
    },
    SourceChanged,
    /// Captured format definitions belong to another workbook or unsupported source records.
    StyleDefinitionsChanged,
    UnavailableValue {
        at: CellKey,
    },
    Protected,
    PartialSpill {
        anchor: CellKey,
    },
    Series,
    LegacyFill,
    TransposeCut,
    TransposeArray,
    TransposeTable,
    TransposeFill,
}

impl fmt::Display for CopyRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata(reason)=>write!(f,"{reason} (dc-971p.40); nothing changed"),
            Self::TransposeCut => f.write_str("Transposing a cut is not modelled (dc-971p.35); use Copy first"),
            Self::TransposeTable => f.write_str("Transposing table cells is not modelled (dc-971p.35); copy their Values to an ordinary range first"),
            Self::TransposeArray => f.write_str("Transposing array formulas is not modelled (dc-971p.35); choose Values or Cell formats instead"),
            Self::TransposeFill => f.write_str("Transposing a fill is not modelled (dc-971p.35); copy the cells and use Paste instead"),
            Self::LegacyFill => f.write_str(
                "fill from a legacy array is not modelled; copy its whole extent instead",
            ),
            Self::SourceChanged => {
                f.write_str("the cut source changed; cut it again before moving it")
            }
            Self::StyleDefinitionsChanged => f.write_str(
                "the copied styles belong to another workbook or have unsupported source changes (dc-971p.6); copy the source again",
            ),
            Self::UnavailableValue { at } => write!(
                f,
                "row {} column {} has no current calculated value; calculate before copying values",
                at.row(),
                at.column()
            ),
            Self::Protected => {
                f.write_str("pasting into a protected sheet is not modelled; nothing was changed")
            }
            Self::PartialSpill { anchor } => write!(
                f,
                "the edit would change part of the array at row {} column {}; select its whole extent",
                anchor.row(),
                anchor.column()
            ),
            Self::Series => f.write_str(
                "this series needs one date, numbered text or evenly spaced numeric seeds in each row or column",
            ),
            CopyRefusal::UnknownSheet => {
                f.write_str("the copy names a sheet the workbook does not hold")
            }
            CopyRefusal::Shape { source, target } => write!(
                f,
                "the paste area ({} by {}) is not a whole multiple of the copy area ({} by {})",
                target.0, target.1, source.0, source.1
            ),
            CopyRefusal::PastTheGrid => f.write_str("the paste area reaches past the grid"),
            CopyRefusal::TooLarge { cells } => {
                write!(
                    f,
                    "the paste area holds {cells} cells, more than a copy writes at once"
                )
            }
            CopyRefusal::Unrelocatable { at } => write!(
                f,
                "the formula at row {} column {} does not parse, so it cannot be copied",
                at.row(),
                at.column()
            ),
            CopyRefusal::PartOfArray { anchor } => write!(
                f,
                "the copy holds part of the array formula anchored at row {} column {}; an array is copied whole",
                anchor.row(),
                anchor.column()
            ),
        }
    }
}

impl std::error::Error for CopyRefusal {}

/// The most cells one copy writes.
pub use calc_value::COPY_LIMIT;

/// Paste Special's independent content/format choices. Formulas includes literal inputs, as Excel does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteMode {
    All,
    Values,
    Formulas,
    Formats,
}

/// An immutable copy of authored content, cell formats and current values. It stays in the host;
/// the page receives only a clipboard token and a text export.
#[derive(Clone, Debug)]
pub struct CopySnapshot {
    from: GridArea,
    model: Arc<calc_doc_model::ModelRoot>,
    styles: BTreeMap<CellKey, u32>,
    values: BTreeMap<CellKey, Result<Option<CellInput>, CopyRefusal>>,
    dates: BTreeSet<CellKey>,
    spills: Vec<GridArea>,
}

impl CopySnapshot {
    pub fn capture(host: &HostState, from: GridArea) -> Result<Self, CopyRefusal> {
        if host.model.sheet_by_id(from.sheet()).is_none() {
            return Err(CopyRefusal::UnknownSheet);
        }
        if from.cell_count() > COPY_LIMIT {
            return Err(CopyRefusal::TooLarge {
                cells: from.cell_count(),
            });
        }
        let cells: Vec<_> = (from.first().row()..=from.last().row())
            .flat_map(|row| {
                (from.first().column()..=from.last().column())
                    .map(move |col| CellKey::new(from.sheet(), row, col).unwrap())
            })
            .collect();
        let page = crate::project(
            host,
            &crate::ProjectionRequest {
                cells: cells.clone(),
            },
        )
        .map_err(|_| CopyRefusal::UnknownSheet)?;
        let values =
            page.cells()
                .iter()
                .map(|cell| {
                    use crate::ProjectedValue;
                    let value = match cell.value() {
                        ProjectedValue::Empty => Ok(None),
                        ProjectedValue::Literal(v)
                        | ProjectedValue::Calculated { value: v, .. } => match value_input(v) {
                            Some(input) => Ok(Some(input)),
                            None if matches!(v.core_ref(), CoreRef::Empty) => Ok(None),
                            None => Err(CopyRefusal::UnavailableValue { at: cell.at() }),
                        },
                        _ => Err(CopyRefusal::UnavailableValue { at: cell.at() }),
                    };
                    (cell.at(), value)
                })
                .collect();
        Ok(Self {
            from,
            model: Arc::clone(&host.model),
            values,
            styles: cells.iter().map(|&at| (at, host.cell_style(at))).collect(),
            dates: cells
                .iter()
                .copied()
                .filter(|&at| date_format(host.number_format(at).unwrap_or("")))
                .collect(),
            spills: host.engine.spills().map(|spill| spill.area()).collect(),
        })
    }

    pub fn area(&self) -> GridArea {
        self.from
    }

    /// Plan one atomic paste. A cut additionally verifies the source still holds the snapshot.
    pub fn edits(
        &self,
        host: &HostState,
        to: GridArea,
        mode: PasteMode,
        cut: bool,
        series: Option<bool>,
    ) -> Result<Vec<CellEdit>, CopyRefusal> {
        self.edits_oriented(host, to, mode, cut, series, false)
    }

    /// Sort cell inputs/styles through the clipboard relocator. The complete permutation
    /// applies metadata once, after every source row has been read from immutable snapshots.
    pub(crate) fn sorted_row_edits(
        &self,
        host: &HostState,
        to: GridArea,
    ) -> Result<Vec<CellEdit>, CopyRefusal> {
        let to = self.target(to)?;
        check_destination(host, to, PasteMode::All)?;
        copy_edits_from(
            host,
            self.from,
            to,
            false,
            Some(self),
            PasteMode::All,
            None,
            false,
            false,
            false,
        )
    }

    pub(crate) fn edits_oriented(
        &self,
        host: &HostState,
        to: GridArea,
        mode: PasteMode,
        cut: bool,
        series: Option<bool>,
        transpose: bool,
    ) -> Result<Vec<CellEdit>, CopyRefusal> {
        if transpose && cut {
            return Err(CopyRefusal::TransposeCut);
        }
        if transpose
            && self
                .model
                .definitions()
                .tables()
                .iter()
                .any(|t| t.area.intersection(self.from).is_some())
        {
            return Err(CopyRefusal::TransposeTable);
        }
        if transpose
            && !matches!(mode, PasteMode::Values | PasteMode::Formats)
            && (self
                .spills
                .iter()
                .any(|area| self.from.contains(area.first()))
                || self
                    .model
                    .definitions()
                    .arrays_meeting(self.from)
                    .into_iter()
                    .any(|array| self.from.contains(array.anchor())))
        {
            return Err(CopyRefusal::TransposeArray);
        }
        if cut
            && (host.revision() != self.model.revision()
                || self.styles.iter().any(|(&at, &style)| {
                    host.model.cell(at) != self.model.cell(at) || host.cell_style(at) != style
                }))
        {
            return Err(CopyRefusal::SourceChanged);
        }
        let to = self.target_oriented(to, transpose)?;
        check_destination(host, to, mode)?;
        if cut {
            check_destination(host, self.from, PasteMode::All)?;
        }
        copy_edits_from(
            host,
            self.from,
            to,
            cut,
            Some(self),
            mode,
            series,
            false,
            transpose,
            true,
        )
    }

    /// Extend a seed rectangle along one axis, including an incomplete repetition at the far edge.
    pub fn fill_edits(
        &self,
        host: &HostState,
        to: GridArea,
        down: bool,
        series: bool,
    ) -> Result<Vec<CellEdit>, CopyRefusal> {
        check_destination(host, to, PasteMode::All)?;
        copy_edits_from(
            host,
            self.from,
            to,
            false,
            Some(self),
            PasteMode::All,
            series.then_some(down),
            true,
            false,
            true,
        )
    }

    pub fn target(&self, to: GridArea) -> Result<GridArea, CopyRefusal> {
        self.target_oriented(to, false)
    }

    pub(crate) fn target_oriented(
        &self,
        to: GridArea,
        transpose: bool,
    ) -> Result<GridArea, CopyRefusal> {
        let (height, width) = if transpose {
            (self.from.column_count(), self.from.row_count())
        } else {
            (self.from.row_count(), self.from.column_count())
        };
        if to.cell_count() != 1 {
            return Ok(to);
        }
        let last = CellKey::new(
            to.sheet(),
            to.first().row() + height - 1,
            to.first().column() + width - 1,
        )
        .map_err(|_| CopyRefusal::PastTheGrid)?;
        GridArea::new(to.first(), last).map_err(|_| CopyRefusal::PastTheGrid)
    }
}

fn date_format(code: &str) -> bool {
    let mut quoted = false;
    let mut escaped = false;
    let mut bracket = false;
    code.chars().any(|ch| {
        if escaped {
            escaped = false;
            return false;
        }
        if ch == '\\' {
            escaped = true;
            return false;
        }
        if ch == '"' {
            quoted = !quoted;
            return false;
        }
        if !quoted && ch == '[' {
            bracket = true;
            return false;
        }
        if !quoted && ch == ']' {
            bracket = false;
            return false;
        }
        !quoted && !bracket && matches!(ch, 'd' | 'D' | 'y' | 'Y')
    })
}

/// A destructive range write never edits only part of an existing array. Formats may cover children.
pub fn check_destination(
    host: &HostState,
    to: GridArea,
    mode: PasteMode,
) -> Result<(), CopyRefusal> {
    if host.model.sheet_by_id(to.sheet()).is_none() {
        return Err(CopyRefusal::UnknownSheet);
    }
    if host
        .loaded
        .sheet(to.sheet())
        .is_some_and(|sheet| sheet.protected())
    {
        return Err(CopyRefusal::Protected);
    }
    if to.cell_count() > COPY_LIMIT {
        return Err(CopyRefusal::TooLarge {
            cells: to.cell_count(),
        });
    }
    if mode == PasteMode::Formats {
        return Ok(());
    }
    for area in host
        .engine
        .spills()
        .map(|s| s.area())
        .chain(host.model.definitions().arrays().map(|a| a.area()))
    {
        let overlaps = area.sheet() == to.sheet()
            && area.first().row() <= to.last().row()
            && to.first().row() <= area.last().row()
            && area.first().column() <= to.last().column()
            && to.first().column() <= area.last().column();
        if overlaps && !(to.contains(area.first()) && to.contains(area.last())) {
            return Err(CopyRefusal::PartialSpill {
                anchor: area.first(),
            });
        }
    }
    Ok(())
}

/// The cell `rows` and `columns` from `at`, on `sheet`.
fn offset(
    sheet: calc_doc_model::value::SheetId,
    at: CellKey,
    rows: i64,
    columns: i64,
) -> Option<CellKey> {
    let row = u32::try_from(i64::from(at.row()) + rows).ok()?;
    let column = u32::try_from(i64::from(at.column()) + columns).ok()?;
    CellKey::new(sheet, row, column).ok()
}

/// A value as the input a cell holds for it (a spilled cell copied as its value); `None` for a
/// value no cell holds as an input (an array, a reference, an empty value).
fn value_input(value: &CalcValue) -> Option<CellInput> {
    Some(match value.core_ref() {
        CoreRef::Number(number) => CellInput::Number(FiniteNumber::new(number)?),
        CoreRef::Text(text) => CellInput::Text(CellText::new(text.clone())?),
        CoreRef::Logical(logical) => CellInput::Logical(logical),
        CoreRef::Error(code) => CellInput::Error(code),
        CoreRef::Array(array) => return array.get(0, 0).and_then(value_input),
        _ => return None,
    })
}

/// The address map used by both product cut routes and their history.
pub(crate) fn move_coordinates(
    host: &HostState,
    from: GridArea,
    to: CellKey,
) -> calc_fml::reference::StructuralEdit {
    let name = |sheet| {
        host.model()
            .sheet_by_id(sheet)
            .map(|s| s.name().text().to_string_lossy())
            .unwrap_or_default()
    };
    calc_fml::reference::StructuralEdit::MoveCells {
        area: from,
        to,
        source_name: name(from.sheet()),
        target_name: name(to.sheet()),
    }
}

// Keep the immutable clipboard's definitions when Undo removes them or a history branch
// reuses their indices. Only a shared workbook lineage establishes the raw XML source.
fn restore_styles(
    before: &calc_doc_model::StyleTable,
    after: &calc_doc_model::StyleTable,
) -> Result<(calc_doc_model::StyleTable, Vec<u32>), CopyRefusal> {
    use calc_doc_model::StyleTable;
    if before.cell_style_formats != after.cell_style_formats
        || before.cell_styles != after.cell_styles
        || before.table_styles != after.table_styles
        || before.colors != after.colors
        || before.unmodelled != after.unmodelled
    {
        return Err(CopyRefusal::StyleDefinitionsChanged);
    }
    fn records<T: Clone + Eq>(
        before: &[T],
        sources: &BTreeMap<u32, u32>,
        after: &mut Vec<T>,
        after_sources: &mut BTreeMap<u32, u32>,
    ) -> Result<Vec<u32>, CopyRefusal> {
        let mut ids = Vec::with_capacity(before.len());
        for (index, record) in before.iter().enumerate() {
            let source = sources
                .get(&(index as u32))
                .map(|&old| {
                    ids.get(old as usize)
                        .copied()
                        .ok_or(CopyRefusal::StyleDefinitionsChanged)
                })
                .transpose()?;
            let matching = |i: usize, value: &T| {
                value == record && after_sources.get(&(i as u32)).copied() == source
                    // Imported records can carry distinct unknown XML even when parsed equal.
                    && (source.is_some() || i == index)
            };
            let existing = if after.get(index).is_some_and(|v| matching(index, v)) {
                Some(index as u32)
            } else {
                after
                    .iter()
                    .enumerate()
                    .find(|(i, value)| matching(*i, value))
                    .map(|(i, _)| i as u32)
            };
            let id = if let Some(id) = existing {
                id
            } else {
                // Non-derived records belong to the shared imported prefix. An empty source
                // table can also have new default records with no raw XML to preserve.
                if source.is_none() && index < after.len() {
                    return Err(CopyRefusal::StyleDefinitionsChanged);
                }
                let id =
                    u32::try_from(after.len()).map_err(|_| CopyRefusal::StyleDefinitionsChanged)?;
                after.push(record.clone());
                if let Some(source) = source {
                    after_sources.insert(id, source);
                }
                id
            };
            ids.push(id);
        }
        Ok(ids)
    }
    let mut table: StyleTable = after.clone();
    let fonts = records(
        &before.fonts,
        &before.sources.fonts,
        &mut table.fonts,
        &mut table.sources.fonts,
    )?;
    let fills = records(
        &before.fills,
        &before.sources.fills,
        &mut table.fills,
        &mut table.sources.fills,
    )?;
    let borders = records(
        &before.borders,
        &before.sources.borders,
        &mut table.borders,
        &mut table.sources.borders,
    )?;
    let mut numbers = BTreeMap::new();
    for format in &before.number_formats {
        let id = if let Some(found) = table.number_formats.iter().find(|n| *n == format) {
            // Imported equal-code records can carry different unknown XML by ID.
            found.id
        } else if let Some(found) = table.number_formats.iter().find(|n| n.code == format.code) {
            found.id
        } else {
            let mut next = format.clone();
            if table.number_formats.iter().any(|n| n.id == next.id) {
                next.id = table
                    .number_formats
                    .iter()
                    .map(|n| n.id)
                    .max()
                    .unwrap_or(163)
                    .max(163)
                    .checked_add(1)
                    .ok_or(CopyRefusal::StyleDefinitionsChanged)?;
            }
            let id = next.id;
            table.number_formats.push(next);
            id
        };
        numbers.insert(format.id, id);
    }
    let remap = |id: Option<u32>, ids: &[u32]| -> Result<Option<u32>, CopyRefusal> {
        id.map(|i| {
            ids.get(i as usize)
                .copied()
                .ok_or(CopyRefusal::StyleDefinitionsChanged)
        })
        .transpose()
    };
    let mut formats = before.cell_formats.clone();
    for xf in &mut formats {
        xf.font_id = remap(xf.font_id, &fonts)?;
        xf.fill_id = remap(xf.fill_id, &fills)?;
        xf.border_id = remap(xf.border_id, &borders)?;
        xf.number_format_id = xf
            .number_format_id
            .map(|id| numbers.get(&id).copied().unwrap_or(id));
    }
    let ids = records(
        &formats,
        &before.sources.cell_formats,
        &mut table.cell_formats,
        &mut table.sources.cell_formats,
    )?;
    Ok((table, ids))
}

/// The edits a copy (or cut) makes over the current revision of `host`.
pub(crate) fn copy_edits(
    host: &HostState,
    from: GridArea,
    to: GridArea,
    cut: bool,
) -> Result<Vec<CellEdit>, CopyRefusal> {
    CopySnapshot::capture(host, from)?.edits(host, to, PasteMode::All, cut, None)
}

fn copy_edits_from(
    host: &HostState,
    from: GridArea,
    to: GridArea,
    cut: bool,
    snapshot: Option<&CopySnapshot>,
    mode: PasteMode,
    series: Option<bool>,
    partial: bool,
    transpose: bool,
    include_metadata: bool,
) -> Result<Vec<CellEdit>, CopyRefusal> {
    let restored = if matches!(mode, PasteMode::All | PasteMode::Formats) {
        if let Some(snapshot) = snapshot {
            if !snapshot.model.same_lineage(&host.model) {
                return Err(CopyRefusal::StyleDefinitionsChanged);
            }
            let before = snapshot.model.definitions().styles();
            if Arc::ptr_eq(before, host.styles()) {
                None
            } else {
                Some(restore_styles(before, host.styles())?)
            }
        } else {
            None
        }
    } else {
        None
    };
    let source_model = snapshot.map_or(&host.model, |s| &s.model);
    let source_style = |at| {
        let id = snapshot
            .and_then(|s| s.styles.get(&at).copied())
            .unwrap_or_else(|| host.cell_style(at));
        restored
            .as_ref()
            .map_or(id, |(_, ids)| ids.get(id as usize).copied().unwrap_or(id))
    };
    let model = &host.model;
    if model.sheet_by_id(from.sheet()).is_none() || model.sheet_by_id(to.sheet()).is_none() {
        return Err(CopyRefusal::UnknownSheet);
    }
    let (source_rows, source_cols) = (from.row_count(), from.column_count());
    let (tile_rows, tile_cols) = if transpose {
        (source_cols, source_rows)
    } else {
        (source_rows, source_cols)
    };
    // A single target cell takes the source's shape.
    let to = if !partial && to.row_count() == 1 && to.column_count() == 1 {
        let first = to.first();
        let last = CellKey::new(
            to.sheet(),
            first.row() + tile_rows - 1,
            first.column() + tile_cols - 1,
        )
        .map_err(|_| CopyRefusal::PastTheGrid)?;
        GridArea::new(first, last).expect("both corners are on the target's sheet")
    } else {
        to
    };
    let shape = CopyRefusal::Shape {
        source: (tile_rows, tile_cols),
        target: (to.row_count(), to.column_count()),
    };
    if !partial && (to.row_count() % tile_rows != 0 || to.column_count() % tile_cols != 0) {
        return Err(shape);
    }
    if cut && (to.row_count() != source_rows || to.column_count() != source_cols) {
        return Err(shape);
    }
    if to.cell_count() > COPY_LIMIT {
        return Err(CopyRefusal::TooLarge {
            cells: to.cell_count(),
        });
    }
    // The legacy arrays of the source, each of which must lie in it whole.
    let definitions = source_model.definitions();
    let mut legacy: Vec<ArrayFormula> = Vec::new();
    for array in definitions.arrays_meeting(from) {
        if matches!(mode, PasteMode::Values | PasteMode::Formats) {
            continue;
        }
        if array.kind() != ArrayKind::Legacy {
            continue;
        }
        let area = array.area();
        if !(from.contains(area.first()) && from.contains(area.last())) {
            return Err(CopyRefusal::PartOfArray {
                anchor: array.anchor(),
            });
        }
        legacy.push(*array);
    }
    if partial && !legacy.is_empty() {
        return Err(CopyRefusal::LegacyFill);
    }
    let in_legacy = |at: CellKey| legacy.iter().any(|array| array.area().contains(at));

    let mut edits = Vec::with_capacity(to.cell_count() as usize);
    if let Some((table, _)) = &restored {
        if table != host.styles().as_ref() {
            edits.push(CellEdit::Styles(Arc::new(table.clone())));
        }
    }
    let (top, left) = (to.first().row(), to.first().column());
    // A cell's format, set only where it changes.
    let restyle = |edits: &mut Vec<CellEdit>, at: CellKey, style: u32| {
        if host.cell_style(at) != style {
            edits.push(CellEdit::SetFormat {
                at,
                format: Some(style),
            });
        }
    };
    // A cut empties the source's cells the target does not cover first; the target's edits then
    // write over the rest.
    if cut {
        for row in from.first().row()..=from.last().row() {
            for column in from.first().column()..=from.last().column() {
                let at = CellKey::new(from.sheet(), row, column).expect("inside the source");
                if !to.contains(at) {
                    edits.push(CellEdit::Clear {
                        area: GridArea::cell(at),
                    });
                    restyle(&mut edits, at, 0);
                }
            }
        }
    }
    let formula_text = |source: CellKey, target: CellKey, formula| {
        if cut {
            Some(
                calc_engine::move_references(model, formula, source, from, to.first())
                    .unwrap_or_else(|| formula.clone()),
            )
        } else {
            calc_engine::relocate_formula(source_model, formula, source, target)
        }
    };
    for row in top..=to.last().row() {
        for column in left..=to.last().column() {
            let target = CellKey::new(to.sheet(), row, column).expect("inside the target area");
            let (row_offset, col_offset) = if transpose {
                ((column - left) % source_rows, (row - top) % source_cols)
            } else if partial {
                (
                    (i64::from(row) - i64::from(from.first().row()))
                        .rem_euclid(i64::from(source_rows)) as u32,
                    (i64::from(column) - i64::from(from.first().column()))
                        .rem_euclid(i64::from(source_cols)) as u32,
                )
            } else {
                ((row - top) % source_rows, (column - left) % source_cols)
            };
            let source = CellKey::new(
                from.sheet(),
                from.first().row() + row_offset,
                from.first().column() + col_offset,
            )
            .expect("inside the source area");
            if matches!(mode, PasteMode::All | PasteMode::Formats) {
                restyle(&mut edits, target, source_style(source));
            }
            if mode == PasteMode::Formats {
                continue;
            }
            if mode == PasteMode::Values {
                let input = snapshot
                    .and_then(|s| s.values.get(&source))
                    .cloned()
                    .unwrap_or(Err(CopyRefusal::UnavailableValue { at: source }))?;
                edits.push(match input {
                    Some(input) => CellEdit::Set { at: target, input },
                    None => CellEdit::Clear {
                        area: GridArea::cell(target),
                    },
                });
                continue;
            }
            if let (Some(down), Some(snapshot)) = (series, snapshot) {
                let n = if down { source_rows } else { source_cols };
                let seed_at = |i| {
                    CellKey::new(
                        from.sheet(),
                        if down {
                            from.first().row() + i
                        } else {
                            source.row()
                        },
                        if down {
                            source.column()
                        } else {
                            from.first().column() + i
                        },
                    )
                    .unwrap()
                };
                let number = |i| match source_model.cell(seed_at(i)) {
                    Some(CellInput::Number(n)) => Some(n.get()),
                    _ => None,
                };
                if let Some(first) = number(0) {
                    let step = if n > 1 {
                        number(1).map(|v| v - first)
                    } else if snapshot.dates.contains(&source) {
                        Some(1.0)
                    } else {
                        None
                    };
                    if let Some(step) = step {
                        if (0..n).any(|i| {
                            number(i)
                                .is_none_or(|v| (v - (first + step * f64::from(i))).abs() > 1e-9)
                        }) {
                            return Err(CopyRefusal::Series);
                        }
                        let index = if down {
                            i64::from(row) - i64::from(from.first().row())
                        } else {
                            i64::from(column) - i64::from(from.first().column())
                        };
                        let input = FiniteNumber::new(first + step * index as f64)
                            .ok_or(CopyRefusal::Series)?;
                        edits.push(CellEdit::Set {
                            at: target,
                            input: CellInput::Number(input),
                        });
                        continue;
                    }
                }
                if let Some((prefix, suffix, width, first)) =
                    numbered_text(source_model.cell(seed_at(0)))
                {
                    let step = if n > 1 {
                        numbered_text(source_model.cell(seed_at(1)))
                            .and_then(|(_, _, _, v)| v.checked_sub(first))
                            .ok_or(CopyRefusal::Series)?
                    } else {
                        1
                    };
                    for i in 0..n {
                        let (p, s, _, v) = numbered_text(source_model.cell(seed_at(i)))
                            .ok_or(CopyRefusal::Series)?;
                        if p != prefix
                            || s != suffix
                            || Some(v)
                                != step
                                    .checked_mul(i128::from(i))
                                    .and_then(|v| first.checked_add(v))
                        {
                            return Err(CopyRefusal::Series);
                        }
                    }
                    let index = if down {
                        i128::from(row) - i128::from(from.first().row())
                    } else {
                        i128::from(column) - i128::from(from.first().column())
                    };
                    let value = step
                        .checked_mul(index)
                        .and_then(|v| first.checked_add(v))
                        .filter(|v| *v >= 0)
                        .ok_or(CopyRefusal::Series)?;
                    let mut units = prefix;
                    units.extend(format!("{value:0width$}").encode_utf16());
                    units.extend(suffix);
                    let input = CellText::new(
                        calc_doc_model::value::ExcelText::from_utf16_code_units(units),
                    )
                    .ok_or(CopyRefusal::Series)?;
                    edits.push(CellEdit::Set {
                        at: target,
                        input: CellInput::Text(input),
                    });
                    continue;
                }
            }
            if let Some(array) = legacy.iter().find(|array| array.area().contains(source)) {
                // A legacy array is written once, from its anchor, over its shape.
                if array.anchor() != source {
                    continue;
                }
                let Some(CellInput::Formula(formula)) = source_model.cell(source) else {
                    continue;
                };
                let area = array.area();
                let shift = |at: CellKey| {
                    offset(
                        to.sheet(),
                        at,
                        i64::from(target.row()) - i64::from(source.row()),
                        i64::from(target.column()) - i64::from(source.column()),
                    )
                };
                let placed = GridArea::new(
                    shift(area.first()).ok_or(CopyRefusal::PastTheGrid)?,
                    shift(area.last()).ok_or(CopyRefusal::PastTheGrid)?,
                )
                .expect("both corners are on the target's sheet");
                let formula = formula_text(source, target, formula)
                    .ok_or(CopyRefusal::Unrelocatable { at: source })?;
                edits.push(CellEdit::SetArray {
                    array: ArrayFormula::new(placed, ArrayKind::Legacy),
                    formula,
                });
                continue;
            }
            let edit = match source_model.cell(source) {
                Some(CellInput::Formula(formula)) => {
                    let formula = formula_text(source, target, formula)
                        .ok_or(CopyRefusal::Unrelocatable { at: source })?;
                    CellEdit::Set {
                        at: target,
                        input: CellInput::Formula(formula),
                    }
                }
                Some(input) => CellEdit::Set {
                    at: target,
                    input: input.clone(),
                },
                // A cell a dynamic array spilled into, whose anchor is not copied with it, is
                // copied as its value.
                None => match snapshot
                    .filter(|s| {
                        !s.spills
                            .iter()
                            .any(|area| area.contains(source) && from.contains(area.first()))
                            && !in_legacy(source)
                    })
                    .and_then(|s| s.values.get(&source))
                    .cloned()
                    .transpose()?
                    .flatten()
                {
                    Some(input) if !cut => CellEdit::Set { at: target, input },
                    _ => CellEdit::Clear {
                        area: GridArea::cell(target),
                    },
                },
            };
            edits.push(edit);
        }
    }
    // A cut moves every reference to the moved cells, in formulas outside the source as well.
    if cut {
        let target_cells: BTreeSet<CellKey> = edits
            .iter()
            .filter_map(|edit| match edit {
                CellEdit::Set { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        for entry in model.sheets() {
            for (at, input) in model.sheet_cells(entry.id()).into_iter().flatten() {
                let CellInput::Formula(formula) = input else {
                    continue;
                };
                if from.contains(at) || to.contains(at) || target_cells.contains(&at) {
                    continue;
                }
                if let Some(moved) =
                    calc_engine::move_references(model, formula, at, from, to.first())
                {
                    if let Some(array) = definitions
                        .array_containing(at)
                        .filter(|a| a.anchor() == at)
                    {
                        edits.push(CellEdit::SetArray {
                            array: *array,
                            formula: moved,
                        });
                    } else {
                        edits.push(CellEdit::Set {
                            at,
                            input: CellInput::Formula(moved),
                        });
                    }
                }
            }
        }
        // Definition edits rebind their graph users through scoped name watches.
        let mut moved_names: Vec<(String, calc_doc_model::NameScope, String)> = Vec::new();
        for name in definitions.name_table().definitions() {
            if let Some(formula) =
                calc_engine::move_name(model, &name.formula, name.scope, from, to.first())
            {
                moved_names.push((name.name.clone(), name.scope, formula.clone()));
                edits.push(CellEdit::SetName {
                    name: name.name.clone(),
                    scope: name.scope,
                    formula: Some(formula),
                });
            }
        }
        // A formula moved to another sheet that uses a name scoped to its old sheet, without the
        // sheet, takes the name with it: the target sheet is given a name of that scope with the
        // same formula, unless it holds one already, as Excel does (Excel 16.0.20430, 2026-10-27:
        // =LocalIn*2 moved from Data to Other kept its text and Other gained LocalIn =
        // Data!$A$2; tests/fixtures/dc_gf0/cut-excel.json, local-name-to-other).
        if from.sheet() != to.sheet() {
            let source = calc_doc_model::NameScope::Sheet(from.sheet());
            let target = calc_doc_model::NameScope::Sheet(to.sheet());
            let moving: Vec<String> = model
                .cells(from)
                .into_iter()
                .flatten()
                .filter_map(|(_, input)| match input {
                    CellInput::Formula(formula) => {
                        Some(fold_workbook_name(&formula.text().to_string_lossy()))
                    }
                    _ => None,
                })
                .collect();
            for name in definitions
                .name_table()
                .definitions()
                .filter(|name| name.scope == source)
            {
                let lower = fold_workbook_name(&name.name);
                let taken = definitions
                    .name_table()
                    .lookup(target, &name.name)
                    .is_some();
                if taken || !moving.iter().any(|text| uses_name(text, &lower)) {
                    continue;
                }
                let formula = moved_names
                    .iter()
                    .find(|(moved, scope, _)| *scope == source && moved == &name.name)
                    .map_or_else(|| name.formula.clone(), |(_, _, formula)| formula.clone());
                edits.push(CellEdit::SetName {
                    name: name.name.clone(),
                    scope: target,
                    formula: Some(formula),
                });
            }
        }
    }
    if include_metadata {
        crate::copy_metadata::append(
            host,
            source_model,
            from,
            to,
            mode,
            cut,
            transpose,
            &mut edits,
        )?;
    }
    Ok(edits)
}

/// Whether the formula `text` holds `name` as a whole word, both folded as workbook names are
/// compared (calc-value's `fold_workbook_name`, dc-cpr): a name a formula may use. A text or
/// another name that holds it as a word counts too, which only sets a formula again that did not
/// need it.
fn uses_name(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '.' || c == '\\';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

/// The last ASCII digit run, retaining all other UTF-16 units and any leading zero width.
fn numbered_text(input: Option<&CellInput>) -> Option<(Vec<u16>, Vec<u16>, usize, i128)> {
    let CellInput::Text(text) = input? else {
        return None;
    };
    let units = text.text().utf16_code_units();
    let digit = |u: &u16| (u16::from(b'0')..=u16::from(b'9')).contains(u);
    let end = units.iter().rposition(digit)? + 1;
    let start = units[..end]
        .iter()
        .rposition(|u| !digit(u))
        .map_or(0, |i| i + 1);
    let mut number = 0i128;
    for &unit in &units[start..end] {
        number = number
            .checked_mul(10)?
            .checked_add(i128::from(unit - u16::from(b'0')))?;
    }
    let width = if end - start > 1 && units[start] == u16::from(b'0') {
        end - start
    } else {
        0
    };
    Some((
        units[..start].to_vec(),
        units[end..].to_vec(),
        width,
        number,
    ))
}

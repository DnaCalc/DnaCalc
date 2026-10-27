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
//! ([`calc_engine::move_name`]), and the formulas that use a name so changed are set again, so
//! they are bound to its new formula; a legacy array moves whole (dc-09v, dc-gf0).
//!
//! Every edit is read from the revision before the copy, so a target that overlaps its source
//! copies what the source held, and the edits commit as one transaction and one undo step.
//!
//! Left as Excel does differently, for now: a sheet-scoped name used without its sheet in a
//! formula that moves to another sheet is not given its sheet.

use crate::command::CellEdit;
use crate::state::HostState;
use calc_doc_model::value::{CalcValue, CellKey, CoreValue, GridArea};
use calc_doc_model::{ArrayFormula, ArrayKind, CellInput, CellText, FiniteNumber};
use std::collections::BTreeSet;
use std::fmt;

/// Why a copy made no edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyRefusal {
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
    TooLarge { cells: u64 },
    /// A source formula's text does not parse, so it cannot be relocated.
    Unrelocatable { at: CellKey },
    /// The source holds part of a legacy array formula, which is copied or moved only whole.
    PartOfArray { anchor: CellKey },
}

impl fmt::Display for CopyRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
pub const COPY_LIMIT: u64 = 1_000_000;

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
    Some(match value.core() {
        CoreValue::Number(number) => CellInput::Number(FiniteNumber::new(*number)?),
        CoreValue::Text(text) => CellInput::Text(CellText::new(text.clone())?),
        CoreValue::Logical(logical) => CellInput::Logical(*logical),
        CoreValue::Error(code) => CellInput::Error(*code),
        _ => return None,
    })
}

/// The edits a copy (or, with `cut`, a move) of `from` into `to` makes over the current revision
/// of `host`.
pub(crate) fn copy_edits(
    host: &HostState,
    from: GridArea,
    to: GridArea,
    cut: bool,
) -> Result<Vec<CellEdit>, CopyRefusal> {
    let model = &host.model;
    if model.sheet_by_id(from.sheet()).is_none() || model.sheet_by_id(to.sheet()).is_none() {
        return Err(CopyRefusal::UnknownSheet);
    }
    let (source_rows, source_cols) = (from.row_count(), from.column_count());
    // A single target cell takes the source's shape.
    let to = if to.row_count() == 1 && to.column_count() == 1 {
        let first = to.first();
        let last = CellKey::new(
            to.sheet(),
            first.row() + source_rows - 1,
            first.column() + source_cols - 1,
        )
        .map_err(|_| CopyRefusal::PastTheGrid)?;
        GridArea::new(first, last).expect("both corners are on the target's sheet")
    } else {
        to
    };
    let shape = CopyRefusal::Shape {
        source: (source_rows, source_cols),
        target: (to.row_count(), to.column_count()),
    };
    if to.row_count() % source_rows != 0 || to.column_count() % source_cols != 0 {
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
    let definitions = model.definitions();
    let mut legacy: Vec<ArrayFormula> = Vec::new();
    for array in definitions.arrays_meeting(from) {
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
    let in_legacy = |at: CellKey| legacy.iter().any(|array| array.area().contains(at));

    let mut edits = Vec::with_capacity(to.cell_count() as usize);
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
            calc_engine::relocate_formula(model, formula, source, target)
        }
    };
    for row in top..=to.last().row() {
        for column in left..=to.last().column() {
            let target = CellKey::new(to.sheet(), row, column).expect("inside the target area");
            let source = CellKey::new(
                from.sheet(),
                from.first().row() + (row - top) % source_rows,
                from.first().column() + (column - left) % source_cols,
            )
            .expect("inside the source area");
            restyle(&mut edits, target, host.cell_style(source));
            if let Some(array) = legacy.iter().find(|array| array.area().contains(source)) {
                // A legacy array is written once, from its anchor, over its shape.
                if array.anchor() != source {
                    continue;
                }
                let Some(CellInput::Formula(formula)) = model.cell(source) else {
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
            let edit = match model.cell(source) {
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
                None => match host
                    .engine
                    .spill_covering(source)
                    .filter(|spill| !from.contains(spill.anchor()) && !in_legacy(source))
                    .and_then(|spill| value_input(spill.value_at(source)))
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
        // Each defined name whose reference follows the moved cells, and the formulas that use
        // it, set again so they are bound to its new formula.
        let mut renamed: Vec<String> = Vec::new();
        let mut moved_names: Vec<(String, calc_doc_model::NameScope, String)> = Vec::new();
        for name in definitions.names() {
            if let Some(formula) =
                calc_engine::move_name(model, &name.formula, name.scope, from, to.first())
            {
                renamed.push(name.name.to_ascii_lowercase());
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
                        Some(formula.text().to_string_lossy().to_ascii_lowercase())
                    }
                    _ => None,
                })
                .collect();
            for name in definitions
                .names()
                .iter()
                .filter(|name| name.scope == source)
            {
                let lower = name.name.to_ascii_lowercase();
                let taken = definitions
                    .names()
                    .iter()
                    .any(|held| held.scope == target && held.name.eq_ignore_ascii_case(&name.name));
                if taken || !moving.iter().any(|text| uses_name(text, &lower)) {
                    continue;
                }
                let formula = moved_names
                    .iter()
                    .find(|(moved, scope, _)| *scope == source && moved == &name.name)
                    .map_or_else(|| name.formula.clone(), |(_, _, formula)| formula.clone());
                renamed.push(lower);
                edits.push(CellEdit::SetName {
                    name: name.name.clone(),
                    scope: target,
                    formula: Some(formula),
                });
            }
        }
        if !renamed.is_empty() {
            let set: BTreeSet<CellKey> = edits
                .iter()
                .filter_map(|edit| match edit {
                    CellEdit::Set { at, .. } => Some(*at),
                    CellEdit::SetArray { array, .. } => Some(array.anchor()),
                    _ => None,
                })
                .collect();
            for entry in model.sheets() {
                for (at, input) in model.sheet_cells(entry.id()).into_iter().flatten() {
                    let CellInput::Formula(formula) = input else {
                        continue;
                    };
                    if from.contains(at) || to.contains(at) || set.contains(&at) {
                        continue;
                    }
                    let text = formula.text().to_string_lossy().to_ascii_lowercase();
                    if !renamed.iter().any(|name| uses_name(&text, name)) {
                        continue;
                    }
                    match definitions
                        .array_containing(at)
                        .filter(|a| a.anchor() == at)
                    {
                        Some(array) => edits.push(CellEdit::SetArray {
                            array: *array,
                            formula: formula.clone(),
                        }),
                        None => edits.push(CellEdit::Set {
                            at,
                            input: CellInput::Formula(formula.clone()),
                        }),
                    }
                }
            }
        }
    }
    Ok(edits)
}

/// Whether the formula `text` (lower case) holds `name` (lower case) as a whole word: a name a
/// formula may use. A text or another name that holds it as a word counts too, which only sets a
/// formula again that did not need it.
fn uses_name(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '.' || c == '\\';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

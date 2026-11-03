//! Undo and redo: each committed edit is one step, recorded with the edits that restore the cells
//! it touched to what they held before it. Undoing a step commits those edits as a new
//! transaction, and redoing it commits the step's own edits again, so every revision the host
//! makes, an undo's included, is a transaction through [`crate::dispatch`] and the authored
//! revision only moves forward.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnatreecalc-host/src/app/dispatcher.rs`
//! (`undo_stack`, `redo_stack`, `RevisionCursorEntry`, `undo_revision`, `redo_revision`,
//! `record_revision_undo_boundary`): two stacks of entries that carry the selection to restore, a
//! new edit clearing the redo stack, and an undo or redo that fails putting its entry back. The
//! legacy entries named OxCalc revisions to navigate back to; calc-host keeps one engine state and
//! commits forward, so an entry here carries the inverse edits instead.

use crate::command::CellEdit;
use calc_doc_model::value::{CellKey, GridArea};
use calc_doc_model::{AxisEdit, CellInput, ModelRoot};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

pub use calc_doc_model::value::Selection;

/// Why an edit cannot be undone: what it removed that no cell edit can put back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrreversibleReason {
    /// The edit set or cleared a cell whose content the model does not represent (a rich value, a
    /// data table's cell), so what it held cannot be authored again.
    UnloadedCell,
}

/// An edit that cannot be undone, and the first cell that makes it so.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Irreversible {
    pub at: CellKey,
    pub reason: IrreversibleReason,
}

impl fmt::Display for Irreversible {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.reason {
            IrreversibleReason::UnloadedCell => "a cell whose content is not modelled",
        };
        write!(
            f,
            "the edit removed {what} at row {} column {}, which no cell edit restores",
            self.at.row(),
            self.at.column()
        )
    }
}

/// What undoes a step.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Inverse {
    /// The edits that restore every cell the step touched.
    Edits(Vec<CellEdit>),
    /// The revision before the step, brought back whole ([`crate::command::restore`]), with the
    /// axis edits that led to it.
    Restore {
        model: Arc<ModelRoot>,
        axis_edits: Vec<AxisEdit>,
    },
}

/// One undo step: a committed edit, the edits that undo it, and the selection before and after.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Step {
    pub(crate) forward: Vec<CellEdit>,
    pub(crate) coordinates: Option<calc_fml::reference::StructuralEdit>,
    pub(crate) inverse: Result<Inverse, Irreversible>,
    pub(crate) before: Selection,
    pub(crate) after: Selection,
}

/// Undo follows the surviving locations back; it never restores a deleted scope.
pub(crate) fn inverse_coordinates(
    edit: &calc_fml::reference::StructuralEdit,
) -> calc_fml::reference::StructuralEdit {
    use calc_fml::reference::StructuralEdit as E;
    match edit {
        E::ShiftCells { area, down, insert } => E::ShiftCells {
            area: *area,
            down: *down,
            insert: !insert,
        },
        E::MoveCells {
            area,
            to: _,
            source_name,
            target_name,
        } => E::MoveCells {
            area: edit.area(*area).expect("a validated cut fits on the grid"),
            to: area.first(),
            source_name: target_name.clone(),
            target_name: source_name.clone(),
        },
        _ => unreachable!("axis and sheet transforms use snapshot history"),
    }
}

/// The areas each edit touches, in order.
fn touched(edits: &[CellEdit]) -> Vec<GridArea> {
    let mut areas: Vec<GridArea> = Vec::new();
    for edit in edits {
        let area = match edit {
            CellEdit::Set { at, .. } => GridArea::cell(*at),
            CellEdit::Clear { area } => *area,
            CellEdit::SetArray { array, .. } => array.area(),
            CellEdit::AutoFilter { .. }
            | CellEdit::Styles(_)
            | CellEdit::Layout(_)
            | CellEdit::ConditionalRules { .. }
            | CellEdit::Axis(_)
            | CellEdit::Sheet(_)
            | CellEdit::AxisFormat(_)
            | CellEdit::PaneLayout { .. }
            | CellEdit::FrozenPanes { .. }
            | CellEdit::SetFormat { .. }
            | CellEdit::SetName { .. }
            | CellEdit::RestoreNames(_)
            | CellEdit::SetIteration(_) => continue,
        };
        if !areas.contains(&area) {
            areas.push(area);
        }
    }
    areas
}

/// The edits that restore every cell `edits` touches to what `model` (the revision they are
/// committed against) holds: each touched area cleared, then each array formula whose anchor an
/// edit touches authored again over its area, then each cell of the touched areas and of those
/// arrays' areas that held an input set to it again (re-authoring an array clears its area, a
/// value blocking a dynamic array's spill included), then each cell format an edit sets returned to
/// the one the cell held, and each defined name an edit sets to the formula it held (the formulas
/// that use it are among the cells the edits set, so they are set again), and the iteration
/// settings to those the workbook held when an edit sets them (dc-y2o.2). Refused when an edit
/// touches an unloaded cell, whose content no edit authors.
pub(crate) fn inverse(
    model: &Arc<ModelRoot>,
    axis_edits: &[AxisEdit],
    edits: &[CellEdit],
) -> Result<Inverse, Irreversible> {
    // A structural edit drops what it deletes and the references to it: the revision before it
    // is brought back whole.
    if edits.iter().any(|edit| {
        matches!(
            edit,
            CellEdit::Axis(_)
                | CellEdit::Layout(_)
                | CellEdit::Sheet(
                    calc_doc_model::SheetEdit::Insert { .. }
                        | calc_doc_model::SheetEdit::Rename { .. }
                        | calc_doc_model::SheetEdit::Move { .. }
                        | calc_doc_model::SheetEdit::Remove { .. }
                )
        )
    }) {
        return Ok(Inverse::Restore {
            model: Arc::clone(model),
            axis_edits: axis_edits.to_vec(),
        });
    }
    let areas = touched(edits);
    let definitions = model.definitions();
    let mut arrays = Vec::new();
    for area in &areas {
        for array in definitions
            .arrays_meeting(*area)
            .filter(|array| area.contains(array.anchor()))
        {
            if !arrays.contains(array) {
                arrays.push(*array);
            }
        }
        if let Some((at, _)) = definitions.unloaded_in(*area).next() {
            return Err(Irreversible {
                at,
                reason: IrreversibleReason::UnloadedCell,
            });
        }
    }
    // The inputs to put back: those of the touched areas, and those of each array formula's area
    // that is authored again, which re-authoring it clears (a value in a dynamic array's area,
    // which blocks its spill, lies outside the touched areas when only the anchor was edited).
    let mut prior = BTreeMap::new();
    let restored = arrays.iter().map(|array| array.area());
    for area in areas.iter().copied().chain(restored) {
        for (at, input) in model.cells(area).into_iter().flatten() {
            prior.entry(at).or_insert_with(|| input.clone());
        }
    }
    let mut inverse: Vec<CellEdit> = areas
        .into_iter()
        .map(|area| CellEdit::Clear { area })
        .collect();
    for array in &arrays {
        let Some(CellInput::Formula(formula)) = prior.remove(&array.anchor()) else {
            unreachable!("a model's array formula's anchor holds its formula");
        };
        inverse.push(CellEdit::SetArray {
            array: *array,
            formula,
        });
    }
    inverse.extend(
        prior
            .into_iter()
            .map(|(at, input)| CellEdit::Set { at, input }),
    );
    for edit in edits {
        if let CellEdit::Sheet(calc_doc_model::SheetEdit::SetVisibility { sheet, .. }) = edit {
            if let Some(entry) = model.sheet_by_id(*sheet) {
                inverse.push(CellEdit::Sheet(calc_doc_model::SheetEdit::SetVisibility {
                    sheet: *sheet,
                    visibility: entry.visibility(),
                }));
            }
        }
    }
    let mut filters = BTreeSet::new();
    for edit in edits {
        if let CellEdit::AutoFilter { sheet, .. } = edit && filters.insert(*sheet) {
            inverse.push(CellEdit::AutoFilter { sheet:*sheet, filter:definitions.auto_filter(*sheet).cloned() });
        }
    }
    let mut conditional = BTreeSet::new();
    for edit in edits {
        if let CellEdit::ConditionalRules { sheet, .. } = edit
            && conditional.insert(*sheet)
        {
            inverse.push(CellEdit::ConditionalRules {
                sheet: *sheet,
                rules: definitions.conditional_rules(*sheet).to_vec(),
            });
        }
    }
    if edits.iter().any(|e| matches!(e, CellEdit::Styles(_))) {
        inverse.push(CellEdit::Styles(Arc::clone(definitions.styles())));
    }
    let mut formatted = BTreeSet::new();
    let mut panes = BTreeSet::new();
    let mut dimensioned = BTreeSet::new();
    for edit in edits {
        if let CellEdit::AxisFormat(edit) = edit
            && dimensioned.insert((
                edit.sheet(),
                edit.axis() == calc_doc_model::Axis::Rows,
                edit.index(),
            ))
        {
            let old = definitions
                .axis_layouts()
                .get(edit.sheet(), edit.axis(), edit.index());
            inverse.push(CellEdit::AxisFormat(
                calc_doc_model::AxisFormatEdit::new(edit.sheet(), edit.axis(), edit.index(), old)
                    .expect("a validated band"),
            ));
        }
        if let CellEdit::FrozenPanes { sheet, .. } | CellEdit::PaneLayout {sheet,..} = edit
            && panes.insert(*sheet)
        {
            inverse.push(CellEdit::PaneLayout {
                sheet: *sheet,
                layout: definitions.pane_layout(*sheet),
            });
        }
        if let CellEdit::SetFormat { at, .. } = edit
            && formatted.insert(*at)
        {
            inverse.push(CellEdit::SetFormat {
                at: *at,
                format: definitions.cell_format(*at),
            });
        }
    }
    let mut keys = std::collections::BTreeSet::new();
    let mut positions = false;
    for edit in edits {
        match edit {
            CellEdit::SetName {
                name,
                scope,
                formula,
            } => {
                keys.insert(calc_doc_model::value::NameKey::new(*scope, name));
                positions |= formula.is_none();
            }
            CellEdit::RestoreNames(names) => {
                keys.extend(names.iter().map(|name| name.key.clone()));
                positions = true;
            }
            _ => {}
        }
    }
    if !keys.is_empty() {
        inverse.push(CellEdit::RestoreNames(
            definitions
                .name_table()
                .snapshot_keys(&keys.into_iter().collect::<Vec<_>>(), positions),
        ));
    }
    if edits
        .iter()
        .any(|edit| matches!(edit, CellEdit::SetIteration(_)))
    {
        inverse.push(CellEdit::SetIteration(definitions.iteration()));
    }
    Ok(Inverse::Edits(inverse))
}

/// The undo and redo stacks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    limit: usize,
}

impl History {
    pub(crate) fn new(limit: usize) -> History {
        History {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    /// Records a committed edit: it becomes the step an undo takes back, the oldest step is
    /// dropped beyond the limit, and nothing is left to redo.
    pub(crate) fn record(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub(crate) fn pop_undo(&mut self) -> Option<Step> {
        self.undo.pop()
    }

    pub(crate) fn pop_redo(&mut self) -> Option<Step> {
        self.redo.pop()
    }

    /// Puts back a step an undo did not commit.
    pub(crate) fn restore_undo(&mut self, step: Step) {
        self.undo.push(step);
    }

    /// Puts back a step a redo did not commit.
    pub(crate) fn restore_redo(&mut self, step: Step) {
        self.redo.push(step);
    }

    /// A step an undo committed: a redo commits it again.
    pub(crate) fn undone(&mut self, step: Step) {
        self.redo.push(step);
    }

    /// A step a redo committed: an undo takes it back again.
    pub(crate) fn redone(&mut self, step: Step) {
        self.undo.push(step);
    }

    pub(crate) fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub(crate) fn redo_depth(&self) -> usize {
        self.redo.len()
    }
}

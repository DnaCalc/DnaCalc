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
use calc_doc_model::ModelRoot;
use calc_doc_model::value::{CellKey, GridArea};
use std::collections::BTreeMap;
use std::fmt;

/// A cell or area selection: the active cell and the selected areas.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub active: Option<CellKey>,
    pub areas: Vec<GridArea>,
}

impl Selection {
    /// One cell, active and selected.
    pub fn cell(at: CellKey) -> Selection {
        Selection {
            active: Some(at),
            areas: vec![GridArea::cell(at)],
        }
    }
}

/// Why an edit cannot be undone: what it removed that no cell edit can put back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrreversibleReason {
    /// The edit set or cleared the anchor of an array formula, which removed the array; no cell
    /// edit authors an array formula (the operation that does belongs to this package, dc-35p).
    ArrayFormula,
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
            IrreversibleReason::ArrayFormula => "an array formula",
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

/// One undo step: a committed edit, the edits that undo it, and the selection before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) forward: Vec<CellEdit>,
    pub(crate) inverse: Result<Vec<CellEdit>, Irreversible>,
    pub(crate) before: Selection,
    pub(crate) after: Selection,
}

/// The areas each edit touches, in order.
fn touched(edits: &[CellEdit]) -> Vec<GridArea> {
    let mut areas: Vec<GridArea> = Vec::new();
    for edit in edits {
        let area = match edit {
            CellEdit::Set { at, .. } => GridArea::cell(*at),
            CellEdit::Clear { area } => *area,
        };
        if !areas.contains(&area) {
            areas.push(area);
        }
    }
    areas
}

/// The edits that restore every cell `edits` touches to what `model` (the revision they are
/// committed against) holds: each touched area cleared, then each touched cell that held an input
/// set to it again. Refused when an edit touches an array formula's anchor or an unloaded cell.
pub(crate) fn inverse(
    model: &ModelRoot,
    edits: &[CellEdit],
) -> Result<Vec<CellEdit>, Irreversible> {
    let areas = touched(edits);
    let definitions = model.definitions();
    for area in &areas {
        if let Some(array) = definitions
            .arrays_meeting(*area)
            .find(|array| area.contains(array.anchor()))
        {
            return Err(Irreversible {
                at: array.anchor(),
                reason: IrreversibleReason::ArrayFormula,
            });
        }
        if let Some((at, _)) = definitions.unloaded_in(*area).next() {
            return Err(Irreversible {
                at,
                reason: IrreversibleReason::UnloadedCell,
            });
        }
    }
    let mut prior = BTreeMap::new();
    for area in &areas {
        for (at, input) in model.cells(*area).into_iter().flatten() {
            prior.entry(at).or_insert_with(|| input.clone());
        }
    }
    let mut inverse: Vec<CellEdit> = areas
        .into_iter()
        .map(|area| CellEdit::Clear { area })
        .collect();
    inverse.extend(
        prior
            .into_iter()
            .map(|(at, input)| CellEdit::Set { at, input }),
    );
    Ok(inverse)
}

/// The undo and redo stacks.
#[derive(Clone, Debug, PartialEq, Eq)]
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

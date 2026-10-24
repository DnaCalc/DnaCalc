//! Structural edits of a model root (L3, dc-m3u): rows and columns inserted or deleted, sheets
//! inserted, renamed, moved and removed, as [`ModelRoot::apply_structural`] applies them.
//!
//! The model moves what it holds with the cells: each cell's input, array formulas, tables,
//! unloaded cells, hidden rows and the anchors of the skin layout's blocks. It does not parse
//! formulas: a [`FormulaRewriter`] the caller gives (CalcEngine's, over CalcFml) rewrites the
//! references of every formula and defined name, so this package keeps depending on no formula
//! language.

use crate::definitions::{ArrayFormula, DefinedName, Definitions, NameScope, Table};
use crate::input::{CellInput, FormulaText};
use crate::op::{Axis, AxisChange, AxisEdit, OpBatch, SheetEdit, WorkbookOp};
use crate::root::{ApplyError, ModelRoot, SheetEntry};
use crate::sheet::{SheetName, SheetVisibility};
use calc_value::{CellKey, GridArea, SheetId};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A structural change whose effect on references a [`FormulaRewriter`] computes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralChange {
    /// Rows or columns inserted or deleted.
    Axis(AxisEdit),
    /// A sheet renamed.
    RenameSheet { sheet: SheetId, name: SheetName },
    /// A sheet removed.
    RemoveSheet { sheet: SheetId },
}

/// Rewrites the references of formulas for a structural change: what a formula language knows
/// and the model does not. Each method is given the root before the change.
pub trait FormulaRewriter {
    /// The text `change` leaves of `formula`, in the cell `at` of `root`; `None` when the change
    /// leaves it as it is.
    fn formula(
        &self,
        root: &ModelRoot,
        formula: &FormulaText,
        at: CellKey,
        change: &StructuralChange,
    ) -> Option<FormulaText>;

    /// The text `change` leaves of a defined name's formula `formula` (as the file stores it,
    /// without `=`), of scope `scope`; `None` when it is left as it is.
    fn name(
        &self,
        root: &ModelRoot,
        formula: &str,
        scope: NameScope,
        change: &StructuralChange,
    ) -> Option<String>;
}

/// Where a 1-based position goes when `edit` inserts or deletes on its axis: `None` when it is
/// deleted or pushed past the grid's edge.
pub fn moved_position(edit: AxisEdit, index: u32) -> Option<u32> {
    let max = edit.axis().size();
    match edit.change() {
        AxisChange::Insert => {
            if index < edit.first() {
                Some(index)
            } else {
                index
                    .checked_add(edit.count())
                    .filter(|moved| *moved <= max)
            }
        }
        AxisChange::Delete => {
            if index < edit.first() {
                Some(index)
            } else if index <= edit.last() {
                None
            } else {
                Some(index - edit.count())
            }
        }
    }
}

/// Where a run of positions goes: grown by positions inserted inside it, shrunk by those deleted
/// from it, cut at the grid's edge; `None` when all of it is deleted or pushed past the edge.
pub fn moved_run(edit: AxisEdit, start: u32, end: u32) -> Option<(u32, u32)> {
    let max = edit.axis().size();
    match edit.change() {
        AxisChange::Insert => {
            let before = edit.first();
            if before > end {
                Some((start, end))
            } else if before <= start {
                let moved = start.checked_add(edit.count())?;
                (moved <= max).then(|| (moved, end.saturating_add(edit.count()).min(max)))
            } else {
                Some((start, end.saturating_add(edit.count()).min(max)))
            }
        }
        AxisChange::Delete => {
            let (first, last) = (edit.first(), edit.last());
            if last < start {
                return Some((start - edit.count(), end - edit.count()));
            }
            if first > end {
                return Some((start, end));
            }
            let overlap = end.min(last) - start.max(first) + 1;
            let length = end - start + 1;
            if overlap == length {
                return None;
            }
            let new_start = start.min(first);
            Some((new_start, new_start + (length - overlap) - 1))
        }
    }
}

/// The cell `at` moves to under `edit`, `None` when it is deleted or pushed off the grid.
pub fn moved_cell(edit: AxisEdit, at: CellKey) -> Option<CellKey> {
    if at.sheet() != edit.sheet() {
        return Some(at);
    }
    let (row, column) = match edit.axis() {
        Axis::Rows => (moved_position(edit, at.row())?, at.column()),
        Axis::Columns => (at.row(), moved_position(edit, at.column())?),
    };
    CellKey::new(at.sheet(), row, column).ok()
}

/// The area `area` covers under `edit`, `None` when all of it is deleted.
pub fn moved_area(edit: AxisEdit, area: GridArea) -> Option<GridArea> {
    if area.sheet() != edit.sheet() {
        return Some(area);
    }
    let (first, last) = (area.first(), area.last());
    let ((top, bottom), (left, right)) = match edit.axis() {
        Axis::Rows => (
            moved_run(edit, first.row(), last.row())?,
            (first.column(), last.column()),
        ),
        Axis::Columns => (
            (first.row(), last.row()),
            moved_run(edit, first.column(), last.column())?,
        ),
    };
    GridArea::new(
        CellKey::new(area.sheet(), top, left).ok()?,
        CellKey::new(area.sheet(), bottom, right).ok()?,
    )
    .ok()
}

/// A root's contents taken apart for a structural change.
pub(crate) struct Parts {
    pub(crate) sheets: Vec<SheetEntry>,
    /// Each sheet's cells, by sheet ID.
    pub(crate) cells: BTreeMap<SheetId, BTreeMap<CellKey, CellInput>>,
    pub(crate) layout: crate::layout::SkinLayout,
    pub(crate) definitions: Definitions,
}

impl ModelRoot {
    /// The root that applying `batch` makes when it holds structural edits: as
    /// [`ModelRoot::apply`] does for cell and layout edits, and also inserting and deleting rows
    /// and columns ([`WorkbookOp::EditAxis`]) and inserting, renaming, moving, removing and showing
    /// or hiding sheets ([`WorkbookOp::EditSheet`]). Every formula's and defined name's references
    /// are rewritten by `rewriter`, against the root each operation applies to; cells, array
    /// formulas, tables, unloaded cells, hidden rows and the layout's block anchors move with the
    /// cells, and those of a removed sheet go. A sheet an insert adds takes the ID `issue_sheet`
    /// gives.
    ///
    /// Refused, with nothing made, as [`ModelRoot::apply`] refuses, and when a sheet edit names a
    /// sheet the root does not hold. A batch with no structural edit gives the root `apply`
    /// gives.
    pub fn apply_structural(
        &self,
        batch: &OpBatch,
        rewriter: &dyn FormulaRewriter,
        issue_sheet: &mut dyn FnMut() -> SheetId,
    ) -> Result<Arc<ModelRoot>, ApplyError> {
        let structural = batch
            .operations
            .iter()
            .any(|op| matches!(op, WorkbookOp::EditAxis(_) | WorkbookOp::EditSheet(_)));
        if !structural {
            return self.apply(batch);
        }
        if batch.base != self.revision() {
            return Err(ApplyError::StaleBase {
                batch: batch.base,
                root: self.revision(),
            });
        }
        let revision = self
            .revision()
            .checked_next()
            .ok_or(ApplyError::RevisionExhausted)?;
        let mut current: Arc<ModelRoot> = self.rebuilt(self.parts(), self.revision());
        for (operation, op) in batch.operations.iter().enumerate() {
            let next = match op {
                WorkbookOp::EditAxis(edit) => {
                    current.require_sheet(operation, edit.sheet())?;
                    current.axis_edit(*edit, rewriter)
                }
                WorkbookOp::EditSheet(edit) => {
                    current.sheet_edit(operation, edit, rewriter, issue_sheet)?
                }
                other => {
                    // One cell or layout edit, applied as `apply` applies it, at this revision.
                    let single = OpBatch {
                        base: current.revision(),
                        operations: vec![other.clone()],
                    };
                    let applied = current.apply(&single).map_err(|error| match error {
                        ApplyError::UnknownSheet { sheet, .. } => {
                            ApplyError::UnknownSheet { operation, sheet }
                        }
                        ApplyError::Layout { error, .. } => ApplyError::Layout { operation, error },
                        other => other,
                    })?;
                    applied.rebuilt(applied.parts(), current.revision())
                }
            };
            current = next;
        }
        Ok(current.rebuilt(current.parts(), revision))
    }

    fn require_sheet(&self, operation: usize, sheet: SheetId) -> Result<(), ApplyError> {
        match self.sheet_by_id(sheet) {
            Some(_) => Ok(()),
            None => Err(ApplyError::UnknownSheet { operation, sheet }),
        }
    }

    /// Every formula of the root rewritten for `change`.
    fn rewritten_cells(
        &self,
        rewriter: &dyn FormulaRewriter,
        change: &StructuralChange,
        mut cells: BTreeMap<SheetId, BTreeMap<CellKey, CellInput>>,
    ) -> BTreeMap<SheetId, BTreeMap<CellKey, CellInput>> {
        for sheet_cells in cells.values_mut() {
            for (at, input) in sheet_cells.iter_mut() {
                if let CellInput::Formula(text) = input
                    && let Some(rewritten) = rewriter.formula(self, text, *at, change)
                {
                    *text = rewritten;
                }
            }
        }
        cells
    }

    /// Every defined name's formula rewritten for `change`.
    fn rewritten_names(
        &self,
        rewriter: &dyn FormulaRewriter,
        change: &StructuralChange,
        names: &[DefinedName],
    ) -> Vec<DefinedName> {
        names
            .iter()
            .map(|name| {
                let mut name = name.clone();
                if let Some(formula) = rewriter.name(self, &name.formula, name.scope, change) {
                    name.formula = formula;
                }
                name
            })
            .collect()
    }

    fn axis_edit(&self, edit: AxisEdit, rewriter: &dyn FormulaRewriter) -> Arc<ModelRoot> {
        let change = StructuralChange::Axis(edit);
        let parts = self.parts();
        let cells = self.rewritten_cells(rewriter, &change, parts.cells);
        let cells = cells
            .into_iter()
            .map(|(sheet, held)| {
                let moved = held
                    .into_iter()
                    .filter_map(|(at, input)| Some((moved_cell(edit, at)?, input)))
                    .collect();
                (sheet, moved)
            })
            .collect();
        let definitions = &parts.definitions;
        let arrays = definitions.arrays().filter_map(|array| {
            moved_cell(edit, array.anchor())?;
            Some(ArrayFormula::new(
                moved_area(edit, array.area())?,
                array.kind(),
            ))
        });
        let names = self.rewritten_names(rewriter, &change, definitions.names());
        let tables = definitions
            .tables()
            .iter()
            .filter_map(|table| {
                let area = moved_area(edit, table.area)?;
                Some(Table {
                    area,
                    ..table.clone()
                })
            })
            .collect();
        let unloaded: Vec<_> = definitions
            .unloaded_cells()
            .filter_map(|(at, why)| Some((moved_cell(edit, at)?, why.clone())))
            .collect();
        let hidden = definitions
            .hidden_rows_all()
            .filter_map(|(sheet, row, why)| {
                if sheet == edit.sheet() && edit.axis() == Axis::Rows {
                    Some((sheet, moved_position(edit, row)?, why))
                } else {
                    Some((sheet, row, why))
                }
            });
        let next = Definitions::new(arrays, names, tables, unloaded)
            .with_hidden_rows(hidden.collect::<Vec<_>>())
            .with_macro_enabled(definitions.macro_enabled())
            .with_iteration(definitions.iteration());
        let layout = parts.layout.with_anchors(|area| moved_area(edit, area));
        self.rebuilt(
            Parts {
                sheets: parts.sheets,
                cells,
                layout,
                definitions: next,
            },
            self.revision(),
        )
    }

    fn sheet_edit(
        &self,
        operation: usize,
        edit: &SheetEdit,
        rewriter: &dyn FormulaRewriter,
        issue_sheet: &mut dyn FnMut() -> SheetId,
    ) -> Result<Arc<ModelRoot>, ApplyError> {
        let mut parts = self.parts();
        let position_of = |parts: &Parts, sheet: SheetId| {
            parts
                .sheets
                .iter()
                .position(|entry| entry.id() == sheet)
                .ok_or(ApplyError::UnknownSheet { operation, sheet })
        };
        match edit {
            SheetEdit::Insert {
                key,
                name,
                position,
            } => {
                let id = issue_sheet();
                let at = (position.get() as usize - 1).min(parts.sheets.len());
                parts.sheets.insert(
                    at,
                    SheetEntry::new(id, *key, name.clone(), SheetVisibility::Visible),
                );
                parts.cells.insert(id, BTreeMap::new());
            }
            SheetEdit::Rename { sheet, name } => {
                let at = position_of(&parts, *sheet)?;
                let change = StructuralChange::RenameSheet {
                    sheet: *sheet,
                    name: name.clone(),
                };
                parts.cells = self.rewritten_cells(rewriter, &change, parts.cells);
                let names = self.rewritten_names(rewriter, &change, parts.definitions.names());
                parts.definitions = parts.definitions.with_names(names);
                let entry = &parts.sheets[at];
                parts.sheets[at] =
                    SheetEntry::new(entry.id(), entry.key(), name.clone(), entry.visibility());
            }
            SheetEdit::Move { sheet, position } => {
                let at = position_of(&parts, *sheet)?;
                let entry = parts.sheets.remove(at);
                let to = (position.get() as usize - 1).min(parts.sheets.len());
                parts.sheets.insert(to, entry);
            }
            SheetEdit::Remove { sheet } => {
                let at = position_of(&parts, *sheet)?;
                let change = StructuralChange::RemoveSheet { sheet: *sheet };
                parts.cells = self.rewritten_cells(rewriter, &change, parts.cells);
                parts.cells.remove(sheet);
                let names = self.rewritten_names(rewriter, &change, parts.definitions.names());
                parts.definitions = parts.definitions.with_names(names).without_sheet(*sheet);
                parts.sheets.remove(at);
                parts.layout = parts
                    .layout
                    .with_anchors(|area| (area.sheet() != *sheet).then_some(area));
            }
            SheetEdit::SetVisibility { sheet, visibility } => {
                let at = position_of(&parts, *sheet)?;
                let entry = &parts.sheets[at];
                parts.sheets[at] =
                    SheetEntry::new(entry.id(), entry.key(), entry.name().clone(), *visibility);
            }
        }
        Ok(self.rebuilt(parts, self.revision()))
    }
}

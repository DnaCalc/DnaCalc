//! Commands: translating a cell-editing command into one authored batch, preparing the
//! document's and the engine's share of it against one base revision, and committing both as one
//! transaction (plan 3.3).

use crate::state::HostState;
use calc_doc::EditError;
use calc_doc_model::value::{CellKey, GridArea};
use calc_doc_model::{
    ArrayFormula, AuthoredRevision, AxisEdit, CellInput, FormulaText, ModelRoot, OpBatch,
    WorkbookOp,
};
use calc_engine::{CalcTick, FormulaNode, InputSeed, NodeChange, PrepareError};
use std::fmt;
use std::sync::Arc;

/// One edit of a cell-editing command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellEdit {
    /// Changes a worksheet window's frozen leading bands without changing cells.
    FrozenPanes {
        sheet: calc_doc_model::value::SheetId,
        panes: Option<calc_doc_model::FrozenPanes>,
    },
    /// Sets a cell of an existing sheet to a value or a formula.
    Set { at: CellKey, input: CellInput },
    /// Removes the contents of every cell of an area of an existing sheet.
    Clear { area: GridArea },
    /// Authors an array formula: the rest of the array's area is emptied and its anchor holds
    /// the formula ([`calc_doc_model::WorkbookOp::SetArray`]). A dynamic array over its anchor
    /// alone is what Excel stores for a formula entered into one cell that can return an array;
    /// a legacy array is a Ctrl+Shift+Enter formula over its area.
    SetArray {
        array: ArrayFormula,
        formula: FormulaText,
    },
    /// Inserts or deletes rows or columns of a sheet ([`calc_doc::prepare_structural_edit`]):
    /// the cells and what moves with them shift, and every formula's and defined name's
    /// references adjust as Excel's do. A command holding one holds only axis edits.
    Axis(AxisEdit),
    /// Gives a cell a cell format: an index into the workbook's cell formats (a cell's `s`), or
    /// `None` to return it to the format the file gave it ([`calc_doc_model::WorkbookOp::SetFormat`],
    /// dc-09v). It changes no input and no value.
    SetFormat { at: CellKey, format: Option<u32> },
    /// Gives the defined name `name` of `scope` the formula `formula`
    /// ([`calc_doc_model::WorkbookOp::SetNameFormula`], dc-gf0). A command that holds one sets
    /// every formula that uses the name again, so the engine binds it to the new formula.
    SetName {
        name: String,
        scope: calc_doc_model::NameScope,
        /// `None` removes the name; a name the workbook does not hold is defined.
        formula: Option<String>,
    },
    /// Sets the workbook's iterative-calculation settings
    /// ([`calc_doc_model::WorkbookOp::SetIteration`], dc-y2o.2): Excel's "Enable iterative
    /// calculation", "Maximum Iterations" and "Maximum Change", which the workbook saves in its
    /// `calcPr`. The formulas on a circular reference are calculated again under them.
    SetIteration(calc_doc_model::IterationSettings),
}

impl CellEdit {
    fn operation(self) -> WorkbookOp {
        match self {
            CellEdit::FrozenPanes { sheet, panes } => WorkbookOp::SetFrozenPanes { sheet, panes },
            CellEdit::Set { at, input } => WorkbookOp::SetCell { at, input },
            CellEdit::Clear { area } => WorkbookOp::Clear { area },
            CellEdit::SetArray { array, formula } => WorkbookOp::SetArray {
                array,
                input: CellInput::Formula(formula),
            },
            CellEdit::Axis(edit) => WorkbookOp::EditAxis(edit),
            CellEdit::SetFormat { at, format } => WorkbookOp::SetFormat { at, format },
            CellEdit::SetName {
                name,
                scope,
                formula,
            } => WorkbookOp::SetNameFormula {
                name,
                scope,
                formula,
            },
            CellEdit::SetIteration(settings) => WorkbookOp::SetIteration(settings),
        }
    }
}

/// A cell-editing command: its edits, applied in order as one transaction, and the authored
/// revision the caller prepared them against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCommand {
    pub base: AuthoredRevision,
    pub edits: Vec<CellEdit>,
}

/// What a committed command published: the one authored revision it made and its dirty set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandReceipt {
    base: AuthoredRevision,
    revision: AuthoredRevision,
    changed_cells: Vec<CellKey>,
    cleared_areas: Vec<GridArea>,
    pending: Vec<CellKey>,
    cancelled_recalc: Option<CalcTick>,
}

impl CommandReceipt {
    /// The revision the command was based on.
    pub fn base(&self) -> AuthoredRevision {
        self.base
    }

    /// The authored revision the command made, which the engine state is also at.
    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    /// The cells whose input a set command changed, in the order of the cells (sheet ID, then row,
    /// then column). A cell set to the input it already held is not among them, and the cells a
    /// clear emptied are in [`CommandReceipt::cleared_areas`], not here.
    pub fn changed_cells(&self) -> &[CellKey] {
        &self.changed_cells
    }

    /// The cleared areas that emptied at least one cell holding an input, in command order.
    pub fn cleared_areas(&self) -> &[GridArea] {
        &self.cleared_areas
    }

    /// The formula cells whose results the next recalculation calculates from (the anchors of
    /// the engine's pending nodes after the commit), in the order of the cells: each formula the
    /// command added or replaced, each formula reading a changed cell, and each formula already
    /// pending before it. The formulas reading these are stale too; [`crate::project`] labels
    /// every stale result.
    pub fn pending(&self) -> &[CellKey] {
        &self.pending
    }

    /// The tick of the recalculation in progress that the commit cancelled: it was calculating
    /// the revision before, so nothing it staged is published.
    pub fn cancelled_recalc(&self) -> Option<CalcTick> {
        self.cancelled_recalc
    }
}

/// Why [`dispatch`] committed nothing. The authored revision, the engine state and every cell are
/// as they were before the command.
#[derive(Debug, PartialEq, Eq)]
pub enum CommandError {
    /// The command holds no edit, so it would make no change to commit.
    NoEdits,
    /// The command was prepared against another revision than the current one.
    StaleBase {
        command: AuthoredRevision,
        current: AuthoredRevision,
    },
    /// The document refused the batch (calc-doc's [`calc_doc::prepare_edit`]): a sheet the
    /// workbook does not hold, or a formula a save could not write.
    Document(EditError),
    /// The engine refused the batch after the document prepared it.
    Engine(PrepareError),
    /// The batch adds formula cells and would leave `formula_cells` of them, more than the host's
    /// [`crate::HostOptions::formula_cell_budget`]; the node IDs the engine reserved for them are
    /// released.
    FormulaBudget { formula_cells: usize, budget: usize },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NoEdits => f.write_str("the command holds no edit"),
            CommandError::StaleBase { command, current } => write!(
                f,
                "the command was prepared against revision {}, the workbook is at {}",
                command.get(),
                current.get()
            ),
            CommandError::Document(error) => write!(f, "the document refused the edit: {error}"),
            CommandError::Engine(error) => write!(f, "the engine refused the edit: {error}"),
            CommandError::FormulaBudget {
                formula_cells,
                budget,
            } => write!(
                f,
                "the edit would leave {formula_cells} formula cells, more than the budget of {budget}"
            ),
        }
    }
}

impl std::error::Error for CommandError {}

/// Commits `command` to the workbook as one transaction, or refuses it with nothing changed.
///
/// 1. A command with no edit is refused, and so is a command based on another revision than the
///    current one.
/// 2. Its edits become one batch, in order, based on the current revision.
/// 3. The document's share is prepared ([`calc_doc::prepare_edit`]): the next revision, made
///    beside the current one, which stays as it is.
/// 4. The engine's share is prepared against the same base ([`calc_engine::prepare_edit`]): its
///    node changes and input seeds, with the IDs of the nodes it adds reserved.
/// 5. The reserved nodes are admitted against the host's formula-cell budget, and the receipt's
///    dirty set is reserved.
/// 6. Both are committed: the engine's change ([`calc_engine::commit_edit`], which returns no
///    error) and the adoption of the prepared revision. From here on nothing returns an error or
///    grows a host buffer past what step 5 reserved. A recalculation in progress, which was
///    calculating the revision before, is cancelled. When the batch changed a cell's input or
///    formula (the engine staged a seed or a node change), the host records that the inputs
///    changed since the workbook was opened, which [`crate::project`] reads for file caches.
///
/// A refusal at steps 1 to 5 drops whatever was prepared; dropping the engine's change releases
/// its reserved node IDs.
pub fn dispatch(
    host: &mut HostState,
    command: HostCommand,
) -> Result<CommandReceipt, CommandError> {
    if command.edits.is_empty() {
        return Err(CommandError::NoEdits);
    }
    let current = host.model.revision();
    if command.base != current {
        return Err(CommandError::StaleBase {
            command: command.base,
            current,
        });
    }
    let batch = OpBatch {
        base: command.base,
        operations: command.edits.into_iter().map(CellEdit::operation).collect(),
    };

    let axis_edits: Vec<AxisEdit> = batch
        .operations
        .iter()
        .filter_map(|op| match op {
            WorkbookOp::EditAxis(edit) => Some(*edit),
            _ => None,
        })
        .collect();
    let document = if axis_edits.is_empty() {
        calc_doc::prepare_edit(&host.model, &batch)
    } else {
        calc_doc::prepare_structural_edit(&host.model, &batch, &calc_engine::ReferenceRewriter)
    }
    .map_err(CommandError::Document)?;

    let formula_cells_before = host.engine.node_count();
    let pending_before = host.engine.pending().count();
    let engine = calc_engine::prepare_edit(&mut host.engine, &host.model, &batch)
        .map_err(CommandError::Engine)?;

    let changes_inputs = !engine.seeds().is_empty() || !engine.node_changes().is_empty();
    let (mut added, mut replaced, mut removed) = (0usize, 0usize, 0usize);
    for change in engine.node_changes() {
        match change {
            NodeChange::Added { .. } => added += 1,
            NodeChange::Replaced { .. } => replaced += 1,
            NodeChange::Removed { .. } => removed += 1,
        }
    }
    let formula_cells = formula_cells_before - removed + added;
    if let Some(budget) = host.options.formula_cell_budget
        && added > removed
        && formula_cells > budget
    {
        return Err(CommandError::FormulaBudget {
            formula_cells,
            budget,
        });
    }

    let mut changed_cells = Vec::new();
    let mut cleared_areas = Vec::new();
    let mut seeded = 0usize;
    for seed in engine.seeds() {
        match seed {
            InputSeed::Cell { at, dependents, .. } => {
                changed_cells.push(*at);
                seeded += dependents.len();
            }
            InputSeed::Area { area, dependents } => {
                cleared_areas.push(*area);
                seeded += dependents.len();
            }
            // The cells a removed array formula filled change though no input of theirs did;
            // the anchor's own cell is among the changed or cleared cells already.
            InputSeed::Spill { dependents, .. } => seeded += dependents.len(),
        }
    }
    // After the commit the pending nodes are those pending before, the added and replaced nodes
    // and the seeds' dependents, less the removed ones: at most this many.
    // A structural edit leaves every node pending.
    let mut pending = Vec::with_capacity(if axis_edits.is_empty() {
        pending_before + added + replaced + seeded
    } else {
        formula_cells
    });

    let committed = calc_engine::commit_edit(engine);
    host.model = document.into_model();
    let cancelled_recalc = host.work.take().map(|work| work.tick());
    if changes_inputs || !axis_edits.is_empty() {
        host.inputs_changed_since_open = true;
    }
    host.axis_edits.extend(axis_edits);

    for node in host.engine.pending() {
        if let Some(record) = host.engine.node(node) {
            pending.push(FormulaNode::anchor(record));
        }
    }
    pending.sort_unstable();
    Ok(CommandReceipt {
        base: command.base,
        revision: committed.revision(),
        changed_cells,
        cleared_areas,
        pending,
        cancelled_recalc,
    })
}

/// Brings back the revision `earlier` (of this host's lineage) as the next revision
/// ([`calc_doc_model::ModelRoot::restored`]), with the engine state built afresh for it
/// ([`calc_engine::EngineState::rebuilt`]: every formula pending, nothing published) and
/// `axis_edits` as the axis edits since the workbook was opened that lead to it. This undoes an
/// edit no batch inverts, a structural one. A recalculation in progress is cancelled.
pub(crate) fn restore(
    host: &mut HostState,
    earlier: &Arc<ModelRoot>,
    axis_edits: Vec<AxisEdit>,
) -> Result<CommandReceipt, CommandError> {
    let base = host.model.revision();
    let model = host
        .model
        .restored(earlier)
        .map_err(|error| CommandError::Document(EditError::Model(error)))?;
    let engine = host.engine.rebuilt(&model);
    host.model = model;
    host.engine = engine;
    let cancelled_recalc = host.work.take().map(|work| work.tick());
    host.inputs_changed_since_open = true;
    host.axis_edits = axis_edits;
    let mut pending: Vec<CellKey> = host
        .engine
        .pending()
        .filter_map(|node| host.engine.node(node).map(FormulaNode::anchor))
        .collect();
    pending.sort_unstable();
    Ok(CommandReceipt {
        base,
        revision: host.model.revision(),
        changed_cells: Vec::new(),
        cleared_areas: Vec::new(),
        pending,
        cancelled_recalc,
    })
}

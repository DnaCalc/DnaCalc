//! Preparing and committing the engine's share of an authored edit (plan 3.3): `prepare_edit`
//! reads the state and the base snapshot, stages the edit's effects on the graph and reserves the
//! IDs of the nodes it adds, changing no node or edge; `commit_edit` applies the prepared change
//! and returns no error.

use crate::state::{EngineState, issue_node_id};
use calc_doc_model::{AuthoredRevision, CellInput, ModelRoot, OpBatch, SheetEdit, WorkbookOp};
use calc_value::{CellKey, GridArea, NodeId, SheetId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A change to the graph's formula nodes that an edit stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeChange {
    /// A formula enters a cell that held none. Its ID is reserved when the change is prepared.
    Added { node: NodeId, at: CellKey },
    /// The cell's formula is set again, to the same text or another. The node keeps its ID and
    /// loses its prerequisites until the new formula's plan is bound.
    Replaced { node: NodeId, at: CellKey },
    /// The cell no longer holds a formula. The node, its edges and its ID go when the change
    /// commits.
    Removed { node: NodeId, at: CellKey },
}

/// An authored input the edit changes, with the nodes that read it directly. Dependents are the
/// nodes whose value edges cover a changed cell, in ID order, without the nodes the same edit
/// replaces or removes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputSeed<'r> {
    /// A cell the batch sets to an input other than the one it holds in the base snapshot.
    /// `before` is that input, borrowed from the snapshot's page (`None` for an empty cell).
    Cell {
        at: CellKey,
        before: Option<&'r CellInput>,
        dependents: Vec<NodeId>,
    },
    /// A cleared area that empties at least one cell holding an input in the base snapshot. Its
    /// dependents are the nodes reading a cell of the area that holds an input in the snapshot
    /// and none after the batch; a node that reads only cells of the area that were already empty
    /// is not among them.
    Area {
        area: GridArea,
        dependents: Vec<NodeId>,
    },
    /// The cells the published spill of the array formula at `anchor`, which the edit removes,
    /// filled, when that is not the file's area of the array formula (which an `Area` seed
    /// covers). Its dependents are the nodes reading a cell of `area`, which no longer reads as
    /// the formula's result.
    Spill {
        anchor: CellKey,
        area: GridArea,
        dependents: Vec<NodeId>,
    },
}

/// Why [`prepare_edit`] refused a batch. A refused batch stages and reserves nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrepareError {
    /// The batch, the engine state and the input snapshot are not all at one revision.
    StaleBase {
        batch: AuthoredRevision,
        engine: AuthoredRevision,
        inputs: AuthoredRevision,
    },
    /// The batch's base is the last revision the counter holds, so the edit has no next one.
    RevisionExhausted,
    /// The operation at this index of the batch names a sheet the snapshot does not hold.
    UnknownSheet { operation: usize, sheet: SheetId },
    /// The operation at this index inserts or deletes rows or columns, or inserts, renames,
    /// moves or removes a sheet. The graph's structural patches (moving anchors and edges,
    /// deleting the nodes of a removed sheet) belong to dc-m3u; until then such a batch is
    /// refused rather than leaving anchors and edges at their old places.
    StructuralEdit { operation: usize },
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrepareError::StaleBase {
                batch,
                engine,
                inputs,
            } => write!(
                f,
                "the batch is based on revision {}, the engine state is at {} and the input snapshot at {}",
                batch.get(),
                engine.get(),
                inputs.get()
            ),
            PrepareError::RevisionExhausted => {
                f.write_str("the batch's base revision has no next revision")
            }
            PrepareError::UnknownSheet { operation, sheet } => write!(
                f,
                "operation {operation} names sheet {sheet:?}, which the input snapshot does not hold"
            ),
            PrepareError::StructuralEdit { operation } => write!(
                f,
                "operation {operation} is a structural edit; the engine's structural patches are dc-m3u's"
            ),
        }
    }
}

impl std::error::Error for PrepareError {}

/// The engine's staged share of one authored batch: the node changes and the cell seeds in anchor
/// order, then the area seeds in operation order. It holds the state it was prepared against, so
/// nothing else changes that state until it is committed or dropped. The IDs of the nodes it adds
/// are reserved in the state's allocator; dropping the change without committing it releases
/// them.
#[must_use = "a prepared change applies nothing until commit_edit, and dropping it releases its node IDs"]
#[derive(Debug)]
pub struct PreparedEngineChange<'s, 'r> {
    state: &'s mut EngineState,
    base: AuthoredRevision,
    next: AuthoredRevision,
    node_changes: Vec<NodeChange>,
    seeds: Vec<InputSeed<'r>>,
}

impl<'r> PreparedEngineChange<'_, 'r> {
    /// The revision the change was prepared against.
    pub fn base(&self) -> AuthoredRevision {
        self.base
    }

    /// The revision the state has after the change commits.
    pub fn next(&self) -> AuthoredRevision {
        self.next
    }

    pub fn node_changes(&self) -> &[NodeChange] {
        &self.node_changes
    }

    pub fn seeds(&self) -> &[InputSeed<'r>] {
        &self.seeds
    }
}

impl Drop for PreparedEngineChange<'_, '_> {
    fn drop(&mut self) {
        // `commit_edit` empties the list, so only an uncommitted change releases anything.
        for node_change in &self.node_changes {
            if let NodeChange::Added { node, .. } = *node_change {
                let released = self.state.ids.release(node);
                debug_assert_eq!(
                    released,
                    Ok(()),
                    "a reserved node ID is live in its allocator"
                );
            }
        }
    }
}

/// Stages the effects of `change` on the graph of `state`, reading the base snapshot `inputs`.
/// The batch, the state and the snapshot must be at one revision. Operations apply in order: a
/// later operation on a cell sees what an earlier one of the batch left there. What a cell holds
/// before the batch is read from the snapshot's page, borrowed and never copied.
///
/// - A cell set to a formula adds a node, or replaces the node already there; a cell set to a
///   literal, or cleared, removes its node.
/// - A cell whose input after the batch differs from its input in the snapshot seeds the nodes
///   that read it; a cell set to the literal it already holds changes no calculation input and
///   seeds nothing.
/// - A cleared area that empties a cell holding an input in the snapshot seeds the nodes that read
///   a cell it empties, and only those: a node that reads only cells of the area that were
///   already empty is not seeded by the clear. Clearing cells that are already empty seeds
///   nothing.
/// - A cell whose array formula the batch sets or clears seeds the nodes that read a cell of the
///   file's area of the array formula ([`InputSeed::Area`]) and, when the formula is removed, of
///   the area its published spill fills ([`InputSeed::Spill`]): those cells no longer show its
///   result.
/// - Setting a sheet's visibility changes no calculation input and stages nothing, and neither
///   does a skin layout edit.
/// - A structural edit is refused ([`PrepareError::StructuralEdit`]).
///
/// Every refusal comes before anything is reserved. Then the IDs of the added nodes are issued, in
/// anchor order, and held by the returned change.
pub fn prepare_edit<'s, 'r>(
    state: &'s mut EngineState,
    inputs: &'r ModelRoot,
    change: &OpBatch,
) -> Result<PreparedEngineChange<'s, 'r>, PrepareError> {
    let engine = state.revision();
    if change.base != engine || inputs.revision() != engine {
        return Err(PrepareError::StaleBase {
            batch: change.base,
            engine,
            inputs: inputs.revision(),
        });
    }
    let next = change
        .base
        .checked_next()
        .ok_or(PrepareError::RevisionExhausted)?;

    // What each cell the batch can change holds after the batch (`None` for empty). A cleared
    // area adds every cell of it that holds an input in the snapshot or a formula node in the
    // state, so a cell is missing here only when the batch leaves it as the snapshot has it.
    let mut after: BTreeMap<CellKey, Option<&CellInput>> = BTreeMap::new();
    let mut cleared = Vec::new();
    for (operation, op) in change.operations.iter().enumerate() {
        match op {
            WorkbookOp::SetCell { at, input } => {
                require_sheet(inputs, operation, at.sheet())?;
                after.insert(*at, Some(input));
            }
            WorkbookOp::Clear { area } => {
                require_sheet(inputs, operation, area.sheet())?;
                for (at, _) in inputs.cells(*area).into_iter().flatten() {
                    after.insert(at, None);
                }
                for at in state.anchors_in(*area) {
                    after.insert(at, None);
                }
                for (at, input) in after.range_mut(area.first()..=area.last()) {
                    if area.contains(*at) {
                        *input = None;
                    }
                }
                cleared.push(*area);
            }
            WorkbookOp::EditAxis(_) => return Err(PrepareError::StructuralEdit { operation }),
            WorkbookOp::EditSheet(edit) => match edit {
                SheetEdit::SetVisibility { sheet, .. } => {
                    require_sheet(inputs, operation, *sheet)?;
                }
                SheetEdit::Insert { .. }
                | SheetEdit::Rename { .. }
                | SheetEdit::Move { .. }
                | SheetEdit::Remove { .. } => {
                    return Err(PrepareError::StructuralEdit { operation });
                }
            },
            WorkbookOp::EditSkinLayout(_) => {}
        }
    }

    let mut adding = Vec::new();
    let mut kept = Vec::new();
    let mut leaving = BTreeSet::new();
    let mut set_cells = Vec::new();
    let mut emptied = BTreeSet::new();
    for (&at, &input) in &after {
        let before = inputs.cell(at);
        let formula = matches!(input, Some(CellInput::Formula(_)));
        match (state.node_at(at), formula) {
            (None, true) => adding.push(at),
            (Some(node), true) => {
                leaving.insert(node);
                kept.push(NodeChange::Replaced { node, at });
            }
            (Some(node), false) => {
                leaving.insert(node);
                kept.push(NodeChange::Removed { node, at });
            }
            (None, false) => {}
        }
        if before != input {
            match input {
                Some(_) => set_cells.push((at, before)),
                None => {
                    emptied.insert(at);
                }
            }
        }
    }

    let mut seeds: Vec<InputSeed<'r>> = set_cells
        .into_iter()
        .map(|(at, before)| InputSeed::Cell {
            at,
            before,
            dependents: state.dependents_of_cell(inputs, at, &leaving),
        })
        .collect();
    for area in cleared {
        let empties: Vec<CellKey> = emptied
            .range(area.first()..=area.last())
            .copied()
            .filter(|at| area.contains(*at))
            .collect();
        if !empties.is_empty() {
            seeds.push(InputSeed::Area {
                area,
                dependents: state.dependents_of_emptied(inputs, area, &empties, &leaving),
            });
        }
    }

    // An array formula the batch removes also fills, beside its anchor, the area of its published
    // spill, which the engine decided and the file's area may not hold: the nodes reading those
    // cells are seeded too (the file's area is seeded below).
    for change in &kept {
        let NodeChange::Removed { at, .. } = *change else {
            continue;
        };
        let Some(area) = state.spills.get(at).map(|spill| spill.area()) else {
            continue;
        };
        if inputs
            .definitions()
            .array(at)
            .is_some_and(|array| array.area() == area)
        {
            continue;
        }
        let mut readers = BTreeSet::new();
        state.index.dependents_of_area(inputs, area, &mut readers);
        readers.remove(
            &state
                .node_at(at)
                .expect("a removed node is live until it commits"),
        );
        let dependents: Vec<NodeId> = readers.difference(&leaving).copied().collect();
        if !dependents.is_empty() {
            seeds.push(InputSeed::Spill {
                anchor: at,
                area,
                dependents,
            });
        }
    }

    // An array formula whose anchor the batch sets or clears is gone after it (the model drops
    // it), so the cells it filled beside its anchor, which read as not available, read as what
    // they hold from then on: the nodes reading them are seeded like readers of emptied cells.
    for array in inputs.definitions().arrays() {
        if !after.contains_key(&array.anchor()) {
            continue;
        }
        let area = array.area();
        let filled: Vec<CellKey> = (area.first().row()..=area.last().row())
            .flat_map(|row| {
                (area.first().column()..=area.last().column())
                    .map(move |column| CellKey::new(area.sheet(), row, column))
            })
            .filter_map(Result::ok)
            .filter(|at| *at != array.anchor())
            .collect();
        if !filled.is_empty() {
            seeds.push(InputSeed::Area {
                area,
                dependents: state.dependents_of_emptied(inputs, area, &filled, &leaving),
            });
        }
    }

    let mut node_changes: Vec<NodeChange> = adding
        .into_iter()
        .map(|at| NodeChange::Added {
            node: issue_node_id(&mut state.ids),
            at,
        })
        .chain(kept)
        .collect();
    node_changes.sort_by_key(|node_change| match *node_change {
        NodeChange::Added { at, .. }
        | NodeChange::Replaced { at, .. }
        | NodeChange::Removed { at, .. } => at,
    });
    Ok(PreparedEngineChange {
        state,
        base: change.base,
        next,
        node_changes,
        seeds,
    })
}

fn require_sheet(inputs: &ModelRoot, operation: usize, sheet: SheetId) -> Result<(), PrepareError> {
    match inputs.sheet_by_id(sheet) {
        Some(_) => Ok(()),
        None => Err(PrepareError::UnknownSheet { operation, sheet }),
    }
}

/// What a committed change did to the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitReceipt {
    revision: AuthoredRevision,
    added: Vec<(CellKey, NodeId)>,
    replaced: Vec<NodeId>,
    removed: Vec<NodeId>,
}

impl CommitReceipt {
    /// The state's revision after the commit.
    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    /// Each added node with its anchor, in anchor order.
    pub fn added(&self) -> &[(CellKey, NodeId)] {
        &self.added
    }

    /// The nodes whose formulas were replaced, now unbound.
    pub fn replaced(&self) -> &[NodeId] {
        &self.replaced
    }

    /// The removed nodes, whose IDs are no longer live.
    pub fn removed(&self) -> &[NodeId] {
        &self.removed
    }
}

/// Applies a change [`prepare_edit`] staged: the replaced nodes are unbound, the removed ones
/// deleted with their edges and IDs, the added ones recorded unbound under the IDs reserved for
/// them, and the state moves to the change's next revision. The added and replaced nodes and
/// every seed's dependents become pending, to be calculated by the next recalculation; a removed
/// node's published result stays until the next publication removes it. Nothing can change the
/// state between preparing and committing, because the change holds it, so there is no refusal
/// to return.
pub fn commit_edit(mut change: PreparedEngineChange<'_, '_>) -> CommitReceipt {
    let node_changes = std::mem::take(&mut change.node_changes);
    let state = &mut *change.state;
    let mut added = Vec::new();
    let mut replaced = Vec::new();
    let mut removed = Vec::new();
    for node_change in node_changes {
        match node_change {
            NodeChange::Added { node, at } => {
                state.add_node(node, at);
                state.pending.insert(node);
                added.push((at, node));
            }
            NodeChange::Replaced { node, .. } => {
                state.unbind_node(node);
                replaced.push(node);
            }
            NodeChange::Removed { node, .. } => {
                state.remove_node(node);
                removed.push(node);
            }
        }
    }
    for seed in &change.seeds {
        match seed {
            InputSeed::Cell { dependents, .. }
            | InputSeed::Area { dependents, .. }
            | InputSeed::Spill { dependents, .. } => {
                state.pending.extend(dependents.iter().copied());
            }
        }
    }
    state.set_revision(change.next);
    CommitReceipt {
        revision: change.next,
        added,
        replaced,
        removed,
    }
}

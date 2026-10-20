//! The caller-owned engine state: interned node IDs, the formula-node records and the one
//! workbook graph.

use crate::graph::{Prerequisite, ReverseIndex};
use calc_doc_model::{AuthoredRevision, CellInput, ModelRoot};
use calc_value::{CellKey, GridArea, IdAllocator, NodeId, SheetId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// One formula participant of the graph: its interned ID, the cell it anchors, and what it reads
/// once its plan is bound. It holds no formula text and no value: the authored formula stays in
/// the document model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaNode {
    id: NodeId,
    anchor: CellKey,
    prerequisites: Option<Vec<Prerequisite>>,
}

impl FormulaNode {
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The cell whose formula this node is.
    pub fn anchor(&self) -> CellKey {
        self.anchor
    }

    /// What the node reads, or `None` while its formula's plan is not bound: a node adopted from
    /// a snapshot, or added or replaced by an edit, has no prerequisites until
    /// [`EngineState::set_prerequisites`] states them.
    pub fn prerequisites(&self) -> Option<&[Prerequisite]> {
        self.prerequisites.as_deref()
    }
}

/// Why [`EngineState::set_prerequisites`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrerequisiteError {
    /// The input snapshot is not at the engine state's revision.
    StaleInputs {
        engine: AuthoredRevision,
        inputs: AuthoredRevision,
    },
    /// The node is not live in this state.
    UnknownNode(NodeId),
    /// A prerequisite names a sheet the input snapshot does not hold.
    UnknownSheet(SheetId),
}

impl fmt::Display for PrerequisiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrerequisiteError::StaleInputs { engine, inputs } => write!(
                f,
                "the input snapshot is at revision {}, the engine state at {}",
                inputs.get(),
                engine.get()
            ),
            PrerequisiteError::UnknownNode(node) => {
                write!(f, "node {node:?} is not live in this engine state")
            }
            PrerequisiteError::UnknownSheet(sheet) => {
                write!(f, "the input snapshot holds no sheet {sheet:?}")
            }
        }
    }
}

impl std::error::Error for PrerequisiteError {}

/// The engine's derived state for one workbook, owned by its caller (plan 3.1): node IDs interned
/// from the cells whose formulas they are, the formula-node records, and one graph whose edges may
/// cross sheets. It holds no authored input, no source bytes and no formatting; literal inputs
/// are read through the pages of the immutable snapshot each call borrows.
///
/// A node's ID is issued by the state's own [`IdAllocator`] and keyed by its anchor [`CellKey`],
/// which carries the sheet, so equal rows and columns on two sheets are two nodes. A removed
/// node's ID is released and never equals a later one.
#[derive(Debug)]
pub struct EngineState {
    revision: AuthoredRevision,
    pub(crate) ids: IdAllocator<NodeId>,
    pub(crate) by_anchor: BTreeMap<CellKey, NodeId>,
    /// Node records by ID slot.
    pub(crate) nodes: Vec<Option<FormulaNode>>,
    pub(crate) index: ReverseIndex,
}

impl EngineState {
    /// The state of the snapshot `inputs`, at its revision: one unbound node for each formula
    /// cell, its ID issued in sheet order and then in row-major order, read through the
    /// snapshot's pages. Literal cells get no node and nothing of them is copied.
    pub fn new(inputs: &ModelRoot) -> Self {
        let mut state = EngineState {
            revision: inputs.revision(),
            ids: IdAllocator::new(),
            by_anchor: BTreeMap::new(),
            nodes: Vec::new(),
            index: ReverseIndex::default(),
        };
        for sheet in inputs.sheets() {
            for (at, input) in inputs.sheet_cells(sheet.id()).into_iter().flatten() {
                if let CellInput::Formula(_) = input {
                    let id = issue_node_id(&mut state.ids);
                    state.add_node(id, at);
                }
            }
        }
        state
    }

    /// The authored revision the graph matches.
    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    /// The node whose formula is in the cell `at`.
    pub fn node_at(&self, at: CellKey) -> Option<NodeId> {
        self.by_anchor.get(&at).copied()
    }

    /// The record of a live node.
    pub fn node(&self, id: NodeId) -> Option<&FormulaNode> {
        self.nodes
            .get(id.slot() as usize)
            .and_then(Option::as_ref)
            .filter(|node| node.id == id)
    }

    /// Every node, in the order of their anchors (sheet ID, then row, then column).
    pub fn nodes(&self) -> impl Iterator<Item = &FormulaNode> {
        self.by_anchor.values().filter_map(|id| self.node(*id))
    }

    pub fn node_count(&self) -> usize {
        self.by_anchor.len()
    }

    /// States what a live node reads, replacing what it read before, and files each prerequisite
    /// in the graph's reverse indexes. Its product callers are CalcFml's plan bindings, which no
    /// bead has built yet (each [`Prerequisite`] variant names its own); until then only tests
    /// call it. Every sheet named must be in `inputs`, the snapshot at this state's revision.
    pub fn set_prerequisites(
        &mut self,
        inputs: &ModelRoot,
        node: NodeId,
        prerequisites: Vec<Prerequisite>,
    ) -> Result<(), PrerequisiteError> {
        if inputs.revision() != self.revision {
            return Err(PrerequisiteError::StaleInputs {
                engine: self.revision,
                inputs: inputs.revision(),
            });
        }
        if self.node(node).is_none() {
            return Err(PrerequisiteError::UnknownNode(node));
        }
        if let Some(sheet) = prerequisites
            .iter()
            .flat_map(|prerequisite| prerequisite.sheets())
            .find(|sheet| inputs.sheet_by_id(*sheet).is_none())
        {
            return Err(PrerequisiteError::UnknownSheet(sheet));
        }
        let record = self.nodes[node.slot() as usize]
            .as_mut()
            .filter(|record| record.id == node)
            .ok_or(PrerequisiteError::UnknownNode(node))?;
        if let Some(old) = record.prerequisites.take() {
            self.index.remove(node, &old);
        }
        self.index.insert(node, &prerequisites);
        record.prerequisites = Some(prerequisites);
        Ok(())
    }

    /// The anchors of the nodes inside `area`.
    pub(crate) fn anchors_in(&self, area: GridArea) -> impl Iterator<Item = CellKey> + '_ {
        self.by_anchor
            .range(area.first()..=area.last())
            .map(|(at, _)| *at)
            .filter(move |at| area.contains(*at))
    }

    /// The nodes with a value edge covering `at`, without those in `leaving`, in ID order.
    pub(crate) fn dependents_of_cell(
        &self,
        inputs: &ModelRoot,
        at: CellKey,
        leaving: &BTreeSet<NodeId>,
    ) -> Vec<NodeId> {
        let mut out = BTreeSet::new();
        self.index.dependents_of_cell(inputs, at, &mut out);
        out.difference(leaving).copied().collect()
    }

    /// The nodes with a value edge covering one of `emptied`, the cells of `area` an edit empties,
    /// without those in `leaving`, in ID order. A node that reads only other cells of `area` is
    /// not among them.
    pub(crate) fn dependents_of_emptied(
        &self,
        inputs: &ModelRoot,
        area: GridArea,
        emptied: &[CellKey],
        leaving: &BTreeSet<NodeId>,
    ) -> Vec<NodeId> {
        let mut out = BTreeSet::new();
        self.index
            .dependents_of_cells_in_area(inputs, area, emptied, &mut out);
        out.difference(leaving).copied().collect()
    }

    pub(crate) fn set_revision(&mut self, revision: AuthoredRevision) {
        self.revision = revision;
    }

    /// Removes a live node: its edges, its anchor entry and its record, and releases its ID.
    pub(crate) fn remove_node(&mut self, node: NodeId) {
        if let Some(record) = self
            .nodes
            .get_mut(node.slot() as usize)
            .and_then(|slot| slot.take_if(|record| record.id == node))
        {
            if let Some(prerequisites) = &record.prerequisites {
                self.index.remove(node, prerequisites);
            }
            self.by_anchor.remove(&record.anchor);
            let released = self.ids.release(node);
            debug_assert_eq!(
                released,
                Ok(()),
                "a live node's ID is live in its allocator"
            );
        }
    }

    /// Drops a live node's prerequisites, leaving it unbound.
    pub(crate) fn unbind_node(&mut self, node: NodeId) {
        if let Some(record) = self
            .nodes
            .get_mut(node.slot() as usize)
            .and_then(Option::as_mut)
            .filter(|record| record.id == node)
            && let Some(prerequisites) = record.prerequisites.take()
        {
            self.index.remove(node, &prerequisites);
        }
    }

    /// Records a node issued by this state's allocator at `anchor`, unbound.
    pub(crate) fn add_node(&mut self, id: NodeId, anchor: CellKey) {
        let slot = id.slot() as usize;
        if self.nodes.len() <= slot {
            self.nodes.resize_with(slot + 1, || None);
        }
        self.nodes[slot] = Some(FormulaNode {
            id,
            anchor,
            prerequisites: None,
        });
        self.by_anchor.insert(anchor, id);
    }
}

/// Issues a node ID. [`IdAllocator::issue`] refuses only when every one of its `u32::MAX` slots is
/// live or retired: about four billion formulas held at once, or every slot issued about four
/// billion times. Like a collection's capacity overflow, that panics instead of returning a
/// refusal.
pub(crate) fn issue_node_id(ids: &mut IdAllocator<NodeId>) -> NodeId {
    ids.issue().expect("every node ID slot is live or retired")
}

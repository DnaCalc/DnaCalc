//! Published values in member-sized columns, independent of freshness and evaluation.
use crate::{FormulaResult, graph::BlockHasher};
use calc_value::{CellKey, NodeId, SheetId};
use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasherDefault;
use std::hash::{Hash, Hasher};
use std::sync::Weak;

/// A storage group only. Plan addresses never act as formula or result certificates.
#[derive(Clone, Debug)]
pub(crate) struct ResultGroup {
    pub sheet: SheetId,
    // Keep the allocation address reserved while this storage group exists, without
    // retaining obsolete bytecode. An address is still never a value certificate.
    pub plan: Weak<calc_fml::CompiledPlan>,
    pub role: u8,
}
impl PartialEq for ResultGroup {
    fn eq(&self, other: &Self) -> bool {
        self.sheet == other.sheet && self.role == other.role && self.plan.ptr_eq(&other.plan)
    }
}
impl Eq for ResultGroup {}
impl Hash for ResultGroup {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.sheet.hash(state);
        self.role.hash(state);
        self.plan.as_ptr().hash(state);
    }
}

#[derive(Clone, Copy, Debug)]
struct Address {
    column: usize,
    row: usize,
}

#[derive(Clone, Debug, Default)]
struct Column {
    group: Option<ResultGroup>,
    nodes: Vec<Option<NodeId>>,
    values: Vec<Option<FormulaResult>>,
    free: Vec<usize>,
}
impl Column {
    fn insert(&mut self, node: NodeId, result: FormulaResult) -> usize {
        if let Some(row) = self.free.pop() {
            self.nodes[row] = Some(node);
            self.values[row] = Some(result);
            row
        } else {
            let row = self.nodes.len();
            self.nodes.push(Some(node));
            self.values.push(Some(result));
            row
        }
    }
    fn remove(&mut self, row: usize) -> Option<FormulaResult> {
        if self.nodes[row].take().is_some() {
            self.free.push(row);
        }
        self.values[row].take()
    }
}

/// Actual result members and allocated rows; a formula extent is never allocated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResultStorageStats {
    pub grouped_columns: usize,
    pub grouped_members: usize,
    pub individual_members: usize,
    pub allocated_slots: usize,
    pub vacant_slots: usize,
    pub reserved_slots: usize,
    pub orphan_results: usize,
    pub slot_index_entries: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Results {
    by_slot: Vec<Option<(NodeId, Address)>>,
    columns: Vec<Column>,
    groups: HashMap<ResultGroup, usize>,
    orphans: HashMap<CellKey, (NodeId, FormulaResult), BuildHasherDefault<BlockHasher>>,
}
impl Results {
    fn address(&self, node: NodeId) -> Option<Address> {
        self.by_slot
            .get(node.slot() as usize)?
            .as_ref()
            .filter(|(held, _)| *held == node)
            .map(|(_, address)| *address)
    }
    pub(crate) fn of(&self, node: NodeId) -> Option<&FormulaResult> {
        let address = self.address(node)?;
        self.columns[address.column].values[address.row].as_ref()
    }
    pub(crate) fn of_mut(&mut self, node: NodeId) -> Option<&mut FormulaResult> {
        let address = self.address(node)?;
        self.columns[address.column].values[address.row].as_mut()
    }
    pub(crate) fn orphan(&self, at: CellKey) -> Option<&FormulaResult> {
        self.orphans.get(&at).map(|(_, result)| result)
    }
    pub(crate) fn publish(
        &mut self,
        node: NodeId,
        at: CellKey,
        result: FormulaResult,
        group: Option<ResultGroup>,
    ) {
        if let Some(address) = self.address(node)
            && self.columns[address.column].group == group
        {
            self.columns[address.column].values[address.row] = Some(result);
        } else {
            let slot = node.slot() as usize;
            if self.by_slot.len() <= slot {
                self.by_slot.resize(slot + 1, None);
            }
            if let Some((_, old)) = self.by_slot[slot].take() {
                self.columns[old.column].remove(old.row);
            }
            if self.columns.is_empty() {
                self.columns.push(Column::default());
            }
            let column = match group {
                None => 0,
                Some(key) => *self.groups.entry(key.clone()).or_insert_with(|| {
                    let index = self.columns.len();
                    self.columns.push(Column {
                        group: Some(key),
                        ..Column::default()
                    });
                    index
                }),
            };
            let row = self.columns[column].insert(node, result);
            self.by_slot[slot] = Some((node, Address { column, row }));
        }
        self.orphans.remove(&at);
    }
    pub(crate) fn retire(&mut self, node: NodeId, at: CellKey) {
        if let Some(address) = self.address(node) {
            self.by_slot[node.slot() as usize] = None;
            if let Some(result) = self.columns[address.column].remove(address.row) {
                self.orphans.insert(at, (node, result));
            }
        }
    }
    pub(crate) fn take_orphan(&mut self, at: CellKey) -> Option<(NodeId, FormulaResult)> {
        self.orphans.remove(&at)
    }
    pub(crate) fn clear_orphans(&mut self) {
        self.orphans.clear();
    }
    pub(crate) fn live(&self) -> impl Iterator<Item = (NodeId, &FormulaResult)> {
        self.columns
            .iter()
            .flat_map(|column| column.nodes.iter().zip(&column.values))
            .filter_map(|(node, value)| Some(((*node)?, value.as_ref()?)))
    }
    pub(crate) fn orphans(&self) -> impl Iterator<Item = (CellKey, &FormulaResult)> {
        self.orphans.iter().map(|(at, (_, value))| (*at, value))
    }
    pub(crate) fn stats(&self) -> ResultStorageStats {
        let mut stats = ResultStorageStats {
            slot_index_entries: self.by_slot.len(),
            orphan_results: self.orphans.len(),
            ..Default::default()
        };
        for column in &self.columns {
            let members = column.nodes.len() - column.free.len();
            stats.allocated_slots += column.nodes.len();
            stats.reserved_slots += column.values.capacity();
            stats.vacant_slots += column.free.len();
            if column.group.is_some() {
                stats.grouped_columns += usize::from(members != 0);
                stats.grouped_members += members;
            } else {
                stats.individual_members += members;
            }
        }
        stats
    }
    /// Bound every scan and move before doing it. Declining leaves every allocation unchanged.
    pub(crate) fn can_compact(&self, anchors: usize, budget: usize) -> bool {
        let mut cost = self
            .by_slot
            .len()
            .saturating_add(self.columns.len())
            .saturating_add(anchors);
        if cost > budget {
            return false;
        }
        for column in &self.columns {
            cost = cost.saturating_add(column.nodes.len());
            if cost > budget {
                return false;
            }
        }
        true
    }
    pub(crate) fn compact(
        &mut self,
        assignments: BTreeMap<NodeId, (CellKey, Option<ResultGroup>)>,
    ) {
        let old = std::mem::take(self);
        for column in old.columns {
            for (node, value) in column.nodes.into_iter().zip(column.values) {
                if let (Some(node), Some(value)) = (node, value) {
                    let (at, group) = assignments
                        .get(&node)
                        .expect("a published result belongs to a live node");
                    self.publish(node, *at, value, group.clone());
                }
            }
        }
        self.orphans = old.orphans;
    }
}

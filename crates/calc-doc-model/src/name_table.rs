//! Persistent defined-name records. Identifiers survive formula edits and bulk rewrites;
//! a removed name's identifier is never issued again in the same table lineage.

use crate::DefinedName;
use calc_value::{NameId, NameKey, NameScope};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

const PAGE: usize = 256;
const BUCKETS: usize = 1024;
type NamePage = Arc<Vec<Option<NameNode>>>;
type NameBucket = Arc<HashMap<Arc<NameKey>, NameId>>;

#[derive(Debug)]
struct Allocation {
    lineage: u64,
    next_slot: AtomicU64,
    next_version: AtomicU64,
}

/// A source record identity retained when Undo creates a new live name identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NameOrigin {
    pub lineage: u64,
    pub record: NameId,
}
impl Default for Allocation {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let lineage = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("name lineage exhausted");
        Self {
            lineage,
            next_slot: AtomicU64::new(0),
            next_version: AtomicU64::new(0),
        }
    }
}

/// An authored non-cell node: its definition and binding version, never a cached evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameNode {
    id: NameId,
    origin: NameOrigin,
    key: Arc<NameKey>,
    definition: Arc<DefinedName>,
    version: u64,
}
impl NameNode {
    pub fn origin(&self) -> NameOrigin {
        self.origin
    }
    pub fn id(&self) -> NameId {
        self.id
    }
    pub fn key(&self) -> &NameKey {
        &self.key
    }
    pub fn definition(&self) -> &DefinedName {
        &self.definition
    }
    pub fn binding_version(&self) -> u64 {
        self.version
    }
}

/// Name lookup and identifier storage share immutable pages across authored snapshots.
/// Ordinary edits copy one record page; lookup buckets change only on insertion/removal.
/// The legacy ordered slice is materialized only for callers that request the whole list.
#[derive(Clone, Default)]
pub struct NameTable {
    pages: Arc<Vec<NamePage>>,
    order: Arc<Vec<Arc<Vec<NameId>>>>,
    index: Arc<Vec<NameBucket>>,
    duplicates: Arc<HashSet<Arc<NameKey>>>,
    len: usize,
    order_len: usize,
    allocation: Arc<Allocation>,
    version: u64,
    flat: Arc<OnceLock<Vec<DefinedName>>>,
}
impl std::fmt::Debug for NameTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NameTable")
            .field("len", &self.len)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}
impl PartialEq for NameTable {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len
            && self
                .nodes()
                .zip(other.nodes())
                .all(|(a, b)| a.definition == b.definition)
    }
}
impl Eq for NameTable {}

fn bucket(key: &NameKey) -> usize {
    let mut hash = DefaultHasher::new();
    key.hash(&mut hash);
    hash.finish() as usize % BUCKETS
}
fn empty_index() -> Vec<NameBucket> {
    vec![Arc::new(HashMap::new()); BUCKETS]
}

impl NameTable {
    pub fn new(names: Vec<DefinedName>) -> Self {
        let mut table = Self::default();
        for name in names {
            table.insert_record(name);
        }
        table
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, id: NameId) -> Option<&NameNode> {
        let slot = id.slot() as usize;
        self.pages
            .get(slot / PAGE)?
            .get(slot % PAGE)?
            .as_ref()
            .filter(|node| node.id == id)
    }
    pub fn lookup(&self, scope: NameScope, spelling: &str) -> Option<&NameNode> {
        self.lookup_key(&NameKey::new(scope, spelling))
    }
    pub fn lookup_key(&self, key: &NameKey) -> Option<&NameNode> {
        self.get(*self.index.get(bucket(key))?.get(key)?)
    }
    /// All imported definitions of one scoped key, first lookup result first. Ordinary
    /// unique keys use only the hash index; only malformed duplicate keys inspect file order.
    pub fn matching<'a>(&'a self, key: &'a NameKey) -> impl Iterator<Item = &'a NameNode> {
        let first = self.lookup_key(key);
        first.into_iter().chain(
            self.duplicates
                .contains(key)
                .then(|| {
                    self.nodes().filter(move |node| {
                        node.key() == key && first.is_none_or(|first| node.id != first.id)
                    })
                })
                .into_iter()
                .flatten(),
        )
    }

    /// Persisted notebook/file order, skipping deleted identities.
    pub fn nodes(&self) -> impl DoubleEndedIterator<Item = &NameNode> {
        self.order
            .iter()
            .flat_map(|page| page.iter())
            .filter_map(|id| self.get(*id))
    }
    pub fn definitions(&self) -> impl DoubleEndedIterator<Item = &DefinedName> {
        self.nodes().map(NameNode::definition)
    }
    pub fn as_slice(&self) -> &[DefinedName] {
        self.flat
            .get_or_init(|| self.definitions().cloned().collect())
    }
    fn advance(&mut self) -> u64 {
        self.version = self
            .allocation
            .next_version
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("name binding version exhausted")
            + 1;
        self.flat = Arc::new(OnceLock::new());
        self.version
    }
    fn index_mut(&mut self, key: &NameKey) -> &mut HashMap<Arc<NameKey>, NameId> {
        let index = Arc::make_mut(&mut self.index);
        if index.is_empty() {
            *index = empty_index();
        }
        Arc::make_mut(&mut index[bucket(key)])
    }
    fn insert_record(&mut self, definition: DefinedName) -> NameId {
        let slot = self
            .allocation
            .next_slot
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                (n <= u64::from(u32::MAX)).then(|| n + 1)
            })
            .expect("name identity space exhausted") as u32;
        let id = NameId::from_parts(slot, 0);
        let key = Arc::new(NameKey::new(definition.scope, &definition.name));
        let version = self.advance();
        let node = NameNode {
            id,
            origin: NameOrigin {
                lineage: self.allocation.lineage,
                record: id,
            },
            key: key.clone(),
            definition: Arc::new(definition),
            version,
        };
        let page = slot as usize / PAGE;
        let pages = Arc::make_mut(&mut self.pages);
        if page >= pages.len() {
            let empty = Arc::new(vec![None; PAGE]);
            pages.resize(page + 1, empty);
        }
        Arc::make_mut(&mut pages[page])[slot as usize % PAGE] = Some(node);
        let page = self.order_len / PAGE;
        let order = Arc::make_mut(&mut self.order);
        if page == order.len() {
            order.push(Arc::new(Vec::new()));
        }
        Arc::make_mut(&mut order[page]).push(id);
        self.order_len += 1;
        self.len += 1;
        // Keep the first imported spelling's lookup, as the former ordered list did.
        if self.index_mut(&key).contains_key(key.as_ref()) {
            Arc::make_mut(&mut self.duplicates).insert(key);
        } else {
            self.index_mut(&key).insert(key, id);
        }
        id
    }
    fn replace_definition(&mut self, id: NameId, definition: DefinedName) -> bool {
        if self
            .get(id)
            .is_some_and(|node| node.definition.as_ref() == &definition)
        {
            return false;
        }
        let version = self.advance();
        let slot = id.slot() as usize;
        let node = Arc::make_mut(&mut Arc::make_mut(&mut self.pages)[slot / PAGE])[slot % PAGE]
            .as_mut()
            .expect("a live name");
        node.key = Arc::new(NameKey::new(definition.scope, &definition.name));
        node.definition = Arc::new(definition);
        node.version = version;
        true
    }
    fn remove_id(&mut self, id: NameId) -> bool {
        let Some(key) = self.get(id).map(|node| node.key.clone()) else {
            return false;
        };
        let primary = self.lookup_key(&key).is_some_and(|node| node.id == id);
        self.advance();
        let slot = id.slot() as usize;
        Arc::make_mut(&mut Arc::make_mut(&mut self.pages)[slot / PAGE])[slot % PAGE] = None;
        self.len -= 1;
        if primary {
            let next = self
                .duplicates
                .contains(&key)
                .then(|| self.nodes().find(|n| n.key == key).map(|n| n.id))
                .flatten();
            match next {
                Some(id) => {
                    self.index_mut(&key).insert(key, id);
                }
                None => {
                    self.index_mut(&key).remove(key.as_ref());
                }
            }
        }
        true
    }
    /// `None` removes a definition. A missing removal and an equal formula make no change.
    /// Existing names retain spelling, attributes and identifier under case-folded edits.
    pub fn set_formula(&mut self, scope: NameScope, spelling: &str, formula: Option<&str>) -> bool {
        let held = self
            .lookup(scope, spelling)
            .map(|node| (node.id, node.definition.clone()));
        match (held, formula) {
            (Some((id, held)), Some(formula)) => {
                if held.formula == formula {
                    return false;
                }
                let mut definition = held.as_ref().clone();
                definition.formula = formula.to_owned();
                self.replace_definition(id, definition)
            }
            (Some((id, _)), None) => self.remove_id(id),
            (None, Some(formula)) => {
                self.insert_record(DefinedName {
                    name: spelling.to_owned(),
                    scope,
                    formula: formula.to_owned(),
                    hidden: false,
                    comment: None,
                    function: false,
                    vb_procedure: false,
                    xlm: false,
                });
                true
            }
            (None, None) => false,
        }
    }
    fn records_for_key(&self, key: &NameKey) -> Vec<&NameNode> {
        if self.duplicates.contains(key) {
            self.nodes()
                .filter(|node| node.key.as_ref() == key)
                .collect()
        } else {
            self.lookup_key(key).into_iter().collect()
        }
    }
    /// Retain only the edited scoped records. Ordinary formula edits use indexed lookup;
    /// deletion history also retains live order in one scan of the existing table.
    pub fn snapshot_keys(&self, keys: &[NameKey], positions: bool) -> Vec<crate::NameRestoration> {
        let selected: HashSet<_> = keys.iter().collect();
        let order: HashMap<_, _> = if positions {
            self.nodes()
                .enumerate()
                .filter(|(_, node)| selected.contains(node.key.as_ref()))
                .map(|(position, node)| (node.id, position))
                .collect()
        } else {
            HashMap::new()
        };
        keys.iter()
            .map(|key| crate::NameRestoration {
                key: key.clone(),
                records: self
                    .records_for_key(key)
                    .into_iter()
                    .map(|node| crate::RetainedName {
                        origin: node.origin,
                        position: order.get(&node.id).copied(),
                        definition: node.definition().clone(),
                    })
                    .collect(),
            })
            .collect()
    }
    pub fn restoration_changes(&self, snapshot: &crate::NameRestoration) -> bool {
        let current = self.records_for_key(&snapshot.key);
        current.len() != snapshot.records.len()
            || current.iter().zip(&snapshot.records).any(|(node, record)| {
                node.origin != record.origin || node.definition() != &record.definition
            })
    }
    /// Restore scoped groups by source origin, retaining live identities where possible.
    /// Recreated rows get fresh live IDs and keep their source record provenance.
    pub(crate) fn restore_records(&mut self, snapshots: &[crate::NameRestoration]) -> bool {
        let keys: HashSet<_> = snapshots
            .iter()
            .map(|snapshot| snapshot.key.clone())
            .collect();
        // Origins move between selected keys without deleting and recreating their live IDs.
        let by_origin: HashMap<_, _> = keys
            .iter()
            .flat_map(|key| self.records_for_key(key))
            .map(|node| (node.origin, node.id))
            .collect();
        let targets_kept: HashSet<_> = snapshots
            .iter()
            .flat_map(|snapshot| &snapshot.records)
            .filter_map(|record| by_origin.get(&record.origin).copied())
            .collect();
        let plans: Vec<_> = snapshots
            .iter()
            .map(|snapshot| {
                let current = self.records_for_key(&snapshot.key);
                let ids: Vec<_> = current.iter().map(|node| node.id).collect();
                let targets: Vec<_> = snapshot
                    .records
                    .iter()
                    .map(|record| by_origin.get(&record.origin).copied())
                    .collect();
                (snapshot, ids, targets)
            })
            .collect();
        let positioned = plans.iter().any(|(snapshot, ids, targets)| {
            snapshot
                .records
                .iter()
                .any(|record| record.position.is_some())
                || targets
                    .iter()
                    .enumerate()
                    .any(|(index, target)| index < ids.len() && *target != Some(ids[index]))
        });
        let before: Vec<_> = if positioned {
            self.nodes().map(NameNode::id).collect()
        } else {
            Vec::new()
        };
        let positions: HashMap<_, _> = before
            .iter()
            .enumerate()
            .map(|(position, id)| (*id, position))
            .collect();
        let mut retained = Vec::new();
        let mut primaries = Vec::new();
        let mut changed = false;
        for (snapshot, ids, targets) in plans {
            let mut used = HashSet::new();
            let mut primary = None;
            for (index, (record, target)) in snapshot.records.iter().zip(targets).enumerate() {
                let id = if let Some(id) = target {
                    changed |= self.replace_definition(id, record.definition.clone());
                    id
                } else {
                    changed = true;
                    let id = self.insert_record(record.definition.clone());
                    self.set_origin(id, record.origin);
                    id
                };
                used.insert(id);
                primary.get_or_insert(id);
                if positioned {
                    retained.push((
                        record
                            .position
                            .or_else(|| ids.get(index).and_then(|id| positions.get(id).copied()))
                            .unwrap_or(usize::MAX),
                        id,
                    ));
                }
            }
            primaries.push((snapshot.key.clone(), primary));
            for id in ids {
                if !used.contains(&id) && !targets_kept.contains(&id) {
                    changed |= self.remove_id(id);
                }
            }
        }
        if positioned {
            let others: Vec<_> = self
                .nodes()
                .filter(|node| !keys.contains(node.key.as_ref()))
                .map(NameNode::id)
                .collect();
            let mut others = others.into_iter();
            retained.sort_by_key(|(position, _)| *position);
            let mut order = Vec::with_capacity(self.len);
            for (position, id) in retained {
                while order.len() < position {
                    let Some(other) = others.next() else { break };
                    order.push(other);
                }
                order.push(id);
            }
            order.extend(others);
            if order != before {
                self.advance();
                changed = true;
                self.order_len = order.len();
                self.order = Arc::new(
                    order
                        .chunks(PAGE)
                        .map(|page| Arc::new(page.to_vec()))
                        .collect(),
                );
            }
        }
        for snapshot in snapshots {
            if snapshot.records.len() > 1 {
                Arc::make_mut(&mut self.duplicates).insert(Arc::new(snapshot.key.clone()));
            }
        }
        for (key, first) in primaries {
            if self.lookup_key(&key).map(NameNode::id) == first {
                continue;
            }
            if let Some(id) = first {
                self.index_mut(&key).insert(Arc::new(key.clone()), id);
            } else {
                self.index_mut(&key).remove(&key);
            }
        }
        changed
    }
    fn set_origin(&mut self, id: NameId, origin: crate::NameOrigin) {
        let slot = id.slot() as usize;
        Arc::make_mut(&mut Arc::make_mut(&mut self.pages)[slot / PAGE])[slot % PAGE]
            .as_mut()
            .expect("live restored name")
            .origin = origin;
    }
    pub(crate) fn restored_from(&self, earlier: &Self) -> Self {
        let keys: std::collections::BTreeSet<_> = self
            .nodes()
            .chain(earlier.nodes())
            .map(|node| node.key().clone())
            .collect();
        let snapshots = earlier.snapshot_keys(&keys.into_iter().collect::<Vec<_>>(), true);
        let mut restored = self.clone();
        restored.restore_records(&snapshots);
        restored
    }
    pub(crate) fn retain(&mut self, keep: impl Fn(&DefinedName) -> bool) {
        let removed: Vec<_> = self
            .nodes()
            .filter(|node| !keep(node.definition()))
            .map(NameNode::id)
            .collect();
        for id in removed {
            self.remove_id(id);
        }
    }
    /// Bulk structural rewrites preserve surviving identities and take the supplied order.
    pub(crate) fn replaced(&self, names: Vec<DefinedName>) -> Self {
        let mut available: HashMap<Arc<NameKey>, VecDeque<NameId>> = HashMap::new();
        for node in self.nodes() {
            available
                .entry(node.key.clone())
                .or_default()
                .push_back(node.id);
        }
        let mut next = self.clone();
        let mut order = Vec::with_capacity(names.len());
        for definition in names {
            let key = NameKey::new(definition.scope, &definition.name);
            let id = match available.get_mut(&key).and_then(VecDeque::pop_front) {
                Some(id) => {
                    next.replace_definition(id, definition);
                    id
                }
                None => next.insert_record(definition),
            };
            order.push(id);
        }
        for id in available.into_values().flatten() {
            next.remove_id(id);
        }
        next.order_len = order.len();
        next.order = Arc::new(
            order
                .chunks(PAGE)
                .map(|page| Arc::new(page.to_vec()))
                .collect(),
        );
        next.index = Arc::new(empty_index());
        next.duplicates = Arc::new(HashSet::new());
        let keys: Vec<_> = next
            .nodes()
            .map(|node| (node.key.clone(), node.id))
            .collect();
        for (key, id) in keys {
            if next.index_mut(&key).contains_key(key.as_ref()) {
                Arc::make_mut(&mut next.duplicates).insert(key);
            } else {
                next.index_mut(&key).insert(key, id);
            }
        }
        next.flat = Arc::new(OnceLock::new());
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn name(name: &str, formula: &str) -> DefinedName {
        DefinedName {
            name: name.into(),
            formula: formula.into(),
            scope: NameScope::Workbook,
            hidden: false,
            comment: None,
            function: false,
            vb_procedure: false,
            xlm: false,
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn names_keep_scope_case_fold_snapshots_and_nonrecycled_ids() {
        let sheet = calc_value::SheetId::from_parts(0, 0);
        let mut table = NameTable::new(vec![name("Größe", "1")]);
        let global = table.lookup(NameScope::Workbook, "GRÖSSE").unwrap().id();
        table.set_formula(NameScope::Sheet(sheet), "GRÖSSE", Some("2"));
        let local = table.lookup(NameScope::Sheet(sheet), "größe").unwrap().id();
        assert_ne!(global, local);
        let old = table.clone();
        assert!(table.set_formula(NameScope::Workbook, "GRÖSSE", Some("50")));
        assert_eq!(table.get(global).unwrap().definition().name, "Größe");
        assert_eq!(old.get(global).unwrap().definition().formula, "1");
        assert!(
            table.get(global).unwrap().binding_version()
                > old.get(global).unwrap().binding_version()
        );
        assert!(table.set_formula(NameScope::Workbook, "GRÖSSE", None));
        assert!(table.get(global).is_none());
        table.set_formula(NameScope::Workbook, "Größe", Some("3"));
        assert_ne!(
            table.lookup(NameScope::Workbook, "größe").unwrap().id(),
            global
        );
        assert_eq!(table.get(local).unwrap().definition().formula, "2");
        assert_eq!(old.get(global).unwrap().definition().formula, "1");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn editing_a_name_copies_one_page_and_one_definition_without_flattening() {
        let mut table =
            NameTable::new((0..1025).map(|n| name(&format!("Name_{n}"), "1")).collect());
        let old = table.clone();
        table.set_formula(NameScope::Workbook, "Name_700", Some("2"));
        assert!(Arc::ptr_eq(&table.index, &old.index));
        assert!(Arc::ptr_eq(&table.order, &old.order));
        assert!(table.flat.get().is_none());
        assert_eq!(
            table
                .pages
                .iter()
                .zip(old.pages.iter())
                .filter(|(a, b)| !Arc::ptr_eq(a, b))
                .count(),
            1
        );
        assert_eq!(
            table
                .nodes()
                .zip(old.nodes())
                .filter(|(a, b)| !Arc::ptr_eq(&a.definition, &b.definition))
                .count(),
            1
        );
        assert_eq!(
            old.lookup(NameScope::Workbook, "Name_700")
                .unwrap()
                .definition()
                .formula,
            "1"
        );
        assert_eq!(table.as_slice()[700].formula, "2");
        let with_slice = table.clone();
        table.set_formula(NameScope::Workbook, "Name_701", Some("3"));
        assert!(table.flat.get().is_none());
        assert_eq!(with_slice.as_slice()[701].formula, "1");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn bulk_rewrites_preserve_ids_and_order_without_reviving_removed_names() {
        let table = NameTable::new(vec![name("A", "1"), name("B", "2"), name("C", "3")]);
        let ids: Vec<_> = table.nodes().map(NameNode::id).collect();
        let next = table.replaced(vec![name("C", "4"), name("A", "1"), name("D", "5")]);
        assert_eq!(
            next.nodes()
                .map(|n| n.definition().name.as_str())
                .collect::<Vec<_>>(),
            ["C", "A", "D"]
        );
        assert_eq!(next.lookup(NameScope::Workbook, "A").unwrap().id(), ids[0]);
        assert_eq!(next.lookup(NameScope::Workbook, "C").unwrap().id(), ids[2]);
        assert!(next.get(ids[1]).is_none());
        assert_ne!(next.lookup(NameScope::Workbook, "D").unwrap().id(), ids[1]);
        assert_eq!(table.get(ids[1]).unwrap().definition().formula, "2");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn forks_share_allocation_but_never_reuse_ids_or_binding_versions() {
        let base = NameTable::new(vec![name("Original", "1")]);
        let id = base.lookup(NameScope::Workbook, "Original").unwrap().id();
        let mut left = base.clone();
        let mut right = base.clone();
        left.set_formula(NameScope::Workbook, "Original", Some("2"));
        right.set_formula(NameScope::Workbook, "Original", Some("3"));
        assert_eq!(left.get(id).unwrap().id(), right.get(id).unwrap().id());
        assert_ne!(
            left.get(id).unwrap().binding_version(),
            right.get(id).unwrap().binding_version()
        );
        left.set_formula(NameScope::Workbook, "Left", Some("4"));
        right.set_formula(NameScope::Workbook, "Right", Some("5"));
        let l = left.lookup(NameScope::Workbook, "Left").unwrap().id();
        let r = right.lookup(NameScope::Workbook, "Right").unwrap().id();
        assert_ne!(l, r);
        assert!(left.get(r).is_none());
        assert!(right.get(l).is_none());
        assert_eq!(base.get(id).unwrap().definition().formula, "1");
        left.set_formula(NameScope::Workbook, "Original", None);
        let restored = left.replaced(base.definitions().cloned().collect());
        assert!(restored.get(id).is_none());
        assert_ne!(
            restored
                .lookup(NameScope::Workbook, "Original")
                .unwrap()
                .id(),
            id
        );
        let undone = right.replaced(base.definitions().cloned().collect());
        assert_eq!(
            undone.lookup(NameScope::Workbook, "Original").unwrap().id(),
            id
        );
        assert!(
            undone.get(id).unwrap().binding_version() > right.get(id).unwrap().binding_version()
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn imported_duplicates_keep_first_lookup_and_fall_back_after_removal() {
        let mut table = NameTable::new(vec![
            name("Mixed", "1"),
            name("MIXED", "2"),
            name("mixed", "3"),
        ]);
        let ids: Vec<_> = table.nodes().map(NameNode::id).collect();
        assert_eq!(
            table.lookup(NameScope::Workbook, "mixed").unwrap().id(),
            ids[0]
        );
        assert!(table.set_formula(NameScope::Workbook, "Mixed", None));
        assert_eq!(
            table.lookup(NameScope::Workbook, "mixed").unwrap().id(),
            ids[1]
        );
        assert_eq!(
            table
                .as_slice()
                .iter()
                .map(|n| n.formula.as_str())
                .collect::<Vec<_>>(),
            ["2", "3"]
        );
        let reordered = table.replaced(vec![name("mixed", "3"), name("MIXED", "2")]);
        assert_eq!(
            reordered.lookup(NameScope::Workbook, "mixed").unwrap().id(),
            ids[1]
        );
        assert_eq!(
            reordered
                .lookup(NameScope::Workbook, "mixed")
                .unwrap()
                .definition()
                .formula,
            "3"
        );
        assert!(table.set_formula(NameScope::Workbook, "Mixed", None));
        assert_eq!(
            table.lookup(NameScope::Workbook, "mixed").unwrap().id(),
            ids[2]
        );
        assert!(table.set_formula(NameScope::Workbook, "Mixed", None));
        assert!(table.lookup(NameScope::Workbook, "mixed").is_none());
        table.set_formula(NameScope::Workbook, "Mixed", Some("4"));
        assert!(!ids.contains(&table.lookup(NameScope::Workbook, "mixed").unwrap().id()));
    }
}

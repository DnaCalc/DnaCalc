//! Same-thread live Range identities and invocation-time reference views. A structural edit
//! updates live handles, never cells individually. Deleted handles never revive on Undo.
use super::StructuralEdit;
use calc_func::value::{GridArea, ReferenceLike};
use calc_func::{ReferenceHandleError as Error, ReferenceHandles, StoredReference};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashMap},
    rc::Rc,
};
#[derive(Debug)]
pub struct ReferenceTracker {
    owner: u64,
    next: Cell<u64>,
    epoch: Cell<u64>,
    closed: Cell<bool>,
    entries: RefCell<BTreeMap<u64, ReferenceLike>>,
    leases: RefCell<BTreeMap<u64, usize>>,
    index: RefCell<HashMap<ReferenceLike, u64>>,
    refused: RefCell<BTreeSet<u64>>,
}
impl ReferenceTracker {
    pub fn new() -> Rc<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let owner = NEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .expect("reference owner identity exhausted");
        Rc::new(Self {
            owner,
            next: Cell::new(1),
            epoch: Cell::new(0),
            closed: Cell::new(false),
            entries: RefCell::default(),
            leases: RefCell::default(),
            index: RefCell::default(),
            refused: RefCell::default(),
        })
    }
    pub fn pin(self: &Rc<Self>) -> Rc<ReferenceView> {
        Rc::new(ReferenceView {
            owner: self.clone(),
            epoch: self.epoch.get(),
            entries: RefCell::new(self.entries.borrow().clone()),
            refused: self.refused.borrow().clone(),
        })
    }
    pub fn close(&self) {
        self.closed.set(true);
        self.entries.borrow_mut().clear();
        self.leases.borrow_mut().clear();
        self.index.borrow_mut().clear();
        self.refused.borrow_mut().clear();
    }
    pub fn apply(&self, edit: &StructuralEdit) {
        let mut entries = self.entries.borrow_mut();
        let mut refused = self.refused.borrow_mut();
        entries.retain(|slot, reference| match transformed(reference, edit) {
            Ok(Some(next)) => {
                *reference = next;
                true
            }
            Ok(None) => false,
            Err(_) => {
                refused.insert(*slot);
                false
            }
        });
        let mut index = self.index.borrow_mut();
        index.clear();
        index.extend(
            entries
                .iter()
                .map(|(slot, reference)| (reference.clone(), *slot)),
        );
        self.epoch.set(
            self.epoch
                .get()
                .checked_add(1)
                .expect("reference view epoch exhausted"),
        );
    }
    fn retain(&self, reference: &ReferenceLike) -> Result<StoredReference, Error> {
        if self.closed.get() {
            return Err(Error::ClosedWorkbook);
        }
        if let Some(slot) = self.index.borrow().get(reference).copied() {
            *self.leases.borrow_mut().entry(slot).or_default() += 1;
            return Ok(StoredReference::Tracked {
                owner: self.owner,
                slot,
            });
        }
        let slot = self.next.get();
        self.next.set(
            slot.checked_add(1)
                .expect("range handle identity exhausted"),
        );
        self.entries.borrow_mut().insert(slot, reference.clone());
        self.leases.borrow_mut().insert(slot, 1);
        self.index.borrow_mut().insert(reference.clone(), slot);
        Ok(StoredReference::Tracked {
            owner: self.owner,
            slot,
        })
    }
}
#[derive(Debug)]
pub struct ReferenceView {
    owner: Rc<ReferenceTracker>,
    epoch: u64,
    entries: RefCell<BTreeMap<u64, ReferenceLike>>,
    refused: BTreeSet<u64>,
}
impl ReferenceHandles for ReferenceView {
    fn release(&self, reference: &StoredReference) {
        let StoredReference::Tracked { owner, slot } = reference else {
            return;
        };
        if *owner != self.owner.owner {
            return;
        }
        let mut leases = self.owner.leases.borrow_mut();
        let Some(count) = leases.get_mut(slot) else {
            return;
        };
        *count -= 1;
        if *count != 0 {
            return;
        }
        leases.remove(slot);
        if let Some(reference) = self.owner.entries.borrow_mut().remove(slot) {
            let mut index = self.owner.index.borrow_mut();
            if index.get(&reference) == Some(slot) {
                index.remove(&reference);
            }
        }
        self.owner.refused.borrow_mut().remove(slot);
        self.entries.borrow_mut().remove(slot);
    }
    fn retain(&self, reference: &ReferenceLike) -> Result<StoredReference, Error> {
        if self.epoch != self.owner.epoch.get() {
            return Err(Error::ExpiredView);
        }
        let handle = self.owner.retain(reference)?;
        if let StoredReference::Tracked { slot, .. } = handle {
            self.entries.borrow_mut().insert(slot, reference.clone());
        }
        Ok(handle)
    }
    fn resolve(&self, reference: &StoredReference) -> Result<ReferenceLike, Error> {
        if self.owner.closed.get() {
            return Err(Error::ClosedWorkbook);
        }
        match reference {
            StoredReference::Tracked { owner, slot }
                if *owner == self.owner.owner && self.refused.contains(slot) =>
            {
                Err(Error::NotModelledEdit)
            }
            StoredReference::Tracked { owner, slot } if *owner == self.owner.owner => self
                .entries
                .borrow()
                .get(slot)
                .filter(|_| self.owner.leases.borrow().contains_key(slot))
                .cloned()
                .ok_or(Error::Deleted),
            _ => Err(Error::ForeignWorkbook),
        }
    }
}
fn transformed(
    reference: &ReferenceLike,
    edit: &StructuralEdit,
) -> Result<Option<ReferenceLike>, Error> {
    let areas: Vec<GridArea> = reference
        .areas()
        .iter()
        .flat_map(|area| edit.tracked_areas(*area))
        .collect();
    // A Range cannot span sheets. Refuse that partial-cross-sheet shape rather than widen it.
    if areas.is_empty() {
        Ok(None)
    } else {
        ReferenceLike::from_areas(areas)
            .map(Some)
            .map_err(|_| Error::NotModelledEdit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::value::{CellKey, IdAllocator, SheetId};
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn release_preserves_other_holders_and_removes_the_last_live_tracker_slot() {
        let sheet = IdAllocator::<SheetId>::new().issue().unwrap();
        let range = ReferenceLike::cell(CellKey::new(sheet, 2, 1).unwrap());
        let tracker = ReferenceTracker::new();
        let first = tracker.pin();
        let handle = first.retain(&range).unwrap();
        let second = tracker.pin();
        assert_eq!(second.retain(&range).unwrap(), handle);
        first.release(&handle);
        assert_eq!(second.resolve(&handle).unwrap(), range);
        tracker.apply(&StructuralEdit::Insert {
            sheet,
            axis: super::super::EditAxis::Rows,
            first: 1,
            count: 1,
        });
        // Releasing through the admitted old view also removes the transformed entry/index.
        second.release(&handle);
        assert!(tracker.entries.borrow().is_empty());
        assert!(tracker.index.borrow().is_empty());
        assert!(tracker.leases.borrow().is_empty());
        assert_eq!(first.resolve(&handle), Err(Error::Deleted));
        let next = tracker.pin().retain(&range).unwrap();
        assert_ne!(next, handle, "released identities never revive");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn partial_cell_shift_splits_a_live_range_and_keeps_its_admitted_view() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = ids.issue().unwrap();
        let cell = |row, column| CellKey::new(sheet, row, column).unwrap();
        let area = GridArea::new(cell(2, 1), cell(3, 2)).unwrap();
        let tracker = ReferenceTracker::new();
        let admitted = tracker.pin();
        let handle = admitted
            .retain(&ReferenceLike::from_areas(vec![area]).unwrap())
            .unwrap();
        tracker.apply(&StructuralEdit::ShiftCells {
            area: GridArea::cell(cell(2, 1)),
            down: true,
            insert: true,
        });
        assert_eq!(admitted.resolve(&handle).unwrap().areas(), &[area]);
        let current = tracker.pin().resolve(&handle).unwrap();
        assert_eq!(
            current.areas(),
            &[
                GridArea::new(cell(2, 2), cell(3, 2)).unwrap(),
                GridArea::new(cell(3, 1), cell(4, 1)).unwrap()
            ]
        );
        assert_eq!(
            admitted.retain(&ReferenceLike::from_areas(vec![area]).unwrap()),
            Err(Error::ExpiredView)
        );
        tracker.close();
        assert_eq!(admitted.resolve(&handle), Err(Error::ClosedWorkbook));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn partial_cross_sheet_cut_is_an_explicit_refusal_not_a_deleted_object() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let source = ids.issue().unwrap();
        let target = ids.issue().unwrap();
        let a = CellKey::new(source, 1, 1).unwrap();
        let b = CellKey::new(source, 1, 2).unwrap();
        let original = ReferenceLike::from_areas(vec![GridArea::new(a, b).unwrap()]).unwrap();
        let tracker = ReferenceTracker::new();
        let admitted = tracker.pin();
        let held = admitted.retain(&original).unwrap();
        tracker.apply(&StructuralEdit::MoveCells {
            area: GridArea::cell(a),
            to: CellKey::new(target, 1, 1).unwrap(),
            source_name: "Source".into(),
            target_name: "Target".into(),
        });
        assert_eq!(admitted.resolve(&held), Ok(original));
        assert_eq!(tracker.pin().resolve(&held), Err(Error::NotModelledEdit));
        tracker.apply(&StructuralEdit::MoveCells {
            area: GridArea::cell(CellKey::new(target, 1, 1).unwrap()),
            to: a,
            source_name: "Target".into(),
            target_name: "Source".into(),
        });
        assert_eq!(tracker.pin().resolve(&held), Err(Error::NotModelledEdit));
    }
}

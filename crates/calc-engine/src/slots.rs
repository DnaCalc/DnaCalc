//! Slot storage for calculation scratch and immutable requirement cuts.
use std::collections::BTreeMap;
use std::sync::Arc;

/// Small closures allocate only their members, including members near the end of a large
/// workbook. Large closures switch to direct indexing; iteration always follows slot order.
#[derive(Debug)]
pub(crate) enum Slots<T> {
    Sparse {
        entries: BTreeMap<u32, T>,
        slots: usize,
    },
    Dense(Vec<Option<T>>),
}
impl<T> Default for Slots<T> {
    fn default() -> Self {
        Self::new(usize::MAX, 0)
    }
}
impl<T> Slots<T> {
    pub fn new(slots: usize, expected: usize) -> Self {
        if expected > 0 && expected >= slots.div_ceil(4) {
            Self::Dense(std::iter::repeat_with(|| None).take(slots).collect())
        } else {
            Self::Sparse {
                entries: BTreeMap::new(),
                slots,
            }
        }
    }
    pub fn get(&self, slot: u32) -> Option<&T> {
        match self {
            Self::Sparse { entries, .. } => entries.get(&slot),
            Self::Dense(entries) => entries.get(slot as usize)?.as_ref(),
        }
    }
    pub fn get_mut(&mut self, slot: u32) -> Option<&mut T> {
        match self {
            Self::Sparse { entries, .. } => entries.get_mut(&slot),
            Self::Dense(entries) => entries.get_mut(slot as usize)?.as_mut(),
        }
    }
    pub fn insert(&mut self, slot: u32, value: T) {
        match self {
            Self::Sparse { entries, slots } => {
                entries.insert(slot, value);
                if entries.len() >= slots.div_ceil(4) {
                    let slots = *slots;
                    self.reserve_dense(slots);
                }
            }
            Self::Dense(entries) => {
                if entries.len() <= slot as usize {
                    entries.resize_with(slot as usize + 1, || None);
                }
                entries[slot as usize] = Some(value);
            }
        }
    }
    pub fn reserve_dense(&mut self, slots: usize) {
        let Self::Sparse { entries, .. } = self else {
            return;
        };
        let size = entries
            .last_key_value()
            .map_or(slots, |(slot, _)| slots.max(*slot as usize + 1));
        let mut dense: Vec<_> = std::iter::repeat_with(|| None).take(size).collect();
        for (slot, value) in std::mem::take(entries) {
            dense[slot as usize] = Some(value);
        }
        *self = Self::Dense(dense);
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let (sparse, dense) = match self {
            Self::Sparse { entries, .. } => (Some(entries.values()), None),
            Self::Dense(entries) => (None, Some(entries.iter().flatten())),
        };
        sparse
            .into_iter()
            .flatten()
            .chain(dense.into_iter().flatten())
    }
    pub fn into_values(self) -> impl Iterator<Item = T> {
        let (sparse, dense) = match self {
            Self::Sparse { entries, .. } => (Some(entries.into_values()), None),
            Self::Dense(entries) => (None, Some(entries.into_iter().flatten())),
        };
        sparse
            .into_iter()
            .flatten()
            .chain(dense.into_iter().flatten())
    }
}

// Four bytes of a u32 slot select three bounded pointer pages and one value page. A cut
// shares its root in O(1); changing a slot copies at most four pages of 256 entries.
type Page<T> = Arc<BTreeMap<u8, T>>;
#[derive(Clone, Debug)]
pub(crate) struct SlotCut<T>(Page<Page<Page<Page<T>>>>);
impl<T> Default for SlotCut<T> {
    fn default() -> Self {
        Self(Arc::new(BTreeMap::new()))
    }
}
impl<T: Clone> SlotCut<T> {
    pub fn get(&self, slot: u32) -> Option<&T> {
        let [a, b, c, d] = slot.to_be_bytes();
        self.0.get(&a)?.get(&b)?.get(&c)?.get(&d)
    }
    pub fn insert(&mut self, slot: u32, value: T) {
        let [a, b, c, d] = slot.to_be_bytes();
        let root = Arc::make_mut(&mut self.0);
        let a = Arc::make_mut(root.entry(a).or_default());
        let b = Arc::make_mut(a.entry(b).or_default());
        Arc::make_mut(b.entry(c).or_default()).insert(d, value);
    }
    pub fn remove(&mut self, slot: u32) {
        if self.get(slot).is_none() {
            return;
        }
        let [a, b, c, d] = slot.to_be_bytes();
        let root = Arc::make_mut(&mut self.0);
        let first = Arc::make_mut(root.get_mut(&a).unwrap());
        let second = Arc::make_mut(first.get_mut(&b).unwrap());
        let leaf = Arc::make_mut(second.get_mut(&c).unwrap());
        leaf.remove(&d);
        if leaf.is_empty() {
            second.remove(&c);
        }
        if second.is_empty() {
            first.remove(&b);
        }
        if first.is_empty() {
            root.remove(&a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn small_work_near_the_last_slot_stays_sparse_and_promotes_without_losing_members() {
        let mut slots = Slots::new(1_000_003, 3);
        for slot in [1_000_002, 1_000_000, 1_000_001] {
            slots.insert(slot, slot);
        }
        assert!(matches!(&slots, Slots::Sparse { entries, .. } if entries.len() == 3));
        assert_eq!(
            slots.iter().copied().collect::<Vec<_>>(),
            vec![1_000_000, 1_000_001, 1_000_002]
        );
        // The dense transition must preserve order and replacements as well as membership.
        slots.reserve_dense(1_000_003);
        slots.insert(1_000_001, 7);
        assert_eq!(
            slots.into_values().collect::<Vec<_>>(),
            vec![1_000_000, 7, 1_000_002]
        );
        let mut growing = Slots::new(16, 1);
        for slot in [15, 8, 9, 10] {
            growing.insert(slot, slot);
        }
        assert!(matches!(&growing, Slots::Dense(_)));
        assert_eq!(
            growing.into_values().collect::<Vec<_>>(),
            vec![8, 9, 10, 15]
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn requirement_cuts_keep_old_pages_across_writes_removal_and_directory_boundaries() {
        let keys = [0, 255, 256, 65_535, 65_536, 0x0100_0000, u32::MAX];
        let mut live = SlotCut::default();
        for slot in keys {
            live.insert(slot, slot);
        }
        let admitted = live.clone();
        assert!(Arc::ptr_eq(&live.0, &admitted.0));
        for slot in keys {
            live.insert(slot, 17);
        }
        for slot in keys {
            assert_eq!(admitted.get(slot), Some(&slot));
        }
        let updated = live.clone();
        for slot in keys {
            live.remove(slot);
        }
        assert!(live.0.is_empty());
        for slot in keys {
            assert_eq!(updated.get(slot), Some(&17));
            assert_eq!(admitted.get(slot), Some(&slot));
        }
    }
}

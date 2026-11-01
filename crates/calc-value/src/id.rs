//! Generational identities for workbooks, sheets, names, tables and calculation nodes.
//!
//! An ID is a slot index and a generation. An allocator issues IDs; `from_parts` reconstructs
//! an issued identity. Releasing an ID frees its slot, and the next ID issued for that
//! slot carries a new generation, so a stale ID never equals a live one. A slot whose generation
//! would wrap past `u32::MAX` is retired instead and never issued again.
//!
//! Uniqueness holds among the IDs one allocator issues. Two allocators can issue equal IDs, so
//! identity across workbooks is the pair of a [`WorkbookId`] and the ID inside that workbook.

use std::fmt;
use std::marker::PhantomData;

/// Why an [`IdAllocator`] refused to issue or release an ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdError {
    /// Every slot index the allocator can address is live or retired.
    Exhausted,
    /// The ID is not live in this allocator: it was released already, its slot was retired, or
    /// this allocator never issued it.
    NotLive,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdError::Exhausted => f.write_str("every slot of the ID allocator is live or retired"),
            IdError::NotLive => f.write_str("the ID is not live in this allocator"),
        }
    }
}

impl std::error::Error for IdError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotState {
    Live,
    Free,
    Retired,
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    generation: u32,
    state: SlotState,
}

/// The slots behind one allocator: each slot's current generation and state, and the free slots
/// in release order (the most recently released slot is issued first).
#[derive(Debug)]
struct SlotTable {
    slots: Vec<Slot>,
    free: Vec<u32>,
    /// The number of slot indices this table may create; `u32::MAX` outside its unit tests.
    limit: u32,
}

impl SlotTable {
    const fn new() -> Self {
        SlotTable {
            slots: Vec::new(),
            free: Vec::new(),
            limit: u32::MAX,
        }
    }

    /// Issues a free slot at its current generation, or a new slot at generation 0.
    fn issue(&mut self) -> Result<(u32, u32), IdError> {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.state = SlotState::Live;
            return Ok((index, slot.generation));
        }
        let index = u32::try_from(self.slots.len())
            .ok()
            .filter(|index| *index < self.limit)
            .ok_or(IdError::Exhausted)?;
        self.slots.push(Slot {
            generation: 0,
            state: SlotState::Live,
        });
        Ok((index, 0))
    }

    /// Releases a live slot: it is freed at the next generation, or retired when that generation
    /// would wrap.
    fn release(&mut self, index: u32, generation: u32) -> Result<(), IdError> {
        let slot = self
            .slots
            .get_mut(index as usize)
            .filter(|slot| slot.state == SlotState::Live && slot.generation == generation)
            .ok_or(IdError::NotLive)?;
        match generation.checked_add(1) {
            Some(next) => {
                slot.generation = next;
                slot.state = SlotState::Free;
                self.free.push(index);
            }
            None => slot.state = SlotState::Retired,
        }
        Ok(())
    }

    fn is_live(&self, index: u32, generation: u32) -> bool {
        self.slots
            .get(index as usize)
            .is_some_and(|slot| slot.state == SlotState::Live && slot.generation == generation)
    }
}

/// Issues and releases the IDs of one kind (`IdAllocator<SheetId>`, `IdAllocator<NodeId>`, ...).
/// It is deliberately not `Clone`: a copy would issue the same IDs again.
pub struct IdAllocator<T> {
    table: SlotTable,
    kind: PhantomData<fn() -> T>,
}

impl<T> IdAllocator<T> {
    /// An allocator that has issued nothing.
    pub const fn new() -> Self {
        IdAllocator {
            table: SlotTable::new(),
            kind: PhantomData,
        }
    }

    /// The number of slots this allocator has created (live, free or retired).
    pub fn slot_count(&self) -> usize {
        self.table.slots.len()
    }
}

impl<T> Default for IdAllocator<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Debug for IdAllocator<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdAllocator")
            .field("kind", &std::any::type_name::<T>())
            .field("slots", &self.table.slots.len())
            .field("free", &self.table.free.len())
            .finish()
    }
}

macro_rules! generational_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        ///
        /// A slot and a generation, issued by an allocator. IDs order by slot, then
        /// generation.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name {
            slot: u32,
            generation: u32,
        }

        impl $name {
            /// The slot index; a released slot is issued again under a new generation.
            pub const fn slot(self) -> u32 {
                self.slot
            }

            /// The generation of the slot when this ID was issued.
            pub const fn generation(self) -> u32 {
                self.generation
            }

            /// The ID with this slot and generation: an ID an allocator issued elsewhere, read
            /// back from a message that carried its [`Self::slot`] and [`Self::generation`] (DNA
            /// Calc's page and its calculation worker, dc-p96). It is live in no allocator on
            /// this side, and names whatever the issuing side's ID names.
            pub const fn from_parts(slot: u32, generation: u32) -> Self {
                $name { slot, generation }
            }
        }

        impl IdAllocator<$name> {
            /// Issues a new ID: the most recently released free slot at its next generation, or
            /// a new slot. Fails only when every addressable slot is live or retired.
            pub fn issue(&mut self) -> Result<$name, IdError> {
                let (slot, generation) = self.table.issue()?;
                Ok($name { slot, generation })
            }

            /// Releases a live ID, so that it is never live again. Fails when the ID is not
            /// live in this allocator.
            pub fn release(&mut self, id: $name) -> Result<(), IdError> {
                self.table.release(id.slot, id.generation)
            }

            /// Whether this allocator issued the ID and has not released it.
            pub fn is_live(&self, id: $name) -> bool {
                self.table.is_live(id.slot, id.generation)
            }
        }
    };
}

generational_id! {
    /// The identity of an open workbook.
    WorkbookId
}

generational_id! {
    /// The identity of a sheet inside one workbook.
    SheetId
}

generational_id! {
    /// The identity of a defined name inside one workbook.
    NameId
}

/// The namespace of a defined name inside one workbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NameScope {
    Workbook,
    Sheet(SheetId),
}

/// A case-folded namespace watch, including names that do not exist yet.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NameKey {
    pub scope: NameScope,
    pub folded: String,
}
impl NameKey {
    pub fn new(scope: NameScope, spelling: &str) -> Self {
        Self {
            scope,
            folded: crate::fold_workbook_name(spelling),
        }
    }
}

generational_id! {
    /// The identity of a table inside one workbook.
    TableId
}

generational_id! {
    /// The identity of a calculation node (a formula participant) inside one workbook.
    NodeId
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_released_slot_is_issued_again_under_the_next_generation() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let first = sheets.issue().unwrap();
        let second = sheets.issue().unwrap();
        assert_eq!((first.slot(), first.generation()), (0, 0));
        assert_eq!((second.slot(), second.generation()), (1, 0));

        sheets.release(first).unwrap();
        assert!(!sheets.is_live(first));
        assert_eq!(sheets.release(first), Err(IdError::NotLive));

        let again = sheets.issue().unwrap();
        assert_eq!((again.slot(), again.generation()), (0, 1));
        assert_ne!(again, first);
        assert!(sheets.is_live(again));
        assert!(sheets.is_live(second));
        assert_eq!(sheets.slot_count(), 2);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_slot_whose_generation_would_wrap_is_retired_and_never_reused() {
        let mut nodes: IdAllocator<NodeId> = IdAllocator::new();
        let first = nodes.issue().unwrap();
        // Advance the live slot to the last generation without four billion releases.
        nodes.table.slots[0].generation = u32::MAX;
        let last = NodeId {
            slot: first.slot(),
            generation: u32::MAX,
        };
        assert!(nodes.is_live(last));

        nodes.release(last).unwrap();
        assert!(!nodes.is_live(last));
        assert_eq!(nodes.release(last), Err(IdError::NotLive));
        assert_eq!(nodes.table.slots[0].state, SlotState::Retired);
        assert!(nodes.table.free.is_empty());

        let next = nodes.issue().unwrap();
        assert_eq!((next.slot(), next.generation()), (1, 0));
        assert_eq!(nodes.slot_count(), 2);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_allocator_with_every_slot_live_or_retired_is_exhausted() {
        let mut names: IdAllocator<NameId> = IdAllocator::new();
        names.table.limit = 2;
        let first = names.issue().unwrap();
        let _second = names.issue().unwrap();
        assert_eq!(names.issue(), Err(IdError::Exhausted));

        names.release(first).unwrap();
        assert_eq!(
            names.issue().map(|id| (id.slot(), id.generation())),
            Ok((0, 1))
        );
        assert_eq!(names.issue(), Err(IdError::Exhausted));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_id_the_allocator_never_issued_is_not_live() {
        let mut tables: IdAllocator<TableId> = IdAllocator::new();
        let never = TableId {
            slot: 5,
            generation: 0,
        };
        assert!(!tables.is_live(never));
        assert_eq!(tables.release(never), Err(IdError::NotLive));
        let issued = tables.issue().unwrap();
        let stale = TableId {
            slot: issued.slot(),
            generation: 1,
        };
        assert_eq!(tables.release(stale), Err(IdError::NotLive));
        assert!(tables.is_live(issued));
    }
}

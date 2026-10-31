//! Typed, versioned state registered by a skin. Registration needs no central state enum.
use crate::{SkinId, SurfaceId};
use calc_value::{CellKey, SheetId};
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};

struct Entry {
    schema: u32,
    value: Box<dyn Any + Send + Sync>,
    reset: fn() -> Box<dyn Any + Send + Sync>,
    pending_reason: fn(&dyn Any) -> Option<&'static str>,
}
pub trait ViewState: Default + Send + Sync + 'static {
    const SCHEMA: u32 = 1;
    fn pending_reason(&self) -> Option<&'static str> {
        None
    }
}
#[derive(Default)]
pub struct ViewStateStore {
    entries: BTreeMap<SkinId, Entry>,
}
impl ViewStateStore {
    pub fn register<T: ViewState>(&mut self, skin: SkinId) -> bool {
        let schema = T::SCHEMA;
        if let Some(entry) = self.entries.get(&skin) {
            assert!(
                entry.schema != schema || entry.value.is::<T>(),
                "a skin registered two state types under one schema"
            );
        }
        let replace = self
            .entries
            .get(&skin)
            .is_none_or(|entry| entry.schema != schema);
        if replace {
            self.entries.insert(
                skin,
                Entry {
                    schema,
                    value: Box::<T>::default(),
                    reset: || Box::<T>::default(),
                    pending_reason: |value| value.downcast_ref::<T>().unwrap().pending_reason(),
                },
            );
        }
        replace
    }
    pub fn get<T: 'static>(&self, skin: SkinId) -> Option<&T> {
        self.entries.get(&skin)?.value.downcast_ref()
    }
    pub fn get_mut<T: 'static>(&mut self, skin: SkinId) -> Option<&mut T> {
        self.entries.get_mut(&skin)?.value.downcast_mut()
    }
    pub fn reset(&mut self) {
        for entry in self.entries.values_mut() {
            entry.value = (entry.reset)();
        }
    }
    pub fn pending_reason(&self) -> Option<&'static str> {
        self.entries
            .values()
            .find_map(|entry| (entry.pending_reason)(entry.value.as_ref()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusRegistry {
    surfaces: BTreeSet<SurfaceId>,
}
impl FocusRegistry {
    pub fn register(&mut self, id: SurfaceId) {
        self.surfaces.insert(id);
    }
    pub fn contains(&self, id: SurfaceId) -> bool {
        self.surfaces.contains(&id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LedgerState {
    pub filter: String,
    pub text: String,
    pub sort: String,
    pub grouped: bool,
    pub this_sheet: bool,
    pub page: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotebookState {
    pub insert_text: String,
    pub address: String,
    pub prose: bool,
    pub pages: BTreeMap<SheetId, usize>,
    pub selected: Option<String>,
    pub last_active: Option<CellKey>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Branch {
    Workbook,
    Names,
    Sheet(SheetId),
    Row(SheetId, u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeFocus {
    Branch(Branch),
    Name(usize),
    Cell(CellKey),
    MoreRows(SheetId),
    MoreCells((SheetId, u32)),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeState {
    pub folded: BTreeSet<Branch>,
    pub shown_rows: BTreeMap<SheetId, usize>,
    pub focus: Option<TreeFocus>,
    pub query: String,
    pub initialized: bool,
    pub last_active: Option<CellKey>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridState {
    pub views: BTreeMap<(SheetId, u8), crate::viewport::GridView>,
    pub active: BTreeMap<SheetId, u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanvasState {
    pub back: u8,
    pub forward: u8,
    pub zoom: u16,
}
impl Default for CanvasState {
    fn default() -> Self {
        Self {
            back: 1,
            forward: 1,
            zoom: 100,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AtlasState {
    observed_revision: Option<u64>,
    pub reason_page: usize,
    pub selected: Option<String>,
    pub cell_page: usize,
}
impl AtlasState {
    /// A report's ordering can change with workbook facts. Keep the selected reason for recovery,
    /// but start its lists again so newly added reasons and cells remain reachable.
    pub fn observe_revision(&mut self, revision: u64) {
        if self.observed_revision != Some(revision) {
            self.observed_revision = Some(revision);
            self.reason_page = 0;
            self.cell_page = 0;
        }
    }
}
impl ViewState for CanvasState {}
impl ViewState for AtlasState {}
impl ViewState for GridState {}
impl ViewState for LedgerState {}
impl ViewState for TreeState {}
impl ViewState for NotebookState {
    fn pending_reason(&self) -> Option<&'static str> {
        (!self.insert_text.is_empty()).then_some(
            "Return to Notebook to insert or clear the new block content before downloading.",
        )
    }
}

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

/// How a mounted surface routes input. DOM elements belong to its browser adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceRole {
    Navigation { skin: SkinId },
    Editor { continuation: EditorContinuation },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorContinuation {
    SameTarget,
    Selection,
    Navigation(SkinId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardOwner {
    Native,
    Workbook,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceDescriptor {
    pub id: SurfaceId,
    pub role: SurfaceRole,
    pub clipboard: ClipboardOwner,
}
impl SurfaceDescriptor {
    pub const fn editor(id: SurfaceId) -> Self {
        Self {
            id,
            role: SurfaceRole::Editor {
                continuation: EditorContinuation::SameTarget,
            },
            clipboard: ClipboardOwner::Native,
        }
    }
    /// Overlay editors return subsequent queued keys to their skin after a commit closes them.
    pub const fn editor_with_navigation(id: SurfaceId, skin: SkinId) -> Self {
        Self {
            id,
            role: SurfaceRole::Editor {
                continuation: EditorContinuation::Navigation(skin),
            },
            clipboard: ClipboardOwner::Native,
        }
    }
    pub const fn selection_editor(id: SurfaceId) -> Self {
        Self {
            id,
            role: SurfaceRole::Editor {
                continuation: EditorContinuation::Selection,
            },
            clipboard: ClipboardOwner::Native,
        }
    }
    pub const fn navigation(skin: SkinId, clipboard: ClipboardOwner) -> Self {
        Self {
            id: SurfaceId::navigation(skin),
            role: SurfaceRole::Navigation { skin },
            clipboard,
        }
    }
}
/// A shell focus request is valid only while no newer request or native attention supersedes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusTicket(u64);
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusRegistry {
    surfaces: BTreeMap<SurfaceId, SurfaceDescriptor>,
    epoch: u64,
    observed: Option<SurfaceId>,
}
impl FocusRegistry {
    pub fn request(&mut self) -> FocusTicket {
        self.epoch = self.epoch.checked_add(1).expect("focus identity exhausted");
        FocusTicket(self.epoch)
    }
    pub fn observe(&mut self, surface: Option<SurfaceId>) {
        self.observed = surface;
        self.request();
    }
    pub fn observed(&self) -> Option<SurfaceId> {
        self.observed
    }
    pub fn snapshot(&self) -> FocusTicket {
        FocusTicket(self.epoch)
    }
    pub fn accepts(&self, ticket: FocusTicket) -> bool {
        self.epoch == ticket.0
    }

    pub fn register(&mut self, id: SurfaceId) -> bool {
        if self.contains(id) {
            false
        } else {
            self.register_surface(SurfaceDescriptor::editor(id))
        }
    }
    pub fn register_surface(&mut self, descriptor: SurfaceDescriptor) -> bool {
        if self.surfaces.get(&descriptor.id) == Some(&descriptor) {
            return false;
        }
        self.surfaces.insert(descriptor.id, descriptor);
        true
    }
    pub fn contains(&self, id: SurfaceId) -> bool {
        self.surfaces.contains_key(&id)
    }
    pub fn surface(&self, id: SurfaceId) -> Option<SurfaceDescriptor> {
        self.surfaces.get(&id).copied()
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

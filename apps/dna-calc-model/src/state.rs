//! Application state with no browser, reactive toolkit or host dependency.
use crate::{SkinId, SurfaceId, ViewStateStore};
use calc_value::{CellKey, GridArea, SheetId};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionState {
    /// Explicit navigation, including a request to reveal the already active cell.
    pub reveal_epoch: u64,
    pub sheet: Option<SheetId>,
    pub active: (u32, u32),
    pub corner: (u32, u32),
    pub areas: Vec<GridArea>,
    pub remembered: BTreeMap<SheetId, (CellKey, Vec<GridArea>)>,
}
impl Default for SelectionState {
    fn default() -> Self {
        Self {
            reveal_epoch: 0,
            sheet: None,
            active: (1, 1),
            corner: (1, 1),
            areas: Vec::new(),
            remembered: BTreeMap::new(),
        }
    }
}
impl SelectionState {
    pub fn active_key(&self) -> Option<CellKey> {
        CellKey::new(self.sheet?, self.active.0, self.active.1).ok()
    }
    pub fn request_reveal(&mut self) {
        self.reveal_epoch = self.reveal_epoch.wrapping_add(1);
    }
    pub fn select(&mut self, at: CellKey, areas: Vec<GridArea>) {
        self.request_reveal();
        if self.sheet != Some(at.sheet())
            && let Some(previous) = self.active_key()
        {
            self.remembered
                .insert(previous.sheet(), (previous, self.areas.clone()));
        }
        self.sheet = Some(at.sheet());
        self.active = (at.row(), at.column());
        self.corner = areas
            .first()
            .map_or(self.active, |a| crate::selection::opposite(*a, at));
        self.areas = areas;
    }
}

/// Registered surfaces share one target, draft and cancel/commit boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditSession {
    /// Changes on a new session, retaining identity across surface transfers and completion.
    pub id: u64,
    pub surface: Option<SurfaceId>,
    pub target: Option<CellKey>,
    pub original: String,
    pub draft: String,
    pub caret: Option<TextSelection>,
    pub areas: Vec<GridArea>,
    pub point_mode: bool,
    pub pointed: Option<PointPick>,
    pub pointed_text: String,
}
/// UTF-16 offsets, matching the shared formula editor's text operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelection {
    pub start: usize,
    pub end: usize,
    pub backward: bool,
}
impl TextSelection {
    pub fn bounded(self, len: usize) -> Self {
        Self {
            start: self.start.min(len),
            end: self.end.min(len).max(self.start.min(len)),
            backward: self.backward,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointPick {
    pub sheet: SheetId,
    pub start: usize,
    pub end: usize,
    pub anchor: (u32, u32),
    pub corner: (u32, u32),
}
impl EditSession {
    pub fn begin(
        &mut self,
        surface: SurfaceId,
        target: CellKey,
        original: String,
        seed: Option<String>,
        areas: Vec<GridArea>,
    ) {
        if self.surface.is_some() {
            self.surface = Some(surface);
            return;
        }
        self.id = self
            .id
            .checked_add(1)
            .expect("edit session identity exhausted");
        self.point_mode = seed.is_some();
        self.draft = seed.unwrap_or_else(|| original.clone());
        self.original = original;
        self.target = Some(target);
        self.areas = areas;
        self.pointed = None;
        let end = self.draft.encode_utf16().count();
        self.caret = Some(TextSelection {
            start: end,
            end,
            backward: false,
        });
        self.surface = Some(surface);
    }
    /// A transfer changes only the registered surface, retaining the original target and draft.
    pub fn transfer(&mut self, surface: SurfaceId) -> bool {
        if self.surface.is_none() || self.surface == Some(surface) {
            return false;
        }
        self.surface = Some(surface);
        true
    }
    pub fn finish(&mut self) {
        self.surface = None;
        self.target = None;
        self.pointed = None;
        self.caret = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindMode {
    Find,
    Replace,
    GoTo,
}
impl FindMode {
    pub fn stable_id(self) -> &'static str {
        match self {
            Self::Find => "find",
            Self::Replace => "replace",
            Self::GoTo => "goto",
        }
    }
}

#[derive(Default)]
pub struct Model {
    pub selection: SelectionState,
    pub edit: EditSession,
    pub views: ViewStateStore,
    pub active_skin: Option<String>,
    pub find: Option<FindMode>,
    pub focus: crate::FocusRegistry,
}
impl Model {
    /// A workbook replacement resets workbook-relative state, retaining registered schemas.
    pub fn reset_workbook(&mut self) {
        self.selection = SelectionState::default();
        self.edit = EditSession::default();
        self.find = None;
        self.focus.observe(None);
        self.views.reset();
    }
    pub fn reveal_completed(&mut self, skin: SkinId) {
        self.active_skin = Some(skin.stable_id().to_owned());
    }
}

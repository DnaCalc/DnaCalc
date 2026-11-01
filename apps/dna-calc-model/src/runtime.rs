//! The application values retained across mounts. Reactive signals and browser handles are
//! adapters over these fields, never a second writable copy.
use crate::ViewMirror;
use crate::projection::{self as wire, Outcome, Summary};
use calc_value::{GridArea, SheetId};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Snapshot<T>(std::sync::Arc<T>);
impl<T> Clone for Snapshot<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Default> Default for Snapshot<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
impl<T> Snapshot<T> {
    pub fn new(value: T) -> Self {
        Self(std::sync::Arc::new(value))
    }
}
impl<T> From<T> for Snapshot<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}
impl<T> std::ops::Deref for Snapshot<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: Clone> std::ops::DerefMut for Snapshot<T> {
    fn deref_mut(&mut self) -> &mut T {
        std::sync::Arc::make_mut(&mut self.0)
    }
}
impl<T: PartialEq> PartialEq for Snapshot<T> {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}
impl<T: Eq> Eq for Snapshot<T> {}

/// What the status line says, and whether it reports a refusal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub refused: bool,
}

/// How long the last open and the last intent took, in milliseconds of the browser's clock.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timings {
    pub open_ms: Option<f64>,
    pub last_intent_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub state: String,
    pub pending: usize,
}

/// Shared Find/Replace/Go To inputs and workbook-relative query results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolState {
    pub query: String,
    pub replacement: String,
    pub target: String,
    pub case: bool,
    pub whole: bool,
    pub wildcards: bool,
    pub formulas: bool,
    pub scope: String,
    pub found: Option<Snapshot<wire::Found>>,
    pub selected: usize,
    pub found_revision: u64,
    pub message: String,
    pub epoch: u64,
    pub anchor: Option<wire::Key>,
    pub areas: Vec<wire::Area>,
    pub permission: Option<Result<(), String>>,
    pub selection_stats: Option<Result<wire::SelectionStats, String>>,
}
impl Default for ToolState {
    fn default() -> Self {
        Self {
            query: String::new(),
            replacement: String::new(),
            target: String::new(),
            case: false,
            whole: false,
            wildcards: false,
            formulas: true,
            scope: "sheet".to_owned(),
            found: None,
            selected: 0,
            found_revision: 0,
            message: String::new(),
            epoch: 0,
            anchor: None,
            areas: Vec::new(),
            permission: None,
            selection_stats: None,
        }
    }
}
impl ToolState {
    /// Retire an outstanding search without changing its inputs or original selection.
    pub fn invalidate_search(&mut self) {
        self.epoch += 1;
        self.found = None;
        self.message.clear();
    }
    /// Keep user inputs; retire results, permissions and locations from the old workbook.
    pub fn reset_workbook(&mut self) {
        self.invalidate_search();
        self.selected = 0;
        self.found_revision = 0;
        self.anchor = None;
        self.areas.clear();
        self.permission = None;
        self.selection_stats = None;
    }
}

pub struct RuntimeState {
    pub tools: ToolState,
    pub active_calculation: Option<(u64, u64)>,
    pub calculation_frontier: u64,
    pub selection_focus: Option<crate::FocusTicket>,
    pub callback_focus: Option<crate::FocusTicket>,
    pub context_menu: Option<(f64, f64)>,
    pub review_open: bool,
    pub validation_decision: Option<wire::ValidationDecision>,
    pub validation_outcome: Option<Outcome>,
    pub structural_choice: Option<bool>,
    pub grid_geometry: Snapshot<BTreeMap<SheetId, wire::Geometry>>,
    pub next_subscription: u64,
    pub summary: Snapshot<Summary>,
    pub revision: u64,
    pub generation: u64,
    pub clipboard: Option<GridArea>,
    pub copied: Option<Snapshot<wire::Clipboard>>,
    pub copy_pending: usize,
    pub copy_epoch: u64,
    pub clipboard_fallback: Option<bool>,
    pub paste_preview: Option<wire::PastePreview>,
    pub paste_cut: bool,
    pub paste_mode: String,
    pub fill_series: String,
    pub cut: bool,
    pub selection: Snapshot<ViewMirror>,
    pub status: Status,
    pub timings: Timings,
    pub transport: Health,
    pub download_unconfirmed: bool,
    pub pending_actions: usize,
    pub replacing: bool,
    pub pending_changes: usize,
    pub selection_epoch: u64,
    pub deferred_version: u64,
    pub replaying: bool,
    pub flush_scheduled: bool,
    pub next_projection: u64,
    pub projected_views: BTreeMap<u64, Snapshot<ViewMirror>>,
}
impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            tools: ToolState::default(),
            active_calculation: None,
            calculation_frontier: 0,
            selection_focus: None,
            callback_focus: None,
            context_menu: None,
            review_open: false,
            validation_decision: None,
            validation_outcome: None,
            structural_choice: None,
            grid_geometry: Snapshot::new(BTreeMap::new()),
            next_subscription: 1,
            summary: Snapshot::new(Summary::default()),
            revision: 0,
            generation: 0,
            clipboard: None,
            copied: None,
            copy_pending: 0,
            copy_epoch: 0,
            clipboard_fallback: None,
            paste_preview: None,
            paste_cut: false,
            paste_mode: "all".to_owned(),
            fill_series: "series".to_owned(),
            cut: false,
            selection: Snapshot::new(ViewMirror::default()),
            status: Status::default(),
            timings: Timings::default(),
            transport: Health::default(),
            download_unconfirmed: false,
            pending_actions: 0,
            replacing: false,
            pending_changes: 0,
            selection_epoch: 0,
            deferred_version: 0,
            replaying: false,
            flush_scheduled: false,
            next_projection: 1,
            projected_views: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tool_tests {
    use super::*;

    fn populated() -> ToolState {
        ToolState {
            query: "needle".into(),
            replacement: "thread".into(),
            target: "C9".into(),
            case: true,
            whole: true,
            wildcards: true,
            formulas: false,
            scope: "workbook".into(),
            found: Some(
                wire::Found {
                    revision: 7,
                    rows: Vec::new(),
                    scanned: 3,
                    partial: false,
                }
                .into(),
            ),
            selected: 2,
            found_revision: 7,
            message: "Searching…".into(),
            epoch: 9,
            anchor: Some(wire::Key(0, 0, 2, 3)),
            areas: vec![wire::Area(wire::Key(0, 0, 2, 3), wire::Key(0, 0, 4, 5))],
            permission: Some(Ok(())),
            selection_stats: Some(Ok(wire::SelectionStats::default())),
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn search_invalidation_retires_receipt_and_preserves_inputs_and_anchor() {
        let mut state = populated();
        let before = state.clone();
        state.invalidate_search();
        assert_ne!(state.epoch, before.epoch);
        assert!(state.found.is_none());
        assert!(state.message.is_empty());
        assert_eq!(state.anchor, before.anchor);
        assert_eq!(state.areas, before.areas);
        state.epoch = before.epoch;
        state.found = before.found.clone();
        state.message = before.message.clone();
        assert_eq!(state, before);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn workbook_reset_keeps_preferences_and_drops_old_workbook_results() {
        let mut model = crate::Model::default();
        model.runtime.tools = populated();
        let before = model.runtime.tools.clone();
        model.reset_workbook();
        let state = &model.runtime.tools;
        assert_ne!(state.epoch, before.epoch);
        assert!(state.found.is_none() && state.anchor.is_none() && state.areas.is_empty());
        assert!(state.permission.is_none() && state.selection_stats.is_none());
        assert!(state.message.is_empty());
        assert_eq!((state.selected, state.found_revision), (0, 0));
        assert_eq!(
            (
                &state.query,
                &state.replacement,
                &state.target,
                &state.scope
            ),
            (
                &before.query,
                &before.replacement,
                &before.target,
                &before.scope
            )
        );
        assert_eq!(
            (state.case, state.whole, state.wildcards, state.formulas),
            (true, true, true, false)
        );
    }
}

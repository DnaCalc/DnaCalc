//! The application values retained across mounts. Reactive signals and browser handles are
//! adapters over these fields, never a second writable copy.
use crate::projection::{self as wire, Cells, Outcome, Summary};
use calc_value::{CellKey, GridArea, SheetId};
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

/// The cells of one subscription as the server last sent them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewMirror {
    seq: u64,
    cells: BTreeMap<CellKey, wire::Cell>,
}

impl ViewMirror {
    /// Applies the server's changed cells: a reset replaces every cell. A change that is not the
    /// next of the subscription is refused (`false`) unless it is a reset.
    pub fn apply(&mut self, cells: &Cells) -> bool {
        if !cells.reset && cells.seq != self.seq + 1 {
            return false;
        }
        if cells.reset {
            self.cells.clear();
        }
        for at in &cells.emptied {
            if let Some(at) = at.cell() {
                self.cells.remove(&at);
            }
        }
        for cell in &cells.cells {
            if let Some(at) = cell.at.cell() {
                self.cells.insert(at, cell.clone());
            }
        }
        self.seq = cells.seq;
        true
    }

    pub fn cell(&self, at: CellKey) -> Option<&wire::Cell> {
        self.cells.get(&at)
    }

    /// Every cell, in cell order.
    pub fn cells(&self) -> impl Iterator<Item = (&CellKey, &wire::Cell)> {
        self.cells.iter()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub state: String,
    pub pending: usize,
}

pub struct RuntimeState {
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

//! The application's controller: the workbook's server behind reactive signals, shared by the
//! shell and every skin. It is `Copy`, so any view can hold it.
//!
//! The workbook itself lives with its server ([`crate::server`]), reached through
//! [`crate::port`] by messages ([`crate::wire`], dc-p96): the controller sends a request and
//! applies the reply when it comes, so nothing here waits for the server.
//!
//! - Changes go through [`Controller::dispatch`] (a [`wire::Intent`]): when the outcome comes,
//!   the summary (sheets, name, calculation mode) is updated, the revision advances on a commit
//!   or a recalculation, the active cell follows the session's selection, and a refused intent's
//!   reason goes to the status line. The server pushes the cells of the page's subscriptions as
//!   they change; [`Controller::subscribe`] registers a [`ViewMirror`] for one.
//! - The application's own subscription follows the session's selection: the formula bar and the
//!   status line show the active cell from it.
//! - A skin reads what it shows with [`Controller::ask`], asking again when
//!   [`Controller::revision`] advances.
//!
//! The active sheet and cell are the controller's, so they stay when the skin changes; every move
//! of the active cell is also a session selection, so an undo restores the selection the edit was
//! made in.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-app/src/app.rs` (`CalcApp`'s workspace,
//! delta and selection signals, the dispatcher that marks the document changed) and
//! `src/dnacalc-skin-leptos/src/state_handles.rs` (shared state behind one handle), onto calc-host's
//! session.

use crate::grid::selection::{GridExtent, NavAction, next_active};
use crate::port;
use crate::wire::{self, Area, Cells, Event, Intent, Key, Outcome, Reply, Request, Sheet, Summary};
use crate::workbook::SheetTab;
use calc_host::value::{CellKey, GridArea, SheetId};
use dna_formula_ui::editor::{
    cycle_anchors, expects_reference, point_reference_span, replace_text,
};
use dna_formula_ui::formula_field::FormulaAssist;
use leptos::prelude::*;
use std::collections::BTreeMap;

/// What the status line says, and whether it reports a refusal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub refused: bool,
}

/// Where the text being edited is typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOrigin {
    /// The editor over the active cell.
    Cell,
    /// The formula bar.
    Bar,
}

/// A reference being pointed into the draft, separate from the workbook selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointPick {
    pub sheet: SheetId,
    pub start: usize,
    pub end: usize,
    pub anchor: (u32, u32),
    pub corner: (u32, u32),
}

fn field_selection(assist: FormulaAssist) -> (usize, usize) {
    assist.field.get_untracked().map_or_else(
        || {
            let at = assist.caret.get_untracked();
            (at, at)
        },
        |field| {
            (
                field.selection_start().ok().flatten().unwrap_or(0) as usize,
                field.selection_end().ok().flatten().unwrap_or(0) as usize,
            )
        },
    )
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

/// The bytes of the wasm instance's linear memory; `None` off the browser.
pub fn memory_bytes() -> Option<f64> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let memory: js_sys::WebAssembly::Memory = wasm_bindgen::memory().dyn_into().ok()?;
        let buffer: js_sys::ArrayBuffer = memory.buffer().dyn_into().ok()?;
        Some(f64::from(buffer.byte_length()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Milliseconds on a clock: the browser's, or off the browser the time since the first call.
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0
    }
}

#[derive(Clone, Copy)]
pub struct Controller {
    mirrors: StoredValue<BTreeMap<u64, RwSignal<ViewMirror>>, LocalStorage>,
    next_subscription: StoredValue<u64>,
    /// The workbook as the shell shows it: name, sheets, calculation mode, revision.
    pub summary: RwSignal<Summary>,
    /// Advances with every change to the workbook, an open included.
    pub revision: RwSignal<u64>,
    /// Advances with every workbook opened: subscriptions belong to one generation.
    pub generation: RwSignal<u64>,
    pub sheet: RwSignal<Option<SheetId>>,
    /// The active cell's row and column on the active sheet.
    pub active: RwSignal<(u32, u32)>,
    /// The selected range's corner opposite the active cell: the range is the rectangle of the
    /// two (one cell when they are the same).
    pub corner: RwSignal<(u32, u32)>,
    /// The range copied (Ctrl+C), which a paste copies from.
    pub clipboard: RwSignal<Option<GridArea>>,
    /// Whether the range on the clipboard was cut (Ctrl+X): a paste moves it.
    pub cut: RwSignal<bool>,
    /// The cells of the session's selection, as the application's subscription delivered them.
    pub selection: RwSignal<ViewMirror>,
    pub status: RwSignal<Status>,
    /// Where an edit is in progress, if one is.
    pub editing: RwSignal<Option<EditOrigin>>,
    /// The text being edited, or the active cell's input when nothing is.
    pub edit_text: RwSignal<String>,
    pub assist: FormulaAssist,
    edit_field: StoredValue<FormulaAssist>,
    edit_target: RwSignal<Option<CellKey>>,
    edit_original: StoredValue<String>,
    pub point_mode: RwSignal<bool>,
    pub pointed: RwSignal<Option<PointPick>>,
    pointed_text: StoredValue<String>,
    pub timings: RwSignal<Timings>,
    pub transport: RwSignal<port::Health>,
    pub download_unconfirmed: RwSignal<bool>,
    pub notebook_drafts: RwSignal<BTreeMap<CellKey, String>>,
}

impl Controller {
    /// A controller over a new workbook.
    pub fn new() -> Controller {
        let edit_text = RwSignal::new(String::new());
        let assist = FormulaAssist::new(edit_text);
        let controller = Controller {
            mirrors: StoredValue::new_local(BTreeMap::new()),
            next_subscription: StoredValue::new(1),
            summary: RwSignal::new(Summary::default()),
            revision: RwSignal::new(0),
            generation: RwSignal::new(0),
            sheet: RwSignal::new(None),
            active: RwSignal::new((1, 1)),
            corner: RwSignal::new((1, 1)),
            clipboard: RwSignal::new(None),
            cut: RwSignal::new(false),
            selection: RwSignal::new(ViewMirror::default()),
            status: RwSignal::new(Status::default()),
            editing: RwSignal::new(None),
            edit_text,
            assist,
            edit_field: StoredValue::new(assist),
            edit_target: RwSignal::new(None),
            edit_original: StoredValue::new(String::new()),
            point_mode: RwSignal::new(false),
            pointed: RwSignal::new(None),
            pointed_text: StoredValue::new(String::new()),
            timings: RwSignal::new(Timings::default()),
            transport: RwSignal::new(port::Health::default()),
            download_unconfirmed: RwSignal::new(false),
            notebook_drafts: RwSignal::new(BTreeMap::new()),
        };
        port::start();
        port::set_events(move |event| controller.receive(event));
        port::set_health(move |health| {
            controller.transport.try_set(health);
        });
        controller.new_workbook();
        controller
    }

    /// Asks the server; `answer` gets the reply when it comes (a failure goes to the status line).
    pub fn ask(&self, request: Request, answer: impl FnOnce(Reply) + 'static) {
        let controller = *self;
        port::call(request, None, move |reply, _| match reply {
            Ok(reply) => answer(reply),
            Err(message) => controller.say(format!("The workbook did not answer: {message}"), true),
        });
    }

    /// The sheets, in workbook order.
    pub fn sheets(&self) -> Vec<SheetTab> {
        self.summary.with(|summary| {
            summary
                .sheets
                .iter()
                .map(|(sheet, name)| SheetTab {
                    id: sheet.id(),
                    name: name.clone(),
                })
                .collect()
        })
    }

    /// The active cell's key.
    pub fn active_key(&self) -> Option<CellKey> {
        let sheet = self.sheet.get()?;
        let (row, column) = self.active.get();
        CellKey::new(sheet, row, column).ok()
    }

    pub fn active_key_untracked(&self) -> Option<CellKey> {
        let sheet = self.sheet.get_untracked()?;
        let (row, column) = self.active.get_untracked();
        CellKey::new(sheet, row, column).ok()
    }

    /// The active cell as the selection subscription holds it; `None` when it is empty.
    pub fn active_cell(&self) -> Option<wire::Cell> {
        let at = self.active_key()?;
        self.selection.with(|mirror| mirror.cell(at).cloned())
    }

    fn say(&self, text: impl Into<String>, refused: bool) {
        self.status.set(Status {
            text: text.into(),
            refused,
        });
    }

    /// The server's events: a subscription's changed cells go to its mirror.
    fn receive(&self, event: Event) {
        match event {
            Event::Cells(cells) => {
                if let Some(mirror) = self.mirrors.with_value(|m| m.get(&cells.id).copied()) {
                    mirror.try_update(|mirror| {
                        if !mirror.apply(&cells) {
                            // A gap means a change was lost: the next reset brings the mirror back.
                            *mirror = ViewMirror::default();
                        }
                    });
                }
            }
        }
    }

    /// A new workbook is open (an open or a new workbook): a new generation, the first sheet
    /// active at A1, and the application's selection subscription.
    fn installed(&self, summary: Summary, message: String) {
        self.mirrors.update_value(BTreeMap::clear);
        let first = summary.sheets.first().map(|(sheet, _)| sheet.id());
        self.summary.set(summary);
        self.download_unconfirmed.set(false);
        self.notebook_drafts.set(BTreeMap::new());
        self.assist.readonly.set(false);
        self.sheet.set(first);
        self.active.set((1, 1));
        self.corner.set((1, 1));
        self.clipboard.set(None);
        self.editing.set(None);
        self.selection.set(ViewMirror::default());
        self.subscribe(wire::Interest::Selection, self.selection);
        if let Some(at) = self.active_key_untracked()
            && let Ok(area) = GridArea::new(at, at)
        {
            self.dispatch(
                Intent::Select {
                    active: Key::of(at),
                    area: Area::of(area),
                },
                |_| {},
            );
        }
        self.generation.update(|n| *n += 1);
        self.revision.update(|n| *n += 1);
        self.say(message, false);
    }

    /// Replacing a workbook also risks an uncommitted draft or an unconfirmed download.
    pub fn has_dirty_work(&self) -> bool {
        self.summary.get().changed
            || self.download_unconfirmed.get()
            || !self.notebook_drafts.with(BTreeMap::is_empty)
            || (self.editing.get().is_some()
                && self.edit_text.get() != self.edit_original.get_value())
    }

    pub fn focus_pending_draft(&self) {
        if self.editing.get_untracked().is_some() {
            self.focus_editor();
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let sheet = self.sheet.get_untracked();
            let at = self.notebook_drafts.with_untracked(|drafts| {
                drafts.keys().find(|at| Some(at.sheet()) == sheet).copied()
            });
            if let Some(at) = at {
                let selector = format!(
                    "[data-dnacalc=block][data-cell={}] input",
                    crate::cells::address(at.row(), at.column())
                );
                if let Some(field) = document()
                    .query_selector(&selector)
                    .ok()
                    .flatten()
                    .and_then(|field| field.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    let _ = field.focus();
                    return;
                }
            }
            crate::grid::focus_grid();
        }
    }

    pub fn history_available(&self, redo: bool) -> bool {
        if self.assist.readonly.get() {
            return false;
        }
        if self.editing.get().is_some() {
            self.assist.history_available(redo)
        } else {
            self.summary.with(|summary| {
                if redo {
                    summary.redo > 0
                } else {
                    summary.undo > 0
                }
            })
        }
    }

    pub fn value_state(&self) -> dna_formula_ui::result::ValueState {
        use dna_formula_ui::result::ValueState as S;
        if self.transport.get().pending > 0 {
            return S::Pending;
        }
        let Some(cell) = self.active_cell() else {
            return S::Empty;
        };
        match cell.provenance {
            crate::cells::Provenance::Refused => S::Refused,
            crate::cells::Provenance::Stale => S::Stale,
            crate::cells::Provenance::FileCache => S::FileCache,
            _ if cell.kind == "error" => S::WorksheetError,
            crate::cells::Provenance::Empty => S::Empty,
            crate::cells::Provenance::Literal => S::Literal,
            crate::cells::Provenance::Calculated => S::Calculated,
        }
    }

    /// A new, empty workbook.
    pub fn new_workbook(&self) {
        let controller = *self;
        port::call(Request::New, None, move |reply, _| match reply {
            Ok(Reply::Done { outcome, .. }) => {
                controller.installed(outcome.summary, "New workbook".to_owned())
            }
            Err(error) => controller.say(format!("New workbook failed: {error}"), true),
            _ => controller.say("New workbook did not answer", true),
        });
    }

    /// Opens a package's bytes under `name`; the current workbook stays when it is refused.
    /// `opened` hears whether it opened.
    pub fn open(&self, name: &str, bytes: Vec<u8>, opened: impl FnOnce(bool) + 'static) {
        let controller = *self;
        let size = bytes.len();
        let name = name.to_owned();
        port::call(
            Request::Open { name: name.clone() },
            Some(bytes),
            move |reply, _| match reply {
                Ok(Reply::Done {
                    outcome,
                    opened: Some((elapsed, calculated)),
                    ..
                }) => {
                    controller
                        .timings
                        .update(|timings| timings.open_ms = Some(elapsed));
                    let message = match calculated {
                        Some(calculated) => format!(
                            "Opened {name}: {size} bytes in {elapsed:.0} ms: read in {:.0} ms, calculated in {calculated:.0} ms",
                            elapsed - calculated
                        ),
                        None => format!(
                            "Opened {name}: {size} bytes in {elapsed:.0} ms; the file calculates manually, so its formulas show the values it stored until Calculate"
                        ),
                    };
                    controller.installed(outcome.summary, message);
                    opened(true);
                }
                Ok(Reply::Done { outcome, .. }) => {
                    controller.say(
                        format!("{name} did not open: {}", outcome.error.unwrap_or_default()),
                        true,
                    );
                    opened(false);
                }
                Ok(_) | Err(_) => {
                    controller.say(format!("{name} did not open"), true);
                    opened(false);
                }
            },
        );
    }

    /// Carries out an intent; when its outcome comes, applies it (the summary, the revision, the
    /// selection, the status line) and hands it to `then`.
    pub fn dispatch(&self, intent: Intent, then: impl FnOnce(&Outcome) + 'static) {
        self.dispatch_result(intent, move |result| {
            if let Ok(outcome) = result {
                then(&outcome);
            }
        });
    }

    fn dispatch_result(
        &self,
        intent: Intent,
        then: impl FnOnce(Result<Outcome, String>) + 'static,
    ) {
        let controller = *self;
        port::call(Request::Intent(intent), None, move |reply, _| match reply {
            Ok(Reply::Done { outcome, .. }) => {
                controller.applied(&outcome);
                then(Ok(outcome));
            }
            reply => {
                let message = match reply {
                    Err(message) => format!("The workbook did not answer: {message}"),
                    _ => "The workbook answered an intent with something else".to_owned(),
                };
                controller.say(message.clone(), true);
                then(Err(message));
            }
        });
    }

    fn applied(&self, outcome: &Outcome) {
        self.timings
            .update(|timings| timings.last_intent_ms = Some(outcome.elapsed_ms));
        if outcome.summary != self.summary.get_untracked() {
            self.summary.set(outcome.summary.clone());
        }
        // The active cell follows the session's selection: an undo or a redo restores the
        // selection of its edit, which may be on another sheet, and then that sheet is shown (as
        // Excel does), so the formula bar and the status line read the cell the selection holds.
        if let Some((at, area)) = &outcome.selection
            && let Some(at) = at.cell()
        {
            if Some(at.sheet()) != self.sheet.get_untracked() {
                self.sheet.set(Some(at.sheet()));
            }
            if (at.row(), at.column()) != self.active.get_untracked() {
                self.active.set((at.row(), at.column()));
            }
            let corner = area
                .and_then(Area::area)
                .map_or((at.row(), at.column()), |area| opposite(area, at));
            if corner != self.corner.get_untracked() {
                self.corner.set(corner);
            }
        }
        // Only a change of the workbook or its calculation advances the revision: a selection or
        // an interest moved changes no cell, so what reads the workbook is not read again.
        if outcome.committed || outcome.recalculated {
            self.revision.update(|n| *n += 1);
        }
        match &outcome.error {
            Some(error) => self.say(error.clone(), true),
            None => {
                if let Some(recalc) = &outcome.recalc {
                    self.say(
                        format!("{} in {:.0} ms", capitalised(recalc), outcome.elapsed_ms),
                        false,
                    );
                }
                if let Some((kind, text)) = &outcome.note {
                    self.say(format!("{kind}, entry kept: {text}"), true);
                }
            }
        }
    }

    /// Moves the active cell and the session's selection to one cell.
    pub fn select(&self, row: u32, column: u32) {
        self.select_range((row, column), (row, column));
    }

    /// Selects the range from the active cell `active` to its opposite corner `corner`.
    pub fn select_range(&self, active: (u32, u32), corner: (u32, u32)) {
        self.active.set(active);
        self.corner.set(corner);
        if let (Some(at), Some(area)) =
            (self.active_key_untracked(), self.selected_area_untracked())
        {
            self.dispatch(
                Intent::Select {
                    active: Key::of(at),
                    area: Area::of(area),
                },
                |_| {},
            );
        }
    }

    /// Moves the selected range's far corner, keeping the active cell (Shift+Arrow, Shift+click).
    pub fn extend(&self, corner: (u32, u32)) {
        self.select_range(self.active.get_untracked(), corner);
    }

    /// The selected range.
    pub fn selected_area(&self) -> Option<GridArea> {
        self.active.track();
        self.corner.track();
        self.selected_area_untracked()
    }

    pub fn selected_area_untracked(&self) -> Option<GridArea> {
        let sheet = self.sheet.get_untracked()?;
        let (row, column) = self.active.get_untracked();
        let (corner_row, corner_column) = self.corner.get_untracked();
        GridArea::new(
            CellKey::new(sheet, row, column).ok()?,
            CellKey::new(sheet, corner_row, corner_column).ok()?,
        )
        .ok()
    }

    /// A cell's label with its sheet's name: `Sheet1!B3`.
    pub fn cell_label(&self, at: CellKey) -> String {
        self.summary
            .with_untracked(|summary| dna_formula_ui::display::cell(summary, at))
    }

    /// An area's label with its sheet's name: `Sheet1!A1:B3`.
    pub fn area_label(&self, area: GridArea) -> String {
        self.summary
            .with_untracked(|summary| dna_formula_ui::display::area(summary, area))
    }

    /// Copies the selected range (Ctrl+C): a paste copies from it.
    pub fn copy(&self) {
        self.take(false);
    }

    /// Cuts the selected range (Ctrl+X): the next paste moves it there.
    pub fn cut(&self) {
        self.take(true);
    }

    fn take(&self, cut: bool) {
        if let Some(area) = self.selected_area_untracked() {
            self.clipboard.set(Some(area));
            self.cut.set(cut);
            let label = self.area_label(area);
            let verb = if cut { "Cut" } else { "Copied" };
            self.say(format!("{verb} {label}: select where to paste it"), false);
        }
    }

    /// Pastes the copied range into the selected one (Ctrl+V): one calc-host copy, which
    /// relocates formulas and repeats the source over a larger target; after a cut, a move, which
    /// empties the source and takes references to the moved cells with them, and empties the
    /// clipboard, as Excel does.
    pub fn paste(&self) {
        let (Some(from), Some(to)) = (
            self.clipboard.get_untracked(),
            self.selected_area_untracked(),
        ) else {
            self.say("Nothing is copied to paste", true);
            return;
        };
        let cut = self.cut.get_untracked();
        let controller = *self;
        self.dispatch(
            Intent::Copy {
                from: Area::of(from),
                to: Area::of(to),
                cut,
            },
            move |outcome| {
                if cut && outcome.accepted {
                    controller.clipboard.set(None);
                    controller.cut.set(false);
                }
            },
        );
    }

    /// Fills the selected range down from its first row (Ctrl+D) or right from its first column
    /// (Ctrl+R): a calc-host copy of that row or column over the rest.
    pub fn fill(&self, down: bool) {
        let Some(area) = self.selected_area_untracked() else {
            return;
        };
        match fill_parts(area, down) {
            Some((from, to)) => self.dispatch(
                Intent::Copy {
                    from: Area::of(from),
                    to: Area::of(to),
                    cut: false,
                },
                |_| {},
            ),
            None => self.say(
                if down {
                    "Fill down needs a range of more than one row"
                } else {
                    "Fill right needs a range of more than one column"
                },
                true,
            ),
        }
    }

    /// Shows another sheet, at its A1.
    pub fn show_sheet(&self, sheet: SheetId) {
        if self.editing.get_untracked().is_some() {
            if self.edit_text.get_untracked().trim_start().starts_with('=') {
                self.carry_edit_to_bar();
            } else {
                let controller = *self;
                self.commit_edit(move |ok| {
                    if ok {
                        controller.show_sheet(sheet);
                    } else {
                        controller.focus_editor();
                    }
                });
                return;
            }
        }
        self.sheet.set(Some(sheet));
        self.select(1, 1);
    }

    /// Makes `at` the active cell, showing its sheet (a skin's jump to a cell: the tree's, the
    /// canvas's, the ledger's).
    pub fn go_to(&self, at: CellKey) {
        self.navigate_to(at, |_| {});
    }

    /// Navigate after resolving the current draft; a rejected draft keeps the original address.
    pub fn navigate_to(&self, at: CellKey, arrived: impl FnOnce(bool) + 'static) {
        if self.editing.get_untracked().is_some() {
            let controller = *self;
            self.commit_edit(move |ok| {
                if ok {
                    controller.navigate_to(at, arrived);
                } else {
                    controller.focus_editor();
                    arrived(false);
                }
            });
            return;
        }
        if Some(at.sheet()) != self.sheet.get_untracked() {
            self.cancel_edit();
            self.sheet.set(Some(at.sheet()));
        }
        self.select(at.row(), at.column());
        arrived(true);
    }

    /// Starts an edit of the active cell: with `seed`, or the cell's input when `None`.
    pub fn begin_edit(&self, origin: EditOrigin, seed: Option<String>) {
        if self.editing.get_untracked().is_some() {
            if self.editing.get_untracked() != Some(origin) {
                self.editing.set(Some(origin));
            }
            return;
        }
        let replacing = seed.is_some();
        let original = self
            .active_key_untracked()
            .and_then(|at| {
                self.selection
                    .with_untracked(|mirror| mirror.cell(at).map(|cell| cell.input.clone()))
            })
            .unwrap_or_default();
        let text = seed.unwrap_or_else(|| original.clone());
        self.edit_text.set(text);
        self.edit_target.set(self.active_key_untracked());
        self.point_mode.set(replacing);
        self.pointed.set(None);
        self.edit_original.set_value(original.clone());
        self.assist.start_edit(original);
        self.editing.set(Some(origin));
    }

    pub fn activate_editor(&self, origin: EditOrigin, assist: FormulaAssist) {
        self.begin_edit(origin, None);
        self.edit_field.set_value(assist);
    }

    /// Keep the draft and its target when its cell overlay is about to unmount.
    pub fn carry_edit_to_bar(&self) {
        if self.editing.get_untracked().is_some() {
            self.editing.set(Some(EditOrigin::Bar));
            self.edit_field.set_value(self.assist);
            #[cfg(target_arch = "wasm32")]
            {
                let controller = *self;
                leptos::prelude::set_timeout(
                    move || {
                        if controller.editing.try_get_untracked().flatten() != Some(EditOrigin::Bar)
                        {
                            return;
                        }
                        let assist = controller.assist;
                        assist.place(
                            controller.edit_text.get_untracked(),
                            assist.caret.get_untracked(),
                        );
                    },
                    std::time::Duration::ZERO,
                );
            }
        }
    }

    pub fn focus_editor(&self) {
        let assist = self.edit_field.get_value();
        if let Some(field) = assist.field.get_untracked() {
            let _ = field.focus();
        }
    }

    /// Both editors use this route after their shared completion and local undo handlers.
    pub fn editor_key(&self, ev: leptos::ev::KeyboardEvent, assist: FormulaAssist) {
        if assist.composing.get_untracked() || ev.is_composing() || ev.key_code() == 229 {
            return;
        }
        let ctrl = ev.ctrl_key() || ev.meta_key();
        let movement = match ev.key().as_str() {
            "Enter" if !ctrl && !ev.alt_key() => Some(if ev.shift_key() {
                NavAction::Up
            } else {
                NavAction::CommitDown
            }),
            "Tab" if !ctrl && !ev.alt_key() => Some(if ev.shift_key() {
                NavAction::PrevColumn
            } else {
                NavAction::CommitRight
            }),
            _ => None,
        };
        if let Some(movement) = movement {
            ev.prevent_default();
            ev.stop_propagation();
            self.finish_edit(movement);
            return;
        }
        match ev.key().as_str() {
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                self.cancel_edit();
                crate::grid::focus_grid();
            }
            "F2" if !ev.alt_key() => {
                ev.prevent_default();
                ev.stop_propagation();
                if ctrl {
                    match self.editing.get_untracked() {
                        Some(EditOrigin::Cell) => self.carry_edit_to_bar(),
                        _ => {
                            if document()
                                .query_selector("[data-dnacalc=\"grid\"]")
                                .ok()
                                .flatten()
                                .is_some()
                            {
                                self.editing.set(Some(EditOrigin::Cell));
                            }
                        }
                    }
                } else {
                    self.point_mode.update(|point| *point = !*point);
                    self.pointed.set(None);
                }
            }
            "F4" if !ctrl && !ev.alt_key() => {
                ev.prevent_default();
                ev.stop_propagation();
                let (start, end) = field_selection(assist);
                if let Some((text, start, end)) =
                    cycle_anchors(&self.edit_text.get_untracked(), start, end)
                {
                    self.pointed.set(None);
                    assist.place(text, end);
                    if let Some(field) = assist.field.get_untracked() {
                        let _ = field.set_selection_range(start as u32, end as u32);
                    }
                }
            }
            "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
                if self.point_mode.get_untracked() && !ctrl && !ev.alt_key() =>
            {
                let action = match ev.key().as_str() {
                    "ArrowUp" => NavAction::Up,
                    "ArrowDown" => NavAction::Down,
                    "ArrowLeft" => NavAction::Left,
                    _ => NavAction::Right,
                };
                let from = self
                    .pointed
                    .get_untracked()
                    .map(|p| p.corner)
                    .or_else(|| {
                        self.edit_target
                            .get_untracked()
                            .map(|at| (at.row(), at.column()))
                    })
                    .unwrap_or((1, 1));
                let next = next_active(from, action, GridExtent::new(1_048_576, 16_384));
                if self.point_at(next.0, next.1, ev.shift_key()) {
                    ev.prevent_default();
                    ev.stop_propagation();
                }
            }
            _ => {}
        }
    }

    pub fn edit_changed(&self) {
        self.pointed.set(None);
    }

    pub fn edit_label(&self) -> String {
        self.edit_target
            .get()
            .map(|at| self.cell_label(at))
            .unwrap_or_default()
    }

    /// Insert/replace a pointed reference without changing the edit's target or selection.
    pub fn point_at(&self, row: u32, column: u32, extend: bool) -> bool {
        if self.editing.get_untracked().is_none()
            || self.assist.composing.get_untracked()
            || self.assist.readonly.get_untracked()
        {
            return false;
        }
        let assist = self.edit_field.get_value();
        let text = self.edit_text.get_untracked();
        let (start, end) = field_selection(assist);
        let Some(sheet) = self.sheet.get_untracked() else {
            return false;
        };
        let previous = self
            .pointed
            .get_untracked()
            .filter(|p| self.pointed_text.get_value() == text && start == end && end == p.end);
        let slot = previous.map(|p| (p.start, p.end)).or_else(|| {
            if expects_reference(&text, start, end) {
                Some((start, end))
            } else if self.point_mode.get_untracked() {
                point_reference_span(&text, start, end).map(|s| (s.start, s.end()))
            } else {
                None
            }
        });
        let Some((start, end)) = slot else {
            return false;
        };
        let anchor = if extend {
            previous
                .filter(|p| p.sheet == sheet)
                .map_or((row, column), |p| p.anchor)
        } else {
            (row, column)
        };
        let near = (anchor.0.min(row), anchor.1.min(column));
        let far = (anchor.0.max(row), anchor.1.max(column));
        let first = if self
            .edit_target
            .get_untracked()
            .is_some_and(|at| at.sheet() != sheet)
        {
            CellKey::new(sheet, near.0, near.1)
                .ok()
                .map(|at| self.cell_label(at))
                .unwrap_or_default()
        } else {
            crate::cells::address(near.0, near.1)
        };
        let reference = if near == far {
            first
        } else {
            format!("{}:{}", first, crate::cells::address(far.0, far.1))
        };
        let (text, end) = replace_text(&text, start, end, &reference);
        self.pointed_text.set_value(text.clone());
        assist.reset();
        assist.place(text, end);
        self.point_mode.set(true);
        self.pointed.set(Some(PointPick {
            sheet,
            start,
            end,
            anchor,
            corner: (row, column),
        }));
        true
    }

    pub fn finish_edit(&self, movement: NavAction) {
        let controller = *self;
        self.commit_edit(move |accepted| {
            if accepted {
                let next = next_active(
                    controller.active.get_untracked(),
                    movement,
                    GridExtent::new(1_048_576, 16_384),
                );
                controller.select(next.0, next.1);
                crate::grid::focus_grid();
            } else {
                controller.focus_editor();
            }
        });
    }

    /// Ends an edit without entering it.
    pub fn cancel_edit(&self) {
        if self.assist.readonly.get_untracked() {
            return;
        }
        if self.editing.get_untracked().is_some() {
            let target = self.edit_target.get_untracked();
            self.editing.set(None);
            self.edit_target.set(None);
            self.pointed.set(None);
            self.assist.reset();
            if let Some(at) = target {
                self.sheet.set(Some(at.sheet()));
                self.select(at.row(), at.column());
            }
            self.say("Edit cancelled", false);
        }
    }

    /// Enters the edited text into the active cell. `then` hears whether the entry was
    /// accepted; a refused entry keeps the edit open with the reason on the status line.
    pub fn commit_edit(&self, then: impl FnOnce(bool) + 'static) {
        if self.assist.readonly.get_untracked() {
            return;
        }
        let Some(at) = self.edit_target.get_untracked() else {
            then(false);
            return;
        };
        let text = self.edit_text.get_untracked();
        if self.active_key_untracked() != Some(at) {
            self.sheet.set(Some(at.sheet()));
            self.select(at.row(), at.column());
        }
        if text == self.edit_original.get_value() {
            self.editing.set(None);
            self.edit_target.set(None);
            self.pointed.set(None);
            self.assist.reset();
            then(true);
            return;
        }
        self.assist.readonly.set(true);
        let controller = *self;
        self.dispatch_result(
            Intent::EnterCell {
                at: Key::of(at),
                text,
            },
            move |result| {
                controller.assist.readonly.set(false);
                let accepted = result.is_ok_and(|outcome| outcome.accepted);
                if accepted {
                    controller.editing.set(None);
                    controller.edit_target.set(None);
                    controller.pointed.set(None);
                    controller.assist.reset();
                }
                then(accepted);
            },
        );
    }

    /// Text entered into any cell, one undoable edit (the notebook's blocks, a drop-down's item);
    /// `then` gets the outcome.
    pub fn enter(&self, at: CellKey, text: String, then: impl FnOnce(&Outcome) + 'static) {
        self.dispatch(
            Intent::EnterCell {
                at: Key::of(at),
                text,
            },
            then,
        );
    }

    /// Inserts (`insert`) or deletes rows or columns: the selected whole rows (a row header's
    /// selection) or whole columns, through calc-host's axis edit (one undo step; formulas'
    /// references, number formats and sizes move as Excel moves them). Any other selection says
    /// what to select (dc-09v).
    pub fn edit_axis(&self, insert: bool) {
        let Some(area) = self.selected_area_untracked() else {
            return;
        };
        let (first, last) = (area.first(), area.last());
        let whole_rows = first.column() == 1 && last.column() == calc_host::value::MAX_COLUMNS;
        let whole_columns = first.row() == 1 && last.row() == calc_host::value::MAX_ROWS;
        let (rows, start, count) = if whole_rows && !whole_columns {
            (true, first.row(), area.row_count())
        } else if whole_columns && !whole_rows {
            (false, first.column(), area.column_count())
        } else {
            self.say(
                "Select whole rows or columns (click a row or column header) to insert or delete them",
                true,
            );
            return;
        };
        self.dispatch(
            Intent::EditAxis {
                sheet: Sheet::of(area.sheet()),
                rows,
                insert,
                start,
                count,
            },
            |_| {},
        );
    }

    /// Clears the selected range (Delete).
    pub fn clear_active(&self) {
        if let Some(area) = self.selected_area_untracked() {
            self.dispatch(
                Intent::Clear {
                    area: Area::of(area),
                },
                |_| {},
            );
        }
    }

    pub fn undo(&self) {
        if self.editing.get_untracked().is_some() {
            self.pointed.set(None);
            self.edit_field.get_value().undo(false);
            return;
        }
        self.dispatch(Intent::Undo, |_| {});
    }

    pub fn redo(&self) {
        if self.editing.get_untracked().is_some() {
            self.pointed.set(None);
            self.edit_field.get_value().undo(true);
            return;
        }
        self.dispatch(Intent::Redo, |_| {});
    }

    pub fn recalculate(&self) {
        self.dispatch(Intent::Recalculate, |_| {});
    }

    /// Whether the workbook calculates manually.
    pub fn manual(&self) -> bool {
        self.summary.with(|summary| summary.manual)
    }

    /// The workbook's iterative-calculation settings: on, the most iterations, the maximum change
    /// (dc-y2o.2).
    pub fn iteration(&self) -> (bool, u32, f64) {
        self.summary.with(|summary| {
            (
                summary.iteration,
                summary.max_iterations,
                summary.max_change,
            )
        })
    }

    /// Sets the workbook's iterative-calculation settings, one undo step (dc-y2o.2).
    pub fn set_iteration(&self, enabled: bool, max_iterations: u32, max_change: f64) {
        self.dispatch(
            Intent::SetIteration {
                enabled,
                max_iterations,
                max_change_bits: max_change.to_bits(),
            },
            |_| {},
        );
    }

    /// The warning of a circular reference the last recalculation found (dc-y2o.1): `None` when
    /// there is none.
    pub fn circular_warning(&self) -> Option<String> {
        let (first, count) = self
            .summary
            .with(|summary| (summary.circular, summary.circular_count));
        let at = first.and_then(|key| key.cell())?;
        let label = self.cell_label(at);
        Some(if count > 1 {
            format!("Circular references: {label} and {} more", count - 1)
        } else {
            format!("Circular references: {label}")
        })
    }

    pub fn set_calc_mode(&self, manual: bool) {
        let controller = *self;
        self.dispatch(Intent::SetCalcMode { manual }, move |_| {
            // Manual mode recalculates nothing, but the mode itself changed.
            controller.revision.update(|n| *n += 1);
        });
    }

    /// Registers a subscription whose cells go to `mirror`; its number for later changes.
    pub fn subscribe(&self, interest: wire::Interest, mirror: RwSignal<ViewMirror>) -> u64 {
        let id = self.next_subscription.get_value();
        self.next_subscription.set_value(id + 1);
        mirror.set(ViewMirror::default());
        self.mirrors.update_value(|mirrors| {
            mirrors.insert(id, mirror);
        });
        self.ask(Request::Subscribe { id, interest }, |_| {});
        id
    }

    /// Moves a subscription's interest; its mirror is given the new interest's cells.
    pub fn set_interest(&self, id: u64, interest: wire::Interest) {
        self.ask(Request::SetInterest { id, interest }, |_| {});
    }

    pub fn unsubscribe(&self, id: u64) {
        self.mirrors.update_value(|mirrors| {
            mirrors.remove(&id);
        });
        self.ask(Request::Unsubscribe { id }, |_| {});
    }

    /// Saves the workbook as a package's bytes; `saved` gets its name and bytes, and the status
    /// line says how it went.
    pub fn save(&self, saved: impl FnOnce(String, Vec<u8>) + 'static) {
        if !self.notebook_drafts.with_untracked(BTreeMap::is_empty) {
            self.say("Finish the Notebook draft with Enter, or cancel it with Escape, before downloading.", true);
            self.focus_pending_draft();
            return;
        }
        let controller = *self;
        if self.editing.get_untracked().is_some() {
            self.commit_edit(move |accepted| {
                if accepted {
                    controller.save_package(saved);
                } else {
                    controller.focus_editor();
                }
            });
        } else {
            self.save_package(saved);
        }
    }

    fn save_package(&self, saved: impl FnOnce(String, Vec<u8>) + 'static) {
        let controller = *self;
        port::call(Request::Save, None, move |reply, bytes| {
            controller.revision.update(|n| *n += 1);
            match (reply, bytes) {
                (
                    Ok(Reply::Done {
                        outcome,
                        saved: Some(name),
                        ..
                    }),
                    Some(bytes),
                ) => {
                    if controller.summary.get_untracked().changed {
                        controller.download_unconfirmed.set(true);
                    }
                    controller.summary.set(outcome.summary);
                    controller.say(
                        format!("Download package prepared: {name}, {} bytes", bytes.len()),
                        false,
                    );
                    saved(name, bytes);
                }
                (Ok(Reply::Done { outcome, .. }), _) => controller.say(
                    format!(
                        "The workbook was not saved: {}",
                        outcome.error.unwrap_or_default()
                    ),
                    true,
                ),
                (Ok(_), _) => controller.say("The workbook was not saved", true),
                (Err(message), _) => {
                    controller.say(format!("The workbook was not saved: {message}"), true)
                }
            }
        });
    }
}

impl Default for Controller {
    fn default() -> Self {
        Controller::new()
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The corner of `area` opposite `at`, one of its corners.
fn opposite(area: GridArea, at: CellKey) -> (u32, u32) {
    let (first, last) = (area.first(), area.last());
    let row = if at.row() == first.row() {
        last.row()
    } else {
        first.row()
    };
    let column = if at.column() == first.column() {
        last.column()
    } else {
        first.column()
    };
    (row, column)
}

/// A fill's source and target in `area`: its first row and the rows below (`down`), or its first
/// column and the columns right of it; `None` when the area has one row (one column).
fn fill_parts(area: GridArea, down: bool) -> Option<(GridArea, GridArea)> {
    let (first, last) = (area.first(), area.last());
    let sheet = area.sheet();
    if down {
        if first.row() == last.row() {
            return None;
        }
        let source =
            GridArea::new(first, CellKey::new(sheet, first.row(), last.column()).ok()?).ok()?;
        let target = GridArea::new(
            CellKey::new(sheet, first.row() + 1, first.column()).ok()?,
            last,
        )
        .ok()?;
        Some((source, target))
    } else {
        if first.column() == last.column() {
            return None;
        }
        let source =
            GridArea::new(first, CellKey::new(sheet, last.row(), first.column()).ok()?).ok()?;
        let target = GridArea::new(
            CellKey::new(sheet, first.row(), first.column() + 1).ok()?,
            last,
        )
        .ok()?;
        Some((source, target))
    }
}

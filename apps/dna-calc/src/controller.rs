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

use crate::port;
use crate::wire::{self, Area, Cells, Event, Intent, Key, Outcome, Reply, Request, Sheet, Summary};
use crate::workbook::SheetTab;
use calc_host::value::{CellKey, GridArea, SheetId};
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
    pub timings: RwSignal<Timings>,
}

impl Controller {
    /// A controller over a new workbook.
    pub fn new() -> Controller {
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
            edit_text: RwSignal::new(String::new()),
            timings: RwSignal::new(Timings::default()),
        };
        port::start();
        port::set_events(move |event| controller.receive(event));
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

    /// A new, empty workbook.
    pub fn new_workbook(&self) {
        let controller = *self;
        port::call(Request::New, None, move |reply, _| {
            if let Ok(Reply::Done { outcome, .. }) = reply {
                controller.installed(outcome.summary, "New workbook".to_owned());
            }
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
        let controller = *self;
        port::call(Request::Intent(intent), None, move |reply, _| match reply {
            Ok(Reply::Done { outcome, .. }) => {
                controller.applied(&outcome);
                then(&outcome);
            }
            Ok(_) => controller.say("The workbook answered an intent with something else", true),
            Err(message) => controller.say(format!("The workbook did not answer: {message}"), true),
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
                self.editing.set(None);
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
            .with_untracked(|summary| dna_onecalc::display::cell(summary, at))
    }

    /// An area's label with its sheet's name: `Sheet1!A1:B3`.
    pub fn area_label(&self, area: GridArea) -> String {
        self.summary
            .with_untracked(|summary| dna_onecalc::display::area(summary, area))
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
        self.cancel_edit();
        self.sheet.set(Some(sheet));
        self.select(1, 1);
    }

    /// Makes `at` the active cell, showing its sheet (a skin's jump to a cell: the tree's, the
    /// canvas's, the ledger's).
    pub fn go_to(&self, at: CellKey) {
        if Some(at.sheet()) != self.sheet.get_untracked() {
            self.cancel_edit();
            self.sheet.set(Some(at.sheet()));
        }
        self.select(at.row(), at.column());
    }

    /// Starts an edit of the active cell: with `seed`, or the cell's input when `None`.
    pub fn begin_edit(&self, origin: EditOrigin, seed: Option<String>) {
        let text = match seed {
            Some(seed) => seed,
            None => self
                .active_key_untracked()
                .and_then(|at| {
                    self.selection
                        .with_untracked(|mirror| mirror.cell(at).map(|cell| cell.input.clone()))
                })
                .unwrap_or_default(),
        };
        self.edit_text.set(text);
        self.editing.set(Some(origin));
    }

    /// Ends an edit without entering it.
    pub fn cancel_edit(&self) {
        if self.editing.get_untracked().is_some() {
            self.editing.set(None);
            self.say("Edit cancelled", false);
        }
    }

    /// Enters the edited text into the active cell. `then` hears whether the entry was
    /// accepted; a refused entry keeps the edit open with the reason on the status line.
    pub fn commit_edit(&self, then: impl FnOnce(bool) + 'static) {
        let Some(at) = self.active_key_untracked() else {
            then(false);
            return;
        };
        let text = self.edit_text.get_untracked();
        let controller = *self;
        self.dispatch(
            Intent::EnterCell {
                at: Key::of(at),
                text,
            },
            move |outcome| {
                if outcome.accepted {
                    controller.editing.set(None);
                }
                then(outcome.accepted);
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
        self.cancel_edit();
        self.dispatch(Intent::Undo, |_| {});
    }

    pub fn redo(&self) {
        self.cancel_edit();
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
        self.summary
            .with(|summary| (summary.iteration, summary.max_iterations, summary.max_change))
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
                    controller.summary.set(outcome.summary);
                    controller.say(format!("Saved {name}: {} bytes", bytes.len()), false);
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

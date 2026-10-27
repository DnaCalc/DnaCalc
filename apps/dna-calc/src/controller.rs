//! The application's controller: the open [`Workbook`] behind reactive signals, shared by the
//! shell and every skin. It is `Copy`, so any view can hold it.
//!
//! - Reads go through [`Controller::with`], which tracks [`Controller::revision`], so a view that
//!   reads the workbook is shown again after every change.
//! - Changes go through [`Controller::dispatch`] (a [`SkinIntent`]): after the session returns,
//!   the deltas its subscriptions received are applied to the mirrors registered for them
//!   ([`Controller::subscribe`]), the revision advances, and a refused intent's reason goes to the
//!   status line.
//! - The application's own subscription follows the host's selection ([`Interest::Selection`]):
//!   the formula bar and the status line show the active cell from it.
//!
//! The active sheet and cell are the controller's, so they stay when the skin changes; every move
//! of the active cell is also a host selection ([`SkinIntent::Select`]), so an undo restores the
//! selection the edit was made in.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-app/src/app.rs` (`CalcApp`'s workspace,
//! delta and selection signals, the dispatcher that marks the document changed) and
//! `src/dnacalc-skin-leptos/src/state_handles.rs` (shared state behind one handle), onto calc-host's
//! session.

use crate::cells;
use crate::workbook::{OpenRefused, SheetTab, Workbook};
use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{
    CalcMode, CellEdit, DeltaMirror, IntentReceipt, Interest, ProjectedCell, Selection, SkinIntent,
    SubscriptionId,
};
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
    book: StoredValue<Workbook, LocalStorage>,
    mirrors: StoredValue<BTreeMap<SubscriptionId, RwSignal<DeltaMirror>>, LocalStorage>,
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
    /// The cells of the host's selection, as the application's subscription delivered them.
    pub selection: RwSignal<DeltaMirror>,
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
            book: StoredValue::new_local(Workbook::new_blank()),
            mirrors: StoredValue::new_local(BTreeMap::new()),
            revision: RwSignal::new(0),
            generation: RwSignal::new(0),
            sheet: RwSignal::new(None),
            active: RwSignal::new((1, 1)),
            corner: RwSignal::new((1, 1)),
            clipboard: RwSignal::new(None),
            cut: RwSignal::new(false),
            selection: RwSignal::new(DeltaMirror::default()),
            status: RwSignal::new(Status::default()),
            editing: RwSignal::new(None),
            edit_text: RwSignal::new(String::new()),
            timings: RwSignal::new(Timings::default()),
        };
        controller.installed("New workbook".to_owned());
        controller
    }

    /// Reads the workbook, tracking the revision.
    pub fn with<R>(&self, f: impl FnOnce(&Workbook) -> R) -> R {
        self.revision.track();
        self.book.with_value(f)
    }

    /// Reads the workbook without tracking.
    pub fn with_untracked<R>(&self, f: impl FnOnce(&Workbook) -> R) -> R {
        self.book.with_value(f)
    }

    /// Changes the workbook outside an intent (a census through short subscriptions).
    pub fn update_untracked<R>(&self, f: impl FnOnce(&mut Workbook) -> R) -> R {
        self.book
            .try_update_value(f)
            .expect("the workbook lives as long as the page")
    }

    pub fn sheets(&self) -> Vec<SheetTab> {
        self.with(Workbook::sheets)
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
    pub fn active_cell(&self) -> Option<ProjectedCell> {
        let at = self.active_key()?;
        self.selection.with(|mirror| mirror.cell(at).cloned())
    }

    fn say(&self, text: impl Into<String>, refused: bool) {
        self.status.set(Status {
            text: text.into(),
            refused,
        });
    }

    /// Replaces the workbook with `book`: a new generation, the first sheet active at A1, and the
    /// application's selection subscription.
    fn install(&self, book: Workbook, message: String) {
        self.book.set_value(book);
        self.installed(message);
    }

    fn installed(&self, message: String) {
        self.mirrors.update_value(BTreeMap::clear);
        let first = self
            .book
            .with_value(|book| book.sheets().first().map(|tab| tab.id));
        self.sheet.set(first);
        self.active.set((1, 1));
        self.corner.set((1, 1));
        self.clipboard.set(None);
        self.editing.set(None);
        let selection = self.selection;
        selection.set(DeltaMirror::default());
        if let Some(id) = self
            .book
            .try_update_value(|book| book.subscribe(Interest::Selection).ok())
            .flatten()
        {
            self.mirrors.update_value(|mirrors| {
                mirrors.insert(id, selection);
            });
        }
        if let Some(at) = self.active_key_untracked() {
            self.book.update_value(|book| {
                book.dispatch(SkinIntent::Select(Selection::cell(at)));
            });
        }
        self.route_deltas();
        self.generation.update(|n| *n += 1);
        self.revision.update(|n| *n += 1);
        self.say(message, false);
    }

    /// A new, empty workbook.
    pub fn new_workbook(&self) {
        self.install(Workbook::new_blank(), "New workbook".to_owned());
    }

    /// Opens a package's bytes under `name`; the current workbook stays when it is refused.
    pub fn open(&self, name: &str, bytes: Vec<u8>) -> Result<(), OpenRefused> {
        let started = now_ms();
        let size = bytes.len();
        crate::workbook::reserve_for_open(size);
        match Workbook::open(name, bytes) {
            Ok(book) => {
                let elapsed = now_ms() - started;
                self.timings
                    .update(|timings| timings.open_ms = Some(elapsed));
                let message = match book.calculated_on_open() {
                    Some(calculated) => format!(
                        "Opened {name}: {size} bytes in {elapsed:.0} ms: read in {:.0} ms, calculated in {calculated:.0} ms",
                        elapsed - calculated
                    ),
                    None => format!(
                        "Opened {name}: {size} bytes in {elapsed:.0} ms; the file calculates manually, so its formulas show the values it stored until Calculate"
                    ),
                };
                self.install(book, message);
                Ok(())
            }
            Err(refused) => {
                self.say(format!("{name} did not open: {}", refused.0), true);
                Err(refused)
            }
        }
    }

    /// Applies the waiting deltas to their mirrors.
    fn route_deltas(&self) {
        let deltas = self
            .book
            .try_update_value(Workbook::take_deltas)
            .unwrap_or_default();
        if deltas.is_empty() {
            return;
        }
        let mirrors = self.mirrors.get_value();
        for delta in deltas {
            if let Some(mirror) = mirrors.get(&delta.subscription) {
                mirror.update(|mirror| {
                    if mirror.apply(&delta).is_err() {
                        // A gap means a delta was lost: the next reset brings the mirror back.
                        *mirror = DeltaMirror::default();
                    }
                });
            }
        }
    }

    /// Carries out an intent, applies its deltas, and reports a refusal on the status line.
    pub fn dispatch(&self, intent: SkinIntent) -> IntentReceipt {
        let started = now_ms();
        let receipt = self
            .book
            .try_update_value(|book| book.dispatch(intent))
            .expect("the workbook lives as long as the page");
        let elapsed = now_ms() - started;
        self.timings
            .update(|timings| timings.last_intent_ms = Some(elapsed));
        self.route_deltas();
        // The active cell follows the host's selection: an undo or a redo restores the selection
        // of its edit, which may be on another sheet, and then that sheet is shown (as Excel
        // does), so the formula bar and the status line read the cell the selection holds.
        let restored = self.book.with_value(|book| {
            let selection = book.session().selection();
            selection
                .active
                .map(|at| (at, selection.areas.first().copied()))
        });
        if let Some((at, area)) = restored {
            if Some(at.sheet()) != self.sheet.get_untracked() {
                self.editing.set(None);
                self.sheet.set(Some(at.sheet()));
            }
            if (at.row(), at.column()) != self.active.get_untracked() {
                self.active.set((at.row(), at.column()));
            }
            let corner = area.map_or((at.row(), at.column()), |area| opposite(area, at));
            if corner != self.corner.get_untracked() {
                self.corner.set(corner);
            }
        }
        // Only a change of the workbook or its calculation advances the revision: a selection or
        // an interest moved changes no cell, so what reads the workbook is not read again.
        if receipt.committed.is_some() || receipt.recalc.is_some() {
            self.revision.update(|n| *n += 1);
        }
        match &receipt.error {
            Some(error) => self.say(error.to_string(), true),
            None => {
                if let Some(progress) = &receipt.recalc {
                    self.say(
                        format!(
                            "{} in {elapsed:.0} ms",
                            capitalised(&Workbook::recalc_text(progress))
                        ),
                        false,
                    );
                }
            }
        }
        receipt
    }

    /// Moves the active cell and the host's selection to one cell.
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
            self.dispatch(SkinIntent::Select(Selection {
                active: Some(at),
                areas: vec![area],
            }));
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
            let label = self.with_untracked(|book| cells::area_label(book.host(), area));
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
        let receipt = self.dispatch(SkinIntent::Copy { from, to, cut });
        if cut && receipt.accepted() {
            self.clipboard.set(None);
            self.cut.set(false);
        }
    }

    /// Fills the selected range down from its first row (Ctrl+D) or right from its first column
    /// (Ctrl+R): a calc-host copy of that row or column over the rest.
    pub fn fill(&self, down: bool) {
        let Some(area) = self.selected_area_untracked() else {
            return;
        };
        let parts = fill_parts(area, down);
        match parts {
            Some((from, to)) => {
                self.dispatch(SkinIntent::Copy {
                    from,
                    to,
                    cut: false,
                });
            }
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
                    self.selection.with_untracked(|mirror| {
                        mirror.cell(at).map(|cell| cells::input_text(cell.input()))
                    })
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

    /// Enters the edited text into the active cell. `true` when the entry was accepted; a refused
    /// entry keeps the edit open with the reason on the status line.
    pub fn commit_edit(&self) -> bool {
        let Some(at) = self.active_key_untracked() else {
            return false;
        };
        let text = self.edit_text.get_untracked();
        let note = self.book.with_value(|book| book.validation_note(at, &text));
        let receipt = self.dispatch(SkinIntent::EnterCell { at, text });
        if receipt.accepted() {
            self.editing.set(None);
            self.validation_said(note);
            true
        } else {
            false
        }
    }

    /// Text entered into any cell, one undoable edit (the notebook's blocks, a drop-down's item).
    pub fn enter(&self, at: CellKey, text: String) -> IntentReceipt {
        let note = self.book.with_value(|book| book.validation_note(at, &text));
        let receipt = self.dispatch(SkinIntent::EnterCell { at, text });
        if receipt.accepted() {
            self.validation_said(note);
        }
        receipt
    }

    /// A warning or information rule's message about an entry the session took, on the status
    /// line (Excel asks, or tells, in a dialog; the entry is kept here).
    fn validation_said(&self, note: Option<calc_host::ValidationFailure>) {
        if let Some(failure) = note {
            let kind = match failure.style {
                calc_host::ErrorStyle::Warning => "Warning",
                _ => "Information",
            };
            self.say(format!("{kind}, entry kept: {failure}"), true);
        }
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
        let (axis, start, count) = if whole_rows && !whole_columns {
            (calc_host::Axis::Rows, first.row(), area.row_count())
        } else if whole_columns && !whole_rows {
            (
                calc_host::Axis::Columns,
                first.column(),
                area.column_count(),
            )
        } else {
            self.say(
                "Select whole rows or columns (click a row or column header) to insert or delete them",
                true,
            );
            return;
        };
        let change = if insert {
            calc_host::AxisChange::Insert
        } else {
            calc_host::AxisChange::Delete
        };
        match calc_host::AxisEdit::new(area.sheet(), axis, change, start, count) {
            Ok(edit) => {
                self.dispatch(SkinIntent::EditAxis(edit));
            }
            Err(error) => self.say(format!("{error:?}"), true),
        }
    }

    /// Clears the selected range (Delete).
    pub fn clear_active(&self) {
        if let Some(area) = self.selected_area_untracked() {
            self.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::Clear { area }],
            });
        }
    }

    pub fn undo(&self) {
        self.cancel_edit();
        self.dispatch(SkinIntent::Undo);
    }

    pub fn redo(&self) {
        self.cancel_edit();
        self.dispatch(SkinIntent::Redo);
    }

    pub fn recalculate(&self) {
        self.dispatch(SkinIntent::Recalculate);
    }

    pub fn calc_mode(&self) -> CalcMode {
        self.with(|book| book.session().calc_mode())
    }

    pub fn set_calc_mode(&self, mode: CalcMode) {
        self.dispatch(SkinIntent::SetCalcMode(mode));
        // Manual mode recalculates nothing, but the mode itself changed.
        self.revision.update(|n| *n += 1);
    }

    /// Registers a subscription whose deltas go to `mirror`.
    pub fn subscribe(
        &self,
        interest: Interest,
        mirror: RwSignal<DeltaMirror>,
    ) -> Option<SubscriptionId> {
        let id = self
            .book
            .try_update_value(|book| book.subscribe(interest).ok())
            .flatten()?;
        self.mirrors.update_value(|mirrors| {
            mirrors.insert(id, mirror);
        });
        mirror.set(DeltaMirror::default());
        self.route_deltas();
        Some(id)
    }

    /// Moves a subscription's interest; its mirror is given the new interest's cells.
    pub fn set_interest(&self, subscription: SubscriptionId, interest: Interest) {
        self.book.update_value(|book| {
            book.dispatch(SkinIntent::SetInterest {
                subscription,
                interest,
            });
        });
        self.route_deltas();
    }

    pub fn unsubscribe(&self, subscription: SubscriptionId) {
        self.mirrors.update_value(|mirrors| {
            mirrors.remove(&subscription);
        });
        self.book.update_value(|book| {
            book.unsubscribe(subscription);
        });
    }

    /// Saves the workbook as a package's bytes, with its name; the status line says how it went.
    pub fn save(&self) -> Option<(String, Vec<u8>)> {
        let outcome = self.book.try_update_value(|book| {
            let name = book.name().to_owned();
            book.save().map(|(bytes, _)| (name, bytes))
        })?;
        self.revision.update(|n| *n += 1);
        match outcome {
            Ok((name, bytes)) => {
                self.say(format!("Saved {name}: {} bytes", bytes.len()), false);
                Some((name, bytes))
            }
            Err(failure) => {
                self.say(format!("The workbook was not saved: {failure}"), true);
                None
            }
        }
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

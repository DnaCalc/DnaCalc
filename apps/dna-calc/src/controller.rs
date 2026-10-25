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

/// Milliseconds on the browser's clock; 0 off the browser.
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
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
        match Workbook::open(name, bytes) {
            Ok(book) => {
                let elapsed = now_ms() - started;
                self.timings
                    .update(|timings| timings.open_ms = Some(elapsed));
                let message = if book.calculated_on_open() {
                    format!("Opened {name}: {size} bytes, opened and calculated in {elapsed:.0} ms")
                } else {
                    format!(
                        "Opened {name}: {size} bytes, opened in {elapsed:.0} ms; the file calculates manually, so its formulas show the values it stored until Calculate"
                    )
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
        let restored = self
            .book
            .with_value(|book| book.session().selection().active);
        if let Some(at) = restored {
            if Some(at.sheet()) != self.sheet.get_untracked() {
                self.editing.set(None);
                self.sheet.set(Some(at.sheet()));
            }
            if (at.row(), at.column()) != self.active.get_untracked() {
                self.active.set((at.row(), at.column()));
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

    /// Moves the active cell and the host's selection.
    pub fn select(&self, row: u32, column: u32) {
        self.active.set((row, column));
        if let Some(at) = self.active_key_untracked() {
            self.dispatch(SkinIntent::Select(Selection::cell(at)));
        }
    }

    /// Shows another sheet, at its A1.
    pub fn show_sheet(&self, sheet: SheetId) {
        self.cancel_edit();
        self.sheet.set(Some(sheet));
        self.select(1, 1);
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
        let receipt = self.dispatch(SkinIntent::EnterCell { at, text });
        if receipt.accepted() {
            self.editing.set(None);
            true
        } else {
            false
        }
    }

    /// Text entered into any cell, one undoable edit (the notebook's blocks).
    pub fn enter(&self, at: CellKey, text: String) -> IntentReceipt {
        self.dispatch(SkinIntent::EnterCell { at, text })
    }

    /// Clears the active cell.
    pub fn clear_active(&self) {
        if let Some(at) = self.active_key_untracked() {
            self.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::Clear {
                    area: GridArea::cell(at),
                }],
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

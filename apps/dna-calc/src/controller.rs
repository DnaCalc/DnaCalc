//! The shared workbook controller. It owns the command queue, session signals and subscriptions.
//! The shell and every skin can hold a copy of its handle.
//!
//! The workbook lives in the separate worker composition, reached through
//! [`crate::port`] by messages ([`crate::wire`], dc-p96): the controller sends a request and
//! applies the reply when it comes, so nothing here waits for the server.
//!
//! - Changes go through [`Controller::dispatch`] (a [`WorkbookCommand`]): when the outcome comes,
//!   the summary (sheets, name, calculation mode) is updated, the revision advances on a commit
//!   or a recalculation, the active cell follows the session's selection, and a refused intent's
//!   reason goes to the status line. The server pushes the cells of the page's subscriptions as
//!   they change; [`Controller::subscribe`] registers a [`ViewMirror`] for one.
//! - The formula bar and status line use the grid's committed root wherever its coverage includes
//!   the active cell. The application's selection subscription follows the session's selection
//!   for other cells and skins.
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

use dna_calc_model::Target;
use crate::workbook_command::{WorkbookCommand, area_target, cell_target, target_cell};
use crate::model_adapter::{Field, ModelField};
use crate::port;
use crate::wire::{self, Area, Event, Key, Outcome, Reply, Request, Sheet, Summary};
use dna_calc_model::Snapshot;
use dna_calc_model::projection::SheetTab;
use dna_calc_model::selection::{GridExtent, NavAction, Navigation, next_active, opposite};
use dna_calc_model::{
    Command, CommandContext, CommandId, Effect as CommandEffect, FindMode, Model,
    RevealTarget, SkinId, ViewState,
};
pub use dna_calc_model::{MacroPolicy, MacroSecurity, PointPick, SurfaceId as EditOrigin};

/// The button the macro bar offers (dc-15dk).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroButton {
    /// "Enable macros": a held project, an origin that does not block it.
    Enable,
    /// "Unblock and enable": a Mark of the Web or network file, behind the bar's warning.
    Unblock,
}

/// The macro bar's words and button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroBar {
    pub text: String,
    pub button: Option<MacroButton>,
}

impl MacroBar {
    fn note(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            button: None,
        }
    }
}
pub use dna_calc_model::{Status, Timings, ViewMirror};
pub type MirrorHandle = ModelField<Snapshot<ViewMirror>>;
use calc_value::{CellKey, GridArea, SheetId};
use dna_formula_ui::formula_field::FormulaAssist;
use leptos::prelude::*;
use std::collections::{BTreeMap, VecDeque};

pub use dna_formula_ui::web::{memory_bytes, now_ms};

mod clipboard;
mod editing;
#[cfg(target_arch = "wasm32")]
use editing::field_selection;
mod files;
pub(crate) mod grid;

#[path = "input_adapter.rs"]
mod input_adapter;

type CommandAnswer = Box<dyn FnOnce(Result<Reply, String>, Option<Vec<u8>>)>;
type SaveDownload = Box<dyn FnOnce(String, Vec<u8>)>;
type SheetWindows = fn(Controller, SheetId) -> Vec<wire::SheetWindow>;

type ValidationCallback = Box<dyn FnOnce(Result<Outcome, String>)>;

#[derive(Clone, Copy)]
struct ReviewFields {
    edit: ModelField<Option<dna_calc_model::ReviewDraft>>,
    circles: ModelField<std::collections::BTreeSet<SheetId>>,
    scan: ModelField<Option<Result<wire::ValidationScan, String>>>,
    epoch: ModelField<u64>,
}

#[derive(Clone, Copy)]
pub struct Controller {
    model: RwSignal<Model>,
    sheet_windows: StoredValue<Option<SheetWindows>>,
    review: StoredValue<ReviewFields>,
    service: StoredValue<std::rc::Rc<dyn dna_calc_model::WorkbookTransport>, LocalStorage>,
    skin_registry: StoredValue<crate::skin::SkinRegistry>,
    #[cfg(target_arch = "wasm32")]
    input_surfaces: StoredValue<input_adapter::Surfaces, LocalStorage>,
    pub active_skin: ModelField<Option<String>>,
    pub find_open: ModelField<Option<FindMode>>,
    pub context_menu: ModelField<Option<(f64, f64)>>,
    pub review_open: ModelField<bool>,
    pub validation_decision: ModelField<Option<wire::ValidationDecision>>,
    validation_callback: StoredValue<Option<ValidationCallback>, LocalStorage>,
    validation_outcome: ModelField<Option<Outcome>>,
    pub structural_choice: ModelField<Option<bool>>,
    sheet_selections: ModelField<BTreeMap<SheetId, (CellKey, Vec<GridArea>)>>,
    mirrors: StoredValue<BTreeMap<u64, MirrorHandle>, LocalStorage>,
    grid_mirror: RwSignal<Option<(u64, MirrorHandle)>>,
    next_subscription: ModelField<u64>,
    /// The workbook as the shell shows it: name, sheets, calculation mode, revision.
    pub summary: ModelField<Snapshot<Summary>>,
    /// Advances with every change to the workbook, an open included.
    pub revision: ModelField<u64>,
    pub(crate) font_revision: ModelField<u64>,
    /// Advances with every workbook opened: subscriptions belong to one generation.
    pub generation: ModelField<u64>,
    pub sheet: ModelField<Option<SheetId>>,
    /// The active cell's row and column on the active sheet.
    pub active: ModelField<(u32, u32)>,
    pub reveal_epoch: ModelField<u64>,
    /// The selected range's corner opposite the active cell: the range is the rectangle of the
    /// two (one cell when they are the same).
    pub corner: ModelField<(u32, u32)>,
    pub areas: ModelField<Vec<GridArea>>,
    /// The range copied (Ctrl+C), which a paste copies from.
    pub clipboard: ModelField<Option<GridArea>>,
    pub copied: ModelField<Option<Snapshot<wire::Clipboard>>>,
    copy_pending: ModelField<usize>,
    copy_epoch: ModelField<u64>,
    pub clipboard_fallback: ModelField<Option<bool>>,
    pub paste_preview: ModelField<Option<wire::PastePreview>>,
    paste_cut: ModelField<bool>,
    pub paste_mode: ModelField<String>,
    pub fill_series: ModelField<String>,
    /// Whether the range on the clipboard was cut (Ctrl+X): a paste moves it.
    pub cut: ModelField<bool>,
    /// The cells of the session's selection, as the application's subscription delivered them.
    pub selection: ModelField<Snapshot<ViewMirror>>,
    pub status: ModelField<Status>,
    /// Where an edit is in progress, if one is.
    pub editing: ModelField<Option<EditOrigin>>,
    /// The text being edited, or the active cell's input when nothing is.
    pub edit_text: ModelField<String>,
    pub assist: FormulaAssist,
    edit_field: StoredValue<FormulaAssist>,
    edit_target: ModelField<Option<CellKey>>,
    edit_original: ModelField<String>,
    edit_areas: ModelField<Vec<GridArea>>,
    pub point_mode: ModelField<bool>,
    pub pointed: ModelField<Option<PointPick>>,
    pub timings: ModelField<Timings>,
    pub transport: ModelField<port::Health>,
    pub download_unconfirmed: ModelField<bool>,
    pub pending_actions: ModelField<usize>,
    pub saving: ModelField<bool>,
    pub replacing: ModelField<bool>,
    pending_changes: ModelField<usize>,
    selection_epoch: ModelField<u64>,
    deferred: StoredValue<VecDeque<Box<dyn FnOnce()>>, LocalStorage>,
    command_answers: StoredValue<BTreeMap<u64, CommandAnswer>, LocalStorage>,
    personal_template: StoredValue<Option<String>>,
    save_download: StoredValue<Option<(u64, u64, SaveDownload)>, LocalStorage>,
    deferred_version: ModelField<u64>,
    replaying: ModelField<bool>,
    flush_scheduled: ModelField<bool>,
}

impl Controller {
    pub fn review_edit(&self) -> ModelField<Option<dna_calc_model::ReviewDraft>> {
        self.review.get_value().edit
    }
    pub fn begin_review_create(
        &self,
        at: wire::Key,
        revision: u64,
        index: u32,
        threaded: bool,
        parent: Option<u32>,
    ) {
        let timestamp = js_sys::Date::new_0()
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        self.model.update(|m| {
            m.runtime
                .begin_review_create(at, revision, index, threaded, parent, timestamp)
        });
    }
    pub fn begin_review_edit(
        &self,
        at: wire::Key,
        revision: u64,
        index: u32,
        text: String,
        threaded: bool,
    ) {
        self.model.update(|m| {
            m.runtime
                .begin_review_edit(at, revision, index, text, threaded)
        });
    }
    pub fn validation_circles(&self) -> ModelField<std::collections::BTreeSet<SheetId>> {
        self.review.get_value().circles
    }
    pub fn validation_scan(&self) -> ModelField<Option<Result<wire::ValidationScan, String>>> {
        self.review.get_value().scan
    }
    pub fn validation_epoch(&self) -> ModelField<u64> {
        self.review.get_value().epoch
    }

    /// A controller over a new workbook.
    pub fn new() -> Controller {
        Self::with_transport(std::rc::Rc::new(port::BrowserTransport))
    }
    pub fn with_transport(
        service: std::rc::Rc<dyn dna_calc_model::WorkbookTransport>,
    ) -> Controller {
        // Mounting normally installs this executor. Injected transports can compose the
        // browser controller directly, so initialize it before registering reactive effects.
        #[cfg(target_arch = "wasm32")]
        let _ = any_spawner::Executor::init_wasm_bindgen();
        let model = RwSignal::new(Model::default());
        crate::presentation::install(model);
        let edit_text = Field::new(model, |m| &m.edit.draft, |m| &mut m.edit.draft);
        let assist = FormulaAssist::new_with_state(
            edit_text.text_binding(),
            dna_formula_ui::formula_field::AssistBindings {
                caret: Field::new(model, |m| &m.editor.caret, |m| &mut m.editor.caret)
                    .state_binding(),
                dismissed: Field::new(model, |m| &m.editor.dismissed, |m| &mut m.editor.dismissed)
                    .state_binding(),
                composing: Field::new(model, |m| &m.editor.composing, |m| &mut m.editor.composing)
                    .state_binding(),
                readonly: Field::new(model, |m| &m.editor.readonly, |m| &mut m.editor.readonly)
                    .state_binding(),
                history: dna_formula_ui::formula_field::HistoryBinding::new(Field::new(
                    model,
                    |m| &m.editor.history,
                    |m| &mut m.editor.history,
                )),
                popup: model_view_field::<dna_formula_ui::editor::CompletionState, _>(
                    model,
                    SkinId::new("shared-editor"),
                    |s| &s.popup,
                    |s| &mut s.popup,
                )
                .state_binding(),
            },
        );
        let review = StoredValue::new(ReviewFields {
            edit: Field::new(
                model,
                |m| &m.runtime.review_edit,
                |m| &mut m.runtime.review_edit,
            ),
            circles: Field::new(
                model,
                |m| &m.runtime.validation_circles,
                |m| &mut m.runtime.validation_circles,
            ),
            scan: Field::new(
                model,
                |m| &m.runtime.validation_scan,
                |m| &mut m.runtime.validation_scan,
            ),
            epoch: Field::new(
                model,
                |m| &m.runtime.validation_epoch,
                |m| &mut m.runtime.validation_epoch,
            ),
        });
        let controller = Controller {
            model,
            sheet_windows: StoredValue::new(None),
            review,
            service: StoredValue::new_local(service.clone()),
            skin_registry: StoredValue::new(crate::skin::SkinRegistry::new()),
            #[cfg(target_arch = "wasm32")]
            input_surfaces: StoredValue::new_local(input_adapter::Surfaces::default()),
            active_skin: Field::new(model, |m| &m.active_skin, |m| &mut m.active_skin),
            find_open: Field::new(model, |m| &m.find, |m| &mut m.find),
            context_menu: Field::new(
                model,
                |m| &m.runtime.context_menu,
                |m| &mut m.runtime.context_menu,
            ),
            review_open: Field::new(
                model,
                |m| &m.runtime.review_open,
                |m| &mut m.runtime.review_open,
            ),
            validation_decision: Field::new(
                model,
                |m| &m.runtime.validation_decision,
                |m| &mut m.runtime.validation_decision,
            ),
            validation_callback: StoredValue::new_local(None),
            validation_outcome: Field::new(
                model,
                |m| &m.runtime.validation_outcome,
                |m| &mut m.runtime.validation_outcome,
            ),
            structural_choice: Field::new(
                model,
                |m| &m.runtime.structural_choice,
                |m| &mut m.runtime.structural_choice,
            ),
            sheet_selections: Field::new(
                model,
                |m| &m.selection.remembered,
                |m| &mut m.selection.remembered,
            ),
            mirrors: StoredValue::new_local(BTreeMap::new()),
            grid_mirror: RwSignal::new(None),
            next_subscription: Field::new(
                model,
                |m| &m.runtime.next_subscription,
                |m| &mut m.runtime.next_subscription,
            ),
            summary: Field::new(model, |m| &m.runtime.summary, |m| &mut m.runtime.summary),
            revision: Field::new(model, |m| &m.runtime.revision, |m| &mut m.runtime.revision),
            font_revision: Field::new(
                model,
                |m| &m.runtime.font_revision,
                |m| &mut m.runtime.font_revision,
            ),
            generation: Field::new(
                model,
                |m| &m.runtime.generation,
                |m| &mut m.runtime.generation,
            ),
            sheet: Field::new(model, |m| &m.selection.sheet, |m| &mut m.selection.sheet),
            reveal_epoch: Field::new(
                model,
                |m| &m.selection.reveal_epoch,
                |m| &mut m.selection.reveal_epoch,
            ),
            active: Field::new(model, |m| &m.selection.active, |m| &mut m.selection.active),
            corner: Field::new(model, |m| &m.selection.corner, |m| &mut m.selection.corner),
            areas: Field::new(model, |m| &m.selection.areas, |m| &mut m.selection.areas),
            clipboard: Field::new(
                model,
                |m| &m.runtime.clipboard,
                |m| &mut m.runtime.clipboard,
            ),
            copied: Field::new(model, |m| &m.runtime.copied, |m| &mut m.runtime.copied),
            copy_pending: Field::new(
                model,
                |m| &m.runtime.copy_pending,
                |m| &mut m.runtime.copy_pending,
            ),
            copy_epoch: Field::new(
                model,
                |m| &m.runtime.copy_epoch,
                |m| &mut m.runtime.copy_epoch,
            ),
            clipboard_fallback: Field::new(
                model,
                |m| &m.runtime.clipboard_fallback,
                |m| &mut m.runtime.clipboard_fallback,
            ),
            paste_preview: Field::new(
                model,
                |m| &m.runtime.paste_preview,
                |m| &mut m.runtime.paste_preview,
            ),
            paste_cut: Field::new(
                model,
                |m| &m.runtime.paste_cut,
                |m| &mut m.runtime.paste_cut,
            ),
            paste_mode: Field::new(
                model,
                |m| &m.runtime.paste_mode,
                |m| &mut m.runtime.paste_mode,
            ),
            fill_series: Field::new(
                model,
                |m| &m.runtime.fill_series,
                |m| &mut m.runtime.fill_series,
            ),
            cut: Field::new(model, |m| &m.runtime.cut, |m| &mut m.runtime.cut),
            selection: Field::new(
                model,
                |m| &m.runtime.selection,
                |m| &mut m.runtime.selection,
            ),
            status: Field::new(model, |m| &m.runtime.status, |m| &mut m.runtime.status),
            editing: Field::new(model, |m| &m.edit.surface, |m| &mut m.edit.surface),
            edit_text,
            assist,
            edit_field: StoredValue::new(assist),
            edit_target: Field::new(model, |m| &m.edit.target, |m| &mut m.edit.target),
            edit_original: Field::new(model, |m| &m.edit.original, |m| &mut m.edit.original),
            edit_areas: Field::new(model, |m| &m.edit.areas, |m| &mut m.edit.areas),
            point_mode: Field::new(model, |m| &m.edit.point_mode, |m| &mut m.edit.point_mode),
            pointed: Field::new(model, |m| &m.edit.pointed, |m| &mut m.edit.pointed),
            timings: Field::new(model, |m| &m.runtime.timings, |m| &mut m.runtime.timings),
            transport: Field::new(
                model,
                |m| &m.runtime.transport,
                |m| &mut m.runtime.transport,
            ),
            download_unconfirmed: Field::new(
                model,
                |m| &m.runtime.download_unconfirmed,
                |m| &mut m.runtime.download_unconfirmed,
            ),
            pending_actions: Field::new(
                model,
                |m| &m.runtime.pending_actions,
                |m| &mut m.runtime.pending_actions,
            ),
            saving: Field::new(model, |m| &m.runtime.saving, |m| &mut m.runtime.saving),
            replacing: Field::new(
                model,
                |m| &m.runtime.replacing,
                |m| &mut m.runtime.replacing,
            ),
            pending_changes: Field::new(
                model,
                |m| &m.runtime.pending_changes,
                |m| &mut m.runtime.pending_changes,
            ),
            selection_epoch: Field::new(
                model,
                |m| &m.runtime.selection_epoch,
                |m| &mut m.runtime.selection_epoch,
            ),
            deferred: StoredValue::new_local(VecDeque::new()),
            command_answers: StoredValue::new_local(BTreeMap::new()),
            personal_template: StoredValue::new(None),
            save_download: StoredValue::new_local(None),
            deferred_version: Field::new(
                model,
                |m| &m.runtime.deferred_version,
                |m| &mut m.runtime.deferred_version,
            ),
            replaying: Field::new(
                model,
                |m| &m.runtime.replaying,
                |m| &mut m.runtime.replaying,
            ),
            flush_scheduled: Field::new(
                model,
                |m| &m.runtime.flush_scheduled,
                |m| &mut m.runtime.flush_scheduled,
            ),
        };
        controller.register_editor_surface(
            dna_calc_model::SurfaceDescriptor::selection_editor(EditOrigin::Bar),
            assist,
        );
        // F9 on a selected part of the formula being edited asks the workbook for that part's value,
        // as the edited cell would read it (dc-0f1k.7).
        assist.enable_part_evaluation(move |start, len, done| {
            let Some(anchor) = controller.edit_anchor() else {
                done(Err("No cell is being edited".to_owned()));
                return;
            };
            controller.ask_result(
                Request::EvaluatePart {
                    formula: assist.text.get_untracked(),
                    anchor: wire::Key::of(anchor),
                    start,
                    len,
                },
                move |reply| {
                    done(match reply {
                        Ok(Reply::Part(part)) => part.map(|part| part.literal),
                        Ok(other) => Err(format!("Unexpected answer: {other:?}")),
                        Err(message) => Err(format!("The workbook did not answer: {message}")),
                    })
                },
            );
        });
        service.start();
        service.set_events(std::rc::Rc::new(move |event| controller.receive(event)));
        service.set_health(std::rc::Rc::new(move |health| {
            controller.transport.try_set(health);
        }));
        controller.ask_result(Request::Summary, move |reply| match reply {
            Ok(Reply::Summary(summary)) => {
                controller.summary.set(summary.into());
                if let Some((name, bytes)) = crate::files::default_template("Book") {
                    controller.open_template(&name, bytes, move |accepted| {
                        if !accepted {
                            controller.new_workbook();
                        }
                    });
                } else {
                    controller.new_workbook();
                }
            }
            Err(message) => controller.say(format!("New workbook failed: {message}"), true),
            _ => controller.say("New workbook did not answer", true),
        });
        controller
    }

    pub fn input_pending(&self) -> bool {
        self.deferred_version.track();
        self.pending_actions.get() > 0 || self.has_deferred()
    }

    fn has_deferred(&self) -> bool {
        self.deferred.with_value(|queue| !queue.is_empty())
    }
    fn waiting_input(&self) -> bool {
        !self.replaying.get_value()
            && (self.pending_actions.get_untracked() > 0 || self.has_deferred())
    }
    /// Keep a pointer command's meaning behind earlier keys, including a queued formula seed.
    pub fn after_input(&self, action: impl FnOnce() + 'static) {
        if self.waiting_input() {
            self.enqueue(action);
        } else {
            self.perform(action);
        }
    }

    fn enqueue(&self, action: impl FnOnce() + 'static) {
        self.enqueue_admitted(move |accepted| {
            if accepted {
                action();
            }
        });
    }
    fn enqueue_admitted(&self, action: impl FnOnce(bool) + 'static) {
        let mut action = Some(action);
        let queued = self
            .deferred
            .try_update_value(|queue| {
                if queue.len() >= wire::MAX_PENDING_REQUESTS {
                    return false;
                }
                let action = action.take().unwrap();
                queue.push_back(Box::new(move || action(true)));
                true
            })
            .unwrap_or(false);
        if queued {
            self.deferred_version.update(|v| *v += 1);
        } else {
            if let Some(action) = action {
                action(false);
            }
            self.say(
                "Too many pending input actions; wait for the current operation",
                true,
            );
        }
    }
    fn perform(&self, action: impl FnOnce()) {
        let prior = self.replaying.get_value();
        self.replaying.set_value(true);
        action();
        self.replaying.set_value(prior);
    }

    pub(crate) fn call(
        &self,
        request: Request,
        bytes: Option<Vec<u8>>,
        answer: impl FnOnce(Result<Reply, String>, Option<Vec<u8>>) + 'static,
    ) {
        let service = self.service.get_value();
        let controller = *self;
        let focus = self.focus_snapshot();
        service.call(
            request,
            bytes,
            Box::new(move |reply, bytes| {
                controller.with_response_focus(focus, || answer(reply, bytes))
            }),
        );
    }
    /// Asks the server; `answer` gets the reply when it comes (a failure goes to the status line).
    pub fn ask(&self, request: Request, answer: impl FnOnce(Reply) + 'static) {
        let controller = *self;
        self.ask_result(request, move |reply| match reply {
            Ok(reply) => answer(reply),
            Err(message) => controller.say(format!("The workbook did not answer: {message}"), true),
        });
    }

    /// A local query owns its unavailable state without overwriting the last workbook action.
    pub fn ask_result(
        &self,
        request: Request,
        answer: impl FnOnce(Result<Reply, String>) + 'static,
    ) {
        let controller = *self;
        let ticket = self.focus_snapshot();
        self.call(request, None, move |reply, _| {
            controller.with_response_focus(ticket, || answer(reply))
        });
    }

    /// Model submission owns identities, ordering and replacement checks; callbacks and the
    /// browser-authorised effects that consume receipts stay in this adapter.
    fn send_command(
        &self,
        request: Request,
        bytes: Option<Vec<u8>>,
        answer: impl FnOnce(Result<Reply, String>, Option<Vec<u8>>) + 'static,
    ) {
        let answer: CommandAnswer = Box::new(answer);
        let focus = self.focus_snapshot();
        match self
            .model
            .try_update(|model| model.submit_command_with_focus(request, bytes, focus))
        {
            Some(Ok(id)) => {
                self.command_answers.update_value(|answers| {
                    answers.insert(id, answer);
                });
                self.pump_commands();
            }
            Some(Err(reason)) => answer(Err(reason), None),
            None => answer(Err("The application session has closed".into()), None),
        }
    }
    fn pump_commands(&self) {
        let Some(delivery) = self.model.try_update(Model::take_command).flatten() else {
            return;
        };
        let controller = *self;
        let id = delivery.id();
        let focus = delivery.focus();
        let finished = move |reply, bytes| {
            let answer = controller
                .command_answers
                .try_update_value(|answers| answers.remove(&id))
                .flatten();
            if let Some(answer) = answer {
                controller.with_response_focus(focus, || answer(reply, bytes));
            }
            if controller
                .model
                .try_update(|model| model.finish_command(id))
                .is_some()
            {
                controller.pump_commands();
            }
        };
        match delivery {
            dna_calc_model::CommandDelivery::Ready {
                envelope, bytes, ..
            } => self.call(Request::Command(envelope), bytes, finished),
            dna_calc_model::CommandDelivery::Refused { reason, .. } => finished(Err(reason), None),
        }
    }

    /// The sheets, in workbook order.
    pub fn all_sheets(&self) -> Vec<SheetTab> {
        self.summary.with(|summary| summary.all_sheets())
    }

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

    /// The active cell as displayed by the committed projection; `None` when it is empty.
    pub fn active_cell(&self) -> Option<wire::Cell> {
        let at = self.active_key()?;
        self.with_active_cell_mirror(at, |mirror| mirror.cell(at).cloned())
    }

    fn with_active_cell_mirror<R>(&self, at: CellKey, read: impl Fn(&ViewMirror) -> R) -> R {
        if let Some((_, grid)) = self.grid_mirror.get() {
            let workbook = self.summary.with_untracked(|summary| summary.workbook);
            let covered = grid.try_with(|mirror| {
                let current = mirror.stamp().is_some_and(|stamp| stamp.workbook == workbook);
                let covered = mirror.geometry(at.sheet()).is_some_and(|geometry| {
                    geometry
                        .coverage
                        .iter()
                        .filter_map(|area| area.area())
                        .any(|area| area.contains(at))
                });
                (current && covered).then(|| read(mirror))
            });
            if let Some(result) = covered.flatten() {
                return result;
            }
        }
        self.selection.with(|mirror| read(mirror))
    }

    pub(crate) fn say(&self, text: impl Into<String>, refused: bool) {
        self.status.set(Status {
            text: text.into(),
            refused,
        });
    }

    /// Projection controls bracket binary fragments. Only a closed transaction and its
    /// completion barrier replace the root a skin can read.
    fn receive(&self, event: Event) {
        if let Event::WorkbookEventFailed {
            workbook,
            event,
            reason,
            omitted,
            ..
        } = &event
        {
            if self.summary.with_untracked(|current| current.workbook) == *workbook {
                let omitted = if *omitted == 0 {
                    String::new()
                } else {
                    format!("; {omitted} earlier handler failures")
                };
                self.say(format!("{event}: {reason}{omitted}"), true);
            }
            return;
        }
        if let Event::MacroFiles { workbook, count } = event {
            if self.summary.with_untracked(|s| s.workbook) == workbook {
                for _ in 0..count {
                    let controller = *self;
                    self.send_command(Request::TakeMacroFile, None, move |reply, bytes| {
                        if let (Ok(Reply::MacroFile { name, .. }), Some(bytes)) = (reply, bytes) {
                            // The macro named a path; the browser keeps a download's file name only.
                            let file = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_owned();
                            match crate::app::download(&file, &bytes) {
                                Ok(()) => controller.say(format!("A macro wrote {file}; the download started"), false),
                                Err(error) => controller.say(format!("A macro wrote {file}; the download did not start: {error}"), true),
                            }
                        }
                    });
                }
            }
            return;
        }
        if let Event::SaveReady { workbook, token } = event {
            let mut download = None;
            self.save_download.update_value(|pending| {
                if pending
                    .as_ref()
                    .is_some_and(|(owner, id, _)| *owner == workbook && *id == token)
                {
                    download = pending.take().map(|(_, _, saved)| saved);
                }
            });
            if let Some(saved) = download {
                if self.summary.with_untracked(|s| s.workbook) != workbook {
                    return;
                }
                let controller = *self;
                let generation = self.generation.get_untracked();
                self.send_command(Request::PollSave { token }, None, move |reply, bytes| {
                    if controller.generation.get_untracked() == generation {
                        controller.save_reply(reply, bytes, saved);
                    }
                });
            }
            return;
        }
        if let Event::Calculation {
            workbook,
            receipt,
            summary,
            failed_projections,
        } = event
        {
            if self.summary.with_untracked(|current| current.workbook) != workbook {
                return;
            }
            let frontier = self
                .model
                .with_untracked(|m| m.runtime.calculation_frontier);
            match &receipt {
                wire::CalculationEvent::Committed { revision } => {
                    if *revision < frontier {
                        return;
                    }
                    self.model.update(|m| {
                        m.runtime.calculation_frontier = *revision;
                        m.runtime.active_calculation = None;
                    });
                }
                wire::CalculationEvent::Admitted {
                    domain,
                    command,
                    revision,
                    ..
                } => {
                    if *revision < frontier {
                        return;
                    }
                    self.model.update(|m| {
                        m.runtime.calculation_frontier = *revision;
                        m.runtime.active_calculation = Some((*domain, *command));
                    });
                }
                wire::CalculationEvent::Published(unit) => {
                    if self.model.with_untracked(|m| m.runtime.active_calculation)
                        != Some((unit.domain, unit.command))
                    {
                        return;
                    }
                }
                wire::CalculationEvent::Completed {
                    domain,
                    command,
                    revision,
                    ..
                } => {
                    if *revision != frontier
                        || self.model.with_untracked(|m| m.runtime.active_calculation)
                            != Some((*domain, *command))
                    {
                        return;
                    }
                }
                wire::CalculationEvent::Yielded {
                    domain, command, ..
                }
                | wire::CalculationEvent::AwaitingAsync {
                    domain, command, ..
                }
                | wire::CalculationEvent::Reevaluated {
                    domain, command, ..
                }
                | wire::CalculationEvent::TargetsRefused {
                    domain, command, ..
                } => {
                    if self.model.with_untracked(|m| m.runtime.active_calculation)
                        != Some((*domain, *command))
                    {
                        return;
                    }
                }
                _ => {}
            }
            if let Some(summary) = summary {
                if summary.revision
                    < self
                        .model
                        .with_untracked(|m| m.runtime.calculation_frontier)
                    || summary.revision < self.summary.with_untracked(|s| s.revision)
                {
                    return;
                }
                self.summary.set(summary.into());
            }
            if matches!(
                receipt,
                wire::CalculationEvent::Admitted { .. }
                    | wire::CalculationEvent::Published(_)
                    | wire::CalculationEvent::Completed { .. }
            ) {
                self.validation_epoch().update(|epoch| {
                    *epoch = epoch
                        .checked_add(1)
                        .expect("validation publication identities exhausted")
                });
            }
            match receipt {
                wire::CalculationEvent::AwaitingAsync { .. } => {
                    self.say("Waiting for a provider", false)
                }
                wire::CalculationEvent::Yielded { remaining, .. } => {
                    self.say(format!("Calculating {remaining} formulas"), false)
                }
                wire::CalculationEvent::Completed { .. }
                    if !failed_projections.is_empty()
                        || self.mirrors.with_value(|mirrors| {
                            mirrors.values().any(|mirror| {
                                mirror
                                    .try_with_untracked(|m| m.needs_reset())
                                    .unwrap_or(false)
                            })
                        }) =>
                {
                    if !self.status.with_untracked(|s| s.refused) {
                        self.say(
                            "Calculation finished; a displayed area could not be updated",
                            true,
                        );
                    }
                }
                wire::CalculationEvent::Completed {
                    outcome: wire::CalculationOutcome::Complete,
                    ..
                } => self.say("Recalculated", false),
                wire::CalculationEvent::Completed {
                    outcome: wire::CalculationOutcome::Incomplete,
                    ..
                } => {
                    // Excel with iteration on shows no circular-reference state: the cycle's
                    // formulas are iterated, and only the diagnostics keep the cycle facts.
                    let iterating = self
                        .summary
                        .with_untracked(|s| s.iteration && s.circular_count > 0);
                    if iterating {
                        self.say("Recalculated", false)
                    } else {
                        self.say("Calculation finished with unresolved results", true)
                    }
                }
                _ => {}
            }
            return;
        }
        use dna_calc_model::TileApply;
        let id = match &event {
            Event::TileAdmission(s) => s.id,
            Event::Tiles(b) => b.stamp.id,
            Event::TileCompletion(b) => b.id,
            Event::ProjectionFailed { id, .. } => *id,
            Event::Calculation { .. }
            | Event::SaveReady { .. }
            | Event::MacroFiles { .. }
            | Event::WorkbookEventFailed { .. } => {
                unreachable!("owner controls are handled above")
            }
        };
        let Some(mirror) = self.mirrors.with_value(|m| m.get(&id).copied()) else {
            return;
        };
        let was_failed = mirror
            .try_with_untracked(|m| m.needs_reset())
            .unwrap_or(true);
        let result = mirror.try_update(|mirror| match &event {
            Event::TileAdmission(stamp) => mirror.admit(*stamp),
            Event::Tiles(batch) => mirror.stage(batch),
            Event::TileCompletion(barrier) => mirror.complete(*barrier),
            Event::Calculation { .. }
            | Event::SaveReady { .. }
            | Event::MacroFiles { .. }
            | Event::WorkbookEventFailed { .. } => {
                unreachable!("owner controls are handled above")
            }
            Event::ProjectionFailed { .. } => {
                mirror.invalidate();
                Ok(TileApply::Ignored)
            }
        });
        match result {
            Some(Ok(TileApply::Applied(barrier))) => self.ask(Request::TileAck(barrier), |_| {}),
            Some(Err(reason)) if !was_failed => {
                self.say(
                    format!(
                        "Display publication interrupted ({reason:?}); requesting its current tiles"
                    ),
                    true,
                );
                self.ask(
                    Request::ResetTiles {
                        workbook: self.summary.with_untracked(|s| s.workbook),
                        id,
                    },
                    |_| {},
                );
            }
            _ => {}
        }
        if let Event::ProjectionFailed { reason, .. } = event {
            self.say(reason, true);
        }
    }

    /// A new workbook is open (an open or a new workbook): a new generation, the first sheet
    /// active at A1, and the application's selection subscription.
    fn installed(&self, summary: Summary, message: String) {
        self.personal_template.set_value(None);
        self.saving.set(false);
        let restore_focus = self
            .captured_command_focus()
            .is_none_or(|ticket| self.model.with_untracked(|m| m.focus.accepts(ticket)));
        let prior_focus = self.model.with_untracked(|m| m.runtime.callback_focus);
        self.model.update(|m| {
            m.reset_workbook();
            m.runtime.tools.page.views.extend(
                summary
                    .sheet_view_modes
                    .iter()
                    .map(|&(sheet, mode)| (sheet.id(), mode)),
            );
            m.runtime.active_calculation = None;
            m.runtime.calculation_frontier = summary.revision;
        });
        if restore_focus {
            let fresh = self.model.with_untracked(|m| m.focus.snapshot());
            self.model
                .update(|m| m.runtime.callback_focus = Some(fresh));
        }
        self.generation.update(|n| *n += 1);
        self.save_download.set_value(None);
        self.model.update(|model| model.runtime.grid_geometry.clear());
        self.mirrors.update_value(BTreeMap::clear);
        let first = summary
            .initial_cell
            .and_then(Key::cell)
            .map(|at| at.sheet())
            .filter(|id| summary.sheets.iter().any(|(s, _)| s.id() == *id))
            .or_else(|| summary.sheets.first().map(|(sheet, _)| sheet.id()));
        let initial = summary
            .initial_cell
            .and_then(Key::cell)
            .filter(|at| Some(at.sheet()) == first)
            .map_or((1, 1), |at| (at.row(), at.column()));
        let initial_selection = summary.initial_selection;
        // Other sheets open at the selection their file stored, the first time they are shown.
        let stored: BTreeMap<SheetId, (CellKey, Vec<GridArea>)> = summary
            .sheet_views
            .iter()
            .filter(|(sheet, ..)| Some(sheet.id()) != first)
            .filter_map(|(sheet, active, selection)| {
                let at = CellKey::new(sheet.id(), active.0, active.1).ok()?;
                let far = selection
                    .filter(|(r, c, b, d)| {
                        (active.0 == *r || active.0 == *b) && (active.1 == *c || active.1 == *d)
                    })
                    .map_or(*active, |(r, c, b, d)| (r + b - active.0, c + d - active.1));
                let area = GridArea::new(at, CellKey::new(sheet.id(), far.0, far.1).ok()?).ok()?;
                Some((sheet.id(), (at, vec![area])))
            })
            .collect();
        self.sheet_selections.update_value(|all| {
            all.clear();
            all.extend(stored);
        });
        self.summary.set(summary.into());
        self.download_unconfirmed.set(false);
        self.assist.readonly.set(false);
        self.find_open.set(None);
        self.context_menu.set(None);
        self.validation_decision.set(None);
        self.validation_callback.update_value(|callback| {
            callback.take();
        });
        self.validation_outcome.set_value(None);
        // A stored range selection (with the active cell at a corner) opens as that range.
        let corner = initial_selection
            .filter(|(r, c, b, d)| {
                (initial.0 == *r || initial.0 == *b) && (initial.1 == *c || initial.1 == *d)
            })
            .map(|(r, c, b, d)| (r + b - initial.0, c + d - initial.1));
        self.sheet.set(first);
        self.active.set(initial);
        self.corner.set(corner.unwrap_or(initial));
        self.areas.set(
            first
                .and_then(|sheet| {
                    let far = corner.unwrap_or(initial);
                    GridArea::new(
                        CellKey::new(sheet, initial.0, initial.1).ok()?,
                        CellKey::new(sheet, far.0, far.1).ok()?,
                    )
                    .ok()
                })
                .into_iter()
                .collect(),
        );
        self.deferred.update_value(VecDeque::clear);
        self.deferred_version.update(|v| *v += 1);
        self.selection_epoch.update_value(|v| *v += 1);
        self.clipboard.set(None);
        self.copied.set(None);
        self.copy_pending.set(0);
        self.clipboard_fallback.set(None);
        self.paste_preview.set(None);
        self.selection.set(ViewMirror::default().into());
        self.subscribe(wire::Interest::Selection, self.selection);
        if let Some(at) = self.active_key_untracked() {
            // The stored range itself, not the active cell alone: the reply resets the areas.
            let areas = self.areas.get_untracked();
            if !areas.is_empty() {
                self.dispatch(
                    WorkbookCommand::Select {
                        active: cell_target(at),
                        areas: Target::Cells(areas),
                    },
                    |_| {},
                );
            }
        }
        self.revision.update(|n| *n += 1);
        self.say(message, false);
        self.model
            .update(|m| m.runtime.callback_focus = prior_focus);
    }

    /// Replacing a workbook also risks an uncommitted draft or an unconfirmed download.
    pub fn has_dirty_work(&self) -> bool {
        self.deferred_version.track();
        self.has_deferred()
            || self.pending_changes.get() > 0
            || self.summary.get().changed
            || self.summary.get().unsaved
            || self.download_unconfirmed.get()
            || self.model.with(|m| m.views.pending_reason().is_some())
            || (self.editing.get().is_some()
                && self.edit_text.get() != self.edit_original.get_value())
    }

    pub fn focus_pending_draft(&self) {
        if self.editing.get_untracked().is_some() {
            self.focus_editor();
            return;
        }
        self.focus_active_skin();
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
    /// Carries out an intent; when its outcome comes, applies it (the summary, the revision, the
    /// selection, the status line) and hands it to `then`.
    pub fn dispatch(&self, intent: WorkbookCommand, then: impl FnOnce(&Outcome) + 'static) {
        self.dispatch_result(intent, move |result| {
            if let Ok(outcome) = result {
                then(&outcome);
            }
        });
    }

    /// Carries out an intent that takes bytes beside it (a picture's image, dc-zjsg.4).
    pub fn dispatch_bytes(&self, intent: WorkbookCommand, bytes: Vec<u8>, then: impl FnOnce(&Outcome) + 'static) {
        self.dispatch_result_with(intent, Some(bytes), move |result| {
            if let Ok(outcome) = result {
                then(&outcome);
            }
        });
    }

    fn dispatch_result(
        &self,
        intent: WorkbookCommand,
        then: impl FnOnce(Result<Outcome, String>) + 'static,
    ) {
        self.dispatch_result_with(intent, None, then);
    }

    fn dispatch_result_with(
        &self,
        intent: WorkbookCommand,
        bytes: Option<Vec<u8>>,
        then: impl FnOnce(Result<Outcome, String>) + 'static,
    ) {
        let controller = *self;
        let selects = intent.moves_selection();
        let changes = intent.edits_workbook();
        let request = match intent.request() {
            Ok(request) => request,
            Err(message) => {
                self.say(message, true);
                self.perform(move || then(Err(message.to_owned())));
                return;
            }
        };
        if selects {
            let ticket = self.focus_snapshot();
            self.model
                .update(|m| m.runtime.selection_focus = Some(ticket));
            self.selection_epoch.update_value(|v| *v += 1);
        }
        let epoch = self.selection_epoch.get_value();
        if changes {
            self.pending_changes.update(|n| *n += 1);
        }
        self.pending_actions.update(|n| *n += 1);
        self.send_command(request, bytes, move |reply, _| {
            if changes {
                controller
                    .pending_changes
                    .update(|n| *n = n.saturating_sub(1));
            }
            controller
                .pending_actions
                .update(|n| *n = n.saturating_sub(1));
            match reply {
                Ok(Reply::Done { mut outcome, .. }) => {
                    let current = epoch == controller.selection_epoch.get_value();
                    controller.applied(&outcome, current);
                    if !current {
                        outcome.selection = None;
                    }
                    if let Some(decision) = outcome.validation.clone() {
                        controller.validation_outcome.set_value(Some(outcome));
                        controller
                            .validation_callback
                            .set_value(Some(Box::new(then)));
                        controller.validation_decision.set(Some(decision));
                    } else {
                        controller.perform(move || then(Ok(outcome)));
                    }
                }
                reply => {
                    let message = match reply {
                        Err(message) => format!("The workbook did not answer: {message}"),
                        _ => "The workbook answered an intent with something else".to_owned(),
                    };
                    controller.say(message.clone(), true);
                    controller.perform(move || then(Err(message)));
                }
            }
        });
    }

    /// The modal has closed before this runs, so returning focus can reach the draft.
    pub fn validation_answer(&self, choice: &str) {
        let Some(decision) = self.validation_decision.get_untracked() else {
            return;
        };
        self.validation_decision.set(None);
        let callback = self
            .validation_callback
            .try_update_value(Option::take)
            .flatten();
        let old = self
            .validation_outcome
            .try_update_value(Option::take)
            .flatten();
        if choice == "keep" && decision.style != "Stop" {
            if let Some(callback) = callback {
                self.dispatch_result(
                    WorkbookCommand::AcceptValidation {
                        token: decision.token,
                    },
                    callback,
                );
            }
        } else {
            if let Some(callback) = callback {
                callback(old.ok_or_else(|| "The entry decision expired".into()));
            }
            if choice == "cancel" {
                self.deferred.update_value(VecDeque::clear);
                self.deferred_version.update(|v| *v += 1);
                self.cancel_edit();
                self.focus_active_skin();
            } else {
                self.focus_editor();
            }
        }
    }

    fn applied(&self, outcome: &Outcome, apply_selection: bool) {
        self.model.update(|model| {
            model.confirmed_selection = outcome.selection.as_ref().and_then(|(at, areas)| {
                Some((at.cell()?, areas.iter().filter_map(|a| a.area()).collect()))
            })
        });
        self.timings
            .update(|timings| timings.last_intent_ms = Some(outcome.elapsed_ms));
        if &outcome.summary != &*self.summary.get_untracked() {
            self.summary.set(outcome.summary.clone().into());
        }
        // The active cell follows the session's selection: an undo or a redo restores the
        // selection of its edit, which may be on another sheet, and then that sheet is shown (as
        // Excel does), so the formula bar and the status line read the cell the selection holds.
        if apply_selection
            && let Some((at, areas)) = &outcome.selection
            && let Some(at) = at.cell()
            && outcome
                .summary
                .sheets
                .iter()
                .any(|(sheet, _)| sheet.id() == at.sheet())
        {
            if Some(at.sheet()) != self.sheet.get_untracked() {
                self.switch_sheet(at.sheet());
            }
            if (at.row(), at.column()) != self.active.get_untracked() {
                self.active.set((at.row(), at.column()));
            }
            let areas: Vec<_> = areas.iter().filter_map(|area| area.area()).collect();
            if self.areas.get_untracked() != areas {
                self.areas.set(areas.clone());
            }
            let corner = areas
                .first()
                .copied()
                .map_or((at.row(), at.column()), |area| opposite(area, at));
            if corner != self.corner.get_untracked() {
                self.corner.set(corner);
            }
        }
        if !outcome
            .summary
            .sheets
            .iter()
            .any(|(id, _)| Some(id.id()) == self.sheet.get_untracked())
            && let Some((sheet, _)) = outcome.summary.sheets.first()
        {
            self.show_sheet(sheet.id());
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
        if self.has_deferred() && !self.replaying.get_value() {
            let controller = *self;
            self.enqueue(move || controller.select_range(active, corner));
            return;
        }
        self.set_range(active, corner, true);
    }

    fn set_range(&self, active: (u32, u32), corner: (u32, u32), reveal_active: bool) {
        if reveal_active {
            self.model.update(|model| model.selection.request_reveal());
        }
        if reveal_active || self.active.get_untracked() != active {
            self.active.set(active);
        }
        self.corner.set(corner);
        if let Some(sheet) = self.sheet.get_untracked() {
            if let (Ok(first), Ok(last)) = (
                CellKey::new(sheet, active.0, active.1),
                CellKey::new(sheet, corner.0, corner.1),
            ) {
                if let Ok(area) = GridArea::new(first, last) {
                    self.areas.set(vec![area]);
                }
            }
        }
        if let (Some(at), Some(area)) =
            (self.active_key_untracked(), self.selected_area_untracked())
        {
            self.dispatch(
                WorkbookCommand::Select {
                    active: cell_target(at),
                    areas: area_target(area),
                },
                |_| {},
            );
        }
    }

    /// Moves the selected range's far corner, keeping the active cell (Shift+Arrow, Shift+click).
    pub fn extend(&self, corner: (u32, u32)) {
        if self.has_deferred() && !self.replaying.get_value() {
            let controller = *self;
            self.enqueue(move || controller.extend(corner));
            return;
        }
        self.set_range(self.active.get_untracked(), corner, false);
    }

    /// The selected range.
    pub fn selected_area(&self) -> Option<GridArea> {
        self.areas.with(|areas| areas.first().copied())
    }

    pub fn selected_area_untracked(&self) -> Option<GridArea> {
        self.areas.with_untracked(|areas| areas.first().copied())
    }

    pub fn navigate(
        &self,
        nav: Navigation,
        extend: bool,
        page_span: u32,
    ) {
        self.navigate_then(nav, extend, page_span, |_| {});
    }

    pub fn navigate_then(
        &self,
        nav: Navigation,
        extend: bool,
        page_span: u32,
        then: impl FnOnce(&Outcome) + 'static,
    ) {
        self.dispatch(
            WorkbookCommand::Navigate {
                nav,
                extend,
                page_height: page_span,
            },
            then,
        );
    }

    pub fn select_areas(&self, at: CellKey, areas: Vec<GridArea>) {
        if self.has_deferred() && !self.replaying.get_value() {
            let controller = *self;
            self.enqueue(move || controller.select_areas(at, areas));
            return;
        }
        self.model.update(|m| m.selection.select(at, areas.clone()));
        self.dispatch(
            WorkbookCommand::Select {
                active: cell_target(at),
                areas: Target::Cells(areas),
            },
            |_| {},
        );
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
    pub fn sheet_command(
        &self,
        sheet: Option<SheetId>,
        change: wire::SheetChange,
        done: impl FnOnce(bool) + 'static,
    ) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue_admitted(move |accepted| {
                if accepted {
                    controller.sheet_command(sheet, change, done);
                } else {
                    done(false);
                }
            });
            return;
        }
        if self.editing.get_untracked().is_some() {
            self.say(
                "Enter or cancel the current edit before changing worksheets",
                true,
            );
            done(false);
            return;
        }
        let add_name = match &change {
            wire::SheetChange::Add { name } => Some(name.clone()),
            _ => None,
        };
        let controller = *self;
        self.dispatch_result(
            WorkbookCommand::EditSheet {
                sheet: sheet.map(Target::Sheet),
                change,
            },
            move |result| {
                let accepted = result.is_ok_and(|outcome| outcome.accepted);
                if accepted && let Some(name) = add_name {
                    let sheet = controller
                        .sheets()
                        .into_iter()
                        .find(|s| s.name == name)
                        .map(|s| s.id);
                    if let Some(sheet) = sheet {
                        controller.show_sheet(sheet);
                    }
                }
                done(accepted);
            },
        );
    }

    /// Shows another sheet, at its A1.
    pub fn show_sheet(&self, sheet: SheetId) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.show_sheet(sheet));
            return;
        }
        if !self
            .summary
            .with_untracked(|summary| summary.sheets.iter().any(|(id, _)| id.id() == sheet))
        {
            self.say("Unhide this worksheet before navigating to it", true);
            return;
        }
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
        if self.sheet.get_untracked() == Some(sheet) {
            return;
        }
        let remembered = self
            .sheet_selections
            .with_value(|all| all.get(&sheet).cloned());
        let at = remembered
            .as_ref()
            .map(|(at, _)| *at)
            .unwrap_or_else(|| CellKey::new(sheet, 1, 1).unwrap());
        let areas = remembered
            .map(|(_, areas)| areas)
            .unwrap_or_else(|| vec![GridArea::cell(at)]);
        self.select_areas(at, areas);
    }

    fn switch_sheet(&self, sheet: SheetId) {
        if self.sheet.get_untracked() != Some(sheet) {
            if let Some(at) = self.active_key_untracked() {
                let areas = self.areas.get_untracked();
                self.sheet_selections.update_value(|all| {
                    all.insert(at.sheet(), (at, areas));
                });
            }
            self.sheet.set(Some(sheet));
        }
    }
    pub fn show_validation_circles(&self, show: bool) {
        if let Some(sheet) = self.sheet.get_untracked() {
            self.validation_circles().update(|all| {
                if show {
                    all.insert(sheet);
                } else {
                    all.remove(&sheet);
                }
            });
            if !show {
                self.validation_scan().set(None);
            } else {
                self.validation_epoch().update(|v| {
                    *v = v
                        .checked_add(1)
                        .expect("validation audit identities exhausted")
                });
            }
        }
    }
    pub fn go_to(&self, at: CellKey) {
        self.navigate_to(at, |_| {});
    }

    /// Navigate after resolving the current draft; a rejected draft keeps the original address.
    pub fn navigate_to(&self, at: CellKey, arrived: impl FnOnce(bool) + 'static) {
        self.navigate_to_with_ticket(at, arrived, None);
    }
    fn navigate_to_with_ticket(
        &self,
        at: CellKey,
        arrived: impl FnOnce(bool) + 'static,
        ticket: Option<dna_calc_model::FocusTicket>,
    ) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue_admitted(move |accepted| {
                if accepted {
                    controller.navigate_to_with_ticket(at, arrived, ticket);
                } else {
                    arrived(false);
                }
            });
            return;
        }
        if self.editing.get_untracked().is_some() {
            let controller = *self;
            self.commit_edit(move |ok| {
                if ok {
                    controller.navigate_to_with_ticket(at, arrived, ticket);
                } else {
                    controller.focus_editor_for(ticket);
                    arrived(false);
                }
            });
            return;
        }
        if Some(at.sheet()) != self.sheet.get_untracked() {
            self.cancel_edit();
            self.switch_sheet(at.sheet());
        }
        self.select(at.row(), at.column());
        arrived(true);
    }

    pub fn go_to_text(&self, text: String, arrived: impl FnOnce(bool) + 'static) {
        self.go_to_text_with_ticket(text, arrived, None);
    }
    fn go_to_text_with_ticket(
        &self,
        text: String,
        arrived: impl FnOnce(bool) + 'static,
        ticket: Option<dna_calc_model::FocusTicket>,
    ) {
        let controller = *self;
        if self.waiting_input() {
            self.enqueue_admitted(move |accepted| {
                if accepted {
                    controller.go_to_text_with_ticket(text, arrived, ticket);
                } else {
                    arrived(false);
                }
            });
            return;
        }
        if self.editing.get_untracked().is_some() {
            self.commit_edit(move |accepted| {
                if accepted {
                    controller.go_to_text_with_ticket(text, arrived, ticket)
                } else {
                    controller.focus_editor_for(ticket);
                    arrived(false)
                }
            });
            return;
        }
        let Some(at) = self.active_key_untracked() else {
            arrived(false);
            return;
        };
        self.selection_epoch.update_value(|v| *v += 1);
        let epoch = self.selection_epoch.get_value();
        self.pending_actions.update(|n| *n += 1);
        self.call(
            Request::ResolveReference {
                anchor: Key::of(at),
                text,
            },
            None,
            move |reply, _| {
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                if epoch != controller.selection_epoch.get_value() {
                    arrived(false);
                    return;
                }
                controller.perform(move || match reply {
                    Ok(Reply::ResolvedReference(Ok(area))) => {
                        if let Some(area) = area.area() {
                            controller.select_areas(area.first(), vec![area]);
                            arrived(true);
                        } else {
                            arrived(false)
                        }
                    }
                    Ok(Reply::ResolvedReference(Err(reason))) | Err(reason) => {
                        controller.say(reason, true);
                        arrived(false)
                    }
                    _ => {
                        controller.say("The workbook did not resolve the reference", true);
                        arrived(false)
                    }
                });
            },
        );
    }

    /// Starts an edit of the active cell: with `seed`, or the cell's input when `None`.
    /// Inserts (`insert`) or deletes rows or columns: the selected whole rows (a row header's
    /// selection) or whole columns, through calc-host's axis edit (one undo step; formulas'
    /// references, number formats and sizes move as Excel moves them). Any other selection says
    /// what to select (dc-09v).
    pub fn edit_axis(&self, insert: bool) {
        if self.editing.get_untracked().is_some() {
            self.say(
                "Enter or cancel the current edit before inserting or deleting cells",
                true,
            );
            return;
        }
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.edit_axis(insert));
            return;
        }
        if self.areas.with_untracked(|areas| areas.len() > 1) {
            self.say("Select one rectangle for this command", true);
            return;
        }
        let Some(area) = self.selected_area_untracked() else {
            return;
        };
        let (first, last) = (area.first(), area.last());
        let whole_rows = first.column() == 1 && last.column() == calc_value::MAX_COLUMNS;
        let whole_columns = first.row() == 1 && last.row() == calc_value::MAX_ROWS;
        let (rows, start, count) = if whole_rows && !whole_columns {
            (true, first.row(), area.row_count())
        } else if whole_columns && !whole_rows {
            (false, first.column(), area.column_count())
        } else {
            self.structural_choice.set(Some(insert));
            return;
        };
        self.dispatch(
            WorkbookCommand::EditAxis {
                sheet: Target::Sheet(area.sheet()),
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
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.clear_active());
            return;
        }
        if self.selected_area_untracked().is_some() {
            self.dispatch(
                WorkbookCommand::Clear {
                    areas: Target::Cells(self.areas.get_untracked()),
                },
                |_| {},
            );
        }
    }

    pub fn undo(&self) {
        self.command(Command::Undo);
    }
    pub fn redo(&self) {
        self.command(Command::Redo);
    }
    pub fn recalculate(&self) {
        self.command(Command::Recalculate);
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
            WorkbookCommand::SetIteration {
                enabled,
                max_iterations,
                max_change_bits: max_change.to_bits(),
            },
            |_| {},
        );
    }

    /// The warning of a circular reference the last recalculation found (dc-y2o.1): `None` when
    /// there is none.
    /// The macro bar: its words and the button it offers, if any (dc-0f1k.22, dc-15dk). Shown for a
    /// workbook whose VBA project is held (disabled, or blocked by its Mark of the Web or network
    /// origin) and for one whose macros ran in spite of such an origin (an information notice).
    pub fn macro_bar(&self) -> Option<MacroBar> {
        self.summary.with(|summary| {
            match &summary.macros {
                wire::MacroStatus::Disabled => Some(MacroBar {
                    text: format!("Macros are disabled for this workbook: its VBA project is held and saved with it, but nothing in it runs until you enable macros"),
                    button: Some(MacroButton::Enable),
                }),
                wire::MacroStatus::Blocked { reason } => Some(MacroBar {
                    text: format!(
                        "{reason}. Warning: unblocking lets its macros run with your access to files and the network; unblock only a file you trust"
                    ),
                    button: Some(MacroButton::Unblock),
                }),
                wire::MacroStatus::Enabled if !summary.macro_note.is_empty() => {
                    Some(MacroBar::note(summary.macro_note.clone()))
                }
                wire::MacroStatus::None | wire::MacroStatus::Enabled => None,
            }
        })
    }

    /// The saved VBA project's portability on the workbook worker's host.
    pub fn portability_badge(&self) -> Option<wire::PortabilityBadge> {
        self.summary.with(|summary| summary.portability.clone())
    }

    /// The level and what pins it, as the page always shows them (dc-15dk).
    pub fn macro_policy(&self) -> MacroPolicy {
        self.summary.with(|summary| summary.macro_policy)
    }

    /// The person chose a macro security level in the page; the native host stores it per user.
    pub fn set_macro_security(&self, level: MacroSecurity) {
        let controller = *self;
        self.pending_actions.update(|n| *n += 1);
        self.send_command(
            Request::SetMacroSecurity { level },
            None,
            move |reply, _| {
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                match reply {
                    Ok(Reply::Done { outcome, .. }) => {
                        controller.applied(&outcome, false);
                        controller.say(format!("Macro security is {}", level.label()), false);
                    }
                    Err(error) => {
                        controller.say(format!("Macro security not changed: {error}"), true)
                    }
                    _ => controller.say("Macro security did not answer", true),
                }
            },
        );
    }

    /// The person enabled macros for the open workbook (the macro bar's button, dc-0f1k.22), or
    /// unblocked a marked or network file and enabled them (dc-15dk).
    pub fn enable_macros(&self, button: MacroButton) {
        let controller = *self;
        let request = match button {
            MacroButton::Enable => Request::EnableMacros,
            MacroButton::Unblock => Request::UnblockMacros,
        };
        self.pending_actions.update(|n| *n += 1);
        self.send_command(request, None, move |reply, _| {
            controller
                .pending_actions
                .update(|n| *n = n.saturating_sub(1));
            match reply {
                Ok(Reply::Done { outcome, .. }) => {
                    controller.applied(&outcome, false);
                    if controller.editing.get_untracked().is_some() {
                        controller.offer_defined_names();
                    }
                    controller.say("Macros are enabled for this workbook", false);
                }
                Err(error) => controller.say(format!("Macros were not enabled: {error}"), true),
                _ => controller.say("Enable macros did not answer", true),
            }
        });
    }

    pub fn circular_warning(&self) -> Option<String> {
        // Excel names a circular reference only while iteration is off.
        let (first, count) = self.summary.with(|summary| {
            if summary.iteration {
                (None, 0)
            } else {
                (summary.circular, summary.circular_count)
            }
        });
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
        self.dispatch(WorkbookCommand::SetCalcMode { manual }, move |_| {
            // Manual mode recalculates nothing, but the mode itself changed.
            controller.revision.update(|n| *n += 1);
        });
    }

    pub fn new_mirror(&self) -> MirrorHandle {
        self.allocate_mirror().1
    }

    pub(crate) fn new_grid_mirror(&self) -> MirrorHandle {
        let (id, field) = self.allocate_mirror();
        let grid_mirror = self.grid_mirror;
        grid_mirror.set(Some((id, field)));
        on_cleanup(move || {
            grid_mirror.try_maybe_update(|mounted| {
                let current = mounted.is_some_and(|(current, _)| current == id);
                if current {
                    *mounted = None;
                }
                (current, ())
            });
        });
        field
    }

    fn allocate_mirror(&self) -> (u64, MirrorHandle) {
        let mut id = 0;
        self.model.update(|model| {
            id = model.runtime.next_projection;
            model.runtime.next_projection += 1;
            model
                .runtime
                .projected_views
                .insert(id, ViewMirror::default().into());
        });
        let model = self.model;
        let field = Field::new(
            model,
            move |m| m.runtime.projected_views.get(&id).expect("live projection"),
            move |m| {
                m.runtime
                    .projected_views
                    .get_mut(&id)
                    .expect("live projection")
            },
        );
        on_cleanup(move || {
            field.retire();
            model.try_update(|model| {
                model.runtime.projected_views.remove(&id);
            });
        });
        (id, field)
    }

    /// Registers a subscription whose cells go to `mirror`; its number for later changes.
    pub fn subscribe(&self, interest: wire::Interest, mirror: MirrorHandle) -> u64 {
        let id = self.next_subscription.get_value();
        self.next_subscription.set_value(
            id.checked_add(1)
                .expect("Subscription identities exhausted"),
        );
        mirror.set(ViewMirror::default().into());
        self.mirrors.update_value(|mirrors| {
            mirrors.insert(id, mirror);
        });
        self.ask(
            Request::Subscribe {
                workbook: self.summary.with_untracked(|s| s.workbook),
                id,
                interest,
            },
            |_| {},
        );
        id
    }

    /// Moves a subscription's interest; its mirror is given the new interest's cells.
    pub fn set_interest(&self, id: u64, interest: wire::Interest) {
        self.ask(
            Request::SetInterest {
                workbook: self.summary.with_untracked(|s| s.workbook),
                id,
                interest,
            },
            |_| {},
        );
    }

    pub fn unsubscribe(&self, id: u64) {
        self.mirrors.update_value(|mirrors| {
            mirrors.remove(&id);
        });
        self.ask(
            Request::Unsubscribe {
                workbook: self.summary.with_untracked(|s| s.workbook),
                id,
            },
            |_| {},
        );
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

/// A fill's source and target in `area`: its first row and the rows below (`down`), or its first
/// column and the columns right of it; `None` when the area has one row (one column).
fn model_view_field<S, T>(
    model: RwSignal<Model>,
    skin: SkinId,
    get: fn(&S) -> &T,
    get_mut: fn(&mut S) -> &mut T,
) -> ModelField<T>
where
    S: ViewState,
    T: Clone + PartialEq + Send + Sync + 'static,
{
    model.maybe_update(|m| m.views.register::<S>(skin));
    Field::new(
        model,
        move |m| get(m.views.get::<S>(skin).expect("registered view schema")),
        move |m| get_mut(m.views.get_mut::<S>(skin).expect("registered view schema")),
    )
}

impl Controller {
    pub(crate) fn evaluate_name(
        &self,
        request: Request,
        answer: impl FnOnce(Result<Reply, String>) + 'static,
    ) {
        self.send_command(request, None, move |reply, _| answer(reply));
    }
    pub(crate) fn name_field<T>(
        &self,
        get: fn(&dna_calc_model::names::NameState) -> &T,
        get_mut: fn(&mut dna_calc_model::names::NameState) -> &mut T,
    ) -> ModelField<T>
    where
        T: Clone + PartialEq + Send + Sync + 'static,
    {
        Field::new(
            self.model,
            move |m| get(&m.names),
            move |m| get_mut(&mut m.names),
        )
    }
    pub(crate) fn format_field<T>(
        &self,
        get: fn(&dna_calc_model::format::FormatState) -> &T,
        get_mut: fn(&mut dna_calc_model::format::FormatState) -> &mut T,
    ) -> ModelField<T>
    where
        T: Clone + PartialEq + Send + Sync + 'static,
    {
        Field::new(
            self.model,
            move |m| get(&m.formatting),
            move |m| get_mut(&mut m.formatting),
        )
    }

    pub(crate) fn tool_field<T>(
        &self,
        get: fn(&dna_calc_model::ToolState) -> &T,
        get_mut: fn(&mut dna_calc_model::ToolState) -> &mut T,
    ) -> ModelField<T>
    where
        T: Clone + PartialEq + Send + Sync + 'static,
    {
        Field::new(
            self.model,
            move |m| get(&m.runtime.tools),
            move |m| get_mut(&mut m.runtime.tools),
        )
    }

    pub(crate) fn invalidate_search(&self) {
        self.model.update(|m| m.runtime.tools.invalidate_search());
    }

    /// A skin registers a pure state schema and holds reactive accessors into that state.
    pub fn view_field<S, T>(
        &self,
        skin: SkinId,
        get: fn(&S) -> &T,
        get_mut: fn(&mut S) -> &mut T,
    ) -> ModelField<T>
    where
        S: ViewState,
        T: Clone + PartialEq + Send + Sync + 'static,
    {
        model_view_field(self.model, skin, get, get_mut)
    }

    pub fn update_view<S: ViewState>(&self, skin: SkinId, update: impl FnOnce(&mut S)) {
        self.model.update(|model| {
            model.views.register::<S>(skin);
            update(
                model
                    .views
                    .get_mut::<S>(skin)
                    .expect("registered view schema"),
            );
        });
    }

    fn command_context(&self) -> CommandContext {
        CommandContext {
            ready: self.sheet.get().is_some(),
            readonly: self.assist.readonly.get(),
            undo: self.history_available(false),
            redo: self.history_available(true),
        }
    }
    pub fn command_availability(&self, id: CommandId) -> Result<(), dna_calc_model::Unavailable> {
        let context = self.command_context();
        self.model.with(|model| model.availability(id, context))
    }

    /// Menus, shell keys and skin gestures all enter here. The model decides the effect;
    /// this adapter executes it against the one worker and the registered focus surfaces.
    pub fn command(&self, command: Command) {
        let captured = self.captured_command_focus();
        if let Some(request) = self
            .model
            .try_update(|model| model.prepare_command_with_focus(command, captured))
        {
            self.dispatch_command(request);
        }
    }
    fn dispatch_command(&self, request: dna_calc_model::CommandRequest) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.dispatch_command(request));
            return;
        }
        let context = untrack(|| self.command_context());
        let result = self
            .model
            .try_update(|model| model.dispatch_request(request, context));
        let Some(result) = result else {
            return;
        };
        match result {
            Err(reason) => self.say(reason.to_string(), true),
            Ok(CommandEffect::Undo { editor }) | Ok(CommandEffect::Redo { editor }) => {
                let redo = matches!(result, Ok(CommandEffect::Redo { .. }));
                if editor {
                    self.pointed.set(None);
                    self.edit_field.get_value().undo(redo);
                } else {
                    self.dispatch(if redo { WorkbookCommand::Redo } else { WorkbookCommand::Undo }, |_| {});
                }
            }
            Ok(CommandEffect::PasteSpecial) => crate::clipboard::open_paste_special(),
            Ok(CommandEffect::FlashFill) => self.flash_fill(),
            Ok(CommandEffect::Recalculate) => self.dispatch(WorkbookCommand::Recalculate, |_| {}),
            Ok(CommandEffect::Dialog) => self.focus_command_dialog(),
            Ok(
                CommandEffect::PageSetup
                | CommandEffect::PrintPreview
                | CommandEffect::FormatCells
                | CommandEffect::NameManager,
            ) => {}
            Ok(CommandEffect::FocusSkin { skin, ticket }) => {
                // The model already transferred the edit; only the ticketed effect may focus it.
                self.edit_field.set_value(self.assist);
                self.focus_after_mount(skin, false, ticket);
            }
            Ok(CommandEffect::Reveal {
                target,
                skin,
                ticket,
            }) => {
                let controller = *self;
                let arrived = move |ok| {
                    if ok {
                        controller
                            .model
                            .update(|model| model.reveal_completed(skin));
                        controller.focus_after_mount(skin, true, ticket);
                    }
                };
                match target {
                    RevealTarget::Cell(at) => {
                        self.navigate_to_with_ticket(at, arrived, Some(ticket))
                    }
                    RevealTarget::Reference(text) => {
                        self.go_to_text_with_ticket(text, arrived, Some(ticket))
                    }
                }
            }
        }
    }

    pub fn reveal(&self, target: Target, skin: SkinId) {
        let Some(at) = target_cell(&target) else {
            self.say("Reveal needs one source cell", true);
            return;
        };
        self.command(Command::Reveal {
            target: RevealTarget::Cell(at),
            skin,
        });
    }
    pub fn reveal_reference(&self, text: String, skin: SkinId) {
        self.command(Command::Reveal {
            target: RevealTarget::Reference(text),
            skin,
        });
    }
}

#[cfg(test)]
mod projection_lifetime_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn deferred_input_overflow_retires_copy_and_answers_navigation_without_running_it() {
        let root = Owner::new();
        root.with(|| {
            use std::{cell::RefCell, rc::Rc};
            let controller = Controller::with_transport(Rc::new(
                dna_calc_worker::transport::InProcessTransport::default(),
            ));
            controller.pending_actions.set(1);
            let seen = Rc::new(RefCell::new(Vec::new()));
            for index in 0..wire::MAX_PENDING_REQUESTS {
                let seen = seen.clone();
                controller.after_input(move || seen.borrow_mut().push(index));
            }
            assert!(seen.borrow().is_empty());
            assert_eq!(
                controller.deferred.with_value(|q| q.len()),
                wire::MAX_PENDING_REQUESTS
            );
            controller.copy();
            controller.cut();
            assert_eq!(controller.copy_pending.get_untracked(), 0);
            let replies = Rc::new(RefCell::new(Vec::new()));
            let at = controller.active_key_untracked().unwrap();
            let reply = replies.clone();
            controller.navigate_to(at, move |ok| reply.borrow_mut().push(ok));
            let reply = replies.clone();
            controller.go_to_text("B9".into(), move |ok| reply.borrow_mut().push(ok));
            let reply = replies.clone();
            controller.sheet_command(
                None,
                wire::SheetChange::Add {
                    name: "Overflow".into(),
                },
                move |ok| reply.borrow_mut().push(ok),
            );
            assert_eq!(&*replies.borrow(), &[false, false, false]);
            assert_eq!(controller.active_key_untracked(), Some(at));
            assert!(!controller.sheets().iter().any(|s| s.name == "Overflow"));
            assert!(controller.status.get_untracked().refused);
            assert_eq!(
                controller.deferred.with_value(|q| q.len()),
                wire::MAX_PENDING_REQUESTS
            );
            // Use the same FIFO and performer as the browser's scheduled input flusher.
            while let Some(action) = controller
                .deferred
                .try_update_value(|q| q.pop_front())
                .flatten()
            {
                controller.perform(action);
            }
            assert_eq!(
                &*seen.borrow(),
                &(0..wire::MAX_PENDING_REQUESTS).collect::<Vec<_>>()
            );
        });
    }

    /// dc-971p.43.2: a file's stored range selection opens as that range, through the open route (the
    /// worker's reply to the opening Select must not reset it to the active cell alone).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_opened_workbook_keeps_its_stored_range_selection() {
        let root = Owner::new();
        root.with(|| {
            let mut source = dna_calc_worker::server::Server::new();
            let id = source.book_mut().host().sheets()[0].id();
            let sheet = Sheet::of(id);
            let geometry = source.geometry(sheet);
            let revision = source.book_mut().host().revision().get();
            let bytes = source
                .execute_current(
                    Request::SaveWithContext {
                        calculate_before_save: Some(false),
                        views: Some(wire::SaveWindow {
                            revision,
                            active_sheet: sheet,
                            sheet_view_modes: vec![(sheet, wire::SheetViewMode::PageBreakPreview)],
                            sheets: vec![wire::SheetWindow {
                                geometry,
                                sheet,
                                active: (2, 2),
                                selection: Some((2, 2, 4, 3)),
                                active_pane: 0,
                                panes: vec![wire::PaneWindow {
                                    index: 0,
                                    x: 0.,
                                    y: 0.,
                                    zoom: 1.,
                                }],
                            }],
                        }),
                    },
                    None,
                )
                .bytes
                .expect("a saved image");
            let controller = Controller::with_transport(std::rc::Rc::new(
                dna_calc_worker::transport::InProcessTransport::default(),
            ));
            controller.open("stored.xlsx", bytes, |opened| assert!(opened));
            let first = controller.sheet.get_untracked().unwrap();
            let want = GridArea::new(
                CellKey::new(first, 2, 2).unwrap(),
                CellKey::new(first, 4, 3).unwrap(),
            )
            .unwrap();
            assert_eq!(controller.active.get_untracked(), (2, 2));
            assert_eq!(controller.corner.get_untracked(), (4, 3));
            assert_eq!(controller.areas.get_untracked(), vec![want]);
            let modes = controller.tool_field(|s| &s.page.views, |s| &mut s.page.views);
            assert_eq!(modes.get_untracked()[&first], wire::SheetViewMode::PageBreakPreview);
            modes.update(|values| {
                values.insert(first, wire::SheetViewMode::PageLayout);
            });
            let image = std::rc::Rc::new(std::cell::RefCell::new(None));
            let target = image.clone();
            controller.save(move |_, bytes| *target.borrow_mut() = Some(bytes));
            let bytes = image.borrow_mut().take().expect("the controller saved a package");
            controller.open("layout.xlsx", bytes, |opened| assert!(opened));
            let first = controller.sheet.get_untracked().unwrap();
            assert_eq!(modes.get_untracked()[&first], wire::SheetViewMode::PageLayout);
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pointing_a_closed_edit_never_reads_a_retired_editor_handle() {
        let root = Owner::new();
        root.with(|| {
            let controller = Controller::with_transport(std::rc::Rc::new(
                dna_calc_worker::transport::InProcessTransport::default(),
            ));
            let mounted = Owner::new();
            let retired = mounted.with(|| controller.assist.another_field());
            controller.edit_field.set_value(retired);
            mounted.cleanup();
            assert!(retired.field.try_get_untracked().is_none());
            assert!(!controller.point_at(1, 1, false));
            assert!(controller.editing.get_untracked().is_none());
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ordinary_table_save_downloads_after_the_worker_ready_event_and_reopen_cancels_old_callback()
    {
        use calc_host::{CalcMode, HostProgress, SkinIntent, WorkBudget};
        for reopen in [false, true] {
            let root = Owner::new();
            root.with(|| {
                let mut server = dna_calc_worker::server::Server::new();
                server
                    .execute_current(
                        Request::Open {
                            name: "tables.xlsx".into(),
                        },
                        Some(
                            include_bytes!(
                                "../../../crates/calc-engine/tests/fixtures/dc_10y/oracle.xlsx"
                            )
                            .to_vec(),
                        ),
                    )
                    .reply
                    .unwrap();
                while server.has_pending_calculation() {
                    server.advance_calculation();
                }
                let s = server
                    .book()
                    .host()
                    .sheets()
                    .iter()
                    .find(|s| s.name().text().to_string_lossy() == "DT")
                    .unwrap()
                    .id();
                server
                    .book_mut()
                    .dispatch(SkinIntent::SetCalcMode(CalcMode::AutomaticExceptTables));
                for (row, column, text) in
                    [(1, 26, "1"), (3, 2, "=B1*Z1"), (4, 1, "2"), (5, 1, "3")]
                {
                    server.book_mut().dispatch(SkinIntent::EnterCell {
                        at: CellKey::new(s, row, column).unwrap(),
                        text: text.into(),
                    });
                    while server.has_pending_calculation() {
                        server.advance_calculation();
                    }
                }
                server.book_mut().dispatch(SkinIntent::Recalculate);
                while server.has_pending_calculation() {
                    server.advance_calculation();
                }
                server.book_mut().dispatch(SkinIntent::EnterCell {
                    at: CellKey::new(s, 1, 26).unwrap(),
                    text: "10".into(),
                });
                while server.has_pending_calculation() {
                    server.advance_calculation();
                }
                server.set_calculation_budget(WorkBudget::evaluations(1));
                let (package, _) = server.book_mut().save().unwrap();
                let transport =
                    std::rc::Rc::new(dna_calc_worker::transport::InProcessTransport::default());
                let controller = Controller::with_transport(transport.clone());
                controller.open("tables.xlsx", package, |opened| assert!(opened));
                let images = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                let target = images.clone();
                controller.save(move |name, bytes| target.borrow_mut().push((name, bytes)));
                assert!(
                    images.borrow().is_empty(),
                    "a pending save has no package bytes"
                );
                let pending = controller
                    .save_download
                    .with_value(|p| p.as_ref().map(|(book, token, _)| (*book, *token)))
                    .expect("retained download callback");
                if reopen {
                    controller.new_workbook();
                    controller.receive(Event::SaveReady {
                        workbook: pending.0,
                        token: pending.1,
                    });
                    assert!(controller.save_download.with_value(|p| p.is_none()));
                    assert!(images.borrow().is_empty());
                } else {
                    for _ in 0..1000 {
                        assert!(!matches!(
                            transport.advance_calculation().unwrap(),
                            HostProgress::Failed(_)
                        ));
                        if !images.borrow().is_empty() {
                            break;
                        }
                    }
                    assert_eq!(images.borrow().len(), 1);
                    assert!(controller.save_download.with_value(|p| p.is_none()));
                    let bytes = images.borrow()[0].1.clone();
                    let reopened = calc_host::HostState::open(
                        std::sync::Arc::new(calc_host::MemorySource::new(bytes)),
                        &calc_host::OpenPolicy::default(),
                        calc_host::HostOptions {
                            calc_mode: Some(CalcMode::Manual),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    let s = reopened
                        .sheets()
                        .iter()
                        .find(|s| s.name().text().to_string_lossy() == "DT")
                        .unwrap()
                        .id();
                    for (row, expected) in [(4, 20.0), (5, 30.0)] {
                        let page = calc_host::project(
                            &reopened,
                            &calc_host::ProjectionRequest {
                                cells: vec![CellKey::new(s, row, 2).unwrap()],
                            },
                        )
                        .unwrap();
                        assert!(matches!(
                            page.cells()[0].value(),
                            calc_host::ProjectedValue::FileCached(cache)
                                if cache.value().to_value().as_number() == Some(expected)
                        ));
                    }
                }
            });
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn retired_projection_removes_model_data_and_refuses_late_events() {
        let root = Owner::new();
        root.with(|| {
            let controller = Controller::with_transport(std::rc::Rc::new(
                dna_calc_worker::transport::InProcessTransport::default(),
            ));
            let mounted = Owner::new();
            let mirror = mounted.with(|| controller.new_mirror());
            let id = controller.subscribe(wire::Interest::Selection, mirror);
            assert_eq!(
                controller
                    .model
                    .with_untracked(|m| m.runtime.projected_views.len()),
                1
            );
            mounted.cleanup();
            assert!(
                controller
                    .model
                    .with_untracked(|m| m.runtime.projected_views.is_empty())
            );
            assert!(mirror.try_get_untracked().is_none());
            assert!(mirror.try_update(|_| ()).is_none());
            let stamp = wire::TileStamp {
                calculation_through: None,
                id,
                workbook: 0,
                transaction: 1,
                base: 0,
                next: 1,
                revision: 0,
                geometry_version: 1,
                style_version: 1,
                dictionary: 1,
                reset: true,
            };
            controller.receive(Event::TileAdmission(stamp));
            controller.receive(Event::Tiles(wire::TileBatch {
                stamp,
                fragment: 0,
                final_fragment: true,
                geometry: Vec::new(),
                cells: Vec::new(),
                removed: Vec::new(),
            }));
            controller.receive(Event::TileCompletion(stamp.barrier()));
            assert!(
                controller
                    .model
                    .with_untracked(|m| m.runtime.projected_views.is_empty())
            );
            controller.unsubscribe(id);
        });
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delayed_calculation_controls_cannot_undo_a_newer_manual_edit() {
        let root = Owner::new();
        root.with(|| {
            let controller = Controller::with_transport(std::rc::Rc::new(
                dna_calc_worker::transport::InProcessTransport::default(),
            ));
            let original = controller.summary.get_untracked();
            let workbook = original.workbook;
            let send = |receipt, summary| {
                controller.receive(Event::Calculation {
                    workbook,
                    receipt,
                    summary,
                    failed_projections: Vec::new(),
                })
            };
            send(
                wire::CalculationEvent::Admitted {
                    domain: 7,
                    command: 11,
                    pass: 0,
                    revision: original.revision,
                    scope: wire::CalculationScope::Workbook,
                    selection: wire::CalculationSelection::ForceAll,
                    range_post_state: wire::CalculationRangePostState::Satisfied,
                    predecessor: None,
                },
                None,
            );
            send(
                wire::CalculationEvent::Committed {
                    revision: original.revision + 1,
                },
                None,
            );
            let settled_epoch = controller.validation_epoch().get_untracked();
            let mut current = (*original).clone();
            current.revision += 1;
            current.changed = true;
            controller.summary.set(current.clone().into());
            controller.say("Entered", false);
            send(
                wire::CalculationEvent::Published(wire::Publication {
                    domain: 7,
                    command: 11,
                    sequence: 1,
                }),
                Some((*original).clone()),
            );
            send(
                wire::CalculationEvent::Completed {
                    domain: 7,
                    command: 11,
                    revision: original.revision,
                    outcome: wire::CalculationOutcome::Complete,
                    required: 1,
                    calculated: 1,
                    refused: 0,
                    held: 0,
                    uncertified: 0,
                    boundary_cached: 0,
                    imported_cached: 0,
                },
                Some((*original).clone()),
            );
            assert_eq!(*controller.summary.get_untracked(), current);
            assert_eq!(controller.status.get_untracked().text, "Entered");
            assert_eq!(
                controller.validation_epoch().get_untracked(),
                settled_epoch,
                "Rejected stale publications cannot invalidate a current preview"
            );
        });
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calculation_completion_preserves_a_failed_projection_message() {
        for client_failure in [false, true] {
            let root = Owner::new();
            root.with(|| {
                let controller = Controller::with_transport(std::rc::Rc::new(
                    dna_calc_worker::transport::InProcessTransport::default(),
                ));
                let summary = controller.summary.get_untracked();
                let display_epoch = controller.revision.get_untracked();
                controller.receive(Event::Calculation {
                    workbook: summary.workbook,
                    receipt: wire::CalculationEvent::Admitted {
                        domain: 7,
                        command: 11,
                        pass: 0,
                        revision: summary.revision,
                        scope: wire::CalculationScope::Workbook,
                        selection: wire::CalculationSelection::ForceAll,
                        range_post_state: wire::CalculationRangePostState::Satisfied,
                        predecessor: None,
                    },
                    summary: None,
                    failed_projections: Vec::new(),
                });
                let admitted_epoch = controller.validation_epoch().get_untracked();
                controller.receive(Event::Calculation {
                    workbook: summary.workbook,
                    receipt: wire::CalculationEvent::Published(wire::Publication {
                        domain: 7,
                        command: 11,
                        sequence: 1,
                    }),
                    summary: None,
                    failed_projections: Vec::new(),
                });
                let published_epoch = controller.validation_epoch().get_untracked();
                assert!(
                    published_epoch > admitted_epoch,
                    "An accepted publication invalidates preview snapshots"
                );
                assert_eq!(
                    controller.revision.get_untracked(),
                    display_epoch,
                    "A print snapshot invalidation must not rebuild the grid during typing"
                );
                if client_failure {
                    controller.selection.update(|mirror| mirror.invalidate());
                }
                controller.say("The displayed area exceeds its tile budget", true);
                controller.receive(Event::Calculation {
                    workbook: summary.workbook,
                    receipt: wire::CalculationEvent::Completed {
                        domain: 7,
                        command: 11,
                        revision: summary.revision,
                        outcome: wire::CalculationOutcome::Complete,
                        required: 1,
                        calculated: 1,
                        refused: 0,
                        held: 0,
                        uncertified: 0,
                        boundary_cached: 0,
                        imported_cached: 0,
                    },
                    summary: Some((*summary).clone()),
                    failed_projections: if client_failure { Vec::new() } else { vec![11] },
                });
                assert!(
                    controller.validation_epoch().get_untracked() > published_epoch,
                    "Completion refreshes a preview refused during calculation"
                );
                assert_eq!(controller.revision.get_untracked(), display_epoch);
                assert!(controller.status.get_untracked().refused);
                assert_eq!(
                    controller.status.get_untracked().text,
                    "The displayed area exceeds its tile budget"
                );
            });
        }
    }
}

#[cfg(test)]
mod template_lifecycle_tests {
    use super::*;
    use dna_calc_model::WorkbookTransport;
    #[derive(Default)]
    struct HeldTransport {
        inner: dna_calc_worker::transport::InProcessTransport,
        hold: std::cell::Cell<bool>,
        hold_tile_ack: std::cell::Cell<Option<u64>>,
        hold_paste: std::cell::Cell<bool>,
        hold_drag_query: std::cell::Cell<u8>,
        pending:
            std::cell::RefCell<Vec<(Request, Option<Vec<u8>>, dna_calc_model::TransportAnswer)>>,
    }
    impl dna_calc_model::WorkbookTransport for HeldTransport {
        fn start(&self) {
            self.inner.start();
        }
        fn set_events(&self, h: std::rc::Rc<dyn Fn(Event)>) {
            self.inner.set_events(h);
        }
        fn set_health(&self, h: std::rc::Rc<dyn Fn(port::Health)>) {
            self.inner.set_health(h);
        }
        fn call(&self, r: Request, b: Option<Vec<u8>>, a: dna_calc_model::TransportAnswer) {
            let inner = match &r {
                Request::Command(envelope) => envelope.request.as_ref(),
                _ => &r,
            };
            let query = matches!(
                (self.hold_drag_query.get(), inner),
                (1, Request::Capture { .. }) | (2, Request::PreparePaste { .. })
            );
            if self.hold.get()
                || query
                || matches!(inner, Request::TileAck(barrier)
                    if self.hold_tile_ack.get() == Some(barrier.id))
                || (self.hold_paste.get()
                    && matches!(inner, Request::Intent(wire::Intent::ApplyPaste { .. })))
            {
                self.pending.borrow_mut().push((r, b, a));
            } else {
                self.inner.call(r, b, a);
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn edit_reply_can_precede_its_grid_publication() {
        let root = Owner::new();
        root.with(|| {
            let transport = std::rc::Rc::new(HeldTransport::default());
            let controller = Controller::with_transport(transport.clone());
            let at = controller.active_key_untracked().unwrap();
            controller.enter(at, "1".into(), |outcome| assert!(outcome.accepted));
            let grid = controller.new_grid_mirror();
            transport.hold_tile_ack.set(Some(controller.next_subscription.get_value()));
            controller.subscribe(
                wire::Interest::Areas(vec![wire::Area::of(GridArea::cell(at))]),
                grid,
            );

            let observed = std::rc::Rc::new(std::cell::RefCell::new(None));
            let in_reply = observed.clone();
            controller.enter(at, "2".into(), move |outcome| {
                assert!(outcome.accepted);
                assert_eq!(
                    grid.with_untracked(|mirror| mirror.cell(at).unwrap().text.clone()),
                    "1"
                );
                *in_reply.borrow_mut() = untrack(|| controller.active_cell().map(|cell| cell.input));
            });
            assert_eq!(observed.borrow().as_deref(), Some("1"));
            transport.hold_tile_ack.set(None);
            let pending = transport.pending.take();
            assert_eq!(pending.len(), 1);
            for (request, bytes, answer) in pending {
                transport.inner.call(request, bytes, answer);
            }
            assert_eq!(untrack(|| controller.active_cell()).unwrap().input, "2");
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn edit_and_undo_receipts_arrive_while_the_grid_ack_is_held() {
        let root = Owner::new();
        root.with(|| {
            let transport = std::rc::Rc::new(HeldTransport::default());
            let controller = Controller::with_transport(transport.clone());
            let at = controller.active_key_untracked().unwrap();
            let grid = controller.new_grid_mirror();
            transport.hold_tile_ack.set(Some(controller.next_subscription.get_value()));
            controller.subscribe(
                wire::Interest::Areas(vec![wire::Area::of(GridArea::cell(at))]),
                grid,
            );

            let observed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let in_edit = observed.clone();
            controller.enter(at, "42".into(), move |outcome| {
                assert!(outcome.accepted);
                let source = untrack(|| controller.active_cell().map(|cell| cell.input));
                in_edit.borrow_mut().push(("edit", source));
            });
            let in_undo = observed.clone();
            controller.dispatch(WorkbookCommand::Undo, move |outcome| {
                assert!(outcome.accepted);
                let source = untrack(|| controller.active_cell().map(|cell| cell.input));
                in_undo.borrow_mut().push(("undo", source));
            });
            assert_eq!(*observed.borrow(), [("edit", None), ("undo", None)]);
            transport.hold_tile_ack.set(None);
            let pending = transport.pending.take();
            assert_eq!(pending.len(), 1);
            for (request, bytes, answer) in pending {
                transport.inner.call(request, bytes, answer);
            }
            assert!(grid.with_untracked(|mirror| mirror.cell(at).is_none()));
            assert!(untrack(|| controller.active_cell()).is_none());
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn renderer_entry_uses_the_resolved_cell_and_sheet() {
        use std::rc::Rc;
        let root = Owner::new();
        root.with(|| {
            let transport = Rc::new(HeldTransport::default());
            let controller = Controller::with_transport(transport.clone());
            let original = controller.active_key_untracked().unwrap();
            controller.enter(original, "original".into(), |result| assert!(result.accepted));
            controller.sheet_command(
                None,
                wire::SheetChange::Add {
                    name: "Other sheet".into(),
                },
                |ok| assert!(ok),
            );
            let other = controller.active_key_untracked().unwrap().sheet();
            assert_ne!(other, original.sheet());
            controller.show_sheet(original.sheet());

            let local = CellKey::new(original.sheet(), 2, 2).unwrap();
            let remote = CellKey::new(other, 12, 27).unwrap();
            for (reference, target, input) in [
                ("B2", local, "local"),
                ("'Other sheet'!AA12", remote, "=20+22"),
            ] {
                transport.hold.set(true);
                controller.go_to_text(reference.into(), move |arrived| assert!(arrived));
                // Grid typing can arrive while the worker is resolving the reference.
                controller.begin_edit(EditOrigin::Cell, Some(input.into()));
                assert!(controller.editing.get_untracked().is_none());
                assert_eq!(transport.pending.borrow().len(), 1);
                transport.hold.set(false);
                for (request, bytes, answer) in transport.pending.take() {
                    transport.inner.call(request, bytes, answer);
                }
                while let Some(action) = controller
                    .deferred
                    .try_update_value(|queue| queue.pop_front())
                    .flatten()
                {
                    controller.perform(action);
                }
                assert_eq!(controller.active_key_untracked(), Some(target));
                assert_eq!(controller.edit_target.get_untracked(), Some(target));
                controller.commit_edit(|accepted| assert!(accepted));
            }

            for (at, expected) in [
                (original, "original"),
                (local, "local"),
                (remote, "42"),
                (CellKey::new(original.sheet(), 12, 27).unwrap(), ""),
            ] {
                controller.call(
                    Request::Capture {
                        area: Area::of(GridArea::cell(at)),
                        keep: None,
                    },
                    None,
                    move |reply, _| match reply.unwrap() {
                        Reply::Clipboard(captured) => assert_eq!(captured.text, expected),
                        other => panic!("expected a clipboard reply, got {other:?}"),
                    },
                );
            }
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_q327__navigation_during_drag_preparation_cancels_the_stale_drag() {
        use std::rc::Rc;
        for stage in [1, 2] {
            let root = Owner::new();
            root.with(|| {
                let transport = Rc::new(HeldTransport::default());
                let controller = Controller::with_transport(transport.clone());
                let source = controller.active_key_untracked().unwrap();
                let destination = CellKey::new(source.sheet(), 3, 4).unwrap();
                controller.dispatch(
                    WorkbookCommand::EnterCell {
                        at: cell_target(source),
                        text: "5".into(),
                    },
                    |outcome| assert!(outcome.accepted),
                );
                let revision = controller.summary.get_untracked().revision;
                transport.hold_drag_query.set(stage);
                grid::GridController::new(controller).drag_range(
                    GridArea::cell(source),
                    GridArea::cell(destination),
                    false,
                    controller.generation.get_untracked(),
                    revision,
                );
                assert_eq!(transport.pending.borrow().len(), 1);
                controller.navigate(
                    Navigation::Step(NavAction::Right),
                    false,
                    20,
                );
                transport.hold_drag_query.set(0);
                let pending = std::mem::take(&mut *transport.pending.borrow_mut());
                for (r, b, a) in pending {
                    transport.inner.call(r, b, a);
                }
                while let Some(action) = controller
                    .deferred
                    .try_update_value(|q| q.pop_front())
                    .flatten()
                {
                    controller.perform(action);
                }
                assert_eq!(
                    controller.active_key_untracked(),
                    Some(CellKey::new(source.sheet(), 1, 2).unwrap())
                );
                assert_eq!(controller.summary.get_untracked().revision, revision);
                assert!(controller.paste_preview.get_untracked().is_none());
                assert!(controller.status.get_untracked().refused);
                assert_eq!(controller.pending_actions.get_untracked(), 0);
                assert_eq!(controller.pending_changes.get_untracked(), 0);
            });
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_q327__drag_selection_is_undoable_and_a_late_paste_preserves_newer_navigation() {
        use std::rc::Rc;
        for delayed in [false, true] {
            let root = Owner::new();
            root.with(|| {
                let transport = Rc::new(HeldTransport::default());
                let controller = Controller::with_transport(transport.clone());
                let source = controller.active_key_untracked().unwrap();
                let destination = CellKey::new(source.sheet(), 3, 4).unwrap();
                controller.dispatch(
                    WorkbookCommand::EnterCell {
                        at: cell_target(source),
                        text: "5".into(),
                    },
                    |outcome| assert!(outcome.accepted),
                );
                transport.hold_paste.set(delayed);
                grid::GridController::new(controller).drag_range(
                    GridArea::cell(source),
                    GridArea::cell(destination),
                    false,
                    controller.generation.get_untracked(),
                    controller.summary.get_untracked().revision,
                );
                if delayed {
                    assert_eq!(transport.pending.borrow().len(), 1);
                    controller.navigate(
                        Navigation::Step(NavAction::Right),
                        false,
                        20,
                    );
                    transport.hold_paste.set(false);
                    let pending = std::mem::take(&mut *transport.pending.borrow_mut());
                    for (r, b, a) in pending {
                        transport.inner.call(r, b, a);
                    }
                    while let Some(action) = controller
                        .deferred
                        .try_update_value(|q| q.pop_front())
                        .flatten()
                    {
                        controller.perform(action);
                    }
                    assert_eq!(
                        controller.active_key_untracked(),
                        Some(CellKey::new(source.sheet(), 3, 5).unwrap())
                    );
                } else {
                    assert_eq!(
                        controller.selected_area_untracked(),
                        Some(GridArea::cell(destination))
                    );
                    controller.dispatch(WorkbookCommand::Undo, |outcome| assert!(outcome.accepted));
                    assert_eq!(
                        controller.selected_area_untracked(),
                        Some(GridArea::cell(source))
                    );
                    controller.dispatch(WorkbookCommand::Redo, |outcome| assert!(outcome.accepted));
                    assert_eq!(
                        controller.selected_area_untracked(),
                        Some(GridArea::cell(destination))
                    );
                }
                assert_eq!(controller.pending_actions.get_untracked(), 0);
                assert_eq!(controller.pending_changes.get_untracked(), 0);
            });
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__pending_insertion_protects_replacement_and_applies_its_summary() {
        use dna_calc_model::WorkbookTransport;
        use std::{cell::Cell, rc::Rc};
        let root = Owner::new();
        root.with(|| {
            let transport = Rc::new(HeldTransport::default());
            let controller = Controller::with_transport(transport.clone());
            let mut source = dna_calc_worker::server::Server::new();
            let bytes = source
                .execute_current(
                    Request::SaveAs {
                        name: "Sheet.xltx".into(),
                        format: wire::FileFormat::Xltx,
                        views: None,
                    },
                    None,
                )
                .bytes
                .unwrap();
            transport.hold.set(true);
            let accepted = Rc::new(Cell::new(false));
            let answer = accepted.clone();
            controller.insert_template(bytes, None, move |ok| answer.set(ok));
            assert!(controller.has_dirty_work());
            assert!(controller.input_pending());
            assert_eq!(controller.pending_changes.get_untracked(), 1);
            let later = Rc::new(Cell::new(false));
            let out = later.clone();
            controller.after_input(move || out.set(true));
            assert!(!later.get());
            transport.hold.set(false);
            let pending = std::mem::take(&mut *transport.pending.borrow_mut());
            for (r, b, a) in pending {
                transport.inner.call(r, b, a);
            }
            assert!(accepted.get());
            assert_eq!(controller.summary.get_untracked().sheets.len(), 2);
            assert!(controller.summary.get_untracked().changed);
            assert_eq!(controller.pending_changes.get_untracked(), 0);
            assert_eq!(controller.pending_actions.get_untracked(), 0);
            while let Some(action) = controller
                .deferred
                .try_update_value(|q| q.pop_front())
                .flatten()
            {
                controller.perform(action);
            }
            assert!(later.get());
        });
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__idle_save_as_opens_and_an_unsaved_template_keeps_download_protection() {
        use std::{cell::Cell, rc::Rc};
        let root = Owner::new();
        root.with(|| {
            let controller = Controller::with_transport(Rc::new(
                dna_calc_worker::transport::InProcessTransport::default(),
            ));
            let opened = Rc::new(Cell::new(false));
            let out = opened.clone();
            controller.prepare_save_as(move || out.set(true));
            assert!(
                opened.get(),
                "idle Save As must open without an editor target"
            );
            let mut source = dna_calc_worker::server::Server::new();
            let bytes = source
                .execute_current(
                    Request::SaveAs {
                        name: "Template.xltx".into(),
                        format: wire::FileFormat::Xltx,
                        views: None,
                    },
                    None,
                )
                .bytes
                .unwrap();
            controller.open("Template.xltx", bytes, |ok| assert!(ok));
            assert!(controller.summary.get_untracked().unsaved);
            assert!(!controller.summary.get_untracked().changed);
            let downloaded = Rc::new(Cell::new(false));
            let out = downloaded.clone();
            controller.save_as(
                "New.xlsx".into(),
                wire::FileFormat::Xlsx,
                move |name, bytes| {
                    assert_eq!(name, "New.xlsx");
                    assert!(!bytes.is_empty());
                    out.set(true);
                },
            );
            assert!(downloaded.get());
            assert!(!controller.saving.get_untracked());
            assert!(!controller.summary.get_untracked().unsaved);
            assert!(controller.download_unconfirmed.get_untracked());
            assert!(controller.has_dirty_work());
        });
    }
}

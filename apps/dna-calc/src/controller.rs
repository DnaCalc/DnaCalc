//! The application's controller: the workbook's server behind reactive signals, shared by the
//! shell and every skin. It is `Copy`, so any view can hold it.
//!
//! The workbook lives in the separate worker composition, reached through
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

use crate::model_adapter::{Field, ModelField};
use crate::port;
use crate::wire::{self, Area, Event, Intent, Key, Outcome, Reply, Request, Sheet, Summary};
use dna_calc_model::Snapshot;
use dna_calc_model::projection::SheetTab;
use dna_calc_model::selection::{GridExtent, NavAction, next_active, opposite};
use dna_calc_model::{
    Command, CommandContext, CommandId, Effect as CommandEffect, FindMode, GridState, Model,
    RevealTarget, SkinId, ViewState,
};
pub use dna_calc_model::{PointPick, SurfaceId as EditOrigin};
pub use dna_calc_model::{Status, Timings, ViewMirror};
pub type MirrorHandle = ModelField<Snapshot<ViewMirror>>;
use calc_value::{CellKey, GridArea, SheetId};
use dna_formula_ui::editor::{cycle_anchors, expects_reference, point_reference_span};
use dna_formula_ui::formula_field::FormulaAssist;
use leptos::prelude::*;
use std::collections::{BTreeMap, VecDeque};

fn field_selection(assist: FormulaAssist) -> (usize, usize) {
    assist.field.try_get_untracked().flatten().map_or_else(
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

pub use dna_formula_ui::web::{memory_bytes, now_ms};

#[path = "input_adapter.rs"]
mod input_adapter;

type CommandAnswer = Box<dyn FnOnce(Result<Reply, String>, Option<Vec<u8>>)>;

type ValidationCallback = Box<dyn FnOnce(Result<Outcome, String>)>;

#[derive(Clone, Copy)]
pub struct Controller {
    model: RwSignal<Model>,
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
    grid_views: ModelField<BTreeMap<(SheetId, u8), crate::grid::viewport::GridView>>,
    grid_active: ModelField<BTreeMap<SheetId, u8>>,
    grid_geometry: ModelField<Snapshot<BTreeMap<SheetId, wire::Geometry>>>,
    mirrors: StoredValue<BTreeMap<u64, MirrorHandle>, LocalStorage>,
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
    pub replacing: ModelField<bool>,
    pending_changes: ModelField<usize>,
    selection_epoch: ModelField<u64>,
    deferred: StoredValue<VecDeque<Box<dyn FnOnce()>>, LocalStorage>,
    command_answers: StoredValue<BTreeMap<u64, CommandAnswer>, LocalStorage>,
    deferred_version: ModelField<u64>,
    replaying: ModelField<bool>,
    flush_scheduled: ModelField<bool>,
}

impl Controller {
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
        let controller = Controller {
            model,
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
            grid_views: model_view_field::<GridState, _>(
                model,
                SkinId::Grid,
                |s| &s.views,
                |s| &mut s.views,
            ),
            grid_active: model_view_field::<GridState, _>(
                model,
                SkinId::Grid,
                |s| &s.active,
                |s| &mut s.active,
            ),
            grid_geometry: Field::new(
                model,
                |m| &m.runtime.grid_geometry,
                |m| &mut m.runtime.grid_geometry,
            ),
            mirrors: StoredValue::new_local(BTreeMap::new()),
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
        service.start();
        service.set_events(std::rc::Rc::new(move |event| controller.receive(event)));
        service.set_health(std::rc::Rc::new(move |health| {
            controller.transport.try_set(health);
        }));
        controller.ask_result(Request::Summary, move |reply| match reply {
            Ok(Reply::Summary(summary)) => {
                controller.summary.set(summary.into());
                controller.new_workbook();
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
        self.deferred
            .update_value(|queue| queue.push_back(Box::new(action)));
        self.deferred_version.update(|v| *v += 1);
    }
    fn perform(&self, action: impl FnOnce()) {
        let prior = self.replaying.get_value();
        self.replaying.set_value(true);
        action();
        self.replaying.set_value(prior);
    }

    fn call(
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

    pub(crate) fn say(&self, text: impl Into<String>, refused: bool) {
        self.status.set(Status {
            text: text.into(),
            refused,
        });
    }

    /// Projection controls bracket binary fragments. Only a closed transaction and its
    /// completion barrier replace the root a skin can read.
    fn receive(&self, event: Event) {
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
            match receipt {
                wire::CalculationEvent::Yielded { remaining, .. } => {
                    self.say(format!("Calculating {remaining} formulas"), false)
                }
                wire::CalculationEvent::Completed { .. } if !failed_projections.is_empty() => {
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
                } => self.say("Calculation finished with unresolved results", true),
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
            Event::Calculation { .. } => unreachable!("calculation controls are handled above"),
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
            Event::Calculation { .. } => unreachable!("calculation controls are handled above"),
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
                self.ask(Request::ResetTiles { id }, |_| {});
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
        let restore_focus = self
            .captured_command_focus()
            .is_none_or(|ticket| self.model.with_untracked(|m| m.focus.accepts(ticket)));
        let prior_focus = self.model.with_untracked(|m| m.runtime.callback_focus);
        self.model.update(|m| {
            m.reset_workbook();
            m.runtime.active_calculation = None;
            m.runtime.calculation_frontier = summary.revision;
        });
        if restore_focus {
            let fresh = self.model.with_untracked(|m| m.focus.snapshot());
            self.model
                .update(|m| m.runtime.callback_focus = Some(fresh));
        }
        self.generation.update(|n| *n += 1);
        self.grid_geometry.update_value(|all| all.clear());
        self.mirrors.update_value(BTreeMap::clear);
        let first = summary.sheets.first().map(|(sheet, _)| sheet.id());
        let initial = summary
            .initial_cell
            .and_then(Key::cell)
            .filter(|at| Some(at.sheet()) == first)
            .map_or((1, 1), |at| (at.row(), at.column()));
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
        self.sheet.set(first);
        self.active.set(initial);
        self.corner.set(initial);
        self.areas.set(
            first
                .and_then(|sheet| CellKey::new(sheet, initial.0, initial.1).ok())
                .map(GridArea::cell)
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
        if let Some(at) = self.active_key_untracked()
            && let Ok(area) = GridArea::new(at, at)
        {
            self.dispatch(
                Intent::Select {
                    active: Key::of(at),
                    areas: vec![Area::of(area)],
                },
                |_| {},
            );
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
    pub fn new_workbook(&self) {
        if self.replacing.get_untracked() {
            return;
        }
        self.replacing.set(true);
        self.deferred.update_value(VecDeque::clear);
        self.deferred_version.update(|v| *v += 1);
        self.pending_actions.update(|n| *n += 1);
        let controller = *self;
        self.send_command(Request::New, None, move |reply, _| {
            controller
                .pending_actions
                .update(|n| *n = n.saturating_sub(1));
            controller.replacing.set(false);
            match reply {
                Ok(Reply::Done { outcome, .. }) => {
                    controller.installed(outcome.summary, "New workbook".to_owned())
                }
                Err(error) => controller.say(format!("New workbook failed: {error}"), true),
                _ => controller.say("New workbook did not answer", true),
            }
        });
    }

    /// Opens a package's bytes under `name`; the current workbook stays when it is refused.
    /// `opened` hears whether it opened.
    pub fn open(&self, name: &str, bytes: Vec<u8>, opened: impl FnOnce(bool) + 'static) {
        if self.replacing.get_untracked() {
            return;
        }
        self.replacing.set(true);
        self.deferred.update_value(VecDeque::clear);
        self.deferred_version.update(|v| *v += 1);
        self.pending_actions.update(|n| *n += 1);
        let controller = *self;
        let size = bytes.len();
        let name = name.to_owned();
        self.send_command(
            Request::Open { name: name.clone() },
            Some(bytes),
            move |reply, _| {
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                controller.replacing.set(false);
                match reply {
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
        let selects = matches!(
            intent,
            Intent::Select { .. } | Intent::Navigate { .. } | Intent::Undo | Intent::Redo
        );
        if selects {
            let ticket = self.focus_snapshot();
            self.model
                .update(|m| m.runtime.selection_focus = Some(ticket));
            self.selection_epoch.update_value(|v| *v += 1);
        }
        let epoch = self.selection_epoch.get_value();
        let changes = !matches!(intent, Intent::Select { .. } | Intent::Navigate { .. });
        if changes {
            self.pending_changes.update(|n| *n += 1);
        }
        self.pending_actions.update(|n| *n += 1);
        self.send_command(Request::Intent(intent), None, move |reply, _| {
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
                    Intent::AcceptValidation {
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
                Intent::Select {
                    active: Key::of(at),
                    areas: vec![Area::of(area)],
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
        nav: crate::grid::selection::Navigation,
        extend: bool,
        page_height: u32,
    ) {
        self.navigate_then(nav, extend, page_height, |_| {});
    }

    pub fn navigate_then(
        &self,
        nav: crate::grid::selection::Navigation,
        extend: bool,
        page_height: u32,
        then: impl FnOnce(&Outcome) + 'static,
    ) {
        self.dispatch(
            Intent::Navigate {
                nav,
                extend,
                page_height,
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
            Intent::Select {
                active: Key::of(at),
                areas: areas.into_iter().map(Area::of).collect(),
            },
            |_| {},
        );
    }

    pub fn toggle_cell(&self, row: u32, col: u32) {
        if self.has_deferred() && !self.replaying.get_value() {
            let controller = *self;
            self.enqueue(move || controller.toggle_cell(row, col));
            return;
        }
        let Some(sheet) = self.sheet.get_untracked() else {
            return;
        };
        let Ok(at) = CellKey::new(sheet, row, col) else {
            return;
        };
        let areas = self.areas.get_untracked();
        let removing = areas.iter().any(|area| area.contains(at));
        let mut areas: Vec<_> = if removing {
            areas
                .into_iter()
                .flat_map(|area| crate::grid::selection::subtract(area, GridArea::cell(at)))
                .collect()
        } else {
            let mut areas = areas;
            areas.push(GridArea::cell(at));
            areas
        };
        if areas.is_empty() {
            areas.push(GridArea::cell(at));
        }
        let active = if areas.iter().any(|area| area.contains(at)) {
            at
        } else {
            areas[0].first()
        };
        self.select_areas(active, areas);
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
        self.copy_pending.update(|n| *n += 1);
        self.take(false, self.copy_epoch.get_value());
    }

    /// Cuts the selected range (Ctrl+X): the next paste moves it there.
    pub fn cut(&self) {
        self.copy_pending.update(|n| *n += 1);
        self.take(true, self.copy_epoch.get_value());
    }

    fn take(&self, cut: bool, epoch: u64) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.take(cut, epoch));
            return;
        }
        if epoch != self.copy_epoch.get_value() {
            return;
        }
        if self.areas.with_untracked(|areas| areas.len() > 1) {
            self.copy_pending.update(|n| *n = n.saturating_sub(1));
            self.say("Select one rectangle for this command", true);
            return;
        }
        if let Some(area) = self.selected_area_untracked() {
            let controller = *self;
            let generation = self.generation.get_untracked();
            self.pending_actions.update(|n| *n += 1);
            let keep = self.copied.get_untracked().map(|c| c.token);
            self.call(
                Request::Capture {
                    area: Area::of(area),
                    keep,
                },
                None,
                move |reply, _| {
                    controller
                        .pending_actions
                        .update(|n| *n = n.saturating_sub(1));
                    if generation != controller.generation.get_untracked()
                        || epoch != controller.copy_epoch.get_value()
                    {
                        return;
                    }
                    match reply {
                        Ok(Reply::Clipboard(payload)) => {
                            controller.clipboard.set(Some(area));
                            controller.cut.set(cut);
                            controller.copied.set(Some(payload.clone().into()));
                            controller.say(
                                format!(
                                    "{} {} — snapshot captured; select where to paste",
                                    if cut { "Cut" } else { "Copied" },
                                    controller.area_label(area)
                                ),
                                false,
                            );
                            wasm_bindgen_futures::spawn_local(async move {
                                if crate::clipboard::write_system(&payload).await.is_err() {
                                    if generation != controller.generation.get_untracked()
                                        || epoch != controller.copy_epoch.get_value()
                                    {
                                        return;
                                    }
                                    controller.clipboard_fallback.set(Some(true));
                                    controller.say("Browser clipboard access was denied. Copy table text in the dialog, or use Paste copied cells.",true);
                                }
                                if generation == controller.generation.get_untracked()
                                    && epoch == controller.copy_epoch.get_value()
                                {
                                    controller.copy_pending.update(|n| *n = n.saturating_sub(1));
                                }
                            });
                        }
                        Err(reason) => {
                            controller.copy_pending.update(|n| *n = n.saturating_sub(1));
                            controller.say(reason, true);
                        }
                        _ => {
                            controller.copy_pending.update(|n| *n = n.saturating_sub(1));
                            controller.say("The clipboard did not answer", true);
                        }
                    }
                },
            );
        } else {
            self.copy_pending.update(|n| *n = n.saturating_sub(1));
        }
    }

    /// A native copy outside the grid supersedes an in-flight system write, while retaining the
    /// explicit copied-cell snapshot. It also cancels a pending cut move.
    pub fn system_clipboard_changed(&self) {
        self.copy_epoch.update_value(|n| *n += 1);
        self.copy_pending.set(0);
        self.clipboard.set(None);
        self.cut.set(false);
    }

    pub fn copy_requested(&self) -> bool {
        self.copy_pending.get() > 0
    }

    fn chosen_paste_mode(&self) -> wire::PasteMode {
        match self.paste_mode.get_untracked().as_str() {
            "values" => wire::PasteMode::Values,
            "formulas" => wire::PasteMode::Formulas,
            "formats" => wire::PasteMode::Formats,
            _ => wire::PasteMode::All,
        }
    }

    /// Paste the captured workbook snapshot explicitly; system paste reads the native event instead.
    pub fn paste(&self) {
        let controller = *self;
        self.after_input(move || {
            if let Some(payload) = controller.copied.get_untracked() {
                controller.prepare_paste(
                    wire::PasteSource::Internal {
                        token: payload.token,
                        cut: controller.cut.get_untracked(),
                    },
                    false,
                );
            } else {
                controller.say("Nothing is copied to paste", true);
            }
        });
    }

    pub fn paste_system(&self) {
        let controller = *self;
        self.after_input(move || {
            if controller.areas.with_untracked(|areas| areas.len() != 1) {
                controller.say("Select one rectangle for this command", true);
                return;
            }
            let Some(to)=controller.selected_area_untracked() else {return;};
            let generation=controller.generation.get_untracked();
            let mode=controller.chosen_paste_mode();
            controller.pending_actions.update(|n|*n+=1);
            controller.pending_changes.update(|n|*n+=1);
            wasm_bindgen_futures::spawn_local(async move {
                let result=crate::clipboard::read_system().await;
                controller.pending_actions.update(|n|*n=n.saturating_sub(1));
                controller.pending_changes.update(|n|*n=n.saturating_sub(1));
                if generation!=controller.generation.get_untracked() {return;}
                match result {
                    Ok((text,html))=>match controller.clipboard_source(&text,&html) {
                        Ok(source)=>controller.prepare_paste_at_mode(source,to,false,mode),
                        Err(reason)=>controller.say(reason,true),
                    },
                    Err(_)=>{controller.clipboard_fallback.set(Some(false));controller.say("Browser clipboard access was denied. Paste table text in the dialog, or use Paste copied cells.",true);}
                }
            });
        });
    }

    fn clipboard_source(&self, text: &str, html: &str) -> Result<wire::PasteSource, String> {
        if text.is_empty() && html.is_empty() {
            return Err("The clipboard contains no text table".to_owned());
        }
        let (rows, token) = if !html.is_empty() {
            match crate::clipboard::decode_html(html) {
                Err(reason) if reason == "The clipboard HTML has no table" && !text.is_empty() => {
                    (crate::clipboard::decode_tsv(text)?, None)
                }
                other => other?,
            }
        } else {
            (crate::clipboard::decode_tsv(text)?, None)
        };
        let internal = self.copied.get_untracked().filter(|p| {
            !p.session.is_empty()
                && Some((p.session.clone(), p.token)) == token
                && (text.is_empty()
                    || crate::clipboard::decode_tsv(&p.text).ok()
                        == crate::clipboard::decode_tsv(text).ok())
        });
        Ok(internal.map_or(wire::PasteSource::External(rows), |p| {
            wire::PasteSource::Internal {
                token: p.token,
                cut: self.cut.get_untracked(),
            }
        }))
    }

    /// Own the native event's data before queueing; replaying Ctrl+V supplies no system data.
    pub fn paste_external(&self, text: String, html: String) {
        let controller = *self;
        if self.copy_pending.get_untracked() > 0 {
            self.after_input(move || controller.paste());
            return;
        }
        self.after_input(move || match controller.clipboard_source(&text, &html) {
            Ok(source) => controller.prepare_paste(source, false),
            Err(reason) => controller.say(reason, true),
        });
    }

    pub fn prepare_paste(&self, source: wire::PasteSource, force_preview: bool) {
        if self.areas.with_untracked(|a| a.len() > 1) {
            self.say("Select one rectangle for this command", true);
            return;
        }
        let Some(to) = self.selected_area_untracked() else {
            return;
        };
        self.prepare_paste_at(source, to, force_preview);
    }

    pub fn prepare_paste_at(&self, source: wire::PasteSource, to: GridArea, force_preview: bool) {
        self.prepare_paste_at_mode(source, to, force_preview, self.chosen_paste_mode());
    }

    fn prepare_paste_at_mode(
        &self,
        source: wire::PasteSource,
        to: GridArea,
        force_preview: bool,
        chosen_mode: wire::PasteMode,
    ) {
        let controller = *self;
        let generation = self.generation.get_untracked();
        let cut = matches!(source, wire::PasteSource::Internal { cut: true, .. });
        let mode = if matches!(source, wire::PasteSource::Fill { .. }) {
            wire::PasteMode::All
        } else {
            chosen_mode
        };
        self.pending_actions.update(|n| *n += 1);
        self.pending_changes.update(|n| *n += 1);
        self.call(
            Request::PreparePaste {
                source,
                to: Area::of(to),
                mode,
            },
            None,
            move |reply, _| {
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                controller
                    .pending_changes
                    .update(|n| *n = n.saturating_sub(1));
                if generation != controller.generation.get_untracked() {
                    return;
                }
                match reply {
                    Ok(Reply::PastePreview(preview)) => {
                        controller.paste_cut.set_value(cut);
                        controller.paste_preview.set(Some(preview.clone()));
                        if preview.replaces == 0 && !force_preview {
                            controller.apply_paste(preview.id);
                        }
                    }
                    Err(reason) => controller.say(reason, true),
                    _ => controller.say("The paste preview did not answer", true),
                }
            },
        );
    }

    pub fn apply_paste(&self, id: u64) {
        self.paste_preview.set(None);
        let controller = *self;
        let cut = self.paste_cut.get_value();
        self.dispatch(Intent::ApplyPaste { id }, move |outcome| {
            if outcome.accepted && cut {
                controller.clipboard.set(None);
                controller.copied.set(None);
                controller.cut.set(false);
            }
            controller.focus_active_skin();
        });
    }

    /// Keyboard equivalent of the handle: the end cell is resolved by the workbook catalogue.
    pub fn fill_through(&self, text: String) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say(
                    "Finish or cancel the current entry before filling cells",
                    true,
                );
                return;
            }
            if controller.areas.with_untracked(|a| a.len() != 1) {
                controller.say("Select one rectangle for this command", true);
                return;
            }
            let Some(from) = controller.selected_area_untracked() else {
                return;
            };
            let generation = controller.generation.get_untracked();
            let series = controller.fill_series.get_untracked() == "series";
            controller.pending_actions.update(|n| *n += 1);
            controller.call(
                Request::ResolveReference {
                    anchor: Key::of(from.first()),
                    text,
                },
                None,
                move |reply, _| {
                    controller
                        .pending_actions
                        .update(|n| *n = n.saturating_sub(1));
                    if generation != controller.generation.get_untracked() {
                        return;
                    }
                    match reply {
                        Ok(Reply::ResolvedReference(Ok(area))) => {
                            let Some(area) = area
                                .area()
                                .filter(|a| a.sheet() == from.sheet() && a.cell_count() == 1)
                            else {
                                controller
                                    .say("Fill through needs one end cell on this worksheet", true);
                                return;
                            };
                            let at = area.first();
                            if !(from.first().row()..=from.last().row()).contains(&at.row())
                                && !(from.first().column()..=from.last().column())
                                    .contains(&at.column())
                            {
                                controller
                                    .say("Choose a fill end in the selected rows or columns", true);
                                return;
                            }
                            if let Some((to, down)) =
                                crate::clipboard::fill_extension(from, at.row(), at.column())
                            {
                                controller.prepare_paste_at(
                                    wire::PasteSource::Fill {
                                        from: Area::of(from),
                                        down,
                                        series,
                                    },
                                    to,
                                    true,
                                );
                            } else {
                                controller
                                    .say("Choose an end cell outside the seed rectangle", true);
                            }
                        }
                        Ok(Reply::ResolvedReference(Err(reason))) | Err(reason) => {
                            controller.say(reason, true)
                        }
                        _ => controller.say("The fill end did not resolve", true),
                    }
                },
            );
        });
    }

    /// Fills the selected range down from its first row (Ctrl+D) or right from its first column
    /// (Ctrl+R): a calc-host copy of that row or column over the rest.
    pub fn fill(&self, down: bool) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.fill(down));
            return;
        }
        if self.areas.with_untracked(|areas| areas.len() > 1) {
            self.say("Select one rectangle for this command", true);
            return;
        }
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

    /// A worksheet command through the same worker/history path as cell commands.
    pub fn sheet_command(
        &self,
        sheet: Option<SheetId>,
        change: wire::SheetChange,
        done: impl FnOnce(bool) + 'static,
    ) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.sheet_command(sheet, change, done));
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
            Intent::EditSheet {
                sheet: sheet.map(Sheet::of),
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
    pub fn grid_view(&self, sheet: SheetId) -> Option<crate::grid::viewport::GridView> {
        self.grid_view_for(sheet, 0)
    }
    pub fn grid_view_for(
        &self,
        sheet: SheetId,
        index: u8,
    ) -> Option<crate::grid::viewport::GridView> {
        self.grid_views
            .with_value(|all| all.get(&(sheet, index)).copied())
    }
    pub fn active_grid_pane(&self, sheet: SheetId) -> Option<u8> {
        self.grid_active.with_value(|all| all.get(&sheet).copied())
    }
    pub fn remember_active_grid_pane(&self, sheet: SheetId, index: u8) {
        self.grid_active.update_value(|all| {
            all.insert(sheet, index);
        });
    }
    pub fn grid_geometry(&self, sheet: SheetId) -> Option<wire::Geometry> {
        self.grid_geometry
            .with_value(|all| all.get(&sheet).cloned())
    }
    pub fn remember_grid_geometry(&self, sheet: SheetId, geometry: wire::Geometry) {
        self.grid_geometry.update_value(|all| {
            all.insert(sheet, geometry);
        });
    }
    pub fn remember_grid_view(&self, sheet: SheetId, view: crate::grid::viewport::GridView) {
        self.remember_grid_view_for(sheet, 0, view);
    }
    pub fn remember_grid_view_for(
        &self,
        sheet: SheetId,
        index: u8,
        mut view: crate::grid::viewport::GridView,
    ) {
        if self.sheet.get_untracked() != Some(sheet) {
            if let Some(active) = self
                .sheet_selections
                .with_value(|all| all.get(&sheet).map(|(at, _)| (at.row(), at.column())))
            {
                view.active = active;
            }
        }
        self.grid_views.update_value(|all| {
            all.insert((sheet, index), view);
        });
    }
    pub fn axis_format(&self, rows: bool, size: Option<String>, hidden: Option<bool>) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say(
                    "Finish or cancel the current entry before changing dimensions",
                    true,
                );
                return;
            }
            let areas = controller.areas.get_untracked();
            if areas.len() != 1 {
                controller.say("Choose one rectangle before changing dimensions", true);
                return;
            }
            let area = areas[0];
            let (first, last) = if rows {
                (area.first().row(), area.last().row())
            } else {
                (area.first().column(), area.last().column())
            };
            let size_bits = match size {
                Some(text) => match text.trim().parse::<f64>() {
                    Ok(n) => Some(n.to_bits()),
                    Err(_) => {
                        controller.say("Enter a numeric size", true);
                        return;
                    }
                },
                None => None,
            };
            controller.send_axis_format(
                area.sheet(),
                rows,
                first,
                last - first + 1,
                size_bits,
                hidden,
            );
        });
    }

    fn send_axis_format(
        &self,
        sheet: SheetId,
        rows: bool,
        first: u32,
        count: u32,
        size_bits: Option<u64>,
        hidden: Option<bool>,
    ) {
        let controller = *self;
        self.dispatch(
            Intent::AxisFormat {
                sheet: wire::Sheet::of(sheet),
                rows,
                first,
                count,
                size_bits,
                hidden,
            },
            move |outcome| {
                if outcome.accepted {
                    controller.say(
                        if hidden == Some(true)
                            || size_bits.is_some_and(|b| f64::from_bits(b) == 0.0)
                        {
                            if rows {
                                "Rows hidden"
                            } else {
                                "Columns hidden"
                            }
                        } else if hidden == Some(false) {
                            if rows { "Rows shown" } else { "Columns shown" }
                        } else {
                            if rows {
                                "Row heights updated"
                            } else {
                                "Column widths updated"
                            }
                        },
                        false,
                    );
                }
            },
        );
    }
    pub fn resize_axis(&self, drag: crate::grid::axis::ResizeDrag) {
        let controller = *self;
        self.after_input(move || {
            if controller.generation.get_untracked() != drag.generation
                || controller.sheet.get_untracked() != Some(drag.sheet)
                || controller.editing.get_untracked().is_some()
            {
                return;
            }
            let areas = controller.areas.get_untracked();
            if areas.len() != 1 {
                controller.say("Choose one rectangle before resizing", true);
                return;
            }
            let (first, count) =
                crate::grid::axis::selected_bands(&areas, drag.band.rows, drag.band.index);
            controller.send_axis_format(
                drag.sheet,
                drag.band.rows,
                first,
                count,
                Some(drag.input().to_bits()),
                None,
            );
        });
    }

    pub fn autofit(&self, rows: bool) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say("Finish or cancel the entry before fitting contents", true);
                return;
            }
            let areas = controller.areas.get_untracked();
            if areas.len() != 1 {
                controller.say("Choose one rectangle before fitting contents", true);
                return;
            }
            controller.fit_contents(rows, areas[0]);
        });
    }
    pub fn autofit_header(&self, band: crate::grid::axis::BandEdge) {
        let controller = *self;
        self.after_input(move || {
            let Some(sheet) = controller.sheet.get_untracked() else {
                return;
            };
            if controller.editing.get_untracked().is_some() {
                return;
            }
            let (first, count) = crate::grid::axis::selected_bands(
                &controller.areas.get_untracked(),
                band.rows,
                band.index,
            );
            let last = first + count - 1;
            let (top, left, bottom, right) = if band.rows {
                (first, 1, last, 16384)
            } else {
                (1, first, 1048576, last)
            };
            let area = GridArea::new(
                CellKey::new(sheet, top, left).unwrap(),
                CellKey::new(sheet, bottom, right).unwrap(),
            )
            .unwrap();
            controller.fit_contents(band.rows, area);
        });
    }
    fn fit_contents(&self, rows: bool, area: GridArea) {
        let controller = *self;
        let generation = self.generation.get_untracked();
        self.pending_actions.update(|n| *n += 1);
        self.call(
            Request::FitContents {
                area: Area::of(area),
            },
            None,
            move |reply, _| {
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                if controller.generation.get_untracked() != generation
                    || controller.sheet.get_untracked() != Some(area.sheet())
                {
                    return;
                }
                match reply {
                    Ok(Reply::FitContents { revision, cells }) => {
                        match crate::grid::autofit::measure(rows, &cells) {
                            Ok(sizes) if sizes.is_empty() => {
                                controller.say("No visible nonempty contents to fit", false)
                            }
                            Ok(sizes) => controller.dispatch(
                                Intent::FitContents {
                                    sheet: Sheet::of(area.sheet()),
                                    rows,
                                    revision,
                                    sizes,
                                },
                                move |outcome| {
                                    if outcome.accepted {
                                        controller.say("Fitted contents within size limits", false);
                                    }
                                },
                            ),
                            Err(error) => controller.say(error, true),
                        }
                    }
                    Err(error) => controller.say(error, true),
                    _ => controller.say("The workbook did not return contents to fit", true),
                }
            },
        );
    }

    pub fn freeze_panes(&self, choice: &'static str) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say(
                    "Finish or cancel the current entry before changing panes",
                    true,
                );
                return;
            }
            let Some(sheet) = controller.sheet.get_untracked() else {
                return;
            };
            if choice != "none" {
                let offset = controller.grid_geometry(sheet).is_some_and(|g| {
                    (g.frozen_rows > 0 || g.frozen_columns > 0)
                        && g.top_left.is_some_and(|at| at != (1, 1))
                });
                if offset {
                    controller.say(
                        "Unfreeze this imported view before choosing new frozen bands",
                        true,
                    );
                    return;
                }
                if choice == "active"
                    && controller
                        .grid_view_for(sheet, controller.active_grid_pane(sheet).unwrap_or(0))
                        .is_some_and(|v| v.viewport.scroll_x > 0.5 || v.viewport.scroll_y > 0.5)
                {
                    controller.say(
                        "Return to the top-left of the sheet before freezing at the active cell",
                        true,
                    );
                    return;
                }
            }
            let (row, col) = controller.active.get_untracked();
            let (rows, columns) = match choice {
                "first-row" => (1, 0),
                "first-column" => (0, 1),
                "active" => (row - 1, col - 1),
                _ => (0, 0),
            };
            controller.dispatch(
                Intent::FreezePanes {
                    sheet: wire::Sheet::of(sheet),
                    rows,
                    columns,
                },
                move |outcome| {
                    if outcome.accepted {
                        controller.say(
                            if rows == 0 && columns == 0 {
                                "Panes unfrozen".to_owned()
                            } else {
                                format!("Frozen: {rows} rows, {columns} columns")
                            },
                            false,
                        );
                    }
                },
            );
        });
    }

    /// Makes `at` the active cell, showing its sheet (a skin's jump to a cell: the tree's, the
    /// canvas's, the ledger's).
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
            self.enqueue(move || controller.navigate_to_with_ticket(at, arrived, ticket));
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
            self.enqueue(move || controller.go_to_text_with_ticket(text, arrived, ticket));
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
    pub fn begin_edit(&self, origin: EditOrigin, seed: Option<String>) {
        if self.editing.get_untracked().is_some() {
            if self.editing.get_untracked() != Some(origin) {
                self.model.maybe_update(|model| model.edit.transfer(origin));
            }
            return;
        }
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.begin_edit(origin, seed));
            return;
        }
        if self.active_key_untracked().is_none() {
            self.say("Wait for the workbook before editing", true);
            return;
        }
        let original = self
            .active_key_untracked()
            .and_then(|at| {
                self.selection
                    .with_untracked(|mirror| mirror.cell(at).map(|cell| cell.input.clone()))
            })
            .unwrap_or_default();
        let target = self.active_key_untracked().expect("checked active cell");
        let areas = self.areas.get_untracked();
        self.model
            .update(|m| m.edit.begin(origin, target, original.clone(), seed, areas));
        self.assist.start_edit(original);
    }

    pub fn activate_editor(&self, origin: EditOrigin, assist: FormulaAssist) {
        self.bind_editor_surface(origin, assist);
        self.begin_edit(origin, None);
        self.edit_field.set_value(assist);
    }

    /// Keep the draft and its target when its cell overlay is about to unmount.
    pub fn carry_edit_to_bar(&self) {
        if self.editing.get_untracked().is_some() {
            self.model
                .maybe_update(|model| model.edit.transfer(EditOrigin::Bar));
            self.edit_field.set_value(self.assist);
            #[cfg(target_arch = "wasm32")]
            {
                let controller = *self;
                let generation = self.generation.get_untracked();
                let session = self.model.with_untracked(|model| model.edit.id);
                leptos::prelude::set_timeout(
                    move || {
                        if controller.generation.try_get_untracked() != Some(generation)
                            || controller.editing.try_get_untracked().flatten()
                                != Some(EditOrigin::Bar)
                            || controller.model.with_untracked(|model| model.edit.id) != session
                        {
                            return;
                        }
                        controller.focus_mounted_editor(controller.assist);
                    },
                    std::time::Duration::ZERO,
                );
            }
        }
    }

    pub fn focus_editor(&self) {
        if self
            .captured_command_focus()
            .is_some_and(|ticket| !self.model.with_untracked(|m| m.focus.accepts(ticket)))
        {
            return;
        }
        let assist = self.edit_field.get_value();
        if let Some(field) = assist.field.try_get_untracked().flatten() {
            #[cfg(target_arch = "wasm32")]
            self.restoring_editor(|| {
                let _ = field.focus();
            });
            #[cfg(not(target_arch = "wasm32"))]
            let _ = field.focus();
        }
    }

    /// Both editors use this route after their shared completion and local undo handlers.
    pub fn editor_key(&self, ev: leptos::ev::KeyboardEvent, assist: FormulaAssist) {
        if assist.composing.get_untracked() || ev.is_composing() || ev.key_code() == 229 {
            return;
        }
        let ctrl = ev.ctrl_key() || ev.meta_key();
        if ctrl && !ev.shift_key() && !ev.alt_key() && ev.key() == "Enter" {
            ev.prevent_default();
            ev.stop_propagation();
            let controller = *self;
            self.commit_edit_as(true, move |accepted| {
                if accepted {
                    controller.focus_active_skin();
                } else {
                    controller.focus_editor();
                }
            });
            return;
        }
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
                self.focus_active_skin();
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
        self.remember_editor_selection();
    }

    pub fn edit_label(&self) -> String {
        self.edit_target
            .get()
            .map(|at| self.cell_label(at))
            .unwrap_or_default()
    }

    /// Syntax and DOM reads feed the model's one pointed-edit transition.
    pub fn point_at(&self, row: u32, column: u32, extend: bool) -> bool {
        if !self.model.with_untracked(Model::can_point) {
            return false;
        }
        let assist = self.edit_field.get_value();
        let text = self.edit_text.get_untracked();
        let (start, end) = field_selection(assist);
        let Some(sheet) = self.sheet.get_untracked() else {
            return false;
        };
        let Ok(at) = CellKey::new(sheet, row, column) else {
            return false;
        };
        let syntax = if expects_reference(&text, start, end) {
            Some(dna_calc_model::PointSyntax::Insert { start, end })
        } else if self.point_mode.get_untracked() {
            point_reference_span(&text, start, end).map(|span| {
                dna_calc_model::PointSyntax::ReplaceReference {
                    start: span.start,
                    end: span.end(),
                }
            })
        } else {
            None
        };
        let next = self
            .model
            .try_update(|model| {
                model.point_at(
                    at,
                    extend,
                    dna_calc_model::TextSelection {
                        start,
                        end,
                        backward: false,
                    },
                    syntax,
                )
            })
            .flatten();
        let Some(next) = next else {
            return false;
        };
        assist.reset();
        assist.reflect(&next.text, next.caret);
        true
    }

    pub fn finish_edit(&self, movement: NavAction) {
        let controller = *self;
        self.commit_edit(move |accepted| {
            if accepted {
                let horizontal = matches!(
                    movement,
                    NavAction::CommitRight | NavAction::NextColumn | NavAction::PrevColumn
                );
                let reverse = matches!(movement, NavAction::Up | NavAction::PrevColumn);
                controller.navigate(
                    crate::grid::selection::Navigation::Walk {
                        horizontal,
                        reverse,
                    },
                    false,
                    0,
                );
                controller.focus_active_skin();
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
            self.model.update(|model| model.edit.finish());
            self.assist.reset();
            if let Some(at) = target {
                self.switch_sheet(at.sheet());
                self.select_areas(at, self.edit_areas.get_value());
            }
            self.say("Edit cancelled", false);
        }
    }

    /// Enters the edited text into the active cell. `then` hears whether the entry was
    /// accepted; a refused entry keeps the edit open with the reason on the status line.
    pub fn commit_edit(&self, then: impl FnOnce(bool) + 'static) {
        self.commit_edit_as(false, then);
    }

    fn commit_edit_as(&self, fill_selection: bool, then: impl FnOnce(bool) + 'static) {
        if self.assist.readonly.get_untracked() {
            return;
        }
        let Some(at) = self.edit_target.get_untracked() else {
            then(false);
            return;
        };
        let text = self.edit_text.get_untracked();
        if self.active_key_untracked() != Some(at)
            || self.areas.get_untracked() != self.edit_areas.get_value()
        {
            self.switch_sheet(at.sheet());
            self.select_areas(at, self.edit_areas.get_value());
        }
        if !fill_selection && text == self.edit_original.get_value() {
            self.model.update(|model| model.edit.finish());
            self.assist.reset();
            then(true);
            return;
        }
        self.assist.readonly.set(true);
        let controller = *self;
        self.dispatch_result(
            if fill_selection {
                Intent::EnterSelection { text }
            } else {
                Intent::EnterCell {
                    at: Key::of(at),
                    text,
                }
            },
            move |result| {
                controller.assist.readonly.set(false);
                let accepted = result.is_ok_and(|outcome| outcome.accepted);
                if accepted {
                    controller.model.update(|model| model.edit.finish());
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

    pub fn structure_command(&self, insert: bool, mode: &str) {
        if self.editing.get_untracked().is_some() {
            self.say(
                "Enter or cancel the current edit before inserting or deleting cells",
                true,
            );
            return;
        }
        let Some(area) = self.selected_area_untracked() else {
            return;
        };
        if self.areas.with_untracked(|a| a.len()) != 1 {
            self.say("Select one rectangle", true);
            return;
        }
        let intent = match mode {
            "down" | "right" => Intent::ShiftCells {
                area: Area::of(area),
                down: mode == "down",
                insert,
            },
            "rows" | "columns" => Intent::EditAxis {
                sheet: Sheet::of(area.sheet()),
                rows: mode == "rows",
                insert,
                start: if mode == "rows" {
                    area.first().row()
                } else {
                    area.first().column()
                },
                count: if mode == "rows" {
                    area.row_count()
                } else {
                    area.column_count()
                },
            },
            _ => return,
        };
        let controller = *self;
        self.dispatch_result(intent, move |result| {
            if result.is_ok_and(|outcome| outcome.accepted) {
                controller.structural_choice.set(None);
                controller.focus_active_skin();
            }
        });
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
                Intent::Clear {
                    areas: self
                        .areas
                        .get_untracked()
                        .into_iter()
                        .map(Area::of)
                        .collect(),
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

    pub fn new_mirror(&self) -> MirrorHandle {
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
        field
    }

    /// Registers a subscription whose cells go to `mirror`; its number for later changes.
    pub fn subscribe(&self, interest: wire::Interest, mirror: MirrorHandle) -> u64 {
        let id = self.next_subscription.get_value();
        self.next_subscription.set_value(id + 1);
        mirror.set(ViewMirror::default().into());
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
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.save(saved));
            return;
        }
        if let Some(reason) = self.model.with_untracked(|m| m.views.pending_reason()) {
            self.say(reason, true);
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
        self.send_command(Request::Save, None, move |reply, bytes| {
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
                    controller.summary.set(outcome.summary.into());
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
                    self.dispatch(if redo { Intent::Redo } else { Intent::Undo }, |_| {});
                }
            }
            Ok(CommandEffect::Recalculate) => self.dispatch(Intent::Recalculate, |_| {}),
            Ok(CommandEffect::Dialog) => self.focus_command_dialog(),
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

    pub fn reveal(&self, at: CellKey, skin: SkinId) {
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
                geometry_version: 0,
                style_version: 0,
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
        });
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calculation_completion_preserves_a_failed_projection_message() {
        let root = Owner::new();
        root.with(|| {
            let controller = Controller::with_transport(std::rc::Rc::new(
                dna_calc_worker::transport::InProcessTransport::default(),
            ));
            let summary = controller.summary.get_untracked();
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
                failed_projections: vec![11],
            });
            assert!(controller.status.get_untracked().refused);
            assert_eq!(
                controller.status.get_untracked().text,
                "The displayed area exceeds its tile budget"
            );
        });
    }
}

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
type SaveDownload = Box<dyn FnOnce(String, Vec<u8>)>;

type ValidationCallback = Box<dyn FnOnce(Result<Outcome, String>)>;

#[derive(Clone, Copy)]
struct ReviewFields {
    edit:ModelField<Option<dna_calc_model::ReviewDraft>>,
    circles:ModelField<std::collections::BTreeSet<SheetId>>,
    scan:ModelField<Option<Result<wire::ValidationScan,String>>>,
    epoch:ModelField<u64>,
}

#[derive(Clone, Copy)]
pub struct Controller {
    model: RwSignal<Model>,
    review:StoredValue<ReviewFields>,
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
    pub fn review_edit(&self)->ModelField<Option<dna_calc_model::ReviewDraft>> {self.review.get_value().edit}
    pub fn begin_review_create(&self,at:wire::Key,revision:u64,index:u32,threaded:bool,parent:Option<u32>){
        let timestamp=js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default();
        self.model.update(|m|m.runtime.begin_review_create(at,revision,index,threaded,parent,timestamp));
    }
    pub fn begin_review_edit(&self, at:wire::Key, revision:u64, index:u32, text:String, threaded:bool) {
        self.model.update(|m|m.runtime.begin_review_edit(at,revision,index,text,threaded));
    }
    pub fn validation_circles(&self)->ModelField<std::collections::BTreeSet<SheetId>> {self.review.get_value().circles}
    pub fn validation_scan(&self)->ModelField<Option<Result<wire::ValidationScan,String>>> {self.review.get_value().scan}
    pub fn validation_epoch(&self)->ModelField<u64> {self.review.get_value().epoch}

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
        let review=StoredValue::new(ReviewFields {
            edit:Field::new(model,|m|&m.runtime.review_edit,|m|&mut m.runtime.review_edit),
            circles:Field::new(model,|m|&m.runtime.validation_circles,|m|&mut m.runtime.validation_circles),
            scan:Field::new(model,|m|&m.runtime.validation_scan,|m|&mut m.runtime.validation_scan),
            epoch:Field::new(model,|m|&m.runtime.validation_epoch,|m|&mut m.runtime.validation_epoch),
        });
        let controller = Controller {
            model,
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
            saving: Field::new(model,|m|&m.runtime.saving,|m|&mut m.runtime.saving),
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
        service.start();
        service.set_events(std::rc::Rc::new(move |event| controller.receive(event)));
        service.set_health(std::rc::Rc::new(move |health| {
            controller.transport.try_set(health);
        }));
        controller.ask_result(Request::Summary, move |reply| match reply {
            Ok(Reply::Summary(summary)) => {
                controller.summary.set(summary.into());
                if let Some((name,bytes))=crate::files::default_template("Book") {controller.open_template(&name,bytes,move |accepted|if !accepted{controller.new_workbook();});}
                else {controller.new_workbook();}
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
    pub fn all_sheets(&self)->Vec<SheetTab>{self.summary.with(|summary|summary.all_sheets())}

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
        if let Event::WorkbookEventFailed { workbook, event, reason, omitted, .. } = &event {
            if self.summary.with_untracked(|current| current.workbook) == *workbook {
                let omitted = if *omitted == 0 { String::new() } else { format!("; {omitted} earlier handler failures") };
                self.say(format!("{event}: {reason}{omitted}"), true);
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
                wire::CalculationEvent::Admitted { .. } | wire::CalculationEvent::Published(_)
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
            Event::Calculation { .. } | Event::SaveReady { .. } | Event::WorkbookEventFailed { .. } => {
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
            Event::Calculation { .. } | Event::SaveReady { .. } | Event::WorkbookEventFailed { .. } => {
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
        self.grid_geometry.update_value(|all| all.clear());
        self.mirrors.update_value(BTreeMap::clear);
        let first = summary.initial_cell.and_then(Key::cell).map(|at|at.sheet()).filter(|id|summary.sheets.iter().any(|(s,_)|s.id()==*id)).or_else(||summary.sheets.first().map(|(sheet,_)|sheet.id()));
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
        self.open_mode(name,bytes,false,opened);
    }
    pub fn open_mode(&self,name:&str,bytes:Vec<u8>,edit_template:bool,opened:impl FnOnce(bool)+'static) {self.open_kind(name,bytes,edit_template,false,opened);}
    pub fn open_template(&self,name:&str,bytes:Vec<u8>,opened:impl FnOnce(bool)+'static) {self.open_kind(name,bytes,false,true,opened);}
    pub fn edit_personal_template(&self,name:String,bytes:Vec<u8>) {
        let controller=*self;let owned=name.clone();
        self.open_mode(&name,bytes,true,move |accepted|if accepted{controller.personal_template.set_value(Some(owned));});
    }
    fn open_kind(&self,name:&str,bytes:Vec<u8>,edit_template:bool,template_only:bool,opened:impl FnOnce(bool)+'static) {
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
            if template_only {Request::OpenTemplate{name:name.clone()}} else if edit_template { Request::OpenForEditing {name:name.clone()} } else {Request::Open { name: name.clone() }},
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
                            None if outcome.summary.manual => format!(
                                "Opened {name}: {size} bytes in {elapsed:.0} ms; the file calculates manually, so its formulas show the values it stored until Calculate"
                            ),
                            None => format!("Opened {name}: {size} bytes in {elapsed:.0} ms; calculation continues in the worker"),
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
            self.enqueue_admitted(move |accepted| {
                if accepted {
                    controller.take(cut, epoch);
                } else {
                    controller.copy_pending.update(|n| *n = n.saturating_sub(1));
                }
            });
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

    // Construct this small adapter on demand. Keeping it out of the copied Controller avoids
    // multiplying its size through every retained view closure on the wasm stack.
    pub fn paste_transpose(&self) -> ModelField<bool> {
        Field::new(self.model, |m| &m.runtime.paste_transpose, |m| &mut m.runtime.paste_transpose)
    }

    fn chosen_paste_mode(&self) -> wire::PasteMode {
        match self.paste_mode.get_untracked().as_str() {
            "values" => wire::PasteMode::Values,
            "formulas" => wire::PasteMode::Formulas,
            "formats" => wire::PasteMode::Formats,
            _ => wire::PasteMode::All,
        }
    }

    /// Move or copy a rectangle through the same captured, revision-bound paste as the clipboard.
    /// A drag owns no system clipboard data and leaves an existing copied snapshot available.
    pub fn drag_range(&self, from: GridArea, to: GridArea, copy: bool, generation: u64, revision: u64) {
        let controller = *self;
        self.after_input(move || {
            if generation != controller.generation.get_untracked()
                || revision != controller.summary.get_untracked().revision
                || controller.selected_area_untracked() != Some(from)
                || controller.editing.get_untracked().is_some()
            {
                controller.say("The workbook or selection changed during the drag; drag again", true);
                return;
            }
            if from == to { return; }
            controller.pending_actions.update(|n| *n += 1);
            controller.pending_changes.update(|n| *n += 1);
            let keep = controller.copied.get_untracked().map(|c| c.token);
            let epoch = controller.selection_epoch.get_value();
            controller.call(Request::Capture { area: Area::of(from), keep }, None, move |reply, _| {
                controller.pending_actions.update(|n| *n = n.saturating_sub(1));
                controller.pending_changes.update(|n| *n = n.saturating_sub(1));
                if generation != controller.generation.get_untracked() { return; }
                if epoch != controller.selection_epoch.get_value() || controller.editing.get_untracked().is_some() {
                    controller.say("The selection changed during the drag; drag again", true);
                    return;
                }
                if revision != controller.summary.get_untracked().revision {
                    controller.say("The workbook changed during the drag; drag again", true);
                    return;
                }
                match reply {
                    Ok(Reply::Clipboard(payload)) if payload.revision == revision => {
                        controller.prepare_paste_at_mode(
                            wire::PasteSource::Internal { token: payload.token, cut: !copy },
                            to, false, wire::PasteMode::All, false, true,
                        );
                    }
                    Ok(Reply::Clipboard(_)) => controller.say("The drag source changed; drag again", true),
                    Err(reason) => controller.say(reason, true),
                    _ => controller.say("The drag snapshot did not answer", true),
                }
            });
        });
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
            let transpose=controller.model.with_untracked(|m| m.runtime.paste_transpose);
            controller.pending_actions.update(|n|*n+=1);
            controller.pending_changes.update(|n|*n+=1);
            wasm_bindgen_futures::spawn_local(async move {
                let result=crate::clipboard::read_system().await;
                controller.pending_actions.update(|n|*n=n.saturating_sub(1));
                controller.pending_changes.update(|n|*n=n.saturating_sub(1));
                if generation!=controller.generation.get_untracked() {return;}
                match result {
                    Ok((text,html))=>match controller.clipboard_source(&text,&html) {
                        Ok(source)=>controller.prepare_paste_at_mode(source,to,false,mode,transpose,false),
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
        self.prepare_paste_at_mode(source, to, force_preview, self.chosen_paste_mode(), self.model.with_untracked(|m| m.runtime.paste_transpose), false);
    }

    fn prepare_paste_at_mode(
        &self,
        source: wire::PasteSource,
        to: GridArea,
        force_preview: bool,
        chosen_mode: wire::PasteMode,
        transpose: bool,
        drag: bool,
    ) {
        let controller = *self;
        let generation = self.generation.get_untracked();
        let selection_epoch = self.selection_epoch.get_value();
        let cut = matches!(source, wire::PasteSource::Internal { cut: true, .. });
        let mode = if matches!(source, wire::PasteSource::Fill { .. }) {
            wire::PasteMode::All
        } else {
            if matches!(source,wire::PasteSource::FlashFill){wire::PasteMode::Values}else{chosen_mode}
        };
        self.pending_actions.update(|n| *n += 1);
        self.pending_changes.update(|n| *n += 1);
        self.call(
            Request::PreparePaste {
                transpose: transpose && !matches!(source, wire::PasteSource::Fill { .. }|wire::PasteSource::FlashFill),
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
                if drag && (selection_epoch != controller.selection_epoch.get_value() || controller.editing.get_untracked().is_some()) {
                    controller.say("The selection changed during the drag; drag again", true);
                    return;
                }
                match reply {
                    Ok(Reply::PastePreview(preview)) => {
                        controller.paste_cut.set_value(cut && !drag);
                        controller.model.update(|m| m.runtime.paste_drag = drag);
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
        let select_target = self.model.with_untracked(|m| m.runtime.paste_drag);
        self.paste_preview.set(None);
        let controller = *self;
        let cut = self.paste_cut.get_value();
        self.dispatch(Intent::ApplyPaste { id, select_target }, move |outcome| {
            if outcome.accepted && cut {
                controller.clipboard.set(None);
                controller.copied.set(None);
                controller.cut.set(false);
            }
            controller.focus_active_skin();
        });
    }

    /// Infer basic text extraction from examples, then use the ordinary paste preview.
    pub fn flash_fill(&self){let controller=*self;self.after_input(move ||{
        if controller.editing.get_untracked().is_some(){controller.say("Finish or cancel the current entry before Flash Fill",true);return;}
        if controller.areas.with_untracked(|areas|areas.len()!=1){controller.say("Select one output column for Flash Fill",true);return;}
        if let Some(to)=controller.selected_area_untracked(){controller.prepare_paste_at(wire::PasteSource::FlashFill,to,true);}
    });}

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

    /// Validate personal-template package content without changing the current workbook.
    pub fn inspect_template(&self,bytes:Vec<u8>,done:impl FnOnce(Result<wire::FileFormat,String>)+'static) {
        self.call(Request::InspectTemplate,Some(bytes),move |reply,_|done(match reply{Ok(Reply::TemplateFormat(f))=>Ok(f),Err(e)=>Err(e),_=>Err("Template inspection did not answer".into())}));
    }
    pub fn insert_template(&self,bytes:Vec<u8>,name:Option<String>,done:impl FnOnce(bool)+'static) {
        if self.waiting_input(){let controller=*self;self.enqueue_admitted(move |accepted|if accepted{controller.insert_template(bytes,name,done)}else{done(false)});return;}
        if self.editing.get_untracked().is_some(){self.say("Enter or cancel the current edit before inserting worksheets",true);done(false);return;}
        let controller=*self;
        let before:Vec<_>=self.sheets().into_iter().map(|s|s.id).collect();
        let revision=self.summary.get_untracked().revision;
        let epoch=self.selection_epoch.get_value();
        self.pending_changes.update(|n|*n+=1);self.pending_actions.update(|n|*n+=1);
        self.send_command(Request::InsertTemplate{name,revision},Some(bytes),move |reply,_|{
            controller.pending_changes.update(|n|*n=n.saturating_sub(1));controller.pending_actions.update(|n|*n=n.saturating_sub(1));
            match reply {
                Ok(Reply::Done{outcome,..})=>{
                    controller.applied(&outcome,epoch==controller.selection_epoch.get_value());
                    let accepted=outcome.accepted;
                    controller.perform(move ||{
                        if accepted{if let Some(sheet)=controller.sheets().into_iter().find(|s|!before.contains(&s.id)){controller.show_sheet(sheet.id);}controller.say("Inserted template worksheets",false);}
                        done(accepted);
                    });
                }
                Err(e)=>{controller.say(e,true);controller.perform(move ||done(false));},_=>{controller.say("Template insertion did not answer",true);controller.perform(move ||done(false));}
            }
        });
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
    pub fn outline_geometry(&self) -> Option<wire::Geometry> {
        self.grid_geometry.track();
        self.sheet.get().and_then(|sheet|self.grid_geometry(sheet))
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
    pub fn outline(&self, rows:bool, change:wire::OutlineChange) {
        let controller=*self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {controller.say("Finish or cancel the current entry before outlining",true);return;}
            let Some(sheet)=controller.sheet.get_untracked() else{return;};
            controller.dispatch(Intent::Outline {sheet:wire::Sheet::of(sheet),rows,revision:controller.summary.get_untracked().revision,change},move |outcome|{if outcome.accepted {controller.say("Outline updated",false);}});
        });
    }
    pub fn outline_selection(&self, rows:bool, action:u8) {
        let controller=*self;
        self.after_input(move || {
            let areas=controller.areas.get_untracked();
            if areas.len()!=1 {controller.say("Choose one rectangle before outlining",true);return;}
            let area=areas[0];
            let (first,last)=if rows {(area.first().row(),area.last().row())}else{(area.first().column(),area.last().column())};
            let change=match action {
                0=>wire::OutlineChange::Group{first,last},1=>wire::OutlineChange::Ungroup{first,last},
                _=>wire::OutlineChange::Detail{first,last,level:None,show:action==3},
            };
            controller.outline(rows,change);
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


    pub fn table_view(&self)->Option<wire::TableView>{let at=self.active_key()?;self.grid_geometry.with(|all|all.get(&at.sheet()).and_then(|g|g.tables.iter().find(|t|t.area.area().is_some_and(|a|a.contains(at))).cloned()))}
    pub fn filter_view(&self)->Option<wire::FilterView>{if let Some(table)=self.table_view(){return table.filter;}let sheet=self.sheet.get()?;self.grid_geometry.with(|all|all.get(&sheet).and_then(|g|g.auto_filter.clone()))}
    pub fn filter_command(&self,change:wire::FilterChange) {self.filter_command_revision(change,None);}
    fn filter_command_revision(&self,change:wire::FilterChange,expected:Option<u64>) {
        let controller=*self;
        let generation=self.generation.get_untracked(); let target=self.sheet.get_untracked();let table=self.table_view().map(|t|t.part);
        self.after_input(move || {
            if controller.generation.get_untracked()!=generation || controller.sheet.get_untracked()!=target || controller.table_view().map(|t|t.part)!=table {controller.say("The filter target changed; reopen it",true);return;}
            if controller.editing.get_untracked().is_some(){controller.say("Finish the entry before filtering",true);return;}
            let Some(sheet)=controller.sheet.get_untracked()else{return;};
            let revision=controller.summary.get_untracked().revision;
            if expected.is_some_and(|r|r!=revision){controller.say("The filter draft is out of date; load its values again",true);return;}
            controller.dispatch(Intent::AutoFilter{sheet:Sheet::of(sheet),revision,table,change},move |outcome|{
                if outcome.accepted {
                    controller.model.update(|m|{m.runtime.tools.filter.loaded=false;m.runtime.tools.filter.epoch+=1;});
                    controller.say("AutoFilter updated",false);
                }
            });
        });
    }
    pub fn enable_filter(&self) {
        let areas=self.areas.get_untracked();
        let [area]=areas.as_slice()else{self.say("Select one rectangle including its header",true);return;};
        self.filter_command(wire::FilterChange::Enable(Area::of(*area)));
    }
    pub fn open_filter(&self,column:u32) {
        let Some(sheet)=self.sheet.get_untracked()else{return;};
        let Some(filter)=self.filter_view()else{return;};
        let Some(area)=filter.area.area()else{return;};
        self.model.update(|m|{let f=&mut m.runtime.tools.filter;f.column=(column-area.first().column()+1).to_string();f.open=true;});
        self.load_filter_values();
    }
    pub fn load_filter_values(&self) {
        let controller=*self;
        self.after_input(move || {
            let Some(sheet)=controller.sheet.get_untracked()else{return;};
            let Some(filter)=controller.filter_view()else{return;};
            let Some(area)=filter.area.area()else{return;};
            let relative=controller.model.with_untracked(|m|m.runtime.tools.filter.column.parse::<u32>());
            let Ok(relative)=relative else{controller.say("Enter a column number within the filtered range",true);return;};
            if relative==0 || relative>area.column_count(){controller.say("The filter column must be within its range",true);return;}
            let column=area.first().column()+relative-1;
            let generation=controller.generation.get_untracked();
            let epoch=controller.model.try_update(|m|{let f=&mut m.runtime.tools.filter;f.epoch+=1;f.loaded=false;f.message="Loading values…".into();f.epoch}).unwrap();
            let table=filter.table.clone();
            controller.call(Request::FilterValues{sheet:Sheet::of(sheet),table:table.clone(),column},None,move |reply,_|{
                if controller.generation.get_untracked()!=generation || controller.sheet.get_untracked()!=Some(sheet) || controller.filter_view().and_then(|f|f.table)!=table || controller.model.with_untracked(|m|m.runtime.tools.filter.epoch!=epoch || m.runtime.tools.filter.column.parse::<u32>().ok()!=Some(relative)){return;}
                controller.model.update(|m|{
                    let f=&mut m.runtime.tools.filter;
                    match reply {
                        Ok(Reply::FilterValues{revision,values:Ok((values,blank)),criteria}) if revision==m.runtime.summary.revision=>{
                            f.table=table.clone();f.sheet=Some(Sheet::of(sheet));f.revision=revision;
                            f.values=values;
                            f.blank=blank;
                            f.operator="equal".into(); f.value.clear(); f.operator2.clear(); f.value2.clear(); f.and=true;
                            if let Some(wire::FilterChange::Custom{and,predicates,..})=criteria {f.and=and;if let Some((op,v))=predicates.first(){f.operator=op.clone();f.value=v.clone();}if let Some((op,v))=predicates.get(1){f.operator2=op.clone();f.value2=v.clone();}}
                            f.loaded=true;f.search.clear();f.message="Choose values, then Apply. All values in the range are listed.".into();
                        },
                        Ok(Reply::FilterValues{values:Err(e),..})|Err(e)=>f.message=e,
                        _=>f.message="The workbook changed; load its values again".into(),
                    }
                });
            });
        });
    }
    pub fn apply_filter(&self,custom:bool) {
        let Some(sheet)=self.sheet.get_untracked()else{return;};
        let Some(filter)=self.filter_view()else{return;};
        let Some(area)=filter.area.area()else{return;};
        let f=self.model.with_untracked(|m|m.runtime.tools.filter.clone());
        let Some(column)=f.column.parse::<u32>().ok().filter(|c|*c>0 && *c<=area.column_count()).map(|c|area.first().column()+c-1)else{self.say("Choose a column within the filter range",true);return;};
        let change=if custom {
            let mut predicates=vec![(f.operator,f.value)];
            if !f.operator2.is_empty(){predicates.push((f.operator2,f.value2));}
            wire::FilterChange::Custom{column,and:f.and,predicates}
        }else{
            if !f.loaded || f.table!=filter.table || f.sheet!=Some(Sheet::of(sheet)) || f.revision!=self.summary.get_untracked().revision {self.say("The filter list is out of date; load its values again",true);return;}
            wire::FilterChange::Values{column,values:f.values.into_iter().filter(|(_,checked)|*checked).map(|(v,_)|v).collect(),blank:f.blank}
        };
        self.filter_command_revision(change,Some(self.summary.get_untracked().revision));
    }
    pub fn color_filter(&self,cell_color:bool,color:String) {
        let Some(sheet)=self.sheet.get_untracked()else{return;};
        let Some(area)=self.filter_view().and_then(|f|f.area.area())else{return;};
        if let Some(column)=self.model.with_untracked(|m|m.runtime.tools.filter.column.parse::<u32>().ok()).filter(|c|*c>0 && *c<=area.column_count()).map(|c|area.first().column()+c-1) {self.filter_command_revision(wire::FilterChange::Color{column,cell_color,color},Some(self.summary.get_untracked().revision));}else{self.say("Choose a column within the filter range",true);}
    }
    pub fn special_filter(&self, date:Option<String>, top:bool, percent:bool, value:String, blank:bool) {
        let Some(sheet)=self.sheet.get_untracked()else{return;};
        let Some(area)=self.filter_view().and_then(|f|f.area.area())else{return;};
        let Some(column)=self.model.with_untracked(|m|m.runtime.tools.filter.column.parse::<u32>().ok()).filter(|c|*c>0 && *c<=area.column_count()).map(|c|area.first().column()+c-1)else{self.say("Choose a column within the filter range",true);return;};
        let change=if let Some(date)=date {
            let groups=date.lines().filter(|s|!s.trim().is_empty()).map(|s|s.trim().split(['-','T',':']).map(str::parse::<u16>).collect::<Result<Vec<_>,_>>()).collect::<Result<Vec<_>,_>>();
            let Ok(groups)=groups else{self.say("Enter years such as 2026, months such as 2026-11, or dates such as 2026-11-03T14:30",true);return;};
            if groups.is_empty(){self.say("Enter at least one date group",true);return;}
            wire::FilterChange::Dates{column,groups,values:Vec::new(),blank}
        }else{wire::FilterChange::Top{column,top,percent,value}};
        self.filter_command_revision(change,Some(self.summary.get_untracked().revision));
    }
    pub fn clear_filter_column(&self) {
        let Some(sheet)=self.sheet.get_untracked()else{return;};
        let Some(filter)=self.filter_view()else{return;};
        let Some(area)=filter.area.area()else{return;};
        if let Some(c)=self.model.with_untracked(|m|m.runtime.tools.filter.column.parse::<u32>().ok()).filter(|c|*c>0 && *c<=area.column_count()) {self.filter_command(wire::FilterChange::ClearColumn(area.first().column()+c-1));}
    }

    pub fn sort_rows(&self, descending: bool) {
        let controller=*self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {return;}
            let areas=controller.areas.get_untracked();
            let [area]=areas.as_slice() else {
                controller.say("Select one rectangular range to sort",true);return;
            };
            let area=*area;
            let column=controller.model.with_untracked(|m|m.runtime.tools.sort_column.parse::<u32>());
            let Ok(column)=column else {controller.say("Enter a column number within the selection",true);return;};
            if column==0 || column>area.column_count() {controller.say("The sort column must be within the selected range (1 is its first column)",true);return;}
            let (header,levels)=controller.model.with_untracked(|m|(m.runtime.tools.sort_header,m.runtime.tools.sort_levels.clone()));
            let mut keys=vec![(area.first().column()+column-1,descending)];
            for (text,reverse) in levels {
                let Some(column)=text.parse::<u32>().ok().filter(|c|*c>0 && *c<=area.column_count()) else {controller.say("Every sort key must be a column within the selection",true);return;};
                keys.push((area.first().column()+column-1,reverse));
            }
            let (case_sensitive,custom_order)=controller.model.with_untracked(|m|(m.runtime.tools.sort_case_sensitive,m.runtime.tools.sort_custom_order.clone()));
            let custom_order:Vec<String>=if custom_order.is_empty(){Vec::new()}else{custom_order.lines().map(str::to_owned).collect()};
            let revision=controller.summary.get_untracked().revision;
            let custom_sort=!custom_order.is_empty();
            let intent=if case_sensitive||!custom_order.is_empty(){Intent::SortRowsWithOptions{area:Area::of(area),keys,header,revision,case_sensitive,custom_order}}else{Intent::SortRows{area:Area::of(area),keys,header,revision}};
            controller.dispatch(intent,move |outcome| {
                if outcome.accepted {controller.say(format!("Sorted {} {}",controller.area_label(area),if custom_sort {"in custom list order"}else if descending {"largest to smallest"}else{"smallest to largest"}),false);}
            });
        });
    }

    pub fn split_panes(&self, x: String, y: String) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                return;
            }
            let (Ok(x), Ok(y)) = (x.trim().parse::<f64>(), y.trim().parse::<f64>()) else {
                controller.say("Enter nonnegative split positions in points", true);
                return;
            };
            if !x.is_finite() || !y.is_finite() || x < 0. || y < 0. || x > 10000. || y > 10000. {
                controller.say("Split positions must be from 0 to 10000 points", true);
                return;
            }
            let Some(sheet) = controller.sheet.get_untracked() else {
                return;
            };
            let (row, column) = controller.active.get_untracked();
            controller.dispatch(
                Intent::SplitPanes {
                    sheet: wire::Sheet::of(sheet),
                    revision: controller.summary.get_untracked().revision,
                    x: (x * 20.).to_bits(),
                    y: (y * 20.).to_bits(),
                    row,
                    column,
                },
                move |o| {
                    if o.accepted {
                        controller.say(
                            if x == 0. && y == 0. {
                                "Split removed"
                            } else {
                                "Worksheet split"
                            },
                            false,
                        );
                    }
                },
            );
        });
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
        let target = self.active_key_untracked().expect("checked active cell");
        if let Some(reason) = self.selection.with_untracked(|mirror| {
            mirror
                .geometry(target.sheet())
                .and_then(|g| g.edit_refusal.clone())
        }) {
            self.say(reason, true);
            return;
        }
        let original = self
            .active_key_untracked()
            .and_then(|at| {
                self.selection
                    .with_untracked(|mirror| mirror.cell(at).map(|cell| cell.input.clone()))
            })
            .unwrap_or_default();
        let areas = self.areas.get_untracked();
        self.model
            .update(|m| m.edit.begin(origin, target, original.clone(), seed, areas));
        self.assist.start_edit(original);
        self.offer_defined_names();
    }

    /// Asks the workbook for its defined names (no sheet rows) and offers them to the formula
    /// field's completion beside the catalogue's functions.
    fn offer_defined_names(&self) {
        let controller = *self;
        let generation = self.generation.get_untracked();
        let shown = self.summary.with_untracked(|s| {
            s.sheets.iter().map(|(sheet, _)| (*sheet, 0)).collect()
        });
        self.ask(
            wire::Request::Outline {
                shown,
                open: Vec::new(),
                reveal: None,
                query: String::new(),
            },
            move |reply| {
                if controller.generation.get_untracked() != generation {
                    return;
                }
                if let wire::Reply::Outline(found) = reply {
                    let names: Vec<String> = found
                        .names
                        .iter()
                        .map(|text| text.split(" = ").next().unwrap_or(text).trim().to_owned())
                        .collect();
                    controller.assist.scope.try_update(|scope| scope.names = names);
                }
            },
        );
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

    /// Saves the workbook as a package's bytes; `saved` gets its name and bytes, and the status
    /// line says how it went.
    pub fn save(&self, saved: impl FnOnce(String, Vec<u8>) + 'static) {
        self.save_choice(None,saved);
    }
    pub fn prepare_save_as(&self, ready: impl FnOnce()+'static) {
        let controller=*self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_none() {ready();}
            else {controller.commit_edit(move |accepted|if accepted {ready();});}
        });
    }
    pub fn save_as(&self,name:String,format:wire::FileFormat,saved:impl FnOnce(String,Vec<u8>)+'static) {
        // The modal opens only after its preceding cell entry has committed. A later
        // pending input needs a fresh user action, never an abandoned modal continuation.
        if self.waiting_input() || self.editing.get_untracked().is_some() {
            self.say("Finish the current cell entry before Save As",true);return;
        }
        if let Some(reason)=self.model.with_untracked(|m|m.views.pending_reason()) {self.say(reason,true);return;}
        self.save_package(Some((name,format)),saved);
    }
    fn save_choice(&self,target:Option<(String,wire::FileFormat)>,saved:impl FnOnce(String,Vec<u8>)+'static) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.save_choice(target,saved));
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
                    controller.save_package(target,saved);
                } else {
                    controller.focus_editor();
                }
            });
        } else {
            self.save_package(target,saved);
        }
    }

    fn save_package(&self, target:Option<(String,wire::FileFormat)>, saved: impl FnOnce(String, Vec<u8>) + 'static) {
        if self.save_download.with_value(|pending| pending.is_some()) {
            self.say("A workbook save is still being prepared", false);
            return;
        }
        let controller = *self;
        let generation = self.generation.get_untracked();
        let Some(active_sheet) = self.sheet.get_untracked() else {
            return;
        };
        let views = self.grid_views.with_value(|all| {
            let mut grouped: BTreeMap<SheetId, Vec<wire::PaneWindow>> = BTreeMap::new();
            for (&(sheet, index), v) in all {
                grouped.entry(sheet).or_default().push(wire::PaneWindow {
                    index,
                    x: v.viewport.scroll_x,
                    y: v.viewport.scroll_y,
                    zoom: v.zoom,
                });
            }
            grouped
                .into_iter()
                .filter(|(sheet, _)| {
                    self.summary
                        .with_untracked(|all| all.sheets.iter().any(|(s, _)| s.id() == *sheet))
                })
                .filter_map(|(sheet, panes)| {
                    let active = if sheet == active_sheet {
                        self.active.get_untracked()
                    } else {
                        self.sheet_selections
                            .with_value(|a| a.get(&sheet).map(|(at, _)| (at.row(), at.column())))
                            .unwrap_or((1, 1))
                    };
                    Some(wire::SheetWindow {
                        geometry: self.grid_geometry(sheet)?,
                        sheet: wire::Sheet::of(sheet),
                        active,
                        active_pane: self.active_grid_pane(sheet).unwrap_or(0),
                        panes,
                    })
                })
                .collect()
        });
        let views=Some(wire::SaveWindow {
                revision: self.summary.get_untracked().revision,
                active_sheet: wire::Sheet::of(active_sheet),
                sheets: views,
            });
        // Applying the worker reply updates Summary before this callback runs. Keep the
        // download's original protection here, including saves which finish asynchronously.
        let unconfirmed = target.is_some()
            || self.summary.with_untracked(|s| s.changed || s.unsaved);
        let request=match target {Some((name,format))=>Request::SaveAs{name,format,views},None=>Request::SaveWithContext {calculate_before_save:false,views}};
        self.saving.set(true);
        self.send_command(request, None, move |reply, bytes| {
            if controller.generation.get_untracked() == generation {
                controller.save_reply(reply, bytes, Box::new(move |name, bytes| {
                    if unconfirmed {
                        controller.download_unconfirmed.set(true);
                    }
                    let personal=controller.personal_template.get_value().filter(|old|old.eq_ignore_ascii_case(&name));
                    let updated=personal.map(|_|crate::files::update_personal_template(&name,&bytes));
                    saved(name, bytes);
                    if let Some(Err(error))=updated{controller.say(format!("The download was prepared, but the personal template was not updated: {error}. Add the downloaded template again."),true);}
                }));
            }
        });
    }
    fn save_reply(
        &self,
        reply: Result<Reply, String>,
        bytes: Option<Vec<u8>>,
        saved: SaveDownload,
    ) {
        self.saving.set(matches!(&reply,Ok(Reply::SavePending {..})));
        let controller = *self;
        controller.revision.update(|n| *n += 1);
        match (reply, bytes) {
            (
                Ok(Reply::Done {
                    outcome,
                    saved: Some(name),
                    ..
                }),
                Some(bytes),
            )
            | (
                Ok(Reply::SaveImage {
                    outcome,
                    saved: name,
                    ..
                }),
                Some(bytes),
            ) => {
                if controller.summary.with_untracked(|s|s.changed || s.unsaved) {
                    controller.download_unconfirmed.set(true);
                }
                controller.summary.set(outcome.summary.into());
                controller.say(
                    format!("Download package prepared: {name}, {} bytes", bytes.len()),
                    false,
                );
                saved(name, bytes);
            }
            (Ok(Reply::SavePending { token, .. }), _) => {
                let workbook = controller.summary.with_untracked(|s| s.workbook);
                controller
                    .save_download
                    .set_value(Some((workbook, token, saved)));
                controller.say("Preparing workbook download", false);
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
                    self.dispatch(if redo { Intent::Redo } else { Intent::Undo }, |_| {});
                }
            }
            Ok(CommandEffect::PasteSpecial) => crate::clipboard::open_paste_special(),
            Ok(CommandEffect::FlashFill) => self.flash_fill(),
            Ok(CommandEffect::Recalculate) => self.dispatch(Intent::Recalculate, |_| {}),
            Ok(CommandEffect::Dialog) => self.focus_command_dialog(),
            Ok(CommandEffect::PageSetup
                | CommandEffect::PrintPreview
                | CommandEffect::FormatCells | CommandEffect::NameManager,
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
                server.execute_current(Request::Open { name:"tables.xlsx".into() }, Some(include_bytes!("../../../crates/calc-engine/tests/fixtures/dc_10y/oracle.xlsx").to_vec())).reply.unwrap();
                while server.has_pending_calculation() { server.advance_calculation(); }
                let s = server.book().host().sheets().iter().find(|s| s.name().text().to_string_lossy()=="DT").unwrap().id();
                server.book_mut().dispatch(SkinIntent::SetCalcMode(CalcMode::AutomaticExceptTables));
                for (row,column,text) in [(1,26,"1"),(3,2,"=B1*Z1"),(4,1,"2"),(5,1,"3")] {
                    server.book_mut().dispatch(SkinIntent::EnterCell { at:CellKey::new(s,row,column).unwrap(),text:text.into() });
                    while server.has_pending_calculation() { server.advance_calculation(); }
                }
                server.book_mut().dispatch(SkinIntent::Recalculate);
                while server.has_pending_calculation() { server.advance_calculation(); }
                server.book_mut().dispatch(SkinIntent::EnterCell { at:CellKey::new(s,1,26).unwrap(),text:"10".into() });
                while server.has_pending_calculation() { server.advance_calculation(); }
                server.set_calculation_budget(WorkBudget::evaluations(1));
                let (package,_) = server.book_mut().save().unwrap();
                let transport=std::rc::Rc::new(dna_calc_worker::transport::InProcessTransport::default());
                let controller=Controller::with_transport(transport.clone());
                controller.open("tables.xlsx",package,|opened| assert!(opened));
                let images=std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                let target=images.clone();
                controller.save(move |name,bytes| target.borrow_mut().push((name,bytes)));
                assert!(images.borrow().is_empty(), "a pending save has no package bytes");
                let pending=controller.save_download.with_value(|p| p.as_ref().map(|(book,token,_)| (*book,*token))).expect("retained download callback");
                if reopen {
                    controller.new_workbook();
                    controller.receive(Event::SaveReady { workbook:pending.0,token:pending.1 });
                    assert!(controller.save_download.with_value(|p| p.is_none()));
                    assert!(images.borrow().is_empty());
                } else {
                    for _ in 0..1000 {
                        assert!(!matches!(transport.advance_calculation().unwrap(),HostProgress::Failed(_)));
                        if !images.borrow().is_empty() { break; }
                    }
                    assert_eq!(images.borrow().len(),1);
                    assert!(controller.save_download.with_value(|p| p.is_none()));
                    let bytes=images.borrow()[0].1.clone();
                    let reopened=calc_host::HostState::open(std::sync::Arc::new(calc_host::MemorySource::new(bytes)),&calc_host::OpenPolicy::default(),calc_host::HostOptions { calc_mode:Some(CalcMode::Manual),..Default::default() }).unwrap();
                    let s=reopened.sheets().iter().find(|s| s.name().text().to_string_lossy()=="DT").unwrap().id();
                    for (row,expected) in [(4,20.0),(5,30.0)] {
                        let page=calc_host::project(&reopened,&calc_host::ProjectionRequest { cells:vec![CellKey::new(s,row,2).unwrap()] }).unwrap();
                        assert!(matches!(page.cells()[0].value(),calc_host::ProjectedValue::FileCached(cache) if cache.value().to_value().as_number()==Some(expected)));
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
                assert!(controller.validation_epoch().get_untracked() > published_epoch,
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
        inner:dna_calc_worker::transport::InProcessTransport,
        hold:std::cell::Cell<bool>,
        hold_paste:std::cell::Cell<bool>,
        hold_drag_query:std::cell::Cell<u8>,
        pending:std::cell::RefCell<Vec<(Request,Option<Vec<u8>>,dna_calc_model::TransportAnswer)>>,
    }
    impl dna_calc_model::WorkbookTransport for HeldTransport {
        fn start(&self){self.inner.start();}
        fn set_events(&self,h:std::rc::Rc<dyn Fn(Event)>){self.inner.set_events(h);}
        fn set_health(&self,h:std::rc::Rc<dyn Fn(port::Health)>){self.inner.set_health(h);}
        fn call(&self,r:Request,b:Option<Vec<u8>>,a:dna_calc_model::TransportAnswer){
            let inner=match &r {Request::Command(envelope)=>envelope.request.as_ref(),_=>&r};
            let query=matches!((self.hold_drag_query.get(),inner),(1,Request::Capture {..})|(2,Request::PreparePaste {..}));
            if self.hold.get() || query || (self.hold_paste.get() && matches!(inner,Request::Intent(Intent::ApplyPaste { .. }))){self.pending.borrow_mut().push((r,b,a));}else{self.inner.call(r,b,a);}
        }
    }
    #[cfg_attr(not(target_arch="wasm32"),test)]
    #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_q327__navigation_during_drag_preparation_cancels_the_stale_drag() {
        use std::rc::Rc;
        for stage in [1,2] {
            let root=Owner::new();root.with(|| {
                let transport=Rc::new(HeldTransport::default());
                let controller=Controller::with_transport(transport.clone());
                let source=controller.active_key_untracked().unwrap();
                let destination=CellKey::new(source.sheet(),3,4).unwrap();
                controller.dispatch(Intent::EnterCell {at:Key::of(source),text:"5".into()},|outcome|assert!(outcome.accepted));
                let revision=controller.summary.get_untracked().revision;
                transport.hold_drag_query.set(stage);
                controller.drag_range(GridArea::cell(source),GridArea::cell(destination),false,controller.generation.get_untracked(),revision);
                assert_eq!(transport.pending.borrow().len(),1);
                controller.navigate(crate::grid::selection::Navigation::Step(NavAction::Right),false,20);
                transport.hold_drag_query.set(0);
                let pending=std::mem::take(&mut *transport.pending.borrow_mut());
                for(r,b,a)in pending{transport.inner.call(r,b,a);}
                while let Some(action)=controller.deferred.try_update_value(|q|q.pop_front()).flatten(){controller.perform(action);}
                assert_eq!(controller.active_key_untracked(),Some(CellKey::new(source.sheet(),1,2).unwrap()));
                assert_eq!(controller.summary.get_untracked().revision,revision);
                assert!(controller.paste_preview.get_untracked().is_none());
                assert!(controller.status.get_untracked().refused);
                assert_eq!(controller.pending_actions.get_untracked(),0);
                assert_eq!(controller.pending_changes.get_untracked(),0);
            });
        }
    }
    #[cfg_attr(not(target_arch="wasm32"),test)]
    #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_q327__drag_selection_is_undoable_and_a_late_paste_preserves_newer_navigation() {
        use std::rc::Rc;
        for delayed in [false, true] {
            let root=Owner::new();root.with(|| {
                let transport=Rc::new(HeldTransport::default());
                let controller=Controller::with_transport(transport.clone());
                let source=controller.active_key_untracked().unwrap();
                let destination=CellKey::new(source.sheet(),3,4).unwrap();
                controller.dispatch(Intent::EnterCell {at:Key::of(source),text:"5".into()},|outcome|assert!(outcome.accepted));
                transport.hold_paste.set(delayed);
                controller.drag_range(GridArea::cell(source),GridArea::cell(destination),false,controller.generation.get_untracked(),controller.summary.get_untracked().revision);
                if delayed {
                    assert_eq!(transport.pending.borrow().len(),1);
                    controller.navigate(crate::grid::selection::Navigation::Step(NavAction::Right),false,20);
                    transport.hold_paste.set(false);
                    let pending=std::mem::take(&mut *transport.pending.borrow_mut());
                    for(r,b,a)in pending{transport.inner.call(r,b,a);}
                    while let Some(action)=controller.deferred.try_update_value(|q|q.pop_front()).flatten(){controller.perform(action);}
                    assert_eq!(controller.active_key_untracked(),Some(CellKey::new(source.sheet(),3,5).unwrap()));
                } else {
                    assert_eq!(controller.selected_area_untracked(),Some(GridArea::cell(destination)));
                    controller.dispatch(Intent::Undo,|outcome|assert!(outcome.accepted));
                    assert_eq!(controller.selected_area_untracked(),Some(GridArea::cell(source)));
                    controller.dispatch(Intent::Redo,|outcome|assert!(outcome.accepted));
                    assert_eq!(controller.selected_area_untracked(),Some(GridArea::cell(destination)));
                }
                assert_eq!(controller.pending_actions.get_untracked(),0);
                assert_eq!(controller.pending_changes.get_untracked(),0);
            });
        }
    }
    #[cfg_attr(not(target_arch="wasm32"),test)]
    #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__pending_insertion_protects_replacement_and_applies_its_summary() {
        use std::{cell::Cell,rc::Rc};use dna_calc_model::WorkbookTransport;
        let root=Owner::new();root.with(||{
            let transport=Rc::new(HeldTransport::default());let controller=Controller::with_transport(transport.clone());
            let mut source=dna_calc_worker::server::Server::new();let bytes=source.execute_current(Request::SaveAs{name:"Sheet.xltx".into(),format:wire::FileFormat::Xltx,views:None},None).bytes.unwrap();
            transport.hold.set(true);let accepted=Rc::new(Cell::new(false));let answer=accepted.clone();controller.insert_template(bytes,None,move |ok|answer.set(ok));
            assert!(controller.has_dirty_work());assert!(controller.input_pending());assert_eq!(controller.pending_changes.get_untracked(),1);
            let later=Rc::new(Cell::new(false));let out=later.clone();controller.after_input(move ||out.set(true));assert!(!later.get());
            transport.hold.set(false);let pending=std::mem::take(&mut *transport.pending.borrow_mut());for(r,b,a)in pending{transport.inner.call(r,b,a);}
            assert!(accepted.get());assert_eq!(controller.summary.get_untracked().sheets.len(),2);assert!(controller.summary.get_untracked().changed);assert_eq!(controller.pending_changes.get_untracked(),0);assert_eq!(controller.pending_actions.get_untracked(),0);
            while let Some(action)=controller.deferred.try_update_value(|q|q.pop_front()).flatten(){controller.perform(action);}assert!(later.get());
        });
    }
    #[cfg_attr(not(target_arch="wasm32"),test)]
    #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__idle_save_as_opens_and_an_unsaved_template_keeps_download_protection() {
        use std::{cell::Cell,rc::Rc};
        let root=Owner::new();root.with(|| {
            let controller=Controller::with_transport(Rc::new(dna_calc_worker::transport::InProcessTransport::default()));
            let opened=Rc::new(Cell::new(false));let out=opened.clone();
            controller.prepare_save_as(move ||out.set(true));assert!(opened.get(),"idle Save As must open without an editor target");
            let mut source=dna_calc_worker::server::Server::new();
            let bytes=source.execute_current(Request::SaveAs{name:"Template.xltx".into(),format:wire::FileFormat::Xltx,views:None},None).bytes.unwrap();
            controller.open("Template.xltx",bytes,|ok|assert!(ok));
            assert!(controller.summary.get_untracked().unsaved);assert!(!controller.summary.get_untracked().changed);
            let downloaded=Rc::new(Cell::new(false));let out=downloaded.clone();
            controller.save_as("New.xlsx".into(),wire::FileFormat::Xlsx,move |name,bytes|{assert_eq!(name,"New.xlsx");assert!(!bytes.is_empty());out.set(true);});
            assert!(downloaded.get());assert!(!controller.saving.get_untracked());assert!(!controller.summary.get_untracked().unsaved);
            assert!(controller.download_unconfirmed.get_untracked());assert!(controller.has_dirty_work());
        });
    }
}

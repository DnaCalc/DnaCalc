//! Level-one frame mounted over a transport-neutral workbook exchange.

use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::*;
use dna_kit::{Registry, WorkbookContext};
use dna_skin_runtime::{Live, workbook::FrameStore};
use leptos::{
    leptos_dom::helpers::request_animation_frame,
    prelude::{Update, *},
};
use std::cell::Cell as Counter;
use std::rc::Rc;

const WINDOW_ROWS: u32 = 36;
const WINDOW_COLUMNS: u32 = 18;
const BUDGET: Budget = Budget {
    cells: 4096,
    nodes: 4096,
    edges: 4096,
    bytes: 4 * 1024 * 1024,
    millis: 100,
};

pub type Complete = Box<dyn FnOnce(Result<WorkbookReply, Refusal>)>;
pub type Exchange = Rc<dyn Fn(WorkbookRequest, Option<Vec<u8>>, Complete)>;

#[derive(Clone, Copy)]
pub struct WorkbookFrame {
    send: StoredValue<Exchange, LocalStorage>,
    store: RwSignal<FrameStore>,
    summary: RwSignal<Option<WorkbookSummary>>,
    summary_watch: RwSignal<Option<WatchId>>,
    calc_watch: RwSignal<Option<WatchId>>,
    calculation: RwSignal<&'static str>,
    history_watch: RwSignal<Option<WatchId>>,
    area_watch: RwSignal<Option<WatchId>>,
    geometry_watch: RwSignal<Option<WatchId>>,
    area: RwSignal<Option<Area>>,
    sheet: RwSignal<Option<Sheet>>,
    window: RwSignal<Option<CellWindow>>,
    geometry: RwSignal<Option<WorksheetGeometry>>,
    active: RwSignal<Option<Key>>,
    draft: RwSignal<Option<Draft>>,
    formula: RwSignal<String>,
    formula_input: NodeRef<leptos::html::Input>,
    notice: RwSignal<String>,
    next_batch: StoredValue<Counter<u64>, LocalStorage>,
    search: RwSignal<bool>,
    details: RwSignal<bool>,
    queued: RwSignal<bool>,
    busy: RwSignal<bool>,
}

#[derive(Clone)]
struct Draft {
    at: Key,
    workbook: WorkbookId,
    model: Revision,
    text: String,
}

impl WorkbookFrame {
    pub fn new(send: Exchange) -> Self {
        let frame = Self {
            send: StoredValue::new_local(send),
            store: RwSignal::new(FrameStore::default()),
            summary: RwSignal::new(None),
            summary_watch: RwSignal::new(None),
            calc_watch: RwSignal::new(None),
            calculation: RwSignal::new(""),
            history_watch: RwSignal::new(None),
            area_watch: RwSignal::new(None),
            geometry_watch: RwSignal::new(None),
            area: RwSignal::new(None),
            sheet: RwSignal::new(None),
            window: RwSignal::new(None),
            geometry: RwSignal::new(None),
            active: RwSignal::new(None),
            draft: RwSignal::new(None),
            formula: RwSignal::new(String::new()),
            formula_input: NodeRef::new(),
            notice: RwSignal::new(String::new()),
            next_batch: StoredValue::new_local(Counter::new(1)),
            search: RwSignal::new(false),
            details: RwSignal::new(false),
            queued: RwSignal::new(false),
            busy: RwSignal::new(false),
        };
        frame.watch(WorkbookQuery::Summary, frame.summary_watch);
        frame.watch(WorkbookQuery::CalcState, frame.calc_watch);
        frame.watch(WorkbookQuery::History, frame.history_watch);
        frame
    }

    fn request(
        self,
        request: WorkbookRequest,
        callback: impl FnOnce(Result<WorkbookReply, Refusal>) + 'static,
    ) {
        self.send
            .with_value(|send| send(request, None, Box::new(callback)));
    }

    pub fn open(self, name: String, bytes: Vec<u8>) {
        if self.draft.get_untracked().is_some() || self.busy.get_untracked() {
            self.notice
                .set("Finish or cancel the cell edit before opening a workbook.".into());
            return;
        }
        self.send.with_value(|send| {
            send(
                WorkbookRequest::Open { name },
                Some(bytes),
                Box::new(move |result| match result {
                    Ok(WorkbookReply::Stamp(_)) => self.notice.set(String::new()),
                    Err(reason) => self.error(reason),
                    _ => self.error(Refusal::Transport("Expected a workbook reply".into())),
                }),
            )
        });
    }

    pub fn reset_tiles(self) {
        self.request(WorkbookRequest::ResetTiles, |_| ());
    }

    pub fn acknowledge_tiles(self, transaction: u64) {
        self.request(WorkbookRequest::AcknowledgeTiles { transaction }, |_| ());
    }

    pub fn error(self, reason: Refusal) {
        self.notice.set(refusal_text(reason));
    }

    fn watch(self, query: WorkbookQuery, slot: RwSignal<Option<WatchId>>) {
        if let Some(old) = slot.get_untracked() {
            self.store.update(|store| store.unwatch(old));
            self.request(WorkbookRequest::Unwatch(old), |_| ());
            slot.set(None);
        }
        self.request(
            WorkbookRequest::Watch {
                query: query.clone(),
                budget: BUDGET,
            },
            move |result| match result {
                Ok(WorkbookReply::Watching(id)) => {
                    let current = match &query {
                        WorkbookQuery::Geometry { sheet } => {
                            self.sheet.get_untracked() == Some(*sheet)
                        }
                        WorkbookQuery::Cells {
                            interest: Interest::Area(area),
                        } => self.area.get_untracked() == Some(*area),
                        _ => true,
                    };
                    if !current {
                        self.request(WorkbookRequest::Unwatch(id), |_| ());
                        return;
                    }
                    if let Some(old) = slot.get_untracked() {
                        self.store.update(|store| store.unwatch(old));
                        self.request(WorkbookRequest::Unwatch(old), |_| ());
                    }
                    slot.set(Some(id));
                    self.store.update(|store| store.watch(id));
                }
                Err(reason) => self.error(reason),
                _ => self.error(Refusal::Transport("Expected a watch reply".into())),
            },
        );
    }

    fn read<T: serde::de::DeserializeOwned>(self, slot: RwSignal<Option<WatchId>>) -> Option<T> {
        let id = slot.get_untracked()?;
        self.store.with_untracked(|store| match store.live(id) {
            Some(Live::Ready(answer)) => serde_json::from_value(answer.body.clone()).ok(),
            _ => None,
        })
    }

    pub fn receive(self, frame: Frame) {
        let previous = self.store.with_untracked(FrameStore::stamp);
        let workbook = frame.stamp.workbook;
        let applied = self
            .store
            .try_update(|store| store.apply(frame))
            .unwrap_or(false);
        if !applied {
            return;
        }
        leptos::prelude::batch(|| {
            if previous.is_some_and(|stamp| stamp.workbook != workbook) {
                self.draft.set(None);
                self.queued.set(false);
                self.window.set(None);
                self.geometry.set(None);
                self.area.set(None);
                self.sheet.set(None);
            }
            if let Some(calc) = self.read::<serde_json::Value>(self.calc_watch) {
                self.calculation.set(match calc["state"].as_i64() {
                    Some(2) if calc["mode"] == "Manual" => "Calculate",
                    Some(2) => "Calculating",
                    _ if !calc["circular"].is_null() => "Circular reference",
                    _ => "",
                });
            }
            if let Some(summary) = self.read(self.summary_watch) {
                self.summary.set(Some(summary));
            }
            let active = self
                .store
                .with_untracked(|store| store.selection().and_then(|selection| selection.active));
            self.set_active(active);
            if let Some(window) = self.read::<CellWindow>(self.area_watch) {
                if window.areas.first().copied() == self.area.get_untracked() {
                    self.window.set(Some(window));
                }
            }
            if let Some(geometry) = self.read(self.geometry_watch) {
                self.geometry.set(Some(geometry));
            }
            self.refresh_formula();
        });
        if self.queued.get_untracked() && !self.busy.get_untracked() {
            self.apply_draft();
        }
    }

    fn set_active(self, active: Option<Key>) {
        self.active.set(active);
        let Some(key) = active else { return };
        let sheet = Sheet(key.0, key.1);
        if self.sheet.get_untracked() != Some(sheet) {
            self.sheet.set(Some(sheet));
            self.geometry.set(None);
            self.watch(WorkbookQuery::Geometry { sheet }, self.geometry_watch);
        }
        let in_window = self
            .area
            .get_untracked()
            .and_then(Area::area)
            .zip(key.cell())
            .is_some_and(|(area, at)| area.contains(at));
        if !in_window {
            let row = ((key.2 - 1) / WINDOW_ROWS) * WINDOW_ROWS + 1;
            let column = ((key.3 - 1) / WINDOW_COLUMNS) * WINDOW_COLUMNS + 1;
            let first = Key(key.0, key.1, row, column);
            let last = Key(
                key.0,
                key.1,
                (row + WINDOW_ROWS - 1).min(1_048_576),
                (column + WINDOW_COLUMNS - 1).min(16_384),
            );
            let area = Area(first, last);
            self.area.set(Some(area));
            self.window.set(None);
            self.watch(
                WorkbookQuery::Cells {
                    interest: Interest::Area(area),
                },
                self.area_watch,
            );
        }
    }

    fn select(self, at: Key) {
        if self.active.get_untracked() == Some(at) {
            return;
        }
        let selection = Selection {
            seq: 0,
            target: source(at),
            active: Some(at),
            grouped: Vec::new(),
            item: None,
        };
        let client_seq = self
            .store
            .try_update(|store| store.select(selection.clone()))
            .unwrap_or(0);
        self.set_active(Some(at));
        self.refresh_formula();
        self.request(
            WorkbookRequest::Select {
                selection,
                client_seq,
            },
            move |result| {
                if let Err(reason) = result {
                    self.store
                        .update(|store| store.refuse_selection(client_seq));
                    let active = self.store.with_untracked(|store| {
                        store.selection().and_then(|selection| selection.active)
                    });
                    self.set_active(active);
                    self.error(reason);
                }
            },
        );
    }

    fn cell_input(self, at: Key) -> String {
        self.window.with_untracked(|window| {
            window
                .as_ref()
                .and_then(|window| window.cells.iter().find(|cell| cell.at == at))
                .map(|cell| cell.input.clone())
                .unwrap_or_default()
        })
    }

    fn refresh_formula(self) {
        if self.draft.get_untracked().is_none() {
            self.formula.set(
                self.active
                    .get_untracked()
                    .map_or_else(String::new, |at| self.cell_input(at)),
            );
        }
    }

    fn begin_edit(self, at: Key, text: Option<String>) {
        if self.busy.get_untracked() {
            return;
        }
        self.select(at);
        let Some(stamp) = self.store.with_untracked(FrameStore::stamp) else {
            return;
        };
        let text = text.unwrap_or_else(|| self.cell_input(at));
        self.formula.set(text.clone());
        self.draft.set(Some(Draft {
            at,
            workbook: stamp.workbook,
            model: stamp.model,
            text,
        }));
        if let Some(input) = self.formula_input.get_untracked() {
            let _ = input.focus();
            let _ =
                input.set_selection_range(input.value().len() as u32, input.value().len() as u32);
        }
    }

    fn cancel(self) {
        if self.busy.get_untracked() {
            return;
        }
        self.queued.set(false);
        self.draft.set(None);
        self.refresh_formula();
        self.notice.set(String::new());
    }

    fn apply_draft(self) {
        if self.busy.get_untracked() {
            return;
        }
        let Some(draft) = self.draft.get_untracked() else {
            return;
        };
        let selection = self
            .store
            .with_untracked(|store| store.confirmed_selection().cloned());
        let Some(selection) = selection else {
            self.queued.set(true);
            return;
        };
        self.queued.set(false);
        if selection.active != Some(draft.at) {
            self.error(Refusal::StaleSelection);
            return;
        }
        let id = self.next_batch.with_value(|counter| {
            let id = counter.get();
            counter.set(id + 1);
            id.to_string()
        });
        let batch = Batch {
            id,
            workbook: Some(draft.workbook),
            base: Some(draft.model),
            expects: vec![source(draft.at)],
            selection_seq: Some(selection.seq),
            label: None,
            changes: vec![Change::Model(ModelChange::Enter {
                at: draft.at,
                text: draft.text,
            })],
        };
        self.busy.set(true);
        self.request(
            WorkbookRequest::Preview {
                batch: batch.clone(),
                depth: PreviewDepth::WritesOnly,
            },
            move |result| match result {
                Ok(WorkbookReply::Preview(Preview {
                    refusal: Some(reason),
                    ..
                }))
                | Err(reason) => {
                    self.busy.set(false);
                    self.error(reason);
                }
                Ok(WorkbookReply::Preview(preview)) if preview.warnings.is_empty() => {
                    self.request(WorkbookRequest::Commit(batch), move |result| {
                        self.busy.set(false);
                        match result {
                            Ok(WorkbookReply::Stamp(_)) => {
                                self.draft.set(None);
                                self.notice.set(String::new());
                                self.refresh_formula();
                            }
                            Err(reason) => self.error(reason),
                            _ => self.error(Refusal::Transport("Expected a commit reply".into())),
                        }
                    });
                }
                Ok(WorkbookReply::Preview(preview)) => {
                    self.busy.set(false);
                    self.notice.set(preview.warnings.join(" · "));
                }
                _ => {
                    self.busy.set(false);
                    self.error(Refusal::Transport("Expected a preview reply".into()));
                }
            },
        );
    }

    fn run(self, operation: Operation) {
        let notice = self
            .read::<serde_json::Value>(self.history_watch)
            .and_then(|history| {
                let (verb, stack) = match operation {
                    Operation::Undo => ("Undid", "undo_entries"),
                    Operation::Redo => ("Redid", "redo_entries"),
                    _ => return None,
                };
                let label = history[stack][0]["label"].as_str()?;
                Some(format!("{verb} · {label}"))
            });
        self.request(
            WorkbookRequest::Run(operation),
            move |result| match result {
                Ok(WorkbookReply::Stamp(_)) => self.notice.set(notice.unwrap_or_default()),
                Err(reason) => self.error(reason),
                _ => self.error(Refusal::Transport("Expected an operation reply".into())),
            },
        );
    }

    fn goto(self, text: String) {
        self.request(
            WorkbookRequest::Ask {
                query: WorkbookQuery::Resolve { address: text },
                budget: BUDGET,
            },
            move |result| match result {
                Ok(WorkbookReply::Answer(answer)) => {
                    match serde_json::from_value::<SourceRef>(answer.body) {
                        Ok(source) => self.request(
                            WorkbookRequest::Navigate(Target::Source(source)),
                            move |result| {
                                if let Err(reason) = result {
                                    self.error(reason);
                                }
                            },
                        ),
                        Err(error) => self.error(Refusal::Transport(error.to_string())),
                    }
                }
                Err(reason) => self.error(reason),
                _ => self.error(Refusal::Transport("Expected an address reply".into())),
            },
        );
    }

    pub fn probe(self) -> serde_json::Value {
        serde_json::json!({
            "stamp": self.store.with_untracked(FrameStore::stamp),
            "active": self.active.get_untracked(),
            "cells": self.window.get_untracked().map(|window| window.cells),
            "formula": self.formula.get_untracked(),
            "notice": self.notice.get_untracked(),
        })
    }
}

fn source(at: Key) -> SourceRef {
    SourceRef::Cells {
        sheet: Sheet(at.0, at.1),
        areas: vec![Area(at, at)],
    }
}

fn a1(at: Key) -> String {
    format!("{}{}", address::column_letters(at.3), at.2)
}

fn refusal_text(reason: Refusal) -> String {
    match reason {
        Refusal::Excel { reason }
        | Refusal::Invalid { reason }
        | Refusal::Validation { reason, .. }
        | Refusal::Transport(reason) => reason,
        Refusal::Conflict { .. } => {
            "The workbook changed. Check the cell and start the edit again.".into()
        }
        Refusal::StaleSelection => "The selection changed during this edit.".into(),
        Refusal::ReadOnly => "This workbook is read-only.".into(),
        Refusal::Protected(_) => "The worksheet is protected.".into(),
        Refusal::NotModelled { what, owner } => format!("{what} is unavailable ({owner})"),
        Refusal::OverBudget { .. } => "Choose a smaller worksheet area.".into(),
        reason => format!("{reason:?}"),
    }
}

pub fn workbook(
    registry: &Registry,
    state: WorkbookFrame,
    file_control: AnyView,
) -> impl IntoView + use<> {
    let stage = registry.mount_workbook(WorkbookContext {
        window: state.window,
        geometry: state.geometry,
        active: state.active,
        select: Callback::new(move |at| state.select(at)),
        edit: Callback::new(move |(at, text)| state.begin_edit(at, text)),
    });
    let search = NodeRef::<leptos::html::Input>::new();
    view! {
        <div class="dna-workbook" data-dnacalc="frame" on:keydown=move |event: leptos::ev::KeyboardEvent| {
            if event.alt_key() && event.key().eq_ignore_ascii_case("q") {
                event.prevent_default();
                state.search.set(true);
                request_animation_frame(move || {
                    if let Some(search) = search.get_untracked() { let _ = search.focus(); }
                });
            } else if event.ctrl_key() && event.key().eq_ignore_ascii_case("z") && state.draft.get_untracked().is_none() {
                event.prevent_default();
                state.run(Operation::Undo);
            }
        }>
            <header class="dna-book-mast">
                <span class="dna-fe-mark" aria-hidden="true"><i></i><i></i></span>
                <strong>"DNA Calc"</strong>
                <span class="dna-book-title">{move || state.summary.get().map(|book| book.name).unwrap_or_default()}</span>
                <span>{move || state.summary.get().map(|book| if book.capabilities.read_only { "Read-only" } else if book.edited { "Edited" } else if book.layout_edited { "Layout edited" } else { "Unchanged" }).unwrap_or("")}</span>
                <span class="dna-book-view">"Classic"</span>
                <button type="button" on:click=move |_| {
                    state.search.update(|show| *show = !*show);
                    request_animation_frame(move || {
                    if let Some(search) = search.get_untracked() { let _ = search.focus(); }
                });
                }>"Go to" <kbd>"Alt+Q"</kbd></button>
            </header>
            <nav class="dna-book-deck" aria-label="Workbook commands">
                {file_control}
                <button type="button" data-command="undo" on:click=move |_| state.run(Operation::Undo)>"Undo"</button>
                <button type="button" data-command="redo" on:click=move |_| state.run(Operation::Redo)>"Redo"</button>
                <button type="button" data-command="calculate" on:click=move |_| state.run(Operation::Calculate { scope: SourceRef::Workbook })>"Calculate"</button>
                <button type="button" on:click=move |_| state.details.update(|value| *value = !*value)>"Details"</button>
            </nav>
            <div class="dna-book-bridge">
                <input class="dna-book-name" aria-label="Cell address" data-dnacalc="name-box"
                    prop:value=move || state.active.get().map(a1).unwrap_or_default()
                    on:keydown=move |event: leptos::ev::KeyboardEvent| {
                        if event.key() == "Enter" { state.goto(event_target_value(&event)); }
                    }/>
                <span class="dna-book-fx" aria-hidden="true">"ƒx"</span>
                <input class="dna-book-formula" aria-label="Formula bar" data-dnacalc="formula-bar" node_ref=state.formula_input
                    prop:value=move || state.formula.get()
                    on:input=move |event| {
                        let text = event_target_value(&event);
                        match state.draft.get_untracked() {
                            Some(mut draft) => {
                                draft.text.clone_from(&text);
                                state.formula.set(text);
                                state.draft.set(Some(draft));
                            }
                            None => {
                                if let Some(at) = state.active.get_untracked() { state.begin_edit(at, Some(text)); }
                            }
                        }
                    }
                    on:keydown=move |event: leptos::ev::KeyboardEvent| match event.key().as_str() {
                        "Enter" => { event.prevent_default(); state.apply_draft(); }
                        "Escape" => { event.prevent_default(); state.cancel(); }
                        _ => (),
                    }/>
                {move || state.draft.get().map(|draft| {
                    view! { <span class="dna-book-seam">{format!("Model change · {}", a1(draft.at))}</span> }
                })}
            </div>
            <form class="dna-book-search" hidden=move || !state.search.get() on:submit=move |event| {
                event.prevent_default();
                if let Some(input) = search.get_untracked() {
                    let command = input.value();
                    match command.trim().to_ascii_lowercase().as_str() {
                        "undo" => state.run(Operation::Undo),
                        "redo" => state.run(Operation::Redo),
                        "calculate" => state.run(Operation::Calculate { scope: SourceRef::Workbook }),
                        "details" => state.details.update(|value| *value = !*value),
                        _ => state.goto(command),
                    }
                    state.search.set(false);
                }
            }>
                <input node_ref=search aria-label="Find a command or cell" placeholder="Command or cell address" type="search"/>
                <button>"Go"</button>
            </form>
            <main class="dna-book-stage">
                {stage}
                <Show when=move || state.details.get()>
                    <aside class="dna-book-details">
                        <h2>"Details"</h2>
                        <dl><dt>"Selected cell"</dt><dd>{move || state.active.get().map(a1).unwrap_or_default()}</dd>
                            <dt>"Entry"</dt><dd>{move || state.formula.get()}</dd></dl>
                    </aside>
                </Show>
            </main>
            <nav class="dna-book-sheets" aria-label="Worksheets">
                {move || state.summary.get().into_iter().flat_map(|book| book.sheets)
                    .filter(|sheet| sheet.visible && sheet.worksheet).map(|sheet| {
                        let id = sheet.sheet;
                        view! {
                            <button type="button" aria-current=move || (state.sheet.get() == Some(id)).to_string()
                                on:click=move |_| state.select(Key(id.0, id.1, 1, 1))>{sheet.name}</button>
                        }
                    }).collect_view()}
            </nav>
            <footer class="dna-book-strip" role="status" data-dnacalc="strip">
                <span>{move || if state.draft.get().is_some() { "Edit" } else { "Ready" }}</span>
                <span>{move || state.calculation.get()}</span>
                <span>{move || state.notice.get()}</span>
            </footer>
        </div>
    }
}

//! Level-one serving of the authoritative CalcHost workbook through the skin contract.

mod batch;
mod contributions;
mod dependencies;
mod inventory;
mod issues;
mod placement;
mod publication;
mod query;
mod rows;
mod structure;
mod survey;

use calc_host::value::{CellKey, GridArea, Selection as HostSelection};
use calc_host::{
    CommandJournal, CommandStamp, CopySnapshot, HostProgress, HostSession, SkinIntent, ValueChange,
};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::query::FormulaResultChange;
use calc_skin::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

const TOUCH_HISTORY: usize = 100;
const CALC_HISTORY: usize = 32;
const MAX_CALC_FOOTPRINT: usize = 4096;
const MAX_WATCHES: usize = 32;

fn invalid(reason: impl Into<String>) -> Refusal {
    Refusal::Invalid {
        reason: reason.into(),
    }
}

fn unavailable(owner: &str, what: &str) -> Refusal {
    Refusal::NotModelled {
        owner: owner.into(),
        what: what.into(),
    }
}

fn cell(key: Key) -> Result<CellKey, Refusal> {
    key.cell()
        .ok_or_else(|| invalid("Cell is outside the worksheet"))
}

fn grid_area(area: Area) -> Result<GridArea, Refusal> {
    area.area().ok_or_else(|| invalid("Invalid worksheet area"))
}

struct Touch {
    revision: Revision,
    // A step with no known write footprint invalidates every stale read set.
    areas: Option<Vec<GridArea>>,
    pending: Vec<GridArea>,
}

struct CalcTouch {
    generation: CalcGeneration,
    // A publication can exceed the retained footprint or come from an opaque host operation.
    areas: Option<Vec<GridArea>>,
}

struct CalculationChanges {
    stamp: Stamp,
    coverage: Coverage,
    values: Vec<FormulaResultChange>,
}

struct State {
    session: HostSession,
    selection_seq: u64,
    selection: HostSelection,
    selection_cause: SelectionCause,
    selection_sender: Option<u64>,
    touches: VecDeque<Touch>,
    calc_touches: VecDeque<CalcTouch>,
    calculation_changes: Option<CalculationChanges>,
    read_only: bool,
    opened_at: Stamp,
}

impl State {
    fn stamp(&self) -> Stamp {
        self.session.host().skin_stamp()
    }

    fn update_selection(&mut self, cause: SelectionCause) {
        self.selection = self.session.selection().clone();
        self.selection_seq += 1;
        self.selection_cause = cause;
        self.selection_sender = None;
    }

    fn selection(&self) -> Selection {
        let active = self.selection.active;
        Selection {
            seq: self.selection_seq,
            target: active.map_or(SourceRef::Workbook, |at| SourceRef::Cells {
                sheet: Sheet::of(at.sheet()),
                areas: self.selection.areas.iter().copied().map(Area::of).collect(),
            }),
            active: active.map(Key::of),
            grouped: Vec::new(),
            item: None,
        }
    }

    fn record_touch(&mut self, before: Stamp, areas: Option<Vec<GridArea>>, pending: Vec<CellKey>) {
        let revision = self.stamp().model;
        if revision != before.model {
            self.touches.push_back(Touch {
                revision,
                areas: if revision.0 == before.model.0 + 1 {
                    areas
                } else {
                    None
                },
                pending: pending.into_iter().map(|at| self.result_area(at)).collect(),
            });
            if self.touches.len() > TOUCH_HISTORY {
                self.touches.pop_front();
            }
        }
    }

    fn result_area(&self, at: CellKey) -> GridArea {
        self.session
            .host()
            .spill_area(at)
            .unwrap_or_else(|| GridArea::cell(at))
    }

    fn record_calculation(&mut self, before: Stamp, progress: Option<&HostProgress>) {
        let stamp = self.stamp();
        let generation = stamp.calc;
        if generation == before.calc {
            return;
        }
        self.calculation_changes = match progress {
            Some(HostProgress::Published(delta)) if generation.0 == before.calc.0 + 1 => {
                delta.differences().map(|differences| CalculationChanges {
                    stamp,
                    coverage: if differences.changes.len() as u64 == differences.total {
                        Coverage::Complete
                    } else {
                        Coverage::Partial {
                            done: differences.changes.len() as u64,
                            total: Some(differences.total),
                            why: PartialWhy::Budget,
                        }
                    },
                    values: differences
                        .changes
                        .iter()
                        .map(|change| FormulaResultChange {
                            at: Key::of(change.at),
                            before: query::result_observation(change.before.as_ref()),
                            after: query::result_observation(change.after.as_ref()),
                        })
                        .collect(),
                })
            }
            _ => None,
        };
        let areas = match progress {
            Some(HostProgress::Published(delta))
                if generation.0 == before.calc.0 + 1
                    && delta.changes().len() + 2 * delta.spill_changes().len()
                        <= MAX_CALC_FOOTPRINT
                    && self
                        .session
                        .host()
                        .model()
                        .definitions()
                        .pivots()
                        .tables()
                        .next()
                        .is_none() =>
            {
                let changed = delta.changes().iter().map(|change| match *change {
                    ValueChange::Published { at, .. } | ValueChange::Removed { at, .. } => {
                        self.result_area(at)
                    }
                });
                let shapes = delta
                    .spill_changes()
                    .iter()
                    .flat_map(|change| [change.before, change.after].into_iter().flatten());
                Some(changed.chain(shapes).collect())
            }
            _ => None,
        };
        self.calc_touches.push_back(CalcTouch { generation, areas });
        if self.calc_touches.len() > CALC_HISTORY {
            self.calc_touches.pop_front();
        }
    }

    fn check_base(&self, request: &Batch) -> Result<(), Refusal> {
        if request
            .workbook
            .is_some_and(|workbook| workbook != self.stamp().workbook)
        {
            return Err(invalid("This edit belongs to a different workbook"));
        }
        let Some(base) = request.base else {
            return Ok(());
        };
        let current = self.stamp().model;
        if base == current {
            return Ok(());
        }
        if base > current
            || self
                .touches
                .front()
                .is_none_or(|oldest| oldest.revision.0 > base.0 + 1)
        {
            return Err(Refusal::Conflict {
                base,
                conflicts: vec![Target::Source(SourceRef::Workbook)],
            });
        }

        let mut expects = request.expects.clone();
        for change in &request.changes {
            match change {
                Change::Model(
                    ModelChange::Enter { at, .. } | ModelChange::RewriteFormula { at, .. },
                ) => {
                    expects.push(batch::source_area(GridArea::cell(cell(*at)?)));
                }
                Change::Model(ModelChange::EnterBound { bound, .. }) => {
                    expects.push(batch::source_area(GridArea::cell(query::bound_cell(self, bound)?)));
                }
                Change::Model(ModelChange::ClearContents { areas }) => {
                    for area in areas {
                        expects.push(batch::source_area(grid_area(*area)?));
                    }
                }
                Change::Model(ModelChange::RewriteFormulaRun { extent, .. }) => {
                    expects.push(batch::source_area(grid_area(*extent)?));
                }
                _ => {}
            }
        }
        let mut conflicts = Vec::new();
        for source in expects {
            let areas = query::source_areas(self, &source).ok();
            let intersects = self
                .touches
                .iter()
                .filter(|touch| touch.revision > base)
                .any(|touch| match (&touch.areas, &areas) {
                    (Some(writes), Some(reads)) => reads.iter().any(|read| {
                        writes
                            .iter()
                            .any(|write| write.intersection(*read).is_some())
                    }),
                    _ => true,
                });
            let target = Target::Source(source);
            if intersects && !conflicts.contains(&target) {
                conflicts.push(target);
            }
        }
        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(Refusal::Conflict { base, conflicts })
        }
    }
}

pub struct SkinHost {
    state: Rc<RefCell<State>>,
    next_connection: Cell<u64>,
}

impl SkinHost {
    pub fn new(session: HostSession) -> Self {
        Self::with_read_only(session, false)
    }

    pub fn with_read_only(mut session: HostSession, read_only: bool) -> Self {
        session.observe_result_differences();
        Self::initial_selection(&mut session);
        let opened_at = session.host().skin_stamp();
        Self {
            next_connection: Cell::new(1),
            state: Rc::new(RefCell::new(State {
                selection: session.selection().clone(),
                session,
                selection_seq: 1,
                selection_cause: SelectionCause::Open,
                selection_sender: None,
                touches: VecDeque::new(),
                calc_touches: VecDeque::new(),
                calculation_changes: None,
                read_only,
                opened_at,
            })),
        }
    }

    fn initial_selection(session: &mut HostSession) {
        if session.selection().active.is_none()
            && let Some(sheet) = session.host().sheets().iter().find(|sheet| {
                sheet.visibility() == calc_doc_model::SheetVisibility::Visible
                    && sheet.kind() == calc_doc_model::SheetKind::Worksheet
            })
        {
            let at = CellKey::new(sheet.id(), 1, 1).expect("A1 is in the worksheet");
            session.dispatch(SkinIntent::Select(HostSelection::cell(at)));
        }
    }

    pub fn connect(&self, origin: Origin, policy: WritePolicy) -> Connection {
        let id = self.next_connection.get();
        self.next_connection.set(id + 1);
        Connection {
            id,
            host: self.state.clone(),
            origin,
            policy,
            formula: RefCell::default(),
            local: RefCell::new(Local {
                journal: CommandJournal::new(128, 1024 * 1024),
                copied: None,
                watches: BTreeMap::new(),
                last_stamp: None,
                last_selection: 0,
                last_rtd: 0,
                next_watch: 1,
                next_publication: 1,
                awaiting_tiles: None,
                last_client_seq: 0,
            }),
        }
    }

    /// Drive one host calculation slice. A carrier calls this while work remains.
    pub fn advance(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.session.has_pending_calculation() {
            let before = state.stamp();
            let progress = state.session.advance_calculation();
            state.record_calculation(before, Some(&progress));
        }
        state.session.has_pending_calculation()
    }

    /// Lets the native application perform its existing file or automation operation on this session.
    pub fn with_session<R>(
        &self,
        cause: SelectionCause,
        apply: impl FnOnce(&mut HostSession) -> R,
    ) -> R {
        let mut state = self.state.borrow_mut();
        let before = state.stamp();
        let result = apply(&mut state.session);
        if &state.selection != state.session.selection() {
            state.update_selection(cause);
        }
        state.record_touch(before, None, Vec::new());
        state.record_calculation(before, None);
        result
    }
}

type AnswerResult = Result<Answer<serde_json::Value>, Refusal>;

struct Watch {
    query: WorkbookQuery,
    budget: Budget,
    answer: Option<AnswerResult>,
    footprint: Option<GridArea>,
}

struct Local {
    journal: CommandJournal<Result<Stamp, Refusal>>,
    copied: Option<CopiedCells>,
    watches: BTreeMap<WatchId, Watch>,
    last_stamp: Option<Stamp>,
    last_selection: u64,
    last_rtd: u64,
    next_watch: u64,
    next_publication: u64,
    awaiting_tiles: Option<u64>,
    last_client_seq: u64,
}

struct CopiedCells {
    workbook: WorkbookId,
    id: u64,
    snapshot: CopySnapshot,
}

pub struct Connection {
    id: u64,
    host: Rc<RefCell<State>>,
    origin: Origin,
    policy: WritePolicy,
    formula: RefCell<Option<calc_formula_host::FormulaHost>>,
    local: RefCell<Local>,
}

impl Connection {
    fn formula_request(
        &self,
        request: impl FnOnce(&mut calc_formula_host::FormulaHost) -> Result<FormulaAnswer, Refusal>,
    ) -> Result<WorkbookReply, Refusal> {
        if !matches!(self.origin, Origin::User { .. }) {
            return Err(Refusal::PolicyRefused);
        }
        let mut host = self.formula.borrow_mut();
        request(host.get_or_insert_with(Default::default)).map(WorkbookReply::Formula)
    }

    fn check_write(&self, state: &State) -> Result<(), Refusal> {
        if state.read_only {
            Err(Refusal::ReadOnly)
        } else if matches!(self.policy, WritePolicy::Look | WritePolicy::Propose) {
            Err(Refusal::PolicyRefused)
        } else {
            Ok(())
        }
    }

    fn select_source(
        &self,
        state: &mut State,
        target: SourceRef,
        active: Option<Key>,
        cause: SelectionCause,
    ) -> Result<(), Refusal> {
        let areas = query::source_areas(state, &target)?;
        let first = areas.first().ok_or_else(|| invalid("Selection is empty"))?;
        if areas.iter().any(|area| area.sheet() != first.sheet()) {
            return Err(invalid("Selection areas must share one worksheet"));
        }
        let active = active
            .map(cell)
            .transpose()?
            .unwrap_or_else(|| first.first());
        if !areas.iter().any(|area| area.contains(active)) {
            return Err(invalid("The active cell is outside selection"));
        }
        let receipt = state.session.dispatch(SkinIntent::Select(HostSelection {
            active: Some(active),
            areas,
        }));
        if let Some(error) = receipt.error {
            return Err(batch::refusal(error));
        }
        state.update_selection(cause);
        Ok(())
    }
}

impl WorkbookPort for Connection {
    fn capabilities(&self) -> Capabilities {
        query::capabilities(&self.host.borrow())
    }

    fn ask(&self, query: WorkbookQuery, budget: Budget) -> AnswerResult {
        query::answer(&self.host.borrow(), &query, budget)
    }

    fn watch(&self, query: WorkbookQuery, budget: Budget) -> Result<WatchId, Refusal> {
        let mut local = self.local.borrow_mut();
        if local.watches.len() >= MAX_WATCHES {
            return Err(invalid("Too many workbook watches"));
        }
        let id = WatchId(local.next_watch);
        local.next_watch += 1;
        local.watches.insert(
            id,
            Watch {
                query,
                budget,
                answer: None,
                footprint: None,
            },
        );
        Ok(id)
    }

    fn unwatch(&self, watch: WatchId) {
        self.local.borrow_mut().watches.remove(&watch);
    }

    fn select(&self, selection: Selection, client_seq: u64) -> Result<(), Refusal> {
        if !selection.grouped.is_empty() || selection.item.is_some() {
            return Err(unavailable(
                "dc-s2xup",
                "Group or presentation-item selection",
            ));
        }
        let mut local = self.local.borrow_mut();
        if client_seq <= local.last_client_seq {
            return Err(Refusal::StaleSelection);
        }
        local.last_client_seq = client_seq;
        let mut state = self.host.borrow_mut();
        self.select_source(
            &mut state,
            selection.target,
            selection.active,
            SelectionCause::ClientEcho { client_seq },
        )?;
        state.selection_sender = Some(self.id);
        Ok(())
    }

    fn navigate(&self, target: Target) -> Result<(), Refusal> {
        let mut state = self.host.borrow_mut();
        let source = match target {
            Target::Source(source) => source,
            Target::Anchor(id) => query::anchor_source(&state, id)?,
            Target::Item(_) => {
                return Err(unavailable("dc-aje2v", "Navigation to presentation items"));
            }
        };
        let cause = match &self.origin {
            Origin::Agent { agent, .. } => SelectionCause::Agent {
                agent: agent.clone(),
            },
            Origin::Macro { procedure, .. } => SelectionCause::Macro {
                procedure: procedure.clone(),
            },
            _ => SelectionCause::Open,
        };
        self.select_source(&mut state, source, None, cause)
    }

    fn preview(&self, request: &Batch, depth: PreviewDepth) -> Result<Preview, Refusal> {
        if !matches!(depth, PreviewDepth::WritesOnly) {
            return Err(unavailable("dc-zh096", "Preview with calculation effects"));
        }
        let mut state = self.host.borrow_mut();
        let base = state.stamp();
        let local = self.local.borrow();
        Ok(
            match self
                .check_write(&state)
                .and_then(|()| batch::prepare(&mut state, request, local.copied.as_ref()))
            {
                Ok(staged) => staged.preview,
                Err(refusal) => Preview {
                    base,
                    writes: Vec::new(),
                    properties: Vec::new(),
                    presentation: Vec::new(),
                    warnings: Vec::new(),
                    refusal: Some(refusal),
                },
            },
        )
    }

    fn commit(&self, request: Batch) -> Result<Stamp, Refusal> {
        let mut state = self.host.borrow_mut();
        self.check_write(&state)?;
        let id = request
            .id
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0 && id.to_string() == request.id)
            .ok_or_else(|| invalid("A command ID must be a positive serial number"))?;
        let payload =
            serde_json::to_string(&request).map_err(|error| invalid(error.to_string()))?;
        let before = state.stamp();
        let mut local = self.local.borrow_mut();
        let command = CommandStamp {
            id,
            workbook: before.workbook.0,
            base_revision: before.model.0,
        };
        if let Some(receipt) = local
            .journal
            .admit(command, before.workbook.0, before.model.0, &payload)
            .map_err(|error| invalid(error.to_string()))?
        {
            return receipt;
        }
        let result =
            (|| {
                let prepared = batch::prepare(&mut state, &request, local.copied.as_ref())?;
                let Some(kind) = prepared.preview.kind() else {
                    return Ok(before);
                };
                let mut targets = Vec::new();
                let mut cells_touched = BTreeSet::new();
                for write in &prepared.preview.writes {
                    cells_touched.insert(write.at);
                    targets.push(Target::Source(batch::source_area(GridArea::cell(cell(
                        write.at,
                    )?))));
                }
                let mut formatted = Vec::new();
                let mut scope_changed = false;
                for property in &prepared.preview.properties {
                    match property {
                        ModelPropertyDiff::WorksheetInserted { sheet, .. } => {
                            scope_changed = true;
                            targets.push(Target::Source(SourceRef::Sheet(*sheet)));
                        }
                        ModelPropertyDiff::CellFormat { at, .. } => {
                            let area = GridArea::cell(cell(*at)?);
                            formatted.push(area);
                            if cells_touched.insert(*at) {
                                targets.push(Target::Source(batch::source_area(area)));
                            }
                        }
                        ModelPropertyDiff::Scope { source, .. } => {
                            scope_changed = true;
                            let target = Target::Source(source.clone());
                            if !targets.contains(&target) {
                                targets.push(target);
                            }
                        }
                    }
                }
                for change in &prepared.preview.presentation {
                    let anchor = match change {
                        PresentationChange::Bind { anchor, .. }
                        | PresentationChange::RemoveAnchor { anchor }
                        | PresentationChange::PutNote { anchor, .. } => Some(*anchor),
                        PresentationChange::RemoveNote { id } => state
                            .session
                            .host()
                            .model()
                            .view_part()
                            .note(*id)
                            .map(|note| note.anchor),
                        _ => None,
                    };
                    if let Some(anchor) = anchor {
                        let target = Target::Anchor(anchor);
                        if !targets.contains(&target) {
                            targets.push(target);
                        }
                    }
                }
                let label = request.label.clone().unwrap_or_else(|| {
                    match prepared.preview.writes.as_slice() {
                        [write] => format!(
                            "Entered {}{}",
                            address::column_letters(write.at.3),
                            write.at.2
                        ),
                        [] => "Edited layout".into(),
                        writes => format!("Edited {} cells", writes.len()),
                    }
                });
                let entry = HistoryEntry {
                    id: EntryId(0),
                    at: before,
                    label,
                    origin: self.origin.clone(),
                    kind,
                    targets,
                    cells_touched: cells_touched.len() as u64,
                    reversibility: Reversibility::Reversible,
                    group: None,
                    unreviewed: self.policy == WritePolicy::ApplyThenReview,
                };
                let receipt = state.session.dispatch_skin_edit(prepared.edits, entry);
                if let Some(error) = receipt.error {
                    return Err(batch::refusal(error));
                }
                let pending = receipt
                    .committed
                    .as_ref()
                    .map_or_else(Vec::new, |commit| commit.pending().to_vec());
                state.record_calculation(before, receipt.recalc.as_ref());
                let touched = if scope_changed {
                    None
                } else {
                    receipt.committed.map(|commit| {
                        formatted.extend_from_slice(commit.input_writes());
                        formatted
                    })
                };
                state.record_touch(before, touched, pending);
                if &state.selection != state.session.selection() {
                    state.update_selection(SelectionCause::History);
                }
                Ok(state.stamp())
            })();
        local.journal.remember(id, payload, result.clone(), 512);
        result
    }

    fn run(&self, operation: Operation) -> Result<Stamp, Refusal> {
        let mut state = self.host.borrow_mut();
        let before = state.stamp();
        let intent = match operation {
            Operation::Undo => {
                self.check_write(&state)?;
                SkinIntent::Undo
            }
            Operation::Redo => {
                self.check_write(&state)?;
                SkinIntent::Redo
            }
            Operation::CancelCalculation => {
                state.session.cancel_calculation();
                return Ok(state.stamp());
            }
            Operation::Calculate {
                scope: SourceRef::Workbook,
            } => SkinIntent::Recalculate,
            Operation::Calculate { scope } => SkinIntent::CalculateRange {
                areas: query::source_areas(&state, &scope)?,
            },
            Operation::Revert(_) => {
                return Err(unavailable("dc-zh096", "Reverting an earlier edit"));
            }
            Operation::CopyCells { workbook, copy, from } => {
                if workbook != before.workbook {
                    return Err(Refusal::StaleSelection);
                }
                let from = grid_area(from)?;
                let mut local = self.local.borrow_mut();
                if let Some(previous) = local.copied.as_ref().filter(|c| c.workbook == workbook) {
                    if copy == previous.id && from == previous.snapshot.area() {
                        return Ok(before);
                    }
                    if copy <= previous.id {
                        return Err(invalid("The clipboard request has expired."));
                    }
                }
                let snapshot = CopySnapshot::capture(state.session.host(), from)
                    .map_err(|error| Refusal::Excel { reason: error.to_string() })?;
                local.copied = Some(CopiedCells { workbook, id: copy, snapshot });
                return Ok(before);
            }
        };
        let receipt = state.session.dispatch(intent);
        if let Some(error) = receipt.error {
            return Err(batch::refusal(error));
        }
        if &state.selection != state.session.selection() {
            state.update_selection(SelectionCause::History);
        }
        let pending = receipt
            .committed
            .as_ref()
            .map_or_else(Vec::new, |commit| commit.pending().to_vec());
        state.record_calculation(before, receipt.recalc.as_ref());
        state.record_touch(
            before,
            receipt.committed.and_then(|change| {
                (!change.input_writes().is_empty()).then(|| change.input_writes().to_vec())
            }),
            pending,
        );
        Ok(state.stamp())
    }

    fn next_frame(&self) -> Result<Option<Frame>, Refusal> {
        let state = self.host.borrow();
        if !state.session.screen_updating() {
            return Ok(None);
        }
        let mut local = self.local.borrow_mut();
        let stamp = state.stamp();
        let mut updates = Vec::new();
        let mut events = Vec::new();
        let prior = local.last_stamp;
        let selection_changed = local.last_selection != state.selection_seq;
        let rtd = state.session.host().rtd_revision();
        let rtd_changed = local.last_rtd != rtd;
        for (&watch, slot) in &mut local.watches {
            if slot.answer.is_some()
                && !query::invalidated(
                    &state,
                    &slot.query,
                    slot.footprint,
                    prior,
                    selection_changed,
                    rtd_changed,
                )
            {
                continue;
            }
            let answer = query::answer(&state, &slot.query, slot.budget);
            let same = prior.is_some_and(|previous| previous.workbook == stamp.workbook)
                && match (slot.answer.as_ref(), &answer) {
                    (Some(Ok(old)), Ok(new)) => {
                        old.coverage == new.coverage && old.body == new.body
                    }
                    (Some(Err(old)), Err(new)) => old == new,
                    _ => false,
                };
            if !same {
                updates.push(match &answer {
                    Ok(answer) => Update::Ready {
                        watch,
                        coverage: answer.coverage.clone(),
                        body: answer.body.clone(),
                    },
                    Err(reason) => Update::Refused {
                        watch,
                        reason: reason.clone(),
                    },
                });
            }
            slot.answer = Some(answer);
            slot.footprint = query::footprint(&state, &slot.query);
        }
        if selection_changed {
            let cause = if state
                .selection_sender
                .is_some_and(|sender| sender != self.id)
            {
                SelectionCause::Open
            } else {
                state.selection_cause.clone()
            };
            events.push(Event::Selection {
                selection: state.selection(),
                cause,
            });
        }
        local.last_stamp = Some(stamp);
        local.last_selection = state.selection_seq;
        local.last_rtd = rtd;
        if updates.is_empty() && events.is_empty() && prior == Some(stamp) {
            return Ok(None);
        }
        Ok(Some(Frame {
            stamp,
            events,
            updates,
        }))
    }
}

impl Connection {
    /// Dispatch one control packet on this connection. The origin and write policy stay at the host.
    pub fn handle(&self, packet: Packet) -> Packet {
        let request = packet.envelope.request;
        let result = match (packet.envelope.message, packet.bytes) {
            (Message::WorkbookRequest(WorkbookRequest::Open { name }), Some(bytes)) => {
                self.open(name, bytes).map(WorkbookReply::Stamp)
            }
            (Message::WorkbookRequest(message), None) => self.request(message),
            _ => Err(invalid("Unexpected workbook packet")),
        };
        let message = match result {
            Ok(reply) => Message::WorkbookReply(reply),
            Err(reason) => Message::Failure(reason),
        };
        Packet {
            envelope: Envelope::new(request, message),
            bytes: None,
        }
    }

    /// Produce one atomic update. A connection has one binary publication in flight.
    pub fn next_packets(&self) -> Result<Vec<Packet>, Refusal> {
        if self.local.borrow().awaiting_tiles.is_some() {
            return Ok(Vec::new());
        }
        let Some(mut frame) = self.next_frame()? else {
            return Ok(Vec::new());
        };
        let mut local = self.local.borrow_mut();
        let sequence = local.next_publication;
        let mut packets = Vec::new();
        let mut remaining = calc_skin::tiles::MAX_PUBLICATION_BYTES;
        for update in &mut frame.updates {
            let Update::Ready {
                watch,
                coverage,
                body,
            } = update
            else {
                continue;
            };
            if !matches!(
                local.watches.get(watch).map(|slot| &slot.query),
                Some(WorkbookQuery::Cells { .. })
            ) {
                continue;
            }
            let result = serde_json::from_value(body.take())
                .map_err(|error| error.to_string())
                .and_then(|window| {
                    publication::window(frame.stamp, *watch, sequence, window, remaining)
                });
            match result {
                Ok((mut batch, retained)) => {
                    remaining -= retained;
                    let Some(Message::Tile(stamp)) =
                        batch.last().map(|part| &part.envelope.message)
                    else {
                        return Err(Refusal::Transport("No binary window completion".into()));
                    };
                    *update = Update::Tiles {
                        watch: *watch,
                        coverage: coverage.clone(),
                        barrier: stamp.barrier(),
                    };
                    packets.append(&mut batch);
                }
                Err(reason) => {
                    *update = Update::Refused {
                        watch: *watch,
                        reason: Refusal::Transport(reason),
                    }
                }
            }
        }
        if !packets.is_empty() {
            local.awaiting_tiles = Some(sequence);
            local.next_publication += 1;
        }
        packets.push(Packet {
            envelope: Envelope::new(0, Message::Frame(frame)),
            bytes: None,
        });
        Ok(packets)
    }

    fn open(&self, name: String, bytes: Vec<u8>) -> Result<Stamp, Refusal> {
        if !matches!(self.policy, WritePolicy::Commit)
            || !matches!(self.origin, Origin::User { .. })
        {
            return Err(Refusal::PolicyRefused);
        }
        let locale = self.host.borrow().session.host().entry_environment().profile;
        let mut session = calc_host::HostSession::open_named(
            std::sync::Arc::new(calc_host::MemorySource::new(bytes)),
            &calc_host::OpenPolicy::default(),
            calc_host::HostOptions {
                locale,
                ..calc_host::HostOptions::default()
            },
            &name,
        )
        .map_err(|error| Refusal::Excel {
            reason: format!("{error:?}"),
        })?;
        session.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
        session.observe_result_differences();
        SkinHost::initial_selection(&mut session);
        let stamp = session.host().skin_stamp();
        let mut state = self.host.borrow_mut();
        state.session = session;
        state.opened_at = stamp;
        state.touches.clear();
        state.calc_touches.clear();
        state.calculation_changes = None;
        state.update_selection(SelectionCause::Open);
        self.local.borrow_mut().copied = None;
        Ok(stamp)
    }

    fn request(&self, message: WorkbookRequest) -> Result<WorkbookReply, Refusal> {
        Ok(match message {
            WorkbookRequest::Open { .. } => return Err(invalid("The workbook file is missing")),
            WorkbookRequest::Ask { query, budget } => {
                WorkbookReply::Answer(self.ask(query, budget)?)
            }
            WorkbookRequest::Watch { query, budget } => {
                WorkbookReply::Watching(self.watch(query, budget)?)
            }
            WorkbookRequest::Unwatch(id) => {
                self.unwatch(id);
                WorkbookReply::Accepted
            }
            WorkbookRequest::AcknowledgeTiles { transaction } => {
                let mut local = self.local.borrow_mut();
                if local.awaiting_tiles == Some(transaction) {
                    local.awaiting_tiles = None;
                }
                WorkbookReply::Accepted
            }
            WorkbookRequest::ResetTiles => {
                let mut local = self.local.borrow_mut();
                local.awaiting_tiles = None;
                for watch in local.watches.values_mut() {
                    if matches!(watch.query, WorkbookQuery::Cells { .. }) {
                        watch.answer = None;
                    }
                }
                WorkbookReply::Accepted
            }
            WorkbookRequest::Select {
                selection,
                client_seq,
            } => {
                self.select(selection, client_seq)?;
                WorkbookReply::Accepted
            }
            WorkbookRequest::Navigate(target) => {
                self.navigate(target)?;
                WorkbookReply::Accepted
            }
            WorkbookRequest::Preview { batch, depth } => {
                WorkbookReply::Preview(self.preview(&batch, depth)?)
            }
            WorkbookRequest::Commit(batch) => WorkbookReply::Stamp(self.commit(batch)?),
            WorkbookRequest::Run(operation) => WorkbookReply::Stamp(self.run(operation)?),
            WorkbookRequest::FormulaQuery(query) => self.formula_request(|host| host.handle(query))?,
            WorkbookRequest::FormulaOperation(operation) => {
                self.formula_request(|host| host.operate(operation))?
            }
        })
    }
}

//! Level-one serving of the authoritative CalcHost workbook through the skin contract.

mod batch;
mod comparison;
mod contributions;
mod dependencies;
mod effects;
mod inventory;
mod issues;
mod names;
mod placement;
mod proposal;
mod publication;
mod query;
mod rows;
mod snapshot;
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
use std::sync::Arc;

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
    execution: Option<(Stamp, calc_skin::query::ExecutionReport)>,
    iteration: Option<(Stamp, IterationFacts)>,
    read_only: bool,
    opened_at: Stamp,
    baseline: Option<comparison::Baseline>,
    baseline_revision: u64,
    look: Option<LookOffer>,
    look_revision: u64,
    proposals: BTreeMap<ProposalId, proposal::StagedProposal>,
    proposal_revision: u64,
    clock: Option<Rc<calc_host::ModelClock>>,
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
        self.execution = match progress {
            Some(HostProgress::Published(delta)) if generation.0 == before.calc.0 + 1 => {
                delta.events().iter().find_map(|event| {
                    let calc_host::CalcEvent::ExecutionObserved(trace) = event else { return None };
                    Some((stamp, calc_skin::query::ExecutionReport {
                        total_entries: trace.total_entries,
                        finished: delta.completion().is_some(),
                        bands: trace.bands.iter().map(|band| calc_skin::query::ExecutionBand {
                            pass: band.pass,
                            workers: band.workers,
                            cells: band.cells.iter().copied().map(Key::of).collect(),
                        }).collect(),
                    }))
                })
            }
            _ => None,
        };
        self.iteration = match progress {
            Some(HostProgress::Published(delta)) if generation.0 == before.calc.0 + 1 => {
                delta.events().iter().rev().find_map(|entry| {
                    let calc_host::CalcEvent::IterationObserved(observed) = entry else { return None };
                    let stop = match observed.stop {
                        calc_host::IterationStop::MaximumIterations => IterationStop::MaximumIterations,
                        calc_host::IterationStop::Converged => IterationStop::Converged,
                        calc_host::IterationStop::NoNumericSample => IterationStop::NoNumericSample,
                    };
                    Some((stamp, IterationFacts {
                        members: observed.members.iter().copied().map(Key::of).collect(),
                        omitted_members: observed.omitted_members,
                        passes: observed.passes,
                        residual_bits: observed.residual_bits,
                        max_change_bits: observed.max_change_bits,
                        stop,
                        reads: observed.reads.iter().map(|read| IterationRead {
                            reader: Key::of(read.reader),
                            source: Key::of(read.source),
                            current_pass: read.kind == calc_host::IterationReadKind::CurrentPass,
                        }).collect(),
                        reads_limited: observed.reads_limited,
                    }))
                })
            }
            _ => None,
        };
        self.calculation_changes = match progress {
            Some(HostProgress::Published(delta)) if generation.0 == before.calc.0 + 1 => {
                delta.differences().map(|differences| CalculationChanges {
                    stamp,
                    coverage: if !differences.followers_limited && differences.changes.len() as u64 == differences.total {
                        Coverage::Complete
                    } else {
                        Coverage::Partial {
                            done: differences.changes.len() as u64,
                            total: (!differences.followers_limited).then_some(differences.total),
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
                            spill_anchor: change.spill_anchor.map(Key::of),
                            aliases: Vec::new(),
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
                diffs: Vec::new(),
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
            Err(Refusal::Conflict { base, conflicts, diffs: Vec::new() })
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
        session.observe_rtd_receipt_times(snapshot::captured_unix_ms);
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
                execution: None,
                iteration: None,
                read_only,
                opened_at,
                baseline: None,
                baseline_revision: 0,
                look: None,
                look_revision: 0,
                proposals: BTreeMap::new(),
                proposal_revision: 0,
                clock: None,
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
                last_baseline: 0,
                last_look: self.state.borrow().look_revision,
                last_proposal: 0,
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

    /// Adds the local RTD clock for this workbook and subsequent opens on the carrier.
    pub fn enable_model_clock(&self) {
        let mut state = self.state.borrow_mut();
        if state.clock.is_none() {
            state.clock = Some(state.session.install_model_clock());
        }
    }

    /// Publish due values through the workbook's ordinary sequenced RTD input.
    pub fn advance_model_clock(&self, elapsed: std::time::Duration) -> bool {
        let mut state = self.state.borrow_mut();
        let Some(clock) = &state.clock else {
            return false;
        };
        let updates = clock.advance(elapsed, !state.session.has_unfinished_calculation());
        let mut changed = false;
        for update in updates {
            changed |= state.session.update_rtd(
                update.topic, update.lease, update.sequence, update.value,
            ) == calc_host::RtdDisposition::Accepted;
        }
        changed
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
    last_baseline: u64,
    last_look: u64,
    last_proposal: u64,
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
        let mut state = self.host.borrow_mut();
        let base = state.stamp();
        let local = self.local.borrow();
        let access = if !state.read_only && self.policy == WritePolicy::Propose
            && matches!(self.origin, Origin::Agent { .. })
        {
            Ok(())
        } else {
            self.check_write(&state)
        };
        let mut staged = match access.and_then(|()| {
            batch::prepare(&mut state, request, local.copied.as_ref())
        }) {
            Ok(staged) => staged,
            Err(refusal) => return Ok(Preview {
                base,
                writes: Vec::new(),
                properties: Vec::new(),
                effects: Vec::new(),
                presentation: Vec::new(),
                warnings: Vec::new(),
                refusal: Some(refusal),
            }),
        };
        if let PreviewDepth::WithEffects { budget } = depth {
            effects::append(state.session.host(), &mut staged.preview, staged.after, budget);
        }
        Ok(staged.preview)
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
                    let source = match property {
                        ModelPropertyDiff::CellMoved { from, to } => {
                            for at in [*from, *to] {
                                let target = Target::Source(batch::source_area(GridArea::cell(cell(at)?)));
                                if !targets.contains(&target) {
                                    targets.push(target);
                                }
                            }
                            continue;
                        }
                        ModelPropertyDiff::DefinedName { scope, before, after } => {
                            scope_changed = true;
                            for name in before.iter().chain(after) {
                                let target = Target::Source(SourceRef::Name {
                                    scope: scope.clone(), name: name.name.clone(),
                                });
                                if !targets.contains(&target) {
                                    targets.push(target);
                                }
                            }
                            continue;
                        }
                        ModelPropertyDiff::WorksheetInserted { sheet, .. } => SourceRef::Sheet(*sheet),
                        ModelPropertyDiff::CellFormat { at, .. } => {
                            let area = GridArea::cell(cell(*at)?);
                            formatted.push(area);
                            if cells_touched.insert(*at) {
                                targets.push(Target::Source(batch::source_area(area)));
                            }
                            continue;
                        }
                        ModelPropertyDiff::Scope { source, .. } => source.clone(),
                        ModelPropertyDiff::AddedScenario { scenario, .. }
                        | ModelPropertyDiff::EditedScenario { scenario, .. } => SourceRef::Sheet(scenario.sheet),
                    };
                    scope_changed = true;
                    let target = Target::Source(source);
                    if !targets.contains(&target) {
                        targets.push(target);
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
                    proposal_unit: None,
                    unreviewed: self.policy == WritePolicy::ApplyThenReview,
                };
                let receipt = match prepared.change {
                    batch::PreparedChange::Paste(paste) => state.session.dispatch_skin_paste(paste, entry),
                    batch::PreparedChange::Edits(edits) => state.session.dispatch_skin_edit(edits, entry),
                };
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
            Operation::Propose(declaration) => {
                return proposal::offer(&mut state, &self.origin, self.policy, declaration);
            }
            Operation::Decide { proposal, accept, reject } => {
                if !matches!(self.origin, Origin::User { .. }) {
                    return Err(Refusal::PolicyRefused);
                }
                self.check_write(&state)?;
                return proposal::decide(&mut state, &proposal, &accept, &reject);
            }
            Operation::TryOn { expected, look } => {
                let Origin::Agent { agent, proposal } = &self.origin else {
                    return Err(Refusal::PolicyRefused);
                };
                if expected.workbook != before.workbook
                    || expected.model != before.model
                    || expected.view != before.view
                {
                    return Err(invalid("The workbook changed. Submit the look again."));
                }
                match &look {
                    TryOnPayload::Skin(skin) => {
                        if skin.level != Level::Workbook {
                            return Err(invalid("Choose a workbook skin"));
                        }
                        for page in skin.pages.values() {
                            query::verify_page_bindings(&state, page)?;
                        }
                    }
                    TryOnPayload::Page(page) => query::verify_page_bindings(&state, page)?,
                }
                state.look_revision += 1;
                state.look = Some(LookOffer {
                    at: before,
                    agent: agent.clone(),
                    proposal: proposal.clone(),
                    look,
                });
                return Ok(before);
            }
            Operation::PinBaseline { expected, bounds, budget } => {
                if expected != before {
                    return Err(Refusal::StaleSelection);
                }
                if !matches!(self.origin, Origin::User { .. }) {
                    return Err(invalid("A workbook user can pin a baseline"));
                }
                state.baseline = Some(comparison::Baseline::capture(&state, &bounds, budget)?);
                state.baseline_revision += 1;
                return Ok(before);
            }
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
            Operation::Calculate {
                scope: SourceRef::Sheet(sheet),
            } => SkinIntent::Calculate {
                scope: calc_host::CalcScope::Sheet(sheet.id()),
                selection: calc_host::EvalSelection::ForceAll,
            },
            Operation::Calculate { scope } => SkinIntent::CalculateRange {
                areas: query::source_areas(&state, &scope)?,
            },
            Operation::CalculateFull => SkinIntent::Calculate {
                scope: calc_host::CalcScope::Workbook,
                selection: calc_host::EvalSelection::ForceAll,
            },
            Operation::CalculateRebuild => SkinIntent::Rebuild,
            Operation::CalculateObserved => SkinIntent::CalculateObserved,
            Operation::Revert(_) => {
                return Err(unavailable("dc-zh096", "Reverting an earlier edit"));
            }
            Operation::ToggleGridlines { workbook, sheet } => {
                self.check_write(&state)?;
                if workbook != before.workbook {
                    return Err(Refusal::StaleSelection);
                }
                state.session.toggle_gridlines(sheet.id()).map_err(batch::refusal)?;
                return Ok(state.stamp());
            }
            Operation::ToggleFreezePanes { workbook, at, off_origin } => {
                self.check_write(&state)?;
                let at = cell(at)?;
                if workbook != before.workbook || state.selection.active != Some(at) {
                    return Err(Refusal::StaleSelection);
                }
                let sheet = at.sheet();
                let view = state.session.host().sheet_view(sheet)
                    .ok_or_else(|| invalid("The worksheet is unavailable"))?;
                let (rows, columns) = if view.frozen.is_some() {
                    (0, 0)
                } else {
                    if off_origin || view.top_left.is_some_and(|origin| origin != (1, 1)) {
                        return Err(unavailable("dc-s2xup", "Freezing from a scrolled worksheet (Return to the top-left to freeze at the active cell)"));
                    }
                    if (at.row(), at.column()) == (1, 1) {
                        return Err(unavailable("dc-s2xup", "Freezing at the first cell using the window midpoint"));
                    }
                    (at.row() - 1, at.column() - 1)
                };
                SkinIntent::Command(calc_host::WorkbookCommand::FreezePanes { sheet, rows, columns })
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
        let definitions = state.session.host().model().definitions().clone();
        let audit = match &intent {
            SkinIntent::Undo => proposal::history_trail_event(&state, false),
            SkinIntent::Redo => proposal::history_trail_event(&state, true),
            _ => None,
        };
        let receipt = state.session.dispatch(intent);
        if let Some(error) = receipt.error {
            return Err(batch::refusal(error));
        }
        if let Some(audit) = audit {
            state.session.record_trail(vec![audit]).map_err(batch::refusal)?;
        }
        if &state.selection != state.session.selection() {
            state.update_selection(SelectionCause::History);
        }
        let pending = receipt
            .committed
            .as_ref()
            .map_or_else(Vec::new, |commit| commit.pending().to_vec());
        state.record_calculation(before, receipt.recalc.as_ref());
        // Undo can restore names and cells together, so a cell-only footprint would hide the name change.
        let same_definitions = Arc::ptr_eq(&definitions, state.session.host().model().definitions());
        state.record_touch(
            before,
            receipt.committed.and_then(|change| {
                (same_definitions && !change.input_writes().is_empty())
                    .then(|| change.input_writes().to_vec())
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
        let baseline_changed = local.last_baseline != state.baseline_revision;
        let proposals_changed = local.last_proposal != state.proposal_revision;
        for (&watch, slot) in &mut local.watches {
            let completed_snapshot = matches!(slot.query, WorkbookQuery::ConsistentBindings { .. })
                && !state.session.has_unfinished_calculation()
                && slot.answer.as_ref().is_some_and(|result| matches!(
                    result,
                    Ok(answer) if matches!(answer.coverage, Coverage::Partial {
                        why: PartialWhy::Pending, ..
                    })
                ));
            if slot.answer.is_some()
                && !completed_snapshot
                && !(proposals_changed && matches!(slot.query, WorkbookQuery::Proposals | WorkbookQuery::Summary))
                && !(baseline_changed && matches!(
                    slot.query,
                    WorkbookQuery::NumericComparison { .. }
                        | WorkbookQuery::ChartImage { comparison: Some(_), .. }
                        | WorkbookQuery::ConsistentBindings { .. }
                ))
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
        if local.last_look != state.look_revision {
            local.last_look = state.look_revision;
            if matches!(self.origin, Origin::User { .. })
                && let Some(look) = &state.look
            {
                events.push(Event::TryOn(look.clone()));
            }
        }
        local.last_stamp = Some(stamp);
        local.last_selection = state.selection_seq;
        local.last_rtd = rtd;
        local.last_baseline = state.baseline_revision;
        local.last_proposal = state.proposal_revision;
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
        let (locale, use_clock) = {
            let state = self.host.borrow();
            (
                state.session.host().entry_environment().profile,
                state.clock.is_some(),
            )
        };
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
        session.observe_rtd_receipt_times(snapshot::captured_unix_ms);
        let clock = use_clock.then(|| session.install_model_clock());
        SkinHost::initial_selection(&mut session);
        let stamp = session.host().skin_stamp();
        let mut state = self.host.borrow_mut();
        state.session = session;
        state.clock = clock;
        state.opened_at = stamp;
        state.look = None;
        state.proposals.clear();
        state.proposal_revision += 1;
        state.touches.clear();
        state.calc_touches.clear();
        state.calculation_changes = None;
        state.execution = None;
        state.iteration = None;
        state.baseline = None;
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
            WorkbookRequest::PreviewProposal { proposal, units, depth } => {
                if !matches!(self.origin, Origin::User { .. }) {
                    return Err(Refusal::PolicyRefused);
                }
                let mut state = self.host.borrow_mut();
                WorkbookReply::Preview(proposal::preview(&mut state, &proposal, &units, depth)?)
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

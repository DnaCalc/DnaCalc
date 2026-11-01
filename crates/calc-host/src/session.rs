//! The session a skin works through: intents in, receipts and deltas out, independent of any UI
//! toolkit. A grid, a notebook, a tree or a canvas skin, a headless tool or a test all drive one
//! [`HostSession`] the same way:
//!
//! - [`HostSession::dispatch`] takes a [`SkinIntent`]: a cell entry, a multi-cell edit, undo,
//!   redo, recalculation, the calculation mode, the selection, or a subscription's interest. Each
//!   edit commits as one transaction ([`crate::dispatch`]) and is one undo step; unless the
//!   host's calculation mode ([`HostState::calc_mode`]) is [`CalcMode::Manual`], the workbook is
//!   recalculated after each commit, an undo's and a redo's included. Every intent gets an [`IntentReceipt`] and is logged as an
//!   [`IntentRecord`], which [`replay`] can dispatch again against another session.
//! - [`HostSession::subscribe`] registers a [`DeltaSink`] for an [`Interest`] (an area, a sheet,
//!   or the selection: "viewing is subscribing"). The sink first gets the interest's cells as
//!   they stand ([`HostDelta::reset`]), then, after each committed transaction and each
//!   recalculation, a [`HostDelta`] holding every cell of the interest whose input, value or
//!   provenance changed ([`crate::ProjectedCell`]); a cell that became empty is delivered as
//!   empty. Moving the interest ([`SkinIntent::SetInterest`], or a new selection for a selection
//!   interest) delivers the new interest's cells as a reset.
//!
//! A delta is computed by projecting the interest's cells ([`crate::project`]) before and after
//! the change and keeping the cells that differ, so it carries exactly what a fresh projection of
//! the interest would show; [`DeltaMirror`] applies the deltas a sink receives.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7): the intent vocabulary, receipts, audited intent log
//! and replay of `src/dnacalc-skin-ir/src/intent.rs` (`WorkspaceIntent`'s grid, undo, redo,
//! recalculation, selection and grid-interest intents; `IntentReceipt`, `IntentRecord`,
//! `ReplayOutcome`, `replay`), the selection of `src/dnacalc-skin-ir/src/selection.rs`, the
//! "viewing is subscribing" grid interest and the before-and-after projection diff of
//! `src/dnacalc-host-core/src/lib.rs` (`SetGridInterest`, `GridChanged`), and the delta mirror of
//! `src/dnacalc-skin-ir/src/session_channel.rs` (`apply_delta`). Left behind: node and tree
//! identities, candidates, scenarios, sweeps, personas and Leptos signals.

use crate::command::{CellEdit, CommandError, CommandReceipt, HostCommand, dispatch};
use crate::entry::{EntryRefusal, interpret_entry_in};
use crate::history::{History, Inverse, Irreversible, Selection, Step, inverse};
use crate::project::{ProjectedCell, ProjectedValue, ProjectionRequest, project};
use crate::recalc::{HostProgress, calculate, step};
use crate::save::{SaveFailed, save};
use crate::state::{HostOptions, HostState, OpenFailure};
use calc_doc::{CalcMode, OpenPolicy, PackageRead, PackageWrite, SavePolicy, SaveReceipt};
use calc_doc_model::value::ExcelText;
use calc_doc_model::value::{CellKey, GridArea, SheetId};
use calc_doc_model::{ArrayFormula, ArrayKind, AuthoredRevision, AxisEdit, CellInput, FormulaText};
use calc_engine::{CalcTick, Unobserved, WorkBudget};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

/// The number of undo steps a session keeps (Excel's default).
pub const UNDO_LIMIT: usize = 100;

/// The intent log keeps at least this many of the latest records and at most twice as many, so
/// the selection and interest moves a skin dispatches as it is used do not grow it without bound
/// (dc-rif): when it reaches twice this many, the older half is dropped.
pub const INTENT_LOG_LIMIT: usize = 10_000;

/// What a subscription watches.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Interest {
    /// The cells of one area.
    Area(GridArea),
    /// Several bounded windows of the same projection, such as frozen panes and the scrolling body.
    Areas(Vec<GridArea>),
    /// Every cell of one sheet.
    Sheet(SheetId),
    /// The cells of the session's selection, its active cell and its areas, following it as it
    /// changes.
    Selection,
}

/// A subscription's identity within its session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// What a [`HostDelta`] follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeltaCause {
    /// The subscription was registered: its interest's cells as they stand.
    Subscribed,
    /// The subscription's interest moved (a new area, or a new selection for a selection
    /// interest): the new interest's cells as they stand.
    InterestMoved,
    /// A transaction committed this revision.
    Committed { revision: AuthoredRevision },
    /// A recalculation of this revision published a generation.
    Recalculated {
        publication: calc_engine::contract::PublicationUnitId,
        revision: AuthoredRevision,
        tick: CalcTick,
    },
    /// A recalculation of this revision published nothing; the results it would have replaced
    /// are stale ([`crate::StaleCause::RecalcFailed`]).
    RecalcFailed { revision: AuthoredRevision },
}

/// The cells of a subscription's interest that changed.
#[derive(Clone, Debug, PartialEq)]
pub struct HostDelta {
    pub subscription: SubscriptionId,
    /// Counts the deltas of this subscription from 1, so a mirror can see a gap.
    pub seq: u64,
    pub cause: DeltaCause,
    /// The authored revision the cells were projected at.
    pub authored: AuthoredRevision,
    /// The cells are the whole interest as it stands: every cell not among them is empty.
    pub reset: bool,
    /// The changed cells (or, on a reset, every cell of the interest that is not empty), in cell
    /// order. A cell that became empty is here with no input and [`ProjectedValue::Empty`].
    pub cells: Vec<ProjectedCell>,
}

/// Where a subscription's deltas go: any closure, so a skin of any toolkit can forward them.
pub type DeltaSink = Box<dyn FnMut(&HostDelta)>;

/// What a skin asks of the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkinIntent {
    /// A typed workbook command, prepared and committed entirely by the host.
    Command(crate::WorkbookCommand),
    /// Text typed into a cell, read as Excel reads a cell entry ([`crate::interpret_entry`]): a
    /// formula, a value, or, for an empty text, a clear. One transaction, one undo step.
    EnterCell { at: CellKey, text: String },
    /// One validated entry and its layout metadata in the same undo group.
    EnterCellAndLayout {
        at: CellKey,
        text: String,
        layout: Vec<calc_doc_model::SkinLayoutEdit>,
    },
    /// Enter one source across the current selection, relocating formulas from its active cell.
    /// All entries are validated before one transaction and one undo step.
    EnterSelection { text: String },
    /// Cell edits (values, formulas and clears, over any cells) applied in order as one
    /// transaction and one undo step.
    Edit { edits: Vec<CellEdit> },
    /// Inserts or deletes rows or columns of a sheet, as one transaction and one undo step: the
    /// cells and what moves with them shift, and references adjust as Excel's do (a reference
    /// to a deleted cell becomes #REF!). Undo brings back the revision before it.
    EditAxis(AxisEdit),
    /// Takes back the last edit not yet undone, as a new transaction.
    Undo,
    /// Commits again the last edit undone, as a new transaction.
    Redo,
    /// Recalculates the workbook now, whatever the calculation mode ([`crate::calculate`]: what
    /// Excel's Calculate Now does).
    Recalculate,
    /// Explicit exact-area calculation. Formula caches outside the areas are a bounded P02
    /// refusal until boundary-cache qualifications are implemented.
    CalculateRange { areas: Vec<GridArea> },
    /// Explicit SliceV1 scope and selection. Manual range calculation normalizes to ForceAll
    /// through `CalculateRange`; this request preserves the supplied selection.
    Calculate {
        scope: calc_engine::CalcScope,
        selection: calc_engine::EvalSelection,
    },
    /// Sets the host's calculation mode; leaving manual mode recalculates.
    SetCalcMode(CalcMode),
    /// Moves the selection. A selection interest follows it.
    Select(Selection),
    /// Moves a subscription's interest.
    SetInterest {
        subscription: SubscriptionId,
        interest: Interest,
    },
    /// Copies the cells of `from` into `to` as Excel's copy and paste does: values as they are,
    /// formulas relocated, `to` filled by repeating `from` (a fill down or right is the copy of
    /// its first row or column over the rest), a single cell taking `from`'s shape
    /// ([`crate::CopyRefusal`] says what is refused). With `cut`, a move: the source's cells go to
    /// the target, the source is emptied, and every reference to the moved cells follows them, as
    /// Excel's cut and paste does. One transaction, one undo step (dc-09v).
    Copy {
        from: GridArea,
        to: GridArea,
        cut: bool,
    },
}

/// Why an intent changed nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum IntentError {
    /// A workbook command was refused before committing any edit.
    Workbook(String),
    /// The entered text is refused.
    Entry(EntryRefusal),
    /// The transaction was refused; the workbook is as it was.
    Command(CommandError),
    EmptySelection,
    NothingToUndo,
    NothingToRedo,
    /// The step an undo would take back cannot be undone; it stays the last step.
    Irreversible(Irreversible),
    /// No subscription of this session has this identity.
    UnknownSubscription(SubscriptionId),
    /// The workbook holds no sheet with this identity.
    UnknownSheet(SheetId),
    /// The copy was refused; the workbook is as it was.
    Copy(crate::copy::CopyRefusal),
    /// The cell's data validation, a stop rule, does not allow the entry (dc-6yw).
    Validation(crate::validation::ValidationFailure),
}

impl fmt::Display for IntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntentError::Workbook(reason) => f.write_str(reason),
            IntentError::Entry(refusal) => write!(f, "the entry was refused: {refusal}"),
            IntentError::Command(error) => write!(f, "the edit was refused: {error}"),
            IntentError::EmptySelection => f.write_str("there is no active selection"),
            IntentError::NothingToUndo => f.write_str("there is nothing to undo"),
            IntentError::NothingToRedo => f.write_str("there is nothing to redo"),
            IntentError::Irreversible(irreversible) => {
                write!(f, "the last edit cannot be undone: {irreversible}")
            }
            IntentError::UnknownSubscription(id) => {
                write!(f, "the session has no subscription {}", id.get())
            }
            IntentError::UnknownSheet(sheet) => write!(f, "the workbook holds no sheet {sheet:?}"),
            IntentError::Validation(failure) => write!(f, "{failure}"),
            IntentError::Copy(refusal) => write!(f, "the copy was refused: {refusal}"),
        }
    }
}

impl std::error::Error for IntentError {}

/// What one dispatched intent did.
#[derive(Debug, PartialEq)]
pub struct IntentReceipt {
    /// The intent's place in the session's log, from 1.
    pub seq: u64,
    pub error: Option<IntentError>,
    /// The transaction the intent committed, if any.
    pub committed: Option<CommandReceipt>,
    /// The recalculation the intent ran, if any.
    pub recalc: Option<HostProgress>,
    /// The authored revision after the intent.
    pub revision: AuthoredRevision,
    /// The number of deltas delivered to sinks.
    pub deltas: usize,
}

impl IntentReceipt {
    pub fn accepted(&self) -> bool {
        self.error.is_none()
    }
}

/// One intent of the session's log, with its outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentRecord {
    /// The host that recorded the command; replay may rebind only plans from this scope.
    scope: u64,
    pub seq: u64,
    pub intent: SkinIntent,
    pub accepted: bool,
    /// The authored revision after the intent.
    pub revision: AuthoredRevision,
}

/// What [`replay`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayOutcome {
    pub dispatched: usize,
    pub accepted: usize,
    /// The records whose acceptance differed when replayed: the target diverged from the
    /// recorded session.
    pub mismatches: Vec<u64>,
}

/// Dispatches every record's intent, in order, against `session`, and counts where the outcome
/// differs from the record's. A rejected record must be rejected again for a faithful replay.
pub fn replay(records: &[IntentRecord], session: &mut HostSession) -> ReplayOutcome {
    let mut outcome = ReplayOutcome::default();
    for record in records {
        let intent = match record.intent.clone() {
            SkinIntent::Command(crate::WorkbookCommand::Paste(plan)) => SkinIntent::Command(
                crate::WorkbookCommand::Paste(plan.for_replay(session.host(), record.scope)),
            ),
            intent => intent,
        };
        let receipt = session.dispatch(intent);
        outcome.dispatched += 1;
        if receipt.accepted() {
            outcome.accepted += 1;
        }
        if receipt.accepted() != record.accepted {
            outcome.mismatches.push(record.seq);
        }
    }
    outcome
}

struct Subscription {
    interest: Interest,
    sink: DeltaSink,
    seq: u64,
    /// The interest's cells that are not empty, as last delivered.
    cells: BTreeMap<CellKey, ProjectedCell>,
}

/// One open workbook as skins see it: the host, its undo history, the selection, the
/// subscriptions and the intent log.
pub struct HostSession {
    recalc_budget: WorkBudget,
    host: HostState,
    sheet_keys: BTreeSet<crate::SheetKey>,
    history: History,
    selection: Selection,
    subscriptions: BTreeMap<SubscriptionId, Subscription>,
    next_subscription: u64,
    log: Vec<IntentRecord>,
    /// The sequence number of the next intent.
    next_seq: u64,
}

impl fmt::Debug for HostSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostSession")
            .field("revision", &self.host.revision())
            .field("selection", &self.selection)
            .field("mode", &self.host.calc_mode())
            .field("subscriptions", &self.subscriptions.len())
            .field("log", &self.log.len())
            .finish()
    }
}

impl HostSession {
    /// A session over an open host, in the host's calculation mode, with nothing selected.
    pub fn new(host: HostState) -> HostSession {
        HostSession {
            recalc_budget: WorkBudget::UNLIMITED,
            sheet_keys: host.sheets().iter().map(|s| s.key()).collect(),
            host,
            history: History::new(UNDO_LIMIT),
            selection: Selection::default(),
            subscriptions: BTreeMap::new(),
            next_subscription: 1,
            log: Vec::new(),
            next_seq: 1,
        }
    }

    /// Opens a package ([`HostState::open`]) into a new session.
    pub fn open(
        source: Arc<dyn PackageRead>,
        policy: &OpenPolicy,
        options: HostOptions,
    ) -> Result<HostSession, OpenFailure> {
        HostState::open(source, policy, options).map(HostSession::new)
    }

    /// Each dispatch and continuation performs at most one step under this budget. Zero is
    /// normalized to one evaluation so a worker never schedules an empty busy loop.
    pub fn set_calculation_budget(&mut self, budget: WorkBudget) {
        self.recalc_budget = if budget == WorkBudget::evaluations(0) {
            WorkBudget::evaluations(1)
        } else {
            budget
        };
    }
    pub fn has_pending_calculation(&self) -> bool {
        self.host.has_pending_calculation()
    }
    pub fn take_calculation_events(&mut self) -> Vec<calc_engine::CalcEvent> {
        self.host.take_calculation_events()
    }

    pub fn cancel_calculation(&mut self) {
        if let Some(mut work) = self.host.work.take() {
            self.host.calc_events.extend(work.take_events());
            self.host
                .calc_events
                .push(calc_engine::CalcEvent::Completed(work.completion(
                    calc_engine::contract::CompletionOutcome::Cancelled,
                    self.host.engine.result_view(),
                    None,
                )));
        }
        self.host.calculate_requested = false;
        self.host.full_calculate_requested = false;
        self.host.requested_calculation = None;
        self.host.transformed_calculation_scope = None;
        self.host.calculation_source = None;
        self.host.calculation_cancelled = true;
    }

    /// Continues an already authorized calculation. It creates no authored command or history
    /// entry and never widens its scope.
    pub fn advance_calculation(&mut self) -> HostProgress {
        let mut receipt = IntentReceipt {
            seq: self.next_seq,
            error: None,
            committed: None,
            recalc: None,
            revision: self.host.revision(),
            deltas: 0,
        };
        self.recalculate(&mut receipt);
        receipt.recalc.expect("one step always reports progress")
    }

    /// The host, to read: its sheets, revision and projections ([`crate::project`]).
    pub fn host(&self) -> &HostState {
        &self.host
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Sets the clock NOW and TODAY read ([`HostState::set_clock`]).
    pub fn set_clock(&mut self, now: Option<f64>) {
        self.host.set_clock(now);
    }

    /// The host's calculation mode ([`HostState::calc_mode`]).
    pub fn calc_mode(&self) -> CalcMode {
        self.host.calc_mode()
    }

    /// The number of steps an undo can take back and a redo can commit again.
    pub fn history_depth(&self) -> (usize, usize) {
        (self.history.undo_depth(), self.history.redo_depth())
    }

    /// The latest intents dispatched, with their outcomes, in order: every one while fewer than
    /// twice [`INTENT_LOG_LIMIT`] have been dispatched, and never fewer than the last
    /// [`INTENT_LOG_LIMIT`]. A record's `seq` counts every intent the session dispatched.
    pub fn intent_log(&self) -> &[IntentRecord] {
        &self.log
    }

    /// Pins the current model and result view without invoking or draining calculation.
    pub fn prepare_save(&self, policy: &SavePolicy) -> Result<crate::PreparedSave, SaveFailed> {
        crate::prepare_save(&self.host, policy)
    }

    /// Writes the current revision ([`crate::save`]).
    pub fn save(
        &self,
        policy: &SavePolicy,
        out: &mut dyn PackageWrite,
    ) -> Result<SaveReceipt, SaveFailed> {
        save(&self.host, policy, out)
    }

    /// Registers `sink` for the cells of `interest` and delivers them as they stand.
    pub fn subscribe(
        &mut self,
        interest: Interest,
        sink: DeltaSink,
    ) -> Result<SubscriptionId, IntentError> {
        self.check_interest(&interest)?;
        let id = SubscriptionId(self.next_subscription);
        self.next_subscription += 1;
        self.subscriptions.insert(
            id,
            Subscription {
                interest,
                sink,
                seq: 0,
                cells: BTreeMap::new(),
            },
        );
        self.deliver_reset(id, DeltaCause::Subscribed);
        Ok(id)
    }

    /// Removes a subscription; `false` when the session has none with this identity.
    pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
        self.subscriptions.remove(&id).is_some()
    }

    /// Carries out `intent` and logs it.
    pub fn dispatch(&mut self, intent: SkinIntent) -> IntentReceipt {
        let seq = self.next_seq;
        self.next_seq = self
            .next_seq
            .checked_add(1)
            .expect("host command identity exhausted");
        let mut receipt = IntentReceipt {
            seq,
            error: None,
            committed: None,
            recalc: None,
            revision: self.host.revision(),
            deltas: 0,
        };
        let previous_source = self.host.calculation_source;
        if !matches!(
            intent,
            SkinIntent::Select(_) | SkinIntent::SetInterest { .. }
        ) {
            self.host.calculation_source = Some(calc_engine::contract::CommandId {
                client_session: self.host.command_scope,
                sequence: seq,
            });
        }
        let outcome = match intent.clone() {
            SkinIntent::Command(command) => {
                let coordinates = match &command {
                    crate::WorkbookCommand::ShiftCells { area, down, insert } => {
                        Some(calc_fml::reference::StructuralEdit::ShiftCells {
                            area: *area,
                            down: *down,
                            insert: *insert,
                        })
                    }
                    crate::WorkbookCommand::Paste(plan) => plan.coordinates(),
                    _ => None,
                };
                crate::actions::prepare(&self.host, &mut self.sheet_keys, command)
                    .map_err(IntentError::Workbook)
                    .and_then(|edits| {
                        if edits.is_empty() {
                            return Ok(());
                        }
                        let edits = edits.into_iter().map(|edit| self.authored(edit)).collect();
                        self.edit_coordinates(edits, coordinates, &mut receipt)
                    })
            }
            SkinIntent::EnterCell { at, text } => self
                .prepare_entries(at, &text, &[GridArea::cell(at)])
                .and_then(|edits| self.edit(edits, &mut receipt)),
            SkinIntent::EnterCellAndLayout { at, text, layout } => self
                .prepare_entries(at, &text, &[GridArea::cell(at)])
                .and_then(|mut edits| {
                    edits.extend(layout.into_iter().map(CellEdit::Layout));
                    self.edit(edits, &mut receipt)
                }),
            SkinIntent::EnterSelection { text } => match self.selection.active {
                Some(at) => self
                    .prepare_entries(at, &text, &self.selection.areas)
                    .and_then(|edits| self.edit(edits, &mut receipt)),
                None => Err(IntentError::EmptySelection),
            },
            SkinIntent::Edit { edits } => {
                let edits = edits.into_iter().map(|edit| self.authored(edit)).collect();
                self.edit(edits, &mut receipt)
            }
            SkinIntent::EditAxis(edit) => self.edit(vec![CellEdit::Axis(edit)], &mut receipt),
            SkinIntent::Copy { from, to, cut } => {
                match crate::copy::copy_edits(&self.host, from, to, cut) {
                    Ok(edits) => {
                        let edits = edits.into_iter().map(|edit| self.authored(edit)).collect();
                        let coordinates = cut
                            .then(|| crate::copy::move_coordinates(&self.host, from, to.first()));
                        self.edit_coordinates(edits, coordinates, &mut receipt)
                    }
                    Err(refusal) => Err(IntentError::Copy(refusal)),
                }
            }
            SkinIntent::Undo => self.undo(&mut receipt),
            SkinIntent::Redo => self.redo(&mut receipt),
            SkinIntent::Recalculate => {
                calculate(&mut self.host);
                self.recalculate(&mut receipt);
                Ok(())
            }
            SkinIntent::CalculateRange { areas } => self.request_calculation(
                calc_engine::CalcScope::Range(areas),
                calc_engine::EvalSelection::ForceAll,
                &mut receipt,
            ),
            SkinIntent::Calculate { scope, selection } => {
                self.request_calculation(scope, selection, &mut receipt)
            }
            SkinIntent::SetCalcMode(mode) => {
                self.host.options.calc_mode = Some(mode);
                if mode != CalcMode::Manual {
                    self.recalculate(&mut receipt);
                }
                Ok(())
            }
            SkinIntent::Select(selection) => self.select(selection, &mut receipt),
            SkinIntent::SetInterest {
                subscription,
                interest,
            } => self.set_interest(subscription, interest, &mut receipt),
        };
        receipt.error = outcome.err();
        if receipt.error.is_some() {
            self.host.calculation_source = previous_source;
        }
        receipt.revision = self.host.revision();
        self.log.push(IntentRecord {
            scope: self.host.command_scope,
            seq,
            intent,
            accepted: receipt.error.is_none(),
            revision: receipt.revision,
        });
        if self.log.len() >= 2 * INTENT_LOG_LIMIT {
            self.log.drain(..INTENT_LOG_LIMIT);
        }
        receipt
    }

    fn request_calculation(
        &mut self,
        scope: calc_engine::CalcScope,
        selection: calc_engine::EvalSelection,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        if self.host.work.is_some() {
            return Err(IntentError::Workbook(
                "Finish or cancel the active calculation before starting a scoped calculation"
                    .into(),
            ));
        }
        let valid = match &scope {
            calc_engine::CalcScope::Workbook => true,
            calc_engine::CalcScope::Sheet(sheet) => self.host.model.sheet_by_id(*sheet).is_some(),
            calc_engine::CalcScope::Range(areas) => {
                !areas.is_empty()
                    && areas
                        .iter()
                        .all(|area| self.host.model.sheet_by_id(area.sheet()).is_some())
            }
        };
        if !valid {
            return Err(IntentError::Workbook(
                "The calculation scope must name existing worksheet cells".into(),
            ));
        }
        self.host.requested_calculation = Some((scope, selection));
        calculate(&mut self.host);
        self.recalculate(receipt);
        Ok(())
    }

    fn prepare_entries(
        &self,
        origin: CellKey,
        text: &str,
        areas: &[GridArea],
    ) -> Result<Vec<CellEdit>, IntentError> {
        let count = areas.iter().map(|area| area.cell_count()).sum::<u64>();
        if count == 0 {
            return Err(IntentError::EmptySelection);
        }
        if count > crate::copy::COPY_LIMIT {
            return Err(IntentError::Copy(crate::copy::CopyRefusal::TooLarge {
                cells: count,
            }));
        }
        let entry = interpret_entry_in(text, Some(&self.host.entry_environment()))
            .map_err(IntentError::Entry)?;
        let mut edits = Vec::new();
        let mut seen = BTreeSet::new();
        for area in areas {
            for at in cells_of(Some(*area)).filter(|at| seen.insert(*at)) {
                let input = match &entry {
                    Some(CellInput::Formula(formula)) if at != origin => Some(CellInput::Formula(
                        calc_engine::relocate_formula(&self.host.model, formula, origin, at)
                            .ok_or(IntentError::Copy(crate::copy::CopyRefusal::Unrelocatable {
                                at: origin,
                            }))?,
                    )),
                    other => other.clone(),
                };
                if let Err(failure) = self.host.check_entry(at, input.as_ref()) {
                    if failure.style == crate::validation::ErrorStyle::Stop {
                        return Err(IntentError::Validation(failure));
                    }
                }
                edits.push(match input {
                    Some(input) => self.entry_edit(at, input),
                    None => CellEdit::Clear {
                        area: GridArea::cell(at),
                    },
                });
            }
        }
        Ok(edits)
    }

    /// The edit an entry makes ([`HostSession::authored`] of a set).
    fn entry_edit(&self, at: CellKey, input: CellInput) -> CellEdit {
        self.authored(CellEdit::Set { at, input })
    }

    /// An edit as the model holds it. A formula's text is put in its file form
    /// ([`calc_doc::file_form`]): a function newer than Excel 2007 takes the `_xlfn.` prefix the
    /// package holds it with, which is how the model and the engine read a loaded formula (without
    /// it, `SEQUENCE(3)` names no function and is #NAME?). A formula set into one cell that can
    /// return an array is authored as a dynamic array over that cell, as Excel stores a formula
    /// typed or pasted into a cell, so the engine spills its result. A formula whose text the
    /// parser does not read is left as typed, for the edit to refuse.
    fn authored(&self, edit: CellEdit) -> CellEdit {
        let names = self.host.model.definitions().names();
        let links = self.host.model.definitions().external_file_names();
        let file_form = |formula: &FormulaText| {
            let form = calc_doc::file_form_with_links(formula, names, &links).ok()?;
            let text = FormulaText::new(ExcelText::from_utf16_code_units(form.text)).ok()?;
            Some((text, form.dynamic))
        };
        match edit {
            CellEdit::Set {
                at,
                input: CellInput::Formula(formula),
            } => match file_form(&formula) {
                Some((text, true)) => CellEdit::SetArray {
                    array: ArrayFormula::new(
                        GridArea::cell(at),
                        ArrayKind::Dynamic { collapsed: false },
                    ),
                    formula: text,
                },
                Some((text, false)) => CellEdit::Set {
                    at,
                    input: CellInput::Formula(text),
                },
                None => CellEdit::Set {
                    at,
                    input: CellInput::Formula(formula),
                },
            },
            CellEdit::SetArray { array, formula } => CellEdit::SetArray {
                array,
                formula: file_form(&formula).map_or(formula, |(text, _)| text),
            },
            edit => edit,
        }
    }

    fn check_interest(&self, interest: &Interest) -> Result<(), IntentError> {
        if let Interest::Areas(areas) = interest {
            for area in areas {
                self.check_interest(&Interest::Area(*area))?;
            }
            return Ok(());
        }
        let sheet = match interest {
            Interest::Area(area) => area.sheet(),
            Interest::Sheet(sheet) => *sheet,
            Interest::Selection => return Ok(()),
            Interest::Areas(_) => unreachable!("checked above"),
        };
        match self.host.model.sheet_by_id(sheet) {
            Some(_) => Ok(()),
            None => Err(IntentError::UnknownSheet(sheet)),
        }
    }

    /// Commits `edits` as one transaction, records it as an undo step, delivers its deltas and,
    /// in automatic mode, recalculates.
    fn edit(
        &mut self,
        edits: Vec<CellEdit>,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        self.edit_coordinates(edits, None, receipt)
    }

    fn edit_coordinates(
        &mut self,
        edits: Vec<CellEdit>,
        coordinates: Option<calc_fml::reference::StructuralEdit>,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        let inverse = inverse(&self.host.model, &self.host.axis_edits, &edits);
        self.commit_coordinates(edits.clone(), coordinates.as_ref(), receipt)?;
        self.history.record(Step {
            forward: edits,
            coordinates,
            inverse,
            before: self.selection.clone(),
            after: self.selection.clone(),
        });
        self.after_commit(receipt);
        Ok(())
    }

    fn commit_coordinates(
        &mut self,
        edits: Vec<CellEdit>,
        coordinates: Option<&calc_fml::reference::StructuralEdit>,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        let scope = coordinates.and_then(|edit| {
            crate::command::active_scope(&self.host).map(|scope| scope.transformed(edit))
        });
        let refresh_rules = edits
            .iter()
            .any(|edit| matches!(edit, CellEdit::ConditionalRules { .. }));
        let base = self.host.revision();
        let committed =
            dispatch(&mut self.host, HostCommand { base, edits }).map_err(IntentError::Command)?;
        if let Some(scope) = scope {
            self.host.transformed_calculation_scope = Some(scope);
        }
        let revision = committed.revision();
        receipt.committed = Some(committed);
        receipt.deltas += self.deliver_changes(DeltaCause::Committed { revision }, refresh_rules);
        Ok(())
    }

    fn after_commit(&mut self, receipt: &mut IntentReceipt) {
        if self.host.calc_mode() != CalcMode::Manual {
            self.recalculate(receipt);
        }
    }

    fn undo(&mut self, receipt: &mut IntentReceipt) -> Result<(), IntentError> {
        let step = self.history.pop_undo().ok_or(IntentError::NothingToUndo)?;
        let inverse = match &step.inverse {
            Ok(inverse) => inverse.clone(),
            Err(irreversible) => {
                let irreversible = *irreversible;
                self.history.restore_undo(step);
                return Err(IntentError::Irreversible(irreversible));
            }
        };
        let undone = match inverse {
            Inverse::Edits(edits) => self.commit_coordinates(
                edits,
                step.coordinates
                    .as_ref()
                    .map(crate::history::inverse_coordinates)
                    .as_ref(),
                receipt,
            ),
            Inverse::Restore { model, axis_edits } => {
                match crate::command::restore(&mut self.host, &model, axis_edits) {
                    Ok(committed) => {
                        let revision = committed.revision();
                        receipt.committed = Some(committed);
                        receipt.deltas +=
                            self.deliver_changes(DeltaCause::Committed { revision }, false);
                        Ok(())
                    }
                    Err(error) => Err(IntentError::Command(error)),
                }
            }
        };
        if let Err(error) = undone {
            self.history.restore_undo(step);
            return Err(error);
        }
        let selection = step.before.clone();
        self.history.undone(step);
        self.select_quietly(selection, receipt);
        self.after_commit(receipt);
        Ok(())
    }

    fn redo(&mut self, receipt: &mut IntentReceipt) -> Result<(), IntentError> {
        let step = self.history.pop_redo().ok_or(IntentError::NothingToRedo)?;
        if let Err(error) =
            self.commit_coordinates(step.forward.clone(), step.coordinates.as_ref(), receipt)
        {
            self.history.restore_redo(step);
            return Err(error);
        }
        let selection = step.after.clone();
        self.history.redone(step);
        self.select_quietly(selection, receipt);
        self.after_commit(receipt);
        Ok(())
    }

    /// Runs one authorized step and delivers any publication it completed.
    fn recalculate(&mut self, receipt: &mut IntentReceipt) {
        self.host.calculation_interest = self
            .subscriptions
            .values()
            .flat_map(|subscription| self.areas(&subscription.interest))
            .collect();
        let progress = step(&mut self.host, self.recalc_budget, &mut Unobserved);
        let cause = match &progress {
            HostProgress::Current | HostProgress::Manual | HostProgress::Cancelled => None,
            HostProgress::Calculating { .. } => None,
            HostProgress::Published(delta) => Some(DeltaCause::Recalculated {
                publication: delta
                    .publication()
                    .expect("a published delta names its installed unit"),
                revision: delta.revision(),
                tick: delta.tick(),
            }),
            HostProgress::Failed(_) => Some(DeltaCause::RecalcFailed {
                revision: self.host.revision(),
            }),
        };
        if let Some(cause) = cause {
            receipt.deltas += self.deliver_changes(cause, false);
        }
        receipt.recalc = Some(progress);
    }

    fn select(
        &mut self,
        selection: Selection,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        let sheets = selection
            .active
            .iter()
            .map(|at| at.sheet())
            .chain(selection.areas.iter().map(|area| area.sheet()));
        for sheet in sheets {
            if self.host.model.sheet_by_id(sheet).is_none() {
                return Err(IntentError::UnknownSheet(sheet));
            }
        }
        self.select_quietly(selection, receipt);
        Ok(())
    }

    /// Moves the selection and delivers the selection interests' new cells.
    fn select_quietly(&mut self, selection: Selection, receipt: &mut IntentReceipt) {
        if self.selection == selection {
            return;
        }
        self.selection = selection;
        let following: Vec<SubscriptionId> = self
            .subscriptions
            .iter()
            .filter(|(_, subscription)| subscription.interest == Interest::Selection)
            .map(|(id, _)| *id)
            .collect();
        for id in following {
            self.deliver_reset(id, DeltaCause::InterestMoved);
            receipt.deltas += 1;
        }
    }

    fn set_interest(
        &mut self,
        id: SubscriptionId,
        interest: Interest,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        self.check_interest(&interest)?;
        let subscription = self
            .subscriptions
            .get_mut(&id)
            .ok_or(IntentError::UnknownSubscription(id))?;
        subscription.interest = interest;
        self.deliver_reset(id, DeltaCause::InterestMoved);
        receipt.deltas += 1;
        Ok(())
    }

    /// The areas an interest covers now.
    fn areas(&self, interest: &Interest) -> Vec<GridArea> {
        match interest {
            Interest::Area(area) => vec![*area],
            Interest::Areas(areas) => areas.clone(),
            Interest::Sheet(sheet) => vec![GridArea::full_sheet(*sheet)],
            Interest::Selection => self
                .selection
                .active
                .iter()
                .map(|at| GridArea::cell(*at))
                .chain(self.selection.areas.iter().copied())
                .collect(),
        }
    }

    /// The interest's cells that are not empty: every cell holding an input, every cell an array
    /// formula's area holds, every cell a published spill fills, and every unloaded cell,
    /// projected.
    fn snapshot(&self, interest: &Interest) -> BTreeMap<CellKey, ProjectedCell> {
        let mut cells = BTreeSet::new();
        let definitions = self.host.model.definitions();
        let areas = self.areas(interest);
        let bounded = areas.iter().map(|a| a.cell_count()).sum::<u64>() <= 32768;
        let rule_areas = definitions
            .all_conditional_rules()
            .flat_map(|(_, r)| r)
            .flat_map(|r| r.areas.iter().copied())
            .collect::<Vec<_>>();
        for area in areas {
            if self.host.model.sheet_by_id(area.sheet()).is_none() {
                continue;
            }
            cells.extend(
                self.host
                    .model
                    .cells(area)
                    .into_iter()
                    .flatten()
                    .map(|(at, _)| at),
            );
            if bounded {
                for rule_area in &rule_areas {
                    cells.extend(cells_of(intersection(*rule_area, area)));
                }
            }
            for array in definitions.arrays_meeting(area) {
                cells.extend(cells_of(intersection(array.area(), area)));
            }
            // The cells a published spill fills, which no input or definition names: a dynamic
            // array's result spills past the area the model gives it.
            for spill in self.host.engine.spills() {
                cells.extend(cells_of(intersection(spill.area(), area)));
            }
            cells.extend(definitions.unloaded_in(area).map(|(at, _)| at));
        }
        let page = project(
            &self.host,
            &ProjectionRequest {
                cells: cells.into_iter().collect(),
            },
        )
        .expect("the interest's sheets were checked when it was set");
        page.cells()
            .iter()
            .filter(|cell| {
                cell.value() != &ProjectedValue::Empty
                    || cell.input().is_some()
                    || rule_areas.iter().any(|a| a.contains(cell.at()))
            })
            .map(|cell| (cell.at(), cell.clone()))
            .collect()
    }

    fn deliver_reset(&mut self, id: SubscriptionId, cause: DeltaCause) {
        let Some(interest) = self.subscriptions.get(&id).map(|s| s.interest.clone()) else {
            return;
        };
        let cells = self.snapshot(&interest);
        let authored = self.host.revision();
        let subscription = self.subscriptions.get_mut(&id).expect("looked up above");
        subscription.seq += 1;
        let delta = HostDelta {
            subscription: id,
            seq: subscription.seq,
            cause,
            authored,
            reset: true,
            cells: cells.values().cloned().collect(),
        };
        subscription.cells = cells;
        (subscription.sink)(&delta);
    }

    /// Delivers each subscription's changed cells; the number of deltas delivered.
    fn deliver_changes(&mut self, cause: DeltaCause, refresh_rules: bool) -> usize {
        // Rule appearance may depend on unchanged cells and statistics over an entire range.
        // Reuse each bounded subscription's reset when those looks can change, including when
        // removing the last rule. These remain projections of current values, never calculations.
        if refresh_rules
            || self
                .host
                .model
                .definitions()
                .all_conditional_rules()
                .any(|(_, r)| !r.is_empty())
        {
            let ids = self.subscriptions.keys().copied().collect::<Vec<_>>();
            let count = ids.len();
            for id in ids {
                self.deliver_reset(id, cause.clone());
            }
            return count;
        }
        let authored = self.host.revision();
        let ids: Vec<SubscriptionId> = self.subscriptions.keys().copied().collect();
        let mut delivered = 0;
        for id in ids {
            let interest = self.subscriptions[&id].interest.clone();
            let after = self.snapshot(&interest);
            let subscription = self.subscriptions.get_mut(&id).expect("listed above");
            let mut changed: Vec<ProjectedCell> = after
                .iter()
                .filter(|(at, cell)| subscription.cells.get(at) != Some(cell))
                .map(|(_, cell)| cell.clone())
                .collect();
            changed.extend(
                subscription
                    .cells
                    .keys()
                    .filter(|at| !after.contains_key(at))
                    .map(|at| ProjectedCell::empty(*at)),
            );
            changed.sort_by_key(|cell| cell.at());
            subscription.cells = after;
            if changed.is_empty() {
                continue;
            }
            subscription.seq += 1;
            let delta = HostDelta {
                subscription: id,
                seq: subscription.seq,
                cause: cause.clone(),
                authored,
                reset: false,
                cells: changed,
            };
            (subscription.sink)(&delta);
            delivered += 1;
        }
        delivered
    }
}

/// The cells two areas share, as an area; `None` when they share none.
fn intersection(a: GridArea, b: GridArea) -> Option<GridArea> {
    if a.sheet() != b.sheet() {
        return None;
    }
    let top = a.first().row().max(b.first().row());
    let left = a.first().column().max(b.first().column());
    let bottom = a.last().row().min(b.last().row());
    let right = a.last().column().min(b.last().column());
    if top > bottom || left > right {
        return None;
    }
    GridArea::new(
        CellKey::new(a.sheet(), top, left).ok()?,
        CellKey::new(a.sheet(), bottom, right).ok()?,
    )
    .ok()
}

/// Every cell of an area, in cell order; none for `None`.
fn cells_of(area: Option<GridArea>) -> impl Iterator<Item = CellKey> {
    area.into_iter().flat_map(|area| {
        let (first, last) = (area.first(), area.last());
        (first.row()..=last.row()).flat_map(move |row| {
            (first.column()..=last.column())
                .filter_map(move |column| CellKey::new(area.sheet(), row, column).ok())
        })
    })
}

/// A skin's copy of a subscription's cells, kept by applying the deltas its sink receives
/// (ported from DnaTreeCalc's `apply_delta`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeltaMirror {
    seq: u64,
    cells: BTreeMap<CellKey, ProjectedCell>,
}

/// Why [`DeltaMirror::apply`] refused a delta: it does not follow the last one applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeltaGap {
    pub expected: u64,
    pub received: u64,
}

impl DeltaMirror {
    /// Applies a delta: a reset replaces every cell, any other delta replaces the cells it holds
    /// and drops those it delivers as empty. A delta that is not the next of the subscription is
    /// refused unless it is a reset.
    pub fn apply(&mut self, delta: &HostDelta) -> Result<(), DeltaGap> {
        if !delta.reset && delta.seq != self.seq + 1 {
            return Err(DeltaGap {
                expected: self.seq + 1,
                received: delta.seq,
            });
        }
        if delta.reset {
            self.cells.clear();
        }
        for cell in &delta.cells {
            if cell.input().is_none() && cell.value() == &ProjectedValue::Empty {
                self.cells.remove(&cell.at());
            } else {
                self.cells.insert(cell.at(), cell.clone());
            }
        }
        self.seq = delta.seq;
        Ok(())
    }

    /// The cell as the mirror holds it; `None` for an empty cell.
    pub fn cell(&self, at: CellKey) -> Option<&ProjectedCell> {
        self.cells.get(&at)
    }

    /// Every cell the mirror holds, in cell order.
    pub fn cells(&self) -> impl Iterator<Item = &ProjectedCell> {
        self.cells.values()
    }
}

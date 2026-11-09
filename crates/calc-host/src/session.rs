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

use crate::command::{CellEdit, CommandError, CommandReceipt, dispatch_slice};
use crate::entry::{EntryRefusal, interpret_entry_in};
use crate::history::{History, Inverse, Irreversible, Selection, Step, inverse};
use crate::project::{ProjectedCell, ProjectedValue, ProjectionRequest, project};
use crate::recalc::{HostProgress, calculate, step};
use crate::save::SaveFailed;
use crate::state::{HostOptions, HostState, OpenFailure};
use calc_doc::{CalcMode, OpenPolicy, PackageRead, PackageWrite, SavePolicy, SaveReceipt};
use calc_doc_model::value::ExcelText;
use calc_doc_model::value::{CellKey, GridArea, SheetId};
use calc_doc_model::{ArrayFormula, ArrayKind, AuthoredRevision, AxisEdit, CellInput, FormulaText};
use calc_engine::{CalcTick, Unobserved, WorkBudget};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::{cell::Cell, rc::Rc};

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
    ExternalInputsChanged {
        revision: AuthoredRevision,
    },
    CallablesChanged {
        revision: AuthoredRevision,
    },
    /// The subscription was registered: its interest's cells as they stand.
    Subscribed,
    /// The subscription's interest moved (a new area, or a new selection for a selection
    /// interest): the new interest's cells as they stand.
    InterestMoved,
    /// A transaction committed this revision.
    Committed {
        revision: AuthoredRevision,
    },
    /// A recalculation of this revision published a generation.
    Recalculated {
        publication: calc_engine::contract::PublicationUnitId,
        revision: AuthoredRevision,
        tick: CalcTick,
    },
    /// A recalculation of this revision published nothing; the results it would have replaced
    /// are stale ([`crate::StaleCause::RecalcFailed`]).
    RecalcFailed {
        revision: AuthoredRevision,
    },
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
    /// Rebuild formula bindings/dependencies, then force every workbook formula.
    /// Active cell work blocks admission. A retained name handle expires when
    /// this command resets binding authority, before it can enter more user code.
    Rebuild,
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
    pub ingress: CommandIngress,
    /// The intent's place in the session's log, from 1.
    pub seq: u64,
    /// The synchronous command whose handler issued this command, if any.
    pub parent: Option<u64>,
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
    pub parent: Option<u64>,
    pub ingress: CommandIngress,
    pub intent: SkinIntent,
    pub accepted: bool,
    /// The authored revision after the intent.
    pub revision: AuthoredRevision,
}

/// P22 records ordinary COM writes separately from UI edits. This source is
/// retained in the log because it determines the event observation position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandIngress {
    UserInterface,
    Automation,
}

/// What [`replay`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayOutcome {
    pub dispatched: usize,
    pub accepted: usize,
    /// Root records whose acceptance or nested command transcript differed, plus
    /// child records whose recorded parent ancestry is missing or cyclic.
    pub mismatches: Vec<u64>,
}

/// Dispatches root intents in order; synchronous handler commands are regenerated by their
/// owner, never replayed a second time. Counts where the root outcome
/// differs from the record's. A rejected record must be rejected again for a faithful replay.
pub fn replay(records: &[IntentRecord], session: &mut HostSession) -> ReplayOutcome {
    let mut outcome = ReplayOutcome::default();
    let parents: BTreeMap<_, _> = records.iter().map(|r| (r.seq, r.parent)).collect();
    let root = |mut seq| {
        for _ in 0..=parents.len() {
            match parents.get(&seq) {
                Some(Some(parent)) => seq = *parent,
                Some(None) => return Some(seq),
                None => return None,
            }
        }
        None
    };
    outcome.mismatches.extend(
        records
            .iter()
            .filter(|r| r.parent.is_some() && root(r.seq).is_none())
            .map(|r| r.seq),
    );
    for record in records.iter().filter(|record| record.parent.is_none()) {
        // Nested commands are regenerated by their owning handlers, once.
        let intent = match record.intent.clone() {
            SkinIntent::Command(crate::WorkbookCommand::Paste(plan)) => SkinIntent::Command(
                crate::WorkbookCommand::Paste(plan.for_replay(session.host(), record.scope)),
            ),
            intent => intent,
        };
        let receipt = session.dispatch_from(record.ingress, intent);
        outcome.dispatched += 1;
        if receipt.accepted() {
            outcome.accepted += 1;
        }
        let expected = records
            .iter()
            .filter(|r| r.parent.is_some() && root(r.seq) == Some(record.seq))
            .map(|r| {
                (
                    r.seq.checked_sub(record.seq),
                    r.parent.and_then(|p| p.checked_sub(record.seq)),
                    r.ingress,
                    &r.intent,
                    r.accepted,
                )
            })
            .collect::<Vec<_>>();
        let observed = session
            .log
            .iter()
            .filter(|r| r.seq >= receipt.seq && r.parent.is_some())
            .map(|r| {
                (
                    r.seq.checked_sub(receipt.seq),
                    r.parent.and_then(|p| p.checked_sub(receipt.seq)),
                    r.ingress,
                    &r.intent,
                    r.accepted,
                )
            })
            .collect::<Vec<_>>();
        if receipt.accepted() != record.accepted || expected != observed {
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
    pub(crate) workbook_events: crate::events::WorkbookEventState,
    recalc_budget: WorkBudget,
    pub(crate) host: HostState,
    sheet_keys: BTreeSet<crate::SheetKey>,
    history: History,
    selection: Selection,
    subscriptions: BTreeMap<SubscriptionId, Subscription>,
    next_subscription: u64,
    log: Vec<IntentRecord>,
    /// The sequence number of the next intent.
    next_seq: u64,
    active_command: Rc<Cell<Option<u64>>>,
}

struct CommandLoan {
    active: Rc<Cell<Option<u64>>>,
    parent: Option<u64>,
}
impl Drop for CommandLoan {
    fn drop(&mut self) {
        self.active.set(self.parent);
    }
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
    pub fn set_user_functions(&mut self, functions: calc_fml::CallableTable) {
        self.host.set_user_functions(functions);
        self.deliver_changes(
            DeltaCause::CallablesChanged {
                revision: self.host.revision(),
            },
            false,
        );
    }

    /// Bind IMPORTCSV and IMPORTTEXT to a host's file-read route.
    pub fn set_import_route(&mut self, route: std::rc::Rc<dyn calc_func::ImportRoute>) {
        self.host.set_import_route(route);
        self.deliver_changes(
            DeltaCause::CallablesChanged {
                revision: self.host.revision(),
            },
            false,
        );
    }

    /// Bind WEBSERVICE and IMAGE to a host's fetch route.
    pub fn set_fetch_route(&mut self, route: std::rc::Rc<dyn calc_func::FetchRoute>) {
        self.host.set_fetch_route(route);
        self.deliver_changes(
            DeltaCause::CallablesChanged {
                revision: self.host.revision(),
            },
            false,
        );
    }

    /// Publish the file identity of a successful Save As without adding an undo step.
    pub fn set_file_identity(&mut self, name: &str, macro_enabled: bool) {
        if self.host.model.definitions().workbook_name() == Some(name)
            && self.host.model.definitions().macro_enabled() == macro_enabled
        {
            return;
        }
        self.cancel_calculation();
        self.host.model = self.host.model.with_file_identity(name, macro_enabled);
        self.notify_file_identity(name);
        self.host.engine.rebuild_bindings();
        self.host.published = None;
        self.host.last_failure = None;
        self.deliver_changes(
            DeltaCause::CallablesChanged {
                revision: self.host.revision(),
            },
            true,
        );
    }

    /// A session over an open host, in the host's calculation mode, with nothing selected.
    pub fn new(host: HostState) -> HostSession {
        HostSession {
            workbook_events: crate::events::WorkbookEventState::default(),
            recalc_budget: WorkBudget::UNLIMITED,
            sheet_keys: host.sheets().iter().map(|s| s.key()).collect(),
            host,
            history: History::new(UNDO_LIMIT),
            selection: Selection::default(),
            subscriptions: BTreeMap::new(),
            next_subscription: 1,
            log: Vec::new(),
            next_seq: 1,
            active_command: Rc::new(Cell::new(None)),
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
    /// Includes a command parked on an external provider. This is completion
    /// state, separate from the worker's decision to schedule another task.
    pub fn has_unfinished_calculation(&self) -> bool {
        self.host.work.is_some() || self.has_pending_calculation()
    }
    pub fn take_calculation_events(&mut self) -> Vec<calc_engine::CalcEvent> {
        self.host.take_calculation_events()
    }

    /// The owner's capability to join or start RTD topics while this session evaluates. A
    /// provisional value it returns is admitted by the host before the result is published.
    pub fn set_rtd_connector(&mut self, connector: std::rc::Rc<dyn crate::RtdConnector>) {
        self.host.set_rtd_connector(connector);
    }

    /// The RTD values admitted since the last call, in admission order.
    pub fn take_rtd_admissions(&mut self) -> Vec<crate::RtdAdmission> {
        self.host.take_rtd_admissions()
    }

    /// Deliver through the same owner as ordinary application commands; resume is budgeted
    /// by advance_calculation, never performed inside the provider return.
    pub fn retire_rtd(
        &mut self,
        topic: calc_fml::TopicKey,
        lease: calc_fml::RtdLease,
    ) -> crate::RtdDisposition {
        let disposition = self.host.retire_rtd(topic, lease);
        if disposition == crate::RtdDisposition::Accepted {
            self.deliver_changes(
                DeltaCause::ExternalInputsChanged {
                    revision: self.host.revision(),
                },
                false,
            );
        }
        disposition
    }

    pub fn update_rtd(
        &mut self,
        topic: calc_fml::TopicKey,
        lease: calc_fml::RtdLease,
        sequence: u64,
        value: calc_doc_model::value::CalcValue,
    ) -> crate::RtdDisposition {
        let disposition = self.host.update_rtd(topic, lease, sequence, value);
        if disposition == crate::RtdDisposition::Accepted {
            self.deliver_changes(
                DeltaCause::ExternalInputsChanged {
                    revision: self.host.revision(),
                },
                false,
            );
        }
        disposition
    }

    pub fn complete_async(
        &mut self,
        token: &calc_engine::contract::AsyncToken,
        value: calc_doc_model::value::CalcValue,
    ) -> calc_engine::contract::AsyncDisposition {
        self.host.complete_async(token, value)
    }

    pub fn cancel_calculation(&mut self) {
        self.host.revoke_calculation_handles();
        if let Some(mut work) = self.host.work.take() {
            self.host.record_calc_events(work.take_events());
            let receipt = work.completion(
                calc_engine::contract::CompletionOutcome::Cancelled,
                self.host.engine.result_view(),
                None,
            );
            self.host
                .record_calc_events(vec![calc_engine::CalcEvent::Completed(receipt)]);
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
            ingress: CommandIngress::UserInterface,
            seq: self.next_seq,
            parent: self.active_command.get(),
            error: None,
            committed: None,
            recalc: None,
            revision: self.host.revision(),
            deltas: 0,
        };
        self.recalculate(&mut receipt);
        receipt.recalc.expect("one step always reports progress")
    }

    /// Optional bounded physical compaction. No user code runs and no result certificate changes.
    pub fn compact_result_storage(
        &mut self,
        max_slots: usize,
    ) -> Option<calc_engine::ResultStorageStats> {
        self.host.engine.compact_result_storage(max_slots)
    }

    /// Optional authored-source storage compaction at the same revision. No history,
    /// calculation, effect, result view or existing pin is changed.
    pub fn compact_formula_sources(
        &mut self,
        budget: calc_doc::FormulaSourceBudget,
    ) -> Option<calc_doc::RegroupedFormulaSources> {
        let regrouped = calc_doc::regroup_formula_sources(&self.host.model, budget)?;
        self.host.model = regrouped.model.clone();
        Some(regrouped)
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
    pub fn prepare_calculated_save(
        &mut self,
        policy: &SavePolicy,
    ) -> Result<crate::CalculatingSave, crate::CalculatedSaveError> {
        crate::begin_calculated_save(&mut self.host, policy)
    }
    pub fn finish_calculated_save(
        &mut self,
        save: crate::CalculatingSave,
    ) -> Result<crate::PreparedSave, crate::CalculatedSaveError> {
        save.finish(&mut self.host)
    }
    pub fn prepare_save(&self, policy: &SavePolicy) -> Result<crate::PreparedSave, SaveFailed> {
        crate::prepare_save(&self.host, policy)
    }

    /// Ordinary Save(false), including P06's semiautomatic TABLE calculation.
    pub fn save(
        &mut self,
        policy: &SavePolicy,
        out: &mut dyn PackageWrite,
    ) -> Result<SaveReceipt, SaveFailed> {
        let save = self.begin_save(policy)?;
        self.finish_save_synchronously(save)?.write(out)
    }

    pub fn begin_save(&mut self, policy: &SavePolicy) -> Result<crate::OrdinarySave, SaveFailed> {
        let active_sheet = self
            .selection
            .active
            .map(|at| at.sheet())
            .filter(|sheet| self.host.model.sheet_by_id(*sheet).is_some())
            .or_else(|| {
                self.host
                    .loaded
                    .active_sheet()
                    .filter(|sheet| self.host.model.sheet_by_id(*sheet).is_some())
            })
            .or_else(|| self.host.model.sheets().first().map(|sheet| sheet.id()))
            .ok_or_else(|| crate::save::operation_failed("The save has no active worksheet"))?;
        self.begin_save_with_context(
            policy,
            crate::ExcelSaveContext {
                operation: crate::SaveOperation::Save,
                calculate_before_save: false,
                active_sheet,
                disabled_sheets: &[],
                before_save: None,
            },
        )
    }

    /// Begins an ordinary live save. Handlers run once before its model pin;
    /// calculations advance only in later budgeted owner calls.
    pub fn begin_save_with_context(
        &mut self,
        policy: &SavePolicy,
        context: crate::ExcelSaveContext<'_>,
    ) -> Result<crate::OrdinarySave, SaveFailed> {
        let demand = self.normalize_save_context(context)?;
        crate::OrdinarySave::begin(self, policy, demand)
    }

    /// Synchronous convenience over the same cooperative save. A provider wait
    /// returns an explicit error; cooperative callers retain the save instead.
    pub fn prepare_save_with_context(
        &mut self,
        policy: &SavePolicy,
        context: crate::ExcelSaveContext<'_>,
    ) -> Result<crate::PreparedSave, SaveFailed> {
        let save = self.begin_save_with_context(policy, context)?;
        self.finish_save_synchronously(save)
    }

    fn finish_save_synchronously(
        &mut self,
        mut save: crate::OrdinarySave,
    ) -> Result<crate::PreparedSave, SaveFailed> {
        loop {
            match save.advance(self, self.recalc_budget) {
                crate::CalculatedSaveProgress::Ready | crate::CalculatedSaveProgress::Failed => {
                    return save.finish();
                }
                crate::CalculatedSaveProgress::Waiting
                | crate::CalculatedSaveProgress::WaitingForLive => {
                    return Err(crate::save::operation_failed(
                        "The save calculation awaits a provider return",
                    ));
                }
                crate::CalculatedSaveProgress::Calculating => {}
            }
        }
    }

    pub(crate) fn normalize_save_context(
        &mut self,
        mut context: crate::ExcelSaveContext<'_>,
    ) -> Result<crate::save::SaveDemand, SaveFailed> {
        use crate::save::operation_failed;
        if self.host.model.sheet_by_id(context.active_sheet).is_none()
            || context
                .disabled_sheets
                .iter()
                .any(|sheet| self.host.model.sheet_by_id(*sheet).is_none())
        {
            return Err(operation_failed(
                "The save context names an unavailable worksheet",
            ));
        }
        if context.operation != crate::SaveOperation::SaveCopyAs && self.workbook_events_enabled() {
            let outcome = if let Some(handler) = context.before_save.as_mut() {
                handler.before_save(&self.host, context.operation)
            } else {
                self.notify_workbook_before_save(context.operation)
            }
            .map_err(operation_failed)?;
            if !outcome.edits.is_empty() {
                let receipt = self.dispatch(SkinIntent::Edit {
                    edits: outcome.edits,
                });
                if let Some(error) = receipt.error {
                    return Err(operation_failed(error.to_string()));
                }
            }
            if outcome.cancel {
                return Err(SaveFailed::Cancelled);
            }
        }
        if self.host.model.sheet_by_id(context.active_sheet).is_none() {
            return Err(operation_failed(
                "The BeforeSave handler removed the active worksheet",
            ));
        }
        let enabled = |sheet| !context.disabled_sheets.contains(&sheet);
        let scope = if !enabled(context.active_sheet) {
            None
        } else if context.calculate_before_save {
            Some(if context.disabled_sheets.is_empty() {
                calc_engine::CalcScope::Workbook
            } else {
                calc_engine::CalcScope::Range(
                    self.host
                        .model
                        .sheets()
                        .iter()
                        .filter(|sheet| enabled(sheet.id()))
                        .map(|sheet| GridArea::full_sheet(sheet.id()))
                        .collect(),
                )
            })
        } else if self.calc_mode() == CalcMode::AutomaticExceptTables {
            // Ordinary formulas have already had their automatic demand. Save(false)
            // admits the TABLE outputs and their existing dependent cone, not unrelated
            // volatile callbacks. This is the P06 exception to a noncalculating save.
            let mut cells: BTreeSet<_> = self
                .host
                .engine
                .nodes()
                .map(|node| node.anchor())
                .filter(|at| {
                    enabled(at.sheet())
                        && self
                            .host
                            .model
                            .definitions()
                            .data_table_containing(*at)
                            .is_some()
                })
                .collect();
            let mut stack: Vec<_> = cells.iter().copied().collect();
            while let Some(at) = stack.pop() {
                for next in self.host.engine.dependents_at(&self.host.model, at) {
                    if enabled(next.sheet()) && cells.insert(next) {
                        stack.push(next);
                    }
                }
            }
            (!cells.is_empty()).then(|| {
                calc_engine::CalcScope::Range(cells.into_iter().map(GridArea::cell).collect())
            })
        } else {
            None
        };
        let caches = if self.calc_mode() == CalcMode::Manual || !context.disabled_sheets.is_empty()
        {
            crate::SaveCachePolicy::PreservePermittedCaches
        } else {
            crate::SaveCachePolicy::CurrentCalculatedOnly
        };
        Ok(crate::save::SaveDemand {
            scope,
            caches,
            calculate_before_save: context.calculate_before_save,
        })
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
        self.dispatch_from(CommandIngress::UserInterface, intent)
    }

    /// Automation uses the same authored transaction and evaluator as the UI.
    /// Its synchronous automatic calculation precedes Change (P22, 20527 COM).
    pub fn dispatch_from(&mut self, ingress: CommandIngress, intent: SkinIntent) -> IntentReceipt {
        let seq = self.next_seq;
        self.next_seq = self
            .next_seq
            .checked_add(1)
            .expect("host command identity exhausted");
        let parent = self.active_command.replace(Some(seq));
        let _loan = CommandLoan {
            active: self.active_command.clone(),
            parent,
        };
        let mut receipt = IntentReceipt {
            ingress,
            seq,
            parent,
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
            SkinIntent::Command(crate::WorkbookCommand::InsertTemplate {
                bytes,
                name,
                revision,
            }) => self.insert_template(bytes, name, revision, &mut receipt),
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
                let after_selection = match &command {
                    crate::WorkbookCommand::Paste(plan) => plan.selection(),
                    _ => None,
                };
                crate::actions::prepare(&self.host, &mut self.sheet_keys, command)
                    .map_err(IntentError::Workbook)
                    .and_then(|edits| {
                        if edits.is_empty() {
                            return Ok(());
                        }
                        let edits = self.authored_edits(edits);
                        self.edit_coordinates(edits, coordinates, after_selection, &mut receipt)
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
                let edits = self.authored_edits(edits);
                self.edit(edits, &mut receipt)
            }
            SkinIntent::EditAxis(edit) => self.edit(vec![CellEdit::Axis(edit)], &mut receipt),
            SkinIntent::Copy { from, to, cut } => {
                match crate::copy::copy_edits(&self.host, from, to, cut) {
                    Ok(edits) => {
                        let edits = self.authored_edits(edits);
                        let coordinates = cut
                            .then(|| crate::copy::move_coordinates(&self.host, from, to.first()));
                        self.edit_coordinates(edits, coordinates, None, &mut receipt)
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
            SkinIntent::Rebuild => {
                if self.host.work.is_some() {
                    Err(IntentError::Workbook(
                        "Finish or cancel the active calculation before rebuilding dependencies"
                            .into(),
                    ))
                } else {
                    self.host.engine.rebuild_bindings();
                    self.host.published = None;
                    self.request_calculation(
                        calc_engine::CalcScope::Workbook,
                        calc_engine::EvalSelection::ForceAll,
                        &mut receipt,
                    )
                }
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
                if self.host.calc_mode() != mode {
                    self.host.revoke_calculation_handles();
                }
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
            parent,
            ingress,
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
        if selection == calc_engine::EvalSelection::FormulaEntry
            && !matches!(&scope, calc_engine::CalcScope::Range(areas) if areas.len() == 1 && areas[0].cell_count() == 1)
        {
            return Err(IntentError::Workbook(
                "Formula entry must name exactly one assigned cell".into(),
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
        self.authored_with_names(edit, &BTreeMap::new())
    }

    fn authored_edits(&self, edits: Vec<CellEdit>) -> Vec<CellEdit> {
        let mut pending =
            BTreeMap::<calc_doc_model::value::NameKey, std::collections::VecDeque<String>>::new();
        for edit in &edits {
            match edit {
                CellEdit::SetName {
                    name,
                    scope,
                    formula,
                } => {
                    let key = calc_doc_model::value::NameKey::new(*scope, name);
                    let queue = pending.entry(key.clone()).or_insert_with(|| {
                        self.host
                            .model
                            .definitions()
                            .name_table()
                            .matching(&key)
                            .map(|node| node.definition().formula.clone())
                            .collect()
                    });
                    match formula {
                        Some(formula) => {
                            if let Some(first) = queue.front_mut() {
                                *first = formula.clone();
                            } else {
                                queue.push_back(formula.clone());
                            }
                        }
                        None => {
                            queue.pop_front();
                        }
                    }
                }
                CellEdit::RestoreNames(names) => {
                    for snapshot in names {
                        pending.insert(
                            snapshot.key.clone(),
                            snapshot
                                .records
                                .iter()
                                .map(|record| record.definition.formula.clone())
                                .collect(),
                        );
                    }
                }
                _ => {}
            }
        }
        let names: BTreeMap<_, _> = pending
            .into_iter()
            .map(|(key, mut values)| (key, values.pop_front()))
            .collect();
        edits
            .into_iter()
            .map(|edit| self.authored_with_names(edit, &names))
            .collect()
    }

    fn authored_with_names(
        &self,
        edit: CellEdit,
        names: &BTreeMap<calc_doc_model::value::NameKey, Option<String>>,
    ) -> CellEdit {
        let file_form = |formula: &FormulaText, sheet| {
            let lookup = |qualifier: Option<&str>, spelling: &str| {
                let local = match qualifier {
                    Some(name) => self
                        .host
                        .model
                        .sheets()
                        .iter()
                        .find(|entry| {
                            entry
                                .name()
                                .text()
                                .to_string_lossy()
                                .chars()
                                .flat_map(char::to_uppercase)
                                .eq(name.chars().flat_map(char::to_uppercase))
                        })
                        .map(|entry| entry.id())?,
                    None => sheet,
                };
                for scope in [
                    Some(calc_doc_model::NameScope::Sheet(local)),
                    qualifier
                        .is_none()
                        .then_some(calc_doc_model::NameScope::Workbook),
                ]
                .into_iter()
                .flatten()
                {
                    let key = calc_doc_model::value::NameKey::new(scope, spelling);
                    match names.get(&key) {
                        Some(Some(formula)) => return Some(formula.as_str()),
                        Some(None) => continue,
                        None => {
                            if let Some(node) =
                                self.host.model.definitions().name_table().lookup_key(&key)
                            {
                                return Some(node.definition().formula.as_str());
                            }
                        }
                    }
                }
                None
            };
            let links = self.host.model.definitions().external_file_names();
            let form = calc_doc::file_form_with_lookup(formula, &lookup, &links).ok()?;
            let dynamic = if formula.is_form_explicit() {
                formula.form() == calc_doc_model::FormulaForm::Dynamic
            } else {
                form.dynamic
            };
            let text = FormulaText::with_form(
                ExcelText::from_utf16_code_units(form.text),
                if dynamic {
                    calc_doc_model::FormulaForm::Dynamic
                } else {
                    calc_doc_model::FormulaForm::Legacy
                },
            )
            .ok()?;
            Some((text, dynamic))
        };
        match edit {
            CellEdit::Set {
                at,
                input: CellInput::Formula(formula),
            } => match file_form(&formula, at.sheet()) {
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
            CellEdit::SetArray { array, formula } => {
                let text =
                    file_form(&formula, array.anchor().sheet()).map_or(formula, |(text, _)| text);
                CellEdit::SetArray {
                    array,
                    formula: FormulaText::with_form(
                        text.text().clone(),
                        calc_doc_model::FormulaForm::Dynamic,
                    )
                    .expect("a nonempty array source"),
                }
            }
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
        self.edit_coordinates(edits, None, None, receipt)
    }

    fn edit_coordinates(
        &mut self,
        edits: Vec<CellEdit>,
        coordinates: Option<calc_fml::reference::StructuralEdit>,
        after_selection: Option<Selection>,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        let before = self.selection.clone();
        let inverse = inverse(&self.host.model, &self.host.axis_edits, &edits);
        self.commit_coordinates(&edits, coordinates.as_ref(), receipt)?;
        if let Some(selection) = after_selection {
            self.select_quietly(selection, receipt);
        }
        self.history.record(Step {
            forward: edits,
            imported: None,
            coordinates,
            inverse,
            before,
            after: self.selection.clone(),
        });
        self.after_commit(receipt);
        Ok(())
    }

    fn commit_coordinates(
        &mut self,
        edits: &[CellEdit],
        coordinates: Option<&calc_fml::reference::StructuralEdit>,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        let scope = coordinates.and_then(|edit| {
            crate::command::active_scope(&self.host).map(|scope| scope.transformed(edit))
        });
        let refresh_rules = edits.iter().any(|edit| {
            matches!(
                edit,
                CellEdit::ConditionalRules { .. }
                    | CellEdit::Tables(_)
                    | CellEdit::Styles(_)
                    | CellEdit::SetFormat { .. }
            )
        });
        let base = self.host.revision();
        let committed =
            dispatch_slice(&mut self.host, base, edits).map_err(IntentError::Command)?;
        if let Some(edit) = coordinates {
            self.host.references.apply(edit);
        }
        if let Some(scope) = scope {
            self.host.transformed_calculation_scope = Some(scope);
        }
        let revision = committed.revision();
        receipt.committed = Some(committed);
        receipt.deltas += self.deliver_changes(DeltaCause::Committed { revision }, refresh_rules);
        Ok(())
    }

    fn after_commit(&mut self, receipt: &mut IntentReceipt) {
        if receipt.ingress == CommandIngress::UserInterface
            && let Some(committed) = &receipt.committed
        {
            self.notify_workbook_change(committed);
        }
        if self.host.calc_mode() != CalcMode::Manual {
            let budget = self.recalc_budget;
            if receipt.ingress == CommandIngress::Automation {
                self.recalc_budget = WorkBudget::UNLIMITED;
            }
            self.recalculate(receipt);
            if receipt.ingress == CommandIngress::Automation {
                // An ordinary COM setter returns after synchronous calculation.
                // Reuse the owner steps, including every prefix publication;
                // restore the caller budget without starting a second command.
                while self.has_pending_calculation()
                    && !matches!(receipt.recalc, Some(HostProgress::Waiting { .. }))
                {
                    self.recalculate(receipt);
                }
            }
            self.recalc_budget = budget;
        }
        if receipt.ingress == CommandIngress::Automation
            && let Some(committed) = &receipt.committed
        {
            if self.has_unfinished_calculation()
                || matches!(
                    receipt.recalc,
                    Some(
                        HostProgress::Waiting { .. }
                            | HostProgress::Failed(_)
                            | HostProgress::Cancelled
                    )
                )
            {
                self.refuse_automation_change(committed);
            } else {
                self.notify_workbook_change(committed);
            }
        }
    }

    fn restore_template_source(
        &mut self,
        source: calc_doc::LoadedWorkbook,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        let committed =
            crate::command::restore_package_source(&mut self.host, source.model(), &source)
                .map_err(IntentError::Command)?;
        self.host.loaded = source;
        self.host.axis_edits.clear();
        let revision = committed.revision();
        receipt.committed = Some(committed);
        receipt.deltas += self.deliver_changes(DeltaCause::Committed { revision }, true);
        Ok(())
    }
    fn insert_template(
        &mut self,
        bytes: Arc<[u8]>,
        name: Option<String>,
        revision: u64,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        let source =
            crate::templates::prepare(&mut self.host, &self.sheet_keys, bytes, name, revision)
                .map_err(IntentError::Workbook)?;
        let inverse = Ok(Inverse::Restore {
            model: self.host.model.clone(),
            axis_edits: self.host.axis_edits.clone(),
            source: Some(self.host.loaded.clone()),
        });
        self.restore_template_source(source.clone(), receipt)?;
        for sheet in self.host.model.sheets() {
            self.sheet_keys.insert(sheet.key());
            self.host.sheet_identities.insert(sheet.key(), sheet.id());
        }
        self.history.record(Step {
            forward: Vec::new(),
            coordinates: None,
            imported: Some(source),
            inverse,
            before: self.selection.clone(),
            after: self.selection.clone(),
        });
        self.after_commit(receipt);
        Ok(())
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
                &edits,
                step.coordinates
                    .as_ref()
                    .map(crate::history::inverse_coordinates)
                    .as_ref(),
                receipt,
            ),
            Inverse::Restore {
                model,
                axis_edits,
                source,
            } => {
                let refreshed = source.is_some();
                let result = if let Some(source) = &source {
                    crate::command::restore_package_source(&mut self.host, &model, source)
                } else {
                    crate::command::restore(&mut self.host, &model, axis_edits.clone())
                };
                match result {
                    Ok(committed) => {
                        if let Some(source) = source {
                            self.host.loaded = source;
                            self.host.axis_edits = axis_edits;
                        }
                        let revision = committed.revision();
                        receipt.committed = Some(committed);
                        receipt.deltas +=
                            self.deliver_changes(DeltaCause::Committed { revision }, refreshed);
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
        let result = if let Some(source) = &step.imported {
            self.restore_template_source(source.clone(), receipt)
        } else {
            self.commit_coordinates(&step.forward, step.coordinates.as_ref(), receipt)
        };
        if let Err(error) = result {
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
        let (progress, deltas) = self.advance_observed(self.recalc_budget, &mut Unobserved);
        receipt.deltas += deltas;
        receipt.recalc = Some(progress);
    }

    pub(crate) fn advance_observed(
        &mut self,
        budget: WorkBudget,
        observer: &mut dyn calc_engine::ReadObserver,
    ) -> (HostProgress, usize) {
        self.host.calculation_interest = self
            .subscriptions
            .values()
            .flat_map(|subscription| self.interest_areas(&subscription.interest))
            .collect();
        let progress = step(&mut self.host, budget, observer);
        let cause = match &progress {
            HostProgress::Current | HostProgress::Manual | HostProgress::Cancelled => None,
            HostProgress::Calculating { .. } | HostProgress::Waiting { .. } => None,
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
        let deltas = cause.map_or(0, |cause| self.deliver_changes(cause, false));
        self.notify_workbook_calculation(&progress);
        (progress, deltas)
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
    pub fn interest_areas(&self, interest: &Interest) -> Vec<GridArea> {
        let mut areas = match interest {
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
        };
        // Seeing any part of a merge also observes its source anchor. Resolve this on every
        // snapshot so structural edits cannot publish new merge geometry without its value.
        let sheets: BTreeSet<_> = areas.iter().map(|a| a.sheet()).collect();
        for sheet in sheets {
            let anchors: Vec<_> = self
                .host
                .merged_areas(sheet)
                .into_iter()
                .filter(|merge| areas.iter().any(|a| a.intersection(*merge).is_some()))
                .map(|merge| merge.first())
                .collect();
            for anchor in anchors {
                if !areas.iter().any(|a| a.contains(anchor)) {
                    areas.push(GridArea::cell(anchor));
                }
            }
        }
        areas
    }

    /// The interest's cells that are not empty: every cell holding an input, every cell an array
    /// formula's area holds, every cell a published spill fills, and every unloaded cell,
    /// projected.
    fn snapshot(&self, interest: &Interest) -> BTreeMap<CellKey, ProjectedCell> {
        let mut cells = BTreeSet::new();
        let definitions = self.host.model.definitions();
        let areas = self.interest_areas(interest);
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
                cells.extend(cells_of(Some(area)).filter(|at| self.host.cell_style(*at) != 0));
                for rule_area in &rule_areas {
                    cells.extend(cells_of(intersection(*rule_area, area)));
                }
            } else {
                // A sheet-wide observer still receives small authored style-only regions.
                // Large defaults are expanded only by bounded viewport subscriptions.
                let mut remaining = 32768;
                for run in self.host.styles().runs.iter().filter(|r| r.format != 0) {
                    if let Some(part) = intersection(run.area, area)
                        && part.cell_count() <= remaining
                    {
                        remaining -= part.cell_count();
                        cells.extend(
                            cells_of(Some(part)).filter(|at| self.host.cell_style(*at) != 0),
                        );
                    }
                }
            }
            // Explicit pivot outputs have no authored cell inputs. Every skin subscription uses
            // the same sparse engine-owned output map as direct projections and formula reads.
            cells.extend(
                self.host
                    .engine
                    .pivot_results()
                    .cells
                    .range(area.first()..=area.last())
                    .filter(|(at, _)| area.contains(**at))
                    .map(|(at, _)| *at),
            );
            for array in definitions.arrays_meeting(area) {
                cells.extend(cells_of(intersection(array.area(), area)));
            }
            // The cells a published spill fills, which no input or definition names: a dynamic
            // array's result spills past the area the model gives it.
            for spill in self.host.engine.spills() {
                cells.extend(cells_of(intersection(spill.area(), area)));
            }
            cells.extend(definitions.unloaded_in(area).map(|(at, _)| at));
            cells.extend(
                definitions
                    .cell_formats()
                    .filter_map(|(at, style)| (area.contains(at) && style != 0).then_some(at)),
            );
            if let Some(sheet) = self.host.loaded.sheet(area.sheet()) {
                let styled: Vec<CellKey> = sheet
                    .styled_cells()
                    .filter_map(|(row, col, _)| {
                        let at = CellKey::new(area.sheet(), row, col).ok()?;
                        let at = self
                            .host
                            .axis_edits
                            .iter()
                            .try_fold(at, |at, edit| calc_doc_model::moved_cell(*edit, at))?;
                        (area.contains(at) && self.host.cell_style(at) != 0).then_some(at)
                    })
                    .collect();
                // A file holds a formatted rectangle as one record per cell. Like the style runs
                // above, a sheet-wide or large-selection observer receives only small authored
                // style-only regions: publishing 40,000 blank styled cells for a large selection
                // streams them tile by tile, and every later reply waits behind that stream
                // (dc-0f1k.11.3). Bounded viewport subscriptions still receive every one.
                if bounded || styled.len() <= 32768 {
                    cells.extend(styled);
                }
            }
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
                    || self.host.cell_style(cell.at()) != 0
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

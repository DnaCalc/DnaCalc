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
    /// Text typed into a cell, read as Excel reads a cell entry ([`crate::interpret_entry`]): a
    /// formula, a value, or, for an empty text, a clear. One transaction, one undo step.
    EnterCell { at: CellKey, text: String },
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
    /// The entered text is refused.
    Entry(EntryRefusal),
    /// The transaction was refused; the workbook is as it was.
    Command(CommandError),
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
            IntentError::Entry(refusal) => write!(f, "the entry was refused: {refusal}"),
            IntentError::Command(error) => write!(f, "the edit was refused: {error}"),
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
        let receipt = session.dispatch(record.intent.clone());
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
    host: HostState,
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
        self.next_seq += 1;
        let mut receipt = IntentReceipt {
            seq,
            error: None,
            committed: None,
            recalc: None,
            revision: self.host.revision(),
            deltas: 0,
        };
        let outcome = match intent.clone() {
            SkinIntent::EnterCell { at, text } => {
                let entry = interpret_entry_in(&text, Some(&self.host.entry_environment()));
                // A stop rule of the cell's data validation refuses an entry it does not allow.
                let refused = match &entry {
                    Ok(input) => match self.host.check_entry(at, input.as_ref()) {
                        Err(failure) if failure.style == crate::validation::ErrorStyle::Stop => {
                            Some(failure)
                        }
                        _ => None,
                    },
                    Err(_) => None,
                };
                match entry {
                    _ if refused.is_some() => Err(IntentError::Validation(
                        refused.expect("checked just above"),
                    )),
                    Ok(Some(input)) => {
                        let edit = self.entry_edit(at, input);
                        self.edit(vec![edit], &mut receipt)
                    }
                    Ok(None) => self.edit(
                        vec![CellEdit::Clear {
                            area: GridArea::cell(at),
                        }],
                        &mut receipt,
                    ),
                    Err(refusal) => Err(IntentError::Entry(refusal)),
                }
            }
            SkinIntent::Edit { edits } => {
                let edits = edits.into_iter().map(|edit| self.authored(edit)).collect();
                self.edit(edits, &mut receipt)
            }
            SkinIntent::EditAxis(edit) => self.edit(vec![CellEdit::Axis(edit)], &mut receipt),
            SkinIntent::Copy { from, to, cut } => {
                match crate::copy::copy_edits(&self.host, from, to, cut) {
                    Ok(edits) => {
                        let edits = edits.into_iter().map(|edit| self.authored(edit)).collect();
                        self.edit(edits, &mut receipt)
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
        receipt.revision = self.host.revision();
        self.log.push(IntentRecord {
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
        let sheet = match interest {
            Interest::Area(area) => area.sheet(),
            Interest::Sheet(sheet) => *sheet,
            Interest::Selection => return Ok(()),
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
        let inverse = inverse(&self.host.model, &self.host.axis_edits, &edits);
        self.commit(edits.clone(), receipt)?;
        self.history.record(Step {
            forward: edits,
            inverse,
            before: self.selection.clone(),
            after: self.selection.clone(),
        });
        self.after_commit(receipt);
        Ok(())
    }

    fn commit(
        &mut self,
        edits: Vec<CellEdit>,
        receipt: &mut IntentReceipt,
    ) -> Result<(), IntentError> {
        let base = self.host.revision();
        let committed =
            dispatch(&mut self.host, HostCommand { base, edits }).map_err(IntentError::Command)?;
        let revision = committed.revision();
        receipt.committed = Some(committed);
        receipt.deltas += self.deliver_changes(DeltaCause::Committed { revision });
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
            Inverse::Edits(edits) => self.commit(edits, receipt),
            Inverse::Restore { model, axis_edits } => {
                match crate::command::restore(&mut self.host, &model, axis_edits) {
                    Ok(committed) => {
                        let revision = committed.revision();
                        receipt.committed = Some(committed);
                        receipt.deltas += self.deliver_changes(DeltaCause::Committed { revision });
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
        if let Err(error) = self.commit(step.forward.clone(), receipt) {
            self.history.restore_redo(step);
            return Err(error);
        }
        let selection = step.after.clone();
        self.history.redone(step);
        self.select_quietly(selection, receipt);
        self.after_commit(receipt);
        Ok(())
    }

    /// Runs the recalculation to the end and delivers what it changed.
    fn recalculate(&mut self, receipt: &mut IntentReceipt) {
        let progress = loop {
            match step(&mut self.host, WorkBudget::UNLIMITED, &mut Unobserved) {
                HostProgress::Calculating { .. } => continue,
                progress => break progress,
            }
        };
        let cause = match &progress {
            HostProgress::Current | HostProgress::Manual => None,
            HostProgress::Calculating { .. } => unreachable!("the loop runs to the end"),
            HostProgress::Published(delta) => Some(DeltaCause::Recalculated {
                revision: delta.revision(),
                tick: delta.tick(),
            }),
            HostProgress::Failed(_) => Some(DeltaCause::RecalcFailed {
                revision: self.host.revision(),
            }),
        };
        if let Some(cause) = cause {
            receipt.deltas += self.deliver_changes(cause);
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
        for area in self.areas(interest) {
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
            .filter(|cell| cell.value() != &ProjectedValue::Empty || cell.input().is_some())
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
    fn deliver_changes(&mut self, cause: DeltaCause) -> usize {
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

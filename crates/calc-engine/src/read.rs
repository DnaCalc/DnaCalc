//! What a recalculation's evaluations read: the sheets of the input snapshot for CalcFml's
//! binding, and the cells of the workbook for its evaluation.
//!
//! A literal cell's value is its authored input, read from the snapshot's page. A formula cell's
//! value is the result this recalculation staged for it, or else the published result when that
//! was calculated; a file cache, a result not modelled and a formula this recalculation has yet
//! to evaluate are never handed out as values. The engine's reader is the only source of what an
//! evaluation reads: a caller's [`ReadObserver`] sees the runs it reports and cannot change them,
//! and is shown what CalcFml's evaluator reports of each evaluation it ran.

use crate::result::{Calculation, FormulaResult};
use crate::state::EngineState;
use calc_doc_model::{CellInput, ModelRoot, NameScope};
use calc_fml::reference::{CellRun, NameDefinition, NameScope as FmlNameScope, ReferenceReader};
use calc_fml::{EvalFault, EvaluationReport, ExecutedCall, ReferenceCatalog};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, SheetId};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

/// Observes a recalculation's evaluations on the evaluation route, apart from what the
/// recalculation publishes (plan 6.5 and 10.1). [`crate::step`] shows it:
///
/// - [`ReadObserver::observe`]: each run the engine's reader reports for an area that the
///   evaluation of the formula at `formula` asks for, before the evaluation receives that run, so
///   it sees exactly the runs the evaluation receives. `offset` is the index of the run's first
///   cell within `area`, in row-major order from 0: a run at offset 0 begins a read.
/// - [`ReadObserver::called`]: each catalogue call the evaluation ran, as CalcFml's evaluator
///   reports it ([`calc_fml::EvaluationObserver`]), with the near-zero check's inserted negations
///   marked synthetic.
/// - [`ReadObserver::evaluated`]: the outcome of each evaluation, in the report CalcFml's
///   evaluator makes before returning it ([`calc_fml::EvaluationReport`], which only the evaluator
///   can make), forwarded unchanged with the recalculation's revision, tick and numeric mode
///   ([`ObservedEvaluation`]). It is shown before `calc_fml::evaluate` returns to the engine, so it
///   is not the engine's account of the value it then stages; [`crate::finish`], which publishes,
///   is given no observer. A formula CalcFml refused to compile is not evaluated and is not shown.
///
/// An observer cannot change what an evaluation reads or returns: it is given no reader, sink or
/// result, only what it is shown, borrowed. Its one effect is to stop the recalculation: returning
/// [`ControlFlow::Break`] from `observe` withholds the run, the evaluation receives the rest of
/// the area as cells the reader cannot supply, and the recalculation fails with
/// [`crate::RecalcFailure::ReadFailed`] and publishes nothing. `called` and `evaluated` record
/// nothing unless an observer implements them.
pub trait ReadObserver {
    fn observe(
        &mut self,
        formula: CellKey,
        area: GridArea,
        offset: u64,
        run: CellRun<'_>,
    ) -> ControlFlow<()>;

    /// A catalogue call the evaluation of the formula at `formula` ran.
    fn called(&mut self, formula: CellKey, call: ExecutedCall) {
        let _ = (formula, call);
    }

    /// One evaluation's outcome, as the evaluator reported it.
    fn evaluated(&mut self, evaluation: ObservedEvaluation<'_>) {
        let _ = evaluation;
    }
}

/// One evaluation as [`ReadObserver::evaluated`] is shown it: the report CalcFml's evaluator made
/// of it ([`EvaluationReport`]: the formula cell evaluated, the numeric mode and the outcome, a
/// value or a typed refusal), and the calculation the recalculation ran it in (the authored
/// revision its inputs came from, the recalculation's tick and the numeric mode).
///
/// The engine adds only the calculation. It cannot make a report, which only CalcFml's evaluator
/// can, nor change one, and it passes on each report it is shown at most once (a report is not
/// `Clone`): an evaluation the evaluator did not report cannot reach an observer. The value is
/// the evaluator's, whether or not the engine then publishes it as calculated (a value no cell
/// holds here is published as not modelled).
#[derive(Debug)]
pub struct ObservedEvaluation<'a> {
    calculation: &'a Calculation,
    report: EvaluationReport<'a>,
}

impl<'a> ObservedEvaluation<'a> {
    pub(crate) fn new(calculation: &'a Calculation, report: EvaluationReport<'a>) -> Self {
        ObservedEvaluation {
            calculation,
            report,
        }
    }

    /// The formula cell evaluated: the anchor of the instance CalcFml's evaluator ran.
    pub fn formula(&self) -> CellKey {
        self.report.anchor().cell()
    }

    pub fn calculation(&self) -> &'a Calculation {
        self.calculation
    }

    /// The value CalcFml's evaluator returned, or the fault that stopped it.
    pub fn outcome(&self) -> Result<&'a CalcValue, &'a EvalFault> {
        self.report.outcome()
    }

    /// The report as CalcFml's evaluator made it.
    pub fn report(&self) -> &EvaluationReport<'a> {
        &self.report
    }
}

/// The observer that records nothing and never stops a recalculation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Unobserved;

impl ReadObserver for Unobserved {
    fn observe(&mut self, _: CellKey, _: GridArea, _: u64, _: CellRun<'_>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }
}

/// The sheets of a snapshot by name, for CalcFml's binding. A name matches a sheet whose name has
/// the same UTF-16 code units, or, when both decode, the same characters once each is mapped to
/// its Unicode uppercase. Excel compares sheet names without case; which characters its
/// comparison folds is not measured: bead dc-85qk measures Excel's sheet-name rules, case-blind
/// matching included.
pub(crate) struct SnapshotCatalog<'r>(pub(crate) &'r ModelRoot);

impl ReferenceCatalog for SnapshotCatalog<'_> {
    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        let wanted: Vec<u16> = name.encode_utf16().collect();
        self.0
            .sheets()
            .iter()
            .find(|sheet| {
                let units = sheet.name().text().utf16_code_units();
                units == wanted.as_slice()
                    || String::from_utf16(units).is_ok_and(|held| {
                        held.chars()
                            .flat_map(char::to_uppercase)
                            .eq(name.chars().flat_map(char::to_uppercase))
                    })
            })
            .map(|sheet| sheet.id())
    }

    /// The snapshot's defined name `name`, matched as sheet names are, from `scope`: the formula
    /// sheet's own name first, then the workbook's; for a qualified name, the sheet's own. A name
    /// the snapshot does not hold is unknown (#NAME?). A macro or VBA name, and a qualified name
    /// that only the workbook holds, are not modelled.
    fn defined_name(&self, name: &str, scope: FmlNameScope) -> NameDefinition {
        let same = |held: &str| {
            held == name
                || held
                    .chars()
                    .flat_map(char::to_uppercase)
                    .eq(name.chars().flat_map(char::to_uppercase))
        };
        let names = self.0.definitions().names();
        let find = |wanted: NameScope| {
            names
                .iter()
                .find(|defined| defined.scope == wanted && same(&defined.name))
        };
        let found = match scope {
            FmlNameScope::Formula(sheet) => {
                find(NameScope::Sheet(sheet)).or_else(|| find(NameScope::Workbook))
            }
            FmlNameScope::Sheet(sheet) => match find(NameScope::Sheet(sheet)) {
                Some(defined) => Some(defined),
                None if find(NameScope::Workbook).is_some() => {
                    return NameDefinition::NotModelled;
                }
                None => None,
            },
        };
        match found {
            None => NameDefinition::Unknown,
            Some(defined) if defined.function || defined.vb_procedure || defined.xlm => {
                NameDefinition::NotModelled
            }
            Some(defined) => NameDefinition::Formula(defined.formula.clone()),
        }
    }
}

/// The engine's reader of the workbook's cells during one recalculation.
pub(crate) struct WorkbookReader<'a> {
    pub(crate) inputs: &'a ModelRoot,
    pub(crate) state: &'a EngineState,
    /// The results this recalculation has staged.
    pub(crate) staged: &'a BTreeMap<NodeId, FormulaResult>,
    /// The nodes this recalculation evaluates.
    pub(crate) scheduled: &'a BTreeSet<NodeId>,
    /// The first formula cell read that this recalculation schedules but has not evaluated yet.
    pub(crate) unscheduled: Option<CellKey>,
}

impl<'a> WorkbookReader<'a> {
    /// The value of the formula cell `at`, or `None` when it has none to hand out. A published
    /// value it hands out was calculated under this recalculation's numeric mode: when the mode
    /// differs from the published generation's, [`crate::begin_recalc`] schedules every formula
    /// node, and no published value is handed out at all.
    fn formula_value(&mut self, at: CellKey) -> Option<&'a CalcValue> {
        let (state, staged) = (self.state, self.staged);
        let node = state.node_at(at)?;
        let result = match staged.get(&node) {
            Some(staged) => staged,
            None if self.scheduled.contains(&node) => {
                self.unscheduled.get_or_insert(at);
                return None;
            }
            None => state.result(at)?,
        };
        result.calculated().map(|(value, _)| value)
    }
}

impl ReferenceReader for WorkbookReader<'_> {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let total = area.cell_count();
        let inputs = self.inputs;
        let Some(occupied) = inputs.cells(area) else {
            let _ = sink(CellRun::Missing(total));
            return;
        };
        let (first, width) = (area.first(), u64::from(area.column_count()));
        let index_of = |at: CellKey| {
            u64::from(at.row() - first.row()) * width + u64::from(at.column() - first.column())
        };
        let missing = not_available(inputs, area, index_of);
        let mut covered = 0u64;
        for (at, input) in occupied {
            let index = index_of(at);
            if index > covered && gap(covered, index, &missing, sink).is_break() {
                return;
            }
            covered = index + 1;
            let flow = match literal_value(input) {
                Some(value) => sink(CellRun::Values(&[value])),
                None => match self.formula_value(at) {
                    Some(value) => sink(CellRun::Values(std::slice::from_ref(value))),
                    None => sink(CellRun::Missing(1)),
                },
            };
            if flow.is_break() {
                return;
            }
        }
        if total > covered {
            let _ = gap(covered, total, &missing, sink);
        }
    }
}

/// The indexes, within `area` in row-major order, of the cells a calculation reads as not
/// available though the model holds no input for them: the cells an array formula fills beside
/// its anchor, and unloaded cells ([`calc_doc_model::Definitions::not_available`]). A cell that
/// holds an input is read as that input instead, so an index here may also be an occupied cell's.
fn not_available(
    inputs: &ModelRoot,
    area: GridArea,
    index_of: impl Fn(CellKey) -> u64,
) -> BTreeSet<u64> {
    let definitions = inputs.definitions();
    let mut missing = BTreeSet::new();
    for array in definitions.arrays_meeting(area) {
        let (a, b) = (array.area().first(), array.area().last());
        let (first, last) = (area.first(), area.last());
        for row in a.row().max(first.row())..=b.row().min(last.row()) {
            for column in a.column().max(first.column())..=b.column().min(last.column()) {
                let at = CellKey::new(area.sheet(), row, column).expect("a cell of the area");
                if at != array.anchor() {
                    missing.insert(index_of(at));
                }
            }
        }
    }
    for (at, _) in definitions.unloaded_in(area) {
        missing.insert(index_of(at));
    }
    missing
}

/// Reports the cells of indexes `from..to`, which hold no input: blank, except those `missing`
/// holds, which are not available.
fn gap(
    from: u64,
    to: u64,
    missing: &BTreeSet<u64>,
    sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let mut at = from;
    for &index in missing.range(from..to) {
        if index > at {
            sink(CellRun::Blank(index - at))?;
        }
        sink(CellRun::Missing(1))?;
        at = index + 1;
    }
    if to > at {
        sink(CellRun::Blank(to - at))?;
    }
    ControlFlow::Continue(())
}

/// The value of a literal input; `None` for a formula, whose value is its result.
fn literal_value(input: &CellInput) -> Option<CalcValue> {
    match input {
        CellInput::Number(number) => Some(CalcValue::number(number.get())),
        CellInput::Text(text) => Some(CalcValue::text(text.text().clone())),
        CellInput::Logical(logical) => Some(CalcValue::logical(*logical)),
        CellInput::Error(code) => Some(CalcValue::error(*code)),
        CellInput::Formula(_) => None,
    }
}

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
use crate::spill::{Spill, SpillIndex};
use crate::state::EngineState;
use calc_doc_model::{CellInput, ModelRoot, NameScope, RowHidden};
use calc_fml::reference::{
    CellRun, NameDefinition, NameScope as FmlNameScope, ReferenceReader, SpillExtent,
};
use calc_fml::{
    CalcEnvironment, EvalFault, EvaluationReport, ExecutedCall, HostFacts, ReferenceCatalog,
    ResolvedReference, SubtotalExclusions,
};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, SheetId};
use std::cell::Cell;
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

    /// The evaluation of the formula at `formula` resolved reference text to an area as it ran
    /// (INDIRECT's), as CalcFml's evaluator reported it ([`calc_fml::EvaluationObserver::resolved`]).
    /// The engine files the area as one of the formula's dynamic prerequisites.
    fn resolved(&mut self, formula: CellKey, reference: ResolvedReference) {
        let _ = (formula, reference);
    }

    /// The evaluation of the formula at `formula` read `cell`, which this recalculation has yet
    /// to calculate, so its result was discarded and the formula waits to be evaluated again once
    /// `cell` is. The calls and reads shown for that evaluation since it began were of the
    /// discarded attempt, and [`ReadObserver::evaluated`] was not shown its outcome.
    fn deferred(&mut self, formula: CellKey, cell: CellKey) {
        let _ = (formula, cell);
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

/// What one recalculation knows of the workbook's formula results while it runs: the results and
/// spills it has staged, the nodes it calculates and those it has evaluated, over the published
/// generation.
#[derive(Clone, Copy)]
pub(crate) struct RecalcView<'a> {
    pub(crate) inputs: &'a ModelRoot,
    pub(crate) state: &'a EngineState,
    /// Which nodes this recalculation calculates and has evaluated, and what it staged.
    pub(crate) progress: &'a Progress,
    /// The spills of the anchors this recalculation evaluated whose results fill cells.
    pub(crate) spills: &'a SpillIndex,
}

/// Where each node stands in one recalculation, by its ID's slot, which no node of the state
/// shares while the recalculation runs: not calculated by it, calculated and not yet evaluated,
/// or evaluated, with its staged result. A node evaluated in one wave and scheduled again in the
/// next keeps its staged result until it is evaluated again.
#[derive(Debug, Default)]
pub(crate) struct Progress {
    status: Vec<u8>,
    staged: Vec<Option<(NodeId, FormulaResult)>>,
    scheduled: usize,
}

const SCHEDULED: u8 = 1;
const EVALUATED: u8 = 2;

impl Progress {
    pub(crate) fn new(slots: usize) -> Self {
        Progress {
            status: vec![0; slots],
            staged: Vec::new(),
            scheduled: 0,
        }
    }

    fn slot(node: NodeId) -> usize {
        node.slot() as usize
    }

    /// Schedules `node`, which then stands as not yet evaluated.
    pub(crate) fn schedule(&mut self, node: NodeId) {
        let slot = Self::slot(node);
        if slot >= self.status.len() {
            self.status.resize(slot + 1, 0);
        }
        if self.status[slot] == 0 {
            self.scheduled += 1;
        }
        self.status[slot] = SCHEDULED;
    }

    pub(crate) fn is_scheduled(&self, node: NodeId) -> bool {
        self.status
            .get(Self::slot(node))
            .is_some_and(|status| *status != 0)
    }

    pub(crate) fn is_evaluated(&self, node: NodeId) -> bool {
        self.status.get(Self::slot(node)) == Some(&EVALUATED)
    }

    /// Stages `result` for the scheduled `node`, which then stands as evaluated.
    pub(crate) fn stage(&mut self, node: NodeId, result: FormulaResult) {
        let slot = Self::slot(node);
        if slot >= self.staged.len() {
            self.staged.resize_with(slot + 1, || None);
        }
        self.staged[slot] = Some((node, result));
        self.status[slot] = EVALUATED;
    }

    /// The staged result of an evaluated node.
    pub(crate) fn staged(&self, node: NodeId) -> Option<&FormulaResult> {
        self.staged
            .get(Self::slot(node))
            .and_then(Option::as_ref)
            .map(|(_, result)| result)
    }

    /// How many nodes are scheduled.
    pub(crate) fn scheduled(&self) -> usize {
        self.scheduled
    }

    /// Every staged result, with its node, in slot order.
    pub(crate) fn into_staged(self) -> impl Iterator<Item = (NodeId, FormulaResult)> {
        self.staged.into_iter().flatten()
    }
}

/// Where a formula node's result stands for a recalculation in progress.
pub(crate) enum NodeStanding<'a> {
    /// The recalculation calculates the node and has not evaluated it yet.
    Pending,
    /// The recalculation evaluated the node: its staged result.
    Staged(&'a FormulaResult),
    /// The recalculation does not calculate the node: its published result, if any.
    Published(Option<&'a FormulaResult>),
}

impl<'a> RecalcView<'a> {
    pub(crate) fn standing(&self, node: NodeId) -> NodeStanding<'a> {
        if self.progress.is_scheduled(node) {
            match self.progress.staged(node) {
                Some(result) if self.progress.is_evaluated(node) => NodeStanding::Staged(result),
                _ => NodeStanding::Pending,
            }
        } else {
            let at = self.state.node(node).map(|record| record.anchor());
            NodeStanding::Published(at.and_then(|at| self.state.result(at)))
        }
    }

    /// The spill whose filled cells hold `at` as the recalculation stands, with its anchor's node:
    /// a staged spill, or a published spill of an anchor the recalculation does not calculate.
    /// `Err` with the anchor's node and cell when the cell may lie in the spill of an anchor the
    /// recalculation calculates and has not evaluated yet. Both `Ok(None)` when no spill holds
    /// the cell.
    pub(crate) fn spill_at(&self, at: CellKey) -> Result<Option<&'a Spill>, (NodeId, CellKey)> {
        if let Some(spill) = self.spills.covering(at) {
            return Ok(Some(spill));
        }
        if let Some(spill) = self.state.spills.covering(at) {
            let anchor = spill.anchor();
            if let Some(node) = self.state.node_at(anchor) {
                match self.standing(node) {
                    NodeStanding::Pending => return Err((node, anchor)),
                    NodeStanding::Staged(_) => {}
                    NodeStanding::Published(_) => return Ok(Some(spill)),
                }
            }
        }
        Ok(None)
    }
}

/// What the reader reports for a cell that holds no input.
enum Overlay<'a> {
    /// A value an array formula's result fills the cell with.
    Value(&'a CalcValue),
    /// A cell the reader cannot supply: an unloaded cell, a cell an array formula fills whose
    /// result is not calculated, or one a formula not yet evaluated may fill.
    Missing,
    /// A blank cell.
    Blank,
}

/// The engine's reader of the workbook's cells during one recalculation.
pub(crate) struct WorkbookReader<'a> {
    pub(crate) view: RecalcView<'a>,
    /// The first cell read that this recalculation has yet to calculate, with the node that
    /// calculates it: a formula cell, or a cell the spill of a formula not yet evaluated may fill.
    uncomputed: &'a Cell<Option<(CellKey, NodeId)>>,
}

impl<'a> WorkbookReader<'a> {
    pub(crate) fn new(
        view: RecalcView<'a>,
        uncomputed: &'a Cell<Option<(CellKey, NodeId)>>,
    ) -> Self {
        WorkbookReader { view, uncomputed }
    }

    fn wait(&mut self, at: CellKey, node: NodeId) {
        if self.uncomputed.get().is_none() {
            self.uncomputed.set(Some((at, node)));
        }
    }

    /// The value of the formula cell `at`, or `None` when it has none to hand out. A published
    /// value it hands out was calculated under this recalculation's numeric mode: when the mode
    /// differs from the published generation's, [`crate::begin_recalc`] schedules every formula
    /// node, and no published value is handed out at all.
    fn formula_value(&mut self, at: CellKey) -> Option<&'a CalcValue> {
        let node = self.view.state.node_at(at)?;
        let result = match self.view.standing(node) {
            NodeStanding::Pending => {
                self.wait(at, node);
                return None;
            }
            NodeStanding::Staged(result) => result,
            NodeStanding::Published(result) => result?,
        };
        result.calculated().map(|(value, _)| value)
    }

    /// What the cell `at`, which holds no input, reads as.
    fn overlay(&mut self, at: CellKey) -> Overlay<'a> {
        let view = self.view;
        let definitions = view.inputs.definitions();
        if definitions.unloaded(at).is_some() {
            return Overlay::Missing;
        }
        match view.spill_at(at) {
            Ok(Some(spill)) => return Overlay::Value(spill.value_at(at)),
            Err((node, _)) => {
                self.wait(at, node);
                return Overlay::Missing;
            }
            Ok(None) => {}
        }
        let Some(array) = definitions
            .array_containing(at)
            .filter(|array| array.anchor() != at)
        else {
            return Overlay::Blank;
        };
        let Some(node) = view.state.node_at(array.anchor()) else {
            return Overlay::Missing;
        };
        // The file's area of an array formula: a calculated result's own spill, which the engine
        // tracks, has decided the cell is blank; any other result leaves it unknown.
        match view.standing(node) {
            NodeStanding::Pending => {
                self.wait(at, node);
                Overlay::Missing
            }
            NodeStanding::Staged(result) | NodeStanding::Published(Some(result))
                if result.calculated().is_some() =>
            {
                Overlay::Blank
            }
            NodeStanding::Staged(_) | NodeStanding::Published(_) => Overlay::Missing,
        }
    }

    /// The cells of `area` that hold no input and that an array formula, a spill or an unloaded
    /// cell may give something other than blank, by their index in `area` in row-major order.
    fn overlays(&mut self, area: GridArea) -> BTreeMap<u64, Overlay<'a>> {
        let view = self.view;
        let inputs = view.inputs;
        let definitions = inputs.definitions();
        if view.spills.is_empty()
            && view.state.spills.is_empty()
            && definitions.arrays_meeting(area).next().is_none()
            && definitions.unloaded_in(area).next().is_none()
        {
            return BTreeMap::new();
        }
        let (first, width) = (area.first(), u64::from(area.column_count()));
        let mut candidates: BTreeSet<CellKey> = BTreeSet::new();
        let mut add = |spilled: GridArea| {
            let (a, b) = (spilled.first(), spilled.last());
            let (top, bottom) = (a.row().max(first.row()), b.row().min(area.last().row()));
            let (left, right) = (
                a.column().max(first.column()),
                b.column().min(area.last().column()),
            );
            for row in top..=bottom {
                for column in left..=right {
                    let at = CellKey::new(area.sheet(), row, column).expect("a cell of the area");
                    if at != spilled.first() {
                        candidates.insert(at);
                    }
                }
            }
        };
        for spill in view.spills.meeting(area) {
            add(spill.area());
        }
        for spill in view.state.spills.meeting(area) {
            add(spill.area());
        }
        for array in definitions.arrays_meeting(area) {
            add(array.area());
        }
        candidates.extend(definitions.unloaded_in(area).map(|(at, _)| at));
        let mut out = BTreeMap::new();
        for at in candidates {
            if inputs.cell(at).is_some() {
                continue;
            }
            let index =
                u64::from(at.row() - first.row()) * width + u64::from(at.column() - first.column());
            match self.overlay(at) {
                Overlay::Blank => {}
                overlay => {
                    out.insert(index, overlay);
                }
            }
        }
        out
    }
}

impl ReferenceReader for WorkbookReader<'_> {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let total = area.cell_count();
        let inputs = self.view.inputs;
        let Some(occupied) = inputs.cells(area) else {
            let _ = sink(CellRun::Missing(total));
            return;
        };
        let (first, width) = (area.first(), u64::from(area.column_count()));
        let index_of = |at: CellKey| {
            u64::from(at.row() - first.row()) * width + u64::from(at.column() - first.column())
        };
        let overlays = self.overlays(area);
        let mut covered = 0u64;
        for (at, input) in occupied {
            let index = index_of(at);
            if index > covered && gap(covered, index, &overlays, sink).is_break() {
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
            let _ = gap(covered, total, &overlays, sink);
        }
    }

    /// What SUBTOTAL and AGGREGATE leave out of `area`: the cells whose bound formula calls
    /// SUBTOTAL or AGGREGATE, and the rows the snapshot holds hidden, by hand or by a filter
    /// ([`calc_doc_model::Definitions::hidden_rows`]); a row the snapshot does not list is visible.
    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions> {
        let view = self.view;
        let mut exclusions = SubtotalExclusions {
            nested_subtotals: view
                .state
                .subtotal_cells
                .range(area.first()..=area.last())
                .copied()
                .filter(|at| area.contains(*at))
                .collect(),
            ..SubtotalExclusions::default()
        };
        for (row, why) in view.inputs.definitions().hidden_rows(
            area.sheet(),
            area.first().row(),
            area.last().row(),
        ) {
            match why {
                RowHidden::Manual => exclusions.hidden_rows.push(row),
                RowHidden::Filtered => exclusions.filtered_rows.push(row),
            }
        }
        Some(exclusions)
    }

    fn spill_extent(&mut self, anchor: CellKey) -> SpillExtent {
        let view = self.view;
        let Some(node) = view.state.node_at(anchor) else {
            return SpillExtent::None;
        };
        let (result, spill) = match view.standing(node) {
            NodeStanding::Pending => {
                self.wait(anchor, node);
                return SpillExtent::NotModelled;
            }
            NodeStanding::Staged(result) => (Some(result), view.spills.get(anchor)),
            NodeStanding::Published(result) => (result, view.state.spills.get(anchor)),
        };
        // Excel 16.0, measured through COM on 2026-10-23: `=A1#` where A1 holds `=1+1` is 2 and
        // where A1 holds `={5}` is 5 (the anchor alone); where A4 holds `={1;2;3}` blocked by a
        // value in A5 it is #SPILL! and `=ROWS(A4#)` is 1 (the anchor alone); where A10 holds a
        // number it is #REF!. What it gives for a legacy array formula's area is not measured.
        match result {
            Some(FormulaResult::Calculated { .. }) => match spill {
                Some(spill) if spill.is_fixed() => SpillExtent::NotModelled,
                Some(spill) => SpillExtent::Area(spill.area()),
                None => SpillExtent::Area(GridArea::cell(anchor)),
            },
            _ => SpillExtent::NotModelled,
        }
    }

    fn environment(&self) -> Option<CalcEnvironment> {
        self.view.state.environment
    }

    fn host_facts(&mut self) -> Option<&mut dyn HostFacts> {
        Some(self)
    }
}

/// The workbook's facts an evaluation may ask about (calc-func's HostFacts): which cells hold
/// formulas and their text, and the sheets. The formula being evaluated is the caller's to say.
impl HostFacts for WorkbookReader<'_> {
    fn is_formula(&mut self, cell: CellKey) -> Option<bool> {
        Some(matches!(
            self.view.inputs.cell(cell),
            Some(CellInput::Formula(_))
        ))
    }

    /// A formula's text as FORMULATEXT shows it: `=` and the formula. Text that spells a function
    /// with a file prefix (`_xlfn.`, `_xlws.`, `_xludf.`), which Excel shows without it, is not
    /// modelled.
    fn formula_text(&mut self, cell: CellKey) -> Option<Option<String>> {
        match self.view.inputs.cell(cell) {
            Some(CellInput::Formula(text)) => {
                let text = String::from_utf16(text.text().utf16_code_units()).ok()?;
                let lower = text.to_ascii_lowercase();
                if ["_xlfn.", "_xlws.", "_xludf."]
                    .iter()
                    .any(|p| lower.contains(p))
                {
                    return None;
                }
                Some(Some(format!("={text}")))
            }
            _ => Some(None),
        }
    }

    fn sheet_index(&mut self, sheet: SheetId) -> Option<u32> {
        let at = self
            .view
            .inputs
            .sheets()
            .iter()
            .position(|entry| entry.id() == sheet)?;
        u32::try_from(at + 1).ok()
    }

    fn sheet_named(&mut self, name: &str) -> Option<SheetId> {
        let wanted: Vec<u16> = name.encode_utf16().collect();
        let fold = |units: &[u16]| -> Vec<u16> {
            units
                .iter()
                .map(|&u| u8::try_from(u).map_or(u, |b| u16::from(b.to_ascii_uppercase())))
                .collect()
        };
        let wanted = fold(&wanted);
        self.view
            .inputs
            .sheets()
            .iter()
            .find(|entry| fold(entry.name().text().utf16_code_units()) == wanted)
            .map(|entry| entry.id())
    }

    fn sheet_name(&mut self, sheet: SheetId) -> Option<String> {
        let entry = self
            .view
            .inputs
            .sheets()
            .iter()
            .find(|entry| entry.id() == sheet)?;
        String::from_utf16(entry.name().text().utf16_code_units()).ok()
    }

    fn sheet_count(&mut self) -> Option<u32> {
        u32::try_from(self.view.inputs.sheets().len()).ok()
    }
}

/// Reports the cells of indexes `from..to`, which hold no input: blank, except those `overlays`
/// holds.
fn gap(
    from: u64,
    to: u64,
    overlays: &BTreeMap<u64, Overlay<'_>>,
    sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let mut at = from;
    for (&index, overlay) in overlays.range(from..to) {
        if index > at {
            sink(CellRun::Blank(index - at))?;
        }
        match overlay {
            Overlay::Value(value) => sink(CellRun::Values(std::slice::from_ref(*value)))?,
            Overlay::Missing => sink(CellRun::Missing(1))?,
            Overlay::Blank => sink(CellRun::Blank(1))?,
        }
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

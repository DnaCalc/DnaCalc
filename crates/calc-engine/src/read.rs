//! What a recalculation's evaluations read: the sheets of the input snapshot for CalcFml's
//! binding, and the cells of the workbook for its evaluation.
//!
//! A literal cell's value is its authored input, read from the snapshot's page. A formula cell's
//! value is the result this recalculation staged for it, or else the published result when that
//! was calculated. Scoped scalar boundary reads use an admission-time pin and retain cache
//! qualifications; all other file-cache reads, refused results and unfinished in-scope formulas
//! are withheld. The engine's reader is the only source of what an
//! evaluation reads: a caller's [`ReadObserver`] sees the runs it reports and cannot change them,
//! and is shown what CalcFml's evaluator reports of each evaluation it ran.

use crate::result::{Calculation, FormulaResult};
use crate::spill::{Spill, SpillIndex};
use crate::state::EngineState;
use calc_doc_model::{CellInput, ExternalSheet, ModelRoot, NameScope, RowHidden};
use calc_fml::reference::{
    CellRead, CellRun, ExternalBookRef, NameDefinition, NameScope as FmlNameScope,
    ReadContractError, ReferenceReader, SpillExtent, TableInfo, TableLookup,
};
use calc_fml::{
    EvalFault, EvaluationReport, ExecutedCall, HostFacts, ReferenceCatalog, ResolvedReference,
    SubtotalExclusions,
};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, SheetId, WorksheetErrorCode};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
mod provider_tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn provider_targets_are_not_rebound_by_ambiguous_file_name() {
        let book = |folder: &str| calc_doc_model::ExternalBook {
            target: Some("probe.xlam".into()),
            absolute_target: Some(format!("file:///C:/{folder}/probe.xlam")),
            file_name: Some("probe.xlam".into()),
            sheets: Vec::new(),
        };
        let definitions = calc_doc_model::Definitions::default()
            .with_external_books(vec![Some(book("original")), Some(book("other"))]);
        let root = ModelRoot::with_definitions(
            calc_doc_model::AuthoredRevision::new(0),
            Vec::new(),
            [],
            calc_doc_model::SkinLayout::default(),
            definitions,
        )
        .unwrap();
        let catalog = SnapshotCatalog::new(&root);
        assert_eq!(
            catalog.external_function_target(&ExternalBookRef::Number(1)),
            Some("file:///C:/original/probe.xlam".into())
        );
        assert_eq!(
            catalog.external_function_target(&ExternalBookRef::Number(2)),
            Some("file:///C:/other/probe.xlam".into())
        );
        assert_eq!(
            catalog.external_function_target(&ExternalBookRef::File("probe.xlam".into())),
            None
        );
        assert_eq!(
            catalog.external_function_target(&ExternalBookRef::File(
                "file:///C:/original/probe.xlam".into()
            )),
            Some("file:///C:/original/probe.xlam".into())
        );
        assert_eq!(
            catalog.external_function_target(&ExternalBookRef::File(
                "file:///C:/missing/probe.xlam".into()
            )),
            None
        );
    }
}
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
///   ([`ObservedEvaluation`]). It is shown before the formula activation returns its terminal outcome, so it
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
    /// Whether the observer observes nothing at all (dc-2m0): none of its methods records or
    /// stops anything, so a recalculation may evaluate formulas on several threads and show it
    /// nothing in order. The default is `false`.
    fn is_silent(&self) -> bool {
        false
    }

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
    /// The cell of a what-if data table whose value the evaluation gave, when it was one.
    table_cell: Option<CellKey>,
}

impl<'a> ObservedEvaluation<'a> {
    pub(crate) fn new(calculation: &'a Calculation, report: EvaluationReport<'a>) -> Self {
        ObservedEvaluation {
            calculation,
            report,
            table_cell: None,
        }
    }

    /// The evaluation of a data table's formula under the substitution of the table cell `cell`
    /// (dc-10y): the cell's value is the value it returned.
    pub(crate) fn for_table_cell(
        calculation: &'a Calculation,
        report: EvaluationReport<'a>,
        cell: CellKey,
    ) -> Self {
        ObservedEvaluation {
            calculation,
            report,
            table_cell: Some(cell),
        }
    }

    /// The formula cell evaluated: the anchor of the instance CalcFml's evaluator ran, or for a
    /// data table's cell, that cell, whose value the evaluation of the table's formula under its
    /// substitution gave.
    pub fn formula(&self) -> CellKey {
        self.table_cell
            .unwrap_or_else(|| self.report.anchor().cell())
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
    fn is_silent(&self) -> bool {
        true
    }

    fn observe(&mut self, _: CellKey, _: GridArea, _: u64, _: CellRun<'_>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }
}

/// The sheets of a snapshot by name, for CalcFml's binding. A name matches a sheet whose name has
/// the same UTF-16 code units, or, when both decode, the same characters once each is mapped to
/// its Unicode uppercase. Excel compares sheet names without case; which characters its
/// comparison folds is not measured: bead dc-m3u measures Excel's sheet-name rules, case-blind
/// matching included.
pub(crate) struct SnapshotCatalog<'r> {
    root: &'r ModelRoot,
    trace: Option<&'r RefCell<Vec<calc_fml::reference::NameBindingProof>>>,
}
impl<'r> SnapshotCatalog<'r> {
    pub(crate) fn new(root: &'r ModelRoot) -> Self {
        Self { root, trace: None }
    }
    pub(crate) fn traced(
        root: &'r ModelRoot,
        trace: &'r RefCell<Vec<calc_fml::reference::NameBindingProof>>,
    ) -> Self {
        Self {
            root,
            trace: Some(trace),
        }
    }
    fn name_proof(
        &self,
        name: &str,
        scope: FmlNameScope,
        parent: Option<calc_value::NameId>,
    ) -> calc_fml::reference::NameBindingProof {
        use calc_fml::reference::{BoundName, NameBindingProof, NameProbe};
        let table = self.root.definitions().name_table();
        let sheet = match scope {
            FmlNameScope::Formula(sheet) | FmlNameScope::Sheet(sheet) => sheet,
        };
        let local = calc_value::NameKey::new(NameScope::Sheet(sheet), name);
        let found = table.lookup_key(&local);
        let mut probes = vec![NameProbe {
            key: local,
            found: found.map(|node| node.id()),
        }];
        let resolved = if let Some(node) = found {
            Some(node)
        } else {
            let global = calc_value::NameKey::new(NameScope::Workbook, name);
            let node = table.lookup_key(&global);
            probes.push(NameProbe {
                key: global,
                found: node.map(|node| node.id()),
            });
            match scope {
                FmlNameScope::Formula(_) => node,
                FmlNameScope::Sheet(_) => None,
            }
        }
        .map(|node| BoundName {
            id: node.id(),
            version: node.binding_version(),
            scope: node.definition().scope,
        });
        let proof = NameBindingProof {
            resolved,
            probes,
            parent,
        };
        if let Some(trace) = self.trace {
            let mut trace = trace.borrow_mut();
            // defined_name also records lookups for callers which do not request a proof.
            // Prefer the parent-qualified witness when the binder supplies one for that query.
            let same_query = |held: &calc_fml::reference::NameBindingProof| {
                held.resolved == proof.resolved && held.probes == proof.probes
            };
            if !trace.iter().any(|held| {
                same_query(held) && (proof.parent.is_none() || held.parent == proof.parent)
            }) {
                if proof.parent.is_some() {
                    trace.retain(|held| held.parent.is_some() || !same_query(held));
                }
                trace.push(proof.clone());
            }
        }
        proof
    }
}

/// Resolve reference text with the catalogue and binder that INDIRECT uses.
/// This reads no values and performs no evaluation; computed names retain the binder's refusal.
pub fn resolve_reference_text(
    inputs: &ModelRoot,
    at: CellKey,
    text: &str,
) -> calc_fml::reference::TextReference {
    calc_fml::reference::bind_text(
        &text.encode_utf16().collect::<Vec<_>>(),
        calc_fml::reference::ReferenceNotation::A1,
        calc_fml::reference::FormulaAnchor::new(at),
        &SnapshotCatalog::new(inputs),
    )
}

impl ReferenceCatalog for SnapshotCatalog<'_> {
    fn external_function_target(&self, book: &ExternalBookRef) -> Option<String> {
        let definitions = self.root.definitions();
        let target = |held: &calc_doc_model::ExternalBook| {
            held.absolute_target
                .as_ref()
                .or(held.target.as_ref())
                .cloned()
        };
        match book {
            ExternalBookRef::Number(number) => target(definitions.external_book(*number)?),
            ExternalBookRef::File(name) => {
                let file_only = !name.contains(['/', '\\', ':']);
                let mut matches =
                    definitions
                        .external_books()
                        .map(|(_, held)| held)
                        .filter(|held| {
                            held.target.as_deref() == Some(name)
                                || held.absolute_target.as_deref() == Some(name)
                                || (file_only
                                    && held.file_name.as_ref().is_some_and(|file| {
                                        calc_value::workbook_names_equal(file, name)
                                    }))
                        });
                let first = matches.next()?;
                if matches.next().is_some() {
                    return None;
                }
                target(first)
            }
        }
    }

    fn workbook_name(&self) -> Option<&str> {
        self.root.definitions().workbook_name()
    }

    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        let wanted: Vec<u16> = name.encode_utf16().collect();
        self.root
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

    /// The snapshot's defined name `name`, matched as Excel matches names (dc-cpr), from `scope`:
    /// the formula sheet's own name first, then the workbook's; for a qualified name, the sheet's
    /// own. A name the snapshot does not hold is unknown (#NAME?). A macro or VBA name, and a
    /// qualified name that only the workbook holds, are not modelled.
    fn defined_name(&self, name: &str, scope: FmlNameScope) -> NameDefinition {
        let proof = self.name_proof(name, scope, None);
        let Some(bound) = proof.resolved else {
            return if matches!(scope, FmlNameScope::Sheet(_))
                && proof
                    .probes
                    .last()
                    .is_some_and(|probe| probe.found.is_some())
            {
                NameDefinition::NotModelled
            } else {
                NameDefinition::Unknown
            };
        };
        let defined = self
            .root
            .definitions()
            .name_table()
            .get(bound.id)
            .expect("resolved name is in the pinned table")
            .definition();
        if defined.function || defined.vb_procedure || defined.xlm {
            NameDefinition::NotModelled
        } else {
            NameDefinition::Formula(defined.formula.clone())
        }
    }

    fn name_binding(
        &self,
        name: &str,
        scope: FmlNameScope,
        parent: Option<calc_value::NameId>,
    ) -> Option<calc_fml::reference::NameBindingProof> {
        Some(self.name_proof(name, scope, parent))
    }

    /// The snapshot's table whose display name is `name`, matched as Excel matches names (L3,
    /// dc-cpr).
    fn table(&self, name: &str) -> TableLookup {
        let found = self.root.definitions().tables().iter().find(|table| {
            table.display_name == name
                || calc_value::workbook_names_equal(&table.display_name, name)
        });
        match found {
            Some(table) => TableLookup::Found(table_info(table)),
            None => TableLookup::Unknown,
        }
    }

    /// The snapshot's table whose area holds `cell` (L3).
    fn table_containing(&self, cell: CellKey) -> TableLookup {
        match self
            .root
            .definitions()
            .tables()
            .iter()
            .find(|table| table.area.contains(cell))
        {
            Some(table) => TableLookup::Found(table_info(table)),
            None => TableLookup::Unknown,
        }
    }

    /// The sheets from `first` to `last` in the snapshot's sheet order, both included (L3).
    fn sheet_span(&self, first: SheetId, last: SheetId) -> Option<Vec<SheetId>> {
        let sheets = self.root.sheets();
        let at = |id: SheetId| sheets.iter().position(|sheet| sheet.id() == id);
        let (a, b) = (at(first)?, at(last)?);
        let (from, to) = (a.min(b), a.max(b));
        Some(sheets[from..=to].iter().map(|sheet| sheet.id()).collect())
    }

    /// A call of a name no function or defined name holds is #NAME? unless the workbook is
    /// macro-enabled, whose VBA project may define the function.
    fn unknown_calls_are_names(&self) -> bool {
        !self.root.definitions().macro_enabled()
    }

    /// The sheet of another workbook the snapshot's link to it caches (dc-ylr): the ID the load
    /// issued it. The book is the link `[N]` numbers or, named by file name, the first link whose
    /// target has that name; the sheet name is matched without regard to case, as a sheet name of
    /// the workbook is.
    fn external_sheet(&self, book: &ExternalBookRef, sheet: &str) -> Option<SheetId> {
        let definitions = self.root.definitions();
        let number = match book {
            ExternalBookRef::Number(number) => *number,
            ExternalBookRef::File(name) => definitions.external_book_named(name)?,
        };
        let book = definitions.external_book(number)?;
        let wanted: Vec<char> = sheet.chars().flat_map(char::to_uppercase).collect();
        book.sheets
            .iter()
            .find(|held| {
                held.name
                    .chars()
                    .flat_map(char::to_uppercase)
                    .eq(wanted.iter().copied())
            })?
            .id
    }
}

/// A document table as CalcFml's binding reads it.
fn table_info(table: &calc_doc_model::Table) -> TableInfo {
    TableInfo {
        name: table.display_name.clone(),
        sheet: table.sheet,
        area: table.area,
        header_rows: table.header_rows,
        totals_rows: table.totals_rows,
        columns: table.columns.clone(),
    }
}

/// An admission-time cache pin, including the ownership and values of calculated arrays.
/// Imported images join it only when every child cache and the owning anchor are present.
#[derive(Debug)]
pub(crate) struct BoundarySnapshot {
    view: crate::contract::ReadViewId,
    entries: BTreeMap<CellKey, BoundaryValue>,
    spills: SpillIndex,
    empty: CalcValue,
    refused: BTreeSet<CellKey>,
}
#[derive(Debug)]
struct BoundaryValue {
    node: NodeId,
    value: CalcValue,
    dirty: bool,
    imported: bool,
    external: bool,
    source: Option<std::sync::Arc<calc_doc_model::SourcePart>>,
    extent: Option<GridArea>,
}
impl BoundarySnapshot {
    pub(crate) fn view(&self) -> crate::contract::ReadViewId {
        self.view
    }
    pub(crate) fn capture(
        state: &EngineState,
        inputs: &ModelRoot,
        scope: &crate::CalcScope,
    ) -> Self {
        let mut entries = BTreeMap::new();
        let mut spills = SpillIndex::default();
        for node in state.nodes() {
            let at = node.anchor();
            if scope.contains(at) || !matches!(inputs.cell(at), Some(CellInput::Formula(_))) {
                continue;
            }
            let Some(result) = state.permitted_cache(at) else {
                continue;
            };
            let (value, imported, external) = match result {
                FormulaResult::Calculated { value, calculation } => (
                    value.clone(),
                    result
                        .stamp()
                        .is_some_and(|stamp| stamp.reads.uses_imported_cache),
                    calculation.reads_external_links(),
                ),
                FormulaResult::FileCached(cache)
                    if inputs.definitions().array(at).is_none()
                        || state.cached_arrays.contains_key(&node.id()) =>
                {
                    (cache.value().to_value(), true, false)
                }
                FormulaResult::FileCached(_) => continue,
                FormulaResult::NotModeled { .. } => continue,
            };
            let requirement = state.contract.requirement(node.id());
            let held = state
                .spills
                .get(at)
                .or_else(|| state.cached_arrays.get(&node.id()));
            let extent = held.map(Spill::area);
            if let Some(spill) = held {
                spills.insert(spill.clone());
            }
            entries.insert(
                at,
                BoundaryValue {
                    source: match result {
                        FormulaResult::FileCached(cache) => Some(cache.source().clone()),
                        _ => None,
                    },
                    node: node.id(),
                    extent,
                    value,
                    imported,
                    external,
                    dirty: state.range_redemand.contains(&node.id())
                        || state.pending.contains(&node.id())
                        || requirement.required != requirement.satisfied,
                },
            );
        }
        Self {
            view: state.result_view(),
            entries,
            spills,
            empty: CalcValue::empty(),
            refused: BTreeSet::new(),
        }
    }
    pub(crate) fn span(&self, region: GridArea) -> Option<crate::contract::BoundaryReadSpan> {
        let mut span = crate::contract::BoundaryReadSpan {
            region,
            view: self.view,
            values: 0,
            dirty: 0,
            imported: 0,
            imported_sources: Vec::new(),
        };
        let anchors: BTreeSet<_> = self
            .entries
            .range(region.first()..=region.last())
            .filter(|(at, _)| region.contains(**at))
            .map(|(at, _)| *at)
            .chain(self.spills.meeting(region).into_iter().map(Spill::anchor))
            .collect();
        for at in anchors {
            let value = &self.entries[&at];
            let cells = value
                .extent
                .and_then(|area| area.intersection(region))
                .map_or_else(|| u64::from(region.contains(at)), |area| area.cell_count());
            if cells != 0 {
                if let Some(source) = &value.source {
                    if !span.imported_sources.contains(source) {
                        span.imported_sources.push(source.clone());
                    }
                }
                span.values += cells;
                span.dirty += cells * u64::from(value.dirty);
                span.imported += cells * u64::from(value.imported);
            }
        }
        (span.values != 0).then_some(span)
    }
}

/// What one recalculation knows of the workbook's formula results while it runs: the results and
/// spills it has staged, the nodes it calculates and those it has evaluated, over the published
/// generation.
#[derive(Clone, Copy)]
pub(crate) struct RecalcView<'a> {
    pub(crate) cached_observation: bool,
    pub(crate) caller: Option<NodeId>,
    pub(crate) scope: Option<&'a crate::contract::CalcScope>,
    pub(crate) boundary: Option<&'a Cell<Option<calc_fml::ReadDenial>>>,
    pub(crate) boundary_caches: Option<&'a BoundarySnapshot>,
    pub(crate) boundary_used: Option<&'a Cell<bool>>,
    pub(crate) workbook_facts: Option<&'a std::sync::Arc<dyn calc_fml::WorkbookFacts>>,
    pub(crate) inputs: &'a ModelRoot,
    pub(crate) state: &'a EngineState,
    /// Which nodes this recalculation calculates and has evaluated, and what it staged.
    pub(crate) progress: &'a Progress,
    /// The spills of the anchors this recalculation evaluated whose results fill cells.
    pub(crate) spills: &'a SpillIndex,
    /// With iteration on, the value each formula on a cycle holds as the recalculation stands
    /// (L3, dc-m3u): one not yet evaluated in this pass reads as it, and no read of it waits.
    pub(crate) iterates: Option<&'a BTreeMap<NodeId, Option<CalcValue>>>,
    /// With iteration off, the formulas a circular reference left uncalculated, and the flag a
    /// read of one sets (dc-10y): such a formula's staged value is what its cell holds, not a
    /// value this recalculation calculated, so a read of it gives no value and marks the reading
    /// formula as reading a circular reference.
    pub(crate) kept: Option<(&'a BTreeSet<NodeId>, &'a Cell<bool>)>,
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
    pub(crate) footprints: BTreeMap<NodeId, GridArea>,
}

const SCHEDULED: u8 = 1;
const EVALUATED: u8 = 2;

impl Progress {
    pub(crate) fn new(slots: usize) -> Self {
        Progress {
            status: vec![0; slots],
            staged: Vec::new(),
            scheduled: 0,
            footprints: BTreeMap::new(),
        }
    }

    fn slot(node: NodeId) -> usize {
        node.slot() as usize
    }

    /// Makes room for the staged results of the nodes of slots below `slots` at once (dc-dxm), so
    /// that staging does not grow the storage, moving what it staged, as it goes.
    pub(crate) fn reserve_slots(&mut self, slots: usize) {
        if self.staged.len() < slots {
            self.staged.resize_with(slots, || None);
        }
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

    /// The node's status: 0 when this recalculation does not calculate it, else scheduled or
    /// evaluated.
    fn status(&self, node: NodeId) -> u8 {
        self.status.get(Self::slot(node)).copied().unwrap_or(0)
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

    pub(crate) fn staged_results(&self) -> impl Iterator<Item = (NodeId, &FormulaResult)> {
        self.staged
            .iter()
            .flatten()
            .map(|(node, result)| (*node, result))
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
        match self.state.node(node) {
            Some(record) => self.standing_at(node, record.anchor()),
            None => self.standing_at_unknown(node),
        }
    }

    /// [`RecalcView::standing`] of `node`, whose anchor is `at` (dc-dxm: a read of a formula cell
    /// knows the cell already).
    pub(crate) fn standing_at(&self, node: NodeId, at: CellKey) -> NodeStanding<'a> {
        // The node's status read once (dc-dxm), then its staged result only when it has one.
        match self.progress.status(node) {
            0 => NodeStanding::Published(self.state.result_of(node, at)),
            EVALUATED => match self.progress.staged(node) {
                Some(result) => NodeStanding::Staged(result),
                None => NodeStanding::Pending,
            },
            _ => NodeStanding::Pending,
        }
    }

    /// The standing of a node that is not live: its staged result, or no published one.
    fn standing_at_unknown(&self, node: NodeId) -> NodeStanding<'a> {
        if self.progress.is_scheduled(node) {
            match self.progress.staged(node) {
                Some(result) if self.progress.is_evaluated(node) => NodeStanding::Staged(result),
                _ => NodeStanding::Pending,
            }
        } else {
            NodeStanding::Published(None)
        }
    }

    /// The spill whose filled cells hold `at` as the recalculation stands, with its anchor's node:
    /// a staged spill, or a published spill of an anchor the recalculation does not calculate.
    /// `Err` with the anchor's node and cell when the cell may lie in the spill of an anchor the
    /// recalculation calculates and has not evaluated yet. Both `Ok(None)` when no spill holds
    /// the cell.
    pub(crate) fn spill_at(&self, at: CellKey) -> Result<Option<&'a Spill>, (NodeId, CellKey)> {
        if self.cached_observation {
            return Ok(self.boundary_caches.and_then(|pin| pin.spills.covering(at)));
        }
        if let Some(spill) = self.spills.covering(at) {
            return Ok(Some(spill));
        }
        if let Some(spill) = self.boundary_caches.and_then(|pin| pin.spills.covering(at)) {
            if let Some(used) = self.boundary_used {
                used.set(true);
            }
            return Ok(Some(spill));
        }
        if let Some(spill) = self.state.spills.covering(at) {
            let anchor = spill.anchor();
            if self.scope.is_some_and(|scope| !scope.contains(anchor)) {
                if let Some(spill) = self.boundary_caches.and_then(|pin| pin.spills.covering(at)) {
                    if let Some(used) = self.boundary_used {
                        used.set(true);
                    }
                    return Ok(Some(spill));
                }
                if let Some(boundary) = self.boundary {
                    boundary.set(Some(scope_denial()));
                }
                if let Some(node) = self.state.node_at(anchor) {
                    return Err((node, anchor));
                }
            }
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

/// Whether two areas share a cell.
fn areas_meet(a: GridArea, b: GridArea) -> bool {
    a.sheet() == b.sheet()
        && a.first().row() <= b.last().row()
        && b.first().row() <= a.last().row()
        && a.first().column() <= b.last().column()
        && b.first().column() <= a.last().column()
}

/// Why [`WorkbookReader::formula_lookup`] gives no value for a formula cell, where reading it has
/// an effect: the evaluation waits for the node, or (iteration off) it reads a formula a circular
/// reference left uncalculated.
enum Unread {
    Waits(NodeId),
    Kept,
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
    /// The storage a read of several cells gathers their values in (dc-dxm: handed from one
    /// evaluation to the next by [`WorkbookReader::with_batch`] and [`WorkbookReader::into_batch`]).
    batch: Vec<CalcValue>,
    /// A formula result read was calculated from values another workbook's link caches
    /// ([`Calculation::reads_external_links`], dc-ylr).
    external: Cell<bool>,
    denial: Cell<Option<calc_fml::ReadDenial>>,
}

impl<'a> WorkbookReader<'a> {
    pub(crate) fn new(
        view: RecalcView<'a>,
        uncomputed: &'a Cell<Option<(CellKey, NodeId)>>,
    ) -> Self {
        WorkbookReader {
            view,
            uncomputed,
            batch: Vec::new(),
            external: Cell::new(false),
            denial: Cell::new(None),
        }
    }

    /// Whether a formula result this reader handed out was calculated from values another
    /// workbook's link caches (dc-ylr).
    pub(crate) fn read_external_links(&self) -> bool {
        self.external.get()
    }

    /// The reader, gathering values in `batch`'s storage.
    pub(crate) fn with_batch(mut self, batch: Vec<CalcValue>) -> Self {
        self.batch = batch;
        self
    }

    /// The storage the reader gathered values in, empty.
    pub(crate) fn into_batch(mut self) -> Vec<CalcValue> {
        self.batch.clear();
        self.batch
    }

    /// The runs of `occupied`, the occupied cells of an area of `total` cells in row-major
    /// order, with `overlays` for the cells between them, values gathered in `batch`.
    fn read_occupied(
        &mut self,
        occupied: impl Iterator<Item = (CellKey, &'a CellInput)>,
        overlays: &BTreeMap<u64, Overlay<'a>>,
        index_of: impl Fn(CellKey) -> u64,
        total: u64,
        batch: &mut Vec<CalcValue>,
        sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
    ) {
        const BATCH: usize = 256;
        let flush = |batch: &mut Vec<CalcValue>,
                     sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>| {
            if batch.is_empty() {
                return ControlFlow::Continue(());
            }
            let flow = sink(CellRun::Values(batch));
            batch.clear();
            flow
        };
        let mut covered = 0u64;
        for (at, input) in occupied {
            let index = index_of(at);
            if index > covered
                && (flush(batch, sink).is_break() || gap(covered, index, overlays, sink).is_break())
            {
                return;
            }
            covered = index + 1;
            let flow = match literal_value(input) {
                Some(value) => {
                    batch.push(value);
                    ControlFlow::Continue(())
                }
                None => match self.formula_lookup(at) {
                    Ok(Some(value)) => {
                        batch.push(value.clone());
                        ControlFlow::Continue(())
                    }
                    Ok(None) => match flush(batch, sink) {
                        ControlFlow::Continue(()) => sink(CellRun::Missing(1)),
                        stop => stop,
                    },
                    Err(unread) => match flush(batch, sink) {
                        ControlFlow::Continue(()) => {
                            self.unread(at, unread);
                            sink(CellRun::Missing(1))
                        }
                        stop => stop,
                    },
                },
            };
            if flow.is_break() || (batch.len() >= BATCH && flush(batch, sink).is_break()) {
                return;
            }
        }
        if flush(batch, sink).is_break() {
            return;
        }
        if total > covered {
            let _ = gap(covered, total, overlays, sink);
        }
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
        match self.formula_lookup(at) {
            Ok(value) => value,
            Err(unread) => {
                self.unread(at, unread);
                None
            }
        }
    }

    /// [`WorkbookReader::formula_value`] of the formula cell `at`, whose node is `node`.
    fn node_value(&mut self, node: NodeId, at: CellKey) -> Option<&'a CalcValue> {
        match self.node_lookup(node, at) {
            Ok(value) => value,
            Err(unread) => {
                self.unread(at, unread);
                None
            }
        }
    }

    /// The effect of reading a cell [`WorkbookReader::formula_lookup`] gives no value for.
    fn unread(&mut self, at: CellKey, unread: Unread) {
        match unread {
            Unread::Waits(node) => self.wait(at, node),
            Unread::Kept => {
                if let Some((_, read)) = self.view.kept {
                    read.set(true);
                }
            }
        }
    }

    /// [`WorkbookReader::formula_value`] without its effects: `Err` where reading the cell has
    /// one ([`WorkbookReader::unread`]).
    fn formula_lookup(&self, at: CellKey) -> Result<Option<&'a CalcValue>, Unread> {
        let Some(node) = self.view.state.node_at(at) else {
            return Ok(None);
        };
        self.node_lookup(node, at)
    }

    /// [`WorkbookReader::formula_lookup`] of the formula cell `at`, whose node is `node`.
    fn node_lookup(&self, node: NodeId, at: CellKey) -> Result<Option<&'a CalcValue>, Unread> {
        if self.view.cached_observation {
            let Some(pin) = self.view.boundary_caches else {
                return Ok(None);
            };
            if pin.refused.contains(&at)
                || (!pin.entries.contains_key(&at)
                    && self.view.inputs.definitions().array(at).is_some())
            {
                self.denial.set(Some(calc_fml::ReadDenial {
                    construct: "a cached VBA observation without a permitted value",
                    owner: "dc-hsa5.4",
                }));
                return Ok(None);
            }
            if pin.entries.get(&at).is_some_and(|value| value.external) {
                self.external.set(true);
            }
            return Ok(pin.entries.get(&at).map(|value| &value.value).or_else(|| {
                self.view
                    .inputs
                    .definitions()
                    .array(at)
                    .is_none()
                    .then_some(&pin.empty)
            }));
        }

        if self.view.scope.is_some_and(|scope| !scope.contains(at)) {
            if let Some(value) = self
                .view
                .boundary_caches
                .and_then(|pin| pin.entries.get(&at))
                .filter(|value| value.node == node)
            {
                if let Some(used) = self.view.boundary_used {
                    used.set(true);
                }
                if value.external {
                    self.external.set(true);
                }
                return Ok(Some(&value.value));
            }
            if let Some(boundary) = self.view.boundary {
                boundary.set(Some(scope_denial()));
            }
            return Ok(None);
        }
        let result = match self.view.standing_at(node, at) {
            NodeStanding::Pending => {
                if let Some(value) = self.view.iterates.and_then(|iterates| iterates.get(&node)) {
                    return Ok(value.as_ref());
                }
                return Err(Unread::Waits(node));
            }
            NodeStanding::Staged(result) => {
                if let Some((kept, _)) = self.view.kept
                    && kept.contains(&node)
                {
                    return Err(Unread::Kept);
                }
                result
            }
            NodeStanding::Published(None) => return Ok(None),
            NodeStanding::Published(Some(result)) => result,
        };
        Ok(result.calculated().map(|(value, calculation)| {
            if calculation.reads_external_links() {
                self.external.set(true);
            }
            value
        }))
    }

    /// What the cell `at`, which holds no input, reads as.
    fn overlay(&mut self, at: CellKey) -> Overlay<'a> {
        let view = self.view;
        let definitions = view.inputs.definitions();
        // L3 (dc-m3u): a data table's cell reads as its node's result.
        if definitions.data_table_containing(at).is_some() && view.state.node_at(at).is_some() {
            return match self.formula_value(at) {
                Some(value) => Overlay::Value(value),
                None => Overlay::Missing,
            };
        }
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
            && view.boundary_caches.is_none_or(|pin| pin.spills.is_empty())
            && definitions.arrays_meeting(area).next().is_none()
            && definitions.unloaded_in(area).next().is_none()
            && !definitions
                .data_tables()
                .iter()
                .any(|table| areas_meet(table.area(), area))
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
        if let Some(pin) = view.boundary_caches {
            for spill in pin.spills.meeting(area) {
                add(spill.area());
            }
        }
        for array in definitions.arrays_meeting(area) {
            add(array.area());
        }
        candidates.extend(definitions.unloaded_in(area).map(|(at, _)| at));
        for table in definitions.data_tables() {
            let held = table.area();
            if !areas_meet(held, area) {
                continue;
            }
            let (a, b) = (held.first(), held.last());
            for row in a.row().max(first.row())..=b.row().min(area.last().row()) {
                for column in a.column().max(first.column())..=b.column().min(area.last().column())
                {
                    candidates.insert(
                        CellKey::new(area.sheet(), row, column).expect("a cell of the area"),
                    );
                }
            }
        }
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

/// One cell that holds an input or a formula node, as [`WorkbookReader::single`] reads it.
enum Single<'a> {
    Formula(&'a CalcValue),
    Literal(CalcValue),
    Missing,
}

impl<'a> WorkbookReader<'a> {
    /// One cell that holds an input or has a formula node, read straight from the engine's index
    /// and its page without the walk of an area (most reads; dc-10d9, dc-dxm), as the walk would
    /// give it, the cell of a what-if table's node included; `None` for an empty cell, which an
    /// overlay may fill and the walk reads.
    fn single(&mut self, at: CellKey) -> Option<Single<'a>> {
        if let Some(node) = self.view.state.node_at(at) {
            return Some(match self.node_value(node, at) {
                Some(value) => Single::Formula(value),
                None => Single::Missing,
            });
        }
        let input = self.view.inputs.cell(at)?;
        Some(match literal_value(input) {
            Some(value) => Single::Literal(value),
            None => match self.formula_value(at) {
                Some(value) => Single::Formula(value),
                None => Single::Missing,
            },
        })
    }
}

impl<'a> WorkbookReader<'a> {
    /// The sheet of another workbook that `sheet` names, when it names one (dc-ylr).
    fn external_sheet(&self, sheet: SheetId) -> Option<&'a ExternalSheet> {
        let view = self.view;
        view.inputs.definitions().external_sheet(sheet)
    }

    /// Reports the cells of `area` on the sheet of another workbook `held` (dc-ylr), as the
    /// link's cache gives them: a cached cell's value; a cell it does not hold is blank where the
    /// cache holds the sheet's data, #REF! where that data is marked as not refreshed, and
    /// unsupplied where the cache holds no data for the sheet. The read marks the result as
    /// reading link caches.
    fn read_external(
        &mut self,
        held: &ExternalSheet,
        area: GridArea,
        sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
    ) {
        self.external.set(true);
        let (first, last) = (area.first(), area.last());
        let width = u64::from(area.column_count());
        let ref_error = [CalcValue::error(WorksheetErrorCode::Ref)];
        let gap = |count: u64, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>| {
            if count == 0 {
                return ControlFlow::Continue(());
            }
            if !held.cached {
                return sink(CellRun::Missing(count));
            }
            if !held.refresh_error {
                return sink(CellRun::Blank(count));
            }
            for _ in 0..count {
                sink(CellRun::Values(&ref_error))?;
            }
            ControlFlow::Continue(())
        };
        let mut next = 0u64;
        for (&(row, column), cached) in held
            .cells
            .range((first.row(), first.column())..=(last.row(), last.column()))
        {
            if column < first.column() || column > last.column() {
                continue;
            }
            let index = u64::from(row - first.row()) * width + u64::from(column - first.column());
            if gap(index - next, sink).is_break() {
                return;
            }
            if sink(CellRun::Values(&[cached.to_value()])).is_break() {
                return;
            }
            next = index + 1;
        }
        let _ = gap(area.cell_count() - next, sink);
    }
}

fn scope_denial() -> calc_fml::ReadDenial {
    calc_fml::ReadDenial {
        construct: "an out-of-scope formula without a complete permitted cache (dc-hsa5.4)",
        owner: "dc-hsa5.4",
    }
}

impl ReferenceReader for WorkbookReader<'_> {
    fn read_denial(&self) -> Option<calc_fml::ReadDenial> {
        self.denial
            .get()
            .or_else(|| self.view.boundary.and_then(Cell::get))
    }

    fn read_fact(&mut self, _fact: calc_fml::FactRead) {}

    fn read_cell(
        &mut self,
        request: calc_fml::ReadRequest,
        at: CellKey,
    ) -> Result<CellRead, ReadContractError> {
        if self.external_sheet(at.sheet()).is_some() {
            return calc_fml::reference::read_cell_by_area(self, request, at);
        }
        Ok(match self.single(at) {
            Some(Single::Formula(value)) => CellRead::Value(value.clone()),
            Some(Single::Literal(value)) => CellRead::Value(value),
            Some(Single::Missing) => CellRead::Missing,
            None => return calc_fml::reference::read_cell_by_area(self, request, at),
        })
    }

    fn read_area(
        &mut self,
        _request: calc_fml::ReadRequest,
        area: GridArea,
        sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
    ) {
        if let Some(held) = self.external_sheet(area.sheet()) {
            return self.read_external(held, area, sink);
        }
        let total = area.cell_count();
        let inputs = self.view.inputs;
        // One cell that holds an input (most reads; dc-10d9): its value, read straight from its
        // page, as the walk below would give it without building the walk. An empty cell, which
        // an overlay may fill, takes the walk.
        // A formula cell (dc-dxm: found in the engine's own index first, without the page walk):
        // its value as `formula_value` gives it, which is what the walk would give, the cell of a
        // what-if table's node included.
        if total == 1
            && let Some(single) = self.single(area.first())
        {
            let _ = match single {
                Single::Formula(value) => sink(CellRun::Values(std::slice::from_ref(value))),
                Single::Literal(value) => sink(CellRun::Values(&[value])),
                Single::Missing => sink(CellRun::Missing(1)),
            };
            return;
        }
        let pending: Vec<(NodeId, CellKey, Option<calc_fml::ReadDenial>)> =
            if self.view.iterates.is_some() {
                Vec::new()
            } else {
                self.view
                    .state
                    .bindings
                    .iter()
                    .filter_map(|(&node, binding)| {
                        if Some(node) == self.view.caller
                            || !matches!(
                                binding,
                                crate::state::Binding::Placed(_, crate::state::ArrayRole::Spill)
                            )
                        {
                            return None;
                        }
                        let anchor = self.view.state.node(node)?.anchor();
                        let outside = self.view.scope.is_some_and(|scope| !scope.contains(anchor));
                        let result = if self.view.progress.is_evaluated(node) {
                            self.view.progress.staged(node)
                        } else if self.view.progress.is_scheduled(node) {
                            None
                        } else {
                            self.view.state.results.of(node)
                        };
                        let unknown = matches!(result, Some(FormulaResult::NotModeled { .. }));
                        // Pure in-scope candidates may consume a provisional blank. Their absence
                        // proof remains invalid until all producers finish, and publication repairs
                        // the candidate if an actual footprint reaches it. This avoids inventing a
                        // circular dependency from a merely possible southeast footprint.
                        // Excluded producers use the admission-time cached absence. The
                        // certificate records that qualification and their unmet requirements.
                        let unresolved = unknown && !outside;
                        (unresolved
                            && anchor.sheet() == area.sheet()
                            && anchor.row() <= area.last().row()
                            && anchor.column() <= area.last().column())
                        .then_some((
                            node,
                            anchor,
                            if outside {
                                Some(scope_denial())
                            } else if unknown {
                                Some(calc_fml::ReadDenial {
                                    construct: "a blank beside an unmodelled spill producer",
                                    owner: "dc-hsa5",
                                })
                            } else {
                                None
                            },
                        ))
                    })
                    .collect()
            };
        let uncomputed = self.uncomputed;
        let boundary = self.view.boundary;
        let mut offset = 0;
        let mut guarded = |run: CellRun<'_>| {
            let len = run.len();
            let waiting = if matches!(run, CellRun::Blank(_)) {
                pending.iter().find_map(|&(node, anchor, outside)| {
                    crate::contract::potential_blank(
                        inputs,
                        anchor,
                        area,
                        crate::contract::BlankRun { offset, len },
                    )
                    .map(|at| (node, at, outside))
                })
            } else {
                None
            };
            offset += len;
            match waiting {
                Some((_node, _at, Some(denial))) => {
                    if let Some(boundary) = boundary {
                        boundary.set(Some(denial));
                    }
                    sink(CellRun::Missing(len))
                }
                Some((node, at, None)) => {
                    if uncomputed.get().is_none() {
                        uncomputed.set(Some((at, node)));
                    }
                    sink(CellRun::Missing(len))
                }
                None => sink(run),
            }
        };
        let sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()> = &mut guarded;
        let Some(occupied) = inputs.cells(area) else {
            let _ = sink(CellRun::Missing(total));
            return;
        };
        let (first, width) = (area.first(), u64::from(area.column_count()));
        let index_of = |at: CellKey| {
            u64::from(at.row() - first.row()) * width + u64::from(at.column() - first.column())
        };
        let overlays = self.overlays(area);
        // Consecutive cells' values go to the sink as one run (dc-dxm), up to BATCH of them. A
        // cell this recalculation has yet to calculate ends the run before its wait is noted, so
        // a sink that stops reading earlier notes no wait for it, as with one run per cell.
        let mut batch = std::mem::take(&mut self.batch);
        batch.clear();
        self.read_occupied(occupied, &overlays, index_of, total, &mut batch, sink);
        batch.clear();
        self.batch = batch;
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
        if view.cached_observation {
            return view
                .boundary_caches
                .and_then(|pin| pin.spills.get(anchor))
                .map_or(SpillExtent::None, |spill| SpillExtent::Area(spill.area()));
        }
        let Some(node) = view.state.node_at(anchor) else {
            return SpillExtent::None;
        };
        if view.scope.is_some_and(|scope| !scope.contains(anchor)) {
            if let Some(pin) = view.boundary_caches
                && pin
                    .entries
                    .get(&anchor)
                    .is_some_and(|value| value.node == node)
            {
                if let Some(used) = view.boundary_used {
                    used.set(true);
                }
                return match pin.spills.get(anchor) {
                    Some(spill) if spill.is_fixed() => SpillExtent::NotModelled,
                    Some(spill) => SpillExtent::Area(spill.area()),
                    None => SpillExtent::Area(GridArea::cell(anchor)),
                };
            }
            if let Some(boundary) = view.boundary {
                boundary.set(Some(scope_denial()));
            }
            return SpillExtent::NotModelled;
        }
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
}

/// The workbook's facts an evaluation may ask about (calc-func's HostFacts): which cells hold
/// formulas and their text, and the sheets. The formula being evaluated is the caller's to say.
impl HostFacts for WorkbookReader<'_> {
    fn caller(&mut self) -> Option<CellKey> {
        None
    }
    fn is_external_sheet(&mut self, sheet: SheetId) -> bool {
        self.external_sheet(sheet).is_some()
    }

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

    fn workbook_facts(&mut self) -> Option<std::sync::Arc<dyn calc_fml::WorkbookFacts>> {
        self.view.workbook_facts.cloned()
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

/// An immutable observation view. A result retains its historical model and caches;
/// validation checks this pin rather than demanding the observed cells again.
#[derive(Clone)]
pub struct CachedReadLease(std::sync::Arc<CachedReadView>);
#[derive(Debug)]
struct CachedReadView {
    id: u64,
    model: std::sync::Arc<ModelRoot>,
    cache: BoundarySnapshot,
    inputs: u64,
    progress: Progress,
    spills: SpillIndex,
    provisional: BTreeSet<CellKey>,
}
impl std::fmt::Debug for CachedReadLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedReadLease")
            .field("id", &self.0.id)
            .field("model", &self.0.model.revision())
            .field("results", &self.0.cache.view)
            .finish()
    }
}
impl PartialEq for CachedReadLease {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Eq for CachedReadLease {}
impl std::hash::Hash for CachedReadLease {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.0.id, state)
    }
}
impl CachedReadLease {
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn frontier(&self) -> crate::contract::ObservationFrontier {
        crate::contract::ObservationFrontier {
            model: self.0.model.revision(),
            results: self.0.cache.view,
            inputs: self.0.inputs,
        }
    }
    pub(crate) fn capture(
        state: &EngineState,
        inputs: &ModelRoot,
        progress: &Progress,
        spills: &SpillIndex,
        inputs_cut: u64,
    ) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .expect("observation view identity exhausted");
        let mut cache =
            BoundarySnapshot::capture(state, inputs, &crate::CalcScope::Range(Vec::new()));
        let mut refused: BTreeSet<_> = state
            .nodes()
            .filter_map(|node| {
                let at = node.anchor();
                let unsupported = !matches!(inputs.cell(at), Some(CellInput::Formula(_)))
                    || (!state.entered.contains(&node.id())
                        && matches!(state.result(at), Some(FormulaResult::NotModeled { .. }))
                        && state.permitted_cache(at).is_none());
                unsupported.then_some(at)
            })
            .collect();
        for (node, result) in progress.staged_results() {
            let Some(record) = state.node(node) else {
                continue;
            };
            let at = record.anchor();
            cache.spills.remove(at);
            match result {
                FormulaResult::Calculated { value, calculation }
                    if matches!(inputs.cell(at), Some(CellInput::Formula(_))) =>
                {
                    let spill = spills.get(at);
                    if let Some(spill) = spill {
                        cache.spills.insert(spill.clone());
                    }
                    cache.entries.insert(
                        at,
                        BoundaryValue {
                            node,
                            value: value.clone(),
                            dirty: false,
                            imported: result
                                .stamp()
                                .is_some_and(|stamp| stamp.reads.uses_imported_cache),
                            external: calculation.reads_external_links(),
                            source: None,
                            extent: spill.map(Spill::area),
                        },
                    );
                }
                FormulaResult::NotModeled { .. } => {
                    cache.entries.remove(&at);
                    refused.insert(at);
                }
                FormulaResult::FileCached(_) => {}
                FormulaResult::Calculated { .. } => {
                    cache.entries.remove(&at);
                    refused.insert(at);
                }
            }
        }
        cache.refused = refused.clone();
        let provisional = state
            .nodes()
            .map(|node| node.anchor())
            .filter(|at| !cache.entries.contains_key(at) && !refused.contains(at))
            .collect();
        Self(std::sync::Arc::new(CachedReadView {
            id,
            model: inputs.pin(),
            cache,
            inputs: inputs_cut,
            progress: Progress::new(0),
            spills: SpillIndex::default(),
            provisional,
        }))
    }
    pub(crate) fn view<'a>(
        &'a self,
        state: &'a EngineState,
        facts: Option<&'a std::sync::Arc<dyn calc_fml::WorkbookFacts>>,
    ) -> RecalcView<'a> {
        RecalcView {
            cached_observation: true,
            caller: None,
            scope: None,
            boundary: None,
            boundary_caches: Some(&self.0.cache),
            boundary_used: None,
            workbook_facts: facts,
            inputs: &self.0.model,
            state,
            progress: &self.0.progress,
            spills: &self.0.spills,
            iterates: None,
            kept: None,
        }
    }
    pub(crate) fn observation(
        &self,
        request: calc_fml::ReadRequest,
        region: GridArea,
    ) -> crate::contract::CachedObservation {
        crate::contract::CachedObservation {
            view: self.clone(),
            request,
            region,
            external_cache: self
                .0
                .model
                .definitions()
                .external_sheet(region.sheet())
                .is_some()
                || self.0.cache.entries.iter().any(|(at, value)| {
                    value.external
                        && (region.contains(*at)
                            || value
                                .extent
                                .is_some_and(|extent| extent.intersection(region).is_some()))
                }),
            lineage: self.0.cache.span(region),
            provisional: self
                .0
                .provisional
                .range(region.first()..=region.last())
                .any(|at| region.contains(*at)),
        }
    }
}

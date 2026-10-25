//! What-if data tables (`TABLE()`, L3 dc-m3u): each cell of a table is a node of the graph whose
//! result is its formula cell's value with the table's input cells set to the values its row and
//! column give ([`calc_doc_model::DataTable::what_if`]).
//!
//! The cell is calculated by evaluating the formula cell again under those values: every formula
//! that reads an input cell, directly or through other formulas, is evaluated again too, each
//! once per table cell, through a reader that gives the input cells their substituted values and
//! those formulas their values under them; every other cell reads as the recalculation stands.
//! Nothing of it is staged or published but the table cell's own result.
//!
//! A workbook that calculates automatically except tables (`calcMode="autoNoTable"`, Excel's
//! "Automatic except for data tables") leaves the table cells, and the formulas that read only
//! through them, uncalculated until a recalculation is asked for: Excel 16.0 (2026-10-23,
//! `Application.Calculation = xlCalculationSemiautomatic`) left `TABLE(,B1)` at 15 and
//! `=SUM(B4:B6)` at 75 after an input changed, and gave 16 and 78 after `Calculate`.

use crate::read::{ObservedEvaluation, ReadObserver, RecalcView, SnapshotCatalog, WorkbookReader};
use crate::recalc::dirty_closure;
use crate::result::{Calculation, NotModeled};
use crate::state::{ArrayRole, Binding, EngineState};
use calc_doc_model::{CellInput, DataTable, ModelRoot};
use calc_fml::reference::{CellRun, ReferenceReader, SpillExtent};
use calc_fml::{
    EvalFault, EvalFrame, EvalScratch, EvaluationObserver, EvaluationReport, EvaluationServices,
    evaluate,
};
use calc_value::{CalcValue, CellKey, GridArea, NodeId, NumericMode};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

/// A refusal of a data table cell, owned by L3.
pub(crate) fn refusal(construct: &'static str) -> NotModeled {
    NotModeled::Evaluation(EvalFault::NotModelled {
        construct,
        owner: "dc-10y",
    })
}

/// What evaluating one data table cell gives.
pub(crate) enum TableOutcome {
    Value(CalcValue),
    Refused(NotModeled),
    /// A cell the substitution reads has yet to be calculated by this recalculation.
    Waits {
        on: NodeId,
        cell: CellKey,
    },
}

/// The value of the table cell `at` of `table` as the recalculation stands (`view`).
pub(crate) fn evaluate_cell(
    state: &EngineState,
    inputs: &ModelRoot,
    view: RecalcView<'_>,
    mode: &NumericMode,
    table: &DataTable,
    at: CellKey,
    observe: Option<(&Calculation, &mut dyn ReadObserver)>,
) -> TableOutcome {
    let Some((formula, pairs)) = table.what_if(at) else {
        return TableOutcome::Refused(refusal("a data table cell outside its table"));
    };
    let uncomputed = Cell::new(None);
    // The values the input cells are set to, read as the recalculation stands.
    let mut overrides = BTreeMap::new();
    for (input, from) in pairs {
        let mut value = None;
        WorkbookReader::new(view, &uncomputed).read_area(GridArea::cell(from), &mut |run| {
            value = Some(match run {
                CellRun::Values(values) => Some(values[0].clone()),
                CellRun::Blank(_) => Some(CalcValue::empty()),
                CellRun::Missing(_) => None,
            });
            ControlFlow::Continue(())
        });
        if let Some((cell, on)) = uncomputed.get() {
            return TableOutcome::Waits { on, cell };
        }
        match value.flatten() {
            Some(value) => {
                overrides.insert(input, value);
            }
            None => {
                return TableOutcome::Refused(refusal(
                    "a data table whose row or column value is not available",
                ));
            }
        }
    }
    // The formulas that read an input cell, directly or through others.
    let mut roots = BTreeSet::new();
    for input in overrides.keys() {
        state.index.dependents_of_cell(inputs, *input, &mut roots);
    }
    let affected: BTreeSet<NodeId> = dirty_closure(state, inputs, roots, &|_| Vec::new())
        .members()
        .iter()
        .copied()
        .collect();
    let what_if = WhatIf {
        state,
        inputs,
        view,
        mode,
        overrides,
        affected,
        memo: RefCell::new(BTreeMap::new()),
        refused: RefCell::new(None),
        uncomputed: &uncomputed,
        observe: RefCell::new(observe.map(|(calculation, observer)| TableObserver {
            calculation,
            observer,
            formula: state.node_at(formula),
            cell: at,
        })),
    };
    let outcome = match what_if.overrides.get(&formula) {
        Some(value) => Ok(value.clone()),
        None => match state.node_at(formula) {
            Some(node) if what_if.affected.contains(&node) => what_if.value(node),
            _ => match what_if.base_value(formula) {
                Some(value) => Ok(value),
                None => Err(refusal("a data table whose formula cell holds no value")),
            },
        },
    };
    if let Some((cell, on)) = uncomputed.get() {
        return TableOutcome::Waits { on, cell };
    }
    match outcome {
        Ok(value) if value.is_empty() => TableOutcome::Value(CalcValue::number(0.0)),
        Ok(value) => TableOutcome::Value(value),
        Err(refused) => TableOutcome::Refused(refused),
    }
}

/// One table cell's substitution.
struct WhatIf<'a> {
    state: &'a EngineState,
    inputs: &'a ModelRoot,
    view: RecalcView<'a>,
    mode: &'a NumericMode,
    overrides: BTreeMap<CellKey, CalcValue>,
    affected: BTreeSet<NodeId>,
    /// Each affected formula's value under the substitution: `None` while it is being evaluated.
    memo: RefCell<BTreeMap<NodeId, Option<Result<CalcValue, NotModeled>>>>,
    /// The first refusal an affected formula gave, which the table cell is refused with.
    refused: RefCell<Option<NotModeled>>,
    uncomputed: &'a Cell<Option<(CellKey, NodeId)>>,
    /// The observer shown the evaluation of the table's formula, as the table cell's.
    observe: RefCell<Option<TableObserver<'a>>>,
}

/// The caller's observer, shown the evaluation of the data table's formula node `formula` under
/// the substitution of `cell` as that cell's evaluation (dc-10y), unless the evaluation read a
/// cell the recalculation has yet to calculate and is discarded.
struct TableObserver<'a> {
    calculation: &'a Calculation,
    observer: &'a mut dyn ReadObserver,
    formula: Option<NodeId>,
    cell: CellKey,
}

/// The instrument of one evaluation of a data table's formula.
struct TableInstrument<'o, 'a> {
    observe: &'o mut TableObserver<'a>,
    uncomputed: &'o Cell<Option<(CellKey, NodeId)>>,
    /// Set when the evaluation read a formula a circular reference left uncalculated.
    kept_read: Option<&'o Cell<bool>>,
}

impl EvaluationObserver for TableInstrument<'_, '_> {
    /// The calls of the table's formula under a substitution are not shown: the observer knows
    /// them as the formula's own when the formula is calculated.
    fn called(&mut self, _call: calc_fml::ExecutedCall) {}

    fn returned(&mut self, report: EvaluationReport<'_>) {
        if self.uncomputed.get().is_some() || self.kept_read.is_some_and(Cell::get) {
            return;
        }
        let evaluation =
            ObservedEvaluation::for_table_cell(self.observe.calculation, report, self.observe.cell);
        self.observe.observer.evaluated(evaluation);
    }
}

impl WhatIf<'_> {
    /// The value of the cell `at` as the recalculation stands, without the substitution.
    fn base_value(&self, at: CellKey) -> Option<CalcValue> {
        let mut value = None;
        WorkbookReader::new(self.view, self.uncomputed).read_area(GridArea::cell(at), &mut |run| {
            value = match run {
                CellRun::Values(values) => Some(values[0].clone()),
                CellRun::Blank(_) => Some(CalcValue::empty()),
                CellRun::Missing(_) => None,
            };
            ControlFlow::Continue(())
        });
        value
    }

    /// The value of the affected formula `node` under the substitution.
    fn value(&self, node: NodeId) -> Result<CalcValue, NotModeled> {
        if let Some(held) = self.memo.borrow().get(&node) {
            return match held {
                Some(result) => result.clone(),
                None => Err(refusal(
                    "a data table whose substitution reaches a circular reference",
                )),
            };
        }
        self.memo.borrow_mut().insert(node, None);
        let result = self.evaluate(node);
        if let Err(refused) = &result {
            self.refused
                .borrow_mut()
                .get_or_insert_with(|| refused.clone());
        }
        self.memo.borrow_mut().insert(node, Some(result.clone()));
        result
    }

    fn evaluate(&self, node: NodeId) -> Result<CalcValue, NotModeled> {
        let instance = match self.state.bindings.get(&node) {
            Some(Binding::Placed(instance, ArrayRole::Cell)) => instance,
            Some(Binding::Refused(refusal)) => return Err(refusal.clone()),
            Some(_) => {
                return Err(refusal(
                    "a data table whose substitution reaches an array formula or another table",
                ));
            }
            None => return Err(refusal("a data table whose formula is not bound")),
        };
        let mut reader = WhatIfReader { what_if: self };
        let mut scratch = EvalScratch::new();
        let catalog = SnapshotCatalog(self.inputs);
        // The observer is taken out while the table's formula is evaluated, so the evaluations
        // of the formulas it reads under the substitution are not shown as the cell's.
        let top = self.observe.borrow().as_ref().and_then(|observe| observe.formula) == Some(node);
        let mut taken = if top { self.observe.borrow_mut().take() } else { None };
        let evaluated = {
            let mut instrument = taken.as_mut().map(|observe| TableInstrument {
                observe,
                uncomputed: self.uncomputed,
                kept_read: self.view.kept.map(|(_, read)| read),
            });
            let mut services =
                EvaluationServices::new(self.mode, &mut reader).with_catalog(&catalog);
            if let Some(instrument) = instrument.as_mut() {
                services = services.observed_by(instrument);
            }
            evaluate(EvalFrame::new(instance, &mut scratch), &mut services)
        };
        if taken.is_some() {
            *self.observe.borrow_mut() = taken;
        }
        match evaluated {
            Ok(value) if value.as_array().is_some() || value.callable_value().is_some() => Err(
                refusal("a data table whose formula's value is an array or a LAMBDA value"),
            ),
            Ok(value) => Ok(value),
            Err(fault) => Err(self
                .refused
                .borrow()
                .clone()
                .unwrap_or(NotModeled::Evaluation(fault))),
        }
    }

    /// The cells of `area` the substitution changes, by their index in row-major order: the
    /// input cells and the affected formulas' anchors in it.
    fn changed_in(&self, area: GridArea) -> BTreeMap<u64, Changed> {
        let width = u64::from(area.column_count());
        let index = |at: CellKey| {
            u64::from(at.row() - area.first().row()) * width
                + u64::from(at.column() - area.first().column())
        };
        let mut out = BTreeMap::new();
        for at in self.overrides.keys().filter(|at| area.contains(**at)) {
            out.insert(index(*at), Changed::Input(*at));
        }
        for node in &self.affected {
            if let Some(record) = self.state.node(*node)
                && area.contains(record.anchor())
            {
                out.entry(index(record.anchor()))
                    .or_insert(Changed::Formula(*node));
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
enum Changed {
    Input(CellKey),
    Formula(NodeId),
}

/// The reader of one substituted evaluation: the input cells read as their substituted values,
/// the affected formulas as their values under the substitution, and every other cell as the
/// engine's reader gives it.
struct WhatIfReader<'w, 'a> {
    what_if: &'w WhatIf<'a>,
}

impl ReferenceReader for WhatIfReader<'_, '_> {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let what_if = self.what_if;
        let changed = what_if.changed_in(area);
        let mut base = WorkbookReader::new(what_if.view, what_if.uncomputed);
        if changed.is_empty() {
            base.read_area(area, sink);
            return;
        }
        let width = u64::from(area.column_count());
        let first = area.first();
        // Reads the cells of indexes `from..to` (row-major) through the engine's reader, one row
        // segment at a time.
        let mut plain =
            |from: u64, to: u64, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>| {
                let mut at = from;
                while at < to {
                    let row = at / width;
                    let row_end = ((row + 1) * width).min(to);
                    let segment = GridArea::new(
                        CellKey::new(
                            area.sheet(),
                            first.row() + row as u32,
                            first.column() + (at % width) as u32,
                        )
                        .expect("a cell of the area"),
                        CellKey::new(
                            area.sheet(),
                            first.row() + row as u32,
                            first.column() + ((row_end - 1) % width) as u32,
                        )
                        .expect("a cell of the area"),
                    )
                    .expect("one row of the area");
                    let mut stopped = false;
                    base.read_area(segment, &mut |run| {
                        let flow = sink(run);
                        stopped |= flow.is_break();
                        flow
                    });
                    if stopped {
                        return ControlFlow::Break(());
                    }
                    at = row_end;
                }
                ControlFlow::Continue(())
            };
        let mut next = 0u64;
        for (index, change) in changed {
            if plain(next, index, sink).is_break() {
                return;
            }
            let value = match change {
                Changed::Input(at) => Some(what_if.overrides[&at].clone()),
                Changed::Formula(node) => what_if.value(node).ok(),
            };
            let flow = match value {
                Some(value) => sink(CellRun::Values(std::slice::from_ref(&value))),
                None => sink(CellRun::Missing(1)),
            };
            if flow.is_break() {
                return;
            }
            next = index + 1;
        }
        let _ = plain(next, area.cell_count(), sink);
    }

    fn spill_extent(&mut self, anchor: CellKey) -> SpillExtent {
        WorkbookReader::new(self.what_if.view, self.what_if.uncomputed).spill_extent(anchor)
    }

    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<calc_fml::SubtotalExclusions> {
        WorkbookReader::new(self.what_if.view, self.what_if.uncomputed).subtotal_exclusions(area)
    }

    fn environment(&self) -> Option<calc_fml::CalcEnvironment> {
        self.what_if.state.environment
    }
}

/// Whether `at` holds no input and lies in one of the snapshot's data tables: a node of the
/// graph whose result the table gives.
pub(crate) fn is_table_cell(inputs: &ModelRoot, at: CellKey) -> bool {
    !matches!(inputs.cell(at), Some(CellInput::Formula(_)))
        && inputs.definitions().data_table_containing(at).is_some()
}

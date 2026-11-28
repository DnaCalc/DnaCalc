//! Budgeted callers may calculate the ordinary graph of a retained model without live providers.
use crate::{HostProgress, HostState, ReadObserver, WorkBudget};
use calc_doc_model::{CellInput, FormulaForm, ModelRoot};
use calc_engine::{CalcScope, EngineState, EvalSelection, Prerequisite};
use calc_skin::Refusal;
use calc_value::{CellKey, GridArea, IdAllocator};
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

/// Formula bindings for a retained model. No formula can advance before the requested scope is checked.
pub struct TrialGraph {
    host: HostState,
}

impl TrialGraph {
    /// Bind the graph of the retained model before evaluating any formula.
    pub fn bind(source: &HostState, model: Arc<ModelRoot>) -> Result<Self, Refusal> {
        if source.loaded.has_vba_project()
            || source
                .loaded
                .sheets()
                .iter()
                .any(|sheet| sheet.is_macro_sheet())
        {
            return Err(unavailable(
                "workbooks containing executable macro projects or sheets",
            ));
        }
        let engine = EngineState::new(&model);
        let mut host = HostState::from_parts(
            source.loaded.clone(),
            model,
            IdAllocator::new(),
            engine,
            source.options.clone(),
        );
        host.axis_edits = source.axis_edits.clone();
        host.file = source.file.clone();
        host.application = source.application.clone();
        crate::calculate_full(&mut host);
        match crate::step(
            &mut host,
            WorkBudget::evaluations(0),
            &mut crate::Unobserved,
        ) {
            HostProgress::Calculating { .. }
            | HostProgress::Published(_)
            | HostProgress::Current => {}
            _ => return Err(unavailable("a workbook whose calculation cannot start")),
        }
        Ok(Self { host })
    }

    fn excluded(&self) -> impl Iterator<Item = (CellKey, &'static str)> + '_ {
        self.host.engine.nodes().filter_map(|node| {
            let reason = match self.host.engine.instance(node.id()) {
                None => "unbound formulas and what-if data tables",
                Some(instance) => {
                    let plan = instance.plan();
                    if plan.reads_volatile_inputs() {
                        "volatile formulas"
                    } else if plan.has_user_calls()
                        || plan.reads_external_links()
                        || plan
                            .calls()
                            .any(|call| call.spelling().eq_ignore_ascii_case("RTD"))
                    {
                        "user functions, RTD and external workbook links"
                    } else {
                        return None;
                    }
                }
            };
            Some((node.anchor(), reason))
        })
    }

    /// The reason a full calculation of this snapshot would be refused.
    pub fn whole_refusal(&self) -> Option<&'static str> {
        self.excluded().next().map(|(_, reason)| reason)
    }

    /// True when a partial calculation can use scalar graph edges to delimit the effects.
    pub fn scalar_graph(&self) -> bool {
        self.host.model.definitions().arrays().next().is_none()
            && self.host.model.definitions().data_tables().is_empty()
            && self.host.engine.nodes().all(|node| {
                !matches!(self.host.model.cell(node.anchor()),
                    Some(CellInput::Formula(formula)) if formula.form() == FormulaForm::Dynamic)
            })
    }

    /// Follow direct and transitive placed readers of scalar writes.
    pub fn readers_of(
        &self,
        writes: &[CellKey],
        visit: &mut dyn FnMut() -> Result<(), Refusal>,
    ) -> Result<BTreeSet<CellKey>, Refusal> {
        let mut affected = BTreeSet::new();
        let mut pending: VecDeque<_> = writes.iter().copied().collect();
        while let Some(at) = pending.pop_front() {
            if !affected.insert(at) {
                continue;
            }
            visit()?;
            pending.extend(
                self.host
                    .engine
                    .readers_of_area(&self.host.model, GridArea::cell(at)),
            );
        }
        Ok(affected)
    }

    /// Authorize a whole snapshot calculation when its plans are safe in isolation.
    pub fn whole(self) -> Result<CalculationTrial, Refusal> {
        if let Some(reason) = self.whole_refusal() {
            return Err(unavailable(reason));
        }
        Ok(CalculationTrial {
            host: self.host,
            empty: false,
        })
    }

    /// Authorize the selected scalar formulas and the placed formula prerequisites they read.
    pub fn restrict(
        mut self,
        outputs: &BTreeSet<CellKey>,
        visit: &mut dyn FnMut() -> Result<(), Refusal>,
    ) -> Result<CalculationTrial, Refusal> {
        if !self.scalar_graph() {
            return Err(unavailable(
                "a partial graph with array or data table outputs",
            ));
        }
        let mut cells = BTreeSet::new();
        let mut pending: VecDeque<_> = outputs.iter().copied().collect();
        while let Some(at) = pending.pop_front() {
            let Some(node) = self
                .host
                .engine
                .node_at(at)
                .and_then(|id| self.host.engine.node(id))
            else {
                continue;
            };
            if !cells.insert(at) {
                continue;
            }
            visit()?;
            for edge in node.prerequisites().unwrap_or_default() {
                let areas = match *edge {
                    Prerequisite::Name(_) => continue,
                    Prerequisite::Cell(cell) => vec![GridArea::cell(cell)],
                    Prerequisite::Area(area) => vec![area],
                    Prerequisite::AxisValue(interval) => vec![interval.area()],
                    Prerequisite::SheetSpan(span) => {
                        let area = span.target();
                        span.sheets(&self.host.model)
                            .into_iter()
                            .flatten()
                            .filter_map(|sheet| {
                                let cell = |at: CellKey| {
                                    CellKey::new(sheet.id(), at.row(), at.column()).ok()
                                };
                                GridArea::new(cell(area.first())?, cell(area.last())?).ok()
                            })
                            .collect()
                    }
                };
                for area in areas {
                    visit()?;
                    pending.extend(self.host.engine.anchors_in(area));
                }
            }
        }
        if let Some((_, effect)) = self.excluded().find(|(at, _)| cells.contains(at)) {
            return Err(unavailable(effect));
        }
        self.host.work = None;
        if cells.is_empty() {
            return Ok(CalculationTrial {
                host: self.host,
                empty: true,
            });
        }
        let scope = CalcScope::Range(cells.into_iter().map(GridArea::cell).collect());
        self.host.requested_calculation = Some((scope, EvalSelection::DirtyAndVolatile));
        self.host.calculate_requested = true;
        Ok(CalculationTrial {
            host: self.host,
            empty: false,
        })
    }
}

/// A private result destination for consequence review with no workbook edit or provider API.
pub struct CalculationTrial {
    host: HostState,
    empty: bool,
}

impl CalculationTrial {
    /// Advance the ordinary host calculation under the caller's budget.
    pub fn advance(&mut self, budget: WorkBudget, observer: &mut dyn ReadObserver) -> HostProgress {
        if self.empty {
            return HostProgress::Current;
        }
        crate::step(&mut self.host, budget, observer)
    }

    /// Whether the requested scope published its completed generation.
    pub fn is_complete(&self) -> bool {
        self.empty || self.host.work.is_none() && self.host.published == Some(self.host.revision())
    }

    /// The disposable host, for projections against the retained model.
    pub fn host(&self) -> &HostState {
        &self.host
    }

    /// The formula anchors in the bound graph.
    pub fn anchors(&self) -> impl Iterator<Item = CellKey> + '_ {
        self.host.engine.nodes().map(|node| node.anchor())
    }

    /// The result spill areas of the trial's calculation.
    pub fn spills(&self) -> impl Iterator<Item = GridArea> + '_ {
        self.host.engine.spills().map(|spill| spill.area())
    }
}

fn unavailable(what: &str) -> Refusal {
    Refusal::NotModelled {
        owner: "dc-zh096".into(),
        what: format!("Value preview for {what}"),
    }
}

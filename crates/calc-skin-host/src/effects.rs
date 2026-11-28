use crate::query::{QueryWork, raw, shown_value_for};
use calc_doc_model::{ArrayKind, CellInput, ModelRoot};
use calc_host::value::{CellKey, GridArea};
use calc_host::{
    CalculationTrial, CellRun, HostProgress, HostState, ProjectionRequest, ReadObserver,
    TrialGraph, WorkBudget,
};
use calc_skin::{Budget, Preview, Provenance, RawValue, Refusal, Shown, ValueEffect, grid::Key};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::ControlFlow,
    sync::Arc,
};

pub(crate) fn append(
    host: &HostState,
    preview: &mut Preview,
    after: Arc<ModelRoot>,
    budget: Budget,
) {
    if !matches!(
        preview.kind(),
        Some(calc_skin::ChangeKind::Model | calc_skin::ChangeKind::Both)
    ) {
        return;
    }
    let writes = preview
        .writes
        .iter()
        .map(|write| crate::cell(write.at))
        .collect::<Result<Vec<_>, _>>();
    match writes
        .and_then(|writes| compare(host, after, &writes, !preview.properties.is_empty(), budget))
    {
        Ok(effects) => preview.effects = effects,
        Err(refusal) => {
            preview.refusal = Some(refusal);
            preview
                .warnings
                .push("Review writes only, or commit and undo with Ctrl+Z".into());
        }
    }
}

fn compare(
    host: &HostState,
    model: Arc<ModelRoot>,
    writes: &[CellKey],
    structural: bool,
    budget: Budget,
) -> Result<Vec<ValueEffect>, Refusal> {
    let mut meter = QueryWork::new(budget, "Review writes only, or commit and undo with Ctrl+Z");
    let previous = host.model();
    let next = model.clone();
    let before = prepare(host, previous.clone(), &mut meter)?;
    let after = prepare(host, model, &mut meter)?;
    let blocked = before.whole_refusal().or_else(|| after.whole_refusal());
    let mut cells = BTreeSet::new();
    let (mut before, mut after) = if let Some(reason) = blocked {
        if structural || !before.scalar_graph() || !after.scalar_graph() {
            return Err(unavailable(reason));
        }
        let mut visit = || meter.work(0, 0, 1);
        cells.extend(before.readers_of(writes, &mut visit)?);
        cells.extend(after.readers_of(writes, &mut visit)?);
        cells.retain(|at| {
            [previous.as_ref(), next.as_ref()]
                .iter()
                .any(|model| matches!(model.cell(*at), Some(CellInput::Formula(_))))
        });
        (
            before.restrict(&cells, &mut visit)?,
            after.restrict(&cells, &mut visit)?,
        )
    } else {
        (before.whole()?, after.whole()?)
    };
    calculate(&mut before, &mut meter)?;
    calculate(&mut after, &mut meter)?;
    if blocked.is_none() {
        add_outputs(&before, &mut cells, &mut meter)?;
        add_outputs(&after, &mut cells, &mut meter)?;
    }
    meter.work(cells.len() as u64, 0, 0)?;
    let previous = capture(before.host(), &cells, &mut meter)?;
    let next = capture(after.host(), &cells, &mut meter)?;
    let mut effects = Vec::new();
    for at in cells {
        let blank = || Shown {
            value: RawValue::Blank,
            provenance: Provenance::Empty,
        };
        let before = previous.get(&at).cloned().unwrap_or_else(blank);
        let after = next.get(&at).cloned().unwrap_or_else(blank);
        if before.value != after.value {
            let effect = ValueEffect {
                at: Key::of(at),
                before,
                after,
            };
            meter.charge_bytes(&effect)?;
            effects.push(effect);
        }
    }
    meter.check()?;
    Ok(effects)
}

fn add_outputs(
    trial: &CalculationTrial,
    cells: &mut BTreeSet<CellKey>,
    meter: &mut QueryWork,
) -> Result<(), Refusal> {
    cells.extend(trial.anchors());
    let legacy = trial
        .host()
        .model()
        .definitions()
        .arrays()
        .filter(|array| array.kind() == ArrayKind::Legacy)
        .map(|array| array.area());
    for area in trial.spills().chain(legacy) {
        meter.work(area.cell_count(), 0, 0)?;
        for row in area.first().row()..=area.last().row() {
            for column in area.first().column()..=area.last().column() {
                cells.insert(CellKey::new(area.sheet(), row, column).expect("a cell in a spill"));
            }
        }
    }
    Ok(())
}

fn prepare(
    source: &HostState,
    model: Arc<ModelRoot>,
    meter: &mut QueryWork,
) -> Result<TrialGraph, Refusal> {
    for sheet in model.sheets() {
        for (_, input) in model.sheet_cells(sheet.id()).into_iter().flatten() {
            meter.work(1, u64::from(matches!(input, CellInput::Formula(_))), 0)?;
        }
    }
    TrialGraph::bind(source, model)
}

fn calculate(trial: &mut CalculationTrial, meter: &mut QueryWork) -> Result<(), Refusal> {
    let mut observer = Reads {
        meter,
        refusal: None,
    };
    while !trial.is_complete() {
        observer.meter.check()?;
        let progress = trial.advance(WorkBudget::evaluations(64), &mut observer);
        if let Some(error) = observer.refusal.take() {
            return Err(error);
        }
        match progress {
            HostProgress::Current => break,
            HostProgress::Published(_) | HostProgress::Calculating { .. } => {}
            _ => {
                return Err(unavailable(
                    "The retained workbook calculation could not finish",
                ));
            }
        }
    }
    if !trial.host().circular_cells().is_empty() {
        return Err(unavailable("circular calculations"));
    }
    observer.meter.check()
}

struct Reads<'a> {
    meter: &'a mut QueryWork,
    refusal: Option<Refusal>,
}

impl ReadObserver for Reads<'_> {
    fn observe(
        &mut self,
        _: CellKey,
        _: GridArea,
        offset: u64,
        run: CellRun<'_>,
    ) -> ControlFlow<()> {
        match self.meter.work(run.len(), 0, u64::from(offset == 0)) {
            Ok(()) => ControlFlow::Continue(()),
            Err(error) => {
                self.refusal = Some(error);
                ControlFlow::Break(())
            }
        }
    }
}

fn capture(
    host: &HostState,
    cells: &BTreeSet<CellKey>,
    meter: &mut QueryWork,
) -> Result<BTreeMap<CellKey, Shown<RawValue>>, Refusal> {
    let request = ProjectionRequest {
        cells: cells
            .iter()
            .copied()
            .filter(|at| host.model().sheet_by_id(at.sheet()).is_some())
            .collect(),
    };
    let page =
        calc_host::project(host, &request).map_err(|error| unavailable(&error.to_string()))?;
    page.cells()
        .iter()
        .map(|cell| {
            let (value, provenance) = shown_value_for(host, cell.value());
            if !matches!(
                provenance,
                Provenance::Literal | Provenance::Empty | Provenance::Calculated { .. }
            ) {
                return Err(unavailable(
                    "A formula did not produce a calculated result in the retained workbook",
                ));
            }
            let value = value.as_ref().map_or(RawValue::Blank, raw);
            if matches!(value, RawValue::Other { .. }) {
                return Err(unavailable(
                    "A retained output has no scalar representation",
                ));
            }
            let shown = Shown { value, provenance };
            meter.charge_bytes(&shown)?;
            Ok((cell.at(), shown))
        })
        .collect()
}

fn unavailable(what: &str) -> Refusal {
    crate::unavailable("dc-zh096", &format!("{what}. Commit and undo with Ctrl+Z."))
}

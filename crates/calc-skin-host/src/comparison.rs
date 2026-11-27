//! A session comparison against a pinned workbook publication.

use crate::{State, invalid, query};
use calc_host::{ChartView, ProjectionRequest, project, value::CellKey};
use calc_skin::query::{NumericComparison, Query};
use calc_skin::{AnchorId, Bound, Budget, Provenance, RawValue, Refusal, SourceRef, WorkbookId};
use std::collections::BTreeMap;

enum Value {
    Number(u64),
    Chart(ChartView),
}

pub(crate) struct Baseline {
    workbook: WorkbookId,
    values: BTreeMap<AnchorId, (SourceRef, Value)>,
}

fn number(state: &State, at: CellKey) -> Result<Option<u64>, Refusal> {
    let page = project(state.session.host(), &ProjectionRequest { cells: vec![at] })
        .map_err(|error| invalid(error.to_string()))?;
    let Some(cell) = page.cells().first() else {
        return Ok(None);
    };
    let value = query::show_cell(state, cell).shown;
    if let RawValue::Number { bits } = value.value
        && matches!(
            value.provenance,
            Provenance::Literal | Provenance::Calculated { .. }
        )
    {
        Ok(Some(bits))
    } else {
        Ok(None)
    }
}

impl Baseline {
    pub(crate) fn capture(
        state: &State,
        bounds: &[Bound],
        budget: Budget,
    ) -> Result<Self, Refusal> {
        let host = state.session.host();
        if host.has_pending_calculation() {
            return Err(invalid("Finish recalculation before pinning a baseline"));
        }
        let mut meter = query::QueryWork::new(budget, "Pin fewer worksheet values and charts");
        let mut values = BTreeMap::new();
        for bound in bounds {
            if values.contains_key(&bound.anchor) {
                continue;
            }
            let source = query::bound_source(state, bound)?;
            meter.work(1, 1, 0)?;
            let value = match &source {
                SourceRef::Object { sheet, name } => {
                    let chart = host
                        .chart_named(sheet.id(), name)
                        .ok_or_else(|| invalid("Source missing"))?;
                    for series in chart.plots.iter().flat_map(|plot| &plot.series) {
                        meter.work(
                            series.values.len().max(series.categories.len()) as u64,
                            1,
                            0,
                        )?;
                    }
                    if chart
                        .plots
                        .iter()
                        .flat_map(|plot| &plot.series)
                        .any(|series| series.cached)
                    {
                        return Err(invalid(
                            "Calculate this chart's values before pinning a baseline",
                        ));
                    }
                    Some(Value::Chart(chart))
                }
                _ => number(state, query::bound_cell(state, bound)?)?.map(Value::Number),
            };
            if let Some(value) = value {
                values.insert(bound.anchor, (source, value));
            }
        }
        if values.is_empty() {
            return Err(invalid(
                "This page has no live numeric values or charts to pin",
            ));
        }
        Ok(Self {
            workbook: state.stamp().workbook,
            values,
        })
    }

    fn value(&self, workbook: WorkbookId, bound: &Bound, source: &SourceRef) -> Option<&Value> {
        if self.workbook != workbook {
            return None;
        }
        self.values
            .get(&bound.anchor)
            .filter(|(expected, _)| expected == source)
            .map(|(_, value)| value)
    }
}

pub(crate) fn scalar(state: &State, bound: &Bound) -> Result<Option<NumericComparison>, Refusal> {
    let source = query::bound_source(state, bound)?;
    let baseline = state
        .baseline
        .as_ref()
        .and_then(|baseline| baseline.value(state.stamp().workbook, bound, &source));
    let Some(Value::Number(baseline)) = baseline else {
        return Ok(None);
    };
    let current = number(state, query::bound_cell(state, bound)?)?;
    Ok(current.map(|current| NumericComparison {
        baseline: *baseline,
        current,
    }))
}

pub(crate) fn chart<'a>(
    state: &'a State,
    bound: &Bound,
    source: &SourceRef,
) -> Option<&'a ChartView> {
    match state
        .baseline
        .as_ref()?
        .value(state.stamp().workbook, bound, source)?
    {
        Value::Chart(chart) => Some(chart),
        Value::Number(_) => None,
    }
}

//! Read one set of resolved workbook bindings without interleaving calculation work.

use crate::query::{QueryWork, bound_cell, bound_source, chart_image, show_cell};
use crate::{State, invalid};
use calc_skin::{
    BindingKind, BindingRead, BindingSample, BindingSnapshot, BindingValue, Budget, Coverage,
    PartialWhy, Refusal, RowPage, RowWindow, SourceRef,
};

const ALTERNATIVE: &str = "Choose fewer bindings or smaller worksheet ranges";

pub(crate) fn capture(
    state: &State,
    reads: &[BindingRead],
    budget: Budget,
) -> Result<(BindingSnapshot, Coverage), Refusal> {
    if state.session.has_unfinished_calculation() {
        return Ok((
            BindingSnapshot::default(),
            Coverage::Partial {
                done: 0,
                total: Some(reads.len() as u64),
                why: PartialWhy::Pending,
            },
        ));
    }
    let mut total = QueryWork::new(budget, ALTERNATIVE);
    total.work(0, reads.len() as u64, 0)?;
    let mut items = Vec::with_capacity(reads.len());
    for request in reads {
        let budget = total.remaining()?;
        let mut work = QueryWork::new(budget, ALTERNATIVE);
        let mut item = BindingSample {
            request: request.clone(),
            value: read_one(state, request, budget, &mut work),
        };
        if item.value.is_ok()
            && let Err(refusal) = work.charge_bytes(&item)
        {
            item.value = Err(refusal);
        }
        if item.value.is_ok() {
            total.work(work.counts.cells, work.counts.nodes, work.counts.edges)?;
        }
        total.charge_bytes(&item)?;
        items.push(item);
    }
    Ok((
        BindingSnapshot {
            captured_unix_ms: captured_unix_ms(),
            items,
        },
        Coverage::Complete,
    ))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn captured_unix_ms() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn captured_unix_ms() -> Option<u64> {
    let time = js_sys::Date::now();
    (time.is_finite() && time >= 0.0).then_some(time as u64)
}

fn read_one(
    state: &State,
    request: &BindingRead,
    budget: Budget,
    work: &mut QueryWork,
) -> Result<BindingValue, Refusal> {
    work.check()?;
    let bound = &request.bound;
    match request.kind {
        BindingKind::Value => {
            work.work(1, 0, 0)?;
            let at = bound_cell(state, bound)?;
            let page = calc_host::project(
                state.session.host(),
                &calc_host::ProjectionRequest { cells: vec![at] },
            )
            .map_err(|error| invalid(error.to_string()))?;
            let cell = page
                .cells()
                .first()
                .ok_or_else(|| invalid("Source missing"))?;
            Ok(BindingValue::Cell(show_cell(state, cell)))
        }
        BindingKind::Rows { limit } => {
            let source = bound_source(state, bound)?;
            read_rows(state, &source, limit, budget, work).map(BindingValue::Rows)
        }
        BindingKind::Chart { comparison } => {
            let source = bound_source(state, bound)?;
            chart_image(state, &source, comparison.then_some(bound), work).map(BindingValue::Chart)
        }
        BindingKind::Comparison => {
            work.work(1, 0, 0)?;
            crate::comparison::scalar(state, bound).map(BindingValue::Comparison)
        }
    }
}

fn read_rows(
    state: &State,
    source: &SourceRef,
    limit: Option<u32>,
    budget: Budget,
    work: &mut QueryWork,
) -> Result<RowPage, Refusal> {
    let read = |offset, limit| {
        crate::rows::read(state, source, &[], &[], RowWindow { offset, limit }, budget)
            .map(|(page, _)| page)
    };
    let mut result = read(0, 0)?;
    let count = limit
        .unwrap_or(result.matched_rows)
        .min(result.matched_rows);
    let cells = u64::from(count) * u64::from(result.columns)
        + result.totals.as_ref().map_or(0, |row| row.len() as u64);
    work.work(cells, 0, 0)?;
    for offset in (0..count).step_by(crate::rows::MAX_WINDOW_ROWS as usize) {
        let limit = (count - offset).min(crate::rows::MAX_WINDOW_ROWS);
        result.rows.extend(read(offset, limit)?.rows);
        work.check()?;
    }
    Ok(result)
}

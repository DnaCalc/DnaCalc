//! Explicit worksheet pivot refresh over captured source records. This is aggregation, not a
//! second worksheet evaluator. Ordinary source edits do not refresh these snapshots.
use crate::{CalcTick, Calculation};
use calc_doc_model::{AuthoredRevision, pivot::*};
use calc_value::{CalcValue, CellKey, ExcelText, GridArea, WorksheetErrorCode};
use std::{cmp::Ordering, collections::BTreeMap};

pub fn refusal(part: &str, detail: impl Into<String>) -> PivotIssue {
    PivotIssue {
        part: part.into(),
        detail: detail.into(),
        owner: "dc-8wp",
    }
}
struct Group {
    pub(super) key: PivotItem,
    pub(super) shared: u32,
    pub(super) first: usize,
    pub(super) total: Total,
    pub(super) old: usize,
    pub(super) hidden: bool,
    pub(super) excluded: bool,
}
#[derive(Debug)]
pub struct PivotLayout {
    pub table: PivotTable,
    pub cells: BTreeMap<CellKey, CalcValue>,
    /// Explicit data-field number format IDs, keyed by output cell.
    pub number_formats: BTreeMap<CellKey, u32>,
}


mod aggregate;
mod axis;
mod formula;
mod model;
mod records;
mod render;
use aggregate::Total;
use axis::render_axes;
use records::{compare, field_groups, page_allows, page_field_items, resolve};
pub use records::item_value;
use render::{Form, label_text, truthy};
pub use model::{cube_model, valid_model_snapshot};
pub(crate) use model::data_cell as model_data_cell;

/// Calculate a pivot from its retained worksheet records or data-model snapshot.
pub fn layout(table: &PivotTable, cache: &PivotCache) -> Result<PivotLayout, PivotIssue> {
    if matches!(cache.snapshot.as_ref().map(|snapshot| &snapshot.origin), Some(PivotSnapshotOrigin::DataModel { .. })) {
        model::layout(table, cache)
    } else {
        render::layout(table, cache)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PivotResults {
    pub cells: BTreeMap<CellKey, (CalcValue, Calculation)>,
    pub number_formats: BTreeMap<CellKey, u32>,
    pub issues: Vec<PivotIssue>,
}
/// Rebuild only derived output; snapshot records remain the actual source readings.
pub(crate) fn calculate(state: &PivotState, tick: CalcTick) -> PivotResults {
    let mut out = PivotResults::default();
    for table in state.tables() {
        let Some(cache) = state.caches.caches.get(&table.cache_id) else {
            continue;
        };
        let Some(snapshot) = cache.snapshot.as_ref() else {
            continue;
        };
        if table.collapsed {
            // No room for the output: one cell, the top-left, holds the error Excel shows (code 2045).
            let calculation = Calculation::new(
                AuthoredRevision::new(snapshot.source_revision),
                tick,
                snapshot.numeric.clone(),
            );
            out.cells.insert(table.area.first(), (CalcValue::error(WorksheetErrorCode::Spill), calculation));
            continue;
        }
        if table.row_fields.is_empty()
            && table.column_fields.is_empty()
            && table.page_fields.is_empty()
            && table.data_fields.is_empty()
        {
            // The empty pivot: Excel's placeholder frame holds no value (m7-names stub-*).
            continue;
        }
        match layout(table, cache) {
            Ok(layout) if layout.table.area == table.area => {
                let calculation = Calculation::new(
                    AuthoredRevision::new(snapshot.source_revision),
                    tick,
                    snapshot.numeric.clone(),
                );
                for (at, value) in layout.cells {
                    out.cells.insert(at, (value, calculation.clone()));
                }
                out.number_formats.extend(layout.number_formats);
            }
            Ok(_) => out.issues.push(refusal(
                &table.part,
                "The stored output extent does not match its source snapshot",
            )),
            Err(issue) => out.issues.push(issue),
        }
    }
    out
}

mod index;
pub use index::data_cell;
pub(crate) use index::invalid_model_request;

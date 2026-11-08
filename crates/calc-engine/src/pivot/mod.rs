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
}


mod aggregate;
mod axis;
mod formula;
mod records;
mod render;
use aggregate::Total;
use axis::{Walk, axis_item, render_axes};
use records::{compare, field_groups, page_allows, page_field_items, resolve};
pub use records::item_value;
use render::{Form, label_text, truthy};
pub use render::layout;

#[derive(Clone, Debug, Default)]
pub struct PivotResults {
    pub cells: BTreeMap<CellKey, (CalcValue, Calculation)>,
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

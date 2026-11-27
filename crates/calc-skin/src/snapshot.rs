//! Materialized values for a set of bindings captured at one workbook stamp.

use crate::{Bound, ChartImage, Refusal, RowPage, WorkbookCell, query::NumericComparison};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingKind {
    Value,
    /// An absent limit requests the entire source within the caller's budget.
    Rows {
        limit: Option<u32>,
    },
    Chart {
        comparison: bool,
    },
    Comparison,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingRead {
    pub bound: Bound,
    pub kind: BindingKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BindingValue {
    Cell(WorkbookCell),
    Rows(RowPage),
    Chart(ChartImage),
    Comparison(Option<NumericComparison>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BindingSample {
    pub request: BindingRead,
    pub value: Result<BindingValue, Refusal>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BindingSnapshot {
    pub captured_unix_ms: Option<u64>,
    pub items: Vec<BindingSample>,
}

impl BindingSnapshot {
    pub fn get(&self, bound: &Bound, kind: BindingKind) -> Option<&BindingValue> {
        self.items
            .iter()
            .find(|item| item.request.bound == *bound && item.request.kind == kind)?
            .value
            .as_ref()
            .ok()
    }
}

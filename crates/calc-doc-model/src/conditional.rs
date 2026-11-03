//! Authored conditional-format rules, retaining source XML for fields no editor changes.
use crate::value::{CellKey, GridArea};

/// A rule's editable coordinates and condition, beside its lossless source representation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConditionalRuleSource {
    /// A worksheet fragment containing this rule and its namespace context.
    pub source: std::sync::Arc<[u8]>,
    pub areas: Vec<GridArea>,
    pub priority: i64,
    pub stop: bool,
    pub formulas: Vec<String>,
    /// Formula-valued colour/bar/icon thresholds, separate from condition formulas.
    pub threshold_formulas:Vec<String>,
    /// True when a command, rather than loading/structural movement, changed this rule.
    pub authored: bool,
}

impl ConditionalRuleSource {
    /// Its original condition anchor. A removed range has no remaining application.
    pub fn origin(&self) -> Option<CellKey> {
        self.areas.first().map(|a| a.first())
    }
}

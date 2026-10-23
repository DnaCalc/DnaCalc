//! Saving the host's current revision: the package as calc-doc writes it, with each formula
//! cell's calculated value as its cached value.

use crate::state::HostState;
use calc_doc::{PackageWrite, SavePolicy, SaveReceipt, SaveRefusal, WriteError};
use calc_doc_model::value::CoreValue;
use calc_doc_model::{CachedValue, CellInput, CellText, FiniteNumber};
use calc_engine::FormulaResult;
use std::collections::BTreeMap;
use std::fmt;

/// Why [`save`] wrote no package (or an incomplete one, which must be discarded).
#[derive(Debug, PartialEq, Eq)]
pub enum SaveFailed {
    /// calc-doc refused to plan the save; nothing was written.
    Refused(SaveRefusal),
    /// The write failed.
    Write(WriteError),
}

impl fmt::Display for SaveFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveFailed::Refused(refusal) => write!(f, "the save was refused: {refusal}"),
            SaveFailed::Write(error) => write!(f, "the write failed: {error}"),
        }
    }
}

impl std::error::Error for SaveFailed {}

/// The cached value a calculated value is saved as; `None` for a value no cell holds as a cache.
fn cached(core: &CoreValue) -> Option<CachedValue> {
    match core {
        CoreValue::Number(number) => FiniteNumber::new(*number).map(CachedValue::Number),
        CoreValue::Text(text) => CellText::new(text.clone()).map(CachedValue::Text),
        CoreValue::Logical(logical) => Some(CachedValue::Logical(*logical)),
        CoreValue::Error(code) => Some(CachedValue::Error(*code)),
        CoreValue::Empty | CoreValue::Missing | CoreValue::Array(_) | CoreValue::Reference(_) => {
            None
        }
    }
}

/// Writes the host's current revision to `out` through calc-doc's save
/// ([`calc_doc::LoadedWorkbook::snapshot_with_values`]). Each formula cell whose value the engine
/// calculated, and which still stands (no edit since has changed the formula or what it reads),
/// is saved with that value as its cached value; every other formula cell (one the engine
/// refused, one awaiting a recalculation) is saved without one, for Excel to calculate when it
/// opens the package. A revision that is still the loaded one is written as the package was
/// opened.
pub fn save(
    host: &HostState,
    policy: &SavePolicy,
    out: &mut dyn PackageWrite,
) -> Result<SaveReceipt, SaveFailed> {
    let stale = host
        .engine
        .stale(&host.model)
        .expect("the host keeps its authored revision and its engine state at one revision");
    let mut values = BTreeMap::new();
    for sheet in host.model.sheets() {
        for (at, input) in host.model.sheet_cells(sheet.id()).into_iter().flatten() {
            if !matches!(input, CellInput::Formula(_)) || stale.binary_search(&at).is_ok() {
                continue;
            }
            if let Some(FormulaResult::Calculated { value, .. }) = host.engine.result(at)
                && let Some(value) = cached(value.core())
            {
                values.insert(at, value);
            }
        }
    }
    let snapshot = host.loaded.snapshot_with_values(&host.model, values);
    let plan = calc_doc::prepare_save(&snapshot, policy).map_err(SaveFailed::Refused)?;
    calc_doc::write(plan, out).map_err(SaveFailed::Write)
}

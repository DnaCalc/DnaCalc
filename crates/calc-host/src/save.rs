//! Saving the host's current revision: the package as calc-doc writes it, with each formula
//! cell's calculated value as its cached value.

use crate::state::HostState;
use calc_doc::{PackageWrite, SavePolicy, SaveReceipt, SaveRefusal, StructuralEdits, WriteError};
use calc_doc_model::value::{CellKey, CoreRef, GridArea};
use calc_doc_model::{CachedValue, CellInput, CellText, FiniteNumber};
use calc_engine::FormulaResult;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

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

/// The immutable model and published result view selected before package preparation.
/// This is provenance for the save; it makes no workbook-wide freshness claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePin {
    pub workbook: calc_engine::contract::WorkbookInstanceId,
    pub model: calc_doc_model::AuthoredRevision,
    pub results: calc_engine::contract::ReadViewId,
    pub active_calculation: Option<calc_engine::contract::CalcCommandId>,
}

/// Owns the prepared package bytes and their source pin. Writing consumes only this plan,
/// so later edits, calculations, cancellation or replacement of the live host cannot mix views.
pub struct PreparedSave {
    pin: SavePin,
    plan: calc_doc::SavePlan,
}
impl PreparedSave {
    pub fn pin(&self) -> &SavePin {
        &self.pin
    }
    pub fn write(self, out: &mut dyn PackageWrite) -> Result<SaveReceipt, SaveFailed> {
        calc_doc::write(self.plan, out).map_err(SaveFailed::Write)
    }
}
impl fmt::Debug for PreparedSave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedSave")
            .field("pin", &self.pin)
            .finish_non_exhaustive()
    }
}

/// The cached value a calculated value is saved as; `None` for a value no cell holds as a cache.
fn cached(core: CoreRef<'_>) -> Option<CachedValue> {
    match core {
        CoreRef::Number(number) => FiniteNumber::new(number).map(CachedValue::Number),
        CoreRef::Text(text) => CellText::new(text.clone()).map(CachedValue::Text),
        CoreRef::Logical(logical) => Some(CachedValue::Logical(logical)),
        CoreRef::Error(code) => Some(CachedValue::Error(code)),
        CoreRef::Empty | CoreRef::Missing | CoreRef::Array(_) | CoreRef::Reference(_) => None,
    }
}

/// Prepares the host's pinned revision through calc-doc's save
/// ([`calc_doc::LoadedWorkbook::snapshot_with_results`]). Each formula cell whose value the engine
/// calculated, and which still stands (no edit since has changed the formula or what it reads),
/// is saved with that value as its cached value, a newer error (#SPILL!, #CALC!) with its value
/// metadata; every other formula cell (one the engine refused, one awaiting a recalculation) is
/// saved without one, for Excel to calculate when it opens the package. An array formula's anchor
/// whose result stands is saved with the area its result fills as its `ref` (its spill range, a
/// legacy array's area, or the anchor alone for a single value or #SPILL!), and each cell of that
/// area beside the anchor with the value the result shows there. Rows and columns inserted or
/// deleted since the open move the package's rows, columns and what lies on them
/// ([`calc_doc::DocumentSnapshot::with_structural_edits`]); what the save does not move is
/// refused ([`SaveRefusal::StructuralNotMoved`]). A revision that is still the loaded one is
/// written as the package was opened.
pub fn prepare_save(host: &HostState, policy: &SavePolicy) -> Result<PreparedSave, SaveFailed> {
    let pin = SavePin {
        workbook: host.engine.workbook_instance(),
        model: host.revision(),
        results: host.engine.result_view(),
        active_calculation: host.work.as_ref().map(|work| work.admission().command),
    };
    let stale = host
        .engine
        .stale(&host.model)
        .expect("the host keeps its authored revision and its engine state at one revision");
    let mut values = BTreeMap::new();
    let mut spills = BTreeMap::new();
    let definitions = host.model.definitions();
    for sheet in host.model.sheets() {
        for (at, input) in host.model.sheet_cells(sheet.id()).into_iter().flatten() {
            if !matches!(input, CellInput::Formula(_)) || stale.binary_search(&at).is_ok() {
                continue;
            }
            let Some(FormulaResult::Calculated { value, .. }) = host.engine.result(at) else {
                continue;
            };
            let Some(cached_value) = cached(value.core_ref()) else {
                continue;
            };
            values.insert(at, cached_value);
            if let Some(spill) = host.engine.spill(at) {
                let area = spill.area();
                spills.insert(at, area);
                for row in area.first().row()..=area.last().row() {
                    for column in area.first().column()..=area.last().column() {
                        let Ok(member) = CellKey::new(at.sheet(), row, column) else {
                            continue;
                        };
                        if member == at {
                            continue;
                        }
                        if let Some(member_value) = cached(spill.value_at(member).core_ref()) {
                            values.insert(member, member_value);
                        }
                    }
                }
            } else if definitions.array(at).is_some() {
                spills.insert(at, GridArea::cell(at));
            }
        }
    }
    // Rows or columns inserted or deleted since the open: the save moves the package's rows,
    // columns and what lies on them, as the model moved them (dc-jhn).
    let snapshot = host
        .loaded
        .snapshot_with_results(&host.model, values, spills)
        .with_structural_edits(StructuralEdits::new(
            host.axis_edits.clone(),
            Arc::new(calc_engine::ReferenceRewriter),
        ));
    let plan = calc_doc::prepare_save(&snapshot, policy).map_err(SaveFailed::Refused)?;
    Ok(PreparedSave { pin, plan })
}

/// Prepare and write one pinned package through the same route as a deferred save request.
pub fn save(
    host: &HostState,
    policy: &SavePolicy,
    out: &mut dyn PackageWrite,
) -> Result<SaveReceipt, SaveFailed> {
    prepare_save(host, policy)?.write(out)
}

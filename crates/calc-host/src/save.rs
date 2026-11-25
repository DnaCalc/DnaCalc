//! Saving the host's current revision: the package as calc-doc writes it, with each formula
//! cell's permitted cached value and its calculation metadata.

use crate::state::HostState;
use calc_doc::{PackageWrite, SavePolicy, SaveReceipt, SaveRefusal, StructuralEdits, WriteError};
use calc_doc_model::value::{CellKey, CoreRef, GridArea};
use calc_doc_model::{CachedValue, CellInput, CellText, FiniteNumber};
use calc_engine::FormulaResult;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

/// Why [`save`] wrote no package (or an incomplete one, which must be discarded).
#[derive(Debug, PartialEq, Eq)]
pub enum SaveFailed {
    /// calc-doc refused to plan the save; nothing was written.
    Refused(SaveRefusal),
    /// The write failed.
    Write(WriteError),
    /// The BeforeSave handler cancelled the save.
    Cancelled,
    /// The host refused the save before package writing.
    Operation {
        /// Work package responsible for the refusal.
        owner: &'static str,
        /// Explanation suitable for displaying to the caller.
        reason: String,
    },
}

impl fmt::Display for SaveFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveFailed::Refused(refusal) => write!(f, "the save was refused: {refusal}"),
            SaveFailed::Write(error) => write!(f, "the write failed: {error}"),
            SaveFailed::Cancelled => f.write_str("the BeforeSave handler cancelled the save"),
            SaveFailed::Operation { owner, reason } => write!(f, "{reason} ({owner})"),
        }
    }
}

impl std::error::Error for SaveFailed {}

/// Application or automation command that requested the save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOperation {
    /// Save the workbook in place.
    Save,
    /// Save the workbook to a supplied destination.
    SaveAs,
    /// A Save As selected in the application's file dialog.
    SaveAsDialog,
    /// Write a copy of the workbook.
    SaveCopyAs,
}

/// A registered event owner returns one authored batch before the serialization pin.
/// Loading a VBA project does not by itself register an event owner.
pub trait BeforeSaveHandler {
    /// Return the authored edits and cancellation decision for this operation.
    fn before_save(
        &mut self,
        host: &HostState,
        operation: SaveOperation,
    ) -> Result<BeforeSaveOutcome, String>;
}

/// Edits and cancellation requested by a BeforeSave handler.
#[derive(Debug, Default)]
pub struct BeforeSaveOutcome {
    /// Authored cell edits returned as one batch.
    pub edits: Vec<crate::CellEdit>,
    /// Whether to cancel package preparation.
    pub cancel: bool,
}

/// P06's measured command settings, supplied by the application/automation owner.
/// Disabled worksheets are session policy; Excel restores them enabled on reopen.
pub struct ExcelSaveContext<'a> {
    /// Command requesting the save.
    pub operation: SaveOperation,
    /// Whether the caller enabled calculation before saving.
    pub calculate_before_save: bool,
    /// Active worksheet used by the save calculation policy.
    pub active_sheet: calc_value::SheetId,
    /// Worksheets with calculation disabled in this session.
    pub disabled_sheets: &'a [calc_value::SheetId],
    /// Optional handler invoked ahead of the serialization pin.
    pub before_save: Option<&'a mut dyn BeforeSaveHandler>,
}

pub(crate) fn operation_failed(reason: impl Into<String>) -> SaveFailed {
    SaveFailed::Operation {
        owner: "dc-hsa5.4",
        reason: reason.into(),
    }
}

/// What a noncalculating save may retain. Dirty array topology is not yet permitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveCachePolicy {
    /// Write caches from current calculated results only.
    CurrentCalculatedOnly,
    /// Also retain scalar caches allowed by the manual-save profile.
    PreservePermittedCaches,
}

#[derive(Clone)]
pub(crate) struct SaveDemand {
    pub scope: Option<calc_engine::CalcScope>,
    pub caches: SaveCachePolicy,
    pub calculate_before_save: bool,
}

/// A save calculation is complete when every required cell is calculated or typed-refused. Excel saves a
/// workbook holding a cell the engine does not model; the refused cell keeps its file cache (and its provenance)
/// in the saved file, it is never written as calculated (dc-e6b0j).
pub(crate) fn save_completion_valid(receipt: &calc_engine::CompletionReceipt) -> bool {
    // TABLE sessions have real results but retain their Incomplete certificate.
    matches!(
        receipt.outcome,
        crate::CompletionOutcome::Complete | crate::CompletionOutcome::Incomplete
    ) && receipt.coverage.calculated + receipt.coverage.refused == receipt.coverage.required
        && receipt.coverage.held == 0
}

pub(crate) fn save_can_wait_for(host: &HostState, scope: &crate::CalcScope) -> bool {
    host.work.as_ref().is_none_or(|work| {
        work.revision() == host.revision()
            && work.live_inputs_current(&host.engine)
            && work.callables_current(&host.engine)
            && work.admission().range_post_state == crate::RangePostState::Satisfied
            && host
                .engine
                .nodes()
                .filter(|n| scope.contains(n.anchor()))
                .all(|n| work.admission().scope.contains(n.anchor()))
    })
}

pub(crate) fn save_scope_satisfied(host: &HostState, scope: &crate::CalcScope) -> bool {
    !host
        .engine
        .nodes()
        .any(|n| scope.contains(n.anchor()) && host.engine.calculation_required_at(n.anchor()))
        && host
            .engine
            .stale(&host.model)
            .is_some_and(|stale| !stale.iter().any(|at| scope.contains(*at)))
}

/// The immutable model and published result view selected before package preparation.
/// This is provenance for the save; it makes no workbook-wide freshness claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePin {
    /// Result destination that owns the pinned view.
    pub destination: calc_engine::contract::ResultDestination,
    /// Identity of the workbook instance being saved.
    pub workbook: calc_engine::contract::WorkbookInstanceId,
    /// Authored revision selected for serialization.
    pub model: calc_doc_model::AuthoredRevision,
    /// Identifier of the result view selected for serialization.
    pub results: calc_engine::contract::ReadViewId,
    /// Identifier of the calculation active when this pin was taken, if any.
    pub active_calculation: Option<calc_engine::contract::CalcCommandId>,
    /// Which formula caches may be serialized.
    pub cache_policy: SaveCachePolicy,
}

/// Owns the prepared package bytes and their source pin. Writing consumes only this plan,
/// so later edits, calculations, cancellation or replacement of the live host cannot mix views.
pub struct PreparedSave {
    pin: SavePin,
    plan: calc_doc::SavePlan,
    calculation: Option<calc_engine::contract::CompletionReceipt>,
}
impl PreparedSave {
    /// Return the calculation completion receipt attached to this save, if any.
    pub fn calculation(&self) -> Option<&calc_engine::contract::CompletionReceipt> {
        self.calculation.as_ref()
    }
    /// Identify the model, results, and cache policy used to prepare these bytes.
    pub fn pin(&self) -> &SavePin {
        &self.pin
    }
    /// Consume the plan and write to the supplied sink. Discard any partial output on error.
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
fn cached(value: &calc_value::CalcValue) -> Option<CachedValue> {
    if let Some(image) = value.image_value() {
        return Some(CachedValue::Image(image.clone()));
    }
    match value.core_ref() {
        CoreRef::Number(_) if value.is_excel_overflow() => {
            Some(CachedValue::Error(calc_value::WorksheetErrorCode::Num))
        }
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
/// metadata. Manual XML Save(false) retains unchanged scalar caches even when dirty, with
/// ca=1, calcCompleted=0 and calcOnSave=0. Re-entry and dirty array topology never inherit
/// such caches. Other profiles keep current calculated values only; omitted values are left
/// for Excel to calculate when it opens the package. An array formula's anchor
/// whose result stands is saved with the area its result fills as its `ref` (its spill range, a
/// legacy array's area, or the anchor alone for a single value or #SPILL!), and each cell of that
/// area beside the anchor with the value the result shows there. Rows and columns inserted or
/// deleted since the open move the package's rows, columns and what lies on them
/// ([`calc_doc::DocumentSnapshot::with_structural_edits`]); what the save does not move is
/// refused ([`SaveRefusal::StructuralNotMoved`]). The strict profile can copy a loaded revision
/// as opened; the permitted-cache profile writes its pinned cache values and flags.
pub fn prepare_save(host: &HostState, policy: &SavePolicy) -> Result<PreparedSave, SaveFailed> {
    let cache_policy = if host.calc_mode() == calc_doc::CalcMode::Manual
        && matches!(
            policy.format,
            calc_doc::SaveFormat::Xml | calc_doc::SaveFormat::Spreadsheet(_)
        ) {
        SaveCachePolicy::PreservePermittedCaches
    } else {
        SaveCachePolicy::CurrentCalculatedOnly
    };
    prepare_save_with_cache_policy(host, policy, cache_policy)
}

/// Prepare one owned pin through the same writer, choosing which scalar caches it may retain.
pub fn prepare_save_with_cache_policy(
    host: &HostState,
    policy: &SavePolicy,
    cache_policy: SaveCachePolicy,
) -> Result<PreparedSave, SaveFailed> {
    prepare_save_operation(host, policy, cache_policy, false, None)
}

pub(crate) fn effective_save_policy(host: &HostState, policy: &SavePolicy) -> SavePolicy {
    if host.sheet_view_overrides.is_empty() {
        return policy.clone();
    }
    let mut views = policy.views.as_deref().cloned().unwrap_or_else(|| calc_doc::SaveViews {
        revision: host.revision().get(),
        active_sheet: None,
        sheets: BTreeMap::new(),
    });
    for (&sheet, o) in &host.sheet_view_overrides {
        if host.model.sheet_by_id(sheet).is_none() {
            continue;
        }
        if let Some(existing) = views.sheets.get_mut(&sheet) {
            if existing.zoom.is_none() {
                existing.zoom = o.zoom;
            }
            if existing.top_left.is_none() {
                existing.top_left = o.top_left;
            }
            if existing.show_grid_lines.is_none() {
                existing.show_grid_lines = o.show_grid_lines;
            }
            if existing.show_row_col_headers.is_none() {
                existing.show_row_col_headers = o.show_row_col_headers;
            }
            if existing.show_formulas.is_none() {
                existing.show_formulas = o.show_formulas;
            }
            if existing.show_zeros.is_none() {
                existing.show_zeros = o.show_zeros;
            }
            if existing.show_outline_symbols.is_none() {
                existing.show_outline_symbols = o.show_outline_symbols;
            }
        } else if let Some(view) = host.sheet_view(sheet) {
            views.sheets.insert(sheet, view);
        }
    }
    SavePolicy {
        views: Some(Arc::new(views)),
        ..policy.clone()
    }
}

pub(crate) fn prepare_save_operation(
    host: &HostState,
    policy: &SavePolicy,
    cache_policy: SaveCachePolicy,
    calculate_before_save: bool,
    calculation: Option<calc_engine::CompletionReceipt>,
) -> Result<PreparedSave, SaveFailed> {
    let pin = SavePin {
        destination: calc_engine::contract::ResultDestination::Live,
        cache_policy,
        workbook: host.engine.workbook_instance(),
        model: host.revision(),
        results: host.engine.result_view(),
        active_calculation: host.work.as_ref().map(|work| work.admission().command),
    };
    let effective = effective_save_policy(host, policy);
    prepare_image(
        &host.loaded,
        &host.model,
        &host.engine,
        &host.axis_edits,
        host.calc_mode(),
        pin,
        &effective,
        calculation,
        calculate_before_save,
    )
}

pub(crate) fn prepare_image(
    loaded: &calc_doc::LoadedWorkbook,
    model: &Arc<calc_doc_model::ModelRoot>,
    engine: &calc_engine::EngineState,
    axis_edits: &[calc_doc_model::AxisEdit],
    mode: calc_doc::CalcMode,
    pin: SavePin,
    policy: &SavePolicy,
    calculation: Option<calc_engine::contract::CompletionReceipt>,
    calculate_before_save: bool,
) -> Result<PreparedSave, SaveFailed> {
    // The package writer preserves the embedded model bytes. Refuse to save a changed
    // snapshot until those bytes can be updated to agree with the calculated cells.
    let initial = crate::facts::initial_model(loaded.model());
    fn rows(
        root: &calc_doc_model::ModelRoot,
    ) -> Option<Vec<(&str, &[String], &[Vec<Option<CachedValue>>])>> {
        Some(
            root.definitions()
                .data_model()?
                .snapshot
                .as_ref()?
                .tables
                .iter()
                .map(|table| {
                    (
                        table.name.as_str(),
                        table.columns.as_slice(),
                        table.rows.as_slice(),
                    )
                })
                .collect(),
        )
    }
    let source_now = crate::facts::initial_model(model);
    let removed_source = model.definitions().data_model().is_some_and(|data| {
        data.tables.iter().any(|name| {
            let has_table = |root: &calc_doc_model::ModelRoot| {
                root.definitions().tables().iter().any(|table| {
                    calc_value::workbook_names_equal(&table.name, name)
                        || calc_value::workbook_names_equal(&table.display_name, name)
                })
            };
            has_table(&initial) && !has_table(model)
        })
    });
    if rows(model) != rows(&initial) || rows(model) != rows(&source_now) || removed_source {
        return Err(SaveFailed::Operation {
            owner: "dc-8wp.20",
            reason: "Saving changed data model source rows is not yet supported".into(),
        });
    }
    let cache_policy = pin.cache_policy;
    // Serialization may share proven-equal sources without changing the live authored
    // root, results or the pin's revision. An exhausted budget keeps individual formulas.
    let regrouped = calc_doc::regroup_formula_sources(model, Default::default());
    let model = regrouped.as_ref().map_or(model, |grouped| &grouped.model);
    let stale = engine
        .stale(model)
        .expect("the host keeps its authored revision and its engine state at one revision");
    let mut values = BTreeMap::new();
    let mut dirty_caches = BTreeSet::new();
    let mut spills = BTreeMap::new();
    let definitions = model.definitions();
    for sheet in model.sheets() {
        for (at, input) in model.sheet_cells(sheet.id()).into_iter().flatten() {
            if !matches!(input, CellInput::Formula(_)) {
                continue;
            }
            let dirty = stale.binary_search(&at).is_ok();
            if cache_policy == SaveCachePolicy::PreservePermittedCaches
                && definitions.array(at).is_none()
                && engine.spill(at).is_none()
            {
                if engine.permitted_cache(at).is_none()
                    && let Some(cache) = engine.refused_file_cache(at)
                {
                    values.insert(at, cache.value().clone());
                    dirty_caches.insert(at);
                    continue;
                }
                if let Some(result) = engine.permitted_cache(at) {
                    let cache = match result {
                        FormulaResult::Calculated { value, .. } => cached(value),
                        FormulaResult::FileCached(cache) => Some(cache.value().clone()),
                        FormulaResult::NotModeled { .. } => None,
                    };
                    if let Some(cache) = cache {
                        values.insert(at, cache);
                        if dirty
                            || engine.calculation_required_at(at)
                            || matches!(result, FormulaResult::FileCached(_))
                        {
                            dirty_caches.insert(at);
                        }
                        continue;
                    }
                }
            }
            if dirty {
                continue;
            }
            let Some(FormulaResult::Calculated { value, .. }) = engine.result(at) else {
                continue;
            };
            let Some(cached_value) = cached(value) else {
                continue;
            };
            values.insert(at, cached_value);
            if let Some(spill) = engine.spill(at) {
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
                        if let Some(member_value) = cached(&spill.value_at(member)) {
                            values.insert(member, member_value);
                        }
                    }
                }
            } else if definitions.array(at).is_some() {
                spills.insert(at, GridArea::cell(at));
            }
        }
    }
    // TABLE cells have graph nodes and actual calculated results, while their authored
    // definition remains in the codec rather than in CellInput::Formula.
    for node in engine.nodes() {
        let at = node.anchor();
        if definitions.data_table_containing(at).is_none() || stale.binary_search(&at).is_ok() {
            continue;
        }
        if let Some(FormulaResult::Calculated { value, calculation }) = engine.result(at)
            && !calculation.held_by_circular_rule()
            && let Some(value) = cached(value)
        {
            values.insert(at, value);
        }
    }
    // Rows or columns inserted or deleted since the open: the save moves the package's rows,
    // columns and what lies on them, as the model moved them (dc-jhn).
    let cache_complete = dirty_caches.is_empty()
        && calculation.as_ref().is_none_or(|receipt| {
            receipt.outcome == calc_engine::contract::CompletionOutcome::Complete
        })
        && !engine.needs_recalc(model)
        && pin.active_calculation.is_none()
        && definitions.unloaded_cells().all(|(at, why)| {
            matches!(why, calc_doc_model::UnloadedCell::DataTable) && values.contains_key(&at)
        })
        && model.sheets().iter().all(|sheet| {
            model
                .sheet_cells(sheet.id())
                .into_iter()
                .flatten()
                .all(|(at, input)| {
                    !matches!(input, CellInput::Formula(_)) || values.contains_key(&at)
                })
        });
    if let Some(issue) = engine.pivot_results().issues.first() {
        return Err(SaveFailed::Operation {
            owner: issue.owner,
            reason: issue.detail.clone(),
        });
    }
    let pivot_outputs = Arc::new(
        engine
            .pivot_results()
            .cells
            .iter()
            .map(|(at, (value, _))| (*at, value.clone()))
            .collect(),
    );
    let snapshot = loaded
        .snapshot_with_results(model, values, spills)
        .with_pivot_outputs(pivot_outputs)
        .with_structural_edits(StructuralEdits::new(
            axis_edits.to_vec(),
            Arc::new(calc_engine::ReferenceRewriter),
        ));
    let snapshot =
        if cache_policy == SaveCachePolicy::PreservePermittedCaches || calculation.is_some() {
            snapshot.with_calculation_save_state(
                dirty_caches,
                cache_complete,
                mode,
                calculate_before_save,
            )
        } else {
            snapshot
        };
    let plan = calc_doc::prepare_save(&snapshot, policy).map_err(SaveFailed::Refused)?;
    Ok(PreparedSave {
        pin,
        plan,
        calculation,
    })
}

/// Prepare and write one pinned package through the same route as a deferred save request.
pub fn save(
    host: &HostState,
    policy: &SavePolicy,
    out: &mut dyn PackageWrite,
) -> Result<SaveReceipt, SaveFailed> {
    prepare_save(host, policy)?.write(out)
}

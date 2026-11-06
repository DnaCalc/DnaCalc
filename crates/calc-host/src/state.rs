//! The host's state: the loaded workbook, its current authored revision and the engine state of
//! that revision, owned together.

use crate::recalc::RecalcFailed;
use calc_doc::{
    CalcMode, DateSystem, LoadError, LoadedWorkbook, OpenError, OpenPolicy, PackageRead,
};
use calc_doc_model::value::{CellKey, GridArea, IdAllocator, NumericMode, SheetId};
use calc_doc_model::{AuthoredRevision, ModelRoot, SheetEntry};
use calc_engine::{CacheWithoutFormula, EngineState, RecalcWork};
use calc_fml::{CalcEnvironment, DateSystem as FmlDateSystem, FormatProfile};
use std::fmt;
use std::sync::Arc;

/// What a host is opened with besides the package.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostOptions {
    /// The numeric mode every recalculation runs under.
    pub numeric: NumericMode,
    /// The most formula cells an edit may leave the workbook holding when it adds formula cells:
    /// an edit that adds more formula cells than it removes and leaves more than this is refused
    /// before anything commits ([`crate::CommandError::FormulaBudget`]). An edit that adds none
    /// is admitted whatever the count, and opening a workbook is bounded only by the
    /// [`OpenPolicy`] limits. `None` admits every edit the engine reserves node IDs for.
    pub formula_cell_budget: Option<usize>,
    /// The calculation mode [`crate::step`] follows; `None` follows the mode the package's
    /// `calcPr` declares ([`HostState::calc_mode`]).
    pub calc_mode: Option<CalcMode>,
    /// The host's locale profile (decimal and group separators, currency, date order), which
    /// TEXT, VALUE, FIXED, DOLLAR and number text read as a number use. `None` refuses them.
    pub locale: Option<FormatProfile>,
}

/// The calculation context a host's recalculations apply ([`HostState::recalc_context`]), which
/// is what a comparison names as its run's context. It is not what the package declares: a
/// package's `workbookPr` and `calcPr` may name other settings
/// ([`HostState::declared_date1904`]).
///
/// The numeric mode and the iteration settings vary. calc-doc reads the workbook part's
/// `calcPr` iteration settings (`iterate`, `iterateCount`, `iterateDelta`) into the model, and
/// [`calc_engine::begin_recalc`] applies them (L3, dc-m3u): with iteration on, the formulas on a
/// cycle are calculated in passes up to `max_iterations` until no value changes by `max_change`
/// or more; with it off, each formula on a cycle, and each that reads one, keeps what its cell
/// held by the circular-reference rule (dc-y2o.1, [`calc_engine::Calculation::held_by_circular_rule`]).
/// The date system is the one the
/// package's `workbookPr` declares (`date1904`), which every evaluation reads its serial dates in.
/// Whatever the package declares, every result is published at full precision whatever its
/// number format displays: precision as displayed is not applied (no bead owns it yet; the engine
/// has no number formats of results to round to).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecalcContext {
    /// Serial dates count in the 1904 date system; `false`: the 1900 date system.
    pub date1904: bool,
    /// Circular references are calculated iteratively; `false`: their formulas keep what their
    /// cells held, by the circular-reference rule.
    pub iteration: bool,
    /// The most passes an iterating recalculation runs (`calcPr`'s `iterateCount`; Excel's
    /// default, 100, when the package declares none).
    pub max_iterations: u32,
    /// The change below which an iterating recalculation stops, as binary64 bits (`calcPr`'s
    /// `iterateDelta`; Excel's default, 0.001, when the package declares none).
    pub max_change_bits: u64,
    /// Results are rounded to the precision their number format displays; `false`: full
    /// precision.
    pub precision_as_displayed: bool,
    /// The numeric mode every recalculation runs under ([`HostOptions::numeric`]).
    pub numeric: NumericMode,
}

/// Why [`HostState::open`] made no host.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenFailure {
    /// calc-doc refused the package.
    Package(OpenError),
    /// calc-doc refused the package's workbook.
    Workbook(LoadError),
    /// calc-doc's loaded records hold a cached result for a cell whose loaded input is not a
    /// formula, which the engine refuses to publish.
    Caches(CacheWithoutFormula),
}

impl fmt::Display for OpenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenFailure::Package(error) => write!(f, "the package was refused: {error}"),
            OpenFailure::Workbook(error) => write!(f, "the workbook was refused: {error}"),
            OpenFailure::Caches(error) => write!(f, "the file caches were refused: {error}"),
        }
    }
}

impl std::error::Error for OpenFailure {}

/// One open workbook: the workbook calc-doc loaded from a package (which keeps the package for a
/// later save), the current authored revision of its model, and the engine state at that
/// revision. The host is the one owner that changes either: [`crate::dispatch`] commits an edit
/// to both, [`crate::step`] drives the engine's recalculation, and [`crate::project`] reads both.
/// The authored revision and the engine state are always at one revision.
#[derive(Debug)]
pub struct HostState {
    /// Identifies this open host for revision-bound prepared commands.
    pub(crate) command_scope: u64,
    pub(crate) sheet_ids: IdAllocator<SheetId>,
    pub(crate) sheet_identities: std::collections::BTreeMap<calc_doc_model::SheetKey, SheetId>,
    /// The workbook as calc-doc loaded it. Its model is the loaded revision, never the current
    /// one, which is `model`.
    pub(crate) loaded: LoadedWorkbook,
    pub(crate) model: Arc<ModelRoot>,
    pub(crate) engine: EngineState,
    pub(crate) options: HostOptions,
    /// Regional meanings of undeclared built-ins; authored style records retain their IDs.
    pub(crate) builtin_formats: std::collections::BTreeMap<u32,String>,
    /// The recalculation in progress, between two [`crate::step`] calls.
    pub(crate) work: Option<RecalcWork>,
    pub(crate) async_records: calc_engine::AsyncReturns,
    pub(crate) last_calculation_completion: Option<calc_engine::CompletionReceipt>,
    pub(crate) pinned_calculations: std::rc::Rc<std::cell::Cell<usize>>,
    pub(crate) functions: calc_fml::CallableTable,
    pub(crate) rtd_inputs: std::rc::Rc<crate::recalc::RtdSnapshot>,
    pub(crate) retired_rtd: std::collections::BTreeSet<calc_fml::UserInputKey>,
    pub(crate) references: std::rc::Rc<calc_fml::reference::ReferenceTracker>,
    pub(crate) calculation_interest: Vec<calc_doc_model::value::GridArea>,
    pub(crate) full_calculate_requested: bool,
    pub(crate) transformed_calculation_scope: Option<calc_engine::CalcScope>,
    pub(crate) requested_calculation: Option<(calc_engine::CalcScope, calc_engine::EvalSelection)>,
    pub(crate) calculation_source: Option<calc_engine::contract::CommandId>,
    pub(crate) calculation_cancelled: bool,
    pub(crate) calc_events: Vec<calc_engine::CalcEvent>,
    /// The revision the last published generation calculated; `None` before the first.
    pub(crate) published: Option<AuthoredRevision>,
    /// Why the last recalculation published nothing, with the revision it was of, until one
    /// publishes.
    pub(crate) last_failure: Option<(AuthoredRevision, RecalcFailed)>,
    /// Whether an edit committed since the workbook was opened changed a cell's input or formula.
    /// Until one does, every file cache is the package's result for the inputs it holds.
    pub(crate) inputs_changed_since_open: bool,
    /// Whether [`crate::calculate`] asked for a recalculation that has not begun yet.
    pub(crate) calculate_requested: bool,
    /// The serial date and time NOW and TODAY read ([`HostState::set_clock`]); `None` refuses
    /// them.
    pub(crate) clock: Option<f64>,
    /// The row and column insertions and deletions that lead from the workbook as opened to the
    /// current revision, in order: what moves the sizes and formats the file gave
    /// ([`HostState::sheet_geometry`], [`HostState::number_format`]) and what a save moves the
    /// package's rows and columns by.
    pub(crate) axis_edits: Vec<calc_doc_model::AxisEdit>,
    /// The file the workbook was opened from ([`HostState::set_file`]); `None` refuses what
    /// CELL reads of it.
    pub(crate) file: Option<calc_fml::WorkbookFile>,
    /// The application's facts INFO reads ([`HostState::set_application`]).
    pub(crate) application: Option<calc_fml::ApplicationFacts>,
}

pub(crate) fn next_command_scope() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_update(
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
        |current| current.checked_add(1),
    )
    .expect("host command namespace exhausted")
}

impl HostState {
    /// Inspect only existing bindings; candidates still need context and an executed read
    /// certificate before any future value memo could reuse them.
    pub fn name_memo_candidates(
        &self,
        caller: CellKey,
    ) -> Option<Vec<calc_fml::NameMemoCandidate>> {
        self.engine
            .name_memo_candidates(&self.model, self.engine.node_at(caller)?)
    }

    pub fn plan_cache_stats(&self) -> calc_fml::PlanCacheStats {
        self.engine.plan_cache_stats()
    }

    /// Already-bound volatile formula anchors. This passive query neither binds
    /// nor calculates a formula; callers can follow `dependents` for its readers.
    pub fn volatile_formula_cells(&self) -> Vec<CellKey> {
        self.engine.nodes()
            .filter(|node| self.engine.is_volatile(node.id()))
            .map(|node| node.anchor())
            .collect()
    }

    /// A read-only source cut for another workbook's engine context. Imported and
    /// uncertified formula values remain unavailable inside the snapshot.
    pub fn linked_snapshot(&self) -> Arc<calc_engine::LinkedWorkbookSnapshot> {
        self.engine
            .linked_snapshot(self.model.clone())
            .expect("host and engine revisions agree")
    }

    pub fn set_linked_workbook(
        &mut self,
        number: u32,
        source: Option<Arc<calc_engine::LinkedWorkbookSnapshot>>,
    ) -> Result<(), &'static str> {
        if self.model.definitions().external_book(number).is_none() {
            return Err("the workbook has no external link with that number (dc-ylr)");
        }
        self.engine.set_linked_workbook(number, source);
        self.published = None;
        self.last_failure = None;
        Ok(())
    }
    /// Physical result rows; this does not claim calculation coverage or freshness.
    pub fn result_storage_stats(&self) -> calc_engine::ResultStorageStats {
        self.engine.result_storage_stats()
    }

    pub fn user_function_bindings(&self) -> calc_fml::CallableBindings {
        self.functions.bindings()
    }
    pub fn set_user_functions(&mut self, functions: calc_fml::CallableTable) {
        self.functions.replace(functions);
        self.engine
            .set_callable_bindings(&self.model, self.functions.bindings());
        self.published = None;
        self.last_failure = None;
    }

    /// Opens the package `source` under `policy`, loads its workbook at the document's first
    /// revision, and builds the engine state of that revision with every result the file cached
    /// for a formula published as a file cache. Nothing is calculated: each formula cell projects
    /// its file cache, or as stale when the file cached none, until a recalculation calculates it.
    pub fn open(
        source: Arc<dyn PackageRead>,
        policy: &OpenPolicy,
        options: HostOptions,
    ) -> Result<HostState, OpenFailure> {
        let document = calc_doc::open(source, policy).map_err(OpenFailure::Package)?;
        // The allocator issues the session IDs of this document's sheets; no sheet edit is
        // admitted yet, so no later ID is issued from it.
        let mut sheet_ids: IdAllocator<SheetId> = IdAllocator::new();
        let loaded = calc_doc::load(&document, &mut sheet_ids).map_err(OpenFailure::Workbook)?;
        let model = Arc::clone(loaded.model());
        let caches = loaded.sheets().iter().flat_map(|sheet| {
            sheet
                .formulas()
                .filter_map(move |(row, column, record)| {
                    let cache = record.cache()?;
                    let at = CellKey::new(sheet.id(), row, column)
                        .expect("calc-doc loads only cells inside the grid");
                    Some((at, cache.clone()))
                })
                .chain(sheet.value_caches().filter_map(|(row, column, cache)| {
                    let at = CellKey::new(sheet.id(), row, column)
                        .expect("a loaded cell is on the grid");
                    (model.definitions().array_containing(at).is_some()
                        && model.definitions().unloaded(at).is_none())
                    .then(|| (at, cache.clone()))
                }))
        });
        let engine = EngineState::with_file_caches(&model, caches).map_err(OpenFailure::Caches)?;
        let sheet_identities = model.sheets().iter().map(|s| (s.key(), s.id())).collect();
        let mut host = HostState {
            command_scope: next_command_scope(),
            sheet_ids,
            sheet_identities,
            loaded,
            model,
            engine,
            builtin_formats: crate::format::builtin_formats(options.locale.as_ref()),
            options,
            work: None,
            async_records: Default::default(),
            last_calculation_completion: None,
            pinned_calculations: Default::default(),
            functions: calc_fml::CallableTable::new(),
            rtd_inputs: Default::default(),
            retired_rtd: Default::default(),
            references: calc_fml::reference::ReferenceTracker::new(),
            calculation_interest: Vec::new(),
            full_calculate_requested: false,
            transformed_calculation_scope: None,
            requested_calculation: None,
            calculation_source: None,
            calculation_cancelled: false,
            calc_events: Vec::new(),
            published: None,
            last_failure: None,
            inputs_changed_since_open: false,
            calculate_requested: false,
            clock: None,
            axis_edits: Vec::new(),
            file: None,
            application: None,
        };
        // The package's date system, the options' locale profile, and no clock until one is set:
        // NOW, TODAY and a date read without a year are refused.
        let environment = host.entry_environment();
        host.engine.set_environment(Some(environment));
        Ok(host)
    }

    /// The current authored revision.
    pub fn revision(&self) -> AuthoredRevision {
        self.model.revision()
    }

    /// The current revision's model (dc-1yp: what a document-fidelity run compares).
    pub fn model(&self) -> &Arc<ModelRoot> {
        &self.model
    }

    /// The workbook as calc-doc loaded it: the loaded revision's model and the records it was
    /// loaded with (each sheet's styles and geometry as the file states them).
    pub fn styles(&self) -> &Arc<calc_doc_model::StyleTable> {
        self.model.definitions().styles()
    }

    pub fn style_color(&self, color: &calc_doc_model::StyleColor) -> Option<crate::LookColor> {
        crate::conditional::resolve(self, color)
    }

    /// Resolve indexed colours once for display and colour filtering.
    pub fn indexed_color(&self, index:u32)->Option<String> {
        const BASIC:[&str;8]=["#000000","#ffffff","#ff0000","#00ff00","#0000ff","#ffff00","#ff00ff","#00ffff"];
        match index {
            0..=7=>Some(BASIC[index as usize].into()),
            8..=63=>self.palette_color((index-7) as u8).or_else(||Some(calc_func::DEFAULT_PALETTE[(index-8) as usize].into())),
            64=>Some("#000000".into()),65=>Some("#ffffff".into()),_=>None,
        }
    }
    pub fn resolved_style_color(&self,color:&calc_doc_model::StyleColor)->Option<String> {
        match self.style_color(color)? {crate::LookColor::Rgb(rgb)=>Some(rgb),crate::LookColor::Indexed(index)=>self.indexed_color(index)}
    }

    pub fn loaded(&self) -> &LoadedWorkbook {
        &self.loaded
    }

    /// The current revision's sheets, in workbook order: each one's ID, which the cell keys of
    /// commands and projections carry, and its name.
    pub fn sheets(&self) -> &[SheetEntry] {
        self.model.sheets()
    }

    /// The formula cells the last published recalculation found on a circular reference, in
    /// cell order (dc-y2o.1, [`calc_engine::EngineState::circular_cells`]): what an application
    /// warns of, as Excel's status bar does. Empty when there is none.
    pub fn circular_cells(&self) -> Vec<CellKey> {
        self.engine.circular_cells()
    }

    /// The environment a cell entry is read in ([`crate::interpret_entry_in`]) and every
    /// evaluation runs in: the package's date system, the options' locale profile and the clock
    /// set ([`HostState::set_clock`]), none at first.
    pub fn entry_environment(&self) -> CalcEnvironment {
        CalcEnvironment {
            compatibility_version: self.loaded.compatibility_version(),
            date_system: if self.declared_date1904() {
                FmlDateSystem::System1904
            } else {
                FmlDateSystem::System1900
            },
            profile: self.options.locale,
            now: self.clock,
        }
    }

    /// Sets the clock NOW and TODAY read, as a 1900-system serial date and time. The evaluator
    /// converts it to the workbook's declared date system. The application supplies the clock;
    /// calc-host reads no clock of its own. `None` refuses them again. Every recalculation calculates the formulas that call a
    /// volatile function, so the next one reads the clock set here.
    pub fn set_clock(&mut self, now: Option<f64>) {
        self.clock = now;
        let environment = self.entry_environment();
        self.engine.set_environment(Some(environment));
    }

    /// Whether a worker should schedule another finite task. A fault waits for an explicit
    /// request or edit; it is never retried in a busy loop.
    pub fn has_pending_calculation(&self) -> bool {
        if self.pinned_calculations.get() != 0 {
            return false;
        }
        if let Some(work) = &self.work {
            return work.revision() != self.model.revision()
                || !work.live_inputs_current(&self.engine)
                || !work.callables_current(&self.engine)
                || !work.is_waiting_external();
        }
        self.calculate_requested
            || !self.calculation_cancelled
                && self.calc_mode() != CalcMode::Manual
                && self.last_failure.is_none()
                && self.engine.needs_recalc(&self.model)
    }
    pub fn take_calculation_events(&mut self) -> Vec<calc_engine::CalcEvent> {
        std::mem::take(&mut self.calc_events)
    }
    pub(crate) fn scoped_calculation_active(&self) -> bool {
        self.work
            .as_ref()
            .is_some_and(|work| !matches!(work.admission().scope, calc_engine::CalcScope::Workbook))
    }

    pub fn options(&self) -> &HostOptions {
        &self.options
    }

    /// How many threads a recalculation of this workbook may evaluate formulas on (dc-2m0; see
    /// [`calc_engine::EngineState::set_threads`]): at least 1, which is the default. A
    /// recalculation that several threads evaluate publishes the values one thread would.
    pub fn set_threads(&mut self, threads: usize) {
        self.engine.set_threads(threads);
    }

    pub fn threads(&self) -> usize {
        self.engine.threads()
    }

    /// See [`calc_engine::EngineState::set_parallel_minimum`].
    pub fn set_parallel_minimum(&mut self, wave: usize, level: usize) {
        self.engine.set_parallel_minimum(wave, level);
    }

    /// The calculation mode [`crate::step`] follows: the host's option, or else the mode the
    /// package's `calcPr` declares (automatic when it declares none). Under
    /// [`CalcMode::Manual`] a step begins a recalculation only after [`crate::calculate`] asks for
    /// one. [`CalcMode::AutomaticExceptTables`] recalculates as [`CalcMode::Automatic`] does,
    /// except that the cells of what-if data tables, and the formulas that read only through
    /// them, wait for a recalculation asked for (L3, dc-m3u).
    pub fn calc_mode(&self) -> CalcMode {
        self.options
            .calc_mode
            .unwrap_or_else(|| self.loaded.calc_mode())
    }

    /// The calculation context every recalculation of this host applies: the date system the
    /// package declares, full precision whatever the package declares, the iteration settings the
    /// current revision holds, under the numeric mode the host was opened with
    /// ([`RecalcContext`] says why).
    pub fn recalc_context(&self) -> RecalcContext {
        let iteration = self.model.definitions().iteration();
        RecalcContext {
            date1904: self.declared_date1904(),
            iteration: iteration.enabled(),
            max_iterations: iteration.max_iterations(),
            max_change_bits: iteration.max_change().to_bits(),
            precision_as_displayed: false,
            numeric: self.options.numeric.clone(),
        }
    }

    /// The smallest area of `sheet` that holds every cell a projection of the sheet can show as
    /// not empty: each cell with an authored input, each cell of an array formula's area, each
    /// cell a published spill fills, and each unloaded cell (a rich value, a data table's cell).
    /// `None` when the sheet holds none of them, or the current revision holds no such sheet.
    ///
    /// A skin bounds its scrolling and its first view of a sheet by it (dc-rif). It reads every
    /// occupied cell of the sheet once, so a skin keeps it per revision rather than per frame.
    pub fn used_area(&self, sheet: SheetId) -> Option<GridArea> {
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        let mut take = |top: u32, left: u32, bottom: u32, right: u32| {
            bounds = Some(match bounds {
                None => (top, left, bottom, right),
                Some((t, l, b, r)) => (t.min(top), l.min(left), b.max(bottom), r.max(right)),
            });
        };
        for page in self.model.pages(sheet)? {
            for (row, column, _) in page.iter() {
                take(row, column, row, column);
            }
        }
        let definitions = self.model.definitions();
        let full = GridArea::full_sheet(sheet);
        for array in definitions.arrays_meeting(full) {
            let area = array.area();
            take(
                area.first().row(),
                area.first().column(),
                area.last().row(),
                area.last().column(),
            );
        }
        for (at, _) in definitions.unloaded_in(full) {
            take(at.row(), at.column(), at.row(), at.column());
        }
        for spill in self
            .engine
            .spills()
            .filter(|spill| spill.anchor().sheet() == sheet)
        {
            let area = spill.area();
            take(
                area.first().row(),
                area.first().column(),
                area.last().row(),
                area.last().column(),
            );
        }
        let (top, left, bottom, right) = bounds?;
        let first = CellKey::new(sheet, top, left).expect("an occupied cell is inside the grid");
        let last = CellKey::new(sheet, bottom, right).expect("an occupied cell is inside the grid");
        Some(GridArea::new(first, last).expect("both corners are on the sheet"))
    }

    /// Whether `at` is a formula cell as Excel counts them (`Range.HasFormula`), which is the cell
    /// set a comparison's inventory holds: a cell whose input is a formula, every cell of a legacy
    /// (Ctrl+Shift+Enter) array formula's area, and every cell of a what-if data table's results.
    /// The cells a dynamic array spills into beside its anchor are not formula cells (dc-rif).
    /// The area the published result of the array formula anchored at `anchor` fills: its spill
    /// range, a legacy array's area, or the anchor alone; `None` when no array result is
    /// published there (dc-677: what a comparison compares a dynamic array's anchor by).
    /// The actual published array owner of a current calculated cell.
    pub fn spill_owner(&self, at: CellKey) -> Option<CellKey> {
        let owner = self.engine.spill_covering(at)?.anchor();
        let page = crate::project(self, &crate::ProjectionRequest { cells: vec![at] }).ok()?;
        matches!(
            page.cells().first()?.value(),
            crate::ProjectedValue::Calculated { .. }
        )
        .then_some(owner)
    }

    /// A blocked extent is available only while the projected #SPILL! is current.
    pub fn blocked_spill_area(&self, at: CellKey) -> Option<GridArea> {
        let page = crate::project(self, &crate::ProjectionRequest { cells: vec![at] }).ok()?;
        match page.cells().first()?.value() {
            crate::ProjectedValue::Calculated { value, .. }
                if matches!(
                    value.core(),
                    crate::value::CoreValue::Error(crate::value::WorksheetErrorCode::Spill)
                ) =>
            {
                self.engine.blocked_spill_area(at)
            }
            _ => None,
        }
    }

    /// Bounded physical blockers in a current attempted extent; no file-cache inference.
    pub fn spill_blockers(&self, at: CellKey, limit: usize) -> (Vec<GridArea>, bool) {
        let Some(extent) = self.blocked_spill_area(at) else {
            return (Vec::new(), false);
        };
        let mut areas = Vec::new();
        for (cell, _) in self.model.cells(extent).into_iter().flatten() {
            if cell != at {
                areas.push(GridArea::cell(cell));
                if areas.len() > limit {
                    areas.truncate(limit);
                    return (areas, true);
                }
            }
        }
        for (cell, _) in self.model.definitions().unloaded_in(extent) {
            let area = GridArea::cell(cell);
            if cell != at && !areas.contains(&area) {
                areas.push(area);
                if areas.len() > limit {
                    areas.truncate(limit);
                    return (areas, true);
                }
            }
        }
        for spill in self
            .engine
            .spills()
            .filter(|s| s.anchor() != at && s.anchor().sheet() == at.sheet())
        {
            let area = spill.area();
            let top = extent.first().row().max(area.first().row());
            let left = extent.first().column().max(area.first().column());
            let bottom = extent.last().row().min(area.last().row());
            let right = extent.last().column().min(area.last().column());
            if top <= bottom && left <= right {
                let area = GridArea::new(
                    CellKey::new(at.sheet(), top, left).unwrap(),
                    CellKey::new(at.sheet(), bottom, right).unwrap(),
                )
                .unwrap();
                if !areas.contains(&area) {
                    areas.push(area);
                }
            }
            if areas.len() > limit {
                areas.truncate(limit);
                return (areas, true);
            }
        }
        (areas, false)
    }

    pub fn spill_area(&self, anchor: CellKey) -> Option<GridArea> {
        self.engine.spill(anchor).map(|spill| spill.area())
    }

    /// What the formula in the cell `at` reads (dc-gf0, a skin's dependency trace): each placed
    /// reference and each area its last evaluation computed (INDIRECT's, OFFSET's, a spill's),
    /// as areas in the order the engine holds them, each once. Whole rows or columns are the
    /// areas of those rows or columns; a reference over several sheets (`Jan:Mar!A1`) is its area
    /// on its first sheet. `None` when the cell holds no formula, or holds one the engine has not
    /// bound yet (a workbook opened and not calculated).
    pub fn precedents(&self, at: CellKey) -> Option<Vec<GridArea>> {
        let node = self.engine.node(self.engine.node_at(at)?)?;
        let placed = node.prerequisites()?;
        let mut areas: Vec<GridArea> = Vec::new();
        for prerequisite in placed.iter().chain(node.dynamic_prerequisites()) {
            let area = match *prerequisite {
                calc_engine::Prerequisite::Name(_) => continue,
                calc_engine::Prerequisite::Cell(cell) => GridArea::cell(cell),
                calc_engine::Prerequisite::Area(area) => area,
                calc_engine::Prerequisite::AxisValue(interval) => {
                    let (first, last) = match interval.axis() {
                        calc_doc_model::Axis::Rows => (
                            CellKey::new(interval.sheet(), interval.first(), 1),
                            CellKey::new(
                                interval.sheet(),
                                interval.last(),
                                calc_doc_model::value::MAX_COLUMNS,
                            ),
                        ),
                        calc_doc_model::Axis::Columns => (
                            CellKey::new(interval.sheet(), 1, interval.first()),
                            CellKey::new(
                                interval.sheet(),
                                calc_doc_model::value::MAX_ROWS,
                                interval.last(),
                            ),
                        ),
                    };
                    match (first, last) {
                        (Ok(first), Ok(last)) => match GridArea::new(first, last) {
                            Ok(area) => area,
                            Err(_) => continue,
                        },
                        _ => continue,
                    }
                }
                calc_engine::Prerequisite::SheetSpan(span) => span.target(),
            };
            if !areas.contains(&area) {
                areas.push(area);
            }
        }
        Some(areas)
    }

    /// The cells whose formulas read the cell `at` (dc-gf0): those the engine's graph finds, in
    /// cell order. A formula not bound yet is not among them.
    pub fn dependents(&self, at: CellKey) -> Vec<CellKey> {
        self.engine.dependents_at(&self.model, at)
    }

    /// The cells of `sheet` that hold an input, in cell order, read from the document without
    /// projecting them (dc-gf0: the tree's outline of a sheet, cheap enough to read again after
    /// every change); empty for a sheet the workbook does not hold. A dynamic array's spilled
    /// cells hold no input and are not among them.
    pub fn input_cells(&self, sheet: SheetId) -> Vec<CellKey> {
        self.model
            .sheet_cells(sheet)
            .into_iter()
            .flatten()
            .map(|(at, _)| at)
            .collect()
    }

    /// Sparse occupied rectangles for navigation, including published spills and unloaded cells.
    /// Formula text occupies its cell even when its calculated result is an empty string.
    pub fn occupied_areas(&self, sheet: SheetId) -> Vec<GridArea> {
        let mut areas: Vec<_> = self
            .input_cells(sheet)
            .into_iter()
            .map(GridArea::cell)
            .collect();
        let definitions = self.model.definitions();
        let full = GridArea::full_sheet(sheet);
        areas.extend(definitions.arrays_meeting(full).map(|array| array.area()));
        areas.extend(
            definitions
                .data_tables()
                .iter()
                .map(|table| table.area())
                .filter(|area| area.sheet() == sheet),
        );
        areas.extend(
            definitions
                .unloaded_in(full)
                .map(|(at, _)| GridArea::cell(at)),
        );
        areas.extend(
            self.engine
                .spills()
                .filter(|spill| spill.anchor().sheet() == sheet)
                .map(|spill| spill.area()),
        );
        areas
    }

    /// The cells of `sheet` that hold a formula as [`HostState::holds_formula`] counts them (a
    /// formula input, a legacy array's cells, a data table's cells), in cell order, read without
    /// projecting them (dc-gf0: the ledger's rows).
    pub fn formula_cells(&self, sheet: SheetId) -> Vec<CellKey> {
        let inputs: Vec<CellKey> = self
            .model
            .sheet_cells(sheet)
            .into_iter()
            .flatten()
            .filter(|(_, input)| matches!(input, calc_doc_model::CellInput::Formula(_)))
            .map(|(at, _)| at)
            .collect();
        let definitions = self.model.definitions();
        let legacy = definitions
            .arrays()
            .filter(|array| array.kind() == calc_doc_model::ArrayKind::Legacy)
            .map(|array| array.area());
        let tables = definitions.data_tables().iter().map(|table| table.area());
        let filled: Vec<GridArea> = legacy
            .chain(tables)
            .filter(|area| area.sheet() == sheet && area.cell_count() > 1)
            .collect();
        if filled.is_empty() {
            return inputs;
        }
        let mut cells: std::collections::BTreeSet<CellKey> = inputs.into_iter().collect();
        for area in filled {
            for row in area.first().row()..=area.last().row() {
                for column in area.first().column()..=area.last().column() {
                    if let Ok(at) = CellKey::new(sheet, row, column) {
                        cells.insert(at);
                    }
                }
            }
        }
        cells.into_iter().collect()
    }

    pub fn holds_formula(&self, at: CellKey) -> bool {
        if matches!(
            self.model.cell(at),
            Some(calc_doc_model::CellInput::Formula(_))
        ) {
            return true;
        }
        let definitions = self.model.definitions();
        definitions
            .array_containing(at)
            .is_some_and(|array| array.kind() == calc_doc_model::ArrayKind::Legacy)
            || definitions.data_table_containing(at).is_some()
    }

    /// The workbook's defined names at the current revision, their formulas as the file form
    /// holds them (a structural edit adjusts their references).
    pub fn defined_names(&self) -> &[calc_doc_model::DefinedName] {
        self.model.definitions().names()
    }

    /// Address, range and name lookup through the existing engine snapshot catalogue.
    pub fn resolve_reference_text(
        &self,
        at: CellKey,
        text: &str,
    ) -> calc_fml::reference::TextReference {
        calc_engine::resolve_reference_text(&self.model, at, text)
    }

    /// Current physical row heights, column widths and hidden flags from the authored model,
    /// with the worksheet's imported defaults. Structural edits move these same dimensions;
    /// `None` for a sheet the file did not hold.
    pub fn sheet_geometry(&self, sheet: SheetId) -> Option<calc_doc::SheetGeometry> {
        self.model.sheet_by_id(sheet)?;
        Some(
            self.loaded
                .sheet(sheet)
                .map(|s| s.geometry().clone())
                .unwrap_or_default()
                .authored(sheet, &self.model().definitions().visible_axis_layouts()),
        )
    }

    /// Imported merged ranges transformed through the current structural axis history.
    pub fn merged_areas(&self, sheet: SheetId) -> Vec<GridArea> {
        self.loaded
            .sheet(sheet)
            .into_iter()
            .flat_map(|s| s.merged_areas())
            .filter_map(|area| {
                self.axis_edits
                    .iter()
                    .try_fold(*area, |area, edit| calc_doc_model::moved_area(*edit, area))
            })
            .collect()
    }

    /// The number format code of the cell at `at`: the code of the cell format the file gave the
    /// cell when the workbook was opened (`General` when it gave none), or `None` when the file
    /// names a format the styles part neither declares nor builds in (dc-09v). A cell an edit
    /// sets keeps the format of its place; a cell moved by an inserted or deleted row or column
    /// keeps its own, and an inserted one takes the format of the cell before the inserted band
    /// ([`HostState::opened_position`]); a cell format an edit gave the cell
    /// ([`crate::CellEdit::SetFormat`], a copy's) comes first.
    pub fn number_format(&self, at: CellKey) -> Option<&str> {
        self.style_number_format(self.styles(),self.cell_style(at))
    }

    /// The same format resolution for an uncommitted Format Cells preview table.
    pub fn style_number_format<'a>(&'a self, table:&'a calc_doc_model::StyleTable, style:u32)->Option<&'a str> {
        table.number_format_code_with(style,|id|self.builtin_formats.get(&id).map(String::as_str))
    }

    /// The cell format (an index into the workbook's cell formats, a cell's `s`) of the cell at
    /// `at`: the one an edit gave it, or else the one the file gave its place
    /// ([`HostState::number_format`]).
    pub fn cell_style(&self, at: CellKey) -> u32 {
        if let Some(format) = self.model.definitions().cell_format(at) {
            return format;
        }
        if let Some(format) = self.styles().runs.get(at) {
            return format;
        }
        crate::facts::imported_style(self.loaded.sheet(at.sheet()), &self.axis_edits, at)
    }

    /// The effective workbook alignment, through the same resolver CELL("prefix") uses.
    /// Absent horizontal/vertical values mean General/Bottom; the stored record also carries
    /// wrapping, rotation and indentation for consumers of that same alignment.
    pub fn cell_alignment(&self, at: CellKey) -> Option<&calc_doc_model::Alignment> {
        crate::facts::alignment(self.styles(), self.cell_style(at))
    }

    /// The RGB (`#rrggbb`) of entry `index` (1 to 56) of the workbook's color palette when the file
    /// redefines it (`<indexedColors>`, whose entry `index + 7` it is); `None` when it does not,
    /// and the default palette applies (dc-09v).
    pub fn palette_color(&self, index: u8) -> Option<String> {
        let entry = self.styles().colors.indexed.get(usize::from(index) + 7)?;
        let argb = entry.rgb.as_deref()?;
        let rgb = argb.get(argb.len().checked_sub(6)?..)?;
        rgb.chars()
            .all(|c| c.is_ascii_hexdigit())
            .then(|| format!("#{}", rgb.to_ascii_lowercase()))
    }

    /// Whether the package's workbook part declares the 1904 date system (`workbookPr`'s
    /// `date1904`), as calc-doc loaded it; no command changes it. Every recalculation applies it
    /// ([`HostState::recalc_context`]). calc-doc reads `calcPr`'s iteration settings, which the
    /// host applies, and not its precision as displayed.
    pub fn declared_date1904(&self) -> bool {
        match self.loaded.date_system() {
            DateSystem::Date1900 => false,
            DateSystem::Date1904 => true,
        }
    }
}

impl HostState {
    /// Review content follows physical axis edits, without inheriting notes on inserted cells.
    /// The current first window's imported origins. Authored freeze commands use leading bands.
    pub fn sheet_view(&self, sheet: SheetId) -> Option<calc_doc::SheetView> {
        self.model.sheet_by_id(sheet)?;
        let mut view = self.loaded.sheet(sheet).map(|s|s.view()).unwrap_or_default();
        for edit in self.axis_edits.iter().filter(|e| e.sheet() == sheet) {
            let move_at = |(row, col): (u32, u32)| {
                let old = if edit.axis() == calc_doc_model::Axis::Rows {
                    row
                } else {
                    col
                };
                let next = calc_doc_model::moved_position(*edit, old).unwrap_or_else(|| {
                    if edit.change() == calc_doc_model::AxisChange::Delete {
                        edit.first().min(edit.axis().size())
                    } else {
                        edit.axis().size()
                    }
                });
                if edit.axis() == calc_doc_model::Axis::Rows {
                    (next, col)
                } else {
                    (row, next)
                }
            };
            view.top_left = view.top_left.map(move_at);
            view.pane_top_left = view.pane_top_left.map(move_at);
            view.active_cell = view.active_cell.map(move_at);
        }
        if self.model.definitions().pane_layout(sheet) != self.loaded.model().definitions().pane_layout(sheet) {
            view=view.with_pane_layout(self.model.definitions().pane_layout(sheet));
        }
        Some(view)
    }

    pub fn sheet_review(&self, sheet: SheetId) -> calc_doc::SheetReview {
        let mut review = self
            .loaded
            .sheet(sheet)
            .map(|s| s.review().clone())
            .unwrap_or_default();
        review.notes = self.model.definitions().review_notes(sheet).to_vec();
        review.links = self.model.definitions().review_links(sheet).to_vec();
        review
    }
}

impl Drop for HostState {
    fn drop(&mut self) {
        self.references.close();
    }
}

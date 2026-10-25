//! The host's state: the loaded workbook, its current authored revision and the engine state of
//! that revision, owned together.

use crate::recalc::RecalcFailed;
use calc_doc::{
    CalcMode, DateSystem, LoadError, LoadedWorkbook, OpenError, OpenPolicy, PackageRead,
};
use calc_doc_model::value::{CellKey, IdAllocator, NumericMode, SheetId};
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
/// or more; with it off, each formula on a cycle is refused. Whatever the package declares, no
/// recalculation follows a 1904 date system, and every result is published at full precision
/// whatever its number format displays. `date1904 = false` follows from the engine taking no date
/// system; no calculation observes it until a function that reads or returns a serial date is
/// calculated.
///
/// dc-35p carries the workbook's date system and precision as displayed into the engine;
/// `date1904` and `precision_as_displayed` then come from the engine's settings and change with
/// the workbook.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecalcContext {
    /// Serial dates count in the 1904 date system; `false`: the 1900 date system.
    pub date1904: bool,
    /// Circular references are calculated iteratively; `false`: they are refused.
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
    /// The workbook as calc-doc loaded it. Its model is the loaded revision, never the current
    /// one, which is `model`.
    pub(crate) loaded: LoadedWorkbook,
    pub(crate) model: Arc<ModelRoot>,
    pub(crate) engine: EngineState,
    pub(crate) options: HostOptions,
    /// The recalculation in progress, between two [`crate::step`] calls.
    pub(crate) work: Option<RecalcWork>,
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
}

impl HostState {
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
            sheet.formulas().filter_map(move |(row, column, record)| {
                let cache = record.cache()?;
                let at = CellKey::new(sheet.id(), row, column)
                    .expect("calc-doc loads only cells inside the grid");
                Some((at, cache.clone()))
            })
        });
        let mut engine =
            EngineState::with_file_caches(&model, caches).map_err(OpenFailure::Caches)?;
        // The 1900 date system whatever the package declares ([`RecalcContext`]), the options'
        // locale profile, and no clock: NOW, TODAY and a date read without a year are refused.
        engine.set_environment(Some(CalcEnvironment {
            date_system: FmlDateSystem::System1900,
            profile: options.locale,
            now: None,
        }));
        Ok(HostState {
            loaded,
            model,
            engine,
            options,
            work: None,
            published: None,
            last_failure: None,
            inputs_changed_since_open: false,
            calculate_requested: false,
        })
    }

    /// The current authored revision.
    pub fn revision(&self) -> AuthoredRevision {
        self.model.revision()
    }

    /// The current revision's sheets, in workbook order: each one's ID, which the cell keys of
    /// commands and projections carry, and its name.
    pub fn sheets(&self) -> &[SheetEntry] {
        self.model.sheets()
    }

    /// The environment a cell entry is read in ([`crate::interpret_entry_in`]): the 1900 date
    /// system, the options' locale profile and no clock, as the recalculations' (dc-10y).
    pub fn entry_environment(&self) -> CalcEnvironment {
        CalcEnvironment {
            date_system: FmlDateSystem::System1900,
            profile: self.options.locale,
            now: None,
        }
    }

    pub fn options(&self) -> &HostOptions {
        &self.options
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

    /// The calculation context every recalculation of this host applies: the 1900 date system
    /// and full precision, whatever the package declares, the iteration settings the current
    /// revision holds, under the numeric mode the host was opened with ([`RecalcContext`] says
    /// why, and that dc-35p changes the others).
    pub fn recalc_context(&self) -> RecalcContext {
        let iteration = self.model.definitions().iteration();
        RecalcContext {
            date1904: false,
            iteration: iteration.enabled(),
            max_iterations: iteration.max_iterations(),
            max_change_bits: iteration.max_change().to_bits(),
            precision_as_displayed: false,
            numeric: self.options.numeric.clone(),
        }
    }

    /// Whether the package's workbook part declares the 1904 date system (`workbookPr`'s
    /// `date1904`), as calc-doc loaded it; no command changes it. Recalculation does not apply it
    /// ([`HostState::recalc_context`]). calc-doc reads `calcPr`'s iteration settings, which the
    /// host applies, and not its precision as displayed.
    pub fn declared_date1904(&self) -> bool {
        match self.loaded.date_system() {
            DateSystem::Date1900 => false,
            DateSystem::Date1904 => true,
        }
    }
}

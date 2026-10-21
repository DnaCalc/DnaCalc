//! The host's state: the loaded workbook, its current authored revision and the engine state of
//! that revision, owned together.

use crate::recalc::RecalcFailed;
use calc_doc::{LoadError, LoadedWorkbook, OpenError, OpenPolicy, PackageRead};
use calc_doc_model::value::{CellKey, IdAllocator, NumericMode, SheetId};
use calc_doc_model::{AuthoredRevision, ModelRoot, SheetEntry};
use calc_engine::{CacheWithoutFormula, EngineState, RecalcWork};
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
        let engine = EngineState::with_file_caches(&model, caches).map_err(OpenFailure::Caches)?;
        Ok(HostState {
            loaded,
            model,
            engine,
            options,
            work: None,
            published: None,
            last_failure: None,
            inputs_changed_since_open: false,
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

    pub fn options(&self) -> &HostOptions {
        &self.options
    }
}

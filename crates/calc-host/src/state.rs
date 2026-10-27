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
/// or more; with it off, each formula on a cycle is refused. The date system is the one the
/// package's `workbookPr` declares (`date1904`), which every evaluation reads its serial dates in.
/// Whatever the package declares, every result is published at full precision whatever its
/// number format displays: precision as displayed is not applied (no bead owns it yet; the engine
/// has no number formats of results to round to).
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
    /// The serial date and time NOW and TODAY read ([`HostState::set_clock`]); `None` refuses
    /// them.
    pub(crate) clock: Option<f64>,
    /// The row and column insertions and deletions that lead from the workbook as opened to the
    /// current revision, in order: what moves the sizes and formats the file gave
    /// ([`HostState::sheet_geometry`], [`HostState::number_format`]) and what a save moves the
    /// package's rows and columns by.
    pub(crate) axis_edits: Vec<calc_doc_model::AxisEdit>,
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
        let mut host = HostState {
            loaded,
            model,
            engine,
            options,
            work: None,
            published: None,
            last_failure: None,
            inputs_changed_since_open: false,
            calculate_requested: false,
            clock: None,
            axis_edits: Vec::new(),
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

    /// The current revision's sheets, in workbook order: each one's ID, which the cell keys of
    /// commands and projections carry, and its name.
    pub fn sheets(&self) -> &[SheetEntry] {
        self.model.sheets()
    }

    /// The environment a cell entry is read in ([`crate::interpret_entry_in`]) and every
    /// evaluation runs in: the package's date system, the options' locale profile and the clock
    /// set ([`HostState::set_clock`]), none at first.
    pub fn entry_environment(&self) -> CalcEnvironment {
        CalcEnvironment {
            date_system: if self.declared_date1904() {
                FmlDateSystem::System1904
            } else {
                FmlDateSystem::System1900
            },
            profile: self.options.locale,
            now: self.clock,
        }
    }

    /// Sets the clock NOW and TODAY read, as a serial date and time in the workbook's date
    /// system, which the application reads from its platform (calc-host reads no clock of its
    /// own); `None` refuses them again. Every recalculation calculates the formulas that call a
    /// volatile function, so the next one reads the clock set here.
    pub fn set_clock(&mut self, now: Option<f64>) {
        self.clock = now;
        let environment = self.entry_environment();
        self.engine.set_environment(Some(environment));
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
    pub fn spill_area(&self, anchor: CellKey) -> Option<GridArea> {
        self.engine.spill(anchor).map(|spill| spill.area())
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

    /// The row heights, column widths and hidden rows and columns of `sheet`: as the file stated
    /// them when the workbook was opened (dc-09v), moved by the rows and columns inserted and
    /// deleted since ([`calc_doc::SheetGeometry::moved`]); `None` for a sheet the file did not
    /// hold. No command sets a size yet.
    pub fn sheet_geometry(&self, sheet: SheetId) -> Option<calc_doc::SheetGeometry> {
        let mut geometry = self.loaded.sheet(sheet)?.geometry().clone();
        for edit in self.axis_edits.iter().filter(|edit| edit.sheet() == sheet) {
            geometry = geometry.moved(*edit);
        }
        Some(geometry)
    }

    /// Where the cell at `at` was when the workbook was opened, through the rows and columns
    /// inserted and deleted since: an inserted cell's source is the cell before the inserted band
    /// ([`calc_doc_model::source_position`]); `None` for one inserted at the start of an axis.
    pub(crate) fn opened_position(&self, at: CellKey) -> Option<(u32, u32)> {
        let (mut row, mut column) = (at.row(), at.column());
        for edit in self
            .axis_edits
            .iter()
            .rev()
            .filter(|edit| edit.sheet() == at.sheet())
        {
            match edit.axis() {
                calc_doc_model::Axis::Rows => row = calc_doc_model::source_position(*edit, row)?,
                calc_doc_model::Axis::Columns => {
                    column = calc_doc_model::source_position(*edit, column)?
                }
            }
        }
        Some((row, column))
    }

    /// The number format code of the cell at `at`: the code of the cell format the file gave the
    /// cell when the workbook was opened (`General` when it gave none), or `None` when the file
    /// names a format the styles part neither declares nor builds in (dc-09v). A cell an edit
    /// sets keeps the format of its place; a cell moved by an inserted or deleted row or column
    /// keeps its own, and an inserted one takes the format of the cell before the inserted band
    /// ([`HostState::opened_position`]); no command sets a format yet.
    pub fn number_format(&self, at: CellKey) -> Option<&str> {
        let style = match (self.loaded.sheet(at.sheet()), self.opened_position(at)) {
            (Some(sheet), Some((row, column))) => sheet.style(row, column),
            _ => 0,
        };
        self.loaded.styles().number_format_code(style)
    }

    /// The RGB (`#rrggbb`) of entry `index` (1 to 56) of the workbook's color palette when the file
    /// redefines it (`<indexedColors>`, whose entry `index + 7` it is); `None` when it does not,
    /// and the default palette applies (dc-09v).
    pub fn palette_color(&self, index: u8) -> Option<String> {
        let entry = self
            .loaded
            .styles()
            .colors
            .indexed
            .get(usize::from(index) + 7)?;
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

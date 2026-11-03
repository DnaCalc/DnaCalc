//! The package for CalcHost: the authoritative authored workbook state, transactions, commands and the skin contract.
//!
//! Every caller, a skin, a headless tool or future automation, changes and reads a workbook
//! through the same four calls on one [`HostState`]:
//!
//! - [`HostState::open`] opens a package ([`calc_doc::open`]), loads its workbook
//!   ([`calc_doc::load`]) and builds the [`calc_engine::EngineState`] of the loaded revision, with
//!   the results the file cached for its formulas published as file caches
//!   ([`calc_engine::EngineState::with_file_caches`]). The host is the one owner that changes the
//!   authored revision or the engine state.
//! - [`dispatch`] translates a [`HostCommand`] (cell edits on existing sheets) into one
//!   [`calc_doc_model::OpBatch`] based on the revision the command names, refusing a command with
//!   no edit and a command based on another revision than the current one. It prepares the document's share of the batch
//!   ([`calc_doc::prepare_edit`]) and then the engine's ([`calc_engine::prepare_edit`], which
//!   reserves the IDs of the formula nodes it adds) against that one base, admits the reserved
//!   nodes against the host's formula-cell budget, reserves the receipt it will fill, and only
//!   then commits both: [`calc_engine::commit_edit`], which returns no error, and the adoption of
//!   the prepared revision. A refusal at any step before the commit drops both preparations, so
//!   the authored revision, the engine state and every cell stay as they were. A commit publishes
//!   one authored revision and its dirty set ([`CommandReceipt`]).
//! - [`step`] drives the engine's recalculation ([`calc_engine::begin_recalc`],
//!   [`calc_engine::step`], [`calc_engine::finish`]) a budget at a time. A recalculation is not
//!   part of an edit's transaction: when it fails or is refused, the committed edit stays, nothing
//!   is published, and the calculated results and refusals it would have replaced are projected
//!   as stale; a file cache stays a file cache while no edit since open has changed an input or
//!   a formula.
//! - [`project`] reads requested cells: each one's authored input and its value with the value's
//!   provenance ([`ProjectedValue`]): an authored literal, a file cache (never counted as
//!   calculated, and shown only while no edit since the workbook was opened has changed an input
//!   or a formula), a value the engine calculated that still stands, a typed refusal, or a stale
//!   value.
//!
//! [`HostSession`] is what the applications build on: the session a skin (grid, notebook, tree,
//! canvas), a headless tool or a test drives with [`SkinIntent`]s (cell entries read as Excel
//! reads them, multi-cell edits, undo, redo, recalculation, the calculation mode, the selection),
//! each edit one transaction and one undo step, recalculated after each commit in automatic mode;
//! its subscriptions ([`Interest`]: an area, a sheet or the selection) get a [`HostDelta`] of the
//! cells whose input, value or provenance changed after each committed transaction and each
//! recalculation. It is independent of any UI toolkit: a sink is any closure.
//!
//! [`save`] writes the current revision through calc-doc's save, with each standing calculated
//! formula value as its cached value and every other formula cell left for Excel to calculate on
//! open.
//!
//! [`HostState::used_area`] bounds what a skin shows of a sheet, and [`blank_package`] (calc-doc's)
//! is the package of a new workbook, opened as any other.
//!
//! [`HostState::recalc_context`] reports the calculation context every recalculation applies
//! ([`RecalcContext`]), which a comparison names as its run's context, apart from what the
//! package declares ([`HostState::declared_date1904`]).
//!
//! The types these calls take and return are re-exported here, with calc-value as [`value`];
//! calc-doc, calc-doc-model and calc-engine themselves are not, so calc-host offers its callers
//! no way around these calls to change the authored revision or the engine state.

mod actions;
mod format;
mod names;
pub use facts::alignment as style_alignment;
pub use format::{BorderPatch, FormatPatch, format_regions, format_samples, patched_style};
pub use names::NameEdit;
pub mod chart;
mod command;
mod envelope;
pub use envelope::{CommandJournal, CommandRefusal, CommandStamp};
mod paste;
pub mod search;
pub use actions::{ConditionalChange, SheetChange, WorkbookCommand, stored_axis_size};
pub use paste::{PastePlan, PasteSource, matrix_shape, prepare_paste};
pub mod conditional;
mod copy;
mod entry;
mod facts;
mod history;
mod project;
mod recalc;
mod save;
mod save_calculation;
mod shift;
pub use shift::shift_cell_edits;
mod session;
mod state;
pub mod validation;

pub use chart::{ChartCorner, ChartKind, ChartPlace, ChartView, PlotView, SeriesView};
pub use command::{CellEdit, CommandError, CommandReceipt, HostCommand, dispatch};
pub use conditional::{BarLook, CellLook, ConditionalFormats, IconLook, LookColor};
pub use copy::{COPY_LIMIT, CopyRefusal, CopySnapshot, PasteMode, check_destination};
pub use entry::{EntryRefusal, interpret_entry, interpret_entry_in};
pub use history::{Irreversible, IrreversibleReason, Selection};
pub use project::{
    ProjectedCell, ProjectedValue, ProjectionError, ProjectionPage, ProjectionRequest, StaleCause,
    StaleValue, project,
};
pub use recalc::{HostProgress, RecalcFailed, RtdDisposition, calculate, calculate_full, step};
pub use save::{
    BeforeSaveHandler, BeforeSaveOutcome, ExcelSaveContext, PreparedSave, SaveCachePolicy,
    SaveFailed, SaveOperation, SavePin, prepare_save, prepare_save_with_cache_policy, save,
};
pub use save_calculation::{
    CalculatedSaveError, CalculatedSaveProgress, CalculatingSave, begin_calculated_save,
};
pub use session::{
    DeltaCause, DeltaGap, DeltaMirror, DeltaSink, HostDelta, HostSession, INTENT_LOG_LIMIT,
    IntentError, IntentReceipt, IntentRecord, Interest, ReplayOutcome, SkinIntent, SubscriptionId,
    UNDO_LIMIT, replay,
};
pub use state::{HostOptions, HostState, OpenFailure, RecalcContext};
pub use validation::{ErrorStyle, ValidationFailure, ValidationRule};

pub use calc_doc::{
    CalcMode, CellError, EditError, FormulaSourceBudget, LoadError, MemorySource, OpenError,
    OpenPolicy, PackageRead, PackageWrite, RegroupedFormulaSources, SaveFormat, SavePolicy,
    SaveReceipt, SaveRefusal, VbaModuleKind, VbaModuleSource, VbaProjectSource, VbaReadError,
    VbaReadPolicy, WriteError,
};
pub use calc_doc::{ColumnSpan, LoadedSheet, LoadedWorkbook, RowFormat, SheetGeometry, SheetView};
pub use calc_doc::{
    blank_package, conditional_rule, convert_binary_workbook, file_form_with_links,
    new_conditional_rule,
};
pub use calc_doc_model::value;
pub use calc_doc_model::{
    ArrayFormula, ArrayKind, AuthoredRevision, Axis, AxisChange, AxisEdit, AxisEditError,
    AxisFormat, AxisFormatEdit, AxisLayouts, BlockId, BlockKind, CachedValue, CellInput, CellText,
    DefinedName, Definitions, FileCached, FiniteNumber, FormulaText, FrozenPanes,
    IterationSettings, ModelRoot, NameScope, PlacementId, RowHidden, SheetEdit, SheetEntry,
    SheetKey, SheetName, SheetVisibility, SkinLayoutEdit, SourcePart, Spot, StyleColor, StyleTable,
    ViewKind,
};
pub use calc_engine::{
    CacheWithoutFormula, CalcTick, Calculation, NotModeled, PrepareError, ReadObserver,
    RecalcFailure, ResultStorageStats, StartError, Unobserved, ValueChange, ValueDelta, WorkBudget,
};
pub use calc_fml::reference::CellRun;
pub use calc_fml::{CallableHandle, CallableTable, FormatProfile};

/// The existing reference-text binder's typed result, exposed to host consumers.
pub use calc_fml::reference::TextReference as ReferenceTarget;

mod presentation_commands;
pub use calc_engine::{
    AdmissionReceipt, CalcCommandId, CalcEvent, CalcPassId, CalcScope, CompletionReceipt,
    EvalSelection, ResultStamp,
};
pub use presentation_commands::{
    AUTOFIT_CELLS, autofit_cells, can_edit, prepare_notebook_link, prepare_notebook_order,
};

pub use calc_engine::contract::{CompletionOutcome, RangePostState};

pub use calc_engine::contract::{AsyncDisposition, AsyncRejection, AsyncToken, ResultDestination};

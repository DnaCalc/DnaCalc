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
//! formula value as its cached value. A Manual SpreadsheetML save may also retain permitted
//! scalar caches, with dirty flags that distinguish them from current calculated values.
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
//!
//! ## Recalculate and save a workbook in memory
//!
//! Give the session the bytes of an Excel workbook. This example starts with a blank package,
//! enters an input and a formula, explicitly calculates, and recalculates the saved package
//! after reopening to verify the result.
//!
//! ```
//! use std::sync::Arc;
//! use calc_host::value::{CalcValue, CellKey};
//! use calc_host::{
//!     CalcMode, CalcScope, EvalSelection, HostOptions, HostProgress, HostSession, MemorySource,
//!     OpenPolicy, ProjectedValue, ProjectionRequest, SavePolicy, SkinIntent, blank_package, project,
//! };
//!
//! fn open(bytes: Vec<u8>) -> HostSession {
//!     HostSession::open(
//!         Arc::new(MemorySource::new(bytes)),
//!         &OpenPolicy::default(),
//!         HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() },
//!     ).unwrap()
//! }
//!
//! fn check_result(book: &mut HostSession, result: CellKey) {
//!     assert!(book.dispatch(SkinIntent::Calculate {
//!         scope: CalcScope::Workbook,
//!         selection: EvalSelection::ForceAll,
//!     }).accepted());
//!     while book.has_pending_calculation() {
//!         match book.advance_calculation() {
//!             HostProgress::Calculating { .. } | HostProgress::Published(_) => {}
//!             other => panic!("unexpected calculation progress: {other:?}"),
//!         }
//!     }
//!     let page = project(book.host(), &ProjectionRequest { cells: vec![result] }).unwrap();
//!     match page.cells()[0].value() {
//!         ProjectedValue::Calculated { value, .. } => assert_eq!(value, &CalcValue::number(42.0)),
//!         other => panic!("expected a calculated value, got {other:?}"),
//!     }
//! }
//!
//! let mut book = open(blank_package());
//! let sheet = book.host().sheets()[0].id();
//! let input = CellKey::new(sheet, 1, 1).unwrap();
//! let result = CellKey::new(sheet, 2, 1).unwrap();
//! for (at, text) in [(input, "21"), (result, "=A1*2")] {
//!     assert!(book.dispatch(SkinIntent::EnterCell { at, text: text.into() }).accepted());
//! }
//! check_result(&mut book, result);
//! let mut saved = Vec::new();
//! book.save(&SavePolicy::default(), &mut saved).unwrap();
//!
//! let mut reopened = open(saved);
//! let sheet = reopened.host().sheets()[0].id();
//! check_result(&mut reopened, CellKey::new(sheet, 2, 1).unwrap());
//! ```
//!
//! Worked examples and preview status are also in the [engine guide](https://dnacalc.com/developers/engine/).
//!
//! See the [crate contract](https://github.com/DnaCalc/DnaCalc/blob/master/crates/calc-host/CONTRACT.md).

#![warn(missing_docs)]

mod actions;
pub mod automation;
pub mod c_api_commands;
mod page_setup;
pub mod print_members;
pub mod print_plan;
pub use calc_doc_model::{PageBreak, PageSetup};
pub use page_setup::{PageSetupChange, print_name, print_references};
mod outline;
mod templates;
pub use outline::OutlineChange;
mod filter;
mod format;
mod format_edges;
pub use filter::{
    AutoFilter, FilterChange, FilterCriteria, FilterOperator, FilterPredicate, filter_values,
    filter_values_for,
};
mod merge;
mod name_calculation;
mod names;
pub use facts::alignment as style_alignment;
pub use format_edges::{EdgeChange, EdgeLine, EdgeWeight, EdgesPatch, decompose as decompose_edge};
pub use format::{BorderPatch, FillColor, FormatPatch, InteriorPatch, format_regions, format_samples, patched_style};
pub use name_calculation::{NameCalculation, NameCalculationProgress, NameCalculationResult};
pub use names::{NameDiagnosticEvaluation, NameEdit, NameInvocationView};
pub mod chart;
mod command;
mod domain;
pub use domain::{CalculationDomain, DomainError, DomainProgress, DomainSaveId, DomainWorkbookId};
mod envelope;
mod events;
pub use envelope::{CommandJournal, CommandRefusal, CommandStamp};
pub use events::{
    DocumentEvent, WorkbookEvent, WorkbookEventFailure, WorkbookEventFailures, WorkbookEventHandler,
    WorkbookEventObserver, WorkbookEventObserverId,
};
mod flash_fill;
mod paste;
pub mod search;
pub use actions::{
    ConditionalChange, SheetChange, SortKey, SortOptions, WorkbookCommand, stored_axis_size,
};
pub use paste::{PastePlan, PasteSource, matrix_shape, prepare_paste, prepare_paste_oriented};
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
pub use command::{CellEdit, CommandError, CommandReceipt, HostCommand, dispatch, dispatch_slice};
pub use conditional::{BarLook, CellLook, ConditionalFormats, IconLook, LookColor};
pub use copy::{COPY_LIMIT, CopyRefusal, CopySnapshot, PasteMode, check_destination};
pub use entry::{EntryRefusal, interpret_entry, interpret_entry_in};
pub use history::{Irreversible, IrreversibleReason, Selection};
pub use project::{
    ProjectedCell, ProjectedSpill, ProjectedValue, ProjectionError, ProjectionPage, ProjectionRequest,
    StaleCause,
    StaleValue, project,
};
pub use recalc::{
    HostProgress, RecalcFailed, RtdAdmission, RtdDisposition, calculate, calculate_full, step,
};
pub use save::{
    BeforeSaveHandler, BeforeSaveOutcome, ExcelSaveContext, PreparedSave, SaveCachePolicy,
    SaveFailed, SaveOperation, SavePin, prepare_save, prepare_save_with_cache_policy, save,
};
pub use save_calculation::{
    CalculatedSaveError, CalculatedSaveProgress, CalculatingSave, OrdinarySave,
    begin_calculated_save,
};
pub use session::{
    CommandIngress, DeltaCause, DeltaGap, DeltaMirror, DeltaSink, HostDelta, HostSession,
    INTENT_LOG_LIMIT, IntentError, IntentReceipt, IntentRecord, Interest, ReplayOutcome,
    SkinIntent, SubscriptionId, UNDO_LIMIT, replay,
};
pub use state::{HostOptions, HostState, OpenFailure, OpenReport, RecalcContext};
pub use validation::{
    ErrorStyle, ValidationFailure, ValidationRule, ValidationScan, ValidationStatus,
};

pub use calc_doc::{
    CalcMode, CellError, EditError, FormulaSourceBudget, LoadError, LoadRepair, MemorySource, OpenError,
    OpenPolicy, PackageRead, PackageWrite, RegroupedFormulaSources, SaveFormat, SavePolicy,
    SaveReceipt, SaveRefusal, SaveViews, SpreadsheetFormat, VbaProjectPart, VbaReadError,
    VbaReadPolicy, WriteError,
};
pub use calc_doc::{ColumnSpan, LoadedSheet, LoadedWorkbook, RowFormat, SheetGeometry, SheetView, SheetViewMode};
/// An add-in file's (`.xlam`) VBA project part, for a desktop host that loads VBA add-ins.
pub use calc_doc::{SessionVba, read_vba_file};
pub use calc_doc::{
    blank_package, conditional_rule, convert_binary_workbook, file_form_with_links,
    new_conditional_rule,
};
pub use calc_doc_model::pivot::PivotSnapshotOrigin;
pub use calc_doc_model::value;
pub use calc_doc_model::{
    ArrayFormula, ArrayKind, AuthoredRevision, Axis, AxisChange, AxisEdit, AxisEditError,
    AxisFormat, AxisFormatEdit, AxisLayouts, BlockId, BlockKind, CachedValue, CellInput, CellText,
    DefinedName, Definitions, FileCached, FiniteNumber, FormulaForm, FormulaText, FrozenPanes,
    IterationSettings, ModelRoot, NameScope, PaneLayout, PlacementId, ReviewNote, ReviewSource,
    RowHidden, SheetEdit, SheetEntry, SheetKey, SheetName, SheetVisibility, SkinLayoutEdit,
    SourcePart, SplitPanes, Spot, StyleColor, StyleTable, UnloadedCell, ViewKind,
};
pub use calc_engine::{
    CacheWithoutFormula, CalcTick, Calculation, LinkedWorkbookSnapshot, NotModeled, PrepareError,
    ReadObserver, RecalcFailure, ResultStorageStats, StartError, Unobserved, ValueChange,
    ValueDelta, WorkBudget, resolve_reference_text,
};
pub use calc_fml::reference::CellRun;
pub use calc_fml::{
    CallableHandle, CallableTable, FormatProfile, NameResultKind, RtdConnection, RtdConnector,
    RtdLease, TopicKey,
};

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

mod tables;
pub use tables::TableChange;
mod shapes;
pub use calc_doc_model::preset_geometry;
mod sparklines;
pub use sparklines::{FormControlView, SparklineChange, SparklineView};
pub use calc_doc_model::{
    ChartAnchor, ChartErrorBars, ChartTrendline, CustomGeometry, DrawingNode, FormControl, Sparkline, SparklineGroup, GeometryCommand, GeometryPath, LineEnd, PictureMedia, ShapePattern, ShapeStyle, ShapeStyleRef, Shape, ShapeBody, ShapeColour, ShapeFill, ShapeGradient, ShapeKind, ShapeLine, ShapeShadow,
    ShapePicture, ShapeTextRun,
};
pub use shapes::{Points, ShapeChange, ShapeOrder};
mod review;
pub use calc_doc_model::NoteStylePatch;
pub use review::ReviewAction;

mod copy_metadata;

pub use calc_doc_model::note_segments;

mod pivot;
mod pivot_style;
pub use pivot_style::PivotStyleLook;

pub use pivot::source_fields as pivot_source_fields;

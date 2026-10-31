//! The package for the CalcDoc model: the engine-neutral authored workbook model and input-view types.
//!
//! Dependency rule: this is the only part of CalcDoc that calc-engine may depend on.
//!
//! - Revisions: an [`AuthoredRevision`] counts committed authored changes, with a checked next.
//! - Model roots: a [`ModelRoot`] is one immutable, `Arc`-shared authored revision of a
//!   document's sheets, each a [`SheetEntry`] with its session [`calc_value::SheetId`], its
//!   document-stable [`SheetKey`], its [`SheetName`] and its [`SheetVisibility`], and of each
//!   sheet's authored cell inputs. [`ModelRoot::new`] makes a root without cells and
//!   [`ModelRoot::with_cells`] one that holds a document's cells as a file gave them; each starts
//!   a lineage that every root [`ModelRoot::apply`] makes from it continues
//!   ([`ModelRoot::same_lineage`]).
//! - File caches: a formula's result as a file stored it is [`FileCached`], a [`CachedValue`]
//!   with the [`SourcePart`] it was read from. It is kept apart from the formula's input and never
//!   counts as calculated.
//! - Cell pages: a sheet's cells live in immutable, `Arc`-shared [`CellPage`]s, one for each
//!   block of [`PAGE_ROWS`] rows by [`PAGE_COLUMNS`] columns that holds an occupied cell. Reads
//!   borrow from the pages: [`ModelRoot::cell`] for one cell, and [`ModelRoot::cells`] and
//!   [`ModelRoot::sheet_cells`] for the occupied cells of a rectangle or a sheet in row-major order
//!   ([`OccupiedCells`]). [`ModelRoot::apply`] applies a batch's cell operations and makes a new
//!   root that shares every page the batch did not change with the old one by pointer; it refuses
//!   a batch it cannot apply ([`ApplyError`]).
//! - Operations: a [`WorkbookOp`] sets one cell's [`CellInput`], clears an area, or applies a
//!   structural-edit descriptor ([`AxisEdit`] for rows and columns, [`SheetEdit`] for sheets,
//!   [`SkinLayoutEdit`] for the skin layout). An
//!   [`OpBatch`] carries one command's operations against a base revision. An operation holds no
//!   engine type, no session operation and no calculated value, and no [`CellInput`] holds a
//!   calculated or cached value. An operation on an existing sheet names it by its
//!   [`calc_value::SheetId`], which travels on the wire as that sheet's [`SheetKey`];
//!   [`SheetEdit::Insert`] names the sheet it adds by a new [`SheetKey`] alone, because the sheet's
//!   ID is issued when the edit is applied.
//! - Definitions: the workbook's [`ArrayFormula`]s (legacy and dynamic, each an anchor holding
//!   the formula text and the area it fills), [`DefinedName`]s, [`Table`]s, and the
//!   [`UnloadedCell`]s whose content the model does not represent, in one [`Definitions`]
//!   ([`ModelRoot::definitions`], built by [`ModelRoot::with_definitions`]). A calculation reads
//!   a cell an array fills (other than its anchor) and an unloaded cell as not available
//!   ([`Definitions::not_available`]). [`ModelRoot::apply`] drops an array formula whose anchor a
//!   batch sets or clears, and an unloaded cell it sets or clears.
//! - Styles: a styles part's [`StyleTable`], document-only metadata the codec keeps beside the
//!   model.
//! - Document-only metadata: facts that are not calculation inputs: [`SheetVisibility`], and the
//!   [`SkinLayout`] a root holds ([`ModelRoot::layout`]): the semantic [`Block`]s, each a stable
//!   opaque [`BlockId`] over a visible backing cell or range reached through its [`LocatorName`],
//!   and each view's [`Placement`]s, with [`PlacementId`]s of their own, in the four views
//!   [`ViewKind`] names. [`ModelRoot::apply`] applies a batch's [`SkinLayoutEdit`]s; a layout
//!   holds no cell content and no calculated answer.
//! - The wire codec: [`encode_batch`] and [`decode_batch`] write and read a versioned batch whose
//!   operations travel in per-capability envelopes; each capability's codec, and the dispatch to
//!   it, is generated from its declaration ([`op_capabilities`]).
//!
//! [`value`] re-exports calc-value, whose grid coordinates, sheet IDs, text and worksheet error
//! codes these types hold.

mod axis_layout;
mod cache;
mod cells;
mod definitions;
mod input;
mod layout;
pub use axis_layout::{AxisFormat, AxisFormatEdit, AxisLayouts};
mod op;
mod revision;
mod root;
mod sheet;
mod structural;
mod style;
mod wire;

pub use cache::{CachedValue, FileCached, SourcePart};
pub use calc_value as value;
pub use cells::{CellPage, OccupiedCells, PAGE_COLUMNS, PAGE_ROWS};
pub use definitions::{
    ArrayFormula, ArrayKind, DataTable, DataTableInputs, DefinedName, Definitions, ExternalBook,
    ExternalSheet, IterationSettings, NameScope, RowHidden, Table, UnloadedCell,
};
pub use input::{CellInput, CellText, FiniteNumber, FormulaText, FormulaTextError};
pub use layout::{
    Block, BlockId, BlockKind, CanvasArea, LayoutError, LocatorName, Placement, PlacementId,
    SkinLayout, SkinLayoutEdit, Spot, ViewKind,
};
pub use op::{
    Axis, AxisChange, AxisEdit, AxisEditError, FrozenPanes, OpBatch, SheetEdit, WorkbookOp,
};
pub use revision::AuthoredRevision;
pub use root::{ApplyError, ModelRoot, ModelRootError, SheetEntry};
pub use sheet::{
    SHEET_NAME_MAX_UTF16_CODE_UNITS, SheetKey, SheetName, SheetNameError, SheetVisibility,
};
pub use structural::{
    FormulaRewriter, StructuralChange, moved_area, moved_cell, moved_position, moved_run,
    source_position,
};
pub use style::{
    Alignment, Border, BorderEdge, CellFormat, CellStyle, DifferentialFormat, Fill, Font,
    NumberFormat, Protection, StyleColor, StyleTable, TableStyles, WorkbookColors,
    builtin_number_format,
};
pub use wire::{
    BATCH_FORMAT, BATCH_VERSION, DecodeError, EncodeError, OpCapability, decode_batch,
    encode_batch, op_capabilities,
};

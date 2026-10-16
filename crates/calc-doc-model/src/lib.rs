//! The package for the CalcDoc model: the engine-neutral authored workbook model and input-view types.
//!
//! Dependency rule: this is the only part of CalcDoc that calc-engine may depend on.
//!
//! - Revisions: an [`AuthoredRevision`] counts committed authored changes, with a checked next.
//! - Model roots: a [`ModelRoot`] is one immutable, `Arc`-shared authored revision of a
//!   document's sheets, each a [`SheetEntry`] with its session [`calc_value::SheetId`], its
//!   document-stable [`SheetKey`], its [`SheetName`] and its [`SheetVisibility`]. It holds no cell
//!   storage yet.
//! - Operations: a [`WorkbookOp`] sets one cell's [`CellInput`], clears an area, or applies a
//!   structural-edit descriptor ([`AxisEdit`] for rows and columns, [`SheetEdit`] for sheets). An
//!   [`OpBatch`] carries one command's operations against a base revision. An operation holds no
//!   engine type, no session operation and no calculated value, and no [`CellInput`] holds a
//!   calculated or cached value. An operation on an existing sheet names it by its
//!   [`calc_value::SheetId`], which travels on the wire as that sheet's [`SheetKey`];
//!   [`SheetEdit::Insert`] names the sheet it adds by a new [`SheetKey`] alone, because the sheet's
//!   ID is issued when the edit is applied.
//! - Document-only metadata: facts that are not calculation inputs, starting with
//!   [`SheetVisibility`].
//! - The wire codec: [`encode_batch`] and [`decode_batch`] write and read a versioned batch whose
//!   operations travel in per-capability envelopes; each capability's codec, and the dispatch to
//!   it, is generated from its declaration ([`op_capabilities`]).
//!
//! [`value`] re-exports calc-value, whose grid coordinates, sheet IDs, text and worksheet error
//! codes these types hold.

mod input;
mod op;
mod revision;
mod root;
mod sheet;
mod wire;

pub use calc_value as value;
pub use input::{CellInput, CellText, FiniteNumber, FormulaText};
pub use op::{Axis, AxisChange, AxisEdit, AxisEditError, OpBatch, SheetEdit, WorkbookOp};
pub use revision::AuthoredRevision;
pub use root::{ModelRoot, ModelRootError, SheetEntry};
pub use sheet::{
    SHEET_NAME_MAX_UTF16_CODE_UNITS, SheetKey, SheetName, SheetNameError, SheetVisibility,
};
pub use wire::{
    BATCH_FORMAT, BATCH_VERSION, DecodeError, EncodeError, OpCapability, decode_batch,
    encode_batch, op_capabilities,
};

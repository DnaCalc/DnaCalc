//! The package for CalcFunc's lowest layer: the value types and service contracts that CalcFunc
//! and the CalcDoc model share (plan sections 1.3, 2.1 and 2.2).
//!
//! - Identities: [`WorkbookId`], [`SheetId`], [`NameId`], [`TableId`] and [`NodeId`] are
//!   generational (a slot and a generation) and only an [`IdAllocator`] issues them.
//! - Grid coordinates: a [`CellKey`] is a sheet and a 1-based `u32` row and column, checked
//!   against Excel's bounds ([`MAX_ROWS`] by [`MAX_COLUMNS`]); a [`GridArea`] is a rectangle on
//!   one sheet. Both are `Copy` values, so a full-sheet area allocates nothing.
//! - Values: [`CalcValue`] over [`CoreValue`] (number, text, logical, worksheet error, empty,
//!   missing, array, reference) plus an optional [`CallableValue`]. A [`ReferenceLike`] holds one
//!   or more areas of one sheet. Shared payloads (text, array cells, multi-area references,
//!   callable handles) are immutable behind `std::sync::Arc`.
//! - Numeric policy: [`NumericMode`] is `Fast` by default or `Exact` for one
//!   [`NumericTargetId`] (plan section 5). No process-global mode exists; a mode is a value the
//!   caller passes.
//!
//! - Names: [`workbook_names_equal`] and [`parameter_names_equal`] compare defined, table and
//!   column names and LET / LAMBDA parameter names as Excel does, with Unicode case folding
//!   (dc-cpr); [`fold_workbook_name`] and [`fold_parameter_name`] give the keys they compare.
//!
//! The package declares no cargo features, so there is no thread-bound variant of any type:
//! every public value type is `Send + Sync`.

mod case_folding;
mod grid;
mod id;
mod name_case;
mod numeric;
mod value;

/// Whether `left` and `right` are the same workbook-scoped name (a defined name, a table name, a
/// table's column name) as Excel compares them: equal under the full Unicode case fold (`ß` =
/// `SS`, final `ς` = `σ`; accents, width and the Turkic `İ` / `ı` stay significant). See
/// `name_case` for the Excel measurement.
pub fn workbook_names_equal(left: &str, right: &str) -> bool {
    name_case::workbook_names_equal(left, right)
}

/// Whether `left` and `right` are the same LET or LAMBDA parameter name as Excel compares them:
/// equal under the simple Unicode case fold (`ẞ` = `ß` but `ß` ≠ `SS`).
pub fn parameter_names_equal(left: &str, right: &str) -> bool {
    name_case::parameter_names_equal(left, right)
}

/// The key [`workbook_names_equal`] compares: the full Unicode case fold of `text`.
pub fn fold_workbook_name(text: &str) -> String {
    name_case::fold_workbook_name(text)
}

/// The key [`parameter_names_equal`] compares: the simple Unicode case fold of `text`.
pub fn fold_parameter_name(text: &str) -> String {
    name_case::fold_parameter_name(text)
}

pub use grid::{CellKey, GridArea, GridError, Selection, MAX_COLUMNS, MAX_ROWS};
pub use id::{IdAllocator, IdError, NameId, NodeId, SheetId, TableId, WorkbookId};
pub use numeric::{NumericMode, NumericTargetId};
pub use value::{
    ArrayShape, CalcArray, CalcValue, CallableArityShape, CallableValue, CoreRef, CoreValue,
    EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, OpaqueCallable, ReferenceError, ReferenceLike,
    WorksheetErrorCode,
};

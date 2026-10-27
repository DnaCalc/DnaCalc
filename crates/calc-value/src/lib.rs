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
//! The package declares no cargo features, so there is no thread-bound variant of any type:
//! every public value type is `Send + Sync`.

mod grid;
mod id;
mod numeric;
mod value;

pub use grid::{CellKey, GridArea, GridError, MAX_COLUMNS, MAX_ROWS};
pub use id::{IdAllocator, IdError, NameId, NodeId, SheetId, TableId, WorkbookId};
pub use numeric::{NumericMode, NumericTargetId};
pub use value::{
    ArrayShape, CalcArray, CalcValue, CallableArityShape, CallableValue, CoreRef, CoreValue,
    EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, OpaqueCallable, ReferenceError, ReferenceLike,
    WorksheetErrorCode,
};

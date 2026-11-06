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
mod combining_marks;

/// The simple Unicode case fold of one character, shared by names and text matching.
pub fn unicode_simple_case_fold(ch: char) -> char {
    case_folding::simple_fold(ch)
}

/// Whether a character is a Unicode nonspacing or spacing combining mark (Mn or Mc).
pub fn is_combining_mark(ch: char) -> bool {
    combining_marks::is_combining_mark(ch)
}
/// Whether a character has Unicode general category Mn (nonspacing mark).
pub fn is_nonspacing_mark(ch: char) -> bool {
    combining_marks::is_nonspacing_mark(ch)
}

/// Whether a character has Unicode general category Pc (connector punctuation).
pub fn is_connector_punctuation(ch: char) -> bool {
    combining_marks::is_connector_punctuation(ch)
}

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

pub use grid::{CellKey, GridArea, GridError, MAX_COLUMNS, MAX_ROWS, Selection};
pub use id::{
    IdAllocator, IdError, NameId, NameKey, NameScope, NodeId, SheetId, TableId, WorkbookId,
};
pub use numeric::{NumericMode, NumericTargetId};
pub use value::{
    ArrayShape, CalcArray, CalcValue, CallableArityShape, CallableValue, CoreRef, CoreValue,
    EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, ImageFormat, ImageValue, OpaqueCallable,
    ReferenceError, ReferenceLike,
    WorksheetErrorCode,
};

/// Maximum cells in one clipboard table.
pub const COPY_LIMIT: u64 = 1_000_000;
pub fn matrix_shape(rows: &[Vec<String>]) -> Result<(u32, u32), String> {
    let width = rows.first().map_or(0, Vec::len);
    if width == 0 || rows.is_empty() || rows.iter().any(|row| row.len() != width) {
        return Err("Paste a rectangular table".to_owned());
    }
    if rows.len() as u64 * width as u64 > COPY_LIMIT {
        return Err("The clipboard table is too large".to_owned());
    }
    Ok((rows.len() as u32, width as u32))
}

/// Excel's documented points/characters input, with zero meaning Hide. Small character widths
/// below one need a separate default-font measurement and are explicitly unavailable here.
pub fn stored_axis_size(rows: bool, value: f64) -> Result<Option<f64>, &'static str> {
    let limit = if rows { 409.0 } else { 255.0 };
    if !value.is_finite() || value < 0.0 || value > limit {
        return Err(if rows {
            "Use a row height from 0 to 409 points"
        } else {
            "Use a column width from 1 to 255 characters, or 0 to hide"
        });
    }
    if value == 0.0 {
        return Ok(None);
    }
    if !rows && value < 1.0 {
        return Err("Column widths below one character are not available here; use 0 to hide");
    }
    // The retained grid uses Excel's default maximum digit width, seven pixels. The file's
    // character width includes five pixels of padding (ECMA-376 18.3.1.13).
    Ok(Some(if rows {
        value
    } else {
        ((value * 7.0 + 5.0) / 7.0 * 256.0).trunc() / 256.0
    }))
}

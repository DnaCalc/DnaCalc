//! The typed refusals of loading a workbook's sheets and cells.

use crate::error::PartReadError;
use crate::xml::XmlError;
use calc_doc_model::SheetNameError;
use calc_doc_model::value::IdError;
use std::fmt;

/// Why [`crate::load`] refused a document. A refused load makes no model, and every sheet ID it
/// issued has been released.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    /// A part could not be read.
    Part { part: String, error: PartReadError },
    /// A part is not acceptable XML.
    Xml { part: String, error: XmlError },
    /// The package's relationships do not name one workbook part inside it.
    Workbook { detail: &'static str },
    /// The workbook part's content type is not a SpreadsheetML workbook's.
    NotASpreadsheet {
        part: String,
        content_type: Option<String>,
    },
    /// A relationship's target part is not in the package.
    MissingPart { part: String },
    /// A part does not have the shape SpreadsheetML gives it.
    InvalidPart { part: String, detail: &'static str },
    /// A sheet is a chart sheet, a dialog sheet, a macro sheet or another kind than a worksheet.
    UnsupportedSheet { name: String, kind: String },
    /// A sheet's name is not a sheet name.
    InvalidSheetName { name: String, error: SheetNameError },
    /// Two sheets have this name, compared ignoring ASCII case.
    DuplicateSheetName { name: String },
    /// Two sheets have this `sheetId`.
    DuplicateSheetKey { key: u32 },
    /// The caller's allocator could not issue a sheet ID.
    SheetIds(IdError),
    /// A cell's `r` attribute is not a cell reference inside the grid.
    InvalidCellAddress { part: String, text: String },
    /// A row's `r` attribute is not a row number inside the grid.
    InvalidRowNumber { part: String, text: String },
    /// A cell was refused.
    Cell {
        part: String,
        row: u32,
        column: u32,
        error: CellError,
    },
}

/// Why one cell was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum CellError {
    /// The cell is not after the cell or row before it, or lies outside the row that holds it.
    OutOfOrder,
    /// The cell's type (`t`) is one this loader does not read, such as `d` (an ISO 8601 date).
    UnsupportedType { kind: String },
    /// The cell carries cell or value metadata (`cm`, `vm`), whose values are not read here.
    UnsupportedMetadata,
    /// The cell's style (`s`) is not an index into the workbook's cell formats.
    StyleIndex { text: String, cell_formats: u32 },
    /// The cell's value is not a number, or not a finite one.
    InvalidNumber { text: String },
    /// The cell's value is not a boolean.
    InvalidBoolean { text: String },
    /// The cell's value is not a worksheet error this loader knows.
    UnknownError { text: String },
    /// The cell's value is not an index into the shared strings.
    SharedStringIndex { text: String, count: usize },
    /// The cell's text is longer than a cell's text can be.
    TextTooLong { units: usize },
    /// The cell's elements do not fit its type.
    Malformed { detail: &'static str },
    /// The formula is an array or data table formula, or of a kind this loader does not read.
    UnsupportedFormula { kind: String },
    /// The formula's text is empty.
    EmptyFormula,
    /// The formula's text is longer than [`crate::FORMULA_MAX_UTF16_CODE_UNITS`].
    FormulaTooLong { units: usize },
    /// A shared formula's group is not well formed.
    SharedFormula { detail: &'static str },
    /// Moving a shared formula's relative references to this cell takes one outside the grid.
    SharedFormulaPastGrid,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Part { part, error } => write!(f, "the part {part:?} could not be read: {error}"),
            Self::Xml { part, error } => write!(f, "the part {part:?} is refused: {error}"),
            Self::Workbook { detail } => write!(f, "the package names no workbook: {detail}"),
            Self::NotASpreadsheet { part, content_type } => write!(
                f,
                "the workbook part {part:?} has the content type {content_type:?}, not a SpreadsheetML workbook's"
            ),
            Self::MissingPart { part } => write!(f, "the part {part:?} is not in the package"),
            Self::InvalidPart { part, detail } => {
                write!(f, "the part {part:?} is invalid: {detail}")
            }
            Self::UnsupportedSheet { name, kind } => {
                write!(f, "the sheet {name:?} is a {kind:?}, which is not loaded")
            }
            Self::InvalidSheetName { name, error } => {
                write!(f, "the sheet name {name:?} is refused: {error}")
            }
            Self::DuplicateSheetName { name } => write!(f, "two sheets are named {name:?}"),
            Self::DuplicateSheetKey { key } => write!(f, "two sheets have the sheetId {key}"),
            Self::SheetIds(error) => write!(f, "no sheet ID could be issued: {error}"),
            Self::InvalidCellAddress { part, text } => {
                write!(
                    f,
                    "{part}: the cell address {text:?} is not a cell of the grid"
                )
            }
            Self::InvalidRowNumber { part, text } => {
                write!(
                    f,
                    "{part}: the row number {text:?} is not a row of the grid"
                )
            }
            Self::Cell {
                part,
                row,
                column,
                error,
            } => write!(f, "{part}: the cell at row {row} column {column}: {error}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl fmt::Display for CellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfOrder => f.write_str("it is not after the cell or row before it"),
            Self::UnsupportedType { kind } => write!(f, "the cell type {kind:?} is not read"),
            Self::UnsupportedMetadata => f.write_str("cell and value metadata are not read"),
            Self::StyleIndex { text, cell_formats } => write!(
                f,
                "the style {text:?} is not an index into the {cell_formats} cell formats"
            ),
            Self::InvalidNumber { text } => write!(f, "{text:?} is not a finite number"),
            Self::InvalidBoolean { text } => write!(f, "{text:?} is not a boolean"),
            Self::UnknownError { text } => write!(f, "{text:?} is not a known worksheet error"),
            Self::SharedStringIndex { text, count } => write!(
                f,
                "{text:?} is not an index into the {count} shared strings"
            ),
            Self::TextTooLong { units } => write!(
                f,
                "the text has {units} UTF-16 code units, more than a cell holds"
            ),
            Self::Malformed { detail } => f.write_str(detail),
            Self::UnsupportedFormula { kind } => {
                write!(f, "the formula kind {kind:?} is not read")
            }
            Self::EmptyFormula => f.write_str("the formula's text is empty"),
            Self::FormulaTooLong { units } => write!(
                f,
                "the formula has {units} UTF-16 code units, more than {}",
                super::FORMULA_MAX_UTF16_CODE_UNITS
            ),
            Self::SharedFormula { detail } => write!(f, "the shared formula is refused: {detail}"),
            Self::SharedFormulaPastGrid => f.write_str(
                "moving the shared formula's relative references here takes one outside the grid",
            ),
        }
    }
}

impl std::error::Error for CellError {}

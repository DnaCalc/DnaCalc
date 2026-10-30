//! Typed Excel reference binding and value access, without a calculation engine.
//!
//! [`bind_references`] walks a parsed formula and binds each grid reference it holds (A1 or R1C1
//! cells and rectangles, whole rows and columns, unqualified or qualified by one sheet) against a
//! [`ReferenceCatalog`] and the formula's [`FormulaAnchor`], giving a typed [`ReferenceOperand`]
//! per reference. An operand records each row and column as [`AxisRef::Absolute`] or as an
//! [`AxisRef::Relative`] offset from the anchor, so it is the same value for every cell a formula
//! is copied to: `=B3` entered in C5 and `=E8` entered in F10 bind to equal operands, and so does
//! `=R[-2]C[-1]` in R1C1 notation anywhere. The ends of a rectangle or of a run of rows or columns
//! are held in an order that does not depend on the anchor either, so `=R[-2]C[-1]:R7C4` is one
//! operand wherever it is bound. [`ReferenceOperand::instantiate`] places an operand at
//! an anchor and gives the [`GridArea`] it names there, or the typed [`PlacementError`] (a
//! worksheet `#REF!`) when the area would leave the grid or names a deleted sheet or deleted
//! cells.
//!
//! A [`ReferenceReader`] supplies the values of an area: [`read_cells`] asks it for one area by
//! its typed coordinates and receives row-major [`CellRun`]s that borrow stored values and keep
//! blank cells apart from cells the reader cannot supply.
//!
//! Reference text that a formula computes, as INDIRECT reads it, is bound for the evaluator with
//! the same spelling, ordering and catalogue, under INDIRECT's own grammar (the `text` module).
//!
//! The catalogue and the reader are the only ways in to workbook state, and neither receives
//! reference text: the catalogue is asked for a sheet by name, the reader for a [`GridArea`].
//! Anything that holds sheets and cells can implement them, a calculation engine or a small
//! fixture alike.

mod bind;
mod notation;
mod read;
mod structured;
mod template;
mod text;
mod transform;

pub(crate) use bind::{external_operand, qualified_name};
pub use bind::{
    BindError, ExternalBookRef, ExternalOperand, NameDefinition, NameScope, ReferenceCatalog,
    ReferenceSite, bind_references,
};
pub use read::{
    CellRead, CellRun, ReadContractError, ReferenceReader, SpillExtent, read_cell_by_area,
    read_cells,
};
pub use structured::{TableInfo, TableLookup};
pub use template::{template_key, template_key_of_chars};
pub use transform::{
    EditAxis, StructuralEdit, convert_notation, transform_formula, transform_name,
};
pub(crate) use text::bind_text;

use calc_func::value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId, WorksheetErrorCode};

/// The cell a formula belongs to. An unqualified reference names a cell of the anchor's sheet,
/// and a relative row or column is an offset from the anchor's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FormulaAnchor {
    cell: CellKey,
}

impl FormulaAnchor {
    pub const fn new(cell: CellKey) -> Self {
        FormulaAnchor { cell }
    }

    pub const fn cell(self) -> CellKey {
        self.cell
    }

    pub const fn sheet(self) -> SheetId {
        self.cell.sheet()
    }
}

/// The notation a formula's references are written in. The same text can mean different things
/// in each: `R2C3` is a cell in R1C1 notation and a name in A1 notation, and `B3` the reverse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReferenceNotation {
    /// `B3`, `$B$3`, `B3:D7`, `2:4`, `B:D`, with `$` marking an absolute row or column.
    A1,
    /// `R3C2`, `R[-2]C[-1]`, `RC`, `R2:R4`, `C[1]`, with brackets marking a relative offset.
    R1C1,
}

/// One coordinate of a reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisRef {
    /// A fixed 1-based row or column: `$3` or `$B` in A1 notation, `R3` or `C2` in R1C1.
    Absolute(u32),
    /// An offset from the anchor's row or column: `3` or `B` in A1 notation (stored as the
    /// distance from the anchor the formula was bound at), `R[-2]`, `C[1]`, `R` or `C` in R1C1.
    Relative(i32),
}

impl AxisRef {
    /// The coordinate this axis names when the anchor's coordinate is `anchor`; it may lie off
    /// the grid.
    pub fn at(self, anchor: u32) -> i64 {
        match self {
            AxisRef::Absolute(index) => i64::from(index),
            AxisRef::Relative(offset) => i64::from(anchor) + i64::from(offset),
        }
    }
}

/// The row and column of one cell of a reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellRef {
    pub row: AxisRef,
    pub column: AxisRef,
}

/// What a reference covers on its sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReferenceShape {
    /// One cell: `B3`, `R[-2]C[-1]`.
    Cell(CellRef),
    /// The rectangle between two cells: `B3:$D$7`. The binder holds each axis's pair in an order
    /// that does not depend on the anchor: two absolute or two relative coordinates by position
    /// (`D7:B3` binds as `B3:D7`), an absolute and a relative one absolute first (`B3:$D$7` binds
    /// as `$D$7:B3`), since those cross as the anchor moves. `first` and `last` are therefore not
    /// always the top-left and bottom-right corners; placing the operand finds those.
    Area { first: CellRef, last: CellRef },
    /// Whole rows, every column: `2:$4`, `R[-1]:R[1]`, `R3`, `R`, the pair ordered as an
    /// [`ReferenceShape::Area`]'s rows are.
    Rows { first: AxisRef, last: AxisRef },
    /// Whole columns, every row: `B:$D`, `C1:C3`, `C[1]`, `C`, the pair ordered as an
    /// [`ReferenceShape::Area`]'s columns are.
    Columns { first: AxisRef, last: AxisRef },
    /// The `#REF!` Excel writes in place of a reference whose cells were deleted
    /// (`Sheet2!#REF!`): it covers no cell, and placing the operand gives
    /// [`PlacementError::DeletedCells`].
    Deleted,
}

/// The sheet a reference names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SheetTarget {
    /// No qualifier: the sheet of the anchor the operand is placed at, so a formula copied to
    /// another sheet reads that sheet.
    Anchor,
    /// The sheet the catalogue resolved a qualifier (`Sheet2!`, `'Q2 Data'!`) to; it stays put
    /// wherever the formula is copied.
    Sheet(SheetId),
    /// The `#REF!` qualifier Excel writes in place of a deleted sheet (`#REF!A1`). Placing the
    /// operand gives [`PlacementError::DeletedSheet`].
    Deleted,
    /// A run of sheets (`Sheet1:Sheet3!A1`, a 3-D reference): the sheets from `first` to `last`
    /// in the workbook's sheet order, both included, whichever of the two comes first. Compile
    /// expands it into one operand per sheet the catalogue lists
    /// ([`ReferenceCatalog::sheet_span`]); placing a span operand itself gives
    /// [`PlacementError::SheetSpan`].
    Span { first: SheetId, last: SheetId },
}

/// A bound grid reference: its sheet and shape, with every coordinate absolute or relative to the
/// anchor. It holds no anchor, so the operands of a formula and of its copies are equal values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReferenceOperand {
    pub sheet: SheetTarget,
    pub shape: ReferenceShape,
}

/// Why an operand names no area at an anchor. Each is the worksheet error `#REF!` (see
/// [`PlacementError::worksheet_error`]); the variant keeps the cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlacementError {
    /// The reference is qualified by `#REF!`: its sheet was deleted.
    DeletedSheet,
    /// The reference is `#REF!` on its sheet (`Sheet2!#REF!`): its cells were deleted.
    DeletedCells,
    /// A row lands at `resolved`, outside rows 1 to [`MAX_ROWS`].
    RowOffGrid { resolved: i64 },
    /// A column lands at `resolved`, outside columns 1 to [`MAX_COLUMNS`].
    ColumnOffGrid { resolved: i64 },
    /// The operand names a run of sheets ([`SheetTarget::Span`]), which names no one area; compile
    /// places each sheet of the span on its own.
    SheetSpan,
}

impl PlacementError {
    /// The worksheet value of a reference that cannot be placed: `#REF!`. Whether Excel gives
    /// `#REF!` or wraps on every route (copy, fill, R1C1 entry, names) is not measured; the
    /// migration record lists it.
    pub fn worksheet_error(self) -> WorksheetErrorCode {
        match self {
            PlacementError::DeletedSheet
            | PlacementError::DeletedCells
            | PlacementError::RowOffGrid { .. }
            | PlacementError::ColumnOffGrid { .. }
            | PlacementError::SheetSpan => WorksheetErrorCode::Ref,
        }
    }
}

impl ReferenceOperand {
    /// The area this operand names for a formula anchored at `anchor`: its relative coordinates
    /// are offsets from the anchor's, and an unqualified operand lies on the anchor's sheet. The
    /// sheet is checked before the cells, the row of a cell before its column, and the first
    /// corner before the last.
    pub fn instantiate(&self, anchor: FormulaAnchor) -> Result<GridArea, PlacementError> {
        let sheet = match self.sheet {
            SheetTarget::Anchor => anchor.sheet(),
            SheetTarget::Sheet(sheet) => sheet,
            SheetTarget::Deleted => return Err(PlacementError::DeletedSheet),
            SheetTarget::Span { .. } => return Err(PlacementError::SheetSpan),
        };
        let at = anchor.cell();
        let (top, left, bottom, right) = match self.shape {
            ReferenceShape::Cell(cell) => {
                let row = row_at(cell.row, at)?;
                let column = column_at(cell.column, at)?;
                (row, column, row, column)
            }
            ReferenceShape::Area { first, last } => {
                let first_row = row_at(first.row, at)?;
                let first_column = column_at(first.column, at)?;
                let last_row = row_at(last.row, at)?;
                let last_column = column_at(last.column, at)?;
                (first_row, first_column, last_row, last_column)
            }
            ReferenceShape::Rows { first, last } => {
                (row_at(first, at)?, 1, row_at(last, at)?, MAX_COLUMNS)
            }
            ReferenceShape::Columns { first, last } => {
                (1, column_at(first, at)?, MAX_ROWS, column_at(last, at)?)
            }
            ReferenceShape::Deleted => return Err(PlacementError::DeletedCells),
        };
        Ok(area(sheet, top, left, bottom, right))
    }
}

impl ReferenceOperand {
    /// The area this operand names for a formula anchored at `anchor` when it is a reference of
    /// a defined name's formula: as [`ReferenceOperand::instantiate`] places it, except that a
    /// relative row or column wraps around the grid's edge. A file stores a name's references
    /// relative to cell A1 (`Nm!XFD1` for `RC[-1]`), and Excel 16.0 (2026-10-23) reads such a
    /// name at each formula's cell with the offset wrapped: `=LeftCell` for `RC[-1]` in A1 reads
    /// XFD1.
    pub fn instantiate_wrapped(&self, anchor: FormulaAnchor) -> Result<GridArea, PlacementError> {
        let sheet = match self.sheet {
            SheetTarget::Anchor => anchor.sheet(),
            SheetTarget::Sheet(sheet) => sheet,
            SheetTarget::Deleted => return Err(PlacementError::DeletedSheet),
            SheetTarget::Span { .. } => return Err(PlacementError::SheetSpan),
        };
        let at = anchor.cell();
        let row = |axis: AxisRef| wrap(axis, at.row(), MAX_ROWS);
        let column = |axis: AxisRef| wrap(axis, at.column(), MAX_COLUMNS);
        let (top, left, bottom, right) = match self.shape {
            ReferenceShape::Cell(cell) => {
                let (r, c) = (row(cell.row), column(cell.column));
                (r, c, r, c)
            }
            ReferenceShape::Area { first, last } => (
                row(first.row),
                column(first.column),
                row(last.row),
                column(last.column),
            ),
            ReferenceShape::Rows { first, last } => (row(first), 1, row(last), MAX_COLUMNS),
            ReferenceShape::Columns { first, last } => (1, column(first), MAX_ROWS, column(last)),
            ReferenceShape::Deleted => return Err(PlacementError::DeletedCells),
        };
        let (top, bottom) = (top.min(bottom), top.max(bottom));
        let (left, right) = (left.min(right), left.max(right));
        Ok(area(sheet, top, left, bottom, right))
    }
}

/// The 1-based index `axis` names when the anchor's is `anchor`, on an axis of `max` indexes, a
/// relative offset wrapping around the edge.
pub(crate) fn wrap(axis: AxisRef, anchor: u32, max: u32) -> u32 {
    match axis {
        AxisRef::Absolute(index) => index,
        AxisRef::Relative(offset) => {
            let zero_based = (i64::from(anchor) - 1 + i64::from(offset)).rem_euclid(i64::from(max));
            u32::try_from(zero_based + 1).expect("an index below the axis's size")
        }
    }
}

fn row_at(axis: AxisRef, anchor: CellKey) -> Result<u32, PlacementError> {
    on_grid(axis.at(anchor.row()), MAX_ROWS).ok_or(PlacementError::RowOffGrid {
        resolved: axis.at(anchor.row()),
    })
}

fn column_at(axis: AxisRef, anchor: CellKey) -> Result<u32, PlacementError> {
    on_grid(axis.at(anchor.column()), MAX_COLUMNS).ok_or(PlacementError::ColumnOffGrid {
        resolved: axis.at(anchor.column()),
    })
}

fn on_grid(resolved: i64, max: u32) -> Option<u32> {
    u32::try_from(resolved)
        .ok()
        .filter(|index| (1..=max).contains(index))
}

/// The area between two corners whose coordinates `row_at` and `column_at` have checked.
fn area(sheet: SheetId, top: u32, left: u32, bottom: u32, right: u32) -> GridArea {
    let first = CellKey::new(sheet, top, left).expect("row_at and column_at checked the corner");
    let last = CellKey::new(sheet, bottom, right).expect("row_at and column_at checked the corner");
    GridArea::new(first, last).expect("both corners lie on one sheet")
}

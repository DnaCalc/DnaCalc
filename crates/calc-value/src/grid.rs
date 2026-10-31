//! Checked Excel grid coordinates: a [`CellKey`] names one cell of one sheet and a [`GridArea`]
//! a rectangle of one sheet. Both are `Copy` values with private fields, built only through
//! checked constructors, so the full Excel grid is addressable without allocating anything.
//! Reference text (A1 or R1C1) is not parsed here; that belongs to CalcFml.

use crate::id::SheetId;
use std::fmt;

/// The number of rows of an Excel worksheet; rows are numbered 1 to `MAX_ROWS`.
pub const MAX_ROWS: u32 = 1_048_576;

/// The number of columns of an Excel worksheet; columns are numbered 1 (A) to `MAX_COLUMNS` (XFD).
pub const MAX_COLUMNS: u32 = 16_384;

/// Why a coordinate or an area was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridError {
    /// The row is 0 or greater than [`MAX_ROWS`].
    RowOutOfRange(u32),
    /// The column is 0 or greater than [`MAX_COLUMNS`].
    ColumnOutOfRange(u32),
    /// An area's two corners lie on different sheets.
    SheetMismatch { first: SheetId, last: SheetId },
}

impl fmt::Display for GridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GridError::RowOutOfRange(row) => {
                write!(f, "row {row} is outside the grid's rows 1 to {MAX_ROWS}")
            }
            GridError::ColumnOutOfRange(column) => {
                write!(
                    f,
                    "column {column} is outside the grid's columns 1 to {MAX_COLUMNS}"
                )
            }
            GridError::SheetMismatch { first, last } => {
                write!(
                    f,
                    "an area's corners lie on two sheets ({first:?} and {last:?})"
                )
            }
        }
    }
}

impl std::error::Error for GridError {}

/// One cell: a sheet and a 1-based row and column inside Excel's bounds. Keys order by sheet,
/// then row, then column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellKey {
    sheet: SheetId,
    row: u32,
    column: u32,
}

impl CellKey {
    /// The cell at `row` and `column` (both 1-based) of `sheet`, or the typed error for a zero or
    /// out-of-bounds coordinate (the row is checked first).
    pub const fn new(sheet: SheetId, row: u32, column: u32) -> Result<Self, GridError> {
        if row == 0 || row > MAX_ROWS {
            return Err(GridError::RowOutOfRange(row));
        }
        if column == 0 || column > MAX_COLUMNS {
            return Err(GridError::ColumnOutOfRange(column));
        }
        Ok(CellKey { sheet, row, column })
    }

    pub const fn sheet(self) -> SheetId {
        self.sheet
    }

    /// The 1-based row.
    pub const fn row(self) -> u32 {
        self.row
    }

    /// The 1-based column (1 is A).
    pub const fn column(self) -> u32 {
        self.column
    }
}

/// A rectangle of cells on one sheet, from its top-left to its bottom-right corner, both
/// inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GridArea {
    sheet: SheetId,
    top: u32,
    left: u32,
    bottom: u32,
    right: u32,
}

impl GridArea {
    /// The area spanned by two cells of one sheet, in either corner order; the typed error when
    /// they lie on different sheets.
    pub fn new(first: CellKey, last: CellKey) -> Result<Self, GridError> {
        if first.sheet != last.sheet {
            return Err(GridError::SheetMismatch {
                first: first.sheet,
                last: last.sheet,
            });
        }
        Ok(GridArea {
            sheet: first.sheet,
            top: first.row.min(last.row),
            left: first.column.min(last.column),
            bottom: first.row.max(last.row),
            right: first.column.max(last.column),
        })
    }

    /// The single-cell area of `key`.
    pub const fn cell(key: CellKey) -> Self {
        GridArea {
            sheet: key.sheet,
            top: key.row,
            left: key.column,
            bottom: key.row,
            right: key.column,
        }
    }

    /// Every cell of `sheet`: A1 to XFD1048576.
    pub const fn full_sheet(sheet: SheetId) -> Self {
        GridArea {
            sheet,
            top: 1,
            left: 1,
            bottom: MAX_ROWS,
            right: MAX_COLUMNS,
        }
    }

    pub const fn sheet(self) -> SheetId {
        self.sheet
    }

    /// The top-left cell.
    pub const fn first(self) -> CellKey {
        CellKey {
            sheet: self.sheet,
            row: self.top,
            column: self.left,
        }
    }

    /// The bottom-right cell.
    pub const fn last(self) -> CellKey {
        CellKey {
            sheet: self.sheet,
            row: self.bottom,
            column: self.right,
        }
    }

    pub const fn row_count(self) -> u32 {
        self.bottom - self.top + 1
    }

    pub const fn column_count(self) -> u32 {
        self.right - self.left + 1
    }

    /// The number of cells, which for a full sheet (2^34) exceeds `u32`.
    pub const fn cell_count(self) -> u64 {
        self.row_count() as u64 * self.column_count() as u64
    }

    /// Whether `key` lies inside this area (on the same sheet).
    pub fn contains(self, key: CellKey) -> bool {
        key.sheet == self.sheet
            && (self.top..=self.bottom).contains(&key.row)
            && (self.left..=self.right).contains(&key.column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::IdAllocator;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_area_normalizes_its_corners_and_counts_its_cells() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        let b3 = CellKey::new(sheet, 3, 2).unwrap();
        let d1 = CellKey::new(sheet, 1, 4).unwrap();
        let area = GridArea::new(b3, d1).unwrap();
        assert_eq!(area.first(), CellKey::new(sheet, 1, 2).unwrap());
        assert_eq!(area.last(), CellKey::new(sheet, 3, 4).unwrap());
        assert_eq!(
            (area.row_count(), area.column_count(), area.cell_count()),
            (3, 3, 9)
        );
        assert!(area.contains(CellKey::new(sheet, 2, 3).unwrap()));
        assert!(!area.contains(CellKey::new(sheet, 4, 3).unwrap()));
        assert_eq!(GridArea::cell(b3).cell_count(), 1);
        assert_eq!(GridArea::cell(b3).first(), b3);
    }
}

/// A cell or area selection: the active cell and the selected areas.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub active: Option<CellKey>,
    pub areas: Vec<GridArea>,
}

impl Selection {
    /// One cell, active and selected.
    pub fn cell(at: CellKey) -> Selection {
        Selection {
            active: Some(at),
            areas: vec![GridArea::cell(at)],
        }
    }
}

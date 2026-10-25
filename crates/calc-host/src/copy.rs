//! Copy and paste, and fill (dc-09v): the cell edits a copy of one area into another makes, as
//! Excel's copy and paste writes them. A value is copied as it is; a formula is relocated, its
//! relative references keeping their offsets and its absolute ones staying
//! ([`calc_engine::relocate_formula`]); an empty source cell clears its target. A target area
//! that is a whole multiple of the source's shape is filled by repeating the source, which is how
//! a fill down or right copies its first row or column; a single target cell takes the source's
//! shape from it.
//!
//! Every edit is read from the revision before the copy, so a copy whose target overlaps its source
//! copies what the source held, and the edits commit as one transaction and one undo step.
//!
//! Left as Excel's copy does differently, for now: the cells an array formula fills beside its
//! anchor are copied as empty (Excel copies a legacy array only whole), formats are not copied
//! (no command changes formats yet), and cut is not built.

use crate::command::CellEdit;
use crate::state::HostState;
use calc_doc_model::CellInput;
use calc_doc_model::value::{CellKey, GridArea};
use std::fmt;

/// Why a copy made no edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyRefusal {
    /// The source and the target are on sheets the workbook does not hold.
    UnknownSheet,
    /// The target is not a whole multiple of the source's shape (Excel: "the copy area and the
    /// paste area are not the same size").
    Shape {
        source: (u32, u32),
        target: (u32, u32),
    },
    /// The target reaches past the grid.
    PastTheGrid,
    /// The copy would make more edits than a copy takes at once.
    TooLarge { cells: u64 },
    /// A source formula's text does not parse, so it cannot be relocated.
    Unrelocatable { at: CellKey },
}

impl fmt::Display for CopyRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CopyRefusal::UnknownSheet => {
                f.write_str("the copy names a sheet the workbook does not hold")
            }
            CopyRefusal::Shape { source, target } => write!(
                f,
                "the paste area ({} by {}) is not a whole multiple of the copy area ({} by {})",
                target.0, target.1, source.0, source.1
            ),
            CopyRefusal::PastTheGrid => f.write_str("the paste area reaches past the grid"),
            CopyRefusal::TooLarge { cells } => {
                write!(
                    f,
                    "the paste area holds {cells} cells, more than a copy writes at once"
                )
            }
            CopyRefusal::Unrelocatable { at } => write!(
                f,
                "the formula at row {} column {} does not parse, so it cannot be copied",
                at.row(),
                at.column()
            ),
        }
    }
}

impl std::error::Error for CopyRefusal {}

/// The most cells one copy writes.
pub const COPY_LIMIT: u64 = 1_000_000;

/// The edits a copy of `from` into `to` makes over the current revision of `host`.
pub(crate) fn copy_edits(
    host: &HostState,
    from: GridArea,
    to: GridArea,
) -> Result<Vec<CellEdit>, CopyRefusal> {
    let model = &host.model;
    if model.sheet_by_id(from.sheet()).is_none() || model.sheet_by_id(to.sheet()).is_none() {
        return Err(CopyRefusal::UnknownSheet);
    }
    let (source_rows, source_cols) = (from.row_count(), from.column_count());
    // A single target cell takes the source's shape.
    let to = if to.row_count() == 1 && to.column_count() == 1 {
        let first = to.first();
        let last = CellKey::new(
            to.sheet(),
            first.row() + source_rows - 1,
            first.column() + source_cols - 1,
        )
        .map_err(|_| CopyRefusal::PastTheGrid)?;
        GridArea::new(first, last).expect("both corners are on the target's sheet")
    } else {
        to
    };
    if to.row_count() % source_rows != 0 || to.column_count() % source_cols != 0 {
        return Err(CopyRefusal::Shape {
            source: (source_rows, source_cols),
            target: (to.row_count(), to.column_count()),
        });
    }
    if to.cell_count() > COPY_LIMIT {
        return Err(CopyRefusal::TooLarge {
            cells: to.cell_count(),
        });
    }
    let mut edits = Vec::with_capacity(to.cell_count() as usize);
    let (top, left) = (to.first().row(), to.first().column());
    for row in top..=to.last().row() {
        for column in left..=to.last().column() {
            let target = CellKey::new(to.sheet(), row, column).expect("inside the target area");
            let source = CellKey::new(
                from.sheet(),
                from.first().row() + (row - top) % source_rows,
                from.first().column() + (column - left) % source_cols,
            )
            .expect("inside the source area");
            let edit = match model.cell(source) {
                None => CellEdit::Clear {
                    area: GridArea::cell(target),
                },
                Some(CellInput::Formula(formula)) => {
                    let formula = calc_engine::relocate_formula(model, formula, source, target)
                        .ok_or(CopyRefusal::Unrelocatable { at: source })?;
                    CellEdit::Set {
                        at: target,
                        input: CellInput::Formula(formula),
                    }
                }
                Some(input) => CellEdit::Set {
                    at: target,
                    input: input.clone(),
                },
            };
            edits.push(edit);
        }
    }
    Ok(edits)
}

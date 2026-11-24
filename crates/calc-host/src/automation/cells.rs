//! Range structure members: Insert, Delete (dc-oxh8w.4). Whole rows and columns go through the
//! axis edit, any other range through the cell shift, so references adjust as the Insert and Delete
//! commands of the grid do.
use super::*;
use crate::{Axis, AxisChange, AxisEdit, SkinIntent, WorkbookCommand};
use calc_value::{MAX_COLUMNS, MAX_ROWS};

const XL_SHIFT_DOWN: i64 = -4121;
const XL_SHIFT_TO_RIGHT: i64 = -4161;
const XL_SHIFT_UP: i64 = -4162;
const XL_SHIFT_TO_LEFT: i64 = -4159;

impl WorkbookAutomation {
    pub(super) fn register_cells(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Insert", InvokeKind::Method, Self::range_insert);
        handlers.on("Range", "Delete", InvokeKind::Method, Self::range_delete);
    }

    fn range_insert(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.shift_range(target, call, true)
    }
    fn range_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.shift_range(target, call, false)
    }

    /// Insert or Delete: whole rows or columns shift the sheet's axis, other ranges shift cells along the
    /// requested direction, or, with Shift omitted, the way the range's shape says (wider than tall:
    /// vertically; taller than wide: horizontally).
    fn shift_range(&self, target: &ObjectRef, call: &Call<'_>, insert: bool) -> Reply {
        let max_args = if insert { 2 } else { 1 };
        if call.args.len() > max_args {
            return Err(geometry::bad_count());
        }
        if insert {
            match call.arg(1) {
                AutoValue::Missing | AutoValue::Empty => {}
                other if geometry::integer(other)? == 0 => {}
                _ => {
                    return Err(AutoError::not_modelled(
                        "Range.Insert CopyOrigin xlFormatFromRightOrBelow",
                        OWNER,
                    ));
                }
            }
        }
        let shift = match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty => None,
            other => Some(geometry::integer(other)?),
        };
        self.mutate_range(target, call, |_, range| {
            let area = range.areas()[0];
            let sheet = area.sheet();
            let (first, last) = (area.first(), area.last());
            let whole_rows = first.column() == 1 && last.column() == MAX_COLUMNS;
            let whole_columns = first.row() == 1 && last.row() == MAX_ROWS;
            let change = if insert {
                AxisChange::Insert
            } else {
                AxisChange::Delete
            };
            let axis_edit = |axis, first, count| {
                AxisEdit::new(sheet, axis, change, first, count)
                    .map(SkinIntent::EditAxis)
                    .map_err(|_| AutoError::excel(1004, "Application-defined or object-defined error"))
            };
            if whole_rows && !whole_columns {
                return axis_edit(Axis::Rows, first.row(), area.row_count());
            }
            if whole_columns {
                return axis_edit(Axis::Columns, first.column(), area.column_count());
            }
            let (vertical, horizontal) = if insert {
                (XL_SHIFT_DOWN, XL_SHIFT_TO_RIGHT)
            } else {
                (XL_SHIFT_UP, XL_SHIFT_TO_LEFT)
            };
            let down = match shift {
                Some(s) if s == vertical => true,
                Some(s) if s == horizontal => false,
                Some(_) => {
                    return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
                }
                None => area.column_count() >= area.row_count() || area.column_count() == 1 && area.row_count() == 1,
            };
            Ok(SkinIntent::Command(WorkbookCommand::ShiftCells { area, down, insert }))
        })?;
        Ok(AutoValue::Bool(true))
    }
}

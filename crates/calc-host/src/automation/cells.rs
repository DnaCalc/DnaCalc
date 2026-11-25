//! Range structure members: Insert, Delete (dc-oxh8w.4). Whole rows and columns go through the
//! axis edit, any other range through the cell shift, so references adjust as the Insert and Delete
//! commands of the grid do.
use super::*;
use crate::{SortKey, SortOptions, Axis, AxisChange, AxisEdit, SkinIntent, WorkbookCommand};
use calc_value::{MAX_COLUMNS, MAX_ROWS};

/// The open bead that owns the structural remainders this file refuses.
const OWNER: &str = "dc-oxh8w.43";

const XL_SHIFT_DOWN: i64 = -4121;
const XL_SHIFT_TO_RIGHT: i64 = -4161;
const XL_SHIFT_UP: i64 = -4162;
const XL_SHIFT_TO_LEFT: i64 = -4159;

impl WorkbookAutomation {
    pub(super) fn register_cells(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Insert", InvokeKind::Method, Self::range_insert);
        handlers.on("Range", "Delete", InvokeKind::Method, Self::range_delete);
        handlers.on("Range", "Copy", InvokeKind::Method, Self::range_copy);
        handlers.on("Range", "Cut", InvokeKind::Method, Self::range_cut);
        handlers.on("Range", "Sort", InvokeKind::Method, Self::range_sort);
    }

    fn range_insert(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.shift_range(target, call, true)
    }
    fn range_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.shift_range(target, call, false)
    }

    fn range_copy(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.copy_range(target, call, false)
    }
    fn range_cut(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.copy_range(target, call, true)
    }

    /// Copy or Cut with a Destination: the copy and paste of the grid (values, formulas relocated, formats;
    /// a cut also moves references). Without a Destination the source goes to the application's clipboard (clipboard.rs).
    fn copy_range(&self, target: &ObjectRef, call: &Call<'_>, cut: bool) -> Reply {
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        let AutoValue::Object(destination) = call.arg(0) else {
            return if call.arg(0).is_missing() {
                self.copy_to_clipboard(target, cut)
            } else {
                Err(geometry::type_mismatch())
            };
        };
        let held = match &destination.key {
            ObjectKey::Range(held) | ObjectKey::RangeView { reference: held, .. } => held.clone(),
            _ => return Err(geometry::type_mismatch()),
        };
        // A Copy only reads its source: the destination is what is written (a Cut also clears the source).
        self.mutate(target, call, |session, range| {
            let range = range.ok_or_else(geometry::type_mismatch)?;
            if range.area_count() != 1 {
                return Err(AutoError::not_modelled(
                    format!("{} multi-area write", call.label()),
                    "dc-1nbp.6",
                ));
            }
            let to = self.resolve(&mut Access::Session(session, false), &held)?;
            if to.area_count() != 1 {
                return Err(AutoError::not_modelled("Range.Copy to several areas", OWNER));
            }
            self.guard_locked(session, &to)?;
            if cut {
                self.guard_locked(session, &range)?;
            }
            Ok(SkinIntent::Copy {
                from: range.areas()[0],
                to: to.areas()[0],
                cut,
            })
        })?;
        Ok(AutoValue::Bool(true))
    }

    /// Insert or Delete: whole rows or columns shift the sheet's axis, other ranges shift cells along the
    /// requested direction, or, with Shift omitted, the way the range's shape says (wider than tall:
    /// vertically; taller than wide: horizontally).
    fn shift_range(&self, target: &ObjectRef, call: &Call<'_>, insert: bool) -> Reply {
        let max_args = if insert { 2 } else { 1 };
        if call.args.len() > max_args {
            return Err(geometry::bad_count());
        }
        // CopyOrigin: xlFormatFromLeftOrAbove (0, the default) or xlFormatFromRightOrBelow (1).
        let from_after = if insert {
            match call.arg(1) {
                AutoValue::Missing | AutoValue::Empty => false,
                other => match geometry::integer(other)? {
                    0 => false,
                    1 => true,
                    _ => return Err(AutoError::excel(1004, "Application-defined or object-defined error")),
                },
            }
        } else {
            false
        };
        if let Some(other) = Some(call.arg(0)).filter(|a| !a.is_missing() && !matches!(a, AutoValue::Empty)) {
            if ![XL_SHIFT_DOWN, XL_SHIFT_TO_RIGHT, XL_SHIFT_UP, XL_SHIFT_TO_LEFT].contains(&geometry::integer(other)?) {
                return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
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
            // The whole sheet is every row: deleting it clears every cell and adjusts references as Excel does.
            if (whole_rows || whole_columns) && from_after {
                return Err(AutoError::not_modelled(
                    "Range.Insert CopyOrigin xlFormatFromRightOrBelow on whole rows or columns",
                    OWNER,
                ));
            }
            if whole_rows {
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
            Ok(SkinIntent::Command(WorkbookCommand::ShiftCells {
                area,
                down,
                insert,
                from_after,
            }))
        })?;
        Ok(AutoValue::Bool(true))
    }
}

impl WorkbookAutomation {
    /// Range.Sort over a multi-cell range: up to three keys given as ranges (the key's first column),
    /// Order, Header, MatchCase. Sorting columns, custom lists, data options and a sort of one cell
    /// (which Excel widens to the current region) refuse typed.
    fn range_sort(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 16 {
            return Err(geometry::bad_count());
        }
        let refuse = |what: &str| Err(AutoError::not_modelled(format!("Range.Sort {what}"), OWNER));
        let given = |index: usize| !call.arg(index).is_missing();
        let number = |index: usize, default: i64| -> Result<i64, AutoError> {
            if given(index) { geometry::integer(call.arg(index)) } else { Ok(default) }
        };
        if given(3) || given(8) || given(15) {
            return refuse("Type, OrderCustom and SubField1");
        }
        let bad = || AutoError::excel(1004, "Application-defined or object-defined error");
        // xlGuess 0, xlYes 1, xlNo 2; xlSortColumns 1, xlSortRows 2; xlPinYin 1, xlStroke 2; DataOption 0 or 1.
        let header = number(7, 2)?;
        let orientation = number(10, 2)?;
        let method = number(11, 1)?;
        let (order_a, order_b, order_c) = (number(1, 1)?, number(4, 1)?, number(6, 1)?);
        if !(0..=2).contains(&header)
            || !(1..=2).contains(&orientation)
            || !(1..=2).contains(&method)
            || ![number(12, 0)?, number(13, 0)?, number(14, 0)?].iter().all(|d| (0..=1).contains(d))
            || ![order_a, order_b, order_c].iter().all(|o| (1..=2).contains(o))
        {
            return Err(bad());
        }
        if orientation != 2 {
            return refuse("Orientation other than xlSortRows");
        }
        if method != 1 || number(12, 0)? != 0 || number(13, 0)? != 0 || number(14, 0)? != 0 {
            return refuse("SortMethod and DataOption");
        }
        let header = match header {
            1 => true,
            2 => false,
            _ => return refuse("Header xlGuess"),
        };
        let case_sensitive = match call.arg(9) {
            AutoValue::Missing | AutoValue::Empty => false,
            AutoValue::Bool(value) => *value,
            _ => return Err(geometry::type_mismatch()),
        };
        let mut keys = Vec::new();
        for (key, order) in [(0usize, 1usize), (2, 4), (5, 6)] {
            let key = call.arg(key);
            if key.is_missing() {
                continue;
            }
            let AutoValue::Object(ObjectRef {
                key: ObjectKey::Range(held) | ObjectKey::RangeView { reference: held, .. },
                ..
            }) = key
            else {
                return refuse("with a key that is not a Range");
            };
            let descending = match number(order, 1)? {
                1 => false,
                2 => true,
                _ => return Err(AutoError::excel(1004, "Application-defined or object-defined error")),
            };
            keys.push((held.clone(), descending));
        }
        if keys.is_empty() {
            return refuse("without Key1");
        }
        self.mutate_range(target, call, |session, range| {
            let mut area = range.areas()[0];
            // Sorting one cell sorts its current region, as Excel does.
            if area.cell_count() == 1 {
                area = Self::current_region(session.host().model(), area.first());
                if area.cell_count() == 1 {
                    return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
                }
            }
            // The region a one-cell Sort widened to is written too.
            self.guard_area(session, area)?;
            let mut sort_keys = Vec::new();
            for (held, descending) in &keys {
                let key = self.resolve(&mut Access::Session(session, false), held)?;
                let column = key.areas()[0].first().column();
                if key.areas()[0].sheet() != area.sheet()
                    || column < area.first().column()
                    || column > area.last().column()
                {
                    return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
                }
                sort_keys.push(SortKey { column, descending: *descending });
            }
            Ok(SkinIntent::Command(WorkbookCommand::SortRowsWithOptions {
                area,
                keys: sort_keys,
                header,
                revision: session.host().revision().get(),
                options: SortOptions { case_sensitive, custom_order: Vec::new() },
            }))
        })?;
        Ok(AutoValue::Bool(true))
    }
}

impl WorkbookAutomation {
    /// The block of non-empty cells around `at`, grown through every neighbouring non-empty cell (Excel's CurrentRegion).
    pub(super) fn current_region(model: &calc_doc_model::ModelRoot, at: CellKey) -> GridArea {
        let sheet = at.sheet();
        let occupied = |row: u32, column: u32| {
            CellKey::new(sheet, row, column).is_ok_and(|key| model.cell(key).is_some())
        };
        let (mut top, mut left, mut bottom, mut right) = (at.row(), at.column(), at.row(), at.column());
        loop {
            let (t, l, b, r) = (top.saturating_sub(1).max(1), left.saturating_sub(1).max(1), (bottom + 1).min(MAX_ROWS), (right + 1).min(MAX_COLUMNS));
            let ring_has = |row: u32, column: u32| occupied(row, column);
            let mut grown = (top, left, bottom, right);
            if t < top && (l..=r).any(|c| ring_has(t, c)) {
                grown.0 = t;
            }
            if b > bottom && (l..=r).any(|c| ring_has(b, c)) {
                grown.2 = b;
            }
            if l < left && (t..=b).any(|row| ring_has(row, l)) {
                grown.1 = l;
            }
            if r > right && (t..=b).any(|row| ring_has(row, r)) {
                grown.3 = r;
            }
            if grown == (top, left, bottom, right) {
                break;
            }
            (top, left, bottom, right) = grown;
        }
        let corner = |row, column| CellKey::new(sheet, row, column).expect("inside the grid");
        GridArea::new(corner(top, left), corner(bottom, right)).expect("an ordered area")
    }
}

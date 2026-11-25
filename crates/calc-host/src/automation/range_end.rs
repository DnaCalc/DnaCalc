//! `Range.End(Direction)` (dc-oxh8w.47): the Ctrl+arrow rules over the occupied cells (inputs, formulas, arrays,
//! spills). Inside a block the result is the block's last cell in the direction; at a block's edge, or from an
//! empty cell, it is the next occupied cell, else the sheet's edge. A multi-cell or multi-area range refuses.
use super::*;

const XL_DOWN: i64 = -4121;
const XL_UP: i64 = -4162;
const XL_TO_LEFT: i64 = -4159;
const XL_TO_RIGHT: i64 = -4161;
/// Excel answers for a multi-cell range are not recorded (dc-oxh8w.74).
const OWNER: &str = "dc-oxh8w.74";

/// Where Ctrl+arrow lands along one line: `runs` are the occupied spans (start, end), `at` the start position,
/// `forward` the direction, `limit` the last position (or 1 going backwards).
fn end_of_line(mut runs: Vec<(u32, u32)>, at: u32, forward: bool, limit: u32) -> u32 {
    runs.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (start, end) in runs {
        match merged.last_mut() {
            Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let occupied = |position: u32| merged.iter().find(|(s, e)| *s <= position && position <= *e).copied();
    let next = if forward { at.checked_add(1).filter(|n| *n <= limit) } else { at.checked_sub(1).filter(|n| *n >= 1) };
    if let (Some(run), Some(next)) = (occupied(at), next) {
        if occupied(next).is_some() {
            return if forward { run.1 } else { run.0 };
        }
    }
    if forward {
        merged.iter().find(|(s, _)| *s > at).map_or(limit, |(s, _)| *s)
    } else {
        merged.iter().rev().find(|(_, e)| *e < at).map_or(1, |(_, e)| *e)
    }
}

impl WorkbookAutomation {
    pub(super) fn register_range_end(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "End", InvokeKind::Get, Self::range_end);
    }

    fn range_end(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 || call.arg(0).is_missing() {
            return Err(geometry::bad_count());
        }
        let direction = geometry::integer(call.arg(0))?;
        if ![XL_DOWN, XL_UP, XL_TO_LEFT, XL_TO_RIGHT].contains(&direction) {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        }
        let mut landed: Option<CellKey> = None;
        self.with_range(target, |access, _, range, _| {
            if range.area_count() != 1 || range.areas()[0].cell_count() != 1 {
                return Err(AutoError::not_modelled("Range.End of a multi-cell range", OWNER));
            }
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Range.End inside a worksheet invocation", OWNER));
            };
            let start = range.areas()[0].first();
            let sheet = start.sheet();
            let areas = session.host().occupied_areas(sheet);
            let vertical = direction == XL_DOWN || direction == XL_UP;
            let forward = direction == XL_DOWN || direction == XL_TO_RIGHT;
            let (at, limit) = if vertical {
                (start.row(), calc_value::MAX_ROWS)
            } else {
                (start.column(), calc_value::MAX_COLUMNS)
            };
            let runs: Vec<(u32, u32)> = areas
                .iter()
                .filter(|a| {
                    if vertical {
                        a.first().column() <= start.column() && start.column() <= a.last().column()
                    } else {
                        a.first().row() <= start.row() && start.row() <= a.last().row()
                    }
                })
                .map(|a| {
                    if vertical { (a.first().row(), a.last().row()) } else { (a.first().column(), a.last().column()) }
                })
                .collect();
            let to = end_of_line(runs, at, forward, limit);
            let (row, column) = if vertical { (to, start.column()) } else { (start.row(), to) };
            landed = Some(CellKey::new(sheet, row, column).map_err(|_| geometry::type_mismatch())?);
            Ok(AutoValue::Empty)
        })?;
        let at = landed.ok_or_else(geometry::type_mismatch)?;
        self.with_target(target, |access, _| self.acquire(access, &ReferenceLike::area(GridArea::cell(at)), RangeAxis::Cells))
    }
}

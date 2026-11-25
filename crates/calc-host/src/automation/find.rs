//! `Range.Find`, `Range.FindNext` and `Range.FindPrevious` (dc-oxh8w.15), one handler each. The search
//! visits the cells of the range in SearchOrder, starting after `After` (default: the range's first cell,
//! which is therefore visited last) and wrapping round; matching is `search::matches` (Excel's `*`, `?`
//! and `~` syntax, always on for Find). LookIn, LookAt and SearchOrder are saved between calls the way
//! Excel saves them, and FindNext / FindPrevious repeat the last What. A miss is Nothing.
use super::*;
use std::collections::BTreeSet;

const XL_FORMULAS: i64 = -4123;
const XL_VALUES: i64 = -4163;
const XL_WHOLE: i64 = 1;
const XL_PART: i64 = 2;
const XL_BY_ROWS: i64 = 1;
const XL_BY_COLUMNS: i64 = 2;
const XL_NEXT: i64 = 1;
const XL_PREVIOUS: i64 = 2;
const FIND_CELLS: u64 = 1_048_576;

/// What a Find call leaves for the next one.
#[derive(Clone, Debug)]
pub(super) struct FindState {
    look_in: i64,
    look_at: i64,
    order: i64,
    what: Option<String>,
    match_case: bool,
}

impl Default for FindState {
    fn default() -> Self {
        // Excel's initial settings: look in formulas, at part of the cell, by rows.
        Self { look_in: XL_FORMULAS, look_at: XL_PART, order: XL_BY_ROWS, what: None, match_case: false }
    }
}

fn error_1004() -> AutoError {
    AutoError::excel(1004, "Application-defined or object-defined error")
}

impl WorkbookAutomation {
    pub(super) fn register_find(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Find", InvokeKind::Method, Self::range_find);
        handlers.on("Range", "FindNext", InvokeKind::Method, Self::range_find_next);
        handlers.on("Range", "FindPrevious", InvokeKind::Method, Self::range_find_previous);
    }

    fn range_find(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 9 {
            return Err(geometry::bad_count());
        }
        let given = |index: usize| !call.arg(index).is_missing() && !matches!(call.arg(index), AutoValue::Empty);
        let number = |index: usize, default: i64| -> Result<i64, AutoError> {
            if given(index) { geometry::integer(call.arg(index)) } else { Ok(default) }
        };
        let flag = |index: usize| -> Result<bool, AutoError> {
            match call.arg(index) {
                AutoValue::Missing | AutoValue::Empty => Ok(false),
                AutoValue::Bool(value) => Ok(*value),
                _ => Err(geometry::type_mismatch()),
            }
        };
        let what = match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty => return Err(error_1004()),
            other => Self::entry_of(other)?,
        };
        let saved = self.find.borrow().clone();
        let look_in = number(2, saved.look_in)?;
        let look_at = number(3, saved.look_at)?;
        let order = number(4, saved.order)?;
        let direction = number(5, XL_NEXT)?;
        let match_case = flag(6)?;
        if look_in != XL_FORMULAS && look_in != XL_VALUES {
            return Err(AutoError::not_modelled("Range.Find LookIn comments", OWNER));
        }
        if look_at != XL_WHOLE && look_at != XL_PART || !matches!(order, XL_BY_ROWS | XL_BY_COLUMNS) {
            return Err(error_1004());
        }
        if direction != XL_NEXT && direction != XL_PREVIOUS {
            return Err(error_1004());
        }
        if flag(7)? {
            return Err(AutoError::not_modelled("Range.Find MatchByte", OWNER));
        }
        if flag(8)? {
            return Err(AutoError::not_modelled("Range.Find SearchFormat", OWNER));
        }
        let state = FindState { look_in, look_at, order, what: Some(what), match_case };
        *self.find.borrow_mut() = state.clone();
        self.find_from(target, call.arg(1), &state, direction == XL_PREVIOUS)
    }

    fn range_find_next(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.find_again(target, call, false)
    }
    fn range_find_previous(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.find_again(target, call, true)
    }
    fn find_again(&self, target: &ObjectRef, call: &Call<'_>, previous: bool) -> Reply {
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        let state = self.find.borrow().clone();
        if state.what.is_none() {
            return Ok(AutoValue::Empty);
        }
        self.find_from(target, call.arg(0), &state, previous)
    }

    /// The first match visiting the range's cells from `after` (exclusive, wrapping), forwards or backwards.
    fn find_from(&self, target: &ObjectRef, after: &AutoValue, state: &FindState, previous: bool) -> Reply {
        let what = state.what.clone().unwrap_or_default();
        let mut hit: Option<CellKey> = None;
        self.with_range(target, |access, _, range, _| {
            if range.area_count() != 1 {
                return Err(AutoError::not_modelled("Range.Find over several areas", OWNER));
            }
            let area = range.areas()[0];
            // After: one cell of the searched range.
            let start = match after {
                AutoValue::Missing | AutoValue::Empty => area.first(),
                AutoValue::Object(ObjectRef {
                    key: ObjectKey::Range(held) | ObjectKey::RangeView { reference: held, .. },
                    ..
                }) => {
                    let Access::Session(session, _) = access else {
                        return Err(AutoError::not_modelled("Range.Find inside a worksheet invocation", OWNER));
                    };
                    let cell = self.resolve(&mut Access::Session(session, false), held)?;
                    let inside = cell.area_count() == 1
                        && cell.areas()[0].cell_count() == 1
                        && area.sheet() == cell.areas()[0].sheet()
                        && area.contains(cell.areas()[0].first());
                    if !inside {
                        return Err(error_1004());
                    }
                    cell.areas()[0].first()
                }
                _ => return Err(geometry::type_mismatch()),
            };
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Range.Find inside a worksheet invocation", OWNER));
            };
            let host = session.host();
            let key = |at: CellKey| {
                if state.order == XL_BY_ROWS { (at.row(), at.column()) } else { (at.column(), at.row()) }
            };
            // The occupied cells of the range, in search order.
            let mut cells: BTreeSet<(u32, u32)> = BTreeSet::new();
            let mut candidates: Vec<GridArea> = Vec::new();
            if state.look_in == XL_FORMULAS {
                candidates.extend(host.input_cells(area.sheet()).into_iter().map(GridArea::cell));
            } else {
                candidates.extend(host.occupied_areas(area.sheet()));
            }
            let mut count = 0u64;
            for occupied in candidates {
                let (Some(first), Some(last)) = (
                    CellKey::new(
                        area.sheet(),
                        occupied.first().row().max(area.first().row()),
                        occupied.first().column().max(area.first().column()),
                    )
                    .ok(),
                    CellKey::new(
                        area.sheet(),
                        occupied.last().row().min(area.last().row()),
                        occupied.last().column().min(area.last().column()),
                    )
                    .ok(),
                ) else {
                    continue;
                };
                if first.row() > last.row() || first.column() > last.column() {
                    continue;
                }
                for row in first.row()..=last.row() {
                    for column in first.column()..=last.column() {
                        count += 1;
                        if count > FIND_CELLS {
                            return Err(AutoError::not_modelled("Range.Find materialization limit", OWNER));
                        }
                        cells.insert(key(CellKey::new(area.sheet(), row, column).expect("cell in valid area")));
                    }
                }
            }
            let to_cell = |(a, b): (u32, u32)| {
                let (row, column) = if state.order == XL_BY_ROWS { (a, b) } else { (b, a) };
                CellKey::new(area.sheet(), row, column).expect("cell in valid area")
            };
            let pivot = key(start);
            let mut order: Vec<(u32, u32)> = cells.range((std::ops::Bound::Excluded(pivot), std::ops::Bound::Unbounded)).copied().collect();
            order.extend(cells.range(..=pivot).copied());
            if previous {
                // Before the start cell, nearest first, then the rest of the range from its end, the start cell last.
                let mut backwards: Vec<(u32, u32)> = cells.range(..pivot).rev().copied().collect();
                backwards.extend(cells.range(pivot..).rev().copied());
                order = backwards;
            }
            let projected = crate::project(
                host,
                &ProjectionRequest { cells: order.iter().map(|&k| to_cell(k)).collect() },
            )
            .map_err(|e| AutoError::not_modelled(e.to_string(), OWNER))?;
            // Projection answers in the order asked; match each cell's text.
            for (cell, &k) in projected.cells().iter().zip(&order) {
                let at = to_cell(k);
                let text = if state.look_in == XL_FORMULAS {
                    super::formula::formula_entry_text(cell.input())
                } else {
                    Self::text_of_cell(host, cell, at)?
                };
                if text.is_empty() {
                    continue;
                }
                if crate::search::matches(&text, &what, state.match_case, state.look_at == XL_WHOLE, true) {
                    hit = Some(at);
                    break;
                }
            }
            Ok(AutoValue::Empty)
        })?;
        match hit {
            None => Ok(AutoValue::Empty),
            Some(at) => self.with_target(target, |access, _| {
                self.acquire(access, &ReferenceLike::area(GridArea::cell(at)), RangeAxis::Cells)
            }),
        }
    }
}

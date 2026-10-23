//! The reference operators and implicit intersection, on typed references.
//!
//! Ported from OxFunc's `operator_reference_family.rs` and `op_implicit_intersection.rs` (OxFunc
//! surfaces `FUNC.OP_RANGE_REF`, `FUNC.OP_INTERSECTION_REF`, `FUNC.OP_UNION_REF` and
//! `FUNC.OP_IMPLICIT_INTERSECTION`), which compose A1 reference text through a reference-system
//! provider; here the operands are calc-value's typed [`ReferenceLike`] areas and the composition
//! is done directly.
//!
//! - The range operator gives the smallest area holding every area of both operands.
//! - The intersection operator gives the cells both operands hold, area by area; none is #NULL!.
//! - The union operator gives the areas of every operand, in order.
//! - An operand that is an error is the result (the first such, left to right); an operand that
//!   is no reference, and operands on two sheets, are #VALUE!. Measured on Excel 16.0 on
//!   2026-10-23: `=SUM((S1!A1:A2 S2!A1:A2))` and `=SUM((S1!A1,S2!A1))` are #VALUE!,
//!   `=SUM((A1:A2 B1:B2))` is #NULL! and `=ROWS(A1:B2:C4)` is 4.
//!
//! Implicit intersection (`@`) takes one value: a reference's one cell, or for a larger area the
//! cell in the formula's row (when the area spans more than one row) and column (when it spans
//! more than one column), #VALUE! when the formula's row or column lies outside it or the
//! reference has more than one area; an array's first element; any other value as it is.
//! Measured on Excel 16.0 on 2026-10-23: `=@S1!C1:E10` entered on another sheet in D5 gives
//! S1!D5, in F5 and D11 #VALUE!; `=@{7,8}` is 7; `=@S1!A1:A10` in row 5 over a blank S1!A5 is 0.
//! OxFunc gave #VALUE! for every area of more than one row and column; Excel does not.

use calc_func::value::{
    CalcValue, CellKey, CoreValue, GridArea, ReferenceLike, WorksheetErrorCode,
};

/// An operand of a reference operator: its reference, or the error value that is the result.
fn reference(operand: &CalcValue) -> Result<&ReferenceLike, CalcValue> {
    match operand.core() {
        CoreValue::Reference(reference) => Ok(reference),
        CoreValue::Error(_) if operand.callable_value().is_none() => Err(operand.clone()),
        _ => Err(CalcValue::error(WorksheetErrorCode::Value)),
    }
}

/// The references of `operands`, all on one sheet, or the value that is the result.
fn references(operands: &[CalcValue]) -> Result<Vec<&ReferenceLike>, CalcValue> {
    let mut out = Vec::with_capacity(operands.len());
    for operand in operands {
        out.push(reference(operand)?);
    }
    let sheet = out[0].sheet();
    if out.iter().any(|reference| reference.sheet() != sheet) {
        return Err(CalcValue::error(WorksheetErrorCode::Value));
    }
    Ok(out)
}

/// `left:right`.
pub(super) fn range(left: &CalcValue, right: &CalcValue) -> CalcValue {
    let operands = [left.clone(), right.clone()];
    let references = match references(&operands) {
        Ok(references) => references,
        Err(value) => return value,
    };
    let mut areas = references.iter().flat_map(|reference| reference.areas());
    let first = *areas.next().expect("a reference holds an area");
    let (mut top, mut left, mut bottom, mut right) = bounds(first);
    for area in areas {
        let (t, l, b, r) = bounds(*area);
        top = top.min(t);
        left = left.min(l);
        bottom = bottom.max(b);
        right = right.max(r);
    }
    CalcValue::reference(ReferenceLike::area(area(first, top, left, bottom, right)))
}

/// `left right`.
pub(super) fn intersect(left: &CalcValue, right: &CalcValue) -> CalcValue {
    let operands = [left.clone(), right.clone()];
    let references = match references(&operands) {
        Ok(references) => references,
        Err(value) => return value,
    };
    let mut out = Vec::new();
    for a in references[0].areas() {
        for b in references[1].areas() {
            let (at, al, ab, ar) = bounds(*a);
            let (bt, bl, bb, br) = bounds(*b);
            let (top, left, bottom, right) = (at.max(bt), al.max(bl), ab.min(bb), ar.min(br));
            if top <= bottom && left <= right {
                out.push(area(*a, top, left, bottom, right));
            }
        }
    }
    match ReferenceLike::from_areas(out) {
        Ok(reference) => CalcValue::reference(reference),
        Err(_) => CalcValue::error(WorksheetErrorCode::Null),
    }
}

/// `(a,b,...)`.
pub(super) fn union(operands: &[CalcValue]) -> CalcValue {
    let references = match references(operands) {
        Ok(references) => references,
        Err(value) => return value,
    };
    let areas = references
        .iter()
        .flat_map(|reference| reference.areas().iter().copied())
        .collect();
    match ReferenceLike::from_areas(areas) {
        Ok(reference) => CalcValue::reference(reference),
        Err(_) => CalcValue::error(WorksheetErrorCode::Value),
    }
}

/// What implicit intersection takes of `value` for a formula at `at`: a reference to one cell, or
/// a value. An array's empty first element is the empty value.
pub(super) fn implicit(value: &CalcValue, at: CellKey) -> CalcValue {
    match value.core() {
        CoreValue::Reference(reference) => match intersect_cell(reference, at) {
            Some(cell) => CalcValue::reference(ReferenceLike::cell(cell)),
            None => CalcValue::error(WorksheetErrorCode::Value),
        },
        CoreValue::Array(array) => array
            .get(0, 0)
            .cloned()
            .expect("an array holds at least one element"),
        _ => value.clone(),
    }
}

/// The cell implicit intersection picks of `reference` for a formula at `at`, if any.
pub(super) fn intersect_cell(reference: &ReferenceLike, at: CellKey) -> Option<CellKey> {
    let [area] = reference.areas() else {
        return None;
    };
    let (top, left, bottom, right) = bounds(*area);
    let row = if top == bottom {
        top
    } else if (top..=bottom).contains(&at.row()) {
        at.row()
    } else {
        return None;
    };
    let column = if left == right {
        left
    } else if (left..=right).contains(&at.column()) {
        at.column()
    } else {
        return None;
    };
    CellKey::new(area.sheet(), row, column).ok()
}

fn bounds(area: GridArea) -> (u32, u32, u32, u32) {
    (
        area.first().row(),
        area.first().column(),
        area.last().row(),
        area.last().column(),
    )
}

/// The area of `like`'s sheet between the given rows and columns, which lie on the grid.
fn area(like: GridArea, top: u32, left: u32, bottom: u32, right: u32) -> GridArea {
    let sheet = like.sheet();
    GridArea::new(
        CellKey::new(sheet, top, left).expect("a corner of areas on the grid"),
        CellKey::new(sheet, bottom, right).expect("a corner of areas on the grid"),
    )
    .expect("both corners lie on one sheet")
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::value::{IdAllocator, SheetId};

    fn sheets() -> (SheetId, SheetId) {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        (ids.issue().unwrap(), ids.issue().unwrap())
    }

    fn at(sheet: SheetId, row: u32, column: u32) -> CellKey {
        CellKey::new(sheet, row, column).unwrap()
    }

    fn reference(sheet: SheetId, a: (u32, u32), b: (u32, u32)) -> CalcValue {
        CalcValue::reference(ReferenceLike::area(
            GridArea::new(at(sheet, a.0, a.1), at(sheet, b.0, b.1)).unwrap(),
        ))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reference_operators() {
        let (one, two) = sheets();
        // A1:B2:C4 is A1:C4.
        assert_eq!(
            range(
                &reference(one, (1, 1), (2, 2)),
                &reference(one, (4, 3), (4, 3))
            ),
            reference(one, (1, 1), (4, 3))
        );
        // A1:A2 B1:B2 is #NULL!; A1:B2 B2:C3 is B2.
        assert_eq!(
            intersect(
                &reference(one, (1, 1), (2, 1)),
                &reference(one, (1, 2), (2, 2))
            ),
            CalcValue::error(WorksheetErrorCode::Null)
        );
        assert_eq!(
            intersect(
                &reference(one, (1, 1), (2, 2)),
                &reference(one, (2, 2), (3, 3))
            ),
            reference(one, (2, 2), (2, 2))
        );
        // Two sheets: #VALUE!; an error operand is the result; a number is #VALUE!.
        assert_eq!(
            union(&[
                reference(one, (1, 1), (1, 1)),
                reference(two, (1, 1), (1, 1))
            ]),
            CalcValue::error(WorksheetErrorCode::Value)
        );
        assert_eq!(
            range(
                &CalcValue::error(WorksheetErrorCode::Ref),
                &reference(one, (1, 1), (1, 1))
            ),
            CalcValue::error(WorksheetErrorCode::Ref)
        );
        assert_eq!(
            intersect(&CalcValue::number(1.0), &reference(one, (1, 1), (1, 1))),
            CalcValue::error(WorksheetErrorCode::Value)
        );
        let joined = union(&[
            reference(one, (1, 1), (2, 1)),
            reference(one, (2, 2), (2, 2)),
        ]);
        assert_eq!(
            joined.as_reference().map(ReferenceLike::area_count),
            Some(2)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn implicit_intersection() {
        let (one, two) = sheets();
        let c1_e10 = reference(one, (1, 3), (10, 5));
        assert_eq!(
            implicit(&c1_e10, at(two, 5, 4)),
            reference(one, (5, 4), (5, 4))
        );
        for outside in [at(two, 5, 6), at(two, 11, 4)] {
            assert_eq!(
                implicit(&c1_e10, outside),
                CalcValue::error(WorksheetErrorCode::Value)
            );
        }
        let column = reference(one, (1, 1), (10, 1));
        assert_eq!(
            implicit(&column, at(two, 5, 9)),
            reference(one, (5, 1), (5, 1))
        );
        let array = CalcValue::array(
            calc_func::value::CalcArray::from_rows(vec![vec![
                CalcValue::number(7.0),
                CalcValue::number(8.0),
            ]])
            .unwrap(),
        );
        assert_eq!(implicit(&array, at(one, 1, 1)), CalcValue::number(7.0));
        assert_eq!(
            implicit(&CalcValue::number(3.0), at(one, 1, 1)),
            CalcValue::number(3.0)
        );
    }
}

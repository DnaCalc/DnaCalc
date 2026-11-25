//! The dynamic-array family fragment ported from OxFunc (functions/ at 6fca3518):
//! dynamic_array_reshape_family.rs (FILTER, SORT, SORTBY, UNIQUE, TAKE, DROP, CHOOSEROWS,
//! CHOOSECOLS, VSTACK, TOCOL, TOROW, WRAPROWS, WRAPCOLS, EXPAND), sequence.rs (SEQUENCE) and
//! hstack.rs (HSTACK). The shaping kernels are OxFunc's ([`crate::ported::dynamic_array_reshape_family`]);
//! each returns an array, which a formula spills (calc-engine owns the spill).
//!
//! An array argument is a value-or-reference parameter: a reference reaches the kernel as the
//! array of its cells' values (a blank cell as an empty value), an array as itself and a single
//! value as a one-by-one array. Counts, flags and pads are value parameters, so a formula lifts
//! them over an array, except EXPAND's pad: an array pad reaches its scalar-pad validation whole.
//! CHOOSEROWS' and CHOOSECOLS' selectors may be arrays, read in row-major
//! order. A result with no cell is #CALC!.
//!
//! SORT and SORTBY compare as the catalogue's comparison operators do ([`super::comparison`]):
//! numbers by Excel's 15-digit rule, text ignoring case under Excel's collation, FALSE before
//! TRUE; UNIQUE takes text equal under the same collation and numbers equal as binary64. OxFunc orders kinds as blank, numbers, text, logicals, errors,
//! and two errors by their code; this fragment keeps that order. A comparison the operators refuse
//! is refused here too.

use super::comparison::{self, Want};
use super::kit::{fail, grid, opt_flag, scalar_error};
use crate::call::EvalServices;
use crate::kernel::{Exit, Site, Step};
use crate::ported::dynamic_array_reshape_family::{
    self as k, CoercionError, DynamicArrayReshapeEvalError as E,
};
use crate::ported::text_kernels::date_integer_floor;
use crate::value::{
    ArrayShape, CalcArray, CalcValue, CellKey, CoreRef, GridArea, ReferenceLike, WorksheetErrorCode,
};
use std::cmp::Ordering;
use std::collections::HashMap;

use calc_catalogue::ported::arrays::{CODE, ENTRIES as DECLARATIONS};

/// W111 typed array-function controls distinguish a direct scalar source error from an error
/// held in an actual array or reference.
fn source_error(arguments: &[CalcValue]) -> Step<()> {
    scalar_error(&arguments[0])
}

/// The arguments as OxFunc's kernels take them: each reference read as the array of its values.
fn prepared(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<Vec<CalcValue>> {
    let mut out = Vec::with_capacity(a.len());
    for index in 0..a.len() {
        out.push(prepared_one(site, a, index, sv)?);
    }
    Ok(out)
}

fn prepared_one(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    if !matches!(a[index].core_ref(), CoreRef::Reference(_)) {
        return Ok(a[index].clone());
    }
    let g = grid(site, a, index, sv)?;
    let shape = ArrayShape {
        rows: g.rows,
        cols: g.cols,
    };
    CalcArray::new(shape, g.cells)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(Some(index)))
}

/// TOCOL (`column`) or TOROW of a reference of several areas (a 3-D reference): each area flattened
/// by the call's own ignore and scan arguments, in the reference's order (sheet by sheet), the
/// results joined into one column or row; `None` for any other first argument. Excel 16.0.20430
/// (2026-10-25, dc-6au), sheets Ja, Fe and Ma holding 1, 10 / 100 at A1, B1 / A2 (times 1, 2, 3):
/// `=TOCOL(Ja:Ma!A1:B1)` and `=TOROW(..)` give 1,10,2,20,3,30, and `=TOCOL(Ja:Ma!A1:B2,1)`, blanks
/// ignored, 1,10,100,2,20,200,3,30,300 (9 rows). An area whose cells are all left out adds
/// nothing; when every area does, the result is the kernel's own for an empty one.
fn by_area(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    kernel: fn(&[CalcValue]) -> Result<CalcValue, E>,
    column: bool,
) -> Step<Option<CalcValue>> {
    let Some(reference) = a[0].as_reference() else {
        return Ok(None);
    };
    if reference.areas().len() < 2 {
        return Ok(None);
    }
    let mut cells = Vec::new();
    let mut empty = None;
    for &area in reference.areas() {
        let mut args = a.to_vec();
        args[0] = CalcValue::reference(ReferenceLike::area(area));
        args[0] = prepared_one(site, &args, 0, sv)?;
        match result(site, kernel(&args)) {
            Ok(value) => match value.as_array() {
                Some(array) => cells.extend(array.iter_row_major().cloned()),
                None => cells.push(value),
            },
            // An area whose every cell is left out (#CALC!, an empty array).
            Err(Exit::Error(WorksheetErrorCode::Calc)) => {
                empty.get_or_insert(WorksheetErrorCode::Calc);
            }
            Err(other) => return Err(other),
        }
    }
    if cells.is_empty() {
        return match empty {
            Some(code) => fail(code),
            None => Ok(None),
        };
    }
    let count = cells.len();
    let shape = if column {
        ArrayShape {
            rows: count,
            cols: 1,
        }
    } else {
        ArrayShape {
            rows: 1,
            cols: count,
        }
    };
    Ok(Some(CalcValue::array(
        CalcArray::new(shape, cells).expect("one cell per item of the shape"),
    )))
}

/// A kernel's result: its array, or its failure as a worksheet error (a coercion calc-func does
/// not model is refused).
fn result<T>(site: &Site<'_>, r: Result<T, E>) -> Step<T> {
    match r {
        Ok(value) => Ok(value),
        Err(E::Preparation(CoercionError::NotModelled)) => Err(site.not_modelled(None)),
        Err(error) => fail(k::map_dynamic_array_reshape_error_to_ws(&error)),
    }
}

/// Runs `kernel` on the prepared arguments.
fn run(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    kernel: fn(&[CalcValue]) -> Result<CalcValue, E>,
) -> Step<CalcValue> {
    let args = prepared(site, a, sv)?;
    result(site, kernel(&args))
}

/// TAKE (`take`) or DROP of a reference to one area: the reference to the rows and columns kept,
/// or `None` for an array argument. Excel returns a reference here: `=_xlfn.DROP(Data!A1:A3,1)`
/// entered as a legacy formula outside rows 1 to 3 is #VALUE!, the implicit intersection of the
/// reference A2:A3 (the catalogue oracle's capture), not 5, the first of the values.
fn sub_area(site: &Site<'_>, a: &[CalcValue], take: bool) -> Step<Option<CalcValue>> {
    let Some(reference) = a[0].as_reference() else {
        return Ok(None);
    };
    let [area] = reference.areas() else {
        return Ok(None);
    };
    let (rows, cols) = (area.row_count() as usize, area.column_count() as usize);
    let (row_full, col_full) = if take {
        (rows as isize, cols as isize)
    } else {
        (0, 0)
    };
    let row_count = result(site, k::truncated_integer(a.get(1), row_full))?;
    let col_count = result(site, k::truncated_integer(a.get(2), col_full))?;
    let span = |len: usize, n: isize| {
        if take {
            k::take_span(len, n)
        } else {
            k::drop_span(len, n)
        }
    };
    let ((r0, r1), (c0, c1)) = match (span(rows, row_count), span(cols, col_count)) {
        (Ok(r), Ok(c)) => (r, c),
        (Err(error), _) | (_, Err(error)) => {
            return result::<CalcValue>(site, Err(error)).map(|_| None);
        }
    };
    let first = area.first();
    let corner = |row: usize, col: usize| {
        CellKey::new(
            area.sheet(),
            first.row() + row as u32,
            first.column() + col as u32,
        )
    };
    match (corner(r0, c0), corner(r1 - 1, c1 - 1)) {
        (Ok(top_left), Ok(bottom_right)) => match GridArea::new(top_left, bottom_right) {
            Ok(kept) => Ok(Some(CalcValue::reference(ReferenceLike::area(kept)))),
            Err(_) => Err(site.not_modelled(Some(0))),
        },
        _ => Err(site.not_modelled(Some(0))),
    }
}

/// CHOOSEROWS and CHOOSECOLS: the selectors after the first argument, each array flattened in
/// row-major order, as OxFunc's surface expands them.
fn choose(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    kernel: fn(&[CalcValue]) -> Result<CalcValue, E>,
) -> Step<CalcValue> {
    source_error(a)?;
    let args = prepared(site, a, sv)?;
    let mut flat = vec![args[0].clone()];
    for arg in &args[1..] {
        match arg.as_array() {
            Some(array) => flat.extend(array.iter_row_major().cloned()),
            None => flat.push(arg.clone()),
        }
    }
    result(site, kernel(&flat))
}

/// A prepared argument as an array (a single value as one by one).
fn as_array(value: &CalcValue) -> CalcArray {
    match value.as_array() {
        Some(array) => array.clone(),
        None => CalcArray::from_scalar(match value.core_ref() {
            CoreRef::Missing => CalcValue::empty(),
            _ => value.clone(),
        }),
    }
}

/// A numeric argument of SORT and SORTBY: a blank or omitted one is `None`.
fn optional_number(site: &Site<'_>, value: Option<&CalcValue>) -> Step<Option<f64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if matches!(value.core_ref(), CoreRef::Missing | CoreRef::Empty) {
        return Ok(None);
    }
    match k::coerce_prepared_to_number(value) {
        Ok(n) if n.is_finite() => Ok(Some(n)),
        Ok(_) => fail(WorksheetErrorCode::Value),
        Err(CoercionError::WorksheetError(code)) => fail(code),
        Err(CoercionError::NotModelled) => Err(site.not_modelled(None)),
        Err(CoercionError::UnsupportedValueKind(_)) => fail(WorksheetErrorCode::Value),
    }
}

/// Excel 16.0.20625 floors sort indices and orders with the same integer tolerance as date
/// arguments. For example, an order of -0.9 descends and -1.5 gives #VALUE!.
fn sort_integer(site: &Site<'_>, value: Option<&CalcValue>) -> Step<Option<isize>> {
    Ok(optional_number(site, value)?.map(|n| date_integer_floor(n) as isize))
}

/// A sort order: 1 (or omitted) ascending, -1 descending, anything else #VALUE!.
fn descending(site: &Site<'_>, value: Option<&CalcValue>) -> Step<bool> {
    match sort_integer(site, value)? {
        None | Some(1) => Ok(false),
        Some(-1) => Ok(true),
        Some(_) => fail(WorksheetErrorCode::Value),
    }
}

/// The rank of a value's kind in the sort order.
fn kind(value: &CalcValue) -> u8 {
    match value.core_ref() {
        CoreRef::Empty | CoreRef::Missing => 0,
        CoreRef::Number(_) => 1,
        CoreRef::Text(_) => 2,
        CoreRef::Logical(_) => 3,
        CoreRef::Error(_) => 4,
        CoreRef::Array(_) | CoreRef::Reference(_) => 5,
    }
}

/// The order of two values for SORT and SORTBY.
fn order(site: &Site<'_>, x: &CalcValue, y: &CalcValue) -> Step<Ordering> {
    match (x.core_ref(), y.core_ref()) {
        (CoreRef::Number(a), CoreRef::Number(b)) => {
            if x.is_excel_overflow() || y.is_excel_overflow() {
                if (!a.is_finite() && !x.is_excel_overflow())
                    || (!b.is_finite() && !y.is_excel_overflow())
                {
                    return Err(site.not_modelled(None));
                }
                comparison::overflow_numbers(site, a, b)
            } else {
                comparison::numbers(site, a, b)
            }
        }
        (CoreRef::Text(a), CoreRef::Text(b)) => {
            comparison::texts(a.utf16_code_units(), b.utf16_code_units(), Want::Order)
                .map_err(|_| site.not_modelled(None))
        }
        (CoreRef::Logical(a), CoreRef::Logical(b)) => Ok(a.cmp(&b)),
        (CoreRef::Error(a), CoreRef::Error(b)) => Ok((a as u8).cmp(&(b as u8))),
        (CoreRef::Array(_) | CoreRef::Reference(_), _)
        | (_, CoreRef::Array(_) | CoreRef::Reference(_)) => Err(site.not_modelled(None)),
        _ => Ok(kind(x).cmp(&kind(y))),
    }
}

/// Excel 20527 starts with ascending runs and merges adjacent pairs from
/// left to right on each pass. When the two current keys compare equal, take
/// the left row. The comparison order matters when collation has a cycle.
fn sorted_indices(
    site: &Site<'_>,
    len: usize,
    keys: &[(Vec<CalcValue>, bool)],
) -> Step<Vec<usize>> {
    let compare = |i: usize, j: usize| {
        for (key, descending) in keys {
            let left = &key[i];
            let right = &key[j];
            let ordering = order(site, left, right)?;
            // Excel places blank keys last for either direction.
            let blank_order = left.is_empty().cmp(&right.is_empty());
            if blank_order != Ordering::Equal {
                return Ok(blank_order);
            }
            if ordering != Ordering::Equal {
                return Ok(if *descending { ordering.reverse() } else { ordering });
            }
        }
        Ok(Ordering::Equal)
    };
    let mut indices: Vec<usize> = (0..len).collect();
    let mut run_ends = Vec::new();
    for index in 1..len {
        if compare(index, index - 1)? == Ordering::Less {
            run_ends.push(index);
        }
    }
    if run_ends.is_empty() {
        return Ok(indices);
    }
    run_ends.push(len);

    let mut merged = Vec::with_capacity(len);
    while run_ends.len() > 1 {
        merged.clear();
        let mut start = 0;
        let run_count = run_ends.len().div_ceil(2);
        for run in 0..run_count {
            let middle = run_ends[2 * run];
            let end = run_ends.get(2 * run + 1).copied().unwrap_or(middle);
            let mut left = &indices[start..middle];
            let mut right = &indices[middle..end];
            while let (Some((&i, left_tail)), Some((&j, right_tail))) =
                (left.split_first(), right.split_first())
            {
                if compare(j, i)? == Ordering::Less {
                    merged.push(j);
                    right = right_tail;
                } else {
                    merged.push(i);
                    left = left_tail;
                }
            }
            merged.extend_from_slice(left);
            merged.extend_from_slice(right);
            run_ends[run] = end;
            start = end;
        }
        run_ends.truncate(run_count);
        std::mem::swap(&mut indices, &mut merged);
    }
    Ok(indices)
}

/// `array`'s rows (or, `by_col`, its columns) in the order of `indices`, with the function's
/// displayed value for an empty source cell.
fn reordered(
    array: &CalcArray,
    indices: &[usize],
    by_col: bool,
    empty_value: CalcValue,
) -> Step<CalcValue> {
    let ArrayShape { rows, cols } = array.shape();
    let mut cells = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        for col in 0..cols {
            let (r, c) = if by_col {
                (row, indices[col])
            } else {
                (indices[row], col)
            };
            cells.push(
                array
                    .get(r, c)
                    .filter(|value| !value.is_empty() || value.callable_value().is_some())
                    .cloned()
                    .unwrap_or_else(|| empty_value.clone()),
            );
        }
    }
    Ok(CalcValue::array(
        CalcArray::new(ArrayShape { rows, cols }, cells).expect("the array's own shape"),
    ))
}

/// The cells of row (or column, `by_col`) `index` of `array`.
fn line(array: &CalcArray, index: usize, by_col: bool) -> Vec<CalcValue> {
    let ArrayShape { rows, cols } = array.shape();
    if by_col {
        (0..rows)
            .map(|r| {
                array
                    .get(r, index)
                    .cloned()
                    .unwrap_or_else(CalcValue::empty)
            })
            .collect()
    } else {
        (0..cols)
            .map(|c| {
                array
                    .get(index, c)
                    .cloned()
                    .unwrap_or_else(CalcValue::empty)
            })
            .collect()
    }
}

/// SORT: by the sort_index columns (rows, `by_col`), each with its order; an array of indices
/// with an array of orders, or one order for all.
fn sort(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    source_error(a)?;
    let args = prepared(site, a, sv)?;
    let array = as_array(&args[0]);
    // A one-cell source reference holding an error behaves like a scalar
    // source error; an error in an array or a larger reference stays held.
    if a[0].as_reference().is_some()
        && array.shape() == (ArrayShape { rows: 1, cols: 1 })
        && let Some(value) = array.get(0, 0)
    {
        scalar_error(value)?;
    }
    let by_col = opt_flag(site, &args, 3, false)?;
    let ArrayShape { rows, cols } = array.shape();
    let (len, width) = if by_col { (cols, rows) } else { (rows, cols) };
    let mut indices: Vec<CalcValue> = match args.get(1) {
        Some(value) => match value.as_array() {
            Some(list) => list.iter_row_major().cloned().collect(),
            None => vec![value.clone()],
        },
        None => vec![CalcValue::missing()],
    };
    let mut orders: Vec<CalcValue> = match args.get(2) {
        Some(value) => match value.as_array() {
            Some(list) => list.iter_row_major().cloned().collect(),
            None => vec![value.clone()],
        },
        None => vec![CalcValue::missing()],
    };
    // Excel 16.0.20527: an empty cell in sort_index or sort_order is zero,
    // which is invalid; an omitted argument still selects its default.
    for value in indices.iter_mut().chain(orders.iter_mut()) {
        if matches!(value.core_ref(), CoreRef::Empty) {
            *value = CalcValue::number(0.0);
        }
    }
    if orders.len() > 1 && orders.len() != indices.len() {
        return fail(WorksheetErrorCode::Value);
    }
    let mut keys = Vec::with_capacity(indices.len());
    for (n, index) in indices.iter().enumerate() {
        let at = match sort_integer(site, Some(index))? {
            None => 0,
            Some(i) if i >= 1 && (i as usize) <= width => i as usize - 1,
            Some(_) => return fail(WorksheetErrorCode::Value),
        };
        let down = descending(site, orders.get(if orders.len() == 1 { 0 } else { n }))?;
        let key: Vec<CalcValue> = (0..len)
            .map(|i| {
                let (r, c) = if by_col { (at, i) } else { (i, at) };
                array.get(r, c).cloned().unwrap_or_else(CalcValue::empty)
            })
            .collect();
        keys.push((key, down));
    }
    let order = sorted_indices(site, len, &keys)?;
    reordered(&array, &order, by_col, CalcValue::empty())
}

/// SORTBY: by each by_array, a row or column the length of the array's rows (or of its columns,
/// when the array is one row and a key is a row), with its order.
fn sortby(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let args = prepared(site, a, sv)?;
    // An argument that is one error value is the result (Excel 16.0.20430, 2026-10-25, dc-6au:
    // `=SORTBY(1,#REF!)` #REF!, `=SORTBY(1,#DIV/0!)` #DIV/0!, `=SORTBY({3;1;2},{3;1;2},#REF!)`
    // and `=SORTBY(1,1,#REF!)` #REF!).
    for (original, argument) in a.iter().zip(&args) {
        let scalar = if original.as_reference().is_some()
            && let Some(array) = argument.as_array()
            && array.cell_count() == 1
        {
            array.get(0, 0).expect("the only cell in the reference")
        } else {
            argument
        };
        scalar_error(scalar)?;
    }
    let array = as_array(&args[0]);
    let ArrayShape { rows, cols } = array.shape();
    let mut keys: Vec<(Vec<CalcValue>, bool)> = Vec::new();
    let mut by_col = None;
    for pair in args[1..].chunks(2) {
        let key = as_array(&pair[0]);
        let order = pair.get(1);
        // A blank order cell supplies zero. Only a missing argument defaults to ascending.
        if order.is_some_and(CalcValue::is_empty) {
            return fail(WorksheetErrorCode::Value);
        }
        let down = descending(site, order)?;
        let shape = key.shape();
        let (values, columns) = if shape.cols == 1 && shape.rows == rows {
            (
                (0..rows)
                    .map(|r| key.get(r, 0).cloned().unwrap_or_else(CalcValue::empty))
                    .collect(),
                false,
            )
        } else if shape.rows == 1 && shape.cols == cols {
            (
                (0..cols)
                    .map(|c| key.get(0, c).cloned().unwrap_or_else(CalcValue::empty))
                    .collect(),
                true,
            )
        } else {
            return fail(WorksheetErrorCode::Value);
        };
        if *by_col.get_or_insert(columns) != columns {
            return fail(WorksheetErrorCode::Value);
        }
        keys.push((values, down));
    }
    let by_col = by_col.unwrap_or(false);
    let len = if by_col { cols } else { rows };
    let order = sorted_indices(site, len, &keys)?;
    reordered(&array, &order, by_col, CalcValue::number(0.0))
}

/// Whether two cells are the same for UNIQUE: numbers when equal as binary64 (OxFunc's rule;
/// qualified overflow encodings compare by exact bit identity), text ignoring case under the
/// collation, and otherwise equal kinds with equal values.
fn same(site: &Site<'_>, x: &CalcValue, y: &CalcValue) -> Step<bool> {
    match (x.core_ref(), y.core_ref()) {
        (CoreRef::Text(a), CoreRef::Text(b)) => {
            comparison::texts(a.utf16_code_units(), b.utf16_code_units(), Want::Equality)
                .map(Ordering::is_eq)
                .map_err(|_| site.not_modelled(None))
        }
        (CoreRef::Number(a), CoreRef::Number(b)) => {
            if x.is_excel_overflow() || y.is_excel_overflow() {
                if (!a.is_finite() && !x.is_excel_overflow())
                    || (!b.is_finite() && !y.is_excel_overflow())
                {
                    return Err(site.not_modelled(None));
                }
                Ok(x.is_excel_overflow() == y.is_excel_overflow() && a.to_bits() == b.to_bits())
            } else if !a.is_finite() || !b.is_finite() {
                Err(site.not_modelled(None))
            } else {
                Ok(a == b)
            }
        }
        _ => order(site, x, y).map(Ordering::is_eq),
    }
}

/// A coarse key under which equal cells always fall together (text by its collation sort key,
/// numbers by their bits), so each line is compared only with the lines that share it; `None`
/// when a text's sort key is not built (dc-zsn.1) or a cell holds an unqualified nonfinite
/// number, whose equality to the other cells is then not decided.
fn bucket(cells: &[CalcValue], references: &[Option<u64>]) -> Option<String> {
    let mut key = String::new();
    for (cell, reference) in cells.iter().zip(references) {
        if let Some(reference) = reference {
            key.push_str(&format!("r{reference:x}|"));
            continue;
        }
        match cell.core_ref() {
            CoreRef::Number(n) if cell.is_excel_overflow() => {
                key.push_str(&format!("o{:x}|", n.to_bits()));
            }
            CoreRef::Number(n) => {
                if !n.is_finite() {
                    return None;
                }
                key.push_str(&format!("n{:x}|", (n + 0.0).to_bits()));
            }
            CoreRef::Text(t) => {
                key.push('t');
                for byte in comparison::text_bucket(t.utf16_code_units())? {
                    key.push_str(&format!("{byte:02x}"));
                }
                key.push('|');
            }
            CoreRef::Logical(b) => key.push_str(if b { "T|" } else { "F|" }),
            CoreRef::Error(code) => key.push_str(&format!("e{}|", code as u8)),
            _ => key.push_str("b|"),
        }
    }
    Some(key)
}

/// For UNIQUE given an array of references (OFFSET or INDIRECT lifted over an array, dc-b1l): each
/// line's key per cell, `None` for a value, and each held reference replaced by #VALUE!, the
/// value Excel shows for it. Excel 16.0.20527 through Touchstone sheet cases (2026-11-17,
/// family reference-arrays): two references on the formula's own sheet are the same when their
/// top rows are (`=UNIQUE(INDIRECT({"A1";"B1"}))`, `{"A1";"A1:B2"}`, `{"A1:A2";"B1:B2"}` and
/// `{"A1";"XFD1"}` keep one, `{"A1";"A2"}` and `{"A1";"B2"}` two, by row and by column alike); a
/// reference to another sheet is the same as no other (`=UNIQUE(OFFSET(Data!A1,{0;0},0))` keeps
/// two). Not modelled: Excel also keeps apart two own-sheet references INDIRECT built from text
/// naming the sheet (`{"Sheet1!A1";"Sheet1!A1"}` keeps two), which a reference value does not
/// record.
fn held_reference_keys(
    site: &Site<'_>,
    lines: &mut [Vec<CalcValue>],
    sv: &mut EvalServices<'_>,
) -> Step<Vec<Vec<Option<u64>>>> {
    let holds = lines
        .iter()
        .flatten()
        .any(|cell| cell.as_reference().is_some());
    if !holds {
        return Ok(lines.iter().map(|line| vec![None; line.len()]).collect());
    }
    let caller = sv
        .host()
        .and_then(|host| host.caller())
        .ok_or_else(|| site.not_modelled(None))?;
    let mut other_sheet = 0u64;
    let mut keys = Vec::with_capacity(lines.len());
    for line in lines.iter_mut() {
        let mut line_keys = Vec::with_capacity(line.len());
        for cell in line.iter_mut() {
            let Some(reference) = cell.as_reference() else {
                line_keys.push(None);
                continue;
            };
            let first = reference.areas()[0].first();
            line_keys.push(Some(
                if reference.sheet() == caller.sheet() && !reference.spans_sheets() {
                    u64::from(first.row())
                } else {
                    other_sheet += 1;
                    (1 << 40) + other_sheet
                },
            ));
            *cell = CalcValue::error(WorksheetErrorCode::Value);
        }
        keys.push(line_keys);
    }
    Ok(keys)
}

fn unique(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let args = prepared(site, a, sv)?;
    let array = as_array(&args[0]);
    let by_col = opt_flag(site, &args, 1, false)?;
    let exactly_once = opt_flag(site, &args, 2, false)?;
    let ArrayShape { rows, cols } = array.shape();
    let len = if by_col { cols } else { rows };
    let mut lines: Vec<Vec<CalcValue>> = (0..len).map(|i| line(&array, i, by_col)).collect();
    let references = held_reference_keys(site, &mut lines, sv)?;
    // Each line's group: the first earlier line equal to it, or itself.
    let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
    let mut group = vec![0usize; len];
    for i in 0..len {
        let key = bucket(&lines[i], &references[i]).ok_or_else(|| site.not_modelled(None))?;
        let members = buckets.entry(key).or_default();
        let mut found = None;
        for &j in members.iter() {
            let mut equal = true;
            for (k, (x, y)) in lines[i].iter().zip(&lines[j]).enumerate() {
                let same = match (references[i][k], references[j][k]) {
                    (None, None) => same(site, x, y)?,
                    (a, b) => a == b,
                };
                if !same {
                    equal = false;
                    break;
                }
            }
            if equal {
                found = Some(j);
                break;
            }
        }
        match found {
            Some(j) => group[i] = j,
            None => {
                group[i] = i;
                members.push(i);
            }
        }
    }
    let mut counts = vec![0usize; len];
    for &g in &group {
        counts[g] += 1;
    }
    let kept: Vec<usize> = (0..len)
        .filter(|&i| group[i] == i && (!exactly_once || counts[i] == 1))
        .collect();
    if kept.is_empty() {
        return fail(WorksheetErrorCode::Calc);
    }
    let (out_rows, out_cols) = if by_col {
        (rows, kept.len())
    } else {
        (kept.len(), cols)
    };
    let mut cells = Vec::with_capacity(out_rows * out_cols);
    for r in 0..out_rows {
        for c in 0..out_cols {
            let value = if by_col {
                &lines[kept[c]][r]
            } else {
                &lines[kept[r]][c]
            };
            cells.push(value.clone());
        }
    }
    Ok(CalcValue::array(
        CalcArray::new(
            ArrayShape {
                rows: out_rows,
                cols: out_cols,
            },
            cells,
        )
        .expect("a shape of its cells"),
    ))
}

#[derive(Debug)]
struct SequenceTask(k::SequenceWork);
impl crate::call::KernelTask for SequenceTask {
    fn advance(
        &mut self,
        elements: usize,
        _: &mut EvalServices<'_>,
    ) -> Result<Option<CalcValue>, crate::call::FunctionFault> {
        match self.0.advance(elements) {
            None => Ok(None),
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(error)) => Ok(Some(CalcValue::error(
                k::map_dynamic_array_reshape_error_to_ws(&error),
            ))),
        }
    }
    fn completed_elements(&self) -> usize {
        self.0.completed_elements()
    }
}

fn sequence_task(
    site: &Site<'_>,
    args: &[CalcValue],
    _: &mut EvalServices<'_>,
) -> Step<Box<dyn crate::call::KernelTask>> {
    Ok(Box::new(SequenceTask(result(site, k::SequenceWork::new(args))?)))
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 sequence_resume => sequence_task;
    2 filter_fn |s, a, sv| {
        source_error(a)?;
        run(s, a, sv, k::eval_filter_prepared)
    }
    3 sort_fn |s, a, sv| {
        sort(s, a, sv)
    }
    // The arrays to sort by and their orders come in pairs (dc-mx3x, Excel 16.0.20527,
    // 2026-11-07, Formula2 and Formula alike): SORTBY takes 2, 3, 5 and 7 arguments of 1 to 8,
    // the first pair without its order but no later one (`=SORTBY(B1,B1,B1,B1)` is refused).
    4 sortby_fn |s, a, sv| {
        sortby(s, a, sv)
    }
    5 unique_fn |s, a, sv| {
        unique(s, a, sv)
    }
    // A call of TAKE or DROP may give a reference (dc-mx3x): `=COUNTBLANK(TAKE(B1,B1))` and
    // `=TAKE(B1,B1)(3)` enter, and so do DROP's.
    6 take_fn |s, a, sv| {
        source_error(a)?;
        match sub_area(s, a, true)? {
            Some(reference) => Ok(reference),
            None => run(s, a, sv, k::eval_take_prepared),
        }
    }
    7 drop_fn |s, a, sv| {
        source_error(a)?;
        match sub_area(s, a, false)? {
            Some(reference) => Ok(reference),
            None => run(s, a, sv, k::eval_drop_prepared),
        }
    }
    8 chooserows_fn |s, a, sv| {
        choose(s, a, sv, k::eval_chooserows_prepared)
    }
    9 choosecols_fn |s, a, sv| {
        choose(s, a, sv, k::eval_choosecols_prepared)
    }
    10 vstack_fn |s, a, sv| {
        // Fn2: a direct error in any source precedes stacking; reference/array cells remain cells.
        for source in a { scalar_error(source)?; }
        run(s, a, sv, k::eval_vstack_prepared)
    }
    11 hstack_fn |s, a, sv| {
        // Fn2: a direct error in any source precedes stacking; reference/array cells remain cells.
        for source in a { scalar_error(source)?; }
        // dc-h9u: a row of functions, as GROUPBY and PIVOTBY take them
        // (`GROUPBY(A2:A9,B2:B9,HSTACK(SUM,MAX))`, Excel 16.0.20527).
        if a.iter().all(|v| v.callable_value().is_some()) {
            return CalcArray::from_rows(vec![a.to_vec()])
                .map(CalcValue::array)
                .ok_or_else(|| s.not_modelled(None));
        }
        run(s, a, sv, k::eval_hstack_prepared)
    }
    12 tocol_fn |s, a, sv| {
        source_error(a)?;
        match by_area(s, a, sv, k::eval_tocol_prepared, true)? {
            Some(value) => Ok(value),
            None => run(s, a, sv, k::eval_tocol_prepared),
        }
    }
    13 torow_fn |s, a, sv| {
        source_error(a)?;
        match by_area(s, a, sv, k::eval_torow_prepared, false)? {
            Some(value) => Ok(value),
            None => run(s, a, sv, k::eval_torow_prepared),
        }
    }
    14 wraprows_fn |s, a, sv| {
        run(s, a, sv, k::eval_wraprows_prepared)
    }
    15 wrapcols_fn |s, a, sv| {
        run(s, a, sv, k::eval_wrapcols_prepared)
    }
    16 expand_fn |s, a, sv| {
        source_error(a)?;
        let mut args = Vec::with_capacity(a.len());
        args.push(prepared_one(s, a, 0, sv)?);
        // Fn2 EXPAND reads a one-cell reference's scalar error before padding. An actual
        // one-cell array keeps its array origin; HSTACK/VSTACK retain reference errors as cells.
        if a[0].as_reference().is_some()
            && let Some(array) = args[0].as_array()
            && array.shape() == (ArrayShape { rows: 1, cols: 1 })
        {
            scalar_error(array.get(0, 0).expect("one cell"))?;
        }
        for index in 1..a.len() { args.push(prepared_one(s, a, index, sv)?); }
        // Preserve the previous value admission of a one-cell reference pad. Computed
        // arrays reach the kernel whole (w090-successor-00071-expand-arg4).
        if a.get(3).is_some_and(|value| value.as_reference().is_some())
            && let Some(array) = args[3].as_array()
            && array.shape() == (ArrayShape { rows: 1, cols: 1 })
        {
            args[3] = array.get(0, 0).expect("one cell").clone();
        }
        result(s, k::eval_expand_prepared(&args))
    }
}

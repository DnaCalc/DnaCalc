//! The LAMBDA helper family fragment ported from OxFunc (functions/callable_helpers.rs at
//! 6fca3518): MAP, REDUCE, SCAN, BYROW, BYCOL and MAKEARRAY. Each calls the LAMBDA it is given
//! through the services' [`CallableInvoker`] (calc-fml's evaluator); a call without one is refused
//! as not modelled, and a LAMBDA call that stops with a fault stops the helper, whose caller
//! reports that fault.
//!
//! OxFunc's rules are kept. A reference argument is read as the array of its values (a single
//! cell as its value); the items are taken in row-major order, a blank cell as an empty value.
//! The last argument must be a LAMBDA: an error there is the result, any other value #VALUE!, and
//! a LAMBDA that does not take the number of arguments the helper passes is #VALUE!. MAP pads a
//! shorter array with #N/A and takes the shape of its first array when that holds every item (a
//! row otherwise). MAP, BYROW and BYCOL give #CALC! when a call returns an array; SCAN and
//! MAKEARRAY give #VALUE!. REDUCE's accumulator may be an array. MAKEARRAY's dimensions are
//! positive integers, anything else #VALUE!.

use super::kit::{fail, grid};
use crate::call::{CallAbandoned, EvalServices};
use crate::kernel::{Site, Step};
use crate::ported::dynamic_array_reshape_family::coerce_prepared_to_number;
use crate::value::{ArrayShape, CalcArray, CalcValue, CoreValue, WorksheetErrorCode};

type W = WorksheetErrorCode;

const CODE: u16 = 311;

/// Argument `index` as the helper reads it: a reference to one cell as the cell's value, a larger
/// reference as the array of its values, anything else as it is.
fn prepared(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    if !matches!(a[index].core(), CoreValue::Reference(_)) {
        return Ok(a[index].clone());
    }
    let g = grid(site, a, index, sv)?;
    if g.rows == 1 && g.cols == 1 {
        return Ok(g.cells.into_iter().next().unwrap_or_else(CalcValue::empty));
    }
    CalcArray::new(ArrayShape { rows: g.rows, cols: g.cols }, g.cells)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(Some(index)))
}

/// The LAMBDA at argument `index`: an error value is the result, any other value #VALUE!, and a
/// LAMBDA that does not take `count` arguments #VALUE!.
fn lambda<'v>(a: &'v [CalcValue], index: usize, count: usize) -> Step<&'v CalcValue> {
    let value = &a[index];
    match value.callable_value() {
        Some(callable) if callable.arity().accepts(count) => Ok(value),
        Some(_) => fail(W::Value),
        None => match value.core() {
            CoreValue::Error(code) => fail(*code),
            _ => fail(W::Value),
        },
    }
}

/// Calls the LAMBDA through the services' invoker.
fn call(site: &Site<'_>, sv: &mut EvalServices<'_>, f: &CalcValue, args: &[CalcValue]) -> Step<CalcValue> {
    let Some(invoker) = sv.callables() else {
        return Err(site.not_modelled(None));
    };
    match invoker.call(f, args) {
        Ok(value) => Ok(value),
        Err(CallAbandoned) => Err(site.not_modelled(None)),
    }
}

/// An item of an array as the LAMBDA receives it.
fn item(cell: &CalcValue) -> CalcValue {
    if cell.callable_value().is_some() {
        return cell.clone();
    }
    match cell.core() {
        CoreValue::Empty | CoreValue::Missing => CalcValue::empty(),
        CoreValue::Array(_) | CoreValue::Reference(_) => CalcValue::error(W::Value),
        _ => cell.clone(),
    }
}

/// A call's value as one cell of the helper's result, or `None` when it is an array.
fn cell(value: CalcValue) -> Option<CalcValue> {
    if value.callable_value().is_some() {
        return Some(value);
    }
    match value.core() {
        CoreValue::Array(_) | CoreValue::Reference(_) => None,
        CoreValue::Missing => Some(CalcValue::empty()),
        _ => Some(value),
    }
}

/// The items of a prepared value, row-major: an array's cells, or the value itself.
fn items(value: &CalcValue) -> Vec<CalcValue> {
    match value.as_array() {
        Some(array) if value.callable_value().is_none() => array.iter_row_major().map(item).collect(),
        _ => vec![value.clone()],
    }
}

fn array(shape: ArrayShape, cells: Vec<CalcValue>) -> CalcValue {
    CalcValue::array(CalcArray::new(shape, cells).expect("one cell per item of the shape"))
}

fn map(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let inputs = a.len() - 1;
    let mut sources = Vec::with_capacity(inputs);
    for index in 0..inputs {
        sources.push(prepared(site, a, index, sv)?);
    }
    let f = lambda(a, inputs, inputs)?;
    let lists: Vec<Vec<CalcValue>> = sources.iter().map(items).collect();
    let count = lists.iter().map(Vec::len).max().unwrap_or(1);
    let shape = sources
        .iter()
        .find_map(|source| source.as_array().map(CalcArray::shape))
        .filter(|shape| shape.rows * shape.cols == count)
        .unwrap_or(ArrayShape { rows: 1, cols: count.max(1) });
    let mut cells = Vec::with_capacity(count);
    for at in 0..count {
        let args: Vec<CalcValue> = lists
            .iter()
            .map(|list| list.get(at).cloned().unwrap_or_else(|| CalcValue::error(W::NA)))
            .collect();
        match cell(call(site, sv, f, &args)?) {
            Some(value) => cells.push(value),
            None => return fail(W::Calc),
        }
    }
    Ok(array(shape, cells))
}

/// The value a REDUCE or SCAN accumulator starts with: argument 0, a reference read as its value.
fn reduce(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, scan: bool) -> Step<CalcValue> {
    let mut accumulator = prepared(site, a, 0, sv)?;
    let source = prepared(site, a, 1, sv)?;
    let f = lambda(a, 2, 2)?;
    let list = items(&source);
    let shape = match source.as_array() {
        Some(array) if source.callable_value().is_none() => array.shape(),
        _ => ArrayShape { rows: 1, cols: list.len().max(1) },
    };
    let mut cells = Vec::with_capacity(if scan { list.len() } else { 0 });
    for value in list {
        let args = [std::mem::replace(&mut accumulator, CalcValue::missing()), value];
        accumulator = call(site, sv, f, &args)?;
        if scan {
            match cell(accumulator.clone()) {
                Some(value) => cells.push(value),
                None => return fail(W::Value),
            }
        }
    }
    if scan {
        Ok(array(shape, cells))
    } else {
        Ok(accumulator)
    }
}

/// BYROW (`columns` false) and BYCOL: the LAMBDA applied to each row (a one-row array) or column.
fn by(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>, columns: bool) -> Step<CalcValue> {
    let source = prepared(site, a, 0, sv)?;
    let f = lambda(a, 1, 1)?;
    let source = match source.as_array() {
        Some(array) if source.callable_value().is_none() => array.clone(),
        _ => CalcArray::from_scalar(item(&source)),
    };
    let ArrayShape { rows, cols } = source.shape();
    let count = if columns { cols } else { rows };
    let mut cells = Vec::with_capacity(count);
    for at in 0..count {
        let line: Vec<CalcValue> = if columns {
            (0..rows).map(|r| source.get(r, at).cloned().unwrap_or_else(CalcValue::empty)).collect()
        } else {
            source.row_slice(at).map(<[CalcValue]>::to_vec).unwrap_or_default()
        };
        let shape = if columns {
            ArrayShape { rows, cols: 1 }
        } else {
            ArrayShape { rows: 1, cols }
        };
        match cell(call(site, sv, f, &[array(shape, line)])?) {
            Some(value) => cells.push(value),
            None => return fail(W::Calc),
        }
    }
    let shape = if columns {
        ArrayShape { rows: 1, cols: count }
    } else {
        ArrayShape { rows: count, cols: 1 }
    };
    Ok(array(shape, cells))
}

/// A MAKEARRAY dimension: a positive integer, anything else #VALUE!.
fn dimension(site: &Site<'_>, value: &CalcValue) -> Step<usize> {
    use crate::ported::dynamic_array_reshape_family::CoercionError;
    let raw = match coerce_prepared_to_number(value) {
        Ok(raw) => raw,
        Err(CoercionError::WorksheetError(code)) => return fail(code),
        Err(CoercionError::NotModelled) => return Err(site.not_modelled(None)),
        Err(CoercionError::UnsupportedValueKind(_)) => return fail(W::Value),
    };
    if !raw.is_finite() || raw < 1.0 || raw.trunc() != raw {
        return fail(W::Value);
    }
    Ok(raw as usize)
}

fn makearray(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let rows = dimension(site, &a[0])?;
    let cols = dimension(site, &a[1])?;
    let f = lambda(a, 2, 2)?;
    let count = rows.checked_mul(cols).ok_or_else(|| site.not_modelled(None))?;
    let mut cells = Vec::with_capacity(count);
    for at in 0..count {
        let args = [CalcValue::number((at / cols + 1) as f64), CalcValue::number((at % cols + 1) as f64)];
        match cell(call(site, sv, f, &args)?) {
            Some(value) => cells.push(value),
            None => return fail(W::Value),
        }
    }
    Ok(array(ArrayShape { rows, cols }, cells))
}

ported_family! {
    code: CODE;
    1 map_fn "MAP" (2, 254) [R "array1", R "lambda_or_array2"] |s, a, sv| { map(s, a, sv) }
    2 reduce_fn "REDUCE" (3, 3) [R "initial_value", R "array", R "lambda"] |s, a, sv| { reduce(s, a, sv, false) }
    3 scan_fn "SCAN" (3, 3) [R "initial_value", R "array", R "lambda"] |s, a, sv| { reduce(s, a, sv, true) }
    4 byrow_fn "BYROW" (2, 2) [R "array", R "lambda"] |s, a, sv| { by(s, a, sv, false) }
    5 bycol_fn "BYCOL" (2, 2) [R "array", R "lambda"] |s, a, sv| { by(s, a, sv, true) }
    6 makearray_fn "MAKEARRAY" (3, 3) [V "rows", V "cols", R "lambda"] |s, a, sv| { makearray(s, a, sv) }
}

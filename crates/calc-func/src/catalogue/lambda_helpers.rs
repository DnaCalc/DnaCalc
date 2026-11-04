//! The LAMBDA helper family fragment ported from OxFunc (functions/callable_helpers.rs at
//! 6fca3518): MAP, REDUCE, SCAN, BYROW, BYCOL and MAKEARRAY. Each calls the LAMBDA it is given
//! through the services' [`CallableInvoker`] (calc-fml's evaluator); a call without one is refused
//! as not modelled, and a LAMBDA call that stops with a fault stops the helper, whose caller
//! reports that fault.
//!
//! OxFunc's rules are kept. A reference argument is read as the array of its values (a single
//! cell as its value); the items are taken in row-major order, a blank cell as an empty value.
//! The last argument must be a LAMBDA: any other value, an error included, is #VALUE! (Excel
//! 16.0.20430, 2026-10-25, dc-6au: `=BYCOL(1,#REF!)`, `=MAP(1,1,#REF!)`, `=MAKEARRAY(1,1,#REF!)`,
//! `=REDUCE(1,1,#REF!)` and `=BYCOL(1,#DIV/0!)` are #VALUE!, where OxFunc gave the error), and a
//! LAMBDA that does not take the number of arguments the helper passes is #VALUE!. An error among
//! the other arguments is the result (`=BYCOL(#REF!,LAMBDA(x,x))` is #REF!). MAP pads a
//! shorter array with #N/A and takes the shape of its first array when that holds every item (a
//! row otherwise). MAP and SCAN give #CALC! when a call returns an array; BYROW and BYCOL admit
//! a one-cell result array but reject larger ones with #CALC!. MAKEARRAY gives #VALUE! for an
//! array result. REDUCE's accumulator may be an array. MAKEARRAY's dimensions are
//! positive integers, anything else #VALUE!.

use super::kit::{fail, grid, scalar_error};
use crate::call::{CallAbandoned, EvalServices, FunctionFault, KernelTask};
use crate::catalogue::FunctionId;
use crate::kernel::{Site, Step};
use crate::ported::dynamic_array_reshape_family::coerce_prepared_to_number;
use crate::value::{ArrayShape, CalcArray, CalcValue, CoreRef, WorksheetErrorCode};

type W = WorksheetErrorCode;

const CODE: u16 = 311;

/// Argument `index` as the helper reads it: a reference to one cell as the cell's value, a larger
/// reference as the array of its values, anything else as it is.
fn prepared(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    if !matches!(a[index].core_ref(), CoreRef::Reference(_)) {
        return Ok(a[index].clone());
    }
    let g = grid(site, a, index, sv)?;
    if g.rows == 1 && g.cols == 1 {
        return Ok(g.cells.into_iter().next().unwrap_or_else(CalcValue::empty));
    }
    CalcArray::new(
        ArrayShape {
            rows: g.rows,
            cols: g.cols,
        },
        g.cells,
    )
    .map(CalcValue::array)
    .ok_or_else(|| site.not_modelled(Some(index)))
}

/// The LAMBDA at argument `index`: any other value, an error included, is #VALUE!, and so is a
/// LAMBDA that does not take `count` arguments.
fn lambda<'v>(a: &'v [CalcValue], index: usize, count: usize) -> Step<&'v CalcValue> {
    let value = &a[index];
    match value.callable_value() {
        Some(callable) if callable.arity().accepts(count) => Ok(value),
        _ => fail(W::Value),
    }
}

/// An item of an array as the LAMBDA receives it.
fn item(cell: &CalcValue) -> CalcValue {
    if cell.callable_value().is_some() {
        return cell.clone();
    }
    match cell.core_ref() {
        CoreRef::Empty | CoreRef::Missing => CalcValue::empty(),
        CoreRef::Array(_) | CoreRef::Reference(_) => CalcValue::error(W::Value),
        _ => cell.clone(),
    }
}

/// A call's value as one cell of the helper's result, or `None` when it is an array.
fn cell(value: CalcValue) -> Option<CalcValue> {
    if value.callable_value().is_some() {
        return Some(value);
    }
    match value.core_ref() {
        CoreRef::Array(_) | CoreRef::Reference(_) => None,
        CoreRef::Missing => Some(CalcValue::empty()),
        _ => Some(value),
    }
}

/// The items of a prepared value, row-major: an array's cells, or the value itself.
fn items(value: &CalcValue) -> Vec<CalcValue> {
    match value.as_array() {
        Some(array) if value.callable_value().is_none() => {
            array.iter_row_major().map(item).collect()
        }
        _ => vec![value.clone()],
    }
}

fn array(shape: ArrayShape, cells: Vec<CalcValue>) -> CalcValue {
    CalcValue::array(CalcArray::new(shape, cells).expect("one cell per item of the shape"))
}

fn map_task(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
) -> Step<Box<dyn KernelTask>> {
    // Fn2 roots031/034: a direct first-source error precedes even a valid LAMBDA.
    // Root035 keeps invalid-LAMBDA validation ahead of a later scalar source error.
    scalar_error(&a[0])?;
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
        .unwrap_or(ArrayShape {
            rows: 1,
            cols: count.max(1),
        });
    Ok(helper(site, f, HelperInput::Map(lists), shape, count))
}

/// The value a REDUCE or SCAN accumulator starts with: argument 0, a reference read as its value.
fn reduce_task(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    scan: bool,
) -> Step<Box<dyn KernelTask>> {
    let accumulator = prepared(site, a, 0, sv)?;
    let source = prepared(site, a, 1, sv)?;
    let f = lambda(a, 2, 2)?;
    let list = items(&source);
    let shape = match source.as_array() {
        Some(array) if source.callable_value().is_none() => array.shape(),
        _ => ArrayShape {
            rows: 1,
            cols: list.len().max(1),
        },
    };
    let count = list.len();
    Ok(helper(
        site,
        f,
        HelperInput::Reduce {
            list,
            accumulator,
            scan,
        },
        shape,
        count,
    ))
}

fn reduce_start(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
) -> Step<Box<dyn KernelTask>> {
    reduce_task(site, a, sv, false)
}

fn scan_start(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
) -> Step<Box<dyn KernelTask>> {
    reduce_task(site, a, sv, true)
}

/// BYROW (`columns` false) and BYCOL: the LAMBDA applied to each row (a one-row array) or column.
fn by_task(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    columns: bool,
) -> Step<Box<dyn KernelTask>> {
    // unswept-00087/00093: direct scalar errors precede an invalid final argument. A reference
    // held error still follows the existing preparation and LAMBDA validation order below.
    scalar_error(&a[0])?;
    let source = prepared(site, a, 0, sv)?;
    let f = lambda(a, 1, 1)?;
    // An array argument that is one error value is the result (Excel 16.0.20430, 2026-10-25,
    // dc-6au: `=BYCOL(#REF!,LAMBDA(x,x))` is #REF!, not the #CALC! of a LAMBDA returning a row).
    if let CoreRef::Error(code) = source.core_ref() {
        return fail(code);
    }
    let source = match source.as_array() {
        Some(array) if source.callable_value().is_none() => array.clone(),
        _ => CalcArray::from_scalar(item(&source)),
    };
    let ArrayShape { rows, cols } = source.shape();
    let count = if columns { cols } else { rows };
    let shape = if columns {
        ArrayShape {
            rows: 1,
            cols: count,
        }
    } else {
        ArrayShape {
            rows: count,
            cols: 1,
        }
    };
    Ok(helper(
        site,
        f,
        HelperInput::By { source, columns },
        shape,
        count,
    ))
}

fn byrow_start(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
) -> Step<Box<dyn KernelTask>> {
    by_task(site, a, sv, false)
}

fn bycol_start(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
) -> Step<Box<dyn KernelTask>> {
    by_task(site, a, sv, true)
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

fn makearray_task(
    site: &Site<'_>,
    a: &[CalcValue],
    _: &mut EvalServices<'_>,
) -> Step<Box<dyn KernelTask>> {
    let rows = dimension(site, &a[0])?;
    let cols = dimension(site, &a[1])?;
    let f = lambda(a, 2, 2)?;
    let count = rows
        .checked_mul(cols)
        .ok_or_else(|| site.not_modelled(None))?;
    Ok(helper(
        site,
        f,
        HelperInput::MakeArray,
        ArrayShape { rows, cols },
        count,
    ))
}

#[derive(Debug)]
enum HelperInput {
    Map(Vec<Vec<CalcValue>>),
    Reduce {
        list: Vec<CalcValue>,
        accumulator: CalcValue,
        scan: bool,
    },
    By {
        source: CalcArray,
        columns: bool,
    },
    MakeArray,
}

/// One kernel for synchronous calls and retained calls. Completed items and the current
/// arguments survive a yield or provider wait; no completed LAMBDA is entered again.
#[derive(Debug)]
struct HelperTask {
    id: FunctionId,
    callee: CalcValue,
    input: HelperInput,
    shape: ArrayShape,
    count: usize,
    at: usize,
    arguments: Option<Vec<CalcValue>>,
    cells: Vec<CalcValue>,
}

fn helper(
    site: &Site<'_>,
    f: &CalcValue,
    input: HelperInput,
    shape: ArrayShape,
    count: usize,
) -> Box<dyn KernelTask> {
    Box::new(HelperTask {
        id: site.id(),
        callee: f.clone(),
        input,
        shape,
        count,
        at: 0,
        arguments: None,
        cells: Vec::new(),
    })
}

impl KernelTask for HelperTask {
    fn advance(
        &mut self,
        elements: usize,
        sv: &mut EvalServices<'_>,
    ) -> Result<Option<CalcValue>, FunctionFault> {
        let refused = || FunctionFault::NotImplemented {
            id: self.id,
            argument: None,
        };
        for _ in 0..elements {
            if self.at == self.count {
                break;
            }
            let args = self.arguments.get_or_insert_with(|| match &self.input {
                HelperInput::Map(lists) => lists
                    .iter()
                    .map(|list| {
                        list.get(self.at)
                            .cloned()
                            .unwrap_or_else(|| CalcValue::error(W::NA))
                    })
                    .collect(),
                HelperInput::Reduce {
                    list, accumulator, ..
                } => vec![accumulator.clone(), list[self.at].clone()],
                HelperInput::By { source, columns } => {
                    let ArrayShape { rows, cols } = source.shape();
                    let (shape, line) = if *columns {
                        (
                            ArrayShape { rows, cols: 1 },
                            (0..rows)
                                .map(|r| {
                                    source
                                        .get(r, self.at)
                                        .cloned()
                                        .unwrap_or_else(CalcValue::empty)
                                })
                                .collect(),
                        )
                    } else {
                        (
                            ArrayShape { rows: 1, cols },
                            source.row_slice(self.at).unwrap_or_default().to_vec(),
                        )
                    };
                    vec![array(shape, line)]
                }
                HelperInput::MakeArray => vec![
                    CalcValue::number((self.at / self.shape.cols + 1) as f64),
                    CalcValue::number((self.at % self.shape.cols + 1) as f64),
                ],
            });
            let value = match sv
                .callables()
                .ok_or_else(refused)?
                .call_slice(&self.callee, args)
            {
                Ok(Some(value)) => value,
                Ok(None) => return Ok(None),
                Err(CallAbandoned) => return Err(refused()),
            };
            // A one-cell row/column's lifted TEXTSPLIT returns a one-cell array. Fn2
            // roots089/090 admit that single value; larger results still fail below.
            let value = if matches!(&self.input, HelperInput::By { .. })
                && value.callable_value().is_none()
                && let Some(result) = value.as_array()
                && result.shape() == (ArrayShape { rows: 1, cols: 1 })
            {
                result.get(0, 0).expect("one cell").clone()
            } else {
                value
            };
            let (keep, error) = match &mut self.input {
                HelperInput::Reduce {
                    accumulator, scan, ..
                } => {
                    *accumulator = value.clone();
                    // Fn2 root092: SCAN cannot publish an array accumulator as a cell.
                    (*scan, W::Calc)
                }
                HelperInput::MakeArray => (true, W::Value),
                _ => (true, W::Calc),
            };
            if keep {
                let Some(value) = cell(value) else {
                    return Ok(Some(CalcValue::error(error)));
                };
                self.cells.push(value);
            }
            self.arguments = None;
            self.at += 1;
        }
        if self.at != self.count {
            return Ok(None);
        }
        Ok(Some(match &self.input {
            HelperInput::Reduce {
                accumulator,
                scan: false,
                ..
            } => accumulator.clone(),
            _ => array(self.shape, std::mem::take(&mut self.cells)),
        }))
    }

    fn completed_elements(&self) -> usize {
        self.at
    }
}

fn run(
    site: &Site<'_>,
    task: Step<Box<dyn KernelTask>>,
    sv: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    task?
        .advance(usize::MAX, sv)
        .map_err(crate::kernel::Exit::Fault)?
        .ok_or_else(|| site.not_modelled(None))
}

ported_family! {
    code: CODE;
    1 map_fn "MAP" (2, 254) [R "array1", R "lambda_or_array2"] resume map_resume => map_task; |s, a, sv| { run(s, map_task(s, a, sv), sv) }
    2 reduce_fn "REDUCE" (3, 3) [R "initial_value", R "array", R "lambda"] resume reduce_resume => reduce_start; |s, a, sv| { run(s, reduce_start(s, a, sv), sv) }
    3 scan_fn "SCAN" (3, 3) [R "initial_value", R "array", R "lambda"] resume scan_resume => scan_start; |s, a, sv| { run(s, scan_start(s, a, sv), sv) }
    4 byrow_fn "BYROW" (2, 2) [R "array", R "lambda"] resume byrow_resume => byrow_start; |s, a, sv| { run(s, byrow_start(s, a, sv), sv) }
    5 bycol_fn "BYCOL" (2, 2) [R "array", R "lambda"] resume bycol_resume => bycol_start; |s, a, sv| { run(s, bycol_start(s, a, sv), sv) }
    6 makearray_fn "MAKEARRAY" (3, 3) [V "rows", V "cols", R "lambda"] resume makearray_resume => makearray_task; |s, a, sv| { run(s, makearray_task(s, a, sv), sv) }
}

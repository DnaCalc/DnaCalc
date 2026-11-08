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
//! row otherwise). MAP gives #CALC! when a call returns an array; SCAN, BYROW and BYCOL admit
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
fn cell(value: CalcValue, singleton: bool) -> Option<CalcValue> {
    if value.callable_value().is_some() {
        return Some(value);
    }
    match value.core_ref() {
        CoreRef::Array(array)
            if singleton && array.shape() == (ArrayShape { rows: 1, cols: 1 }) =>
        {
            cell(array.get(0, 0).expect("one cell").clone(), false)
        }
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
    // Fn5 helper015: later whole-argument errors are invalid MAP sources.
    // Held errors and errors inside arrays still reach the LAMBDA (016/017).
    for value in &a[1..inputs] {
        if value.callable_value().is_none() && matches!(value.core_ref(), CoreRef::Error(_)) {
            return fail(W::Value);
        }
    }
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
    // No LAMBDA at all: #CALC! (dc-mx3x: `=BYROW(B1:B3)`), as an omitted one is #VALUE!
    // (`=BYROW(B1:B3,)`).
    if a.len() < 2 {
        return fail(W::Calc);
    }
    let source = prepared(site, a, 0, sv)?;
    let f = lambda(a, 1, 1)?;
    // Direct errors were handled above. Fn5 helper018/019 keep held errors
    // inside the row/column argument so the LAMBDA can handle them.
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
            let (keep, error) = match &mut self.input {
                HelperInput::Reduce {
                    accumulator, scan, ..
                } => {
                    *accumulator = value.clone();
                    // Preserve the accumulator itself; publication below admits only
                    // a one-cell result array (fn5 helper011), not larger arrays.
                    (*scan, W::Calc)
                }
                HelperInput::MakeArray => (true, W::Value),
                _ => (true, W::Calc),
            };
            if keep {
                let singleton = matches!(
                    &self.input,
                    HelperInput::By { .. } | HelperInput::Reduce { scan: true, .. }
                );
                let Some(value) = cell(value, singleton) else {
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
    1 map_fn "MAP" (2, 254) [R "array1" ~ ArrayAtRoot, R "lambda_or_array2" ~ AggregateAtRootTwice] resume map_resume => map_task; |s, a, sv| { run(s, map_task(s, a, sv), sv) }
    // A call of REDUCE gives what its LAMBDA gives, which may be a reference or a LAMBDA
    // (dc-mx3x, Excel 16.0.20527, 2026-11-07): `=COUNTBLANK(REDUCE(B1,B1,B1))` and
    // `=REDUCE(B1,B1,B1)(3)` enter; MAP's, SCAN's, BYROW's, BYCOL's and MAKEARRAY's do not.
    2 reduce_fn "REDUCE" (3, 3) [R "initial_value" ~ AggregateAtRoot, R "array" ~ ArrayAtRoot, R "lambda" ~ AggregateAtRoot] result MayBeEither; resume reduce_resume => reduce_start; |s, a, sv| { run(s, reduce_start(s, a, sv), sv) }
    3 scan_fn "SCAN" (3, 3) [R "initial_value" ~ AggregateAtRoot, R "array" ~ ArrayAtRoot, R "lambda" ~ AggregateAtRoot] resume scan_resume => scan_start; |s, a, sv| { run(s, scan_start(s, a, sv), sv) }
    // BYROW and BYCOL take 1 or 2 arguments (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and
    // Formula alike): `=BYROW(B1:B3)` and `=BYCOL(B1:B3)` enter and give #CALC!, while
    // `=MAP({1})`, `=REDUCE(0,{1})`, `=SCAN(0,{1})` and `=MAKEARRAY(1,1)` are refused.
    4 byrow_fn "BYROW" (1, 2) [R "array" ~ ArrayAtRoot, R "lambda" ~ AggregateAtRoot] resume byrow_resume => byrow_start; |s, a, sv| { run(s, byrow_start(s, a, sv), sv) }
    5 bycol_fn "BYCOL" (1, 2) [R "array" ~ ArrayAtRoot, R "lambda" ~ AggregateAtRoot] resume bycol_resume => bycol_start; |s, a, sv| { run(s, bycol_start(s, a, sv), sv) }
    6 makearray_fn "MAKEARRAY" (3, 3) [V "rows", V "cols", R "lambda" ~ AggregateAtRoot] resume makearray_resume => makearray_task; |s, a, sv| { run(s, makearray_task(s, a, sv), sv) }
}

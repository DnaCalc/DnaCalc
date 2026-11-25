//! The LAMBDA helper family fragment ported from OxFunc (functions/callable_helpers.rs at
//! 6fca3518): MAP, REDUCE, SCAN, BYROW, BYCOL and MAKEARRAY. Each calls the LAMBDA it is given
//! through the services' [`CallableInvoker`] (calc-fml's evaluator); a call without one is refused
//! as not modelled, and a LAMBDA call that stops with a fault stops the helper, whose caller
//! reports that fault.
//!
//! The items are taken in row-major order. Excel hands the LAMBDA references into a reference
//! argument of one area, not values (Excel 16.0.20527 env 145016f4397d, 2026-11-16, Touchstone
//! sheet cases): MAP, REDUCE and SCAN pass each cell as a one-cell reference
//! (`=MAP(B1:B2,LAMBDA(x,ISREF(x)))` is {TRUE;TRUE}, `=SCAN(0,B1:B3,LAMBDA(a,v,ROW(v)))` {1;2;3},
//! and `=REDUCE(0,D1:D5,LAMBDA(a,v,MAX(a,v)))` over text cells 0, since MAX skips text it reads
//! through a reference), BYROW and BYCOL each row or column as a reference
//! (`=BYCOL(B1:C3,LAMBDA(c,ROW(c)))` is #CALC!, ROW giving an array); an array argument passes
//! its elements as values (`=MAP({1,2},LAMBDA(x,ISREF(x)))` is {FALSE,FALSE}). A LAMBDA result
//! that is a one-cell reference is published as the cell's value (`=MAP(C1:C3,LAMBDA(x,x))`);
//! REDUCE's accumulator and result keep a reference the LAMBDA gives
//! (`=ISREF(REDUCE(0,B1:B3,LAMBDA(a,v,v)))` is TRUE); a larger reference is an array result.
//! REDUCE's and SCAN's initial value is passed as given (`=SCAN(B1,B1:B3,LAMBDA(a,v,ISREF(a)))`
//! is {TRUE;FALSE;FALSE}); an omitted one starts the calls at the second item and the
//! accumulator at the first: REDUCE passes that item as it is, a reference into a range
//! (`=REDUCE(,B1:B2,LAMBDA(a,v,ISREF(a)))` is TRUE), SCAN its value, which it shows first
//! (`=SCAN(,B1:B3,LAMBDA(a,v,ISREF(a)))` is {1;FALSE;FALSE}); `=REDUCE(,B1:B5,LAMBDA(a,v,a*v))`
//! is the product of B1:B5 and `=SCAN(,{5,6},LAMBDA(a,v,a+v))` {5,11}. MAP lines its arrays up by row and column: the
//! result is as tall as the tallest and as wide as the widest, and a position some array lacks
//! is #N/A without a call (`=MAP(B1:B3,C1:C2,LAMBDA(x,y,IFERROR(y,"pad")))` ends in #N/A).
//!
//! OxFunc's other rules are kept.
//! The last argument must be a LAMBDA: any other value, an error included, is #VALUE! (Excel
//! 16.0.20430, 2026-10-25, dc-6au: `=BYCOL(1,#REF!)`, `=MAP(1,1,#REF!)`, `=MAKEARRAY(1,1,#REF!)`,
//! `=REDUCE(1,1,#REF!)` and `=BYCOL(1,#DIV/0!)` are #VALUE!, where OxFunc gave the error), and a
//! LAMBDA that does not take the number of arguments the helper passes is #VALUE!. An error among
//! the other arguments is the result (`=BYCOL(#REF!,LAMBDA(x,x))` is #REF!). MAP gives #CALC!
//! when a call returns an array; SCAN, BYROW and BYCOL admit
//! a one-cell result array but reject larger ones with #CALC!. MAKEARRAY gives #VALUE! for an
//! array result. REDUCE's accumulator may be an array. MAKEARRAY's dimensions are
//! positive integers, anything else #VALUE!.

use super::kit::{fail, grid, scalar_error};
use crate::call::{CallAbandoned, EvalServices, FunctionFault, KernelTask};
use crate::catalogue::FunctionId;
use crate::kernel::{Site, Step};
use crate::ported::dynamic_array_reshape_family::coerce_prepared_to_number;
use crate::value::{
    ArrayShape, CalcArray, CalcValue, CellKey, CoreRef, GridArea, ReferenceLike, WorksheetErrorCode,
};

type W = WorksheetErrorCode;

use calc_catalogue::ported::lambda_helpers::{CODE, ENTRIES as DECLARATIONS};

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

/// The one area of a reference argument, when it is one.
fn one_area(value: &CalcValue) -> Option<GridArea> {
    match value.core_ref() {
        CoreRef::Reference(reference) => match reference.areas() {
            [area] => Some(*area),
            _ => None,
        },
        _ => None,
    }
}

fn reference_to(area: GridArea) -> CalcValue {
    CalcValue::reference(ReferenceLike::area(area))
}

/// The cell at (`r`, `c`) of `area` (0-based) as a one-cell reference.
fn cell_of(area: GridArea, r: usize, c: usize) -> CalcValue {
    let first = area.first();
    let cell = CellKey::new(area.sheet(), first.row() + r as u32, first.column() + c as u32)
        .expect("a cell inside the area");
    reference_to(GridArea::cell(cell))
}

/// The shape and the row-major items of a source as the LAMBDA receives them: a reference of one
/// area as one-cell references, an array as its elements, anything else as itself (1 by 1).
fn source_items(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<(ArrayShape, Vec<CalcValue>)> {
    if let Some(area) = one_area(&a[index]) {
        let (rows, cols) = (area.row_count() as usize, area.column_count() as usize);
        let cells = (0..rows * cols).map(|k| cell_of(area, k / cols, k % cols)).collect();
        return Ok((ArrayShape { rows, cols }, cells));
    }
    let value = prepared(site, a, index, sv)?;
    let list = items(&value);
    let shape = match value.as_array() {
        Some(array) if value.callable_value().is_none() => array.shape(),
        _ => ArrayShape {
            rows: 1,
            cols: list.len().max(1),
        },
    };
    Ok((shape, list))
}

/// A LAMBDA's result read as Excel publishes it in a helper's cell: a one-cell reference as the
/// cell's value.
fn read_one_cell(id: FunctionId, value: CalcValue, sv: &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault> {
    match one_area(&value) {
        Some(area) if area.cell_count() == 1 => {
            let site = Site::new(id, sv);
            match grid(&site, std::slice::from_ref(&value), 0, sv) {
                Ok(g) => Ok(g.cells.into_iter().next().unwrap_or_else(CalcValue::empty)),
                Err(crate::kernel::Exit::Error(code)) => Ok(CalcValue::error(code)),
                Err(crate::kernel::Exit::Fault(fault)) => Err(fault),
            }
        }
        _ => Ok(value),
    }
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

/// An item of an array as the LAMBDA receives it. A reference an array of references holds (OFFSET
/// or INDIRECT lifted over an array) is passed as the reference (dc-b1l, Excel 16.0.20527 through
/// Touchstone, 2026-11-16: `=REDUCE(0,OFFSET(A1,{0;1;2},0,2),LAMBDA(a,v,a+v))` spills 6|9 over 1..6
/// in A1:A6, `=MAP(OFFSET(A1,{0;1;2},0),LAMBDA(v,v))` spills 1|2|3 and over two-cell references is
/// #CALC!).
fn item(cell: &CalcValue) -> CalcValue {
    if cell.callable_value().is_some() {
        return cell.clone();
    }
    match cell.core_ref() {
        CoreRef::Empty | CoreRef::Missing => CalcValue::empty(),
        CoreRef::Array(_) => CalcValue::error(W::Value),
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
        sources.push(source_items(site, a, index, sv)?);
    }
    let f = lambda(a, inputs, inputs)?;
    // Fn5 helper015: later whole-argument errors are invalid MAP sources.
    // Held errors and errors inside arrays still reach the LAMBDA (016/017).
    // A later source must be an array or a reference: a direct scalar is #VALUE! as well, though
    // the first may be one (Excel 16.0.20527, 2026-11-16: `=MAP(B1:B3,5,LAMBDA(x,y,x+y))`,
    // `=MAP(B1:B3,"x",...)` and `=MAP(5,6,...)` are #VALUE!, `=MAP(5,B1:B3,...)` and
    // `=MAP(B1:B3,{5},...)` are {6;#N/A;#N/A}).
    for value in &a[1..inputs] {
        if value.callable_value().is_none()
            && !matches!(value.core_ref(), CoreRef::Array(_) | CoreRef::Reference(_))
        {
            return fail(W::Value);
        }
    }
    let shape = ArrayShape {
        rows: sources.iter().map(|(shape, _)| shape.rows).max().unwrap_or(1),
        cols: sources.iter().map(|(shape, _)| shape.cols).max().unwrap_or(1),
    };
    let count = shape.rows * shape.cols;
    Ok(helper(site, f, HelperInput::Map(sources), shape, count))
}

/// The value a REDUCE or SCAN accumulator starts with: argument 0, a reference read as its value.
fn reduce_task(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    scan: bool,
) -> Step<Box<dyn KernelTask>> {
    let (shape, list) = source_items(site, a, 1, sv)?;
    let f = lambda(a, 2, 2)?;
    let count = list.len();
    // An omitted initial value: the calls start at the second item, the accumulator at the first,
    // which REDUCE passes as it is (a reference into a range: `=REDUCE(,B1:B2,LAMBDA(a,v,ROW(a)))`
    // is 1) and SCAN as the value it shows first (`=SCAN(,B1:B3,LAMBDA(a,v,ISREF(a)))` is
    // {1;FALSE;FALSE}).
    let (accumulator, at, cells) = if a[0].is_missing() && count > 0 {
        if scan {
            let first = read_one_cell(site.id(), list[0].clone(), sv).map_err(crate::kernel::Exit::Fault)?;
            let shown = cell(first.clone(), true).into_iter().collect();
            (first, 1, shown)
        } else {
            (list[0].clone(), 1, Vec::new())
        }
    } else {
        // A reference stays a reference (`=SCAN(B1,B1:B3,LAMBDA(a,v,ISREF(a)))` begins TRUE).
        let initial = if matches!(a[0].core_ref(), CoreRef::Reference(_)) { a[0].clone() } else { prepared(site, a, 0, sv)? };
        (initial, 0, Vec::new())
    };
    let task = HelperTask {
        id: site.id(),
        callee: f.clone(),
        input: HelperInput::Reduce {
            list,
            accumulator,
            scan,
        },
        shape,
        count,
        at,
        arguments: None,
        cells,
    };
    Ok(Box::new(task))
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
    // A reference of one area: each row or column as a reference into it.
    if let Some(area) = one_area(&a[0]) {
        let f = lambda(a, 1, 1)?;
        let (rows, cols) = (area.row_count() as usize, area.column_count() as usize);
        let count = if columns { cols } else { rows };
        let shape = if columns { ArrayShape { rows: 1, cols: count } } else { ArrayShape { rows: count, cols: 1 } };
        return Ok(helper(site, f, HelperInput::ByArea { area, columns }, shape, count));
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
    // No dimension above the sheet's rows, no result this engine could not hold: a typed
    // refusal before anything is built (dc-ztvv.26.132; Excel's own answer is unmeasured).
    let count = rows
        .checked_mul(cols)
        .filter(|count| rows <= 1_048_576 && cols <= 1_048_576 && *count <= 1 << 26)
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
    /// Each source's shape and row-major items.
    Map(Vec<(ArrayShape, Vec<CalcValue>)>),
    Reduce {
        list: Vec<CalcValue>,
        accumulator: CalcValue,
        scan: bool,
    },
    By {
        source: CalcArray,
        columns: bool,
    },
    ByArea {
        area: GridArea,
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
            // MAP: a position one of the arrays lacks is #N/A, with no call.
            if let HelperInput::Map(sources) = &self.input {
                let (r, c) = (self.at / self.shape.cols, self.at % self.shape.cols);
                if sources.iter().any(|(shape, _)| r >= shape.rows || c >= shape.cols) {
                    self.cells.push(CalcValue::error(W::NA));
                    self.at += 1;
                    continue;
                }
            }
            let (shape_cols, at) = (self.shape.cols, self.at);
            let args = self.arguments.get_or_insert_with(|| match &self.input {
                HelperInput::Map(sources) => sources
                    .iter()
                    .map(|(shape, list)| {
                        let (r, c) = (at / shape_cols, at % shape_cols);
                        list.get(r * shape.cols + c).cloned().unwrap_or_else(|| CalcValue::error(W::NA))
                    })
                    .collect(),
                HelperInput::ByArea { area, columns } => {
                    let first = area.first();
                    let (rows, cols) = (area.row_count(), area.column_count());
                    let (top, left, height, width) = if *columns {
                        (first.row(), first.column() + at as u32, rows, 1)
                    } else {
                        (first.row() + at as u32, first.column(), 1, cols)
                    };
                    let start = CellKey::new(area.sheet(), top, left).expect("inside the area");
                    let end = CellKey::new(area.sheet(), top + height - 1, left + width - 1).expect("inside the area");
                    vec![reference_to(GridArea::new(start, end).expect("one sheet"))]
                }
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
                .call_slice_keeping_reference(&self.callee, args)
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
                    HelperInput::By { .. } | HelperInput::ByArea { .. } | HelperInput::Reduce { scan: true, .. }
                );
                // A one-cell reference is published as the cell's value.
                let value = read_one_cell(self.id, value, sv)?;
                // An array holding references (a row of an array of references INDEX gives,
                // dc-b1l) is #CALC! (Excel 16.0.20527, Touchstone, 2026-11-16:
                // `=MAKEARRAY(2,1,LAMBDA(r,c,INDEX(OFFSET(A1,{0;1;2},0),r)))` is #CALC!, and over a
                // 2 x 2 array of references too).
                let error = match value.as_array() {
                    Some(array)
                        if value.callable_value().is_none()
                            && array
                                .iter_row_major()
                                .any(|v| matches!(v.core_ref(), CoreRef::Reference(_))) =>
                    {
                        W::Calc
                    }
                    _ => error,
                };
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

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 map_resume => map_task;
    // A call of REDUCE gives what its LAMBDA gives, which may be a reference or a LAMBDA
    // (dc-mx3x, Excel 16.0.20527, 2026-11-07): `=COUNTBLANK(REDUCE(B1,B1,B1))` and
    // `=REDUCE(B1,B1,B1)(3)` enter; MAP's, SCAN's, BYROW's, BYCOL's and MAKEARRAY's do not.
    2 reduce_resume => reduce_start;
    3 scan_resume => scan_start;
    // BYROW and BYCOL take 1 or 2 arguments (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and
    // Formula alike): `=BYROW(B1:B3)` and `=BYCOL(B1:B3)` enter and give #CALC!, while
    // `=MAP({1})`, `=REDUCE(0,{1})`, `=SCAN(0,{1})` and `=MAKEARRAY(1,1)` are refused.
    4 byrow_resume => byrow_start;
    5 bycol_resume => bycol_start;
    6 makearray_resume => makearray_task;
}

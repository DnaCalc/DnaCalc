//! The aggregate family fragment: the sums, products, counts, averages and extremes of their
//! arguments, and their kernels.
//!
//! Every kernel reads its arguments in one order: argument by argument; a reference's areas in
//! order, each area's cells in row-major order through the services' cell reader, which visits
//! the cells that hold a value and may also visit blank cells as Empty, so a kernel's work and
//! memory follow the cells the reader visits and not the area's size; an array's elements in
//! row-major order. The arithmetic is a sequential left fold in that order from the operation's
//! identity, each operation an arithmetic site that rounds under the call's numeric mode, so
//! under Exact, which has no implementation, a call that reaches one is refused with
//! ExactUnavailable. The first error met in that order is the result; the counts return none.
//!
//! A direct argument and a value that a reference or an array holds follow different rules. For
//! the sums, products, averages and extremes, a direct argument is read through worksheet number
//! coercion: text that reads as a number and logicals count, other text is #VALUE!, and an
//! omitted argument is 0. A held value counts only as a number: text, logicals and blank cells
//! are skipped. The A-suffixed functions (AVERAGEA, MINA and MAXA) read held text as 0 and held
//! logicals as 1 and 0, and still skip blank cells. COUNT, COUNTA, COUNTBLANK and SUMPRODUCT
//! follow their own rules, which their kernels' docs state: the counts never return an error of
//! their arguments, and SUMPRODUCT reads direct text as 0 and an omitted argument as #VALUE!.
//!
//! An array or a reference held inside an array is not modelled and is refused, and so is a
//! product of two non-zero factors that underflows, because Excel's handling of an underflow is
//! not modelled.

use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{CellReader, EagerKernel, EvalServices, FunctionFault, Unreadable};
use crate::coerce::{self, Numeric, Stop};
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcValue, CallableArityShape, CellKey, CoreValue, GridArea, ReferenceLike, WorksheetErrorCode,
};
use std::ops::ControlFlow;

const CODE: u16 = 4;

const SUM: FunctionId = FunctionId::new(CODE, 1);
const PRODUCT: FunctionId = FunctionId::new(CODE, 2);
const SUMSQ: FunctionId = FunctionId::new(CODE, 3);
const SUMPRODUCT: FunctionId = FunctionId::new(CODE, 4);
const COUNT: FunctionId = FunctionId::new(CODE, 5);
const COUNTA: FunctionId = FunctionId::new(CODE, 6);
const COUNTBLANK: FunctionId = FunctionId::new(CODE, 7);
const AVERAGE: FunctionId = FunctionId::new(CODE, 8);
const AVERAGEA: FunctionId = FunctionId::new(CODE, 9);
const MIN: FunctionId = FunctionId::new(CODE, 10);
const MAX: FunctionId = FunctionId::new(CODE, 11);
const MINA: FunctionId = FunctionId::new(CODE, 12);
const MAXA: FunctionId = FunctionId::new(CODE, 13);

const NUMBERS: &[Parameter] = &[
    Parameter::value_or_reference("number1"),
    Parameter::value_or_reference("number2"),
];
const VALUES: &[Parameter] = &[
    Parameter::value_or_reference("value1"),
    Parameter::value_or_reference("value2"),
];
const ARRAYS: &[Parameter] = &[
    Parameter::value_or_reference("array1"),
    Parameter::value_or_reference("array2"),
];
const RANGE: &[Parameter] = &[Parameter::reference("range")];

/// An eager named call that takes 1 to 255 arguments, the last parameter repeating.
const fn aggregate(
    id: FunctionId,
    spelling: &'static str,
    parameters: &'static [Parameter],
    kernel: EagerKernel,
) -> CatalogueEntry {
    CatalogueEntry {
        id,
        spelling,
        notation: Notation::Call,
        arity: CallableArityShape::range(1, 255),
        parameters,
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(kernel),
    }
}

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        aggregate(SUM, "SUM", NUMBERS, sum),
        aggregate(PRODUCT, "PRODUCT", NUMBERS, product),
        aggregate(SUMSQ, "SUMSQ", NUMBERS, sumsq),
        aggregate(SUMPRODUCT, "SUMPRODUCT", ARRAYS, sumproduct),
        aggregate(COUNT, "COUNT", VALUES, count),
        aggregate(COUNTA, "COUNTA", VALUES, counta),
        CatalogueEntry {
            id: COUNTBLANK,
            spelling: "COUNTBLANK",
            notation: Notation::Call,
            arity: CallableArityShape::exact(1),
            parameters: RANGE,
            demand: ArgumentDemand::Eager,
            kernel: Kernel::Eager(countblank),
        },
        aggregate(AVERAGE, "AVERAGE", NUMBERS, average),
        aggregate(AVERAGEA, "AVERAGEA", VALUES, averagea),
        aggregate(MIN, "MIN", NUMBERS, min),
        aggregate(MAX, "MAX", NUMBERS, max),
        aggregate(MINA, "MINA", VALUES, mina),
        aggregate(MAXA, "MAXA", VALUES, maxa),
    ],
};

/// SUM: the numbers added from 0. A total that overflows is #NUM!, even when a later number would
/// bring it back.
fn sum(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(SUM, services);
    let mut total = 0.0;
    let read = each_number(&site, arguments, services, Held::Numbers, &mut |n| {
        total = site.add(total, n)?;
        Ok(())
    });
    Site::finish(read.and_then(|()| site.number(total)))
}

/// PRODUCT: the numbers multiplied from 1, or 0 when there is none.
fn product(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(PRODUCT, services);
    let mut running: Option<f64> = None;
    let read = each_number(&site, arguments, services, Held::Numbers, &mut |n| {
        running = Some(times(&site, running.unwrap_or(1.0), n)?);
        Ok(())
    });
    Site::finish(read.and_then(|()| site.number(running.unwrap_or(0.0))))
}

/// SUMSQ: each number squared, the squares added from 0.
fn sumsq(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(SUMSQ, services);
    let mut total = 0.0;
    let read = each_number(&site, arguments, services, Held::Numbers, &mut |n| {
        total = site.add(total, times(&site, n, n)?)?;
        Ok(())
    });
    Site::finish(read.and_then(|()| site.number(total)))
}

/// AVERAGE: the numbers added from 0 and divided by how many there are; #DIV/0! when there is
/// none.
fn average(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    mean(AVERAGE, Held::Numbers, arguments, services)
}

/// AVERAGEA: AVERAGE, with held text read as 0 and held logicals as 1 and 0.
fn averagea(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    mean(AVERAGEA, Held::Values, arguments, services)
}

fn mean(
    id: FunctionId,
    held: Held,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    let (mut total, mut counted) = (0.0, 0u64);
    let read = each_number(&site, arguments, services, held, &mut |n| {
        total = site.add(total, n)?;
        counted += 1;
        Ok(())
    });
    // A count is below 2^53, so it converts to binary64 exactly.
    Site::finish(read.and_then(|()| site.number(site.divide(total, counted as f64)?)))
}

/// MIN: the smallest number, or 0 when there is none. Choosing one number needs no rounding.
fn min(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    extreme(MIN, Held::Numbers, |n, best| n < best, arguments, services)
}

/// MAX: the largest number, or 0 when there is none.
fn max(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    extreme(MAX, Held::Numbers, |n, best| n > best, arguments, services)
}

/// MINA: MIN, with held text read as 0 and held logicals as 1 and 0.
fn mina(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    extreme(MINA, Held::Values, |n, best| n < best, arguments, services)
}

/// MAXA: MAX, with held text read as 0 and held logicals as 1 and 0.
fn maxa(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    extreme(MAXA, Held::Values, |n, best| n > best, arguments, services)
}

fn extreme(
    id: FunctionId,
    held: Held,
    replaces: fn(f64, f64) -> bool,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    let mut best: Option<f64> = None;
    let read = each_number(&site, arguments, services, held, &mut |n| {
        if best.is_none_or(|best| replaces(n, best)) {
            best = Some(n);
        }
        Ok(())
    });
    Site::finish(read.and_then(|()| site.number(best.unwrap_or(0.0))))
}

/// COUNT: how many numbers there are. A held number counts; a direct argument counts when it is
/// a number, a logical, text that reads as a number, or omitted (read as 0). Nothing else counts,
/// and an error is not the result. Counting needs no rounding.
fn count(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(COUNT, services);
    let mut counted = 0u64;
    let read = each_value(&site, arguments, services, &mut |index, origin, value| {
        if counts_as_number(&site, index, origin, value)? {
            counted += 1;
        }
        Ok(())
    });
    Site::finish(read.and_then(|()| site.number(counted as f64)))
}

fn counts_as_number(
    site: &Site<'_>,
    index: usize,
    origin: Origin,
    value: &CalcValue,
) -> Step<bool> {
    match (origin, value.core()) {
        (_, CoreValue::Number(n)) => finite(site, index, *n).map(|()| true),
        (Origin::Direct, CoreValue::Logical(_) | CoreValue::Empty | CoreValue::Missing) => Ok(true),
        // Whether the text reads as a number decides; its value is not used, so nothing rounds.
        (Origin::Direct, CoreValue::Text(_)) => match coerce::number(value) {
            Ok(Numeric::Decimal(decimal)) => site.argument(index, decimal.nearest()).map(|_| true),
            Ok(Numeric::Number(_)) => Ok(true),
            Err(Stop::Error(_)) => Ok(false),
            Err(stop) => Err(site.stop(stop, Some(index))),
        },
        (_, CoreValue::Array(_) | CoreValue::Reference(_)) => Err(site.not_modelled(Some(index))),
        (_, CoreValue::Text(_) | CoreValue::Logical(_) | CoreValue::Error(_))
        | (Origin::Held, CoreValue::Empty | CoreValue::Missing) => Ok(false),
    }
}

/// COUNTA: how many values there are. Every direct argument counts, an omitted one included; a
/// held value counts unless its cell is blank, so empty text and an error count. Counting needs
/// no rounding.
fn counta(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(COUNTA, services);
    let mut counted = 0u64;
    let read = each_value(&site, arguments, services, &mut |index, origin, value| {
        match (origin, value.core()) {
            (_, CoreValue::Number(n)) => finite(&site, index, *n)?,
            (Origin::Held, CoreValue::Empty | CoreValue::Missing) => return Ok(()),
            (_, CoreValue::Array(_) | CoreValue::Reference(_)) => {
                return Err(site.not_modelled(Some(index)));
            }
            (_, CoreValue::Text(_) | CoreValue::Logical(_) | CoreValue::Error(_))
            | (Origin::Direct, CoreValue::Empty | CoreValue::Missing) => {}
        }
        counted += 1;
        Ok(())
    });
    Site::finish(read.and_then(|()| site.number(counted as f64)))
}

/// A number a count reads. A NaN or an infinity is no worksheet value and is refused.
fn finite(site: &Site<'_>, index: usize, n: f64) -> Step<()> {
    if n.is_finite() {
        Ok(())
    } else {
        Err(site.not_modelled(Some(index)))
    }
}

/// COUNTBLANK: the cells of its reference, a single area, that are blank or hold empty text: the
/// area's size less the cells the reader visits with any other value, so the cost follows the
/// reader's visits and not the area's size, and a blank cell the reader visits as Empty counts
/// as blank. A reader that visits more such cells than the area holds breaks its contract and
/// cannot supply the area. A reference of several areas is not modelled.
fn countblank(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(COUNTBLANK, services);
    Site::finish(blanks(&site, &arguments[0], services))
}

fn blanks(
    site: &Site<'_>,
    argument: &CalcValue,
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let CoreValue::Reference(reference) = argument.core() else {
        return Err(Exit::Fault(FunctionFault::ReferenceRequired {
            id: site.id(),
            index: 0,
        }));
    };
    let [area] = reference.areas() else {
        return Err(site.not_modelled(Some(0)));
    };
    let cells = reader(site, services, 0)?;
    let mut held = 0u64;
    read_reference(site, cells, reference, 0, &mut |_, _, value| {
        let blank = match value.core() {
            CoreValue::Empty | CoreValue::Missing => true,
            CoreValue::Text(text) => text.len_utf16_code_units() == 0,
            _ => false,
        };
        held += u64::from(!blank);
        Ok(())
    })?;
    let blank = area.cell_count().checked_sub(held).ok_or(Exit::Fault(
        FunctionFault::ReferenceUnreadable {
            id: site.id(),
            index: 0,
        },
    ))?;
    // At most 2^34 cells, which binary64 holds exactly.
    site.number(blank as f64)
}

/// SUMPRODUCT: its arguments, all of one shape, multiplied entry by entry and the products added
/// from 0, in row-major order. An entry that is not a number is 0, and an error entry is the
/// result. A scalar argument is a 1-by-1 array; an omitted one is #VALUE!. Arguments of different
/// shapes are #VALUE!, decided after every argument is read, so an error in any argument comes
/// first. A reference of several areas is not modelled.
///
/// An entry whose factors are all 0 has the product +0, which adds nothing to the total (a zero
/// total's sign aside, which the published +0 removes), so only the entries where some argument
/// holds a number are multiplied, and a whole column costs what the reader visits in it.
fn sumproduct(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(SUMPRODUCT, services);
    Site::finish(products(&site, arguments, services))
}

/// One SUMPRODUCT argument as an array: its shape and the numbers it holds by row-major offset,
/// in increasing order. Every other entry is 0.
struct Factor {
    rows: u64,
    columns: u64,
    numbers: Vec<(u64, f64)>,
}

fn products(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let mut factors = Vec::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().enumerate() {
        factors.push(factor(site, index, argument, services)?);
    }
    let shape = (factors[0].rows, factors[0].columns);
    if factors.iter().any(|f| (f.rows, f.columns) != shape) {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    }
    let mut next = vec![0usize; factors.len()];
    let mut total = 0.0;
    while let Some(offset) = factors
        .iter()
        .zip(&next)
        .filter_map(|(f, &at)| f.numbers.get(at).map(|&(offset, _)| offset))
        .min()
    {
        let mut entry: Option<f64> = None;
        for (f, at) in factors.iter().zip(next.iter_mut()) {
            let value = match f.numbers.get(*at) {
                Some(&(held, value)) if held == offset => {
                    *at += 1;
                    value
                }
                _ => 0.0,
            };
            entry = Some(match entry {
                None => value,
                Some(running) => times(site, running, value)?,
            });
        }
        total = site.add(total, entry.unwrap_or(0.0))?;
    }
    site.number(total)
}

fn factor(
    site: &Site<'_>,
    index: usize,
    argument: &CalcValue,
    services: &mut EvalServices<'_>,
) -> Step<Factor> {
    let entry = |value: &CalcValue| number_of(site, Held::Numbers, index, Origin::Held, value);
    match argument.core() {
        CoreValue::Reference(reference) => {
            let [area] = reference.areas() else {
                return Err(site.not_modelled(Some(index)));
            };
            let (first, columns) = (area.first(), u64::from(area.column_count()));
            let cells = reader(site, services, index)?;
            let mut numbers: Vec<(u64, f64)> = Vec::new();
            read_reference(site, cells, reference, index, &mut |_, key, value| {
                let Some(n) = entry(value)? else {
                    return Ok(());
                };
                let offset = u64::from(key.row() - first.row()) * columns
                    + u64::from(key.column() - first.column());
                if numbers.last().is_some_and(|&(last, _)| last >= offset) {
                    // Cells out of row-major order: the reader breaks its contract.
                    return Err(Exit::Fault(FunctionFault::ReferenceUnreadable {
                        id: site.id(),
                        index,
                    }));
                }
                numbers.push((offset, n));
                Ok(())
            })?;
            Ok(Factor {
                rows: u64::from(area.row_count()),
                columns,
                numbers,
            })
        }
        CoreValue::Array(array) => {
            let mut numbers = Vec::new();
            for (offset, element) in array.iter_row_major().enumerate() {
                if let Some(n) = entry(element)? {
                    numbers.push((offset as u64, n));
                }
            }
            let shape = array.shape();
            Ok(Factor {
                rows: shape.rows as u64,
                columns: shape.cols as u64,
                numbers,
            })
        }
        CoreValue::Empty | CoreValue::Missing => Err(Exit::Error(WorksheetErrorCode::Value)),
        CoreValue::Number(_) | CoreValue::Text(_) | CoreValue::Logical(_) | CoreValue::Error(_) => {
            Ok(Factor {
                rows: 1,
                columns: 1,
                numbers: entry(argument)?.map(|n| (0, n)).into_iter().collect(),
            })
        }
    }
}

/// `a * b` at an arithmetic site. Two non-zero factors whose product underflows to 0 or to a
/// subnormal are refused, because Excel's handling of an underflow is not modelled.
fn times(site: &Site<'_>, a: f64, b: f64) -> Step<f64> {
    let product = site.multiply(a, b)?;
    if a != 0.0 && b != 0.0 && (product == 0.0 || product.is_subnormal()) {
        Err(site.not_modelled(None))
    } else {
        Ok(product)
    }
}

/// Where a value an aggregate reads comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// A direct argument.
    Direct,
    /// A cell of a reference argument or an element of an array argument.
    Held,
}

/// How an aggregate reads the values that a reference or an array holds.
#[derive(Clone, Copy)]
enum Held {
    /// Numbers only: text, logicals and blank cells are skipped.
    Numbers,
    /// The A-suffixed rule: text is 0, TRUE is 1 and FALSE is 0; blank cells are skipped.
    Values,
}

/// The number that `value`, of argument `index`, gives an aggregate that reads held values by
/// `held`, or `None` when the value is skipped. A direct argument goes through worksheet number
/// coercion, which converts text that reads as a number at an arithmetic site.
fn number_of(
    site: &Site<'_>,
    held: Held,
    index: usize,
    origin: Origin,
    value: &CalcValue,
) -> Step<Option<f64>> {
    if origin == Origin::Direct {
        return site.number_argument(index, value).map(Some);
    }
    match (held, value.core()) {
        (_, CoreValue::Number(n)) => site.argument(index, coerce::operand(*n)).map(Some),
        (_, CoreValue::Error(code)) => Err(Exit::Error(*code)),
        (Held::Values, CoreValue::Text(_)) => Ok(Some(0.0)),
        (Held::Values, CoreValue::Logical(b)) => Ok(Some(if *b { 1.0 } else { 0.0 })),
        (Held::Numbers, CoreValue::Text(_) | CoreValue::Logical(_))
        | (_, CoreValue::Empty | CoreValue::Missing) => Ok(None),
        (_, CoreValue::Array(_) | CoreValue::Reference(_)) => Err(site.not_modelled(Some(index))),
    }
}

/// Calls `take` with every number the arguments give, in the reading order.
fn each_number(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    held: Held,
    take: &mut dyn FnMut(f64) -> Step<()>,
) -> Step<()> {
    each_value(
        site,
        arguments,
        services,
        &mut |index, origin, value| match number_of(site, held, index, origin, value)? {
            Some(n) => take(n),
            None => Ok(()),
        },
    )
}

/// Calls `visit` with every value of the arguments in the reading order: a direct argument as
/// itself, and each value a reference or an array holds. Stops at the first step that is not
/// `Ok`.
fn each_value(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    visit: &mut dyn FnMut(usize, Origin, &CalcValue) -> Step<()>,
) -> Step<()> {
    for (index, argument) in arguments.iter().enumerate() {
        match argument.core() {
            CoreValue::Reference(reference) => {
                let cells = reader(site, services, index)?;
                read_reference(site, cells, reference, index, &mut |_, _, value| {
                    visit(index, Origin::Held, value)
                })?;
            }
            CoreValue::Array(array) => {
                for element in array.iter_row_major() {
                    visit(index, Origin::Held, element)?;
                }
            }
            _ => visit(index, Origin::Direct, argument)?,
        }
    }
    Ok(())
}

/// The services' cell reader, which a kernel needs to read the reference at argument `index`.
pub(super) fn reader<'a, 's>(
    site: &Site<'_>,
    services: &'a mut EvalServices<'s>,
    index: usize,
) -> Step<&'a mut (dyn CellReader + 's)> {
    services
        .cells()
        .ok_or(Exit::Fault(FunctionFault::ReferenceUnreadable {
            id: site.id(),
            index,
        }))
}

/// Reads the held cells of every area of `reference`, the argument at `index`, in its order:
/// `visit` receives each cell's area, key and value. A reader that refuses an area, or visits a
/// cell outside it, cannot supply the area, and the call ends in ReferenceUnreadable.
pub(super) fn read_reference(
    site: &Site<'_>,
    cells: &mut dyn CellReader,
    reference: &ReferenceLike,
    index: usize,
    visit: &mut dyn FnMut(GridArea, CellKey, &CalcValue) -> Step<()>,
) -> Step<()> {
    let unreadable = || {
        Exit::Fault(FunctionFault::ReferenceUnreadable {
            id: site.id(),
            index,
        })
    };
    for &area in reference.areas() {
        let mut stopped = Ok(());
        let read = cells.read_area(area, &mut |key, value| {
            let step = if area.contains(key) {
                visit(area, key, value)
            } else {
                Err(unreadable())
            };
            match step {
                Ok(()) => ControlFlow::Continue(()),
                Err(exit) => {
                    stopped = Err(exit);
                    ControlFlow::Break(())
                }
            }
        });
        read.map_err(|Unreadable| unreadable())?;
        stopped?;
    }
    Ok(())
}

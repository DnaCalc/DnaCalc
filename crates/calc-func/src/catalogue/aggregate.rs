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
//! ExactUnavailable. The first error met in that order is the result, except that SUM returns an
//! error of its direct arguments first (see [`sum`]); the counts return none.
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
//! An array held inside an array (a GROUPBY result's cell, dc-h9u) is read through by SUM, COUNT
//! and COUNTA, each of its elements as an element of the outer array, and makes PRODUCT, SUMSQ,
//! AVERAGE, AVERAGEA, MIN, MAX, MINA and MAXA #N/A (Excel 16.0.20527, 2026-10-30: SUM of
//! `GROUPBY(k,v,LAMBDA(x,x))` adds every group's values, MAX of it is #N/A); SUMPRODUCT takes it as
//! no number. The other functions that read arrays here refuse it as not modelled. A reference
//! held inside an array (OFFSET or INDIRECT lifted over an array, dc-b1l) is read at its first
//! cell by SUM, AVERAGE and MAX, is no number to COUNT and a value to COUNTA, and is refused by
//! the others ([`HeldReference`]). A product below
//! binary64's normal range is 0, as Excel gives it (dc-bd5: PRODUCT(1E-200,1E-200),
//! SUMSQ(1E-200) and SUMPRODUCT(1E-200,1E-200) are 0 in Excel 16.0.20430).

use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{CellReader, EagerKernel, EvalServices, FunctionFault, Unreadable};
use crate::coerce::{self, Numeric, Stop};
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcArray, CalcValue, CallableArityShape, CellKey, CoreRef, GridArea, ReferenceLike,
    WorksheetErrorCode,
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

// dc-10y and dc-b1l, Excel 16.0.20527: SUM(A1:A3*2) intersects the range at the root;
// SUMPRODUCT(SUM(A1:A3*2)) is 12, and SUMPRODUCT(SUM(ABS(A1:A3))) intersects inside ABS.
const NUMBERS: &[Parameter] = &[
    Parameter::value_or_reference("number1").legacy(super::LegacyPlace::AggregateUnderArray),
    Parameter::value_or_reference("number2").legacy(super::LegacyPlace::AggregateUnderArrayRepeated),
];
const VALUES: &[Parameter] = &[
    Parameter::value_or_reference("value1").legacy(super::LegacyPlace::AggregateAtRoot),
    Parameter::value_or_reference("value2").legacy(super::LegacyPlace::AggregateAtRootRepeated),
];
const COUNT_VALUES: &[Parameter] = &[
    Parameter::value_or_reference("value1").legacy(super::LegacyPlace::AggregateUnderArray),
    Parameter::value_or_reference("value2").legacy(super::LegacyPlace::AggregateUnderArrayRepeated),
];
const ARRAYS: &[Parameter] = &[
    Parameter::value_or_reference("array1").legacy(super::LegacyPlace::ArrayAtRoot),
    Parameter::value_or_reference("array2").legacy(super::LegacyPlace::ArrayAtRootRepeated),
];
/// COUNTBLANK's range takes only an operand that may give a reference (dc-mx3x, Excel 16.0.20527,
/// 2026-11-07, Formula2 and Formula alike). Refused: `=COUNTBLANK(2)`, `=COUNTBLANK({"",1,#N/A})`,
/// `=COUNTBLANK("")`, `=COUNTBLANK(TRUE)`, `=COUNTBLANK(#N/A)`, `=COUNTBLANK((2))`,
/// `=COUNTBLANK(1+1)`, `=COUNTBLANK(-A1)`, `=COUNTBLANK(A1:A2+0)`, `=COUNTBLANK(A1&"")`,
/// `=COUNTBLANK(SUM(A1:A2))`, `=COUNTBLANK(TRUE())`, `=COUNTBLANK(IFERROR(A1,1))`. Entered:
/// `=COUNTBLANK(A1:A2)` (1), `=COUNTBLANK((A1:A2))`, `=COUNTBLANK(A1:A2 A1)`,
/// `=COUNTBLANK((A1,A2))` (#VALUE!), `=COUNTBLANK(INDEX(A1:A2,1))`, `=COUNTBLANK(OFFSET(A1,0,0))`,
/// `=COUNTBLANK(IF(TRUE,A1:A2))`, `=COUNTBLANK(IF(TRUE,1))` (#VALUE!), `=COUNTBLANK(LET(x,1,x))`
/// (#VALUE!), `=COUNTBLANK(LAMBDA(x,x))` (#VALUE!), `=COUNTBLANK(@A1:A2)`, `=COUNTBLANK(B1#)`
/// and `=COUNTBLANK(nm)` for a name `nm` defined as `=1` (#VALUE!) or not defined (#NAME?).
const RANGE: &[Parameter] =
    &[Parameter::value_or_reference("range").entry(super::EntryClass::Reference)];


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
        groups: None,
        parameters,
        result: super::ResultForm::Value,
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
        aggregate(COUNT, "COUNT", COUNT_VALUES, count),
        aggregate(COUNTA, "COUNTA", COUNT_VALUES, counta),
        CatalogueEntry {
            id: COUNTBLANK,
            spelling: "COUNTBLANK",
            notation: Notation::Call,
            arity: CallableArityShape::exact(1),
            groups: None,
            parameters: RANGE,
            result: super::ResultForm::Value,
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

/// SUM: the numbers added from 0, in the reading order. The error is the first among the direct
/// arguments, in argument order, a coercion's #VALUE! included, before any error that a reference
/// or an array holds; among those, the first in the reading order. A total that overflows is
/// #NUM!, even when a later number would bring it back, and an error read after the overflow is
/// the result. The error_order rows of cfunc-m1.2's oracle workbook measure each of these orders
/// (with #N/A in B1, #DIV/0! in A2 and numbers in A1 and B2): =SUM(A1:B2,#NULL!) is #NULL!,
/// =SUM(B1,"a") and =SUM({1,#REF!},"a") are #VALUE!, =SUM({1,#REF!},#N/A) and
/// =SUM(#N/A,{1,#REF!}) are #N/A, =SUM({1,#REF!},A1:B2) is #REF! and =SUM(A1:B2,{1,#REF!}) #N/A,
/// and =SUM(9.99999999999999E+307,9.99999999999999E+307,B1) is #N/A.
fn sum(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(SUM, services);
    let mut total = 0.0;
    let read_through = read_through_held_arrays(arguments);
    let arguments: &[CalcValue] = read_through.as_deref().unwrap_or(arguments);
    let held = match read_held_references(&site, services, arguments, HeldReference::FirstCell) {
        Ok(held) => held,
        Err(exit) => return Site::finish(Err(exit)),
    };
    let arguments: &[CalcValue] = held.as_deref().unwrap_or(arguments);
    let read = direct_first(&site, arguments).and_then(|()| {
        each_number(&site, arguments, services, Held::Numbers, &mut |n| {
            total = site.add(total, n)?;
            Ok(())
        })
    });
    Site::finish(read.and_then(|()| site.number(total)))
}

/// Reads the direct arguments through worksheet number coercion, in order, and stops at the first
/// that is an error or that coercion stops on. Their numbers are not used: the fold reads them
/// again in the reading order.
pub(super) fn direct_first(site: &Site<'_>, arguments: &[CalcValue]) -> Step<()> {
    for (index, argument) in arguments.iter().enumerate() {
        if !matches!(
            argument.core_ref(),
            CoreRef::Reference(_) | CoreRef::Array(_)
        ) {
            site.number_argument(index, argument)?;
        }
    }
    Ok(())
}

/// PRODUCT: the numbers multiplied from 1, or 0 when there is none. An omitted argument is passed
/// over (dc-bd5: PRODUCT(2,) is 2 in Excel 16.0.20430, where SUM and the others read it as 0).
fn product(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(PRODUCT, services);
    if holds_held_arrays(arguments) {
        return Site::finish(Err(Exit::Error(WorksheetErrorCode::NA)));
    }
    let given: Vec<CalcValue> = arguments
        .iter()
        .filter(|argument| !argument.is_missing())
        .cloned()
        .collect();
    let arguments = given.as_slice();
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
    if holds_held_arrays(arguments) {
        return Site::finish(Err(Exit::Error(WorksheetErrorCode::NA)));
    }
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
    mean(AVERAGEA, Held::AverageValues, arguments, services)
}

fn mean(
    id: FunctionId,
    held: Held,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    if holds_held_arrays(arguments) {
        return Site::finish(Err(Exit::Error(WorksheetErrorCode::NA)));
    }
    let held_references = if id == AVERAGE {
        match read_held_references(&site, services, arguments, HeldReference::FirstCell) {
            Ok(held) => held,
            Err(exit) => return Site::finish(Err(exit)),
        }
    } else {
        None
    };
    let arguments: &[CalcValue] = held_references.as_deref().unwrap_or(arguments);
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
    if holds_held_arrays(arguments) {
        return Site::finish(Err(Exit::Error(WorksheetErrorCode::NA)));
    }
    let held_references = if id == MAX {
        match read_held_references(&site, services, arguments, HeldReference::FirstCell) {
            Ok(held) => held,
            Err(exit) => return Site::finish(Err(exit)),
        }
    } else {
        None
    };
    let arguments: &[CalcValue] = held_references.as_deref().unwrap_or(arguments);
    let mut best: Option<CalcValue> = None;
    let read = each_value(&site, arguments, services, &mut |index, origin, value| {
        let n = if value.is_excel_overflow() {
            value.as_number()
        } else {
            number_of(&site, held, index, origin, value)?
        };
        let Some(n) = n else { return Ok(()); };
        let replace = if let Some(best) = &best
            && (best.is_excel_overflow() || value.is_excel_overflow())
        {
            let order = super::comparison::overflow_numbers(&site, n, best.as_number().expect("selected number"))?;
            if id == MIN || id == MINA { order.is_lt() } else { order.is_gt() }
        } else {
            best.as_ref().is_none_or(|best| {
            let prior = best.as_number().expect("selected number");
            replaces(n, prior)
            })
        };
        if replace {
            best = Some(if value.is_excel_overflow() { value.clone() } else { CalcValue::number(n) });
        }
        Ok(())
    });
    Site::finish(read.and_then(|()| match best {
        Some(value) if value.is_excel_overflow() => Ok(value),
        Some(value) => site.number(value.as_number().expect("selected number")),
        None => site.number(0.0),
    }))
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
    let read_through = read_through_held_arrays(arguments);
    let arguments: &[CalcValue] = read_through.as_deref().unwrap_or(arguments);
    let held = match read_held_references(&site, services, arguments, HeldReference::NoNumber) {
        Ok(held) => held,
        Err(exit) => return Site::finish(Err(exit)),
    };
    let arguments: &[CalcValue] = held.as_deref().unwrap_or(arguments);
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
    match (origin, value.core_ref()) {
        (_, CoreRef::Number(n)) => finite(site, index, n).map(|()| true),
        (Origin::Direct, CoreRef::Logical(_) | CoreRef::Empty | CoreRef::Missing) => Ok(true),
        // Whether the text reads as a number decides; its value is not used, so nothing rounds.
        (Origin::Direct, CoreRef::Text(_)) => match coerce::number(value) {
            Ok(Numeric::Decimal(decimal)) => match decimal.nearest() {
                Ok(_) => Ok(true),
                // Decimal text beyond the range Excel reads is no number, not an error.
                Err(Stop::Error(_)) => Ok(false),
                Err(stop) => Err(site.stop(stop, Some(index))),
            },
            Ok(Numeric::Number(_)) => Ok(true),
            Err(Stop::Error(_)) => Ok(false),
            // Text coercion alone does not read (a date without a year, dc-bd5: COUNT("1/2") is
            // 1): read as VALUE reads it under the call's environment.
            Err(Stop::NotModelled) => match site.read_text(index, value) {
                Ok(Some(_)) => Ok(true),
                Ok(None) => Err(site.not_modelled(Some(index))),
                Err(Exit::Error(_)) => Ok(false),
                Err(exit) => Err(exit),
            },
        },
        (_, CoreRef::Array(_) | CoreRef::Reference(_)) => Err(site.not_modelled(Some(index))),
        (_, CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Error(_))
        | (Origin::Held | Origin::Element, CoreRef::Empty | CoreRef::Missing) => Ok(false),
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
    let read_through = read_through_held_arrays(arguments);
    let arguments: &[CalcValue] = read_through.as_deref().unwrap_or(arguments);
    let held = match read_held_references(&site, services, arguments, HeldReference::Counted) {
        Ok(held) => held,
        Err(exit) => return Site::finish(Err(exit)),
    };
    let arguments: &[CalcValue] = held.as_deref().unwrap_or(arguments);
    let read = each_value(&site, arguments, services, &mut |index, origin, value| {
        match (origin, value.core_ref()) {
            (_, CoreRef::Number(n)) => finite(&site, index, n)?,
            (Origin::Held | Origin::Element, CoreRef::Empty | CoreRef::Missing) => return Ok(()),
            (_, CoreRef::Array(_) | CoreRef::Reference(_)) => {
                return Err(site.not_modelled(Some(index)));
            }
            (_, CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Error(_))
            | (Origin::Direct, CoreRef::Empty | CoreRef::Missing) => {}
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
/// cannot supply the area. A reference of several areas is #VALUE! (dc-bd5: COUNTBLANK((A5,A6))).
/// A computed scalar is #VALUE!; calc-fml lifts computed arrays over this scalar rule (W111
/// count-family and w089-axis-reference-vs-array-variant readings in OxFunc-Evidence).
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
    if argument.callable_value().is_some() {
        return Err(site.not_modelled(Some(0)));
    }
    let reference = match argument.core_ref() {
        CoreRef::Reference(reference) => reference,
        CoreRef::Error(code) => return Err(Exit::Error(code)),
        // Retained fn2 q187: an omitted optional LAMBDA parameter is #VALUE!.
        CoreRef::Missing => return Err(Exit::Error(WorksheetErrorCode::Value)),
        // Array evaluation belongs to calc-fml.
        CoreRef::Array(_) => {
            return Err(Exit::Fault(FunctionFault::ReferenceRequired {
                id: site.id(),
                index: 0,
            }));
        }
        _ => return Err(Exit::Error(WorksheetErrorCode::Value)),
    };
    let [area] = reference.areas() else {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    };
    let cells = reader(site, services, 0)?;
    let mut held = 0u64;
    read_reference(site, cells, reference, 0, &mut |_, _, value| {
        let blank = match value.core_ref() {
            CoreRef::Empty | CoreRef::Missing => true,
            CoreRef::Text(text) => text.len_utf16_code_units() == 0,
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
/// first. A reference of several areas is #VALUE! (dc-bd5: SUMPRODUCT((A1,A7))).
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
    match argument.core_ref() {
        CoreRef::Reference(reference) => {
            let [area] = reference.areas() else {
                return Err(Exit::Error(WorksheetErrorCode::Value));
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
        CoreRef::Array(array) => {
            let mut numbers = Vec::new();
            for (offset, element) in array.iter_row_major().enumerate() {
                // An array an element holds (dc-h9u) is no number.
                if element.as_array().is_some() && element.callable_value().is_none() {
                    continue;
                }
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
        CoreRef::Empty | CoreRef::Missing => Err(Exit::Error(WorksheetErrorCode::Value)),
        CoreRef::Number(_) | CoreRef::Text(_) | CoreRef::Logical(_) | CoreRef::Error(_) => {
            Ok(Factor {
                rows: 1,
                columns: 1,
                numbers: entry(argument)?.map(|n| (0, n)).into_iter().collect(),
            })
        }
    }
}

/// `a * b` at an arithmetic site; a product below binary64's normal range is 0, as Excel flushes
/// it (dc-bd5).
fn times(site: &Site<'_>, a: f64, b: f64) -> Step<f64> {
    let product = site.multiply(a, b)?;
    if product.is_subnormal() {
        Ok(0.0)
    } else {
        Ok(product)
    }
}

/// Whether an array argument holds an array in a cell (dc-h9u).
fn holds_held_arrays(arguments: &[CalcValue]) -> bool {
    arguments.iter().any(|argument| {
        argument.callable_value().is_none()
            && argument.as_array().is_some_and(|array| array.holds_arrays())
    })
}

/// The arguments with each array argument that holds arrays in its cells read through: the held
/// arrays' elements in the place of the cell, in row-major order, as one row (dc-h9u: SUM, COUNT
/// and COUNTA read a held array's elements as the outer array's); `None`, allocating nothing,
/// when no argument holds one.
fn read_through_held_arrays(arguments: &[CalcValue]) -> Option<Vec<CalcValue>> {
    if !holds_held_arrays(arguments) {
        return None;
    }
    let read = arguments
        .iter()
        .map(|argument| match argument.as_array() {
            Some(array) if argument.callable_value().is_none() && array.holds_arrays() => {
                let mut flat = Vec::new();
                flatten(array, &mut flat);
                let shape = crate::value::ArrayShape {
                    rows: 1,
                    cols: flat.len(),
                };
                CalcValue::array(CalcArray::new(shape, flat).expect("at least one element"))
            }
            _ => argument.clone(),
        })
        .collect();
    Some(read)
}

fn flatten(array: &CalcArray, out: &mut Vec<CalcValue>) {
    for element in array.iter_row_major() {
        match element.as_array() {
            Some(held) if element.callable_value().is_none() => flatten(held, out),
            _ => out.push(element.clone()),
        }
    }
}

/// How an aggregate reads a reference an array argument holds (dc-b1l: OFFSET or INDIRECT lifted
/// over an array; Excel 16.0.20527, 2026-10-30).
#[derive(Clone, Copy, PartialEq, Eq)]
enum HeldReference {
    /// As the value of its first cell (SUM, AVERAGE, MAX: `=SUM(OFFSET(A1,{0;1;2},0,2))` is 6
    /// over 1..6 in A1:A6).
    FirstCell,
    /// As no number (COUNT: `=COUNT(OFFSET(A1,{0,1,2},0))` is 0).
    NoNumber,
    /// As a value (COUNTA: 3 for the same).
    Counted,
}

/// The arguments with each reference an array argument holds read by `how`; `None`, allocating
/// nothing, when no array argument holds one.
fn read_held_references(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    arguments: &[CalcValue],
    how: HeldReference,
) -> Step<Option<Vec<CalcValue>>> {
    let holds = |argument: &CalcValue| {
        argument.callable_value().is_none()
            && argument.as_array().is_some_and(|array| {
                array
                    .iter_row_major()
                    .any(|element| matches!(element.core_ref(), CoreRef::Reference(_)))
            })
    };
    if !arguments.iter().any(holds) {
        return Ok(None);
    }
    let mut read = Vec::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().enumerate() {
        let Some(array) = argument.as_array().filter(|_| holds(argument)) else {
            read.push(argument.clone());
            continue;
        };
        let mut cells = Vec::with_capacity(array.cell_count());
        for element in array.iter_row_major() {
            let CoreRef::Reference(reference) = element.core_ref() else {
                cells.push(element.clone());
                continue;
            };
            cells.push(match how {
                HeldReference::NoNumber => CalcValue::text(crate::value::ExcelText::from_utf16_code_units(Vec::new())),
                HeldReference::Counted => CalcValue::logical(true),
                HeldReference::FirstCell => {
                    let first = reference.areas()[0].first();
                    let mut value = CalcValue::empty();
                    let cells_reader = reader(site, services, index)?;
                    cells_reader
                        .read_area(GridArea::cell(first), &mut |_, held| {
                            value = held.clone();
                            ControlFlow::Break(())
                        })
                        .map_err(|Unreadable| {
                            Exit::Fault(FunctionFault::ReferenceUnreadable { id: site.id(), index })
                        })?;
                    value
                }
            });
        }
        read.push(CalcValue::array(
            CalcArray::new(array.shape(), cells).expect("the array's own shape"),
        ));
    }
    Ok(Some(read))
}

/// Where a value an aggregate reads comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    /// A direct argument.
    Direct,
    /// A cell of a reference argument.
    Held,
    /// An element of an array argument.
    Element,
}

/// How an aggregate reads the values that a reference or an array holds.
#[derive(Clone, Copy)]
pub(super) enum Held {
    /// Numbers only: text, logicals and blank cells are skipped.
    Numbers,
    /// MODE skips text/logicals, but a direct blank or omission is #VALUE!.
    /// A single-cell reference follows that scalar rule; larger ranges skip blanks.
    ModeNumbers,
    /// The A-suffixed rule: in a reference, text is 0, TRUE is 1 and FALSE is 0; blank cells are
    /// skipped. In an array text and logicals are skipped (dc-bd5: Excel 16.0.20527 gives
    /// MAXA({-1,"a"}) -1, MINA({1,TRUE}) 1, STDEVA({1,"a",3}) 1.414... and VARA({1,TRUE,3}) 2).
    Values,
    /// AVERAGEA's rule: [`Held::Values`], except that text in an array is 0 (dc-bd5: Excel
    /// 16.0.20527 gives AVERAGEA({1,"2",TRUE}) and AVERAGEA({1,"a"}) 0.5, AVERAGEA({1,TRUE}) 1).
    AverageValues,
}

/// The number that `value`, of argument `index`, gives an aggregate that reads held values by
/// `held`, or `None` when the value is skipped. A direct argument goes through worksheet number
/// coercion, which converts text that reads as a number at an arithmetic site.
pub(super) fn number_of(
    site: &Site<'_>,
    held: Held,
    index: usize,
    origin: Origin,
    value: &CalcValue,
) -> Step<Option<f64>> {
    if matches!(held, Held::ModeNumbers)
        && matches!(value.core_ref(), CoreRef::Text(_) | CoreRef::Logical(_))
    {
        return Ok(None);
    }
    if origin == Origin::Direct {
        if matches!(held, Held::ModeNumbers)
            && matches!(value.core_ref(), CoreRef::Empty | CoreRef::Missing)
        {
            return Err(Exit::Error(WorksheetErrorCode::Value));
        }
        return site.number_argument(index, value).map(Some);
    }
    match (held, value.core_ref()) {
        (_, CoreRef::Number(_)) if value.is_excel_overflow() => Err(Exit::Error(WorksheetErrorCode::Num)),
        (_, CoreRef::Number(n)) => site.argument(index, coerce::operand(n)).map(Some),
        (_, CoreRef::Error(code)) => Err(Exit::Error(code)),
        (Held::AverageValues, CoreRef::Text(_)) => Ok(Some(0.0)),
        (Held::Values, CoreRef::Text(_)) if origin == Origin::Held => Ok(Some(0.0)),
        (Held::Values | Held::AverageValues, CoreRef::Logical(b)) if origin == Origin::Held => {
            Ok(Some(if b { 1.0 } else { 0.0 }))
        }
        (Held::Values | Held::AverageValues, CoreRef::Text(_) | CoreRef::Logical(_)) => Ok(None),
        (Held::Numbers | Held::ModeNumbers, CoreRef::Text(_) | CoreRef::Logical(_))
        | (_, CoreRef::Empty | CoreRef::Missing) => Ok(None),
        (_, CoreRef::Array(_) | CoreRef::Reference(_)) => Err(site.not_modelled(Some(index))),
    }
}

/// Calls `take` with every number the arguments give, in the reading order.
pub(super) fn each_number(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    held: Held,
    take: &mut dyn FnMut(f64) -> Step<()>,
) -> Step<()> {
    read_values(
        site,
        arguments,
        services,
        matches!(held, Held::ModeNumbers),
        &mut |index, origin, value| match number_of(site, held, index, origin, value)? {
            Some(n) => take(n),
            None => Ok(()),
        },
    )
}

/// Calls `visit` with every value of the arguments in the reading order: a direct argument as
/// itself, and each value a reference or an array holds. Stops at the first step that is not
/// `Ok`.
pub(super) fn each_value(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    visit: &mut dyn FnMut(usize, Origin, &CalcValue) -> Step<()>,
) -> Step<()> {
    read_values(site, arguments, services, false, visit)
}

/// MODE's scalar-cell rule uses the same reader and argument order. A sparse
/// reader need not visit a blank cell, so retain Empty until a value is visited.
fn read_values(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    scalar_references: bool,
    visit: &mut dyn FnMut(usize, Origin, &CalcValue) -> Step<()>,
) -> Step<()> {
    for (index, argument) in arguments.iter().enumerate() {
        match argument.core_ref() {
            CoreRef::Reference(reference) => {
                let cells = reader(site, services, index)?;
                if scalar_references && matches!(reference.areas(), [area]
                    if area.row_count() == 1 && area.column_count() == 1)
                {
                    let mut value = CalcValue::empty();
                    read_reference(site, cells, reference, index, &mut |_, _, held| {
                        value = held.clone();
                        Ok(())
                    })?;
                    visit(index, Origin::Direct, &value)?;
                } else {
                    read_reference(site, cells, reference, index, &mut |_, _, value| {
                        visit(index, Origin::Held, value)
                    })?;
                }
            }
            CoreRef::Array(array) => {
                for element in array.iter_row_major() {
                    visit(index, Origin::Element, element)?;
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

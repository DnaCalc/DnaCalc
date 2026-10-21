//! The logical family fragment: IF and its lazy kernel; AND, OR and XOR over the logicals of their
//! arguments; NOT; and TRUE and FALSE.
//!
//! AND, OR and XOR read every argument, also after a value that already decides the result, in
//! two passes: first the direct arguments, in argument order; then the references and arrays, in
//! argument order, a reference's areas in order and each area's cells in row-major order through
//! the services' cell reader (which visits the cells that hold a value and may also visit blank
//! cells as Empty, so a kernel's work follows the cells the reader visits and not the area's
//! size), an array's elements in row-major order. The first error met is the result, so an error
//! given directly comes before any error a reference or an array holds, wherever it stands among
//! the arguments. Otherwise the result follows from the logicals read, and with none at all it is
//! #VALUE!.
//!
//! A direct argument and a value that a reference or an array holds follow different rules. A
//! number is TRUE unless it is 0 and a logical is itself, either way. Direct text that spells
//! TRUE or FALSE as [`coerce::spelled_logical`] reads it is that logical; any other direct text is
//! skipped: text with a digit, empty text, an error's spelling, a spelling with a space or in
//! fullwidth letters. An omitted argument is a FALSE, and so is a blank cell's value given directly
//! (IFNA and IFERROR return one as it is). Held text, held empty text and blank cells are skipped,
//! the spellings TRUE and FALSE included. None of this rounds, so the kernels compute the same
//! under either numeric mode.
//!
//! What is not modelled, and refused naming the argument: a callable (a LAMBDA, whose
//! worksheet-visible core is #CALC! but which is no worksheet error until it reaches a cell), given
//! directly or held in an array; an array or a reference held inside an array; and a number that
//! is no worksheet value (NaN, an infinity or a subnormal).

use super::aggregate::{read_reference, reader};
use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{EagerKernel, EvalServices, FunctionFault, LazyArguments};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CallableArityShape, CoreValue, WorksheetErrorCode};

const CODE: u16 = 5;

const IF: FunctionId = FunctionId::new(CODE, 1);
const AND: FunctionId = FunctionId::new(CODE, 2);
const OR: FunctionId = FunctionId::new(CODE, 3);
const XOR: FunctionId = FunctionId::new(CODE, 4);
const NOT: FunctionId = FunctionId::new(CODE, 5);
const TRUE: FunctionId = FunctionId::new(CODE, 6);
const FALSE: FunctionId = FunctionId::new(CODE, 7);

const LOGICALS: &[Parameter] = &[
    Parameter::value_or_reference("logical1"),
    Parameter::value_or_reference("logical2"),
];

/// An eager named call that takes 1 to `most` logicals, values or references, the last parameter
/// repeating.
const fn logicals(
    id: FunctionId,
    spelling: &'static str,
    most: usize,
    kernel: EagerKernel,
) -> CatalogueEntry {
    CatalogueEntry {
        id,
        spelling,
        notation: Notation::Call,
        arity: CallableArityShape::range(1, most),
        parameters: LOGICALS,
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(kernel),
    }
}

/// An eager named call that takes no argument.
const fn constant(id: FunctionId, spelling: &'static str, kernel: EagerKernel) -> CatalogueEntry {
    CatalogueEntry {
        id,
        spelling,
        notation: Notation::Call,
        arity: CallableArityShape::exact(0),
        parameters: &[],
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(kernel),
    }
}

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        CatalogueEntry {
            id: IF,
            spelling: "IF",
            notation: Notation::Call,
            arity: CallableArityShape::range(2, 3),
            parameters: &[
                Parameter::value("logical_test"),
                Parameter::value_or_reference("value_if_true"),
                Parameter::value_or_reference("value_if_false"),
            ],
            demand: ArgumentDemand::BranchOnCondition,
            kernel: Kernel::Lazy(if_selected),
        },
        logicals(AND, "AND", 255, and),
        logicals(OR, "OR", 255, or),
        // XOR takes one argument fewer than AND and OR: Microsoft's XOR page gives 1 to 254
        // conditions.
        logicals(XOR, "XOR", 254, xor),
        CatalogueEntry {
            id: NOT,
            spelling: "NOT",
            notation: Notation::Call,
            arity: CallableArityShape::exact(1),
            parameters: &[Parameter::value("logical")],
            demand: ArgumentDemand::Eager,
            kernel: Kernel::Eager(not),
        },
        constant(TRUE, "TRUE", true_value),
        constant(FALSE, "FALSE", false_value),
    ],
};

/// IF: the condition alone is asked for first and read as a logical; its error is the result.
/// Then only the chosen branch is asked for, and it is the result as it is (a reference stays a
/// reference); an omitted branch is 0, and a FALSE condition without a third argument is FALSE.
fn if_selected(
    arguments: &mut LazyArguments<'_>,
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(IF, services);
    let condition = arguments.argument(0)?;
    let branch = match coerce::logical(&condition) {
        Ok(true) => 1,
        Ok(false) if arguments.count() > 2 => 2,
        Ok(false) => return Ok(CalcValue::logical(false)),
        Err(stop) => return Site::finish(Err(site.stop(stop, Some(0)))),
    };
    let chosen = arguments.argument(branch)?;
    Ok(if chosen.is_missing() {
        CalcValue::number(0.0)
    } else {
        chosen
    })
}

/// AND: TRUE when every logical read is TRUE, FALSE when one is FALSE.
fn and(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    decide(AND, arguments, services, |tally| {
        tally.trues == tally.logicals
    })
}

/// OR: TRUE when a logical read is TRUE, FALSE when every one is FALSE.
fn or(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    decide(OR, arguments, services, |tally| tally.trues > 0)
}

/// XOR: TRUE when the number of TRUE logicals read is odd, FALSE when it is even.
fn xor(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    decide(XOR, arguments, services, |tally| tally.trues % 2 == 1)
}

/// The logicals that AND, OR or XOR read: how many, and how many of them are TRUE.
#[derive(Default)]
struct Tally {
    logicals: u64,
    trues: u64,
}

/// The result of the entry `id` on `arguments`: the first error met in the reading order, #VALUE!
/// when no logical is read, and otherwise the logical `rule` gives the tally.
fn decide(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    rule: fn(&Tally) -> bool,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    Site::finish(tally(&site, arguments, services).and_then(|tally| {
        if tally.logicals == 0 {
            Err(Exit::Error(WorksheetErrorCode::Value))
        } else {
            Ok(CalcValue::logical(rule(&tally)))
        }
    }))
}

/// Reads every argument in the two passes of the module documentation and counts the logicals;
/// stops at the first error or refusal.
fn tally(site: &Site<'_>, arguments: &[CalcValue], services: &mut EvalServices<'_>) -> Step<Tally> {
    let mut tally = Tally::default();
    let mut take = |logical: Option<bool>| {
        if let Some(logical) = logical {
            tally.logicals += 1;
            tally.trues += u64::from(logical);
        }
    };
    for (index, argument) in arguments.iter().enumerate() {
        if !matches!(
            argument.core(),
            CoreValue::Reference(_) | CoreValue::Array(_)
        ) {
            take(direct(site, index, argument)?);
        }
    }
    for (index, argument) in arguments.iter().enumerate() {
        match argument.core() {
            CoreValue::Reference(reference) => {
                let cells = reader(site, services, index)?;
                read_reference(site, cells, reference, index, &mut |_, _, value| {
                    take(held(site, index, value)?);
                    Ok(())
                })?;
            }
            CoreValue::Array(array) => {
                for element in array.iter_row_major() {
                    take(held(site, index, element)?);
                }
            }
            _ => {}
        }
    }
    Ok(tally)
}

/// A direct argument, at `index`, as a logical, or `None` when it is skipped.
fn direct(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<Option<bool>> {
    not_callable(site, index, value)?;
    match value.core() {
        CoreValue::Number(n) => site
            .argument(index, coerce::operand(*n))
            .map(|n| Some(n != 0.0)),
        CoreValue::Logical(b) => Ok(Some(*b)),
        CoreValue::Text(text) => Ok(coerce::spelled_logical(text)),
        CoreValue::Missing | CoreValue::Empty => Ok(Some(false)),
        CoreValue::Error(code) => Err(Exit::Error(*code)),
        CoreValue::Array(_) | CoreValue::Reference(_) => Err(site.not_modelled(Some(index))),
    }
}

/// A value that a reference or an array at `index` holds, as a logical, or `None` when it is
/// skipped.
fn held(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<Option<bool>> {
    not_callable(site, index, value)?;
    match value.core() {
        CoreValue::Number(n) => site
            .argument(index, coerce::operand(*n))
            .map(|n| Some(n != 0.0)),
        CoreValue::Logical(b) => Ok(Some(*b)),
        CoreValue::Text(_) | CoreValue::Empty | CoreValue::Missing => Ok(None),
        CoreValue::Error(code) => Err(Exit::Error(*code)),
        CoreValue::Array(_) | CoreValue::Reference(_) => Err(site.not_modelled(Some(index))),
    }
}

/// Refuses a callable at `index` as not modelled: its worksheet-visible core is #CALC!, but it is
/// no worksheet error until it reaches a cell.
fn not_callable(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<()> {
    if value.callable_value().is_some() {
        Err(site.not_modelled(Some(index)))
    } else {
        Ok(())
    }
}

/// NOT: its argument read as a logical (as IF reads its condition), reversed; the argument's
/// error is the result. An array argument, over which NOT lifts, and a callable are refused as not
/// modelled.
fn not(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(NOT, services);
    Site::finish(
        site.scalar_operands(arguments)
            .and_then(|()| not_callable(&site, 0, &arguments[0]))
            .and_then(|()| {
                let logical = site.argument(0, coerce::logical(&arguments[0]))?;
                Ok(CalcValue::logical(!logical))
            }),
    )
}

/// TRUE: the logical TRUE.
fn true_value(_: &[CalcValue], _: &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault> {
    Ok(CalcValue::logical(true))
}

/// FALSE: the logical FALSE.
fn false_value(_: &[CalcValue], _: &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault> {
    Ok(CalcValue::logical(false))
}

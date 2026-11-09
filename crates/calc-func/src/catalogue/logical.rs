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
use super::FunctionId;
use crate::call::{EvalServices, FunctionFault, LazyArguments};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

use calc_catalogue::logical::{CODE, ENTRIES as DECLARATIONS, IF, AND, OR, XOR, NOT, TRUE, FALSE};

attach_family! {
    code: CODE; declarations: DECLARATIONS;
    IF => Lazy(if_selected);
    AND => Eager(and);
    OR => Eager(or);
    XOR => Eager(xor);
    NOT => Eager(not);
    TRUE => Eager(true_value);
    FALSE => Eager(false_value);
}

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
            argument.core_ref(),
            CoreRef::Reference(_) | CoreRef::Array(_)
        ) {
            take(direct(site, index, argument)?);
        }
    }
    for (index, argument) in arguments.iter().enumerate() {
        match argument.core_ref() {
            CoreRef::Reference(reference) => {
                let cells = reader(site, services, index)?;
                read_reference(site, cells, reference, index, &mut |_, _, value| {
                    take(held(site, index, value)?);
                    Ok(())
                })?;
            }
            CoreRef::Array(array) => {
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
    match value.core_ref() {
        CoreRef::Number(n) => site
            .argument(index, coerce::operand(n))
            .map(|n| Some(n != 0.0)),
        CoreRef::Logical(b) => Ok(Some(b)),
        CoreRef::Text(text) => Ok(coerce::spelled_logical(text)),
        CoreRef::Missing | CoreRef::Empty => Ok(Some(false)),
        CoreRef::Error(code) => Err(Exit::Error(code)),
        CoreRef::Array(_) | CoreRef::Reference(_) => Err(site.not_modelled(Some(index))),
    }
}

/// A value that a reference or an array at `index` holds, as a logical, or `None` when it is
/// skipped.
fn held(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<Option<bool>> {
    not_callable(site, index, value)?;
    match value.core_ref() {
        CoreRef::Number(n) => site
            .argument(index, coerce::operand(n))
            .map(|n| Some(n != 0.0)),
        CoreRef::Logical(b) => Ok(Some(b)),
        CoreRef::Text(_) | CoreRef::Empty | CoreRef::Missing => Ok(None),
        CoreRef::Error(code) => Err(Exit::Error(code)),
        CoreRef::Array(_) | CoreRef::Reference(_) => Err(site.not_modelled(Some(index))),
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
                if let Some(n) = arguments[0].as_number() && n.is_subnormal() && n.is_sign_negative() {
                    return Ok(CalcValue::logical(false));
                }
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

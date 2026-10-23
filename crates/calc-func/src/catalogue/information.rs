//! The information family fragment: the IS functions, TYPE, N, NA and ERROR.TYPE, and their
//! kernels.
//!
//! Parameter classes follow where Excel inserts `@` (implicit intersection) before a range given to
//! a legacy formula (a workbook cell without dynamic-array metadata). It does for the argument of
//! ISBLANK, ISERR, ISERROR, ISNA, ISNUMBER, ISTEXT, ISNONTEXT, ISLOGICAL and ERROR.TYPE, so each
//! takes a value and the caller dereferences a reference to one cell before the call. It does not
//! for ISREF, ISEVEN, ISODD and N, so these take a value or a reference and read a reference here,
//! through the services' cell reader, where a cell the reader does not visit is blank. TYPE takes a
//! value or a reference too: Excel inserts `@` before its range of one area but not before a
//! reference of several areas. ISFORMULA takes a reference only.
//!
//! An array argument is refused as not modelled, except by TYPE (64) and ISREF (FALSE), before
//! which Excel inserts no `@`: Excel loads it in a legacy formula with `@` (its first element),
//! and in a dynamic-array formula array evaluation applies the function to it; both are array
//! evaluation's (cfml-m3.4). So is a callable argument, whose worksheet-visible core is #CALC! but
//! which no cell holds (LAMBDA, cfml-m2.3), and an omitted one: Excel refuses `=ISBLANK()`,
//! `=ISEVEN()`, `=TYPE()` and such calls with nothing between the parentheses at entry, so no Excel
//! result exists for one. A number that is not finite is no worksheet value and is refused.
//!
//! Excel's results of ISBLANK, ISERR, ISERROR, ISNA, TYPE, N and ERROR.TYPE over #BLOCKED! and
//! #CONNECT! are unknown: no formula known to give either error runs offline, and Excel refuses to
//! open a workbook that holds either as a cell constant. Each such call is refused as not modelled
//! (dc-tt73).
//!
//! ISFORMULA needs to know whether a cell holds a formula, which the cell reader does not supply;
//! every call is refused as not modelled until a host fact reaches the kernel (cfunc-m3.18).

use super::aggregate::{read_reference, reader};
use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{EagerKernel, EvalServices, FunctionFault};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcValue, CallableArityShape, CoreValue, GridArea, ReferenceLike, WorksheetErrorCode,
};

const CODE: u16 = 10;

const ISBLANK: FunctionId = FunctionId::new(CODE, 1);
const ISERR: FunctionId = FunctionId::new(CODE, 2);
const ISERROR: FunctionId = FunctionId::new(CODE, 3);
const ISNA: FunctionId = FunctionId::new(CODE, 4);
const ISNUMBER: FunctionId = FunctionId::new(CODE, 5);
const ISTEXT: FunctionId = FunctionId::new(CODE, 6);
const ISNONTEXT: FunctionId = FunctionId::new(CODE, 7);
const ISLOGICAL: FunctionId = FunctionId::new(CODE, 8);
const ISEVEN: FunctionId = FunctionId::new(CODE, 9);
const ISODD: FunctionId = FunctionId::new(CODE, 10);
const ISREF: FunctionId = FunctionId::new(CODE, 11);
const ISFORMULA: FunctionId = FunctionId::new(CODE, 12);
const TYPE: FunctionId = FunctionId::new(CODE, 13);
const N: FunctionId = FunctionId::new(CODE, 14);
const NA: FunctionId = FunctionId::new(CODE, 15);
const ERROR_TYPE: FunctionId = FunctionId::new(CODE, 16);

const VALUE: &[Parameter] = &[Parameter::value("value")];
const VALUE_OR_REFERENCE: &[Parameter] = &[Parameter::value_or_reference("value")];
const NUMBER: &[Parameter] = &[Parameter::value_or_reference("number")];
const REFERENCE: &[Parameter] = &[Parameter::reference("reference")];
const ERROR_VAL: &[Parameter] = &[Parameter::value("error_val")];

/// An eager named call of one argument.
const fn unary(
    id: FunctionId,
    spelling: &'static str,
    parameters: &'static [Parameter],
    kernel: EagerKernel,
) -> CatalogueEntry {
    CatalogueEntry {
        id,
        spelling,
        notation: Notation::Call,
        arity: CallableArityShape::exact(1),
        parameters,
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(kernel),
    }
}

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        unary(ISBLANK, "ISBLANK", VALUE, isblank),
        unary(ISERR, "ISERR", VALUE, iserr),
        unary(ISERROR, "ISERROR", VALUE, iserror),
        unary(ISNA, "ISNA", VALUE, isna),
        unary(ISNUMBER, "ISNUMBER", VALUE, isnumber),
        unary(ISTEXT, "ISTEXT", VALUE, istext),
        unary(ISNONTEXT, "ISNONTEXT", VALUE, isnontext),
        unary(ISLOGICAL, "ISLOGICAL", VALUE, islogical),
        unary(ISEVEN, "ISEVEN", NUMBER, iseven),
        unary(ISODD, "ISODD", NUMBER, isodd),
        unary(ISREF, "ISREF", VALUE_OR_REFERENCE, isref),
        unary(ISFORMULA, "ISFORMULA", REFERENCE, isformula),
        unary(TYPE, "TYPE", VALUE_OR_REFERENCE, type_of),
        unary(N, "N", VALUE_OR_REFERENCE, n),
        CatalogueEntry {
            id: NA,
            spelling: "NA",
            notation: Notation::Call,
            arity: CallableArityShape::exact(0),
            parameters: &[],
            demand: ArgumentDemand::Eager,
            kernel: Kernel::Eager(na),
        },
        unary(ERROR_TYPE, "ERROR.TYPE", ERROR_VAL, error_type),
    ],
};

/// The argument's worksheet value, or the refusal of a value no kernel here reads: an omitted
/// argument, an array (lifted or intersected by array evaluation), a callable and a number that is
/// not finite (see the module documentation).
fn scalar<'v>(site: &Site<'_>, value: &'v CalcValue) -> Step<&'v CoreValue> {
    match value.core() {
        _ if value.callable_value().is_some() => Err(site.not_modelled(Some(0))),
        CoreValue::Missing | CoreValue::Array(_) => Err(site.not_modelled(Some(0))),
        CoreValue::Number(n) if !n.is_finite() => Err(site.not_modelled(Some(0))),
        core => Ok(core),
    }
}

/// The argument's worksheet value as [`scalar`] reads it, or the refusal of #BLOCKED! and
/// #CONNECT!, over which the results of ISBLANK, ISERR, ISERROR, ISNA, TYPE and N are unknown (see
/// the module documentation).
fn known<'v>(site: &Site<'_>, value: &'v CalcValue) -> Step<&'v CoreValue> {
    match scalar(site, value)? {
        CoreValue::Error(WorksheetErrorCode::Blocked | WorksheetErrorCode::Connect) => {
            Err(site.not_modelled(Some(0)))
        }
        core => Ok(core),
    }
}

/// How a kernel reads its argument's value: [`scalar`] or [`known`].
type Read = for<'v> fn(&Site<'_>, &'v CalcValue) -> Step<&'v CoreValue>;

/// An IS predicate: `holds` applied to the argument's value as `read` gives it, which is not
/// converted (a number held as text is text, as Microsoft's IS functions page states). The caller
/// has dereferenced a reference to one cell, so a blank cell arrives as the empty value.
fn predicate(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &EvalServices<'_>,
    read: Read,
    holds: fn(&CoreValue) -> bool,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    Site::finish(read(&site, &arguments[0]).map(|core| CalcValue::logical(holds(core))))
}

/// ISBLANK: TRUE for a blank cell alone. Empty text, whether a cell holds it as a constant or a
/// formula gives it, is text and not blank.
fn isblank(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISBLANK, arguments, services, known, |core| {
        matches!(core, CoreValue::Empty)
    })
}

/// ISERR: TRUE for every worksheet error but #N/A. Microsoft's page names no error; Excel gives
/// TRUE for #GETTING_DATA, #CALC!, #FIELD!, #SPILL! and #BUSY! too.
fn iserr(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(
        ISERR,
        arguments,
        services,
        known,
        |core| matches!(core, CoreValue::Error(code) if *code != WorksheetErrorCode::NA),
    )
}

/// ISERROR: TRUE for every worksheet error, the newer ones as ISERR reads them.
fn iserror(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISERROR, arguments, services, known, |core| {
        matches!(core, CoreValue::Error(_))
    })
}

/// ISNA: TRUE for #N/A alone.
fn isna(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISNA, arguments, services, known, |core| {
        matches!(core, CoreValue::Error(WorksheetErrorCode::NA))
    })
}

/// ISNUMBER: TRUE for a number; number text and logicals are not numbers here.
fn isnumber(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISNUMBER, arguments, services, scalar, |core| {
        matches!(core, CoreValue::Number(_))
    })
}

/// ISTEXT: TRUE for text, empty text included.
fn istext(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISTEXT, arguments, services, scalar, |core| {
        matches!(core, CoreValue::Text(_))
    })
}

/// ISNONTEXT: TRUE for anything but text: a blank cell and an error included.
fn isnontext(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISNONTEXT, arguments, services, scalar, |core| {
        !matches!(core, CoreValue::Text(_))
    })
}

/// ISLOGICAL: TRUE for TRUE and FALSE; their text is text.
fn islogical(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISLOGICAL, arguments, services, scalar, |core| {
        matches!(core, CoreValue::Logical(_))
    })
}

/// ISREF: TRUE for a reference, of any number of cells or areas, blank cells included; FALSE for
/// every other value, an array and an error among them. Nothing is read.
fn isref(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ISREF, services);
    let argument = &arguments[0];
    let step = match argument.core() {
        CoreValue::Reference(_) => Ok(CalcValue::logical(true)),
        CoreValue::Array(_) => Ok(CalcValue::logical(false)),
        _ => scalar(&site, argument).map(|_| CalcValue::logical(false)),
    };
    Site::finish(step)
}

/// ISFORMULA: refused as not modelled for every reference (cfunc-m3.18); see the module
/// documentation. The call interface refuses any other argument at admission, as Excel refuses
/// such a formula at entry.
fn isformula(_: &[CalcValue], services: &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ISFORMULA, services);
    Site::finish(Err(site.not_modelled(Some(0))))
}

/// ISEVEN: whether the number, truncated toward zero, is even.
fn iseven(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ISEVEN, services);
    Site::finish(odd(&site, &arguments[0], services).map(|odd| CalcValue::logical(!odd)))
}

/// ISODD: whether the number, truncated toward zero, is odd.
fn isodd(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ISODD, services);
    Site::finish(odd(&site, &arguments[0], services).map(CalcValue::logical))
}

/// Excel's tolerance for ISEVEN and ISODD, which no page states: the number's magnitude plus this,
/// rounded once to binary64, is truncated, so a number less than about 1e-10 below an integer
/// counts as that integer.
const PARITY_TOLERANCE: f64 = 1e-10;

/// Whether the argument of ISEVEN or ISODD is odd. A reference to one cell is read as the cell's
/// value; a reference of several areas is #VALUE!, and so is a range of several cells, in a legacy
/// formula placed within the range's rows (no intersection) and in a dynamic-array formula (no
/// lifting) alike.
/// The value is read as a number through worksheet coercion, number text included (Excel reads
/// text such as "2" and "1e3" both typed and in a cell), except that a logical is #VALUE!, typed
/// or in a cell, as Microsoft's page says of a value that is not numeric; a blank cell is 0 and an
/// error is the result. The magnitude plus [`PARITY_TOLERANCE`] is an arithmetic site, which
/// rounds; its truncation toward zero and the remainder by 2 are exact. Every binary64 of 2^53 or
/// more is an even integer.
fn odd(site: &Site<'_>, argument: &CalcValue, services: &mut EvalServices<'_>) -> Step<bool> {
    let value = match argument.core() {
        CoreValue::Reference(reference) => match one_cell(reference) {
            Some(area) => referenced(site, area, services)?,
            None => return Err(Exit::Error(WorksheetErrorCode::Value)),
        },
        _ => argument.clone(),
    };
    let number = match scalar(site, &value)? {
        CoreValue::Logical(_) => return Err(Exit::Error(WorksheetErrorCode::Value)),
        _ => site.number_argument(0, &value)?,
    };
    Ok(site.add(number.abs(), PARITY_TOLERANCE)?.trunc() % 2.0 != 0.0)
}

/// TYPE: 1 for a number or a blank cell, 2 for text (empty text included), 4 for a logical, 16
/// for an error and 64 for an array; Microsoft's page lists all but the blank cell. A reference to
/// one cell gives the type of the cell's value, and a reference of several areas is 16. A reference
/// to one area of several cells is refused as not modelled: Excel loads it in a legacy formula
/// with `@`, whose implicit intersection cfml-m3.3 owns, and its result in a dynamic-array formula
/// is unknown (cfml-m3.4). Compound data (128) is not modelled: a callable is refused. #BLOCKED!
/// and #CONNECT! are refused (see [`known`]).
fn type_of(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(TYPE, services);
    let argument = &arguments[0];
    let step = match argument.core() {
        CoreValue::Array(_) => Ok(64.0),
        CoreValue::Reference(reference) if reference.area_count() > 1 => Ok(16.0),
        CoreValue::Reference(reference) => match one_cell(reference) {
            Some(area) => {
                referenced(&site, area, services).and_then(|value| type_code(&site, &value))
            }
            None => Err(site.not_modelled(Some(0))),
        },
        _ => type_code(&site, argument),
    };
    Site::finish(step.map(CalcValue::number))
}

fn type_code(site: &Site<'_>, value: &CalcValue) -> Step<f64> {
    Ok(match known(site, value)? {
        CoreValue::Number(_) | CoreValue::Empty => 1.0,
        CoreValue::Text(_) => 2.0,
        CoreValue::Logical(_) => 4.0,
        CoreValue::Error(_) => 16.0,
        CoreValue::Missing | CoreValue::Array(_) | CoreValue::Reference(_) => {
            return Err(site.not_modelled(Some(0)));
        }
    })
}

/// N: a number as it is, TRUE as 1 and FALSE as 0, an error as itself, and anything else, text
/// and a blank cell among them, as 0 (Microsoft's page; a date is a number here). A reference is
/// read at the first cell of its first area, of any size: Excel loads a range given to N without
/// `@`, and reads its first cell in a legacy formula placed within the range's rows and in a
/// dynamic-array formula alike. A number Excel's handling of which is not modelled (a subnormal)
/// is refused, as the number rule refuses one, and so are #BLOCKED! and #CONNECT! (see
/// [`known`]).
fn n(arguments: &[CalcValue], services: &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(N, services);
    let argument = &arguments[0];
    let value = match argument.core() {
        CoreValue::Reference(reference) => {
            let first = GridArea::cell(reference.areas()[0].first());
            match referenced(&site, first, services) {
                Ok(value) => value,
                Err(exit) => return Site::finish(Err(exit)),
            }
        }
        _ => argument.clone(),
    };
    let step = known(&site, &value).and_then(|core| match core {
        CoreValue::Number(x) => site
            .argument(0, coerce::operand(*x))
            .and_then(|x| site.number(x)),
        CoreValue::Logical(b) => Ok(CalcValue::number(if *b { 1.0 } else { 0.0 })),
        CoreValue::Error(code) => Err(Exit::Error(*code)),
        CoreValue::Text(_) | CoreValue::Empty => Ok(CalcValue::number(0.0)),
        CoreValue::Missing | CoreValue::Array(_) | CoreValue::Reference(_) => {
            Err(site.not_modelled(Some(0)))
        }
    });
    Site::finish(step)
}

/// NA: the error #N/A. It takes no argument; the arity refuses any.
fn na(_: &[CalcValue], _: &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault> {
    Ok(CalcValue::error(WorksheetErrorCode::NA))
}

/// ERROR.TYPE: the number of an error, and #N/A for any other value. Microsoft's page lists #NULL!
/// to #GETTING_DATA as 1 to 8 and anything else as #N/A; Excel gives #SPILL! 9, #FIELD! 13, #CALC!
/// 14 and #BUSY! 0 instead. The number of #BLOCKED! and #CONNECT! is unknown, so the call is
/// refused as not modelled (see the module documentation).
fn error_type(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ERROR_TYPE, services);
    let step = scalar(&site, &arguments[0]).and_then(|core| {
        use WorksheetErrorCode as W;
        let number = match core {
            CoreValue::Error(code) => match code {
                W::Null => 1.0,
                W::Div0 => 2.0,
                W::Value => 3.0,
                W::Ref => 4.0,
                W::Name => 5.0,
                W::Num => 6.0,
                W::NA => 7.0,
                W::GettingData => 8.0,
                W::Spill => 9.0,
                W::Field => 13.0,
                W::Calc => 14.0,
                W::Busy => 0.0,
                // Unknown: refused until dc-tt73 measures these errors in Excel.
                W::Blocked | W::Connect => return Err(site.not_modelled(Some(0))),
            },
            _ => return Err(Exit::Error(W::NA)),
        };
        Ok(CalcValue::number(number))
    });
    Site::finish(step)
}

/// The one area of `reference` when it is a single cell.
fn one_cell(reference: &ReferenceLike) -> Option<GridArea> {
    match reference.areas() {
        [area] if area.cell_count() == 1 => Some(*area),
        _ => None,
    }
}

/// The value of the cell `area` names, as the services' cell reader supplies it: the empty value
/// for a cell it does not visit. The reference is argument 0.
fn referenced(site: &Site<'_>, area: GridArea, services: &mut EvalServices<'_>) -> Step<CalcValue> {
    let cells = reader(site, services, 0)?;
    let mut value = CalcValue::empty();
    read_reference(
        site,
        cells,
        &ReferenceLike::area(area),
        0,
        &mut |_, _, held| {
            value = held.clone();
            Ok(())
        },
    )?;
    Ok(value)
}

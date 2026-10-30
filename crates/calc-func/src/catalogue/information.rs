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
//! ISEVEN, ISODD and N apply to each element of an array argument, giving the array of the
//! results (dc-b1l, measured in Excel 16.0.20430 on 2026-10-25 as dynamic-array formulas:
//! `=COLUMNS(ISEVEN({2,3}))` is 2 and `=INDEX(N({5,6}),1,2)` is 6); a legacy formula then takes
//! the array's first element at its root (`=ISEVEN({2,3})` TRUE, `=N({5,6})` 5, the corpus's
//! dc-nzpw rows). TYPE gives 64 and ISREF FALSE for an array. Any other array argument is refused
//! as not modelled: Excel loads it in a legacy formula with `@` (its first element), and in a
//! dynamic-array formula array evaluation applies the function to it; both are array evaluation's
//! (dc-m3u). So is a callable argument, whose worksheet-visible core is #CALC! but
//! which no cell holds (LAMBDA, dc-m3u), and an omitted one: Excel refuses `=ISBLANK()`,
//! `=ISEVEN()`, `=TYPE()` and such calls with nothing between the parentheses at entry, so no Excel
//! result exists for one. A number that is not finite is no worksheet value and is refused.
//!
//! #BLOCKED!, #CONNECT! and #BUSY! are read like every other worksheet error: ISERROR and ISERR are
//! TRUE, ISNA and ISBLANK FALSE, TYPE 16, and N gives the error. ERROR.TYPE gives 10 for #CONNECT!
//! and 11 for #BLOCKED!, the numbers in the sequence Microsoft's ERROR.TYPE documentation continues
//! from #SPILL! (9) to #FIELD! (13) and #CALC! (14), which the capture measures. No formula known
//! to give #BLOCKED! or #CONNECT! runs offline, and Excel refuses to enter either as a literal, so
//! no capture measures these rows.
//!
//! ISFORMULA asks the host whether the reference's first cell holds a formula
//! ([`crate::HostFacts::is_formula`]); a call whose services hold no host, or whose host cannot
//! say, is refused as not modelled.

use super::aggregate::{read_reference, reader};
use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{EagerKernel, EvalServices, FunctionFault};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcArray, CalcValue, CallableArityShape, CoreRef, GridArea, ReferenceLike,
    WorksheetErrorCode,
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
fn scalar<'v>(site: &Site<'_>, value: &'v CalcValue) -> Step<CoreRef<'v>> {
    match value.core_ref() {
        _ if value.callable_value().is_some() => Err(site.not_modelled(Some(0))),
        CoreRef::Missing | CoreRef::Array(_) => Err(site.not_modelled(Some(0))),
        CoreRef::Number(n) if !n.is_finite() => Err(site.not_modelled(Some(0))),
        core => Ok(core),
    }
}

/// The argument's worksheet value as [`scalar`] reads it; #BLOCKED! and #CONNECT! are errors like
/// any other (see the module documentation).
fn known<'v>(site: &Site<'_>, value: &'v CalcValue) -> Step<CoreRef<'v>> {
    scalar(site, value)
}

/// How a kernel reads its argument's value: [`scalar`] or [`known`].
type Read = for<'v> fn(&Site<'_>, &'v CalcValue) -> Step<CoreRef<'v>>;

/// An IS predicate: `holds` applied to the argument's value as `read` gives it, which is not
/// converted (a number held as text is text, as Microsoft's IS functions page states). The caller
/// has dereferenced a reference to one cell, so a blank cell arrives as the empty value.
fn predicate(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &EvalServices<'_>,
    read: Read,
    holds: fn(CoreRef<'_>) -> bool,
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
        matches!(core, CoreRef::Empty)
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
        |core| matches!(core, CoreRef::Error(code) if code != WorksheetErrorCode::NA),
    )
}

/// ISERROR: TRUE for every worksheet error, the newer ones as ISERR reads them.
fn iserror(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISERROR, arguments, services, known, |core| {
        matches!(core, CoreRef::Error(_))
    })
}

/// ISNA: TRUE for #N/A alone.
fn isna(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISNA, arguments, services, known, |core| {
        matches!(core, CoreRef::Error(WorksheetErrorCode::NA))
    })
}

/// ISNUMBER: TRUE for a number; number text and logicals are not numbers here.
fn isnumber(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISNUMBER, arguments, services, scalar, |core| {
        matches!(core, CoreRef::Number(_))
    })
}

/// ISTEXT: TRUE for text, empty text included.
fn istext(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISTEXT, arguments, services, scalar, |core| {
        matches!(core, CoreRef::Text(_))
    })
}

/// ISNONTEXT: TRUE for anything but text: a blank cell and an error included.
fn isnontext(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISNONTEXT, arguments, services, scalar, |core| {
        !matches!(core, CoreRef::Text(_))
    })
}

/// ISLOGICAL: TRUE for TRUE and FALSE; their text is text.
fn islogical(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    predicate(ISLOGICAL, arguments, services, scalar, |core| {
        matches!(core, CoreRef::Logical(_))
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
    let step = match argument.core_ref() {
        CoreRef::Reference(_) => Ok(CalcValue::logical(true)),
        CoreRef::Array(_) => Ok(CalcValue::logical(false)),
        _ => scalar(&site, argument).map(|_| CalcValue::logical(false)),
    };
    Site::finish(step)
}

/// ISFORMULA: whether the reference's first cell holds a formula, as the host says; see the
/// module documentation. The call interface refuses any other argument at admission, as Excel refuses
/// such a formula at entry.
fn isformula(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ISFORMULA, services);
    let cell = match arguments[0].core_ref() {
        CoreRef::Reference(reference) => reference.areas()[0].first(),
        _ => return Site::finish(Err(site.not_modelled(Some(0)))),
    };
    let known = services.host().and_then(|host| host.is_formula(cell));
    Site::finish(
        known
            .map(CalcValue::logical)
            .ok_or_else(|| site.not_modelled(Some(0))),
    )
}

/// ISEVEN: whether the number, truncated toward zero, is even.
fn iseven(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ISEVEN, services);
    Site::finish(each(&site, &arguments[0], services, |site, value, services| {
        odd(site, value, services).map(|odd| CalcValue::logical(!odd))
    }))
}

/// ISODD: whether the number, truncated toward zero, is odd.
fn isodd(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ISODD, services);
    Site::finish(each(&site, &arguments[0], services, |site, value, services| {
        odd(site, value, services).map(CalcValue::logical)
    }))
}

/// `apply` to the argument, or to each element of an array argument, giving the array of the
/// results with each element's error in its place (see the module documentation). An element
/// whose result is refused refuses the call.
fn each(
    site: &Site<'_>,
    argument: &CalcValue,
    services: &mut EvalServices<'_>,
    apply: fn(&Site<'_>, &CalcValue, &mut EvalServices<'_>) -> Step<CalcValue>,
) -> Step<CalcValue> {
    let CoreRef::Array(array) = argument.core_ref() else {
        return apply(site, argument, services);
    };
    if argument.callable_value().is_some() {
        return Err(site.not_modelled(Some(0)));
    }
    let mut cells = Vec::with_capacity(array.cell_count());
    for element in array.iter_row_major() {
        cells.push(match apply(site, element, services) {
            Ok(value) => value,
            Err(Exit::Error(code)) => CalcValue::error(code),
            Err(fault) => return Err(fault),
        });
    }
    CalcArray::new(array.shape(), cells)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(Some(0)))
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
    let value = match argument.core_ref() {
        CoreRef::Reference(reference) => match one_cell(reference) {
            Some(area) => referenced(site, area, services)?,
            None => return Err(Exit::Error(WorksheetErrorCode::Value)),
        },
        _ => argument.clone(),
    };
    let number = match scalar(site, &value)? {
        CoreRef::Logical(_) => return Err(Exit::Error(WorksheetErrorCode::Value)),
        _ => site.number_argument(0, &value)?,
    };
    Ok(site.add(number.abs(), PARITY_TOLERANCE)?.trunc() % 2.0 != 0.0)
}

/// TYPE: 1 for a number or a blank cell, 2 for text (empty text included), 4 for a logical, 16
/// for an error and 64 for an array; Microsoft's page lists all but the blank cell. A reference to
/// one cell gives the type of the cell's value, and a reference of several areas is 16. A reference
/// to one area of several cells is 64, as an array: Excel 16.0.20527 gives 64 for the
/// dynamic-array formulas `=TYPE(A101:A103)` and `=TYPE(A101:B101)` and for the legacy
/// `=SUMPRODUCT(TYPE(A101:A103))` (dc-b1l, through COM, 2026-10-30); elsewhere a legacy formula
/// intersects it first (`=TYPE(@A101:A103)`, calc-fml). Compound data (128) is not modelled: a
/// callable is refused. #BLOCKED!
/// and #CONNECT! are refused (see [`known`]).
fn type_of(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(TYPE, services);
    let argument = &arguments[0];
    let step = match argument.core_ref() {
        CoreRef::Array(_) => Ok(64.0),
        CoreRef::Reference(reference) if reference.area_count() > 1 => Ok(16.0),
        CoreRef::Reference(reference) => match one_cell(reference) {
            Some(area) => {
                referenced(&site, area, services).and_then(|value| type_code(&site, &value))
            }
            None => Ok(64.0),
        },
        _ => type_code(&site, argument),
    };
    Site::finish(step.map(CalcValue::number))
}

fn type_code(site: &Site<'_>, value: &CalcValue) -> Step<f64> {
    Ok(match known(site, value)? {
        CoreRef::Number(_) | CoreRef::Empty => 1.0,
        CoreRef::Text(_) => 2.0,
        CoreRef::Logical(_) => 4.0,
        CoreRef::Error(_) => 16.0,
        CoreRef::Missing | CoreRef::Array(_) | CoreRef::Reference(_) => {
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
    Site::finish(each(&site, &arguments[0], services, n_of))
}

/// N of one argument or array element.
fn n_of(site: &Site<'_>, argument: &CalcValue, services: &mut EvalServices<'_>) -> Step<CalcValue> {
    let value = match argument.core_ref() {
        CoreRef::Reference(reference) => {
            let first = GridArea::cell(reference.areas()[0].first());
            referenced(site, first, services)?
        }
        _ => argument.clone(),
    };
    known(site, &value).and_then(|core| match core {
        CoreRef::Number(x) => site
            .argument(0, coerce::operand(x))
            .and_then(|x| site.number(x)),
        CoreRef::Logical(b) => Ok(CalcValue::number(if b { 1.0 } else { 0.0 })),
        CoreRef::Error(code) => Err(Exit::Error(code)),
        CoreRef::Text(_) | CoreRef::Empty => Ok(CalcValue::number(0.0)),
        CoreRef::Missing | CoreRef::Array(_) | CoreRef::Reference(_) => {
            Err(site.not_modelled(Some(0)))
        }
    })
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
            CoreRef::Error(code) => match code {
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
                W::Connect => 10.0,
                W::Blocked => 11.0,
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

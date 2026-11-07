//! What the family fragments ported from OxFunc share: the declaration macro
//! [`ported_family!`], the argument readers, which coerce through [`crate::coerce`] like every
//! other kernel, and the publication of a ported kernel's result.
//!
//! A ported family declares each entry with its member number, kernel name, spelling, arity,
//! parameters (`V` for a value, `R` for a value or reference, `F` for a reference, each followed
//! by `@Reference` where Excel's formula entry takes only an operand that may give a reference),
//! optionally its repeating groups (`groups (from, size, tail);`) and result form
//! (`result MayBeReference;`), and body. The macro generates the entry and its eager kernel. Every generated kernel refuses an array given
//! to a value parameter, since a function of single values applied to an array lifts over its
//! elements, which is not modelled here (L1 owns array lifting), and then reaches one binary64
//! arithmetic site, so a call under the Exact numeric mode is refused, as for every other kernel.

use super::aggregate::{self, Held, Origin};
use super::{
    ArgumentDemand, CatalogueEntry, FunctionId, Kernel, Notation, Parameter, ParameterClass,
};
use crate::call::{EagerKernel, EvalServices};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcArray, CalcValue, CallableArityShape, CoreRef, ExcelText, WorksheetErrorCode,
};

/// A value parameter: the caller dereferences a reference first.
pub(super) const V: ParameterClass = ParameterClass::Value;
/// A value-or-reference parameter: a list the kernel reads itself.
pub(super) const R: ParameterClass = ParameterClass::ValueOrReference;
/// A reference-only parameter.
pub(super) const F: ParameterClass = ParameterClass::Reference;

/// An eager named call of `min..=max` arguments.
pub(super) const fn entry(
    id: FunctionId,
    spelling: &'static str,
    min: usize,
    max: usize,
    parameters: &'static [Parameter],
    kernel: EagerKernel,
) -> CatalogueEntry {
    CatalogueEntry {
        id,
        spelling,
        notation: Notation::Call,
        arity: CallableArityShape::range(min, max),
        groups: None,
        parameters,
        result: super::ResultForm::Value,
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(kernel),
    }
}

/// Declares a ported family: its `FAMILY` fragment and one eager kernel per entry. See the module
/// documentation.
macro_rules! ported_family {
    (
        code: $code:expr;
        $(
            $member:literal $fname:ident $spelling:literal ($min:literal, $max:literal)
            [$($class:ident $pname:literal $(@ $entry:ident)?),*]
            $(groups ($gfrom:literal, $gsize:literal, $gtail:literal);)?
            $(result $result:ident;)?
            $(resume $resumer:ident => $resume:path;)?
            |$s:ident, $a:ident, $sv:ident| $body:block
        )*
    ) => {
        pub(super) const FAMILY: super::Family = super::Family {
            code: $code,
            entries: &[
                $(
                    super::kit::entry(
                        super::FunctionId::new($code, $member),
                        $spelling,
                        $min,
                        $max,
                        &[$(super::Parameter::of($pname, super::kit::$class)$(.entry(super::EntryClass::$entry))?),*],
                        $fname,
                    ).with_continuation(ported_family!(@resume $($resumer)?))
                    $(.with_groups(super::ArgumentGroups::new($gfrom, $gsize, $gtail)))?
                    $(.with_result(super::ResultForm::$result))?,
                )*
            ],
        };
        $(
            $(
                fn $resumer(
                    arguments: &[crate::value::CalcValue],
                    services: &mut crate::call::EvalServices<'_>,
                ) -> Result<Box<dyn crate::call::KernelTask>, crate::call::FunctionFault> {
                    let site = crate::kernel::Site::new(super::FunctionId::new($code, $member), services);
                    let outcome = (|| {
                        let target = crate::catalogue::catalogue().resolve(super::FunctionId::new($code, $member)).expect("a declared entry");
                        super::kit::scalars(&site, arguments, target.entry.parameters)?;
                        site.binary64()?;
                        $resume(&site, arguments, services)
                    })();
                    match outcome {
                        Ok(task) => Ok(task),
                        Err(crate::kernel::Exit::Error(code)) => Ok(Box::new(crate::call::ReadyTask(Some(crate::value::CalcValue::error(code))))),
                        Err(crate::kernel::Exit::Fault(fault)) => Err(fault),
                    }
                }
            )?
            fn $fname(
                arguments: &[crate::value::CalcValue],
                services: &mut crate::call::EvalServices<'_>,
            ) -> Result<crate::value::CalcValue, crate::call::FunctionFault> {
                #[allow(unused_variables)]
                fn body(
                    $s: &crate::kernel::Site<'_>,
                    $a: &[crate::value::CalcValue],
                    $sv: &mut crate::call::EvalServices<'_>,
                ) -> crate::kernel::Step<crate::value::CalcValue> {
                    const PARAMETERS: &[super::Parameter] =
                        &[$(super::Parameter::of($pname, super::kit::$class)$(.entry(super::EntryClass::$entry))?),*];
                    super::kit::scalars($s, $a, PARAMETERS)?;
                    $s.binary64()?;
                    $body
                }
                let site = crate::kernel::Site::new(super::FunctionId::new($code, $member), services);
                crate::kernel::Site::finish(body(&site, arguments, services))
            }
        )*
    };
    (@resume) => { None };
    (@resume $resumer:ident) => { Some($resumer) };
}

/// Refuses an array given to a value parameter: the call would lift over its elements.
pub(super) fn scalars(
    site: &Site<'_>,
    arguments: &[CalcValue],
    parameters: &[Parameter],
) -> Step<()> {
    let last = parameters.len().saturating_sub(1);
    for (index, argument) in arguments.iter().enumerate() {
        if matches!(argument.core_ref(), CoreRef::Array(_))
            && parameters.get(index.min(last)).map(|p| p.class) == Some(ParameterClass::Value)
        {
            return Err(site.not_modelled(Some(index)));
        }
    }
    Ok(())
}

/// Argument `index` as a number, through worksheet number coercion.
pub(super) fn num(site: &Site<'_>, arguments: &[CalcValue], index: usize) -> Step<f64> {
    site.number_argument(index, &arguments[index])
}

/// A kind check for positions whose recorded numeric admission excludes logicals.
/// The caller keeps its existing number, blank, missing and error readers.
pub(super) fn reject_logical(value: &CalcValue) -> Step<()> {
    if matches!(value.core_ref(), CoreRef::Logical(_)) {
        fail(WorksheetErrorCode::Value)
    } else {
        Ok(())
    }
}

/// Number coercion at a position whose measured admission excludes logicals.
pub(super) fn num_without_logical(site: &Site<'_>, arguments: &[CalcValue], index: usize) -> Step<f64> {
    reject_logical(&arguments[index])?;
    num(site, arguments, index)
}

/// Validate a date argument before reading the next argument. Negative fractions are
/// rejected before truncation; the existing calendar validator supplies the upper bound.
pub(super) fn checked_date_serial(serial: f64) -> Step<f64> {
    if serial < 0.0 {
        return fail(WorksheetErrorCode::Num);
    }
    crate::ported::date_kernels::serial_from_number(serial).map_err(Exit::Error)?;
    Ok(serial)
}

/// Number coercion for a kernel that implements its own measured subnormal input policy.
/// Keep the ordinary coercions and refusals for every other kind of value.
pub(super) fn num_with_subnormal(
    site: &Site<'_>,
    arguments: &[CalcValue],
    index: usize,
) -> Step<f64> {
    match arguments[index].core_ref() {
        CoreRef::Number(n) if n.is_subnormal() => Ok(n),
        _ => num(site, arguments, index),
    }
}

/// Argument `index` as a number, or `default` when the call does not reach it. An argument the
/// call leaves empty (`f(1,)`) is read as 0, as coercion reads it.
pub(super) fn opt(
    site: &Site<'_>,
    arguments: &[CalcValue],
    index: usize,
    default: f64,
) -> Step<f64> {
    if index < arguments.len() {
        num(site, arguments, index)
    } else {
        Ok(default)
    }
}

/// Whether argument `index` was supplied and not left empty.
pub(super) fn given(arguments: &[CalcValue], index: usize) -> bool {
    arguments
        .get(index)
        .is_some_and(|value| !value.is_missing())
}

/// Argument `index` as a logical, through worksheet logical coercion.
pub(super) fn flag(site: &Site<'_>, arguments: &[CalcValue], index: usize) -> Step<bool> {
    site.argument(index, coerce::logical(&arguments[index]))
}

/// Argument `index` as a logical, or `default` when the call does not reach it.
pub(super) fn opt_flag(
    site: &Site<'_>,
    arguments: &[CalcValue],
    index: usize,
    default: bool,
) -> Step<bool> {
    if index < arguments.len() {
        flag(site, arguments, index)
    } else {
        Ok(default)
    }
}

/// Argument `index` as text, through worksheet text coercion. Text that is not valid UTF-16 is
/// refused, since the ported kernels read Rust strings.
pub(super) fn text(site: &Site<'_>, arguments: &[CalcValue], index: usize) -> Step<String> {
    let units = site.argument(index, coerce::text(&arguments[index]))?;
    String::from_utf16(&units).map_err(|_| site.not_modelled(Some(index)))
}

/// Argument `index` as Excel text (UTF-16 code units, any), through worksheet text coercion: for
/// the ported text kernels, which read [`ExcelText`] rather than Rust strings.
pub(super) fn text_arg(site: &Site<'_>, arguments: &[CalcValue], index: usize) -> Step<ExcelText> {
    let units = site.argument(index, coerce::text(&arguments[index]))?;
    Ok(ExcelText::from_utf16_code_units(units.into_owned()))
}

/// A ported kernel's number result: its worksheet error, or the number published by the number
/// rule (non-finite is #NUM!, a subnormal is refused).
pub(super) fn out(site: &Site<'_>, result: Result<f64, WorksheetErrorCode>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(code) => Err(Exit::Error(code)),
    }
}

/// Publish a kernel's measured subnormal result as a number; other outcomes use `out`.
/// The caller still runs the family's numeric-mode check before calculating.
pub(super) fn out_with_subnormal(
    site: &Site<'_>,
    result: Result<f64, WorksheetErrorCode>,
) -> Step<CalcValue> {
    match result {
        Ok(n) if n.is_subnormal() => Ok(CalcValue::number(n)),
        other => out(site, other),
    }
}

/// A ported kernel's text result.
pub(super) fn out_text(result: Result<ExcelText, WorksheetErrorCode>) -> Step<CalcValue> {
    result.map(CalcValue::text).map_err(Exit::Error)
}

/// Text as a worksheet value.
pub(super) fn text_value(text: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(text))
}

/// A worksheet error as a step.
pub(super) fn fail<T>(code: WorksheetErrorCode) -> Step<T> {
    Err(Exit::Error(code))
}

/// A formed scalar error, preserving the distinction from a callable or a held array/reference
/// error. The caller decides where this error precedes its own argument validation.
pub(super) fn scalar_error(value: &CalcValue) -> Step<()> {
    match value.core_ref() {
        CoreRef::Error(code) if value.callable_value().is_none() => fail(code),
        _ => Ok(()),
    }
}

/// A matrix result as an array value; each element is published by the number rule.
pub(super) fn out_matrix(site: &Site<'_>, rows: &[Vec<f64>]) -> Step<CalcValue> {
    let mut cells = Vec::with_capacity(rows.len());
    for row in rows {
        let mut out_row = Vec::with_capacity(row.len());
        for &n in row {
            out_row.push(match site.number(n) {
                Ok(value) => value,
                Err(Exit::Error(code)) => CalcValue::error(code),
                Err(fault) => return Err(fault),
            });
        }
        cells.push(out_row);
    }
    CalcArray::from_rows(cells)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(None))
}

/// The numbers the arguments `from..` give an aggregate that reads held values by `held`, in the
/// reading order: a direct argument through number coercion, a held value as the aggregate rule
/// says (see the aggregate family). The first error is the result.
pub(super) fn numbers(
    site: &Site<'_>,
    arguments: &[CalcValue],
    from: usize,
    to: usize,
    services: &mut EvalServices<'_>,
    held: Held,
) -> Step<Vec<f64>> {
    let mut found = Vec::new();
    aggregate::each_number(site, &arguments[from..to], services, held, &mut |n| {
        found.push(n);
        Ok(())
    })?;
    Ok(found)
}

/// Every value of the arguments `from..to` in the reading order, with whether it is held (a cell
/// of a reference or an element of an array) or direct.
pub(super) fn values(
    site: &Site<'_>,
    arguments: &[CalcValue],
    from: usize,
    to: usize,
    services: &mut EvalServices<'_>,
    visit: &mut dyn FnMut(bool, &CalcValue) -> Step<()>,
) -> Step<()> {
    aggregate::each_value(
        site,
        &arguments[from..to],
        services,
        &mut |_, origin, value| visit(origin != Origin::Direct, value),
    )
}

/// The most cells [`grid`] reads into memory; a larger area is refused as not modelled.
const GRID_LIMIT: u64 = 1 << 22;

/// One argument as a dense rectangle in row-major order: a reference of one area (a blank cell
/// is Empty), an array, or a single value as 1 by 1.
pub(super) struct Grid {
    pub(super) rows: usize,
    pub(super) cols: usize,
    pub(super) cells: Vec<CalcValue>,
    /// Whether the cells come from a reference or an array, rather than a single direct value.
    pub(super) held: bool,
}

pub(super) fn grid(
    site: &Site<'_>,
    arguments: &[CalcValue],
    index: usize,
    services: &mut EvalServices<'_>,
) -> Step<Grid> {
    let argument = &arguments[index];
    match argument.core_ref() {
        CoreRef::Reference(reference) => {
            let [area] = reference.areas() else {
                return Err(site.not_modelled(Some(index)));
            };
            if area.cell_count() > GRID_LIMIT {
                return Err(site.not_modelled(Some(index)));
            }
            let (rows, cols) = (area.row_count() as usize, area.column_count() as usize);
            let first = area.first();
            let mut cells = vec![CalcValue::empty(); rows * cols];
            let cells_reader = aggregate::reader(site, services, index)?;
            aggregate::read_reference(
                site,
                cells_reader,
                reference,
                index,
                &mut |_, key, value| {
                    let at = (key.row() - first.row()) as usize * cols
                        + (key.column() - first.column()) as usize;
                    cells[at] = value.clone();
                    Ok(())
                },
            )?;
            Ok(Grid {
                rows,
                cols,
                cells,
                held: true,
            })
        }
        CoreRef::Array(array) => {
            let shape = array.shape();
            Ok(Grid {
                rows: shape.rows,
                cols: shape.cols,
                cells: array.iter_row_major().cloned().collect(),
                held: true,
            })
        }
        _ => Ok(Grid {
            rows: 1,
            cols: 1,
            cells: vec![argument.clone()],
            held: false,
        }),
    }
}

/// The number a cell of a [`Grid`] holds for a numeric-array rule: a number as it is, an error
/// as itself, anything else `None`. A direct (non-held) value goes through number coercion.
pub(super) fn grid_number(
    site: &Site<'_>,
    index: usize,
    held: bool,
    value: &CalcValue,
) -> Step<Option<f64>> {
    if !held {
        return site.number_argument(index, value).map(Some);
    }
    match value.core_ref() {
        CoreRef::Number(n) => site.argument(index, coerce::operand(n)).map(Some),
        CoreRef::Error(code) => Err(Exit::Error(code)),
        CoreRef::Array(_) | CoreRef::Reference(_) => Err(site.not_modelled(Some(index))),
        _ => Ok(None),
    }
}

/// How a paired collector admits a direct scalar rather than an array/reference cell.
#[derive(Clone, Copy)]
pub(super) enum PairScalars {
    Coerce,
    SkipNonnumeric,
}

/// The pairs of numbers two arguments hold at the same positions, for the paired statistics:
/// arguments of different cell counts are #N/A; a position where either holds no number is
/// skipped (direct scalars follow `scalars`); the first error in reading order
/// (the first argument's before the second's at a position) is the result.
pub(super) fn pairs(
    site: &Site<'_>,
    arguments: &[CalcValue],
    x: usize,
    y: usize,
    scalars: PairScalars,
    services: &mut EvalServices<'_>,
) -> Step<Vec<(f64, f64)>> {
    let left = grid(site, arguments, x, services)?;
    let right = grid(site, arguments, y, services)?;
    if left.cells.len() != right.cells.len() {
        return fail(WorksheetErrorCode::NA);
    }
    let mut found = Vec::new();
    for (a, b) in left.cells.iter().zip(&right.cells) {
        let skip_nonnumeric = matches!(scalars, PairScalars::SkipNonnumeric);
        let a = grid_number(site, x, left.held || skip_nonnumeric, a)?;
        let b = grid_number(site, y, right.held || skip_nonnumeric, b)?;
        if let (Some(a), Some(b)) = (a, b) {
            found.push((a, b));
        }
    }
    Ok(found)
}

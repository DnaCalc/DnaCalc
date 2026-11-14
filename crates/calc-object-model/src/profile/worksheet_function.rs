//! One WorksheetFunction handler. Profiles supply conversion of Range arguments and the
//! catalogue call's services; the descriptor supplies every member and its argument list.
use crate::{AutoArray, AutoError, AutoValue, Call, Descriptor, Handler, Handlers};
use calc_func::{EvalServices, FunctionCallScratch, FunctionFault, Notation, PreparedArguments};
use calc_value::{ArrayShape, CalcArray, CalcValue, CoreRef, WorksheetErrorCode};

const OWNER: &str = "dc-1nbp.5";

pub fn register<C>(handlers: &mut Handlers<C>, handler: Handler<C>) {
    let d = Descriptor::get();
    for member in d.members(d.id("WorksheetFunction").unwrap()) {
        handlers.on("WorksheetFunction", &member.name, member.invoke, handler);
    }
}

/// COM spells dotted catalogue names with underscores (Norm_S_Dist, T_Dist_2T).
pub fn catalogue_name(name: &str) -> String {
    name.replace('_', ".").to_ascii_uppercase()
}

pub fn dispatch(
    call: &Call<'_>,
    mut argument: impl FnMut(&AutoValue) -> Result<CalcValue, AutoError>,
    mut invoke: impl FnMut(&str, &[CalcValue]) -> Result<CalcValue, AutoError>,
) -> Result<AutoValue, AutoError> {
    let name = catalogue_name(&call.member.name);
    // Missing catalogue entries name the requested member even when a client catches 1004.
    if calc_func::catalogue()
        .resolve_surface(Notation::Call, &name)
        .is_err()
    {
        return Err(AutoError::not_modelled(call.label(), OWNER));
    }
    let declared: Vec<_> = call.member.arguments().collect();
    if call.args.len() > declared.len() {
        return Err(crate::geometry::bad_count());
    }
    let mut end = call.args.len();
    while end > 0
        && call.arg(end - 1).is_missing()
        && declared[end - 1].has(crate::ParamFlag::Optional)
    {
        end -= 1;
    }
    if declared.iter().enumerate().any(|(i, p)| {
        !p.has(crate::ParamFlag::Optional)
            && !p.has(crate::ParamFlag::Hasdefault)
            && (i >= end || call.arg(i).is_missing())
    }) {
        return Err(crate::geometry::bad_count());
    }
    let args = call.args[..end]
        .iter()
        .map(&mut argument)
        .collect::<Result<Vec<_>, _>>()?;
    let result = invoke(&name, &args)?;
    if result.as_error().is_some() {
        // E7 has not yet measured the exact description. This provisional text is explicitly
        // required by the owner; tests name that limitation.
        return Err(AutoError::excel(
            1004,
            format!(
                "Unable to get the {} property of the WorksheetFunction class",
                call.member.name
            ),
        ));
    }
    from_calc(&result)
}

struct Arguments<'a>(&'a [CalcValue]);
impl PreparedArguments for Arguments<'_> {
    fn count(&self) -> usize {
        self.0.len()
    }
    fn argument(&mut self, index: usize) -> CalcValue {
        self.0[index].clone()
    }
}

/// The ordinary CalcFunc call boundary, with services supplied by the profile's owner.
pub fn catalogue_call(
    name: &str,
    args: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, AutoError> {
    let target = calc_func::catalogue()
        .resolve_surface(Notation::Call, name)
        .map_err(|_| AutoError::not_modelled(format!("WorksheetFunction.{name}"), OWNER))?;
    calc_func::invoke(
        target,
        &mut Arguments(args),
        services,
        &mut FunctionCallScratch::new(),
    )
    .map_err(|fault| match fault {
        FunctionFault::ArityMismatch { .. } => crate::geometry::bad_count(),
        FunctionFault::NotHeld { owner, .. } => {
            AutoError::not_modelled(format!("WorksheetFunction.{name}"), owner)
        }
        other => AutoError::not_modelled(format!("WorksheetFunction.{name}: {other:?}"), OWNER),
    })
}

pub fn to_calc(value: &AutoValue) -> Result<CalcValue, AutoError> {
    Ok(match value {
        AutoValue::Empty => CalcValue::empty(),
        AutoValue::Missing => CalcValue::missing(),
        AutoValue::Bool(v) => CalcValue::logical(*v),
        AutoValue::I2(v) => CalcValue::number(f64::from(*v)),
        AutoValue::I4(v) => CalcValue::number(f64::from(*v)),
        AutoValue::R4(v) if v.is_finite() => CalcValue::number(f64::from(*v)),
        AutoValue::R8(v) if v.is_finite() => CalcValue::number(*v),
        AutoValue::Text(v) => CalcValue::text(v.clone()),
        AutoValue::Error(v) => CalcValue::error(match *v {
            2000 => WorksheetErrorCode::Null,
            2007 => WorksheetErrorCode::Div0,
            2015 => WorksheetErrorCode::Value,
            2023 => WorksheetErrorCode::Ref,
            2029 => WorksheetErrorCode::Name,
            2036 => WorksheetErrorCode::Num,
            2042 => WorksheetErrorCode::NA,
            2043 => WorksheetErrorCode::GettingData,
            _ => return Err(AutoError::not_modelled("Automation CVErr code", OWNER)),
        }),
        AutoValue::Array(a) => {
            let values = a
                .values
                .iter()
                .map(|v| {
                    if matches!(v, AutoValue::Array(_)) {
                        return Err(crate::geometry::type_mismatch());
                    }
                    to_calc(v)
                })
                .collect::<Result<Vec<_>, _>>()?;
            CalcValue::array(
                CalcArray::new(
                    ArrayShape {
                        rows: a.rows as usize,
                        cols: a.columns as usize,
                    },
                    values,
                )
                .ok_or_else(crate::geometry::type_mismatch)?,
            )
        }
        AutoValue::Object(_) => {
            return Err(AutoError::excel(1004, "needs a workbook: open in DNA Calc"));
        }
        AutoValue::Date(_) | AutoValue::Currency(_) => {
            return Err(AutoError::not_modelled(
                "Automation Date/Currency input conversion",
                "dc-1nbp.6",
            ));
        }
        _ => return Err(crate::geometry::type_mismatch()),
    })
}

pub fn from_calc(value: &CalcValue) -> Result<AutoValue, AutoError> {
    Ok(match value.core_ref() {
        CoreRef::Empty => AutoValue::Empty,
        CoreRef::Missing => AutoValue::Missing,
        CoreRef::Number(v) => AutoValue::R8(v),
        CoreRef::Logical(v) => AutoValue::Bool(v),
        CoreRef::Text(v) => AutoValue::Text(v.clone()),
        CoreRef::Error(v) => AutoValue::Error(match v {
            WorksheetErrorCode::Null => 2000,
            WorksheetErrorCode::Div0 => 2007,
            WorksheetErrorCode::Value => 2015,
            WorksheetErrorCode::Ref => 2023,
            WorksheetErrorCode::Name => 2029,
            WorksheetErrorCode::Num => 2036,
            WorksheetErrorCode::NA => 2042,
            WorksheetErrorCode::GettingData => 2043,
            _ => return Err(AutoError::not_modelled("Automation CVErr code", OWNER)),
        }),
        CoreRef::Array(a) => AutoValue::Array(Box::new(AutoArray {
            rows: a.shape().rows as u32,
            columns: a.shape().cols as u32,
            lower: (1, 1),
            values: a
                .iter_row_major()
                .map(from_calc)
                .collect::<Result<Vec<_>, _>>()?,
        })),
        _ => return Err(AutoError::not_modelled("Automation result kind", OWNER)),
    })
}

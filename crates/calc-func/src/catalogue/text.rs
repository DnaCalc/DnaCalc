//! The text family fragment: the concatenation operator and its kernel.

use super::{CatalogueEntry, Family, FunctionId, Notation, Parameter};
use crate::call::{EvalServices, FunctionFault};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcValue, CoreValue, EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, WorksheetErrorCode,
};

const CODE: u16 = 3;

const CONCATENATE: FunctionId = FunctionId::new(CODE, 1);

const TEXTS: &[Parameter] = &[Parameter::value("text1"), Parameter::value("text2")];

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[CatalogueEntry::operator(
        CONCATENATE,
        "&",
        Notation::Infix,
        TEXTS,
        concatenate,
    )],
};

fn concatenate(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(CONCATENATE, services);
    Site::finish(joined(&site, arguments))
}

/// The operands' text joined. An operand's error is the result, the left operand's first, before
/// any operand is read as text: no operand's text can be an error. A result longer than a cell
/// holds is #VALUE!.
fn joined(site: &Site<'_>, arguments: &[CalcValue]) -> Step<CalcValue> {
    site.scalar_operands(arguments)?;
    if let Some(code) = arguments.iter().find_map(|argument| match argument.core() {
        CoreValue::Error(code) => Some(*code),
        _ => None,
    }) {
        return Err(Exit::Error(code));
    }
    let left = site.argument(0, coerce::text(&arguments[0]))?;
    let right = site.argument(1, coerce::text(&arguments[1]))?;
    if left.len() + right.len() > EXCEL_TEXT_MAX_UTF16_CODE_UNITS {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    }
    let mut units = Vec::with_capacity(left.len() + right.len());
    units.extend_from_slice(left);
    units.extend_from_slice(right);
    Ok(CalcValue::text(ExcelText::from_utf16_code_units(units)))
}

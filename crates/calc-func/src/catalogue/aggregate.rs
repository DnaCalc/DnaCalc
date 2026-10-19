//! The aggregate family fragment: SUM.

use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Notation, Parameter};
use crate::call::{EvalServices, FunctionFault};
use crate::value::{CalcValue, CallableArityShape, CoreValue, NumericMode};

const CODE: u16 = 4;

const SUM: FunctionId = FunctionId::new(CODE, 1);

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[CatalogueEntry {
        id: SUM,
        spelling: "SUM",
        notation: Notation::Call,
        arity: CallableArityShape::range(1, 255),
        parameters: &[
            Parameter::value_or_reference("number1"),
            Parameter::value_or_reference("number2"),
        ],
        demand: ArgumentDemand::Eager,
        kernel: Some(sum),
    }],
};

/// SUM of number arguments, added left to right from zero under Fast. Every other argument kind
/// (text, logical, empty, missing, error, array, reference) is not modelled yet and is refused
/// with the argument's index, and a total that is not finite is refused rather than published,
/// because Excel's result for an overflowing sum is not modelled yet.
fn sum(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    if let NumericMode::Exact(_) = services.numeric_mode() {
        return Err(FunctionFault::ExactUnavailable { id: SUM });
    }
    let mut total = 0.0;
    for (index, argument) in arguments.iter().enumerate() {
        match argument.core() {
            CoreValue::Number(number) => total += number,
            _ => {
                return Err(FunctionFault::NotImplemented {
                    id: SUM,
                    argument: Some(index),
                });
            }
        }
    }
    if total.is_finite() {
        Ok(CalcValue::number(total))
    } else {
        Err(FunctionFault::NotImplemented {
            id: SUM,
            argument: None,
        })
    }
}

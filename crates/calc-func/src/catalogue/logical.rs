//! The logical family fragment: IF and its lazy kernel.

use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{EvalServices, FunctionFault, LazyArguments};
use crate::coerce;
use crate::kernel::Site;
use crate::value::{CalcValue, CallableArityShape};

const CODE: u16 = 5;

const IF: FunctionId = FunctionId::new(CODE, 1);

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[CatalogueEntry {
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
    }],
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

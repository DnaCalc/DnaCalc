//! The logical family fragment: IF. Metadata only; no kernel yet.

use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Notation, Parameter};
use crate::value::CallableArityShape;

const CODE: u16 = 5;

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[CatalogueEntry {
        id: FunctionId::new(CODE, 1),
        spelling: "IF",
        notation: Notation::Call,
        arity: CallableArityShape::range(2, 3),
        parameters: &[
            Parameter::value("logical_test"),
            Parameter::value_or_reference("value_if_true"),
            Parameter::value_or_reference("value_if_false"),
        ],
        demand: ArgumentDemand::BranchOnCondition,
        kernel: None,
    }],
};

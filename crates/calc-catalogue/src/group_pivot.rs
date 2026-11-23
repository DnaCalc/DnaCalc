//! Declaration data moved from the existing CalcFunc catalogue.
use super::{ArgumentDemand, FunctionId, Notation, Parameter, WorksheetDeclaration};
use calc_value::CallableArityShape;
/// Stable family code used by the entries in this fragment.
pub const CODE: u16 = 320;

/// Stable ID for `PERCENTOF`.
pub const PERCENTOF: FunctionId = FunctionId::new(CODE, 1);

/// Worksheet declarations in this family's member order.
pub const ENTRIES: &[WorksheetDeclaration] = &[WorksheetDeclaration {
    localized_names: None,
    context_facilities: None,
    id: PERCENTOF,
    spelling: "PERCENTOF",
    notation: Notation::Call,
    kind: super::EntryKind::Worksheet,
    availability: super::Availability::Unspecified,
    arity: CallableArityShape::exact(2),
    groups: None,
    parameters: &[
        Parameter::value_or_reference("data_subset"),
        Parameter::value_or_reference("data_all"),
    ],
    result: super::ResultForm::Value,
    demand: ArgumentDemand::Eager,
    traits: super::EntryTraits::NONE,
}
// dc-b1l (Touchstone 2026-11-16, three-d): `=PERCENTOF(Ja:Ma!A1,1)` is #VALUE!, either argument.
.with_three_d(super::ThreeD::new(None, Some(&[]), None))];

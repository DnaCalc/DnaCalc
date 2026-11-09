//! Declaration data moved from the existing CalcFunc catalogue.
use super::{FunctionId, Notation, Parameter, WorksheetDeclaration};
use calc_value::CallableArityShape;
pub const CODE: u16 = 11;

pub const INDIRECT: FunctionId = FunctionId::new(CODE, 1);

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub const ENTRIES: &[WorksheetDeclaration] = &[WorksheetDeclaration {
    localized_names: None,
    context_facilities: None,
    id: INDIRECT,
    spelling: "INDIRECT",
    notation: Notation::Call,
    kind: super::EntryKind::Worksheet,
    availability: super::Availability::Unspecified,
    arity: CallableArityShape::range(1, 2),
    groups: None,
    parameters: &[Parameter::value("ref_text"), Parameter::value("a1")],
    // A call may give a reference (dc-mx3x, Excel 16.0.20527, 2026-11-07):
    // `=COUNTBLANK(INDIRECT("A1"))` enters, and so does `=INDIRECT("A1")(3)` (#REF!).
    result: super::ResultForm::MayBeReference,
    demand: super::ArgumentDemand::Eager,
    traits: super::EntryTraits::NONE.volatile(),
}
.with_three_d(super::ThreeD::new(None, None, Some(&[])))];

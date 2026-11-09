//! Declaration data for the existing information worksheet functions.
use super::{
    ArgumentDemand, Availability, EntryKind, FunctionId, Notation, Parameter, ResultForm,
    WorksheetDeclaration,
};
use calc_value::CallableArityShape;
pub const CODE: u16 = 10;

pub const ISBLANK: FunctionId = FunctionId::new(CODE, 1);
pub const ISERR: FunctionId = FunctionId::new(CODE, 2);
pub const ISERROR: FunctionId = FunctionId::new(CODE, 3);
pub const ISNA: FunctionId = FunctionId::new(CODE, 4);
pub const ISNUMBER: FunctionId = FunctionId::new(CODE, 5);
pub const ISTEXT: FunctionId = FunctionId::new(CODE, 6);
pub const ISNONTEXT: FunctionId = FunctionId::new(CODE, 7);
pub const ISLOGICAL: FunctionId = FunctionId::new(CODE, 8);
pub const ISEVEN: FunctionId = FunctionId::new(CODE, 9);
pub const ISODD: FunctionId = FunctionId::new(CODE, 10);
pub const ISREF: FunctionId = FunctionId::new(CODE, 11);
pub const ISFORMULA: FunctionId = FunctionId::new(CODE, 12);
pub const TYPE: FunctionId = FunctionId::new(CODE, 13);
pub const N: FunctionId = FunctionId::new(CODE, 14);
pub const NA: FunctionId = FunctionId::new(CODE, 15);
pub const ERROR_TYPE: FunctionId = FunctionId::new(CODE, 16);

const VALUE: &[Parameter] = &[Parameter::value("value")];
/// dc-o9r: TYPE intersects a reference except below an array place.
const TYPE_VALUE: &[Parameter] =
    &[Parameter::value_or_reference("value")
        .legacy(super::LegacyPlace::AggregateIntersectReference)];
const N_VALUE: &[Parameter] = &[Parameter::value_or_reference("value")
    .legacy(super::LegacyPlace::AggregateRootArrayNestedLiftReferences)];
const NUMBER: &[Parameter] =
    &[Parameter::value_or_reference("number").legacy(super::LegacyPlace::AggregateAtRoot)];
const ISREF_VALUE: &[Parameter] =
    &[Parameter::value_or_reference("value").legacy(super::LegacyPlace::AggregateAtRoot)];
/// ISFORMULA's reference takes only an operand that may give one (dc-mx3x, Excel 16.0.20527,
/// 2026-11-07, Formula2 and Formula alike): `=ISFORMULA(2)`, `=ISFORMULA({1,2})` and
/// `=ISFORMULA(1+1)` are refused, `=ISFORMULA(B1:B3)` and `=ISFORMULA(INDEX(B1:B3,0))` enter.
/// ISREF's value takes any operand: `=ISREF(2)` enters.
const REFERENCE: &[Parameter] =
    &[Parameter::reference("reference").entry(super::EntryClass::Reference)];
const ERROR_VAL: &[Parameter] = &[Parameter::value("error_val")];

/// An eager named call of one argument.
const fn unary(
    id: FunctionId,
    spelling: &'static str,
    parameters: &'static [Parameter],
) -> WorksheetDeclaration {
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id,
        spelling,
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::exact(1),
        groups: None,
        parameters,
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub const ENTRIES: &[WorksheetDeclaration] = &[
    unary(ISBLANK, "ISBLANK", VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(ISERR, "ISERR", VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(ISERROR, "ISERROR", VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(ISNA, "ISNA", VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(ISNUMBER, "ISNUMBER", VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(ISTEXT, "ISTEXT", VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(ISNONTEXT, "ISNONTEXT", VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(ISLOGICAL, "ISLOGICAL", VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(ISEVEN, "ISEVEN", NUMBER).with_three_d(super::ThreeD::new(None, Some(&[]), None)),
    unary(ISODD, "ISODD", NUMBER).with_three_d(super::ThreeD::new(None, Some(&[]), None)),
    unary(ISREF, "ISREF", ISREF_VALUE)
        .with_three_d(super::ThreeD::new(None, None, Some(&[])))
        .with_traits(super::EntryTraits::NONE.take_reference_array()),
    unary(ISFORMULA, "ISFORMULA", REFERENCE).with_three_d(super::ThreeD::new(
        None,
        Some(&[]),
        None,
    )),
    unary(TYPE, "TYPE", TYPE_VALUE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    unary(N, "N", N_VALUE)
        .with_three_d(super::ThreeD::new(None, Some(&[]), None))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: NA,
        spelling: "NA",
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::exact(0),
        groups: None,
        parameters: &[],
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    },
    unary(ERROR_TYPE, "ERROR.TYPE", ERROR_VAL).with_three_d(super::ThreeD::new(
        None,
        None,
        Some(&[]),
    )),
];

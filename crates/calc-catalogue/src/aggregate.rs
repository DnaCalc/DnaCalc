//! Declaration data for the existing sum and related worksheet functions.
use super::{
    ArgumentDemand, Availability, EntryKind, FunctionId, Notation, Parameter, ResultForm,
    WorksheetDeclaration,
};
use calc_value::CallableArityShape;
/// Stable family code used by the entries in this fragment.
pub const CODE: u16 = 4;

/// Stable ID for `SUM`.
pub const SUM: FunctionId = FunctionId::new(CODE, 1);
/// Stable ID for `PRODUCT`.
pub const PRODUCT: FunctionId = FunctionId::new(CODE, 2);
/// Stable ID for `SUMSQ`.
pub const SUMSQ: FunctionId = FunctionId::new(CODE, 3);
/// Stable ID for `SUMPRODUCT`.
pub const SUMPRODUCT: FunctionId = FunctionId::new(CODE, 4);
/// Stable ID for `COUNT`.
pub const COUNT: FunctionId = FunctionId::new(CODE, 5);
/// Stable ID for `COUNTA`.
pub const COUNTA: FunctionId = FunctionId::new(CODE, 6);
/// Stable ID for `COUNTBLANK`.
pub const COUNTBLANK: FunctionId = FunctionId::new(CODE, 7);
/// Stable ID for `AVERAGE`.
pub const AVERAGE: FunctionId = FunctionId::new(CODE, 8);
/// Stable ID for `AVERAGEA`.
pub const AVERAGEA: FunctionId = FunctionId::new(CODE, 9);
/// Stable ID for `MIN`.
pub const MIN: FunctionId = FunctionId::new(CODE, 10);
/// Stable ID for `MAX`.
pub const MAX: FunctionId = FunctionId::new(CODE, 11);
/// Stable ID for `MINA`.
pub const MINA: FunctionId = FunctionId::new(CODE, 12);
/// Stable ID for `MAXA`.
pub const MAXA: FunctionId = FunctionId::new(CODE, 13);

// dc-10y and dc-b1l, Excel 16.0.20527: SUM(A1:A3*2) intersects the range at the root;
// SUMPRODUCT(SUM(A1:A3*2)) is 12, and SUMPRODUCT(SUM(ABS(A1:A3))) intersects inside ABS.
const NUMBERS: &[Parameter] = &[
    Parameter::value_or_reference("number1").legacy(super::LegacyPlace::AggregateUnderArray),
    Parameter::value_or_reference("number2")
        .legacy(super::LegacyPlace::AggregateUnderArrayRepeated),
];
const VALUES: &[Parameter] = &[
    Parameter::value_or_reference("value1").legacy(super::LegacyPlace::AggregateAtRoot),
    Parameter::value_or_reference("value2").legacy(super::LegacyPlace::AggregateAtRootRepeated),
];
const COUNT_VALUES: &[Parameter] = &[
    Parameter::value_or_reference("value1").legacy(super::LegacyPlace::AggregateUnderArray),
    Parameter::value_or_reference("value2").legacy(super::LegacyPlace::AggregateUnderArrayRepeated),
];
const ARRAYS: &[Parameter] = &[
    Parameter::value_or_reference("array1").legacy(super::LegacyPlace::ArrayAtRoot),
    Parameter::value_or_reference("array2").legacy(super::LegacyPlace::ArrayAtRootRepeated),
];
/// COUNTBLANK's range takes only an operand that may give a reference (dc-mx3x, Excel 16.0.20527,
/// 2026-11-07, Formula2 and Formula alike). Refused: `=COUNTBLANK(2)`, `=COUNTBLANK({"",1,#N/A})`,
/// `=COUNTBLANK("")`, `=COUNTBLANK(TRUE)`, `=COUNTBLANK(#N/A)`, `=COUNTBLANK((2))`,
/// `=COUNTBLANK(1+1)`, `=COUNTBLANK(-A1)`, `=COUNTBLANK(A1:A2+0)`, `=COUNTBLANK(A1&"")`,
/// `=COUNTBLANK(SUM(A1:A2))`, `=COUNTBLANK(TRUE())`, `=COUNTBLANK(IFERROR(A1,1))`. Entered:
/// `=COUNTBLANK(A1:A2)` (1), `=COUNTBLANK((A1:A2))`, `=COUNTBLANK(A1:A2 A1)`,
/// `=COUNTBLANK((A1,A2))` (#VALUE!), `=COUNTBLANK(INDEX(A1:A2,1))`, `=COUNTBLANK(OFFSET(A1,0,0))`,
/// `=COUNTBLANK(IF(TRUE,A1:A2))`, `=COUNTBLANK(IF(TRUE,1))` (#VALUE!), `=COUNTBLANK(LET(x,1,x))`
/// (#VALUE!), `=COUNTBLANK(LAMBDA(x,x))` (#VALUE!), `=COUNTBLANK(@A1:A2)`, `=COUNTBLANK(B1#)`
/// and `=COUNTBLANK(nm)` for a name `nm` defined as `=1` (#VALUE!) or not defined (#NAME?).
const RANGE: &[Parameter] =
    &[Parameter::value_or_reference("range").entry(super::EntryClass::Reference)];

/// An eager named call that takes 1 to 255 arguments, the last parameter repeating.
const fn aggregate(
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
        arity: CallableArityShape::range(1, 255),
        groups: None,
        parameters,
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
/// Worksheet declarations in this family's member order.
pub const ENTRIES: &[WorksheetDeclaration] = &[
    aggregate(SUM, "SUM", NUMBERS)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.take_reference_array()),
    aggregate(PRODUCT, "PRODUCT", NUMBERS)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.first_cell_references()),
    aggregate(SUMSQ, "SUMSQ", NUMBERS)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.first_cell_references()),
    aggregate(SUMPRODUCT, "SUMPRODUCT", ARRAYS)
        .with_three_d(super::ThreeD::new(None, None, Some(&[])))
        .with_traits(super::EntryTraits::NONE.no_number_references()),
    aggregate(COUNT, "COUNT", COUNT_VALUES)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.take_reference_array()),
    aggregate(COUNTA, "COUNTA", COUNT_VALUES)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.take_reference_array()),
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: COUNTBLANK,
        spelling: "COUNTBLANK",
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::exact(1),
        groups: None,
        parameters: RANGE,
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        // W111 and w089: a computed array lifts; a reference is consumed whole.
        // dc-b1l: an array of references runs it once per reference (Touchstone 2026-11-16).
        traits: super::EntryTraits::NONE
            .lift_computed_array()
            .lift_dynamic_references(),
    }
    .with_three_d(super::ThreeD::new(None, Some(&[]), None)),
    aggregate(AVERAGE, "AVERAGE", NUMBERS)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.take_reference_array()),
    aggregate(AVERAGEA, "AVERAGEA", VALUES)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.first_cell_references()),
    aggregate(MIN, "MIN", NUMBERS)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.first_cell_references()),
    aggregate(MAX, "MAX", NUMBERS)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.take_reference_array()),
    aggregate(MINA, "MINA", VALUES)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.first_cell_references()),
    aggregate(MAXA, "MAXA", VALUES)
        .with_three_d(super::ThreeD::new(Some(&[]), None, None))
        .with_traits(super::EntryTraits::NONE.first_cell_references()),
];

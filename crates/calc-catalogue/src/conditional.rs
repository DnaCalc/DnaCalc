//! Declaration data for the existing conditional worksheet functions.
use super::{ArgumentDemand, ArgumentGroups, Availability, EntryClass, EntryKind, FunctionId, Notation, Parameter, ResultForm, WorksheetDeclaration};
use calc_value::CallableArityShape;
pub const CODE: u16 = 7;

pub const COUNTIF: FunctionId = FunctionId::new(CODE, 1);
pub const COUNTIFS: FunctionId = FunctionId::new(CODE, 2);
pub const SUMIF: FunctionId = FunctionId::new(CODE, 3);
pub const SUMIFS: FunctionId = FunctionId::new(CODE, 4);
pub const AVERAGEIF: FunctionId = FunctionId::new(CODE, 5);
pub const AVERAGEIFS: FunctionId = FunctionId::new(CODE, 6);
pub const MAXIFS: FunctionId = FunctionId::new(CODE, 7);
pub const MINIFS: FunctionId = FunctionId::new(CODE, 8);


// Formula entry (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and Formula alike): every range
// below takes only an operand that may give a reference. `=COUNTIF(2,1)`, `=COUNTIF({1,2},1)`,
// `=COUNTIF(1+1,1)` and `=COUNTIF(,1)` are refused and `=COUNTIF(B1:B3,1)` and
// `=COUNTIF(INDEX(B1:B3,0),1)` enter; so for SUMIF's and AVERAGEIF's range and sum or average
// range, each criteria range of COUNTIFS (`=COUNTIFS(B1:B3,1,2,1)` is refused), and the first
// range and each criteria range of SUMIFS, AVERAGEIFS, MAXIFS and MINIFS
// (`=SUMIFS(B1:B3,B1:B3,1,2,1)` is refused). A criterion takes any operand.
const RANGE_CRITERIA: &[Parameter] = &[
    Parameter::reference("range").entry(EntryClass::Reference).legacy(super::LegacyPlace::ArrayAlwaysLiftReferences),
    Parameter::value_or_reference("criteria"),
];
const SUM_RANGE: &[Parameter] = &[
    Parameter::reference("range").entry(EntryClass::Reference).legacy(super::LegacyPlace::ArrayAlwaysLiftReferences),
    Parameter::value_or_reference("criteria"),
    Parameter::reference("sum_range").entry(EntryClass::Reference),
];
const AVERAGE_RANGE: &[Parameter] = &[
    Parameter::reference("range").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria"),
    Parameter::reference("average_range").entry(EntryClass::Reference),
];
// The catalogue repeats only the last named parameter, so arguments past these names take its
// class, ValueOrReference: a criteria range there reaches the kernel, which requires a reference.
const COUNT_PAIRS: &[Parameter] = &[
    Parameter::reference("criteria_range1").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria2"),
];
const SUM_PAIRS: &[Parameter] = &[
    Parameter::reference("sum_range").entry(EntryClass::Reference),
    Parameter::reference("criteria_range1").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria2"),
];
const AVERAGE_PAIRS: &[Parameter] = &[
    Parameter::reference("average_range").entry(EntryClass::Reference),
    Parameter::reference("criteria_range1").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria2"),
];
const MAX_PAIRS: &[Parameter] = &[
    Parameter::reference("max_range").entry(EntryClass::Reference),
    Parameter::reference("criteria_range1").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria2"),
];
const MIN_PAIRS: &[Parameter] = &[
    Parameter::reference("min_range").entry(EntryClass::Reference),
    Parameter::reference("criteria_range1").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2").entry(EntryClass::Reference),
    Parameter::value_or_reference("criteria2"),
];

/// An eager named call taking `min` to `max` arguments.
const fn conditional(
    id: FunctionId,
    spelling: &'static str,
    (min, max): (usize, usize),
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
        arity: CallableArityShape::range(min, max),
        groups: None,
        parameters,
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
}

// The criteria pairs are groups (dc-mx3x, both forms): `=COUNTIFS(B1:B3,1,B1:B3)` and
// `=SUMIFS(B1:B3,B1:B3,1,B1:B3)` are refused, `=COUNTIFS(B1:B3,1,B1:B3,1)` and
// `=SUMIFS(B1:B3,B1:B3,1,B1:B3,1)` enter: COUNTIFS takes 2, 4, 6 and 8 arguments of 1 to 8, and
// SUMIFS, AVERAGEIFS, MAXIFS and MINIFS 3, 5 and 7.
// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub const ENTRIES: &[WorksheetDeclaration] = &[
        conditional(COUNTIF, "COUNTIF", (2, 2), RANGE_CRITERIA).with_three_d(super::ThreeD::new(None, Some(&[0]), None)).with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
        conditional(COUNTIFS, "COUNTIFS", (2, 254), COUNT_PAIRS).with_three_d(super::ThreeD::new(None, Some(&[0]), None))
            .with_groups(ArgumentGroups::new(0, 2, 0)),
        conditional(SUMIF, "SUMIF", (2, 3), SUM_RANGE).with_three_d(super::ThreeD::new(None, Some(&[0, 2]), None)).with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
        conditional(SUMIFS, "SUMIFS", (3, 255), SUM_PAIRS).with_three_d(super::ThreeD::new(None, Some(&[0, 1]), None))
            .with_groups(ArgumentGroups::new(1, 2, 0)).with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
        conditional(AVERAGEIF, "AVERAGEIF", (2, 3), AVERAGE_RANGE).with_three_d(super::ThreeD::new(None, Some(&[0, 2]), Some(&[1]))).with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
        conditional(AVERAGEIFS, "AVERAGEIFS", (3, 255), AVERAGE_PAIRS).with_three_d(super::ThreeD::new(None, Some(&[0, 1]), Some(&[2])))
        .with_groups(ArgumentGroups::new(1, 2, 0)),
        conditional(MAXIFS, "MAXIFS", (3, 255), MAX_PAIRS).with_three_d(super::ThreeD::new(None, Some(&[0, 1]), None))
            .with_groups(ArgumentGroups::new(1, 2, 0)),
        conditional(MINIFS, "MINIFS", (3, 255), MIN_PAIRS).with_three_d(super::ThreeD::new(None, Some(&[0, 1]), None))
            .with_groups(ArgumentGroups::new(1, 2, 0)),
];

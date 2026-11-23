//! Declaration data for the existing conditional worksheet functions.
use super::{
    ArgumentDemand, ArgumentGroups, Availability, EntryClass, EntryKind, FunctionId, Notation,
    Parameter, ResultForm, WorksheetDeclaration,
};
use calc_value::CallableArityShape;
/// Stable family code used by the entries in this fragment.
pub const CODE: u16 = 7;

/// Stable ID for `COUNTIF`.
pub const COUNTIF: FunctionId = FunctionId::new(CODE, 1);
/// Stable ID for `COUNTIFS`.
pub const COUNTIFS: FunctionId = FunctionId::new(CODE, 2);
/// Stable ID for `SUMIF`.
pub const SUMIF: FunctionId = FunctionId::new(CODE, 3);
/// Stable ID for `SUMIFS`.
pub const SUMIFS: FunctionId = FunctionId::new(CODE, 4);
/// Stable ID for `AVERAGEIF`.
pub const AVERAGEIF: FunctionId = FunctionId::new(CODE, 5);
/// Stable ID for `AVERAGEIFS`.
pub const AVERAGEIFS: FunctionId = FunctionId::new(CODE, 6);
/// Stable ID for `MAXIFS`.
pub const MAXIFS: FunctionId = FunctionId::new(CODE, 7);
/// Stable ID for `MINIFS`.
pub const MINIFS: FunctionId = FunctionId::new(CODE, 8);

// Formula entry (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and Formula alike): every range
// below takes only an operand that may give a reference. `=COUNTIF(2,1)`, `=COUNTIF({1,2},1)`,
// `=COUNTIF(1+1,1)` and `=COUNTIF(,1)` are refused and `=COUNTIF(B1:B3,1)` and
// `=COUNTIF(INDEX(B1:B3,0),1)` enter; so for SUMIF's and AVERAGEIF's range and sum or average
// range, each criteria range of COUNTIFS (`=COUNTIFS(B1:B3,1,2,1)` is refused), and the first
// range and each criteria range of SUMIFS, AVERAGEIFS, MAXIFS and MINIFS
// (`=SUMIFS(B1:B3,B1:B3,1,2,1)` is refused). A criterion takes any operand.
const RANGE_CRITERIA: &[Parameter] = &[
    Parameter::reference("range")
        .entry(EntryClass::Reference)
        .legacy(super::LegacyPlace::ArrayAlwaysLiftReferences),
    Parameter::value_or_reference("criteria"),
];
const SUM_RANGE: &[Parameter] = &[
    Parameter::reference("range")
        .entry(EntryClass::Reference)
        .legacy(super::LegacyPlace::ArrayAlwaysLiftReferences),
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
        context_facilities: Some(&[
            super::ContextFacility::NumericMode,
            super::ContextFacility::Cells,
            super::ContextFacility::Host,
            super::ContextFacility::NumberSeparators,
        ]),
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
// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet. dc-b1l
// (Excel 16.0.20527, Touchstone 2026-11-16, cell-context family three-d): a 3-D reference is a
// #REF! criterion (`=COUNTIF(E1:E2,Ja:Ma!A1)` counts the #REF! in E2) and makes a criteria range
// of the *IFS functions #VALUE! (`=COUNTIFS(A1:A3,">0",Ja:Ma!A1,">0")`).
const COUNTIFS_RANGES: [usize; 127] = super::alternate(&[], 0);
const COUNTIFS_CRITERIA: [usize; 127] = super::alternate(&[], 1);
const SUMIFS_RANGES: [usize; 128] = super::alternate(&[0], 1);
const SUMIFS_CRITERIA: [usize; 127] = super::alternate(&[], 2);
// dc-b1l (Touchstone 2026-11-16, reference-arrays): COUNTIFS, AVERAGEIFS, MAXIFS and MINIFS
// given an array of references run once per reference, as SUMIF, COUNTIF and SUMIFS do.
/// Worksheet declarations in this family's member order.
pub const ENTRIES: &[WorksheetDeclaration] = &[
    conditional(COUNTIF, "COUNTIF", (2, 2), RANGE_CRITERIA)
        .with_three_d(super::ThreeD::new(None, Some(&[0]), Some(&[1])))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    conditional(COUNTIFS, "COUNTIFS", (2, 254), COUNT_PAIRS)
        .with_three_d(super::ThreeD::new(
            None,
            Some(&COUNTIFS_RANGES),
            Some(&COUNTIFS_CRITERIA),
        ))
        .with_groups(ArgumentGroups::new(0, 2, 0))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    conditional(SUMIF, "SUMIF", (2, 3), SUM_RANGE)
        .with_three_d(super::ThreeD::new(None, Some(&[0, 2]), Some(&[1])))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    conditional(SUMIFS, "SUMIFS", (3, 255), SUM_PAIRS)
        .with_three_d(super::ThreeD::new(
            None,
            Some(&SUMIFS_RANGES),
            Some(&SUMIFS_CRITERIA),
        ))
        .with_groups(ArgumentGroups::new(1, 2, 0))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    conditional(AVERAGEIF, "AVERAGEIF", (2, 3), AVERAGE_RANGE)
        .with_three_d(super::ThreeD::new(None, Some(&[0, 2]), Some(&[1])))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    conditional(AVERAGEIFS, "AVERAGEIFS", (3, 255), AVERAGE_PAIRS)
        .with_three_d(super::ThreeD::new(
            None,
            Some(&SUMIFS_RANGES),
            Some(&SUMIFS_CRITERIA),
        ))
        .with_groups(ArgumentGroups::new(1, 2, 0))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    conditional(MAXIFS, "MAXIFS", (3, 255), MAX_PAIRS)
        .with_three_d(super::ThreeD::new(
            None,
            Some(&SUMIFS_RANGES),
            Some(&SUMIFS_CRITERIA),
        ))
        .with_groups(ArgumentGroups::new(1, 2, 0))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    conditional(MINIFS, "MINIFS", (3, 255), MIN_PAIRS)
        .with_three_d(super::ThreeD::new(
            None,
            Some(&SUMIFS_RANGES),
            Some(&SUMIFS_CRITERIA),
        ))
        .with_groups(ArgumentGroups::new(1, 2, 0))
        .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
];

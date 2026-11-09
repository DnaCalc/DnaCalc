//! Declaration data for the existing rounding worksheet functions.
use super::{ArgumentDemand, Availability, EntryKind, FunctionId, Notation, Parameter, ResultForm, WorksheetDeclaration};
use calc_value::CallableArityShape;
pub const CODE: u16 = 6;

pub const ABS: FunctionId = FunctionId::new(CODE, 1);
pub const SIGN: FunctionId = FunctionId::new(CODE, 2);
pub const INT: FunctionId = FunctionId::new(CODE, 3);
pub const TRUNC: FunctionId = FunctionId::new(CODE, 4);
pub const ROUND: FunctionId = FunctionId::new(CODE, 5);
pub const ROUNDUP: FunctionId = FunctionId::new(CODE, 6);
pub const ROUNDDOWN: FunctionId = FunctionId::new(CODE, 7);
pub const MROUND: FunctionId = FunctionId::new(CODE, 8);
pub const CEILING: FunctionId = FunctionId::new(CODE, 9);
pub const FLOOR: FunctionId = FunctionId::new(CODE, 10);
pub const MOD: FunctionId = FunctionId::new(CODE, 11);
pub const QUOTIENT: FunctionId = FunctionId::new(CODE, 12);
pub const EVEN: FunctionId = FunctionId::new(CODE, 13);
pub const ODD: FunctionId = FunctionId::new(CODE, 14);
pub const ECMA_CEILING: FunctionId = FunctionId::new(CODE, 15);

const NUMBER: &[Parameter] = &[Parameter::value("number")];
const DIGITS: &[Parameter] = &[Parameter::value("number"), Parameter::value("num_digits")];
const MULTIPLE: &[Parameter] = &[Parameter::value("number"), Parameter::value("multiple")];
const SIGNIFICANCE: &[Parameter] = &[Parameter::value("number"), Parameter::value("significance")];
const DIVISOR: &[Parameter] = &[Parameter::value("number"), Parameter::value("divisor")];
const FRACTION: &[Parameter] = &[
    Parameter::value("numerator"),
    Parameter::value("denominator"),
];

/// An eager named call that takes from `min` to as many arguments as it names parameters.
const fn call(
    id: FunctionId,
    spelling: &'static str,
    min: usize,
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
        arity: CallableArityShape::range(min, parameters.len()),
        groups: None,
        parameters,
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub const ENTRIES: &[WorksheetDeclaration] = &[
        call(ABS, "ABS", 1, NUMBER).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(SIGN, "SIGN", 1, NUMBER).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(INT, "INT", 1, NUMBER).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(TRUNC, "TRUNC", 1, DIGITS).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.flush_subnormal_inputs().retain_overflow_number().retain_subnormal_output()),
        call(ROUND, "ROUND", 2, DIGITS).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.flush_subnormal_inputs().retain_overflow_number()),
        // W111 directed-rounding padding and multi-cell reference controls.
        call(ROUNDUP, "ROUNDUP", 2, DIGITS).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.padding_precedes_call().flush_subnormal_inputs().retain_subnormal_output()),
        call(ROUNDDOWN, "ROUNDDOWN", 2, DIGITS).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.padding_precedes_call().flush_subnormal_inputs().retain_overflow_number().retain_subnormal_output()),
        call(MROUND, "MROUND", 2, MULTIPLE).with_three_d(super::ThreeD::new(None, Some(&[]), None)).with_traits(super::EntryTraits::NONE.reject_multi_cell_reference().strict_number_arguments()),
        call(CEILING, "CEILING", 2, SIGNIFICANCE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(FLOOR, "FLOOR", 2, SIGNIFICANCE).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(MOD, "MOD", 2, DIVISOR).with_three_d(super::ThreeD::new(None, None, Some(&[]))).with_traits(super::EntryTraits::NONE.flush_subnormal_inputs()),
        call(QUOTIENT, "QUOTIENT", 2, FRACTION).with_three_d(super::ThreeD::new(None, Some(&[]), None)).with_traits(super::EntryTraits::NONE.reject_multi_cell_reference().strict_number_arguments()),
        call(EVEN, "EVEN", 1, NUMBER).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(ODD, "ODD", 1, NUMBER).with_three_d(super::ThreeD::new(None, None, Some(&[]))),
        call(ECMA_CEILING, "ECMA.CEILING", 2, SIGNIFICANCE),
];

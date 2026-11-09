//! Declaration data for the existing logical worksheet functions.
use super::{
    ArgumentDemand, Availability, EntryKind, FunctionId, Notation, Parameter, ResultForm,
    WorksheetDeclaration,
};
use calc_value::CallableArityShape;
pub const CODE: u16 = 5;

pub const IF: FunctionId = FunctionId::new(CODE, 1);
pub const AND: FunctionId = FunctionId::new(CODE, 2);
pub const OR: FunctionId = FunctionId::new(CODE, 3);
pub const XOR: FunctionId = FunctionId::new(CODE, 4);
pub const NOT: FunctionId = FunctionId::new(CODE, 5);
pub const TRUE: FunctionId = FunctionId::new(CODE, 6);
pub const FALSE: FunctionId = FunctionId::new(CODE, 7);

const LOGICALS: &[Parameter] = &[
    Parameter::value_or_reference("logical1").legacy(super::LegacyPlace::AggregateAtRoot),
    Parameter::value_or_reference("logical2").legacy(super::LegacyPlace::AggregateAtRootRepeated),
];

/// An eager named call that takes 1 to `most` logicals, values or references, the last parameter
/// repeating.
const fn logicals(id: FunctionId, spelling: &'static str, most: usize) -> WorksheetDeclaration {
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id,
        spelling,
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::range(1, most),
        groups: None,
        parameters: LOGICALS,
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
}

/// An eager named call that takes no argument.
const fn constant(id: FunctionId, spelling: &'static str) -> WorksheetDeclaration {
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id,
        spelling,
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::exact(0),
        groups: None,
        parameters: &[],
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub const ENTRIES: &[WorksheetDeclaration] = &[
    // A call gives one of its values, which may be a reference or a LAMBDA (dc-mx3x, Excel
    // 16.0.20527, 2026-11-07, Formula2 and Formula alike): `=COUNTBLANK(IF(TRUE,A1:A2))` is 1,
    // and `=IF(TRUE,LAMBDA(x,x))(3)` and `=(IF(TRUE,LAMBDA(x,x)))(3)` are 3.
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: IF,
        spelling: "IF",
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::range(2, 3),
        groups: None,
        parameters: &[
            Parameter::value("logical_test"),
            Parameter::value_or_reference("value_if_true"),
            Parameter::value_or_reference("value_if_false"),
        ],
        result: ResultForm::MayBeEither,
        demand: ArgumentDemand::BranchOnCondition,
        // W111: IF materializes selected array blanks and lifted blanks as zero.
        traits: super::EntryTraits::NONE.if_blanks(),
    }
    .with_three_d(super::ThreeD::new(None, None, Some(&[0, 1]))),
    logicals(AND, "AND", 255).with_three_d(super::ThreeD::new(Some(&[]), None, None)),
    logicals(OR, "OR", 255).with_three_d(super::ThreeD::new(Some(&[]), None, None)),
    // XOR takes one argument fewer than AND and OR: Microsoft's XOR page gives 1 to 254
    // conditions.
    logicals(XOR, "XOR", 254).with_three_d(super::ThreeD::new(Some(&[]), None, None)),
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: NOT,
        spelling: "NOT",
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::exact(1),
        groups: None,
        parameters: &[Parameter::value("logical")],
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
    .with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    constant(TRUE, "TRUE"),
    constant(FALSE, "FALSE"),
];

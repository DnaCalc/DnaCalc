//! Operator declaration fragments moved from the existing CalcFunc catalogue.
use crate::{
    ArgumentDemand, Availability, EntryKind, EntryTraits, FunctionId, Notation, Parameter,
    ResultForm, WorksheetDeclaration,
};

const fn operator(
    id: FunctionId,
    spelling: &'static str,
    notation: Notation,
    parameters: &'static [Parameter],
) -> WorksheetDeclaration {
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id,
        spelling,
        notation,
        parameters,
        kind: EntryKind::Operator {
            file_spelling: None,
        },
        availability: Availability::Unspecified,
        arity: calc_value::CallableArityShape::exact(parameters.len()),
        groups: None,
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: EntryTraits::NONE,
    }
}

/// Arithmetic operators, including prefix signs and postfix percent.
pub mod arithmetic {
    use super::*;
    /// Stable family code used by the entries in this fragment.
    pub const CODE: u16 = 1;
    /// Stable ID for infix addition (`+`).
    pub const PLUS: FunctionId = FunctionId::new(CODE, 1);
    /// Stable ID for infix subtraction (`-`).
    pub const MINUS: FunctionId = FunctionId::new(CODE, 2);
    /// Stable ID for infix multiplication (`*`).
    pub const TIMES: FunctionId = FunctionId::new(CODE, 3);
    /// Stable ID for infix division (`/`).
    pub const DIVIDE: FunctionId = FunctionId::new(CODE, 4);
    /// Stable ID for infix exponentiation (`^`).
    pub const POWER: FunctionId = FunctionId::new(CODE, 5);
    /// Stable ID for prefix negation (`-`).
    pub const NEGATE: FunctionId = FunctionId::new(CODE, 6);
    /// Stable ID for prefix positive sign (`+`).
    pub const UNARY_PLUS: FunctionId = FunctionId::new(CODE, 7);
    /// Stable ID for postfix percent (`%`).
    pub const PERCENT: FunctionId = FunctionId::new(CODE, 8);
    const NUMBERS: &[Parameter] = &[Parameter::value("number1"), Parameter::value("number2")];
    const NUMBER: &[Parameter] = &[Parameter::value("number")];
    /// Worksheet declarations in this family's member order.
    pub const ENTRIES: &[WorksheetDeclaration] = &[
        operator(PLUS, "+", Notation::Infix, NUMBERS),
        operator(MINUS, "-", Notation::Infix, NUMBERS),
        operator(TIMES, "*", Notation::Infix, NUMBERS),
        operator(DIVIDE, "/", Notation::Infix, NUMBERS),
        operator(POWER, "^", Notation::Infix, NUMBERS),
        operator(NEGATE, "-", Notation::Prefix, NUMBER),
        operator(UNARY_PLUS, "+", Notation::Prefix, NUMBER),
        operator(PERCENT, "%", Notation::Postfix, NUMBER),
    ];
}

/// Infix comparisons such as equality and ordering.
pub mod comparison {
    use super::*;
    /// Stable family code used by the entries in this fragment.
    pub const CODE: u16 = 2;
    /// Stable ID for equality (`=`).
    pub const EQUAL: FunctionId = FunctionId::new(CODE, 1);
    /// Stable ID for inequality (`<>`).
    pub const NOT_EQUAL: FunctionId = FunctionId::new(CODE, 2);
    /// Stable ID for less-than comparison (`<`).
    pub const LESS: FunctionId = FunctionId::new(CODE, 3);
    /// Stable ID for less-than-or-equal comparison (`<=`).
    pub const LESS_OR_EQUAL: FunctionId = FunctionId::new(CODE, 4);
    /// Stable ID for greater-than comparison (`>`).
    pub const GREATER: FunctionId = FunctionId::new(CODE, 5);
    /// Stable ID for greater-than-or-equal comparison (`>=`).
    pub const GREATER_OR_EQUAL: FunctionId = FunctionId::new(CODE, 6);
    const VALUES: &[Parameter] = &[Parameter::value("value1"), Parameter::value("value2")];
    /// Worksheet declarations in this family's member order.
    pub const ENTRIES: &[WorksheetDeclaration] = &[
        operator(EQUAL, "=", Notation::Infix, VALUES),
        operator(NOT_EQUAL, "<>", Notation::Infix, VALUES),
        operator(LESS, "<", Notation::Infix, VALUES),
        operator(LESS_OR_EQUAL, "<=", Notation::Infix, VALUES),
        operator(GREATER, ">", Notation::Infix, VALUES),
        operator(GREATER_OR_EQUAL, ">=", Notation::Infix, VALUES),
    ];
}

/// The infix text concatenation operator.
pub mod text {
    use super::*;
    /// Stable family code used by the entries in this fragment.
    pub const CODE: u16 = 3;
    /// Stable ID for infix text concatenation (`&`).
    pub const CONCATENATE: FunctionId = FunctionId::new(CODE, 1);
    const TEXTS: &[Parameter] = &[Parameter::value("text1"), Parameter::value("text2")];
    /// Worksheet declarations in this family's member order.
    pub const ENTRIES: &[WorksheetDeclaration] =
        &[operator(CONCATENATE, "&", Notation::Infix, TEXTS)];
}

//! Operator declaration fragments moved from the existing CalcFunc catalogue.
use crate::{ArgumentDemand, Availability, EntryKind, EntryTraits, FunctionId, Notation, Parameter, ResultForm, WorksheetDeclaration};

const fn operator(id: FunctionId, spelling: &'static str, notation: Notation, parameters: &'static [Parameter]) -> WorksheetDeclaration {
    WorksheetDeclaration {
        id, spelling, notation, parameters, kind: EntryKind::Operator { file_spelling: None },
        availability: Availability::Unspecified, arity: calc_value::CallableArityShape::exact(parameters.len()),
        groups: None, result: ResultForm::Value, demand: ArgumentDemand::Eager, traits: EntryTraits::NONE,
    }
}

pub mod arithmetic {
    use super::*;
    pub const CODE: u16 = 1;
    pub const PLUS: FunctionId = FunctionId::new(CODE, 1);
    pub const MINUS: FunctionId = FunctionId::new(CODE, 2);
    pub const TIMES: FunctionId = FunctionId::new(CODE, 3);
    pub const DIVIDE: FunctionId = FunctionId::new(CODE, 4);
    pub const POWER: FunctionId = FunctionId::new(CODE, 5);
    pub const NEGATE: FunctionId = FunctionId::new(CODE, 6);
    pub const UNARY_PLUS: FunctionId = FunctionId::new(CODE, 7);
    pub const PERCENT: FunctionId = FunctionId::new(CODE, 8);
    const NUMBERS: &[Parameter] = &[Parameter::value("number1"), Parameter::value("number2")];
    const NUMBER: &[Parameter] = &[Parameter::value("number")];
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

pub mod comparison {
    use super::*;
    pub const CODE: u16 = 2;
    pub const EQUAL: FunctionId = FunctionId::new(CODE, 1);
    pub const NOT_EQUAL: FunctionId = FunctionId::new(CODE, 2);
    pub const LESS: FunctionId = FunctionId::new(CODE, 3);
    pub const LESS_OR_EQUAL: FunctionId = FunctionId::new(CODE, 4);
    pub const GREATER: FunctionId = FunctionId::new(CODE, 5);
    pub const GREATER_OR_EQUAL: FunctionId = FunctionId::new(CODE, 6);
    const VALUES: &[Parameter] = &[Parameter::value("value1"), Parameter::value("value2")];
    pub const ENTRIES: &[WorksheetDeclaration] = &[
        operator(EQUAL, "=", Notation::Infix, VALUES),
        operator(NOT_EQUAL, "<>", Notation::Infix, VALUES),
        operator(LESS, "<", Notation::Infix, VALUES),
        operator(LESS_OR_EQUAL, "<=", Notation::Infix, VALUES),
        operator(GREATER, ">", Notation::Infix, VALUES),
        operator(GREATER_OR_EQUAL, ">=", Notation::Infix, VALUES),
    ];
}

pub mod text {
    use super::*;
    pub const CODE: u16 = 3;
    pub const CONCATENATE: FunctionId = FunctionId::new(CODE, 1);
    const TEXTS: &[Parameter] = &[Parameter::value("text1"), Parameter::value("text2")];
    pub const ENTRIES: &[WorksheetDeclaration] = &[
        operator(CONCATENATE, "&", Notation::Infix, TEXTS),
    ];
}

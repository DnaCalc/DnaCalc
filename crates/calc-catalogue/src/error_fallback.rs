//! Declaration data moved from the existing CalcFunc catalogue.
use super::{ArgumentDemand, FunctionId, Notation, Parameter, WorksheetDeclaration};
use calc_value::CallableArityShape;
pub const CODE: u16 = 8;

pub const IFERROR: FunctionId = FunctionId::new(CODE, 1);
pub const IFNA: FunctionId = FunctionId::new(CODE, 2);

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub const ENTRIES: &[WorksheetDeclaration] = &[
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: IFERROR,
        spelling: "IFERROR",
        notation: Notation::Call,
        kind: super::EntryKind::Worksheet,
        availability: super::Availability::Unspecified,
        arity: CallableArityShape::exact(2),
        groups: None,
        parameters: &[
            Parameter::value("value"),
            Parameter::value("value_if_error"),
        ],
        result: super::ResultForm::Value,
        demand: ArgumentDemand::FallbackOnError,
        traits: super::EntryTraits::NONE,
    }
    .with_three_d(super::ThreeD::new(None, None, Some(&[0]))),
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: IFNA,
        spelling: "IFNA",
        notation: Notation::Call,
        kind: super::EntryKind::Worksheet,
        availability: super::Availability::Unspecified,
        arity: CallableArityShape::exact(2),
        groups: None,
        parameters: &[Parameter::value("value"), Parameter::value("value_if_na")],
        result: super::ResultForm::Value,
        demand: ArgumentDemand::FallbackOnError,
        traits: super::EntryTraits::NONE,
    }
    .with_three_d(super::ThreeD::new(None, None, Some(&[0]))),
];

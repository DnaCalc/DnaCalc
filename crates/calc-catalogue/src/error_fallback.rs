//! Declaration data moved from the existing CalcFunc catalogue.
use super::{ArgumentDemand, FunctionId, Notation, Parameter, WorksheetDeclaration};
use calc_value::CallableArityShape;
/// Stable family code used by the entries in this fragment.
pub const CODE: u16 = 8;

/// Stable ID for `IFERROR`.
pub const IFERROR: FunctionId = FunctionId::new(CODE, 1);
/// Stable ID for `IFNA`.
pub const IFNA: FunctionId = FunctionId::new(CODE, 2);

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
/// Worksheet declarations in this family's member order.
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
    // dc-b1l (Touchstone 2026-11-16, three-d): a 3-D fallback is #VALUE! where it is given
    // (`=IFERROR(1,Ja:Ma!A1)` 1, `=IFERROR(NA(),Ja:Ma!A1)` #VALUE!; IFNA likewise).
    .with_three_d(super::ThreeD::new(None, None, Some(&[0])).value_arguments(&[1])),
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
    // dc-b1l (Touchstone 2026-11-16, three-d): a 3-D fallback is #VALUE! where it is given
    // (`=IFERROR(1,Ja:Ma!A1)` 1, `=IFERROR(NA(),Ja:Ma!A1)` #VALUE!; IFNA likewise).
    .with_three_d(super::ThreeD::new(None, None, Some(&[0])).value_arguments(&[1])),
];

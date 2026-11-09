//! File-independent declarations for the reference-offset family.
use crate::{ArgumentDemand, FunctionId, Notation, Parameter, WorksheetDeclaration};
use calc_value::CallableArityShape;

pub const CODE: u16 = 40;

pub const OFFSET: FunctionId = FunctionId::new(CODE, 1);
pub const ISOMITTED: FunctionId = FunctionId::new(CODE, 2);
pub const TRIMRANGE: FunctionId = FunctionId::new(CODE, 3);

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub const ENTRIES: &[WorksheetDeclaration] = &[
    // Formula entry (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and Formula alike): the
    // reference takes only an operand that may give one (`=OFFSET(2,0,0)`, `=OFFSET({1,2},0,0)`,
    // `=OFFSET(1+1,0,0)` and `=OFFSET(,0,0)` are refused, `=OFFSET(B1:B3,0,0)` and
    // `=OFFSET(INDEX(B1:B3,0),0,0)` enter), and a call may give a reference:
    // `=COUNTBLANK(OFFSET(A1,0,0))` enters, and so does `=OFFSET(A1,0,0)(3)` (#REF!).
    WorksheetDeclaration {
        id: OFFSET,
        kind: super::EntryKind::Worksheet,
        spelling: "OFFSET",
        notation: Notation::Call,
        availability: super::Availability::Unspecified,
        arity: CallableArityShape::range(3, 5),
        groups: None,
        parameters: &[
            Parameter::reference("reference").entry(super::EntryClass::Reference),
            Parameter::value("rows"),
            Parameter::value("cols"),
            Parameter::value("height"),
            Parameter::value("width"),
        ],
        result: super::ResultForm::MayBeReference,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE.volatile(),
    }
    .with_three_d(super::ThreeD::new(None, Some(&[0]), Some(&[1, usize::MAX]))),
    WorksheetDeclaration {
        id: ISOMITTED,
        kind: super::EntryKind::Worksheet,
        spelling: "ISOMITTED",
        notation: Notation::Call,
        availability: super::Availability::Unspecified,
        arity: CallableArityShape::exact(1),
        groups: None,
        parameters: &[Parameter::value_or_reference("argument")],
        result: super::ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
    .with_three_d(super::ThreeD::new(None, None, Some(&[]))),
    // A call may give a reference (dc-mx3x): `=COUNTBLANK(TRIMRANGE(B1))` and
    // `=TRIMRANGE(B1)(3)` enter. Its range takes any operand: `=TRIMRANGE(2)` enters.
    WorksheetDeclaration {
        id: TRIMRANGE,
        kind: super::EntryKind::Worksheet,
        spelling: "TRIMRANGE",
        notation: Notation::Call,
        availability: super::Availability::Unspecified,
        arity: CallableArityShape::range(1, 3),
        groups: None,
        parameters: &[
            Parameter::value_or_reference("range"),
            Parameter::value("trim_rows"),
            Parameter::value("trim_cols"),
        ],
        result: super::ResultForm::MayBeReference,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    },
];

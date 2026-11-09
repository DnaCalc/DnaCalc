//! Declaration data for the existing lookup and reference worksheet functions.
use super::{
    ArgumentDemand, Availability, EntryKind, FunctionId, Notation, Parameter, ResultForm,
    WorksheetDeclaration,
};
use calc_value::CallableArityShape;
pub const CODE: u16 = 9;

pub const INDEX: FunctionId = FunctionId::new(CODE, 1);
pub const MATCH: FunctionId = FunctionId::new(CODE, 2);
pub const XMATCH: FunctionId = FunctionId::new(CODE, 3);
pub const XLOOKUP: FunctionId = FunctionId::new(CODE, 4);
pub const VLOOKUP: FunctionId = FunctionId::new(CODE, 5);
pub const HLOOKUP: FunctionId = FunctionId::new(CODE, 6);
pub const LOOKUP: FunctionId = FunctionId::new(CODE, 7);
pub const CHOOSE: FunctionId = FunctionId::new(CODE, 8);
pub const ROW: FunctionId = FunctionId::new(CODE, 9);
pub const COLUMN: FunctionId = FunctionId::new(CODE, 10);
pub const ROWS: FunctionId = FunctionId::new(CODE, 11);
pub const COLUMNS: FunctionId = FunctionId::new(CODE, 12);
pub const AREAS: FunctionId = FunctionId::new(CODE, 13);

/// An eager named call.
const fn eager(
    id: FunctionId,
    spelling: &'static str,
    arity: CallableArityShape,
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
        arity,
        groups: None,
        parameters,
        result: ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
pub const ENTRIES: &[WorksheetDeclaration] = &[
    // A call may give a reference (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and
    // Formula alike): `=COUNTBLANK(INDEX(A1:A2,1))` enters, and so does `=INDEX(A1:A2,1)(3)`
    // (#REF!). Its array takes any operand: `=INDEX(2,1)` enters.
    eager(
        INDEX,
        "INDEX",
        CallableArityShape::range(2, 4),
        &[
            Parameter::value_or_reference("array").legacy(super::LegacyPlace::ArrayAlways),
            Parameter::value("row_num"),
            Parameter::value("column_num"),
            Parameter::value("area_num"),
        ],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[0]), Some(&[1, usize::MAX])))
    .with_result(ResultForm::MayBeReference)
    .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
    eager(
        MATCH,
        "MATCH",
        CallableArityShape::range(2, 3),
        &[
            Parameter::value("lookup_value"),
            Parameter::value_or_reference("lookup_array")
                .legacy(super::LegacyPlace::AggregateAtRoot),
            Parameter::value("match_type"),
        ],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[1, 2]), Some(&[0]))),
    eager(
        XMATCH,
        "XMATCH",
        CallableArityShape::range(2, 4),
        &[
            Parameter::value("lookup_value"),
            Parameter::value_or_reference("lookup_array").legacy(super::LegacyPlace::ArrayAtRoot),
            Parameter::value("match_mode"),
            Parameter::value("search_mode"),
        ],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[1]), Some(&[0, 2]))),
    eager(
        XLOOKUP,
        "XLOOKUP",
        CallableArityShape::range(3, 6),
        &[
            Parameter::value("lookup_value"),
            Parameter::value_or_reference("lookup_array").legacy(super::LegacyPlace::ArrayAtRoot),
            Parameter::value_or_reference("return_array").legacy(super::LegacyPlace::ArrayAtRoot),
            Parameter::value_or_reference("if_not_found")
                .legacy(super::LegacyPlace::AggregateAtRoot),
            Parameter::value("match_mode"),
            Parameter::value("search_mode"),
        ],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[1, 2]), Some(&[0])))
    // A call may give a reference (dc-mx3x): `=COUNTBLANK(XLOOKUP(1,B1:B3,B1:B3))` and
    // `=XLOOKUP(1,B1:B3,B1:B3)(3)` (#REF!) enter.
    .with_result(ResultForm::MayBeReference),
    eager(
        VLOOKUP,
        "VLOOKUP",
        CallableArityShape::range(3, 4),
        &[
            Parameter::value("lookup_value"),
            Parameter::value_or_reference("table_array")
                .legacy(super::LegacyPlace::AggregateAtRoot),
            Parameter::value("col_index_num"),
            Parameter::value("range_lookup"),
        ],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[1]), Some(&[0, 2, 3]))),
    eager(
        HLOOKUP,
        "HLOOKUP",
        CallableArityShape::range(3, 4),
        &[
            Parameter::value("lookup_value"),
            Parameter::value_or_reference("table_array")
                .legacy(super::LegacyPlace::AggregateAtRoot),
            Parameter::value("row_index_num"),
            Parameter::value("range_lookup"),
        ],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[1]), Some(&[0, 2, 3]))),
    eager(
        LOOKUP,
        "LOOKUP",
        CallableArityShape::range(2, 3),
        &[
            Parameter::value("lookup_value"),
            Parameter::value_or_reference("lookup_vector").legacy(super::LegacyPlace::ArrayAtRoot),
            Parameter::value_or_reference("result_vector").legacy(super::LegacyPlace::ArrayAtRoot),
        ],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[1, 2]), Some(&[0]))),
    // A call gives one of its values, which may be a reference or a LAMBDA (dc-mx3x):
    // `=COUNTBLANK(CHOOSE(1,A1))` enters, and `=CHOOSE(1,LAMBDA(x,x))(3)` is 3.
    WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: CHOOSE,
        spelling: "CHOOSE",
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::range(2, 255),
        groups: None,
        parameters: &[
            Parameter::value("index_num"),
            Parameter::value_or_reference("value1"),
            Parameter::value_or_reference("value2"),
        ],
        result: ResultForm::MayBeEither,
        demand: ArgumentDemand::BranchOnCondition,
        // Dynamic fn2 q193: lifted CHOOSE materializes a selected blank as zero.
        traits: super::EntryTraits::NONE.choose_blanks(),
    }
    .with_three_d(super::ThreeD::new(None, None, Some(&[0, 1]))),
    // ROW's, COLUMN's and AREAS's reference takes only an operand that may give one
    // (dc-mx3x, both forms): `=ROW(2)`, `=ROW({1,2})`, `=ROW(1+1)`, `=COLUMN(2)` and `=AREAS(2)`
    // are refused, `=ROW(B1:B3)`, `=ROW(INDEX(B1:B3,0))` and `=AREAS(B1:B3)` enter. ROWS's
    // and COLUMNS's array takes any operand: `=ROWS(2)` and `=ROWS({1,2})` enter.
    eager(
        ROW,
        "ROW",
        CallableArityShape::range(0, 1),
        &[Parameter::value_or_reference("reference")
            .entry(super::EntryClass::Reference)
            .legacy(super::LegacyPlace::ArrayAlways)],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[]), None))
    .with_traits(
        super::EntryTraits::NONE
            .intersect_legacy_result()
            .lift_dynamic_references(),
    ),
    eager(
        COLUMN,
        "COLUMN",
        CallableArityShape::range(0, 1),
        &[Parameter::value_or_reference("reference").entry(super::EntryClass::Reference)],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[]), None))
    .with_traits(super::EntryTraits::NONE.intersect_legacy_result()),
    eager(
        ROWS,
        "ROWS",
        CallableArityShape::exact(1),
        &[Parameter::value_or_reference("array").legacy(super::LegacyPlace::ArrayAtRoot)],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[]), None))
    .with_traits(super::EntryTraits::NONE.take_reference_array()),
    eager(
        COLUMNS,
        "COLUMNS",
        CallableArityShape::exact(1),
        &[Parameter::value_or_reference("array").legacy(super::LegacyPlace::ArrayAtRoot)],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[]), None)),
    eager(
        AREAS,
        "AREAS",
        CallableArityShape::exact(1),
        &[Parameter::value_or_reference("reference")
            .entry(super::EntryClass::Reference)
            .legacy(super::LegacyPlace::ArrayAlwaysLiftReferences)],
    )
    .with_three_d(super::ThreeD::new(None, Some(&[]), None))
    .with_traits(super::EntryTraits::NONE.lift_dynamic_references()),
];

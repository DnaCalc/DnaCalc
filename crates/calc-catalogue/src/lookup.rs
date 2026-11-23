//! Declaration data for the existing lookup and reference worksheet functions.
use super::{
    ArgumentDemand, Availability, EntryKind, FunctionId, Notation, Parameter, ResultForm,
    WorksheetDeclaration,
};
use calc_value::CallableArityShape;
/// Stable family code used by the entries in this fragment.
pub const CODE: u16 = 9;

/// Stable ID for `INDEX`.
pub const INDEX: FunctionId = FunctionId::new(CODE, 1);
/// Stable ID for `MATCH`.
pub const MATCH: FunctionId = FunctionId::new(CODE, 2);
/// Stable ID for `XMATCH`.
pub const XMATCH: FunctionId = FunctionId::new(CODE, 3);
/// Stable ID for `XLOOKUP`.
pub const XLOOKUP: FunctionId = FunctionId::new(CODE, 4);
/// Stable ID for `VLOOKUP`.
pub const VLOOKUP: FunctionId = FunctionId::new(CODE, 5);
/// Stable ID for `HLOOKUP`.
pub const HLOOKUP: FunctionId = FunctionId::new(CODE, 6);
/// Stable ID for `LOOKUP`.
pub const LOOKUP: FunctionId = FunctionId::new(CODE, 7);
/// Stable ID for `CHOOSE`.
pub const CHOOSE: FunctionId = FunctionId::new(CODE, 8);
/// Stable ID for `ROW`.
pub const ROW: FunctionId = FunctionId::new(CODE, 9);
/// Stable ID for `COLUMN`.
pub const COLUMN: FunctionId = FunctionId::new(CODE, 10);
/// Stable ID for `ROWS`.
pub const ROWS: FunctionId = FunctionId::new(CODE, 11);
/// Stable ID for `COLUMNS`.
pub const COLUMNS: FunctionId = FunctionId::new(CODE, 12);
/// Stable ID for `AREAS`.
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
/// Worksheet declarations in this family's member order.
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
    // dc-b1l (Touchstone 2026-11-16, reference-arrays): an array of references is indexed as an
    // array, its selected element a reference (`=INDEX(OFFSET(A1,{0;1;2},0,2),2,1)` spills 2|3,
    // `=ISREF(INDEX(OFFSET(A1,{0;1;2},0,2),1,1))` is TRUE).
    .with_traits(super::EntryTraits::NONE.take_reference_array()),
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
    .with_context(&[super::ContextFacility::NumericMode, super::ContextFacility::Cells])
    .with_three_d(super::ThreeD::new(None, Some(&[1, 2]), Some(&[0])))
    .with_traits(super::EntryTraits::NONE.value_error_references()),
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
    .with_context(&[super::ContextFacility::NumericMode, super::ContextFacility::Cells])
    // dc-b1l (Touchstone 2026-11-16, three-d): a 3-D search_mode is #REF!.
    .with_three_d(super::ThreeD::new(None, Some(&[1]), Some(&[0, 2, 3])))
    .with_traits(super::EntryTraits::NONE.value_error_references()),
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
    .with_context(&[super::ContextFacility::NumericMode, super::ContextFacility::Cells])
    // dc-b1l (Touchstone 2026-11-16, three-d): a 3-D if_not_found is #VALUE! where it is given
    // (`=XLOOKUP(2,A1:A3,B1:B3,Ja:Ma!A1)` 20, `=XLOOKUP(9,...)` #VALUE!), the modes #REF!.
    .with_three_d(super::ThreeD::new(None, Some(&[1, 2]), Some(&[0, 4, 5])).value_arguments(&[3]))
    // A call may give a reference (dc-mx3x): `=COUNTBLANK(XLOOKUP(1,B1:B3,B1:B3))` and
    // `=XLOOKUP(1,B1:B3,B1:B3)(3)` (#REF!) enter.
    .with_result(ResultForm::MayBeReference)
    .with_traits(super::EntryTraits::NONE.value_error_references()),
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
    .with_context(&[super::ContextFacility::NumericMode, super::ContextFacility::Cells])
    .with_three_d(super::ThreeD::new(None, Some(&[1]), Some(&[0, 2, 3])))
    .with_traits(super::EntryTraits::NONE.value_error_references()),
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
    .with_context(&[super::ContextFacility::NumericMode, super::ContextFacility::Cells])
    .with_three_d(super::ThreeD::new(None, Some(&[1]), Some(&[0, 2, 3])))
    .with_traits(super::EntryTraits::NONE.value_error_references()),
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
    .with_context(&[super::ContextFacility::NumericMode, super::ContextFacility::Cells])
    .with_three_d(super::ThreeD::new(None, Some(&[1, 2]), Some(&[0])))
    .with_traits(super::EntryTraits::NONE.value_error_references()),
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
    // dc-b1l (Touchstone 2026-11-16, three-d): a 3-D later value is #VALUE! where it is chosen
    // (`=CHOOSE(1,1,Ja:Ma!A1)` 1, `=CHOOSE(2,1,Ja:Ma!A1)` #VALUE!).
    .with_three_d(super::ThreeD::new(None, None, Some(&[0, 1])).value_arguments(&[2, usize::MAX])),
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
    .with_context(&[super::ContextFacility::NumericMode, super::ContextFacility::Host])
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
    .with_context(&[super::ContextFacility::NumericMode, super::ContextFacility::Host])
    .with_three_d(super::ThreeD::new(None, Some(&[]), None))
    .with_traits(
        super::EntryTraits::NONE
            .intersect_legacy_result()
            .lift_dynamic_references(),
    ),
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
    .with_three_d(super::ThreeD::new(None, Some(&[]), None))
    .with_traits(super::EntryTraits::NONE.value_error_references()),
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

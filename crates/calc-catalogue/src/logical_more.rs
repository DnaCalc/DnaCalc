//! Declaration data for the existing selection worksheet functions.
use super::{ArgumentDemand, Availability, EntryKind, FunctionId, Notation, Parameter, ResultForm, WorksheetDeclaration};
use calc_value::CallableArityShape;
pub const CODE: u16 = 201;
pub const IFS: FunctionId = FunctionId::new(CODE, 1);
pub const SWITCH: FunctionId = FunctionId::new(CODE, 2);

pub const ENTRIES: &[WorksheetDeclaration] = &[
        // Formula entry (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and Formula alike): the
        // conditions and values come in pairs, so IFS takes 2, 4, 6 or 8 arguments of 1 to 8
        // (`=IFS(TRUE)` and `=IFS(TRUE,1,FALSE)` are refused, `=IFS(TRUE,1,FALSE,2)` enters), and
        // a call gives one of its values, which may be a reference or a LAMBDA:
        // `=COUNTBLANK(IFS(TRUE,A1))` enters and `=IFS(TRUE,LAMBDA(x,x))(3)` is 3.
        WorksheetDeclaration {
            localized_names: None,
            context_facilities: None,
            id: IFS,
            spelling: "IFS",
            notation: Notation::Call,
            kind: EntryKind::Worksheet,
            availability: Availability::Unspecified,
            arity: CallableArityShape::range(2, 254),
            groups: Some(super::ArgumentGroups::new(0, 2, 0)),
            parameters: &[
                Parameter::value("logical_test1"),
                Parameter::value("value_if_true1"),
            ],
            result: ResultForm::MayBeEither,
            demand: ArgumentDemand::Eager,
            traits: super::EntryTraits::NONE.paired_result(super::PairedResult::Odd).whole_result(super::WholeResultPolicy::Conditions),
        }.with_three_d(super::ThreeD::new(None, Some(&[1]), Some(&[0]))),
        // The values and results come in pairs, with an optional default after them (dc-mx3x):
        // SWITCH takes every count from 3 to 8 (`=SWITCH(1,1)` is refused, `=SWITCH(1,1,2)` and
        // `=SWITCH(1,1,2,3)` enter), and a call gives one of its results, which may be a
        // reference or a LAMBDA: `=COUNTBLANK(SWITCH(1,1,A1))` enters and
        // `=SWITCH(1,1,LAMBDA(x,x))(3)` is 3.
        WorksheetDeclaration {
            localized_names: None,
            context_facilities: None,
            id: SWITCH,
            spelling: "SWITCH",
            notation: Notation::Call,
            kind: EntryKind::Worksheet,
            availability: Availability::Unspecified,
            arity: CallableArityShape::range(3, 254),
            groups: Some(super::ArgumentGroups::new(1, 2, 1)),
            parameters: &[
                Parameter::value("expression"),
                Parameter::value("value1"),
                Parameter::value("result1"),
            ],
            result: ResultForm::MayBeEither,
            demand: ArgumentDemand::Eager,
            traits: super::EntryTraits::NONE.paired_result(super::PairedResult::EvenAfterSelector).whole_result(super::WholeResultPolicy::Selector),
        }.with_three_d(super::ThreeD::new(None, Some(&[2]), Some(&[0, 1]))),
];

//! Declaration data moved from the existing CalcFunc catalogue.
use super::{ArgumentDemand, FunctionId, Notation, Parameter, WorksheetDeclaration};
use calc_value::CallableArityShape;
pub const CODE: u16 = 320;

pub const PERCENTOF: FunctionId = FunctionId::new(CODE, 1);

pub const ENTRIES: &[WorksheetDeclaration] = &[WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: PERCENTOF,
        spelling: "PERCENTOF",
        notation: Notation::Call,
        kind: super::EntryKind::Worksheet,
        availability: super::Availability::Unspecified,
        arity: CallableArityShape::exact(2),
        groups: None,
        parameters: &[
            Parameter::value_or_reference("data_subset"),
            Parameter::value_or_reference("data_all"),
        ],
        result: super::ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }];

//! Declaration data moved from the existing CalcFunc catalogue.
use super::{ArgumentDemand, FunctionId, Notation, Parameter, WorksheetDeclaration};
use calc_value::CallableArityShape;
const R: super::ParameterClass = super::ParameterClass::ValueOrReference;
pub const CODE: u16 = 322;
pub const EUROCONVERT: FunctionId = FunctionId::new(CODE, 1);

/// The catalogue spelling of the add-in's function, which only a library-qualified call reaches.
pub const SURFACE: &str = "_EUROTOOL.EUROCONVERT";

pub const ENTRIES: &[WorksheetDeclaration] = &[WorksheetDeclaration {
        localized_names: None,
        context_facilities: None,
        id: EUROCONVERT,
        spelling: SURFACE,
        notation: Notation::Call,
        kind: super::EntryKind::AddIn { library: "EUROTOOL.XLAM", export: "EUROCONVERT" },
        availability: super::Availability::Unspecified,
        // The add-in's own check answers a wrong count: #VALUE!, not a refused entry.
        arity: CallableArityShape::range(0, 255),
        groups: None,
        parameters: &[
            Parameter::of("number", R),
            Parameter::of("source", R),
            Parameter::of("target", R),
            Parameter::of("full_precision", R),
            Parameter::of("triangulation_precision", R),
        ],
        result: super::ResultForm::Value,
        demand: ArgumentDemand::Eager,
        traits: super::EntryTraits::NONE,
    }];

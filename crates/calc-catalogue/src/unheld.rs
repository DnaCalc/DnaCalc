//! Names whose execution belongs to another package or is deferred by owner policy.
//! The service deferrals are dc-fetch-f0tp; the compatibility-version-3 list functions are
//! dc-nk93. CALL and REGISTER.ID run only on a macro sheet in Excel and are refused at entry in a
//! worksheet formula (dc-gh22); RTD uses the dc-snz.23.4 host route.
use crate::{
    ArgumentDemand, Availability, BindingForm, Deferral, EntryTraits, FunctionId, Implementation,
    Notation, Parameter, ResultForm,
};
use calc_value::CallableArityShape;

/// File and entry metadata for a function or an operator without an ordinary kernel.
#[derive(Clone, Copy, Debug)]
pub struct UnheldEntry {
    pub id: FunctionId,
    pub spelling: &'static str,
    pub notation: Notation,
    pub kind: UnheldKind,
    pub availability: Availability,
    pub arity: CallableArityShape,
    pub groups: Option<crate::ArgumentGroups>,
    pub parameters: &'static [Parameter],
    pub result: ResultForm,
    pub demand: ArgumentDemand,
    pub traits: EntryTraits,
    pub implementation: Implementation,
}

#[derive(Clone, Copy, Debug)]
pub enum UnheldKind {
    Worksheet,
    Binding(BindingForm),
    Operator { file_spelling: Option<&'static str> },
    Rtd { provider: &'static str },
}

/// Provider identity for the RTD route.
pub const RTD_PROVIDER: &str = "calc-fml/rtd";

const CODE: u16 = 325;

/// Stable IDs of the two reference operators with function-shaped file forms.
pub const IMPLICIT_INTERSECTION_ID: FunctionId = FunctionId::new(CODE, 15);
pub const SPILL_ID: FunctionId = FunctionId::new(CODE, 16);

/// The file spelling of a declared reference operator.
pub fn operator_file_spelling(id: FunctionId) -> Option<&'static str> {
    UNHELD_ENTRIES
        .iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| match entry.kind {
            UnheldKind::Operator { file_spelling } => file_spelling,
            _ => None,
        })
}
/// File form of a trim reference operator measured in the workbook format.
pub fn trim_reference_file_spelling(operator: &str) -> Option<(crate::FilePrefix, &'static str)> {
    let name = match operator {
        ".:" => "_TRO_LEADING",
        ":." => "_TRO_TRAILING",
        ".:." => "_TRO_ALL",
        _ => return None,
    };
    Some((crate::FilePrefix::Xlfn, name))
}
// Signature names and bounds are handbook seeds (calcfunc-catalogue 481f4ab85d3b);
// entry does not claim they were measured in Excel.
const CALL_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("register_id"),
    Parameter::value_or_reference("argument1"),
];
const REGISTER_ID_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("module_text"),
    Parameter::value_or_reference("procedure"),
    Parameter::value_or_reference("type_text"),
];
const RTD_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("ProgID"),
    Parameter::value_or_reference("server"),
    Parameter::value_or_reference("topic1"),
    Parameter::value_or_reference("topic2"),
];
const COPILOT_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("prompt_part1"),
    Parameter::value_or_reference("context1"),
    Parameter::value_or_reference("prompt_part2"),
    Parameter::value_or_reference("context2"),
];
const DETECTLANGUAGE_ARGS: &[Parameter] = &[Parameter::value_or_reference("text")];
const STOCKHISTORY_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("stock"),
    Parameter::value_or_reference("start_date"),
    Parameter::value_or_reference("end_date"),
    Parameter::value_or_reference("interval"),
    Parameter::value_or_reference("headers"),
    Parameter::value_or_reference("property0"),
    Parameter::value_or_reference("property1"),
    Parameter::value_or_reference("property2"),
    Parameter::value_or_reference("property3"),
    Parameter::value_or_reference("property4"),
    Parameter::value_or_reference("property5"),
];
const TRANSLATE_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("text"),
    Parameter::value_or_reference("source_language"),
    Parameter::value_or_reference("target_language"),
];
const PY_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("python_code"),
    Parameter::value_or_reference("return_type"),
];
const HAS_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("array"),
    Parameter::value_or_reference("value"),
];
const HASANY_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("array"),
    Parameter::value_or_reference("values"),
];
const HASALL_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("array"),
    Parameter::value_or_reference("values"),
];
const FLATTEN_ARGS: &[Parameter] = &[
    Parameter::value_or_reference("array"),
    Parameter::value_or_reference("pad_value"),
    Parameter::value_or_reference("levels"),
];
const LET_ARGS: &[Parameter] = &[
    Parameter::value("name"),
    Parameter::value("value"),
    Parameter::value("calculation"),
];
const ONE: &[Parameter] = &[Parameter::value_or_reference("operand")];

const fn entry(
    member: u16,
    spelling: &'static str,
    min: usize,
    max: usize,
    parameters: &'static [Parameter],
    implementation: Implementation,
) -> UnheldEntry {
    UnheldEntry {
        id: FunctionId::new(CODE, member),
        spelling,
        notation: Notation::Call,
        kind: UnheldKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::range(min, max),
        groups: None,
        parameters,
        result: ResultForm::Unknown,
        demand: ArgumentDemand::Eager,
        traits: EntryTraits::NONE,
        implementation,
    }
}
const fn binding(member: u16, spelling: &'static str, form: BindingForm) -> UnheldEntry {
    let mut result = entry(member, spelling, 1, 255, ONE, Implementation::Binding);
    result.kind = UnheldKind::Binding(form);
    match form {
        BindingForm::Let => {
            result.arity = CallableArityShape::range(3, 255);
            result.parameters = LET_ARGS;
        }
        BindingForm::Lambda => {
            result.arity = CallableArityShape::range(1, 254);
            result.parameters = ONE;
        }
    }
    result
}
const fn deferred_list(
    member: u16,
    spelling: &'static str,
    min: usize,
    max: usize,
    parameters: &'static [Parameter],
) -> UnheldEntry {
    let mut result = entry(
        member,
        spelling,
        min,
        max,
        parameters,
        Implementation::Deferred {
            class: Deferral::ListsInCells,
        },
    );
    result.availability = Availability::ListsInCellsBeta;
    result
}

const fn macro_sheet_only(mut entry: UnheldEntry) -> UnheldEntry {
    entry.availability = Availability::MacroSheetOnly;
    entry
}

const fn rtd() -> UnheldEntry {
    let mut result = entry(
        3,
        "RTD",
        3,
        255,
        RTD_ARGS,
        Implementation::NotHeld {
            owner: "dc-snz.23.4",
        },
    );
    result.kind = UnheldKind::Rtd {
        provider: RTD_PROVIDER,
    };
    result
}
const fn reference_operator(
    member: u16,
    spelling: &'static str,
    notation: Notation,
    file_spelling: &'static str,
) -> UnheldEntry {
    let mut result = entry(member, spelling, 1, 255, ONE, Implementation::Binding);
    result.notation = notation;
    result.kind = UnheldKind::Operator {
        file_spelling: Some(file_spelling),
    };
    result.arity = CallableArityShape::exact(1);
    result.parameters = ONE;
    result.result = ResultForm::MayBeReference;
    result
}
pub const UNHELD_ENTRIES: &[UnheldEntry] = &[
    macro_sheet_only(entry(
        1,
        "CALL",
        1,
        255,
        CALL_ARGS,
        Implementation::NotHeld { owner: "dc-gh22" },
    )),
    macro_sheet_only(entry(
        2,
        "REGISTER.ID",
        2,
        3,
        REGISTER_ID_ARGS,
        Implementation::NotHeld { owner: "dc-gh22" },
    )),
    rtd(),
    entry(
        4,
        "COPILOT",
        1,
        255,
        COPILOT_ARGS,
        Implementation::Deferred {
            class: Deferral::ExternalService,
        },
    ),
    entry(
        5,
        "DETECTLANGUAGE",
        1,
        1,
        DETECTLANGUAGE_ARGS,
        Implementation::Deferred {
            class: Deferral::ExternalService,
        },
    ),
    entry(
        6,
        "STOCKHISTORY",
        2,
        11,
        STOCKHISTORY_ARGS,
        Implementation::Deferred {
            class: Deferral::ExternalService,
        },
    ),
    entry(
        7,
        "TRANSLATE",
        1,
        3,
        TRANSLATE_ARGS,
        Implementation::Deferred {
            class: Deferral::ExternalService,
        },
    ),
    entry(
        8,
        "PY",
        2,
        2,
        PY_ARGS,
        Implementation::Deferred {
            class: Deferral::Python,
        },
    ),
    binding(9, "LET", BindingForm::Let),
    binding(10, "LAMBDA", BindingForm::Lambda),
    deferred_list(11, "HAS", 2, 2, HAS_ARGS),
    deferred_list(12, "HASANY", 2, 2, HASANY_ARGS),
    deferred_list(13, "HASALL", 2, 2, HASALL_ARGS),
    deferred_list(14, "FLATTEN", 1, 3, FLATTEN_ARGS),
    reference_operator(15, "@", Notation::Prefix, "_xlfn.SINGLE"),
    reference_operator(16, "#", Notation::Postfix, "_xlfn.ANCHORARRAY"),
];

//! Names whose execution belongs to another package or is deferred by owner policy.
//! The service deferrals are dc-fetch-f0tp; the compatibility-version-3 list functions are
//! dc-nk93. CALL and REGISTER.ID await dc-h9u; RTD uses the dc-snz.23.4 host route.
use super::{
    ArgumentDemand, Availability, BindingForm, CatalogueEntry, Deferral, EntryKind, EntryTraits,
    Family, FunctionId, Kernel, Notation, Parameter, ResultForm,
};
use crate::value::CallableArityShape;

const CODE: u16 = 325;
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
    kernel: Kernel,
) -> CatalogueEntry {
    CatalogueEntry {
        id: FunctionId::new(CODE, member),
        spelling,
        notation: Notation::Call,
        kind: EntryKind::Worksheet,
        availability: Availability::Unspecified,
        arity: CallableArityShape::range(min, max),
        groups: None,
        parameters,
        result: ResultForm::Unknown,
        demand: ArgumentDemand::Eager,
        traits: EntryTraits::NONE,
        kernel,
    }
}
const fn binding(member: u16, spelling: &'static str, form: BindingForm) -> CatalogueEntry {
    let mut result = entry(member, spelling, 1, 255, ONE, Kernel::Binding);
    result.kind = EntryKind::Binding(form);
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
) -> CatalogueEntry {
    let mut result = entry(
        member,
        spelling,
        min,
        max,
        parameters,
        Kernel::Deferred {
            class: Deferral::ListsInCells,
        },
    );
    result.availability = Availability::ListsInCellsBeta;
    result
}

const fn rtd() -> CatalogueEntry {
    let mut result = entry(3, "RTD", 3, 255, RTD_ARGS, Kernel::NotHeld { owner: "dc-snz.23.4" });
    result.kind = EntryKind::HostRoute { provider: crate::RTD_PROVIDER, route: super::HostRouteKind::Rtd };
    result
}
const fn reference_operator(
    member: u16,
    spelling: &'static str,
    notation: Notation,
    file_spelling: &'static str,
) -> CatalogueEntry {
    let mut result = entry(member, spelling, 1, 255, ONE, Kernel::Binding);
    result.notation = notation;
    result.kind = EntryKind::Operator {
        file_spelling: Some(file_spelling),
    };
    result.arity = CallableArityShape::exact(1);
    result.parameters = ONE;
    result.result = ResultForm::MayBeReference;
    result
}
pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        entry(
            1,
            "CALL",
            1,
            255,
            CALL_ARGS,
            Kernel::NotHeld { owner: "dc-h9u" },
        ),
        entry(
            2,
            "REGISTER.ID",
            2,
            3,
            REGISTER_ID_ARGS,
            Kernel::NotHeld { owner: "dc-h9u" },
        ),
        rtd(),
        entry(
            4,
            "COPILOT",
            1,
            255,
            COPILOT_ARGS,
            Kernel::Deferred {
                class: Deferral::ExternalService,
            },
        ),
        entry(
            5,
            "DETECTLANGUAGE",
            1,
            1,
            DETECTLANGUAGE_ARGS,
            Kernel::Deferred {
                class: Deferral::ExternalService,
            },
        ),
        entry(
            6,
            "STOCKHISTORY",
            2,
            11,
            STOCKHISTORY_ARGS,
            Kernel::Deferred {
                class: Deferral::ExternalService,
            },
        ),
        entry(
            7,
            "TRANSLATE",
            1,
            3,
            TRANSLATE_ARGS,
            Kernel::Deferred {
                class: Deferral::ExternalService,
            },
        ),
        entry(
            8,
            "PY",
            2,
            2,
            PY_ARGS,
            Kernel::Deferred {
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
    ],
};

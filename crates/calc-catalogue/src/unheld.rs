//! Names whose execution belongs to another package or is deferred by owner policy.
//! The service deferrals are dc-fetch-f0tp; the compatibility-version-3 list functions are
//! dc-nk93. CALL and REGISTER.ID run only on a macro sheet in Excel and are refused at entry in a
//! worksheet formula (dc-gh22); RTD reads through the host's connector (dc-snz.23.4, CalcFml's provider function) and, where the
//! evaluation has none, through CalcFunc's kernel: no server can be started, Excel's #N/A.
use crate::{
    ArgumentDemand, Availability, BindingForm, Deferral, EntryTraits, FunctionId, Implementation,
    Notation, Parameter, ResultForm,
};
use calc_value::CallableArityShape;

/// File and entry metadata for a function or an operator without an ordinary kernel.
#[derive(Clone, Copy, Debug)]
pub struct UnheldEntry {
    /// `None` means translated names have not been recorded for this entry.
    pub localized_names: Option<&'static [crate::LocalizedDisplayName]>,
    /// `None` means the complete set of facilities has not been classified.
    pub context_facilities: Option<&'static [crate::ContextFacility]>,
    /// The entry's stable catalogue identity.
    pub id: FunctionId,
    /// Invariant spelling used in formulas.
    pub spelling: &'static str,
    /// Named call or operator notation.
    pub notation: Notation,
    /// Syntax or host route used for this entry.
    pub kind: UnheldKind,
    /// Observed build or compatibility availability.
    pub availability: Availability,
    /// Supported range of argument counts.
    pub arity: CallableArityShape,
    /// Repeating parameter groups, when declared.
    pub groups: Option<crate::ArgumentGroups>,
    /// Parameter declarations in call order.
    pub parameters: &'static [Parameter],
    /// Result form visible to formula admission.
    pub result: ResultForm,
    /// When arguments are evaluated.
    pub demand: ArgumentDemand,
    /// Additional evaluation and publication behavior.
    pub traits: EntryTraits,
    /// What owns execution or the reason it is deferred.
    pub implementation: Implementation,
}

#[derive(Clone, Copy, Debug)]
/// Forms represented by a declaration without an ordinary worksheet kernel.
pub enum UnheldKind {
    /// A named worksheet call.
    Worksheet,
    /// A special form that introduces names during binding.
    Binding(BindingForm),
    /// An operator that uses a formula-language route.
    Operator {
        /// Named spelling used when the file stores this operator as a call.
        file_spelling: Option<&'static str>,
    },
    /// A real-time data call routed to a host provider.
    Rtd {
        /// Key used to resolve the provider.
        provider: &'static str,
    },
}

/// The binding form (LET or LAMBDA) a call written `name` introduces: `name` is an invariant
/// spelling without a file prefix, matched in any ASCII case.
pub fn binding_form(name: &str) -> Option<BindingForm> {
    UNHELD_ENTRIES.iter().find_map(|entry| match entry.kind {
        UnheldKind::Binding(form) if entry.spelling.eq_ignore_ascii_case(name) => Some(form),
        _ => None,
    })
}

/// The invariant spelling of a binding form's declaration (`LET`, `LAMBDA`).
pub const fn binding_spelling(form: BindingForm) -> &'static str {
    let mut i = 0;
    while i < UNHELD_ENTRIES.len() {
        if let UnheldKind::Binding(declared) = UNHELD_ENTRIES[i].kind
            && declared as u8 == form as u8
        {
            return UNHELD_ENTRIES[i].spelling;
        }
        i += 1;
    }
    panic!("a binding form without a declaration")
}

/// Provider identity for the RTD route.
pub const RTD_PROVIDER: &str = "calc-fml/rtd";

/// Stable family code for the entries without an ordinary kernel.
pub const CODE: u16 = 325;

/// Stable IDs of the two reference operators with function-shaped file forms.
pub const IMPLICIT_INTERSECTION_ID: FunctionId = FunctionId::new(CODE, 15);
/// Stable ID for the postfix spill-reference operator.
pub const SPILL_ID: FunctionId = FunctionId::new(CODE, 16);
/// Stable ID of RTD.
pub const RTD_ID: FunctionId = FunctionId::new(CODE, 3);
/// Stable ID for `DETECTLANGUAGE`.
pub const DETECTLANGUAGE_ID: FunctionId = FunctionId::new(CODE, 5);
/// Stable ID for `STOCKHISTORY`.
pub const STOCKHISTORY_ID: FunctionId = FunctionId::new(CODE, 6);
/// Stable ID for `TRANSLATE`.
pub const TRANSLATE_ID: FunctionId = FunctionId::new(CODE, 7);

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
/// The trim reference operators: each operator, the name a file stores its call with (after
/// `_xlfn.`; Excel 16.0.20527 saves `C1.:C20` as `_xlfn._TRO_LEADING(C1:C20)`, `C1:.C20` as
/// `_xlfn._TRO_TRAILING(C1:C20)` and `C1.:.C20` as `_xlfn._TRO_ALL(C1:C20)`, dc-h9u), and the
/// trim argument TRIMRANGE ([`crate::offset::TRIMRANGE`]) takes for both rows and columns.
const TRIM_REFERENCES: [(&str, &str, u8); 3] = [
    (".:", "_TRO_LEADING", 1),
    (":.", "_TRO_TRAILING", 2),
    (".:.", "_TRO_ALL", 3),
];

/// The file prefix a trim reference's file call takes.
pub const TRIM_REFERENCE_FILE_PREFIX: crate::FilePrefix = crate::FilePrefix::Xlfn;

/// File form of a trim reference operator measured in the workbook format.
pub fn trim_reference_file_spelling(operator: &str) -> Option<(crate::FilePrefix, &'static str)> {
    TRIM_REFERENCES
        .iter()
        .find(|(written, _, _)| *written == operator)
        .map(|(_, name, _)| (TRIM_REFERENCE_FILE_PREFIX, *name))
}

/// TRIMRANGE's trim argument for a trim reference operator (`.:`, `:.` or `.:.`).
pub fn trim_reference_mode(operator: &str) -> Option<u8> {
    TRIM_REFERENCES
        .iter()
        .find(|(written, _, _)| *written == operator)
        .map(|(_, _, mode)| *mode)
}

/// TRIMRANGE's trim argument for a trim reference's file call name (`_TRO_LEADING`, without its
/// prefix, in any ASCII case).
pub fn trim_reference_file_mode(name: &str) -> Option<u8> {
    TRIM_REFERENCES
        .iter()
        .find(|(_, file, _)| file.eq_ignore_ascii_case(name))
        .map(|(_, _, mode)| *mode)
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
        localized_names: None,
        context_facilities: None,
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
        Implementation::Held,
    );
    result.kind = UnheldKind::Rtd {
        provider: RTD_PROVIDER,
    };
    // A topic's value: RTD never gives a reference or a LAMBDA.
    result.result = ResultForm::Value;
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
/// Declarations for entries whose execution is deferred or supplied by a special route.
pub const UNHELD_ENTRIES: &[UnheldEntry] = &[
    macro_sheet_only(entry(
        1,
        "CALL",
        1,
        255,
        CALL_ARGS,
        Implementation::NotHeld { owner: "dc-snz.2" },
    )),
    macro_sheet_only(entry(
        2,
        "REGISTER.ID",
        2,
        3,
        REGISTER_ID_ARGS,
        Implementation::NotHeld { owner: "dc-snz.2" },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn binding_forms_are_read_from_their_declarations() {
        assert_eq!(binding_form("LET"), Some(BindingForm::Let));
        assert_eq!(binding_form("lambda"), Some(BindingForm::Lambda));
        assert_eq!(binding_form("_xlfn.LET"), None);
        assert_eq!(binding_form("SUM"), None);
        assert_eq!(binding_spelling(BindingForm::Let), "LET");
        assert_eq!(binding_spelling(BindingForm::Lambda), "LAMBDA");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trim_references_have_one_file_name_and_mode() {
        for (operator, name, mode) in [
            (".:", "_TRO_LEADING", 1),
            (":.", "_TRO_TRAILING", 2),
            (".:.", "_TRO_ALL", 3),
        ] {
            assert_eq!(
                trim_reference_file_spelling(operator),
                Some((crate::FilePrefix::Xlfn, name))
            );
            assert_eq!(trim_reference_mode(operator), Some(mode));
            assert_eq!(trim_reference_file_mode(&name.to_ascii_lowercase()), Some(mode));
        }
        assert_eq!(trim_reference_mode(":"), None);
        assert_eq!(trim_reference_file_mode("TRIMRANGE"), None);
    }
}

//! Text for people: formulas, values, cells, areas and calc-fml's refusals in both apps.
//!
//! Value and location labels are never parsed back. Formula display keeps syntax roles and its
//! callers retain the original stored text until a real edit. A number uses the shortest decimal
//! that round-trips its binary64, not in Excel's General format: number formats are calc-fml's
//! (bead dc-m3u) and reach OneCalc through bead onecalc-m2.1. A cell is labelled `Sheet2!B3`,
//! with the sheet name quoted when reference text needs it. The names come through
//! [`SheetNames`]: DNA OneCalc's session and DNA Calc's workbook each implement it.

use calc_fml::reference::BindError;
use calc_fml::syntax::TextSpan;
use calc_fml::{CompileError, EvalFault};
use calc_func::value::{CalcValue, CoreValue, WorksheetErrorCode};
use calc_func::{FunctionFault, FunctionId, Notation, catalogue};

/// Recognized function calls use their entry spelling; stored text is unchanged.
pub use calc_fml::assist::display_formula as formula;

/// A value with its type: `number 7`, `text "ax"`, `logical TRUE`, `error #N/A`, `blank`.
pub fn value(value: &CalcValue) -> String {
    match value.core() {
        CoreValue::Number(number) => format!("number {number}"),
        CoreValue::Text(text) => {
            format!("text \"{}\"", text.to_string_lossy().replace('"', "\"\""))
        }
        CoreValue::Logical(true) => "logical TRUE".to_owned(),
        CoreValue::Logical(false) => "logical FALSE".to_owned(),
        CoreValue::Error(code) => format!("error {}", error_spelling(*code)),
        CoreValue::Empty => "blank".to_owned(),
        CoreValue::Missing => "omitted argument".to_owned(),
        CoreValue::Array(array) => {
            let shape = array.shape();
            format!("array of {} rows by {} columns", shape.rows, shape.cols)
        }
        // calc-fml's evaluator gives a reference result the value of its one cell, and no input
        // holds a reference.
        CoreValue::Reference(_) => "reference".to_owned(),
    }
}

/// The spelling Excel gives a worksheet error.
pub fn error_spelling(code: WorksheetErrorCode) -> &'static str {
    code.excel_spelling()
}

pub use dna_calc_model::address::{SheetNames, address, area, cell, column_letters, parse_address};

/// A span of the formula text (UTF-16 code units): `3..7`.
pub fn span(span: TextSpan) -> String {
    format!("{}..{}", span.start, span.end())
}

/// User-function identities fold for lookup; their diagnostic label uses conventional
/// ASCII uppercase spelling. Keep the runtime's explanation byte-for-byte after the separator.
pub fn user_function_diagnostic(message: &str) -> String {
    match message.split_once(": ") {
        Some((name, detail)) => format!("{}: {detail}", name.to_ascii_uppercase()),
        None => message.to_owned(),
    }
}

#[cfg(test)]
mod user_function_diagnostic_tests {
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn folded_names_get_display_spelling_without_changing_runtime_detail() {
        for name in ["spin", "SPIN", "SpIn"] {
            assert_eq!(
                super::user_function_diagnostic(&format!(
                    "{name}: execution budget exhausted: MiXeD detail"
                )),
                "SPIN: execution budget exhausted: MiXeD detail"
            );
        }
        assert_eq!(
            super::user_function_diagnostic("module.work: mixed Case"),
            "MODULE.WORK: mixed Case"
        );
        assert_eq!(
            super::user_function_diagnostic("No function separator"),
            "No function separator"
        );
    }
}

/// A compile refusal, in words.
pub fn compile_owner(error: &CompileError) -> Option<&str> {
    error.owner()
}

/// A work package carried by the typed fault; never inferred from display text.
pub fn eval_owner(fault: &EvalFault) -> Option<&str> {
    match fault {
        EvalFault::Function(FunctionFault::NotHeld { owner, .. }) => Some(owner),
        EvalFault::Function(FunctionFault::Deferred { class, .. }) => Some(class.owner()),
        EvalFault::NotModelled { owner, .. } => Some(owner),
        _ => None,
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn only_typed_ownership_is_reported() {
        assert_eq!(
            eval_owner(&EvalFault::NotModelled {
                construct: "example",
                owner: "dc-example"
            }),
            Some("dc-example")
        );
        assert_eq!(
            eval_owner(&EvalFault::NearZeroFinalSum { result: 1.0 }),
            None
        );
        assert_eq!(
            compile_owner(&CompileError::NotModelled {
                span: TextSpan::new(0, 1),
                gap: calc_fml::Gap::Construct {
                    construct: "example",
                    owner: "dc-example"
                }
            }),
            Some("dc-example")
        );
        assert_eq!(
            compile_owner(&CompileError::NotModelled {
                span: TextSpan::new(0, 1),
                gap: calc_fml::Gap::NumberLiteral
            }),
            None
        );
    }
}

pub fn compile_error(error: &CompileError) -> String {
    match error {
        CompileError::Rejected {
            span: at,
            rule: calc_fml::Rejection::Grammar(calc_fml::GrammarRule::Syntax),
        } => {
            format!("syntax at {}: the formula does not parse", span(*at))
        }
        CompileError::Rejected { span: at, rule } => {
            format!("Excel refuses this formula at {}: {rule}", span(*at))
        }
        CompileError::Reference { span: at, error } => {
            format!("reference at {} refused: {}", span(*at), bind_error(error))
        }
        CompileError::NotModelled {
            span: at,
            gap: calc_fml::Gap::Construct { construct, owner },
        } => format!(
            "not compiled yet at {}: {construct} (bead {owner})",
            span(*at)
        ),
        CompileError::NotModelled {
            span: at,
            gap: calc_fml::Gap::UnknownFunction { spelling },
        } => {
            format!("unknown function at {}: {spelling}", span(*at))
        }
        CompileError::NotModelled {
            span: at,
            gap: calc_fml::Gap::NumberLiteral,
        } => format!(
            "number at {}: its Excel value is not modelled (bead dc-10y)",
            span(*at)
        ),
    }
}

/// Why calc-fml's binder refused a reference.
pub fn bind_error(error: &BindError) -> String {
    match error {
        BindError::OutsideGrid => "it lies outside the grid".to_owned(),
        BindError::UnknownSheet { name } => format!("there is no sheet named {name}"),
        BindError::ExternalWorkbook => {
            "a reference to another workbook is not modelled (bead dc-10y)".to_owned()
        }
        BindError::SheetSpan => {
            "a run of sheets is not bound without the workbook's sheet order (bead dc-10y)"
                .to_owned()
        }
        BindError::UnknownTable { name } => format!("there is no table named {name}"),
        BindError::UnknownColumn { name } => format!("the table has no column named {name}"),
        BindError::TablesNotModelled => {
            "a structured reference needs the workbook's tables (bead dc-10y)".to_owned()
        }
        BindError::StructuredReference => {
            "this combination of table sections is not bound (bead dc-10y)".to_owned()
        }
        BindError::QualifiedNonReference => {
            "a sheet qualifier stands before something that is not a reference".to_owned()
        }
        BindError::LoneRowOrColumn => "a lone row or column with $ is no A1 reference".to_owned(),
        BindError::InArrayConstant => "an array constant holds constants only".to_owned(),
        BindError::UnsupportedSyntax => "the parser does not support this syntax".to_owned(),
    }
}

/// An evaluation refusal, in words, with the cells and areas it names labelled by `names`.
pub fn eval_fault(names: &(impl SheetNames + ?Sized), fault: &EvalFault) -> String {
    match fault {
        EvalFault::Function(fault) => function_fault(fault),
        EvalFault::MissingInput { cell: at } => {
            format!("missing input: {} has no input", cell(names, *at))
        }
        EvalFault::ReadContract(error) => format!(
            "reader fault: its runs for {} covered {} cells",
            area(names, error.area),
            error.covered
        ),
        EvalFault::AreaAsValue { area: at } => format!(
            "range as value: {} is more than one cell where a legacy formula takes one value at a place not modelled (bead dc-10y)",
            area(names, *at)
        ),
        EvalFault::NotModelled { construct, owner } => {
            format!("not modelled: {construct} (work package {owner})")
        }
        EvalFault::NearZeroFinalSum { result } => format!(
            "near-zero sum: the final + or - gave {result}, a result Excel adjusts by a rule not modelled yet (bead dc-6qki)"
        ),
    }
}

/// How a catalogue entry is written: `SUM`, or `the ^ operator`.
fn function_label(id: FunctionId) -> String {
    match catalogue().resolve(id) {
        Ok(target) => match target.notation() {
            Notation::Call => target.spelling().to_owned(),
            Notation::Prefix => format!("the prefix {} operator", target.spelling()),
            Notation::Infix => format!("the {} operator", target.spelling()),
            Notation::Postfix => format!("the postfix {} operator", target.spelling()),
        },
        Err(_) => format!("function {id:?}"),
    }
}

fn function_fault(fault: &FunctionFault) -> String {
    match fault {
        FunctionFault::NotHeld { id, owner } => format!(
            "function fault in {}: not modelled (work package {owner})",
            function_label(*id)
        ),
        FunctionFault::Deferred { id, class } => format!(
            "function fault in {}: deferred (work package {})",
            function_label(*id), class.owner()
        ),
        FunctionFault::Binding { id } => format!(
            "function fault in {}: this form requires formula evaluation",
            function_label(*id)
        ),
        FunctionFault::ContinuationTarget { expected, received } => format!(
            "function fault: a pending {} call cannot continue as {}",
            function_label(*expected),
            function_label(*received)
        ),
        FunctionFault::ArityMismatch { id, supplied } => format!(
            "function fault in {}: {supplied} arguments, which it does not admit",
            function_label(*id)
        ),
        FunctionFault::ReferenceArgument { id, index } => format!(
            "function fault in {}: argument {} is a reference where a value is needed",
            function_label(*id),
            index + 1
        ),
        FunctionFault::ReferenceRequired { id, index } => format!(
            "function fault in {}: argument {} must be a reference",
            function_label(*id),
            index + 1
        ),
        FunctionFault::ReferenceUnreadable { id, index } => format!(
            "function fault in {}: the reference of argument {} could not be read",
            function_label(*id),
            index + 1
        ),
        FunctionFault::NotImplemented { id, argument } => match argument {
            Some(index) => format!(
                "function fault in {}: Excel's result is not modelled for argument {}",
                function_label(*id),
                index + 1
            ),
            None => format!(
                "function fault in {}: Excel's result is not modelled for this call",
                function_label(*id)
            ),
        },
        FunctionFault::ExactUnavailable { id } => format!(
            "function fault in {}: no Exact implementation",
            function_label(*id)
        ),
        FunctionFault::MissingReturned { id } => format!(
            "function fault in {}: it returned an omitted argument",
            function_label(*id)
        ),
    }
}

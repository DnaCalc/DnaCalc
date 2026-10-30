//! Text for people: values, cells, areas and calc-fml's refusals as both apps show them.
//!
//! These are inspection labels, never parsed back. A number is shown as the shortest decimal
//! that round-trips its binary64, not in Excel's General format: number formats are calc-fml's
//! (bead dc-m3u) and reach OneCalc through bead onecalc-m2.1. A cell is labelled `Sheet2!B3`,
//! with the sheet name quoted when reference text needs it. The names come through
//! [`SheetNames`]: DNA OneCalc's session and DNA Calc's workbook each implement it.

use calc_fml::reference::BindError;
use calc_fml::syntax::TextSpan;
use calc_fml::{CompileError, EvalFault};
use calc_func::value::{CalcValue, CellKey, CoreValue, GridArea, SheetId, WorksheetErrorCode};
use calc_func::{FunctionFault, FunctionId, Notation, catalogue};

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
    match code {
        WorksheetErrorCode::Null => "#NULL!",
        WorksheetErrorCode::Div0 => "#DIV/0!",
        WorksheetErrorCode::Value => "#VALUE!",
        WorksheetErrorCode::Ref => "#REF!",
        WorksheetErrorCode::Name => "#NAME?",
        WorksheetErrorCode::Num => "#NUM!",
        WorksheetErrorCode::NA => "#N/A",
        WorksheetErrorCode::Busy => "#BUSY!",
        WorksheetErrorCode::GettingData => "#GETTING_DATA",
        WorksheetErrorCode::Spill => "#SPILL!",
        WorksheetErrorCode::Calc => "#CALC!",
        WorksheetErrorCode::Field => "#FIELD!",
        WorksheetErrorCode::Blocked => "#BLOCKED!",
        WorksheetErrorCode::Connect => "#CONNECT!",
    }
}

/// A column's letters: 1 is `A`, 26 `Z`, 27 `AA`.
pub fn column_letters(mut column: u32) -> String {
    let mut letters = Vec::new();
    while column > 0 {
        let digit = (column - 1) % 26;
        letters.push(char::from(b'A' + digit as u8));
        column = (column - 1) / 26;
    }
    letters.iter().rev().collect()
}

/// One A1 cell address, optionally anchored with `$`, inside Excel's grid.
pub fn parse_address(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    let text = text.strip_prefix('$').unwrap_or(text);
    let split = text.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, rest) = text.split_at(split);
    let digits = rest.strip_prefix('$').unwrap_or(rest);
    if letters.is_empty()
        || letters.len() > 3
        || digits.is_empty()
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let column = letters.bytes().fold(0u32, |n, b| {
        n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
    });
    let row: u32 = digits.parse().ok()?;
    ((1..=1_048_576).contains(&row) && (1..=16_384).contains(&column)).then_some((row, column))
}

/// What names the sheets a label shows: a DNA OneCalc session names its own sheets, and DNA Calc
/// names a workbook's (dc-rif), so both label cells, areas and refusals with these functions.
pub trait SheetNames {
    /// The name of `sheet`, or `None` when there is no such sheet.
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String>;
}

fn sheet_label(names: &(impl SheetNames + ?Sized), cell: CellKey) -> String {
    names
        .sheet_name_of(cell.sheet())
        .map_or_else(|| "(no sheet)".to_owned(), |name| quoted_sheet(&name))
}

/// A sheet name as reference text: as it is when it is a plain name (a letter or `_`, then
/// letters, digits, `_` and `.`) that cannot be read as a cell (`A1`, `R1C1`, `R`, `C`), and
/// otherwise in single quotes with each quote doubled (`'Q2 Data'`, `'It''s'`). Quoting a name
/// that does not need it is still valid reference text, so the test errs toward quoting.
fn quoted_sheet(name: &str) -> String {
    let mut chars = name.chars();
    let plain = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    let upper = name.to_ascii_uppercase();
    // A cell of the grid: one to three column letters up to XFD, then a row from 1 to 1048576.
    let cell_like = |text: &str| {
        let letters = text.trim_end_matches(|c: char| c.is_ascii_digit());
        let digits = &text[letters.len()..];
        // At most three letters are folded, so the column number cannot overflow.
        let column = (1..=3)
            .contains(&letters.len())
            .then(|| {
                letters.bytes().try_fold(0u32, |n, b| {
                    b.is_ascii_uppercase()
                        .then(|| n * 26 + u32::from(b - b'A' + 1))
                })
            })
            .flatten()
            .filter(|&n| (1..=16_384).contains(&n));
        let row = digits
            .parse::<u32>()
            .ok()
            .filter(|r| (1..=1_048_576).contains(r));
        column.is_some() && row.is_some()
    };
    let r1c1_like = upper == "R"
        || upper == "C"
        || upper
            .strip_prefix('R')
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || c == 'C'));
    if plain && !cell_like(&upper) && !r1c1_like {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// A cell's label: `Sheet2!B3`, or `'Q2 Data'!B3` for a sheet name that needs quotes.
pub fn cell(names: &(impl SheetNames + ?Sized), cell: CellKey) -> String {
    format!(
        "{}!{}{}",
        sheet_label(names, cell),
        column_letters(cell.column()),
        cell.row()
    )
}

/// An area's label: `Sheet1!A1:A3`, or a cell's label for one cell.
pub fn area(names: &(impl SheetNames + ?Sized), area: GridArea) -> String {
    let first = area.first();
    let last = area.last();
    if first == last {
        return cell(names, first);
    }
    format!(
        "{}:{}{}",
        cell(names, first),
        column_letters(last.column()),
        last.row()
    )
}

/// A span of the formula text (UTF-16 code units): `3..7`.
pub fn span(span: TextSpan) -> String {
    format!("{}..{}", span.start, span.end())
}

/// A compile refusal, in words.
pub fn compile_error(error: &CompileError) -> String {
    match error {
        CompileError::Syntax { span: at } => {
            format!("syntax at {}: the formula does not parse", span(*at))
        }
        CompileError::Reference { span: at, error } => {
            format!("reference at {} refused: {}", span(*at), bind_error(error))
        }
        CompileError::Unsupported {
            span: at,
            construct,
            owner,
        } => format!(
            "not compiled yet at {}: {construct} (bead {owner})",
            span(*at)
        ),
        CompileError::UnknownFunction { span: at, spelling } => {
            format!("unknown function at {}: {spelling}", span(*at))
        }
        CompileError::Arity { span: at, supplied } => format!(
            "arity at {}: {supplied} arguments, which the function does not admit",
            span(*at)
        ),
        CompileError::NumberLiteral { span: at } => format!(
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

#[cfg(test)]
mod tests {
    use super::quoted_sheet;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sheet_names_that_need_quotes_are_quoted() {
        for (name, text) in [
            ("Sheet1", "Sheet1"),
            ("Data_2024", "Data_2024"),
            ("Totals1", "Totals1"),
            ("Summary", "Summary"),
            ("Revenue2024", "Revenue2024"),
            ("Summary.1", "Summary.1"),
            (
                "SSSSSSSSSSSSSSSSSSSSSSSSSSSSSSS",
                "SSSSSSSSSSSSSSSSSSSSSSSSSSSSSSS",
            ),
            ("XFD1048576", "'XFD1048576'"),
            ("XFE1", "XFE1"),
            ("A1048577", "A1048577"),
            ("my.sheet", "my.sheet"),
            ("Q2 Data", "'Q2 Data'"),
            ("It's", "'It''s'"),
            ("2024", "'2024'"),
            ("A1", "'A1'"),
            ("xfd1048576", "'xfd1048576'"),
            ("R1C1", "'R1C1'"),
            ("R", "'R'"),
            ("c", "'c'"),
            ("Résumé", "'Résumé'"),
            ("a-b", "'a-b'"),
        ] {
            assert_eq!(quoted_sheet(name), text, "{name}");
        }
    }
}

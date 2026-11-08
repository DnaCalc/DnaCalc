//! Parts of the formula evaluated on their own: F9 on a selected part, and the evaluation trace
//! (the port of DnaOneCalc's X-Ray, dc-wr9).
//!
//! Both ask the session's one evaluator, [`Session::calculate_text`], for the value of a part of the
//! formula text over the same inputs, form, environment and functions as the whole formula. A
//! part inside LET is evaluated with the bindings the LET declares before it (the evaluated text
//! is `LET(name1,value1,...,part)`); a part that uses a LAMBDA parameter has no value until the
//! LAMBDA is called, and says so. The trace does not instrument the evaluator: each step is an
//! evaluation of that part again, so a volatile part (RAND) can differ between steps, and a part
//! an IF skips is still evaluated for its step.

use crate::display;
use crate::result;
use crate::session::Session;
use crate::shown::{Authored, prepare};
use calc_fml::assist::{LocalBinding, local_bindings_at};
use calc_fml::parse_formula;
use calc_fml::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree, TokenKind, lex};
use serde::{Deserialize, Serialize};

/// The most steps a trace lists; a longer formula says so.
pub const MAX_TRACE_STEPS: usize = 120;

/// The value of a selected part: the text that replaces it, and the value as the surface shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartValue {
    /// Formula text that gives the value back (`3`, `"ab"`, `{1,2;3,4}`).
    pub literal: String,
    /// The value with its type (`number 3`).
    pub shown: String,
}

/// One step of an evaluation trace: a part of the formula and its value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceStep {
    /// The part's span in the formula text, in UTF-16 code units.
    pub start: usize,
    pub len: usize,
    /// How deep the part sits in the formula's tree (the whole formula is 0).
    pub depth: usize,
    pub text: String,
    /// The value's type (`number`, `text`, ...), `error` for a worksheet error, or `none` when the
    /// part has no value of its own.
    pub kind: String,
    /// The value as a cell shows it, or why there is none.
    pub value: String,
}

/// The steps of a formula's evaluation, innermost parts first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub steps: Vec<TraceStep>,
    /// What the trace is and is not, shown beside it.
    pub note: String,
}

fn utf16_slice(units: &[u16], start: usize, len: usize) -> Option<String> {
    String::from_utf16(units.get(start..start.checked_add(len)?)?).ok()
}

/// The text to evaluate for the part `text` that starts at `start` in `formula`: the part itself,
/// or the part inside the LET bindings in scope there. A part that uses a LAMBDA parameter is
/// refused.
fn evaluable(formula: &str, tree: &SyntaxTree, start: usize, text: &str) -> Result<String, String> {
    let bindings: Vec<LocalBinding> = local_bindings_at(tree, formula, start);
    if bindings.is_empty() {
        return Ok(format!("={text}"));
    }
    let used: Vec<String> = lex(&format!("={text}"))
        .iter()
        .filter(|token| token.kind() == TokenKind::Identifier)
        .map(|token| token.text().to_ascii_lowercase())
        .collect();
    // The last binding of a name is the one in scope for the part.
    let in_scope = |name: &str| bindings.iter().rev().find(|b| b.name.eq_ignore_ascii_case(name));
    for name in &used {
        if let Some(binding) = in_scope(name)
            && binding.value.is_none()
        {
            return Err(format!(
                "{} is a LAMBDA parameter: it has a value only when the LAMBDA is called",
                binding.name
            ));
        }
    }
    if !bindings
        .iter()
        .any(|binding| binding.value.is_some() && used.contains(&binding.name.to_ascii_lowercase()))
    {
        return Ok(format!("={text}"));
    }
    let mut wrapped = String::from("=LET(");
    for binding in &bindings {
        if let Some(value) = &binding.value {
            wrapped.push_str(&format!("{},{},", binding.name, value));
        }
    }
    wrapped.push_str(text);
    wrapped.push(')');
    Ok(wrapped)
}

/// The value of the part of `authored`'s formula at `start`..`start + len` (UTF-16 code units).
pub fn evaluate_part(
    session: &mut Session,
    authored: &Authored,
    start: usize,
    len: usize,
) -> Result<PartValue, String> {
    prepare(session, authored);
    let units: Vec<u16> = authored.formula.encode_utf16().collect();
    let tree = parse_formula(&authored.formula);
    let part = utf16_slice(&units, start, len).ok_or("The selection is outside the formula")?;
    let mut text = part.trim();
    if start == 0 {
        text = text.strip_prefix('=').unwrap_or(text).trim();
    }
    if text.is_empty() {
        return Err("Select the part of the formula to evaluate".to_owned());
    }
    // The selection's own start, past any leading space.
    let lead = part.encode_utf16().count() - part.trim_start().encode_utf16().count();
    let to_evaluate = evaluable(&authored.formula, &tree, start + lead, text)?;
    value_of(session, &to_evaluate)
}

fn value_of(session: &mut Session, text: &str) -> Result<PartValue, String> {
    let calculation = session.calculate_text(text);
    match calculation.result {
        Some(value) => Ok(PartValue {
            literal: display::formula_literal(&value)?,
            shown: display::value(&value),
        }),
        None => Err(calculation
            .diagnostics
            .first()
            .map_or_else(|| "no value".to_owned(), |d| display::diagnostic(session, d))),
    }
}

/// Whether a node is a part worth a step of its own. A number, text or error constant is its own
/// value, so it has no step unless it is the whole formula.
fn steps_for(kind: SyntaxKind) -> bool {
    !matches!(
        kind,
        SyntaxKind::FormulaRoot
            | SyntaxKind::ArgumentList
            | SyntaxKind::NumberLiteralExpr
            | SyntaxKind::StringLiteralExpr
            | SyntaxKind::ErrorLiteralExpr
            | SyntaxKind::OmittedArgExpr
            | SyntaxKind::MissingExpr
            | SyntaxKind::UnsupportedExpr
    )
}

/// The parts a node's value is built from, in order: for a call, its arguments, except the names
/// a LET or LAMBDA declares.
fn operands(node: &SyntaxNode) -> Vec<&SyntaxNode> {
    match node.kind() {
        // The text of a name, a qualified or a 3-D reference, and an array constant is one value.
        SyntaxKind::QualifiedReferenceExpr
        | SyntaxKind::SheetSpan3DReferenceExpr
        | SyntaxKind::ArrayLiteralExpr
        | SyntaxKind::IdentifierExpr
        | SyntaxKind::QuotedIdentifierExpr => Vec::new(),
        SyntaxKind::CallExpr => {
            let declares = node.children().first().is_some_and(|child| match child {
                SyntaxChild::Token(callee) => {
                    let name = calc_fml::assist::unprefixed(callee.text());
                    name.eq_ignore_ascii_case("LET") || name.eq_ignore_ascii_case("LAMBDA")
                }
                SyntaxChild::Node(_) => false,
            });
            let is_let = node.children().first().is_some_and(|child| match child {
                SyntaxChild::Token(callee) => {
                    calc_fml::assist::unprefixed(callee.text()).eq_ignore_ascii_case("LET")
                }
                SyntaxChild::Node(_) => false,
            });
            let Some(arguments) = node
                .child_nodes()
                .find(|child| child.kind() == SyntaxKind::ArgumentList)
            else {
                return Vec::new();
            };
            let total = 1 + arguments
                .children()
                .iter()
                .filter(|child| matches!(child, SyntaxChild::Token(t) if t.kind() == TokenKind::Comma))
                .count();
            let mut index = 0;
            let mut found = Vec::new();
            for child in arguments.children() {
                match child {
                    SyntaxChild::Token(token) if token.kind() == TokenKind::Comma => index += 1,
                    SyntaxChild::Node(argument) => {
                        let name_slot = declares && index + 1 < total && (!is_let || index % 2 == 0);
                        if !name_slot {
                            found.push(argument);
                        }
                    }
                    SyntaxChild::Token(_) => {}
                }
            }
            found
        }
        // `A1:B2`, `Sheet2!A1:B2`: the operands are references, not parts with values.
        SyntaxKind::RangeExpr
            if node.child_nodes().all(|child| {
                matches!(
                    child.kind(),
                    SyntaxKind::IdentifierExpr
                        | SyntaxKind::QuotedIdentifierExpr
                        | SyntaxKind::QualifiedReferenceExpr
                )
            }) =>
        {
            Vec::new()
        }
        _ => node.child_nodes().collect(),
    }
}

/// The evaluation trace of `authored`'s formula: every part with its value, innermost first.
pub fn trace(session: &mut Session, authored: &Authored) -> Trace {
    prepare(session, authored);
    let formula = &authored.formula;
    let tree = parse_formula(formula);
    let mut result = Trace {
        steps: Vec::new(),
        note: "Each step evaluates that part of the formula again over the same inputs. RAND, NOW and \
               other volatile parts can differ between steps, and a part that IF or CHOOSE would skip \
               still gets its step."
            .to_owned(),
    };
    if !tree.diagnostics().is_empty() {
        result.note = "The formula has a syntax error; there is nothing to trace.".to_owned();
        return result;
    }
    let Some(root) = tree.expression() else {
        return result;
    };
    let units: Vec<u16> = formula.encode_utf16().collect();
    // Post-order with an explicit stack: a node's step follows its operands' steps.
    let mut order: Vec<(&SyntaxNode, usize)> = Vec::new();
    let mut stack: Vec<(&SyntaxNode, usize, bool)> = vec![(root, 0, false)];
    while let Some((node, depth, done)) = stack.pop() {
        if done {
            order.push((node, depth));
            continue;
        }
        stack.push((node, depth, true));
        for operand in operands(node).into_iter().rev() {
            stack.push((operand, depth + 1, false));
        }
    }
    let only_a_constant = root.kind() != SyntaxKind::GroupingExpr && !steps_for(root.kind());
    let mut total = 0usize;
    for (node, depth) in order {
        if !steps_for(node.kind()) && !(only_a_constant && depth == 0) {
            continue;
        }
        total += 1;
        if result.steps.len() >= MAX_TRACE_STEPS {
            continue;
        }
        let span = node.span();
        let Some(text) = utf16_slice(&units, span.start, span.len) else {
            continue;
        };
        let text = text.trim().to_owned();
        let step = match evaluable(formula, &tree, span.start, &text)
            .and_then(|to_evaluate| value_for_step(session, &to_evaluate))
        {
            Ok((kind, value)) => (kind, value),
            Err(reason) => ("none".to_owned(), reason),
        };
        result.steps.push(TraceStep {
            start: span.start,
            len: span.len,
            depth,
            text,
            kind: step.0,
            value: step.1,
        });
    }
    if total > result.steps.len() {
        result.note.push_str(&format!(
            " The formula has {total} parts; the first {} are listed.",
            result.steps.len()
        ));
    }
    result
}

fn value_for_step(session: &mut Session, text: &str) -> Result<(String, String), String> {
    let calculation = session.calculate_text(text);
    let profile = session.environment().and_then(|environment| environment.profile);
    match calculation.result {
        Some(value) => {
            let kind = match value.core() {
                calc_func::value::CoreValue::Error(_) => "error",
                _ => result::kind(&value),
            };
            let shown = match display::formula_literal(&value) {
                Ok(literal) if matches!(value.core(), calc_func::value::CoreValue::Array(_)) => literal,
                _ => result::shown(&value, profile.as_ref()),
            };
            Ok((kind.to_owned(), shown))
        }
        None => Err(calculation
            .diagnostics
            .first()
            .map_or_else(|| "no value".to_owned(), |d| display::diagnostic(session, d))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid;

    fn authored(formula: &str, cells: &[(usize, &str)]) -> Authored {
        let mut state = Authored {
            formula: formula.to_owned(),
            ..Authored::default()
        };
        for (index, text) in cells {
            state.grid[*index] = (*text).to_owned();
        }
        state
    }

    fn span_of(formula: &str, part: &str) -> (usize, usize) {
        let at = formula.find(part).expect("the part is in the formula");
        let start = formula[..at].encode_utf16().count();
        (start, part.encode_utf16().count())
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_selected_part_gives_its_value_as_formula_text() {
        let mut session = Session::new("Sheet1").unwrap();
        let state = authored("=SUM(A1:A2)*2+LEN(\"ab\")", &[(0, "1.5"), (grid::COLUMNS as usize, "2")]);
        let (start, len) = span_of(&state.formula, "SUM(A1:A2)");
        let part = evaluate_part(&mut session, &state, start, len).unwrap();
        assert_eq!((part.literal.as_str(), part.shown.as_str()), ("3.5", "number 3.5"));
        let (start, len) = span_of(&state.formula, "LEN(\"ab\")");
        assert_eq!(evaluate_part(&mut session, &state, start, len).unwrap().literal, "2");
        // A range is an array constant, a text keeps its quotes, an error is its spelling.
        let (start, len) = span_of(&state.formula, "A1:A2");
        assert_eq!(evaluate_part(&mut session, &state, start, len).unwrap().literal, "{1.5;2}");
        let state = authored("=\"a\"&\"b\"&1/0", &[]);
        let (start, len) = span_of(&state.formula, "\"a\"&\"b\"");
        assert_eq!(evaluate_part(&mut session, &state, start, len).unwrap().literal, "\"ab\"");
        let (start, len) = span_of(&state.formula, "1/0");
        assert_eq!(evaluate_part(&mut session, &state, start, len).unwrap().literal, "#DIV/0!");
        // The whole formula, with its `=`; and nothing selected is a request to select.
        let (_, whole) = span_of(&state.formula, &state.formula);
        assert!(evaluate_part(&mut session, &state, 0, whole).unwrap().literal.starts_with('#'));
        assert!(evaluate_part(&mut session, &state, 2, 0).unwrap_err().starts_with("Select"));
        // An unknown function is #NAME?, a value like any other.
        assert!(evaluate_part(&mut session, &authored("=NOSUCH(1)+1", &[]), 1, 9).is_ok_and(|v| v.literal == "#NAME?"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_part_inside_let_sees_the_bindings_before_it_and_a_lambda_parameter_has_no_value() {
        let mut session = Session::new("Sheet1").unwrap();
        let state = authored("=LET(a,2,b,a*10,b+a)", &[]);
        let (start, len) = span_of(&state.formula, "b+a");
        assert_eq!(evaluate_part(&mut session, &state, start, len).unwrap().literal, "22");
        let (start, len) = span_of(&state.formula, "a*10");
        assert_eq!(evaluate_part(&mut session, &state, start, len).unwrap().literal, "20");
        let state = authored("=MAP({1,2},LAMBDA(x,x*3))", &[]);
        let (start, len) = span_of(&state.formula, "x*3");
        let refusal = evaluate_part(&mut session, &state, start, len).unwrap_err();
        assert!(refusal.contains("LAMBDA parameter"), "{refusal}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_trace_lists_every_part_innermost_first_with_its_value() {
        let mut session = Session::new("Sheet1").unwrap();
        let state = authored("=SUM(A1:A2)*2", &[(0, "1.5"), (grid::COLUMNS as usize, "2")]);
        let traced = trace(&mut session, &state);
        let steps: Vec<(&str, &str, usize)> = traced
            .steps
            .iter()
            .map(|s| (s.text.as_str(), s.value.as_str(), s.depth))
            .collect();
        assert_eq!(
            steps,
            [("A1:A2", "{1.5;2}", 2), ("SUM(A1:A2)", "3.5", 1), ("SUM(A1:A2)*2", "7", 0)]
        );
        // The names a LET declares have no step; its values and body do, with the bindings.
        let state = authored("=LET(a,2,a*5)", &[]);
        let traced = trace(&mut session, &state);
        let texts: Vec<&str> = traced.steps.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["a", "a*5", "LET(a,2,a*5)"]);
        assert_eq!(traced.steps[0].value, "2");
        assert_eq!(traced.steps[2].value, "10");
        // A constant formula is its own step; a syntax error is no trace.
        assert_eq!(super::trace(&mut session, &authored("=42", &[])).steps.len(), 1);
        assert!(super::trace(&mut session, &authored("=1+", &[])).steps.is_empty());
    }
}

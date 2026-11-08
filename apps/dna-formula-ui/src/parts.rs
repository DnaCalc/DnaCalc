//! Parts of the formula evaluated on their own: F9 on a selected part, and the evaluation trace
//! (the port of DnaOneCalc's X-Ray, dc-wr9), for whichever engine the owner has.
//!
//! Both ask the owner's one evaluator ([`PartEvaluator`]) for the value of a part of the formula
//! text over the same inputs, form, environment and functions as the whole formula. A part inside
//! LET is evaluated with the bindings the LET declares before it (the evaluated text is
//! `LET(name1,value1,...,part)`); a part that uses a LAMBDA parameter has no value until the
//! LAMBDA is called, and says so. The trace does not instrument the evaluator: each step is an
//! evaluation of that part again, so a volatile part (RAND) can differ between steps, and a part
//! an IF skips is still evaluated for its step.
//!
//! DNA OneCalc evaluates through its session, DNA Calc through its worker's host (a request over
//! the workbook transport); the wire types are `dna_calc_model::parts`.

use crate::display;
use crate::result;
use calc_fml::assist::{LocalBinding, local_bindings_at};
use calc_fml::parse_formula;
use calc_fml::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree, TokenKind, lex};
use calc_func::value::CalcValue;
use calc_fml::FormatProfile;

pub use dna_calc_model::parts::{MAX_TRACE_STEPS, PartValue, Trace, TraceStep};

/// What the owner calculates for a part: the value of formula text (with its leading `=`) over the
/// owner's inputs, or why there is none, and the profile values are shown with.
pub trait PartEvaluator {
    fn calculate(&mut self, text: &str) -> Result<CalcValue, String>;
    fn profile(&self) -> Option<FormatProfile>;
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

/// The value of the part of `formula` at `start`..`start + len` (UTF-16 code units).
pub fn evaluate_part(
    evaluator: &mut dyn PartEvaluator,
    formula: &str,
    start: usize,
    len: usize,
) -> Result<PartValue, String> {
    let units: Vec<u16> = formula.encode_utf16().collect();
    let tree = parse_formula(formula);
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
    let to_evaluate = evaluable(formula, &tree, start + lead, text)?;
    let value = evaluator.calculate(&to_evaluate)?;
    Ok(PartValue {
        literal: display::formula_literal(&value)?,
        shown: display::value(&value),
    })
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

/// The evaluation trace of `formula`: every part with its value, innermost first.
pub fn trace(evaluator: &mut dyn PartEvaluator, formula: &str) -> Trace {
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
            .and_then(|to_evaluate| value_for_step(evaluator, &to_evaluate))
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

fn value_for_step(evaluator: &mut dyn PartEvaluator, text: &str) -> Result<(String, String), String> {
    let value = evaluator.calculate(text)?;
    let profile = evaluator.profile();
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

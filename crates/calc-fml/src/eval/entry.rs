//! Formula entry: the check Excel makes when a formula is entered, before it stores anything
//! (dc-mx3x). The grid's cell entry, the file loader and the compiler ask it, so the three agree.
//!
//! The check reads syntax only, never a workbook: it needs no names, no references and no
//! values. Its rules are of two kinds.
//!
//! - **Grammar**, held here ([`GrammarRule`]): the parser's diagnostics; the argument counts of
//!   LET and LAMBDA; a name declared twice by one LET or one LAMBDA, compared by the fold Excel
//!   matches parameters by; a call postfix after an operand that cannot be called.
//! - **Each function's entry signature**, held by the catalogue
//!   ([`calc_func::FunctionCallTarget::check_entry`]): this module builds the [`OperandForm`] of
//!   each argument from its syntax and asks the call's entry once. No function's spelling is
//!   tested here.
//!
//! Measured on Excel 16.0 build 20527 on 2026-11-07 through `Range.Formula2` and `Range.Formula`
//! (both agreed on every formula; COM's `0x800A03EC` is the refusal). A LET refuses a name it
//! already declared, in any case and anywhere among its names: `=LET(xx,1,XX,2,xx)`,
//! `=LET(xx,1,xx,xx+1,xx)`, `=LET(x,1,y,2,x,3,x)`, `=LET(x1,1,X1,2,x1)` and `=LET(é,1,É,2,é)` are
//! refused. Another LET or LAMBDA is another scope: `=LET(x,1,LET(x,2,x))` is 2,
//! `=LET(x,1,y,LAMBDA(x,x),y(2))` 2, `=LAMBDA(x,LET(x,2,x))(1)` 2, `=LET(x,LAMBDA(x,x),x(1))` 1 and
//! `=LAMBDA(x,LAMBDA(x,x))(1)(2)` 2. A LAMBDA refuses a parameter it already declared:
//! `=LAMBDA(xx,XX,xx)(1,2)`, `=LAMBDA(x,x,1)` and `=LAMBDA(x,y,x,1)` are refused,
//! `=LAMBDA(x,X)(1)` is 1. LET needs pairs and a calculation: `=LET()`, `=LET(x)`, `=LET(x,1)` and
//! `=LET(x,1,y,2)` are refused; LAMBDA needs a calculation: `=LAMBDA()` is refused, while
//! `=LAMBDA(1)`, `=LAMBDA(x)` and `=LAMBDA(x,1)` enter (#CALC!). A call postfix needs an operand
//! that may be called: `=(1)(3)`, `={1}(3)` and `="a"(3)` are refused; `=A1(3)` and
//! `=nm(3)` enter (#REF!, #NAME?), as do `=(LAMBDA(x,x))(3)`, `=LET(f,LAMBDA(x,x),f)(3)` and
//! `=LAMBDA(x,LAMBDA(y,x+y))(1)(2)`.

use super::compile::{
    BindingCall, binding_call, file_function, file_unprefixed, only_token, parameter_key,
    unknown_on_load, without_prefix,
};
use crate::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree, TextSpan, TokenKind};
use calc_func::value::WorksheetErrorCode;
use calc_func::{EntryRule, FunctionCallTarget, FunctionId, Notation, OperandForm, catalogue};
use std::fmt;

/// A formula rule of the grammar that Excel's entry refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GrammarRule {
    /// The parser refused the formula, or it holds syntax the parser accepts and Excel does not
    /// (a LET or LAMBDA name that is no name, an array constant element that is no constant).
    Syntax,
    /// LET without a name, a value and a calculation, or with a name left without its value;
    /// LAMBDA without a calculation.
    BindingArguments,
    /// A LET declares one name twice.
    DuplicateLetName,
    /// A LAMBDA declares one parameter twice.
    DuplicateLambdaParameter,
    /// A call postfix after an operand that cannot be called: a constant, an array constant or
    /// an operation of values.
    NotCallable,
    /// A call of an operator's name that entry does not take: `ANCHORARRAY` and the trim
    /// references `_TRO_ALL`, `_TRO_LEADING` and `_TRO_TRAILING`, which only a file stores (for
    /// `B1#` and `A1:.A2`), and `SINGLE` (`@`) with other than one argument.
    OperatorCall,
}

/// Why Excel's entry refuses a formula: a rule of the grammar, or a rule of one function's entry
/// signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rejection {
    Grammar(GrammarRule),
    Function {
        function: FunctionId,
        rule: EntryRule,
    },
}

impl Rejection {
    /// Whether the rule is one the entry measurements of dc-mx3x name, rather than a parser
    /// diagnostic, which may also be syntax this parser does not hold.
    pub fn is_measured(self) -> bool {
        !matches!(self, Rejection::Grammar(GrammarRule::Syntax))
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejection::Grammar(rule) => f.write_str(match rule {
                GrammarRule::Syntax => "syntax Excel does not accept",
                GrammarRule::BindingArguments => "LET or LAMBDA without its arguments",
                GrammarRule::DuplicateLetName => "a LET declares a name twice",
                GrammarRule::DuplicateLambdaParameter => "a LAMBDA declares a parameter twice",
                GrammarRule::NotCallable => "a call of an operand that cannot be called",
                GrammarRule::OperatorCall => "a call of an operator's name",
            }),
            Rejection::Function { function, rule } => {
                let spelling = catalogue()
                    .resolve(*function)
                    .map(|target| target.spelling())
                    .unwrap_or("a function");
                write!(f, "{spelling}: {rule}")
            }
        }
    }
}

/// A formula Excel's entry refuses: the rule, at the span of the call or name it refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryRejection {
    pub span: TextSpan,
    pub rule: Rejection,
}

/// Whether Excel's entry takes the formula `tree`. `loaded` reads it as a file stores it, where a
/// function a file stores with the prefix `_xlfn.` written without it is an unknown name, which
/// enters. The first refused construct in source order is reported.
pub fn check_entry(tree: &SyntaxTree, loaded: bool) -> Result<(), EntryRejection> {
    if let Some(first) = tree.diagnostics().first() {
        return Err(EntryRejection {
            span: first.span,
            rule: Rejection::Grammar(GrammarRule::Syntax),
        });
    }
    let Some(expression) = tree.expression() else {
        return Err(EntryRejection {
            span: tree.root().span(),
            rule: Rejection::Grammar(GrammarRule::Syntax),
        });
    };
    // A walk on a stack of its own, as the compiler's: a formula nests as deep as its length
    // allows. Children are pushed in reverse so they are visited in source order.
    let mut pending = vec![expression];
    while let Some(node) = pending.pop() {
        check_node(node, loaded).map_err(|rule| EntryRejection {
            span: node.span(),
            rule,
        })?;
        let children: Vec<&SyntaxNode> = node.child_nodes().collect();
        pending.extend(children.into_iter().rev());
    }
    Ok(())
}

/// The rules of one node: a call's or a call postfix's.
fn check_node(node: &SyntaxNode, loaded: bool) -> Result<(), Rejection> {
    match node.kind() {
        SyntaxKind::CallExpr => {
            let (name, list) = match node.children() {
                [SyntaxChild::Token(name), SyntaxChild::Node(list)] => (name, list),
                _ => return Ok(()),
            };
            let arguments: Vec<&SyntaxNode> = list.child_nodes().collect();
            let spelling = file_unprefixed(name.text());
            match binding_call(spelling) {
                Some(BindingCall::Let) => return check_let(&arguments),
                Some(BindingCall::Lambda) => return check_lambda(&arguments),
                None => {}
            }
            if let Some(operator) = operator_call(name.text(), loaded) {
                return match operator {
                    Operator::FileOnly if !loaded => {
                        Err(Rejection::Grammar(GrammarRule::OperatorCall))
                    }
                    _ if arguments.len() != 1 => Err(Rejection::Grammar(GrammarRule::OperatorCall)),
                    _ => Ok(()),
                };
            }
            let Some(target) = catalogue_call(name.text(), loaded) else {
                return Ok(());
            };
            let operands: Vec<OperandForm> = arguments
                .iter()
                .map(|argument| operand_form(argument, loaded))
                .collect();
            target
                .check_entry(&operands)
                .map_err(|rule| Rejection::Function {
                    function: target.id(),
                    rule,
                })
        }
        SyntaxKind::InvokeExpr => {
            let Some(callee) = node.child_nodes().next() else {
                return Ok(());
            };
            match operand_form(callee, loaded) {
                OperandForm::Call(function) => match catalogue().resolve(function) {
                    Ok(target) => target
                        .check_called()
                        .map_err(|rule| Rejection::Function { function, rule }),
                    Err(_) => Ok(()),
                },
                OperandForm::Constant
                | OperandForm::ArrayConstant
                | OperandForm::Expression
                | OperandForm::Missing => Err(Rejection::Grammar(GrammarRule::NotCallable)),
                OperandForm::Reference
                | OperandForm::Lambda
                | OperandForm::Let
                | OperandForm::Name
                | OperandForm::Invocation => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

/// `LET(name1, value1, [name2, value2, ...], calculation)`: an odd count of at least 3, and no
/// name declared twice.
fn check_let(arguments: &[&SyntaxNode]) -> Result<(), Rejection> {
    let count = arguments.len();
    if count < 3 || count.is_multiple_of(2) {
        return Err(Rejection::Grammar(GrammarRule::BindingArguments));
    }
    let names = arguments[..count - 1].iter().step_by(2);
    if declares_twice(names.copied()) {
        return Err(Rejection::Grammar(GrammarRule::DuplicateLetName));
    }
    Ok(())
}

/// `LAMBDA([parameter1, ...], calculation)`: at least the calculation, and no parameter declared
/// twice.
fn check_lambda(arguments: &[&SyntaxNode]) -> Result<(), Rejection> {
    let Some((_, parameters)) = arguments.split_last() else {
        return Err(Rejection::Grammar(GrammarRule::BindingArguments));
    };
    if declares_twice(parameters.iter().copied()) {
        return Err(Rejection::Grammar(GrammarRule::DuplicateLambdaParameter));
    }
    Ok(())
}

/// Whether two of `names` declare the same name. A node that is no name is the compiler's to
/// refuse; it is skipped here.
fn declares_twice<'t>(names: impl Iterator<Item = &'t SyntaxNode>) -> bool {
    let mut keys: Vec<String> = Vec::new();
    for node in names {
        if node.kind() != SyntaxKind::IdentifierExpr {
            continue;
        }
        let Some(text) = only_token(node).map(|token| token.text()) else {
            continue;
        };
        let text = text
            .strip_prefix('[')
            .and_then(|inner| inner.strip_suffix(']'))
            .or_else(|| without_prefix(text, "_xlop."))
            .unwrap_or(text);
        let key = parameter_key(text);
        if keys.contains(&key) {
            return true;
        }
        keys.push(key);
    }
    false
}

/// An operator written as a call.
enum Operator {
    /// `SINGLE` (`@`): entered with or without `_xlfn.` (`=SINGLE(A1)` enters as `=@A1`), read
    /// from a file with it.
    Single,
    /// `ANCHORARRAY` (`#`) and the trim references `_TRO_LEADING`, `_TRO_TRAILING` and
    /// `_TRO_ALL`: a file's spellings, with `_xlfn.`. Entry refuses them however written
    /// (dc-mx3x: `=_xlfn.ANCHORARRAY(B1)`, `=ANCHORARRAY(B1)`, `=_xlfn._xlws.ANCHORARRAY(B1)`,
    /// `=COUNTBLANK(_xlfn.ANCHORARRAY(B1))`, `=_xlfn._TRO_ALL(A1:A2)`, `=_TRO_ALL(A1:A2)`,
    /// `=_xlfn._TRO_LEADING(A1:A2)` and `=_xlfn._TRO_TRAILING(A1:A2)` are refused).
    FileOnly,
}

/// The operator a call written `written` writes, if any. Read from a file, only the spelling
/// with `_xlfn.` is one; a bare spelling there is a name.
fn operator_call(written: &str, loaded: bool) -> Option<Operator> {
    let spelling = file_unprefixed(written);
    let prefixed = spelling.len() != written.len();
    if loaded && !prefixed {
        return None;
    }
    if let Some(operator) = catalogue().resolve_operator_alias(spelling) {
        match operator.notation() {
            Notation::Prefix => Some(Operator::Single),
            Notation::Postfix => Some(Operator::FileOnly),
            _ => None,
        }
    } else if ["_TRO_LEADING", "_TRO_TRAILING", "_TRO_ALL"]
        .iter()
        .any(|operator| spelling.eq_ignore_ascii_case(operator))
    {
        Some(Operator::FileOnly)
    } else {
        None
    }
}

/// The catalogue entry a named call written `written` calls, or `None` for a name that is no
/// catalogue entry (a LET name, a defined name, a user function, a name nothing defines), which
/// enters whatever its arguments, as does a loaded call of an entry a file stores with a prefix
/// written without it.
fn catalogue_call(written: &str, loaded: bool) -> Option<FunctionCallTarget<'static>> {
    let target = catalogue()
        .resolve_surface(Notation::Call, file_function(written))
        .ok()?;
    if matches!(target.implementation(), calc_func::Implementation::NotHeld { .. } | calc_func::Implementation::Deferred { .. }) {
        return None;
    }
    if loaded && unknown_on_load(written, super::Target(target)) {
        return None;
    }
    Some(target)
}

/// The form of an operand, from its syntax: parentheses are transparent.
pub(crate) fn operand_form(node: &SyntaxNode, loaded: bool) -> OperandForm {
    let mut node = node;
    loop {
        return match node.kind() {
            SyntaxKind::GroupingExpr => match node.child_nodes().next() {
                Some(inner) => {
                    node = inner;
                    continue;
                }
                None => OperandForm::Expression,
            },
            // `#REF!` is a reference that refers to nothing: `=COUNTBLANK(#REF!)`, `=ROW(#REF!)`,
            // `=COUNTIF(#REF!,1)` and `=#REF!(3)` enter (#REF!), `=COUNTBLANK(#NULL!)`,
            // `=COUNTBLANK(#DIV/0!)` and `=#N/A(3)` are refused (dc-mx3x).
            SyntaxKind::ErrorLiteralExpr => match only_token(node).and_then(|t| t.error_code()) {
                Some(WorksheetErrorCode::Ref) => OperandForm::Reference,
                _ => OperandForm::Constant,
            },
            SyntaxKind::NumberLiteralExpr | SyntaxKind::StringLiteralExpr => OperandForm::Constant,
            SyntaxKind::ArrayLiteralExpr => OperandForm::ArrayConstant,
            SyntaxKind::OmittedArgExpr => OperandForm::Missing,
            SyntaxKind::IdentifierExpr => {
                let text = only_token(node).map(|token| token.text()).unwrap_or_default();
                if text.eq_ignore_ascii_case("TRUE") || text.eq_ignore_ascii_case("FALSE") {
                    OperandForm::Constant
                } else {
                    // A cell reference or a name: binding decides, and both fill a reference
                    // place.
                    OperandForm::Name
                }
            }
            SyntaxKind::QuotedIdentifierExpr => OperandForm::Name,
            SyntaxKind::QualifiedReferenceExpr
            | SyntaxKind::SheetSpan3DReferenceExpr
            | SyntaxKind::RangeExpr
            | SyntaxKind::TrimRangeExpr
            | SyntaxKind::IntersectionExpr
            | SyntaxKind::UnionExpr => OperandForm::Reference,
            SyntaxKind::PrefixExpr => match node.children() {
                [SyntaxChild::Token(operator), _] if operator.kind() == TokenKind::At => {
                    OperandForm::Reference
                }
                _ => OperandForm::Expression,
            },
            SyntaxKind::PostfixExpr => match node.children() {
                [_, SyntaxChild::Token(operator)] if operator.kind() == TokenKind::Hash => {
                    OperandForm::Reference
                }
                _ => OperandForm::Expression,
            },
            SyntaxKind::BinaryExpr => OperandForm::Expression,
            SyntaxKind::InvokeExpr => OperandForm::Invocation,
            SyntaxKind::CallExpr => match node.children() {
                [SyntaxChild::Token(name), _] => {
                    match binding_call(file_unprefixed(name.text())) {
                        Some(BindingCall::Let) => OperandForm::Let,
                        Some(BindingCall::Lambda) => OperandForm::Lambda,
                        None if operator_call(name.text(), loaded).is_some() => {
                            OperandForm::Reference
                        }
                        None => match catalogue_call(name.text(), loaded) {
                            Some(target) => OperandForm::Call(target.id()),
                            None => OperandForm::Invocation,
                        },
                    }
                }
                _ => OperandForm::Invocation,
            },
            // Only a formula with a diagnostic holds these, and that is refused first.
            SyntaxKind::MissingExpr
            | SyntaxKind::UnsupportedExpr
            | SyntaxKind::FormulaRoot
            | SyntaxKind::ArgumentList => OperandForm::Name,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_formula;

    /// Excel's answers to 3,822 formula entries (dc-mx3x; the file's header says how they were
    /// taken).
    const ANSWERS: &str = include_str!("../../tests/fixtures/entry/excel-entry.tsv");

    /// Native and browser: [`check_entry`] takes exactly the formulas Excel took, and refuses
    /// exactly those it refused, for every measured formula.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn entry_matches_excel() {
        let mut differ = Vec::new();
        let mut count = 0;
        for line in ANSWERS.lines().filter(|line| !line.starts_with('#')) {
            let (answer, formula) = line.split_once('\t').expect("an answer and a formula");
            let taken = answer == "1";
            let checked = check_entry(&parse_formula(formula), false);
            if checked.is_ok() != taken {
                differ.push(format!("{formula}: Excel {answer}, here {checked:?}"));
            }
            count += 1;
        }
        assert_eq!(count, 3822);
        assert!(differ.is_empty(), "{} differ:\n{}", differ.len(), differ.join("\n"));
    }
}

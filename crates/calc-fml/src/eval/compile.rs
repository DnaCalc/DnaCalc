//! Lowering a parsed formula into a [`CompiledPlan`].
//!
//! Literals, bound references, the arithmetic, comparison and concatenation operators and calls
//! of catalogue functions compile; everything else is refused with a typed [`CompileError`] that
//! names the construct and the bead that owns it, never compiled into an approximation.
//!
//! A number literal becomes its nearest binary64 here, once, under no numeric mode (ties to even,
//! at most 15 significant digits); that Excel converts every such literal to the same binary64 is
//! not measured by this package. A plan holds no folded constant: every operation runs at
//! evaluation, under the caller's mode.
//!
//! Lowering walks the tree on a stack of its own [`Task`]s, not on the native stack: a node's
//! operands are lowered before the instruction that consumes them, in source order, so a formula
//! nested as deep as its 8,192 characters allow (`=1+1+...+1`, `=((...(1)...))`, `=--...-1`)
//! lowers without recursion.

use super::{CompiledPlan, Instruction, Literal, PlanCache, Target};
use crate::reference::{
    BindError, FormulaAnchor, ReferenceCatalog, ReferenceNotation, ReferenceOperand,
    bind_references,
};
use crate::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree, TextSpan, Token, TokenKind};
use calc_func::value::ExcelText;
use calc_func::{Notation, catalogue};
use std::collections::HashMap;
use std::sync::Arc;

/// Why a formula did not compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    /// The parser refused the formula; `span` is its first diagnostic's.
    Syntax { span: TextSpan },
    /// A reference that binding refused.
    Reference { span: TextSpan, error: BindError },
    /// Formula syntax this compiler does not lower yet: `construct` says what it is and `owner`
    /// names the bead that owns it.
    Unsupported {
        span: TextSpan,
        construct: &'static str,
        owner: &'static str,
    },
    /// No catalogue entry is written this way: a function the catalogue does not hold yet, or a
    /// name Excel does not know (whose `#NAME?` result is not modelled either).
    UnknownFunction { span: TextSpan, spelling: String },
    /// The call supplies a number of arguments the entry's arity does not admit, which Excel
    /// refuses to enter.
    Arity { span: TextSpan, supplied: usize },
    /// A number literal whose Excel value is not modelled: more than 15 significant digits
    /// (which Excel rounds to 15 at entry), beyond binary64's range, or in or below its subnormal
    /// range. Owned by bead cfml-m2.6 (numeric text policy).
    NumberLiteral { span: TextSpan },
}

/// Compiles the formula `tree`, its references read in `notation` and bound for a formula
/// anchored at `anchor` through `catalog`, and returns the plan `cache` holds for its normal form.
///
/// The plan does not depend on the anchor: binding gives each relative coordinate as an offset
/// from it, so the same formula copied to another anchor compiles to the same plan. Each call
/// lowers the tree and binds its references; the cache only shares the resulting plan.
pub fn compile(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
) -> Result<Arc<CompiledPlan>, CompileError> {
    if let Some(first) = tree.diagnostics().first() {
        return Err(CompileError::Syntax { span: first.span });
    }
    let Some(expression) = tree.expression() else {
        return Err(CompileError::Syntax {
            span: tree.root().span(),
        });
    };
    let mut lowering = Lowering {
        sites: bind_references(tree, notation, anchor, catalog)
            .into_iter()
            .map(|site| (site.span, site.binding))
            .collect(),
        blocks: vec![Vec::new()],
        references: Vec::new(),
    };
    lowering.formula(expression)?;
    let plan = CompiledPlan {
        blocks: lowering
            .blocks
            .into_iter()
            .map(Vec::into_boxed_slice)
            .collect(),
        references: lowering.references.into_boxed_slice(),
    };
    Ok(cache.share(plan))
}

struct Lowering {
    /// The binder's sites by span; each is taken once, by the node it spans.
    sites: HashMap<TextSpan, Result<ReferenceOperand, BindError>>,
    blocks: Vec<Vec<Instruction>>,
    references: Vec<ReferenceOperand>,
}

/// One step of the lowering, on the lowering's own stack: the steps a node pushes run in the
/// reverse order of their pushing, so it pushes the instruction that consumes its operands first
/// and its first operand last.
enum Task<'t> {
    /// Append to block `block` the instructions that push the value of `node`. `tail` says
    /// whether that value is the formula's result: the root expression, through parentheses, a
    /// prefix `+` or `-` and the arguments a lazy function may return.
    Lower {
        node: &'t SyntaxNode,
        tail: bool,
        block: usize,
    },
    /// Append `instruction` to block `block`, after its operands' instructions.
    Emit {
        block: usize,
        instruction: Instruction,
    },
    /// Append to block `block` the check of the formula's final `+` or `-`, written `operator`.
    FinalSumCheck { block: usize, operator: &'t Token },
}

impl Lowering {
    /// Lowers the formula's expression into block 0, and each argument of a lazy call into the
    /// block it is given, in source order.
    fn formula(&mut self, expression: &SyntaxNode) -> Result<(), CompileError> {
        let mut tasks = vec![Task::Lower {
            node: expression,
            tail: true,
            block: 0,
        }];
        while let Some(task) = tasks.pop() {
            match task {
                Task::Lower { node, tail, block } => {
                    self.expression(node, tail, block, &mut tasks)?
                }
                Task::Emit { block, instruction } => self.blocks[block].push(instruction),
                Task::FinalSumCheck { block, operator } => {
                    final_sum_check(operator, &mut self.blocks[block])?
                }
            }
        }
        Ok(())
    }

    /// Lowers `node` into block `block`: a leaf at once, and an operation as the tasks that lower
    /// its operands and then emit it, pushed onto `tasks`. `tail` is as [`Task::Lower`] says.
    fn expression<'t>(
        &mut self,
        node: &'t SyntaxNode,
        tail: bool,
        block: usize,
        tasks: &mut Vec<Task<'t>>,
    ) -> Result<(), CompileError> {
        let span = node.span();
        match node.kind() {
            SyntaxKind::NumberLiteralExpr => {
                if !self.reference(node, block)? {
                    self.blocks[block].push(Instruction::Literal(number(node)?));
                }
            }
            SyntaxKind::IdentifierExpr => {
                if !self.reference(node, block)? {
                    let text = only_token(node).map(Token::text).unwrap_or_default();
                    if text.eq_ignore_ascii_case("TRUE") {
                        self.blocks[block].push(Instruction::Literal(Literal::Logical(true)));
                    } else if text.eq_ignore_ascii_case("FALSE") {
                        self.blocks[block].push(Instruction::Literal(Literal::Logical(false)));
                    } else {
                        return Err(unsupported(span, "a defined name", "cfml-m3.1"));
                    }
                }
            }
            SyntaxKind::QualifiedReferenceExpr | SyntaxKind::SheetSpan3DReferenceExpr => {
                if !self.reference(node, block)? {
                    return Err(unsupported(span, "a sheet-qualified name", "cfml-m3.1"));
                }
            }
            SyntaxKind::RangeExpr => {
                if !self.reference(node, block)? {
                    return Err(unsupported(
                        span,
                        "the range operator between separate references",
                        "cfml-m3.3",
                    ));
                }
            }
            SyntaxKind::QuotedIdentifierExpr => {
                return Err(unsupported(span, "a quoted name", "cfml-m3.1"));
            }
            SyntaxKind::StringLiteralExpr => {
                self.blocks[block].push(Instruction::Literal(text(node)?))
            }
            SyntaxKind::ErrorLiteralExpr => {
                let code = only_token(node)
                    .and_then(Token::error_code)
                    .ok_or(CompileError::Syntax { span })?;
                self.blocks[block].push(Instruction::Literal(Literal::Error(code)));
            }
            SyntaxKind::OmittedArgExpr => self.blocks[block].push(Instruction::Omitted),
            SyntaxKind::GroupingExpr => {
                let inner = node
                    .child_nodes()
                    .next()
                    .ok_or(CompileError::Syntax { span })?;
                tasks.push(Task::Lower {
                    node: inner,
                    tail,
                    block,
                });
            }
            SyntaxKind::BinaryExpr => {
                let (left, operator, right) = match node.children() {
                    [
                        SyntaxChild::Node(left),
                        SyntaxChild::Token(operator),
                        SyntaxChild::Node(right),
                    ] => (left, operator, right),
                    _ => return Err(CompileError::Syntax { span }),
                };
                let target = target(Notation::Infix, operator)?;
                let final_sum =
                    tail && matches!(operator.kind(), TokenKind::Plus | TokenKind::Minus);
                if final_sum {
                    tasks.push(Task::FinalSumCheck { block, operator });
                }
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Call {
                        target,
                        argc: 2,
                        retain: final_sum,
                        synthetic: false,
                    },
                });
                for operand in [right, left] {
                    tasks.push(Task::Lower {
                        node: operand,
                        tail: false,
                        block,
                    });
                }
            }
            SyntaxKind::PrefixExpr => {
                let (operator, operand) = match node.children() {
                    [SyntaxChild::Token(operator), SyntaxChild::Node(operand)] => {
                        (operator, operand)
                    }
                    _ => return Err(CompileError::Syntax { span }),
                };
                if operator.kind() == TokenKind::At {
                    return Err(unsupported(span, "implicit intersection (@)", "cfml-m3.3"));
                }
                let target = target(Notation::Prefix, operator)?;
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Call {
                        target,
                        argc: 1,
                        retain: false,
                        synthetic: false,
                    },
                });
                // A prefix `+` or `-` changes no magnitude (the sign flips exactly), so an operand
                // at the tail stays there for the near-zero check: whether Excel compensates a
                // final sum under a sign is not known, and the check refuses rather than guess.
                tasks.push(Task::Lower {
                    node: operand,
                    tail,
                    block,
                });
            }
            SyntaxKind::PostfixExpr => {
                let (operand, operator) = match node.children() {
                    [SyntaxChild::Node(operand), SyntaxChild::Token(operator)] => {
                        (operand, operator)
                    }
                    _ => return Err(CompileError::Syntax { span }),
                };
                if operator.kind() == TokenKind::Hash {
                    return Err(unsupported(span, "a spilled range (#)", "cfml-m3.3"));
                }
                let target = target(Notation::Postfix, operator)?;
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Call {
                        target,
                        argc: 1,
                        retain: false,
                        synthetic: false,
                    },
                });
                tasks.push(Task::Lower {
                    node: operand,
                    tail: false,
                    block,
                });
            }
            SyntaxKind::CallExpr => self.call(node, tail, block, tasks)?,
            SyntaxKind::ArrayLiteralExpr => {
                return Err(unsupported(span, "an array constant", "cfml-m3.4"));
            }
            SyntaxKind::IntersectionExpr => {
                return Err(unsupported(span, "the intersection operator", "cfml-m3.3"));
            }
            SyntaxKind::UnionExpr => {
                return Err(unsupported(span, "the union operator", "cfml-m3.3"));
            }
            SyntaxKind::InvokeExpr => {
                return Err(unsupported(
                    span,
                    "a call of an expression's value",
                    "cfml-m2.3",
                ));
            }
            // The parser gives each of these a diagnostic, refused above, or never places them
            // under an expression.
            SyntaxKind::MissingExpr
            | SyntaxKind::UnsupportedExpr
            | SyntaxKind::FormulaRoot
            | SyntaxKind::ArgumentList => return Err(CompileError::Syntax { span }),
        }
        Ok(())
    }

    /// A call by name, lowered into block `block`. An eager entry's arguments are pushed in order
    /// before the call; each argument of a lazy entry becomes a block of its own, numbered when
    /// the call is met and run only when the entry asks for it.
    fn call<'t>(
        &mut self,
        node: &'t SyntaxNode,
        tail: bool,
        block: usize,
        tasks: &mut Vec<Task<'t>>,
    ) -> Result<(), CompileError> {
        let span = node.span();
        let (name, list) = match node.children() {
            [SyntaxChild::Token(name), SyntaxChild::Node(list)] => (name, list),
            _ => return Err(CompileError::Syntax { span }),
        };
        let target = target(Notation::Call, name)?;
        let arguments: Vec<&SyntaxNode> = list.child_nodes().collect();
        let argc = arguments.len();
        if !target.0.arity().accepts(argc) {
            return Err(CompileError::Arity {
                span,
                supplied: argc,
            });
        }
        let demand = target.0.argument_demand();
        if !demand.is_lazy() {
            tasks.push(Task::Emit {
                block,
                instruction: Instruction::Call {
                    target,
                    argc,
                    retain: false,
                    synthetic: false,
                },
            });
            for argument in arguments.into_iter().rev() {
                tasks.push(Task::Lower {
                    node: argument,
                    tail: false,
                    block,
                });
            }
            return Ok(());
        }
        let first_block = self.blocks.len();
        self.blocks.resize_with(first_block + argc, Vec::new);
        tasks.push(Task::Emit {
            block,
            instruction: Instruction::LazyCall {
                target,
                first_block,
                argc,
            },
        });
        for (index, argument) in arguments.into_iter().enumerate().rev() {
            tasks.push(Task::Lower {
                node: argument,
                tail: tail && demand.holds_back(index),
                block: first_block + index,
            });
        }
        Ok(())
    }

    /// Pushes onto block `block` the reference `node` binds to, when binding gave it a site;
    /// `false` when it is not a reference.
    fn reference(&mut self, node: &SyntaxNode, block: usize) -> Result<bool, CompileError> {
        let span = node.span();
        match self.sites.remove(&span) {
            None => Ok(false),
            Some(Err(error)) => Err(CompileError::Reference { span, error }),
            Some(Ok(operand)) => {
                self.blocks[block].push(Instruction::Reference(self.references.len()));
                self.references.push(operand);
                Ok(true)
            }
        }
    }
}

fn unsupported(span: TextSpan, construct: &'static str, owner: &'static str) -> CompileError {
    CompileError::Unsupported {
        span,
        construct,
        owner,
    }
}

/// The catalogue entry written as `token` in `notation`.
fn target(notation: Notation, token: &Token) -> Result<Target, CompileError> {
    catalogue()
        .resolve_surface(notation, token.text())
        .map(Target)
        .map_err(|_| CompileError::UnknownFunction {
            span: token.span(),
            spelling: token.text().to_owned(),
        })
}

/// Appends the check of the formula's final `+` or `-`, written `operator`, after the call that
/// computed it and retained its two operands. When the result is a non-zero number, each operand
/// is copied and negated by a call of the catalogue's prefix `-`, which reads it as a number by
/// the same worksheet coercion as the operation, and [`Instruction::NearZeroCheck`] compares the
/// result with the larger negation. The negations are marked synthetic: the formula did not write
/// them, and an observer of the evaluation's calls must not count them as its own.
fn final_sum_check(operator: &Token, out: &mut Vec<Instruction>) -> Result<(), CompileError> {
    let negate = catalogue()
        .resolve_surface(Notation::Prefix, "-")
        .map(Target)
        .map_err(|_| CompileError::UnknownFunction {
            span: operator.span(),
            spelling: "-".to_owned(),
        })?;
    let negation = Instruction::Call {
        target: negate,
        argc: 1,
        retain: false,
        synthetic: true,
    };
    // Stack: left, right, result. Two below the top is the left operand; after its negation is
    // pushed, two below the top is the right operand.
    let check = [
        Instruction::Copy(2),
        negation.clone(),
        Instruction::Copy(2),
        negation,
        Instruction::NearZeroCheck,
    ];
    out.push(Instruction::NearZeroGuard { skip: check.len() });
    out.extend(check);
    Ok(())
}

/// The token of a node that holds exactly one.
fn only_token(node: &SyntaxNode) -> Option<&Token> {
    match node.children() {
        [SyntaxChild::Token(token)] => Some(token),
        _ => None,
    }
}

/// Significant digits Excel keeps of a number it reads.
const SIGNIFICANT_DIGITS: usize = 15;

/// A number literal's value: the binary64 nearest to its decimal (ties to even), a zero as +0. A
/// literal above 9.99999999999999E+307, the largest number Excel lets a cell be given, is admitted
/// up to binary64's maximum (`=1E308`); what Excel does with it in a formula is not measured.
fn number(node: &SyntaxNode) -> Result<Literal, CompileError> {
    let span = node.span();
    let refused = CompileError::NumberLiteral { span };
    let text = only_token(node).map(Token::text).ok_or(refused.clone())?;
    let (digits, nonzero) = significant_digits(text);
    if digits > SIGNIFICANT_DIGITS {
        return Err(refused);
    }
    let value: f64 = text.parse().map_err(|_| refused.clone())?;
    if !value.is_finite() || value.is_subnormal() || (value == 0.0 && nonzero) {
        return Err(refused);
    }
    Ok(Literal::Number(
        if value == 0.0 { 0.0f64 } else { value }.to_bits(),
    ))
}

/// The significant digits of a number literal's mantissa (from its first to its last non-zero
/// digit), and whether it has a non-zero digit at all.
fn significant_digits(text: &str) -> (usize, bool) {
    let mantissa = text.split(['e', 'E']).next().unwrap_or_default();
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let first = digits.iter().position(|&d| d != b'0');
    let last = digits.iter().rposition(|&d| d != b'0');
    match (first, last) {
        (Some(first), Some(last)) => (last - first + 1, true),
        _ => (0, false),
    }
}

/// A string literal's text: inside its quotes, each doubled quote one quote.
fn text(node: &SyntaxNode) -> Result<Literal, CompileError> {
    let span = node.span();
    let quoted = only_token(node)
        .map(Token::text)
        .ok_or(CompileError::Syntax { span })?;
    let inner = quoted
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or(CompileError::Syntax { span })?;
    let units = inner.replace("\"\"", "\"").encode_utf16().collect();
    Ok(Literal::Text(ExcelText::from_utf16_code_units(units)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_formula;
    use calc_func::value::{CellKey, IdAllocator, SheetId};

    struct Sheets(SheetId);

    impl ReferenceCatalog for Sheets {
        fn sheet_named(&self, name: &str) -> Option<SheetId> {
            name.eq_ignore_ascii_case("Sheet2").then_some(self.0)
        }
    }

    fn compiled(formula: &str) -> Result<Arc<CompiledPlan>, CompileError> {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let one = ids.issue().unwrap();
        let two = ids.issue().unwrap();
        let anchor = FormulaAnchor::new(CellKey::new(one, 5, 3).unwrap());
        compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &Sheets(two),
            &mut PlanCache::new(),
        )
    }

    fn refusal(formula: &str) -> CompileError {
        match compiled(formula) {
            Ok(plan) => panic!("{formula} compiled: {plan:?}"),
            Err(error) => error,
        }
    }

    fn owner(formula: &str) -> &'static str {
        match refusal(formula) {
            CompileError::Unsupported { owner, .. } => owner,
            other => panic!("{formula}: {other:?}"),
        }
    }

    /// Each construct this compiler does not lower is refused with its owner, never lowered.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn unsupported_constructs_name_their_owner() {
        assert_eq!(owner("=TaxRate*2"), "cfml-m3.1");
        assert_eq!(owner("=Sheet2!TaxRate"), "cfml-m3.1");
        assert_eq!(owner("=(A1):B2"), "cfml-m3.3");
        assert_eq!(owner("=SUM(A1 B2)"), "cfml-m3.3");
        assert_eq!(owner("=SUM((A1,B2))"), "cfml-m3.3");
        assert_eq!(owner("=@A1:A3"), "cfml-m3.3");
        assert_eq!(owner("=A1#"), "cfml-m3.3");
        assert_eq!(owner("=SUM({1,2})"), "cfml-m3.4");
        assert_eq!(owner("=(A1)(2)"), "cfml-m2.3");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn refusals_are_typed() {
        assert!(matches!(refusal("=1+"), CompileError::Syntax { .. }));
        assert!(matches!(
            refusal("=Nope!A1"),
            CompileError::Reference {
                error: BindError::UnknownSheet { .. },
                ..
            }
        ));
        assert!(matches!(
            refusal("=$XFE$1"),
            CompileError::Reference {
                error: BindError::OutsideGrid,
                ..
            }
        ));
        assert_eq!(
            refusal("=NOSUCH(1)"),
            CompileError::UnknownFunction {
                span: TextSpan::new(1, 6),
                spelling: "NOSUCH".to_owned()
            }
        );
        assert!(matches!(
            refusal("=IF(1)"),
            CompileError::Arity { supplied: 1, .. }
        ));
        assert!(matches!(
            refusal("=SUM()"),
            CompileError::Arity { supplied: 0, .. }
        ));
        for literal in ["=1234567890123456", "=1E400", "=1E-310", "=1E-400"] {
            assert!(
                matches!(refusal(literal), CompileError::NumberLiteral { .. }),
                "{literal}"
            );
        }
    }

    /// Number literals of at most 15 significant digits read as their nearest binary64; leading
    /// and trailing zeros are not significant.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn number_literals() {
        let pushed = |formula: &str| match compiled(formula).unwrap().blocks[0][..] {
            [Instruction::Literal(Literal::Number(bits))] => f64::from_bits(bits),
            ref other => panic!("{formula}: {other:?}"),
        };
        assert_eq!(pushed("=0.1"), 0.1);
        assert_eq!(pushed("=.5"), 0.5);
        assert_eq!(pushed("=5."), 5.0);
        assert_eq!(pushed("=1E3"), 1000.0);
        assert_eq!(pushed("=123456789012345"), 123_456_789_012_345.0);
        assert_eq!(pushed("=1234567890123450000"), 1.23456789012345e18);
        assert_eq!(pushed("=0.000000000000000000001"), 1e-21);
        assert_eq!(pushed("=0e5").to_bits(), 0.0f64.to_bits());
        // Above 9.99999999999999E+307, the largest number Excel lets a cell be given: admitted, and
        // what Excel does with it is not measured.
        assert_eq!(pushed("=1E308"), 1e308);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn string_and_logical_literals() {
        let plan = compiled("=\"a\"\"b\"&TRUE&false").unwrap();
        let literals: Vec<&Instruction> = plan.blocks[0]
            .iter()
            .filter(|instruction| matches!(instruction, Instruction::Literal(_)))
            .collect();
        assert_eq!(
            literals,
            [
                &Instruction::Literal(Literal::Text(ExcelText::from_interop_assignment("a\"b"))),
                &Instruction::Literal(Literal::Logical(true)),
                &Instruction::Literal(Literal::Logical(false)),
            ]
        );
    }

    /// The final operation of the formula is checked, through parentheses, a prefix `+` or `-`
    /// and the branches of IF, and nowhere else.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn final_sums_are_marked() {
        let marked = |formula: &str| -> usize {
            compiled(formula)
                .unwrap()
                .blocks
                .iter()
                .flat_map(|block| block.iter())
                .filter(|instruction| matches!(instruction, Instruction::NearZeroCheck))
                .count()
        };
        assert_eq!(marked("=0.5-0.4-0.1"), 1);
        assert_eq!(marked("=(0.5-0.4-0.1)"), 1);
        assert_eq!(marked("=+(1+2)"), 1);
        assert_eq!(marked("=IF(1+1,2-1,3+4)"), 2);
        assert_eq!(marked("=1*(0.5-0.4-0.1)"), 0);
        assert_eq!(marked("=-(1+2)"), 1);
        assert_eq!(marked("=2*-(1+2)"), 0);
        assert_eq!(marked("=SUM(1+2)"), 0);
        assert_eq!(marked("=(1+2)%"), 0);
        assert_eq!(marked("=1+2=3"), 0);
    }

    /// The check's negations are calls of the catalogue's prefix `-` in the plan, after the final
    /// operation's retaining call, and are the only synthetic calls; no other call retains its
    /// arguments, and a negation the formula writes is not synthetic.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn final_sum_check_instructions() {
        let resolved = |notation, spelling| {
            Target(
                catalogue()
                    .resolve_surface(notation, spelling)
                    .expect("a catalogue entry"),
            )
        };
        let call = |target, argc, retain| Instruction::Call {
            target,
            argc,
            retain,
            synthetic: false,
        };
        let synthetic = |target| Instruction::Call {
            target,
            argc: 1,
            retain: false,
            synthetic: true,
        };
        let minus = resolved(Notation::Infix, "-");
        let negate = resolved(Notation::Prefix, "-");
        let number = |n: f64| Instruction::Literal(Literal::Number(n.to_bits()));
        assert_eq!(
            compiled("=0.5-0.4-0.1").unwrap().blocks[..],
            [vec![
                number(0.5),
                number(0.4),
                call(minus, 2, false),
                number(0.1),
                call(minus, 2, true),
                Instruction::NearZeroGuard { skip: 5 },
                Instruction::Copy(2),
                synthetic(negate),
                Instruction::Copy(2),
                synthetic(negate),
                Instruction::NearZeroCheck,
            ]
            .into_boxed_slice()]
        );
        assert_eq!(
            compiled("=-0.5+1").unwrap().blocks[..],
            [vec![
                number(0.5),
                call(negate, 1, false),
                number(1.0),
                call(resolved(Notation::Infix, "+"), 2, true),
                Instruction::NearZeroGuard { skip: 5 },
                Instruction::Copy(2),
                synthetic(negate),
                Instruction::Copy(2),
                synthetic(negate),
                Instruction::NearZeroCheck,
            ]
            .into_boxed_slice()]
        );
        assert_eq!(
            compiled("=1*(0.5-0.4)").unwrap().blocks[..],
            [vec![
                number(1.0),
                number(0.5),
                number(0.4),
                call(minus, 2, false),
                call(resolved(Notation::Infix, "*"), 2, false),
            ]
            .into_boxed_slice()]
        );
    }
}

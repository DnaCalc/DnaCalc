//! The lossless Excel syntax tree: nodes over tokens, where the tokens in tree order, each
//! preceded by its leading trivia, reproduce the formula text exactly, malformed and incomplete
//! input included.
//!
//! A tree is as deep as its formula nests: `=1+1+...+1` of Excel's maximum length (8,192
//! characters) is 4,096 nodes deep. Dropping, cloning and comparing a node, and listing its
//! tokens, therefore walk the tree with a stack of their own instead of recursing, so none of them
//! grows the native stack with the tree's depth. The derived `Debug` of [`SyntaxNode`],
//! [`SyntaxChild`] and [`SyntaxTree`] is the one walk that still recurses, once per level of the
//! tree: nothing on the route from formula text to value formats a tree.

use super::token::{SyntaxDiagnostic, TextSpan, Token};

/// The node kinds of the Excel formula grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxKind {
    /// The whole formula: an optional `=`, one expression, any trailing tokens the parser could
    /// not place, and the end of input.
    FormulaRoot,
    NumberLiteralExpr,
    StringLiteralExpr,
    ErrorLiteralExpr,
    /// `{1,2;3,4}`: elements separated by `,` within a row and `;` between rows.
    ArrayLiteralExpr,
    /// An unquoted name: a cell or range reference, a defined name, `TRUE`/`FALSE`, a
    /// bracketed qualifier or structured reference token; binding decides which.
    IdentifierExpr,
    /// A quoted name such as `'My Sheet'` that is not followed by `!`.
    QuotedIdentifierExpr,
    /// `Sheet2!A1` or `'My Sheet'!A1`: a sheet qualifier, `!` and the qualified operand. In
    /// `#REF!A1`, the form Excel writes after the referenced sheet is deleted, the qualifier is
    /// the `#REF!` error-literal token, which holds the `!`.
    QualifiedReferenceExpr,
    /// `Sheet1:Sheet3!A1`: a run of sheets qualifying an operand.
    SheetSpan3DReferenceExpr,
    /// A function call by name: `SUM(A1,2)`.
    CallExpr,
    /// A call of an expression's value: `LAMBDA(x,x+1)(2)`.
    InvokeExpr,
    /// `(` arguments separated by `,` `)`.
    ArgumentList,
    /// A binary operator other than the reference operators: comparison, `&`, `+`, `-`, `*`,
    /// `/` and `^`.
    BinaryExpr,
    /// A prefix operator: `+`, `-` or `@` (implicit intersection).
    PrefixExpr,
    /// A postfix operator: `%` or `#` (spilled range).
    PostfixExpr,
    GroupingExpr,
    /// The range operator `:`.
    RangeExpr,
    /// The intersection operator, whitespace between two references; its whitespace tokens
    /// are children of the node, not trivia.
    IntersectionExpr,
    /// The union operator `,` (outside argument lists and array literals).
    UnionExpr,
    /// An argument left empty, as the second argument of `IF(A1,,2)`.
    OmittedArgExpr,
    /// An operand the parser expected but did not find, or an unexpected token in its place
    /// (which the node then holds).
    MissingExpr,
    /// Syntax outside the grammar this parser accepts: a selector that is not Excel syntax, or
    /// an Excel trim reference (`A1.:B10`, `A1:.B10`, `A1.:.B10`). The refused tokens are its
    /// direct, unstructured children, and a
    /// [`super::SyntaxDiagnosticKind::UnsupportedSyntax`] diagnostic spans exactly them; the
    /// operand before a refused member selector and the operands on either side of a trim
    /// operator are structured child nodes.
    UnsupportedExpr,
}

/// A child of a [`SyntaxNode`]: a node or a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxChild {
    Node(SyntaxNode),
    Token(Token),
}

impl SyntaxChild {
    fn span(&self) -> TextSpan {
        match self {
            SyntaxChild::Node(node) => node.span,
            SyntaxChild::Token(token) => token.span(),
        }
    }
}

#[derive(Debug)]
pub struct SyntaxNode {
    kind: SyntaxKind,
    span: TextSpan,
    children: Vec<SyntaxChild>,
}

impl Drop for SyntaxNode {
    /// Takes the child nodes out of every node below this one before it is dropped, so each
    /// drops with no child nodes left and the drop does not recurse.
    fn drop(&mut self) {
        let mut pending = Vec::new();
        take_child_nodes(&mut self.children, &mut pending);
        while let Some(mut node) = pending.pop() {
            take_child_nodes(&mut node.children, &mut pending);
        }
    }
}

/// Moves the child nodes of `children` to `out`, dropping its tokens.
fn take_child_nodes(children: &mut Vec<SyntaxChild>, out: &mut Vec<SyntaxNode>) {
    for child in children.drain(..) {
        if let SyntaxChild::Node(node) = child {
            out.push(node);
        }
    }
}

impl Clone for SyntaxNode {
    /// A copy built bottom-up on a stack of the nodes being copied, without recursion.
    fn clone(&self) -> Self {
        // Each entry: a node being copied, the index of its next child, and its children copied
        // so far.
        let mut copying: Vec<(&SyntaxNode, usize, Vec<SyntaxChild>)> =
            vec![(self, 0, Vec::with_capacity(self.children.len()))];
        loop {
            let top = copying.len() - 1;
            let (node, next) = (copying[top].0, copying[top].1);
            if let Some(child) = node.children.get(next) {
                copying[top].1 += 1;
                match child {
                    SyntaxChild::Token(token) => {
                        copying[top].2.push(SyntaxChild::Token(token.clone()))
                    }
                    SyntaxChild::Node(inner) => {
                        copying.push((inner, 0, Vec::with_capacity(inner.children.len())))
                    }
                }
                continue;
            }
            let (node, _, children) = copying.pop().expect("the node being copied");
            let copy = SyntaxNode {
                kind: node.kind,
                span: node.span,
                children,
            };
            match copying.last_mut() {
                Some((_, _, copied)) => copied.push(SyntaxChild::Node(copy)),
                None => return copy,
            }
        }
    }
}

impl PartialEq for SyntaxNode {
    /// Compares the two trees node by node on a stack of the pairs still to compare, without
    /// recursion.
    fn eq(&self, other: &Self) -> bool {
        let mut pending = vec![(self, other)];
        while let Some((left, right)) = pending.pop() {
            if left.kind != right.kind
                || left.span != right.span
                || left.children.len() != right.children.len()
            {
                return false;
            }
            for pair in left.children.iter().zip(&right.children) {
                match pair {
                    (SyntaxChild::Token(a), SyntaxChild::Token(b)) if a == b => {}
                    (SyntaxChild::Node(a), SyntaxChild::Node(b)) => pending.push((a, b)),
                    _ => return false,
                }
            }
        }
        true
    }
}

impl Eq for SyntaxNode {}

impl SyntaxNode {
    /// A node over `children`, spanning its first child's start to its last child's end (leading
    /// trivia excluded).
    pub(crate) fn new(kind: SyntaxKind, children: Vec<SyntaxChild>) -> Self {
        let span = match (children.first(), children.last()) {
            (Some(first), Some(last)) => TextSpan::covering(first.span(), last.span()),
            _ => TextSpan::default(),
        };
        Self {
            kind,
            span,
            children,
        }
    }

    /// A node without children, with an empty span at `at`.
    pub(crate) fn empty(kind: SyntaxKind, at: usize) -> Self {
        Self {
            kind,
            span: TextSpan::new(at, 0),
            children: Vec::new(),
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// The node's span in UTF-16 code units, without the leading trivia of its first token.
    pub fn span(&self) -> TextSpan {
        self.span
    }

    pub fn children(&self) -> &[SyntaxChild] {
        &self.children
    }

    /// The child nodes, in order.
    pub fn child_nodes(&self) -> impl Iterator<Item = &SyntaxNode> {
        self.children.iter().filter_map(|child| match child {
            SyntaxChild::Node(node) => Some(node),
            SyntaxChild::Token(_) => None,
        })
    }

    /// Every token under the node, in source order.
    pub fn tokens(&self) -> Vec<&Token> {
        let mut out = Vec::new();
        collect_tokens(self, &mut out);
        out
    }

    /// The node's source text: its tokens, each preceded by its leading trivia except the
    /// first.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (index, token) in self.tokens().into_iter().enumerate() {
            if index > 0 {
                for trivia in token.leading_trivia() {
                    out.push_str(trivia.text());
                }
            }
            out.push_str(token.text());
        }
        out
    }
}

/// Appends the tokens under `node` to `out` in source order, walking the tree on a stack of the
/// children still to visit, the next one on top.
fn collect_tokens<'a>(node: &'a SyntaxNode, out: &mut Vec<&'a Token>) {
    let mut pending: Vec<&'a SyntaxChild> = node.children.iter().rev().collect();
    while let Some(child) = pending.pop() {
        match child {
            SyntaxChild::Node(inner) => pending.extend(inner.children.iter().rev()),
            SyntaxChild::Token(token) => out.push(token),
        }
    }
}

/// A parsed formula: the [`SyntaxKind::FormulaRoot`] node and the diagnostics. A formula with no
/// diagnostics is well formed in this grammar. That does not make it a formula Excel accepts:
/// binding, which has not happened yet, and the function catalogue refuse more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTree {
    root: SyntaxNode,
    diagnostics: Vec<SyntaxDiagnostic>,
}

impl SyntaxTree {
    pub(crate) fn new(root: SyntaxNode, diagnostics: Vec<SyntaxDiagnostic>) -> Self {
        Self { root, diagnostics }
    }

    pub fn root(&self) -> &SyntaxNode {
        &self.root
    }

    /// The formula's expression: the root's one child node.
    pub fn expression(&self) -> Option<&SyntaxNode> {
        self.root.child_nodes().next()
    }

    pub fn diagnostics(&self) -> &[SyntaxDiagnostic] {
        &self.diagnostics
    }

    /// Every token of the tree in source order, ending with the end of input, whose leading
    /// trivia is any whitespace after the last token.
    pub fn tokens(&self) -> Vec<&Token> {
        self.root.tokens()
    }

    /// The source text the tree was parsed from, rebuilt from its tokens and trivia.
    pub fn source_text(&self) -> String {
        let mut out = String::new();
        for token in self.tokens() {
            for trivia in token.leading_trivia() {
                out.push_str(trivia.text());
            }
            out.push_str(token.text());
        }
        out
    }
}

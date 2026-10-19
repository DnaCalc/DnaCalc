//! The lossless Excel formula syntax: [`lex`] splits formula text into tokens and
//! [`parse_formula`] builds a [`SyntaxTree`] over them.
//!
//! The tree is lossless: its tokens in order, each preceded by its leading trivia, reproduce the
//! entered text exactly, including malformed and incomplete input. Spans are UTF-16 code units.
//! Syntax outside this grammar is refused at parse time with a
//! [`SyntaxDiagnosticKind::UnsupportedSyntax`] diagnostic and an [`SyntaxKind::UnsupportedExpr`]
//! node: the refused selector or operator tokens are unstructured children of that node, while
//! the operand before a refused member selector, and the two operands of a trim reference, stay
//! structured child nodes.

mod combining_marks;
mod lexer;
mod parser;
mod token;
mod tree;

pub use lexer::lex;
pub use parser::parse_formula;
pub use token::{SyntaxDiagnostic, SyntaxDiagnosticKind, TextSpan, Token, TokenKind, Trivia};
pub use tree::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree};

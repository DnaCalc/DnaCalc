//! Tokens, trivia, source spans and syntax diagnostics of the Excel formula language.
//!
//! Every offset is in UTF-16 code units of the formula text, the unit an editor in the browser
//! and the Excel object model count in, so a character outside the Basic Multilingual Plane
//! occupies two.

use calc_func::value::WorksheetErrorCode;

/// A range of the formula text, in UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextSpan {
    /// Zero-based offset from the start of the input.
    pub start: usize,
    /// Length in UTF-16 code units.
    pub len: usize,
}

impl TextSpan {
    /// Constructs a span from a UTF-16 offset and length.
    pub fn new(start: usize, len: usize) -> Self {
        Self { start, len }
    }

    /// The offset just past the span.
    pub fn end(self) -> usize {
        self.start + self.len
    }

    /// The span from the start of `start` to the end of `end`.
    pub fn covering(start: TextSpan, end: TextSpan) -> Self {
        Self::new(start.start, end.end().saturating_sub(start.start))
    }
}

/// The kinds of token the lexer produces. Every character of the input belongs to exactly one
/// token, so the tokens (whitespace included) reproduce the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// The equals sign, used for comparison or at formula entry.
    Equals,
    /// The `<>` comparison operator.
    NotEqual,
    /// The `<` comparison operator.
    Less,
    /// The `<=` comparison operator.
    LessEqual,
    /// The `>` comparison operator.
    Greater,
    /// The `>=` comparison operator.
    GreaterEqual,
    /// A numeric literal, including its exponent when present.
    Number,
    /// An unquoted identifier or reference token.
    Identifier,
    /// An Excel error literal such as `#REF!`.
    ErrorLiteral,
    /// A single-quoted name or sheet qualifier.
    QuotedIdentifier,
    /// Text enclosed in brackets within a reference.
    BracketedQualifier,
    /// A double-quoted text literal.
    StringLiteral,
    /// An opening parenthesis.
    LParen,
    /// A closing parenthesis.
    RParen,
    /// A comma, used between arguments or as a union operator.
    Comma,
    /// A colon, used to form a range.
    Colon,
    /// The text concatenation operator, `&`.
    Ampersand,
    /// A unary plus or binary addition operator.
    Plus,
    /// A unary minus or binary subtraction operator.
    Minus,
    /// The postfix percent operator.
    Percent,
    /// The exponentiation operator, `^`.
    Caret,
    /// The multiplication operator, `*`.
    Star,
    /// The division operator, `/`.
    Slash,
    /// A period outside a number or identifier.
    Dot,
    /// The implicit-intersection operator, `@`.
    At,
    /// The spill reference operator, `#`.
    Hash,
    /// The sheet-qualifier separator, `!`.
    Bang,
    /// The opening brace of an array literal.
    LBrace,
    /// The closing brace of an array literal.
    RBrace,
    /// A semicolon, used between array rows.
    Semicolon,
    /// Whitespace, which can represent reference intersection.
    Whitespace,
    /// Input the lexer could not classify.
    Unknown,
    /// A zero-width marker at the end of the input.
    Eof,
}

impl TokenKind {
    /// Whether a token of this kind is trivia wherever the grammar does not give it a meaning
    /// (whitespace is the intersection operator between two references and trivia elsewhere).
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace)
    }
}

/// Whitespace kept beside a token so the source reprints exactly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Trivia {
    text: String,
    span: TextSpan,
}

impl Trivia {
    pub(crate) fn new(text: String, span: TextSpan) -> Self {
        Self { text, span }
    }

    /// Returns the exact whitespace text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the position and length of the whitespace.
    pub fn span(&self) -> TextSpan {
        self.span
    }
}

/// One token of the formula text with the whitespace that precedes it.
///
/// A token the parser expected but did not find (a closing parenthesis after `=SUM(A1,`) is a
/// missing token: its text is empty, its span is empty at the place it was expected, and
/// [`Token::is_missing`] is true.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    kind: TokenKind,
    text: String,
    span: TextSpan,
    leading_trivia: Vec<Trivia>,
    missing: bool,
}

impl Token {
    pub(crate) fn new(kind: TokenKind, text: impl Into<String>, span: TextSpan) -> Self {
        Self {
            kind,
            text: text.into(),
            span,
            leading_trivia: Vec::new(),
            missing: false,
        }
    }

    pub(crate) fn missing(kind: TokenKind, at: usize) -> Self {
        Self {
            kind,
            text: String::new(),
            span: TextSpan::new(at, 0),
            leading_trivia: Vec::new(),
            missing: true,
        }
    }

    pub(crate) fn set_leading_trivia(&mut self, trivia: Vec<Trivia>) {
        self.leading_trivia = trivia;
    }

    /// Returns the lexical classification of the token.
    pub fn kind(&self) -> TokenKind {
        self.kind
    }

    /// The token's source text, exactly as entered (empty for a missing token and for the end
    /// of input).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The token's own span, without its leading trivia.
    pub fn span(&self) -> TextSpan {
        self.span
    }

    /// The whitespace between the previous token and this one.
    pub fn leading_trivia(&self) -> &[Trivia] {
        &self.leading_trivia
    }

    /// Reports whether parser recovery inserted this token without consuming input.
    pub fn is_missing(&self) -> bool {
        self.missing
    }

    /// The span of the token together with its leading trivia.
    pub fn full_span(&self) -> TextSpan {
        let start = self
            .leading_trivia
            .first()
            .map(|trivia| trivia.span.start)
            .unwrap_or(self.span.start);
        TextSpan::new(start, self.span.end().saturating_sub(start))
    }

    /// The worksheet error an error-literal token spells (`#DIV/0!` is
    /// [`WorksheetErrorCode::Div0`]); `None` for every other token.
    pub fn error_code(&self) -> Option<WorksheetErrorCode> {
        if self.kind != TokenKind::ErrorLiteral {
            return None;
        }
        ERROR_LITERALS
            .iter()
            .find(|(spelling, _)| spelling.eq_ignore_ascii_case(&self.text))
            .map(|(_, code)| *code)
    }
}

/// The spelling of each worksheet error literal. The lexer tries them in this order and matches
/// without regard to ASCII case.
pub(crate) const ERROR_LITERALS: [(&str, WorksheetErrorCode); 14] = [
    ("#GETTING_DATA", WorksheetErrorCode::GettingData),
    ("#BLOCKED!", WorksheetErrorCode::Blocked),
    ("#CONNECT!", WorksheetErrorCode::Connect),
    ("#FIELD!", WorksheetErrorCode::Field),
    ("#SPILL!", WorksheetErrorCode::Spill),
    ("#VALUE!", WorksheetErrorCode::Value),
    ("#DIV/0!", WorksheetErrorCode::Div0),
    ("#NAME?", WorksheetErrorCode::Name),
    ("#CALC!", WorksheetErrorCode::Calc),
    ("#BUSY!", WorksheetErrorCode::Busy),
    ("#NULL!", WorksheetErrorCode::Null),
    ("#REF!", WorksheetErrorCode::Ref),
    ("#NUM!", WorksheetErrorCode::Num),
    ("#N/A", WorksheetErrorCode::NA),
];

/// The classification of a [`SyntaxDiagnostic`]. Every diagnostic refuses the formula in this
/// parser.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SyntaxDiagnosticKind {
    /// A malformed or incomplete formula (an unexpected or missing token, or an unterminated
    /// quoted text), or a call nested deeper than the parser's limit of nested levels of
    /// functions.
    Message,
    /// Syntax outside the grammar this parser accepts: a `^` selector at the start of an
    /// operand or a `.` member selector, which are not Excel syntax, and Excel's trim references
    /// (`A1.:B10`), which Excel accepts and this parser does not parse yet. The tree keeps the
    /// refused tokens, unstructured, inside a [`super::SyntaxKind::UnsupportedExpr`] node.
    UnsupportedSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A parser or lexer diagnostic attached to the original formula.
pub struct SyntaxDiagnostic {
    /// Human-readable explanation for the editor.
    pub message: String,
    /// Location in the original input, possibly zero-width for a missing token.
    pub span: TextSpan,
    /// Whether the problem is malformed input or an unsupported construct.
    pub kind: SyntaxDiagnosticKind,
}

impl SyntaxDiagnostic {
    /// A diagnostic of kind [`SyntaxDiagnosticKind::Message`].
    pub(crate) fn message(message: impl Into<String>, span: TextSpan) -> Self {
        Self {
            message: message.into(),
            span,
            kind: SyntaxDiagnosticKind::Message,
        }
    }

    /// A diagnostic of kind [`SyntaxDiagnosticKind::UnsupportedSyntax`].
    pub(crate) fn unsupported(message: impl Into<String>, span: TextSpan) -> Self {
        Self {
            message: message.into(),
            span,
            kind: SyntaxDiagnosticKind::UnsupportedSyntax,
        }
    }
}

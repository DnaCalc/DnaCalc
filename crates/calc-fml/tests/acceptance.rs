//! Acceptance cases owned by the calc-fml package, each registered by its exact name in
//! `crates/calc-fml/acceptance.json`. Every case is `#[test]` natively and `#[wasm_bindgen_test]`
//! on wasm32, with no browser or Node.js configuration of its own: `cargo xtask accept` forces the
//! browser with `WASM_BINDGEN_USE_BROWSER`.
//!
//! The cases reach the parser only through calc-fml's public API (`calc_fml::parse_formula` and
//! the `calc_fml::syntax` types). They check the tree against the input text by their own walk:
//! the UTF-16 code units of the input are cut at each token's and trivia's span and compared with
//! its text, and those pieces must tile the input from offset 0 to its end with nothing left out.
//!
//! The cfml-m1.2 cases reach reference binding only through the same API
//! (`calc_fml::bind_references`, `ReferenceOperand::instantiate`, `calc_fml::read_cells`), with a
//! catalogue and a reader defined here that log every call they receive. Expected cells and areas
//! come from the cases' own A1 decoding, not from calc-fml.
//!
//! The cfml-m1.3 cases reach compilation and evaluation only through `calc_fml::compile`,
//! `calc_fml::instantiate` and `calc_fml::evaluate`, over the same logging catalogue and a reader
//! of their own, which reports consecutive stored cells as one run of values and logs the areas it
//! is asked for and the runs it reports: its log of requested areas is the provider-call counter.
//! The expected values are of three kinds, and none has been compared with Excel here:
//! - rows with an Excel value (=1+2*3 is 7) are derived by hand from Excel's documented operator
//!   and function rules;
//! - some values rest on choices Microsoft does not document, made by hand and not measured: SUM
//!   returns the first error of its range in row-major order, and a blank cell read as a number
//!   is 0;
//! - the rows that expect a typed refusal (FunctionFault::NotImplemented for =B1^0.5,
//!   EvalFault::AreaAsValue for =A1:A3+1, EvalFault::NearZeroFinalSum for the near-zero rows)
//!   expect the implementation's current answer, not Excel's value, and are labelled so.
//!
//! cfunc-m1.2's oracle capture is the Excel evidence for the operators and functions; bead
//! dc-r4ci reads it for the near-zero rows.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_fml::parse_formula;
use calc_fml::reference::{
    AxisRef, BindError, CellRef, CellRun, FormulaAnchor, PlacementError, ReferenceNotation,
    ReferenceOperand, ReferenceShape, SheetTarget,
};
use calc_fml::syntax::{
    SyntaxChild, SyntaxDiagnosticKind, SyntaxKind, SyntaxNode, SyntaxTree, TextSpan, Token,
    TokenKind,
};
use calc_fml::{
    CompileError, CompiledPlan, EvalFault, EvalFrame, EvalScratch, EvaluationServices,
    FormulaInstance, PlanCache, compile, evaluate, instantiate,
};
use calc_fml::{ReferenceCatalog, ReferenceReader, bind_references, read_cells};
use calc_func::value::{
    CalcValue, CellKey, ExcelText, GridArea, IdAllocator, MAX_COLUMNS, MAX_ROWS, NumericMode,
    SheetId, WorksheetErrorCode,
};
use calc_func::{FunctionFault, Notation, catalogue};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

/// Prints a line the acceptance runner keeps: on stdout natively, on the browser console (which
/// wasm-bindgen-test-runner forwards under `--nocapture`) on wasm32.
macro_rules! note {
    ($($arg:tt)*) => {{
        let line = format!($($arg)*);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_test::console_log!("{}", line);
        #[cfg(not(target_arch = "wasm32"))]
        println!("{line}");
    }};
}

// ---------------------------------------------------------------------------------------------
// The test's own view of a tree.

/// Every syntax kind, by an exhaustive match: a kind added to the grammar fails to compile here
/// until this list (and the coverage below) says how it is exercised.
fn kind_name(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::FormulaRoot => "Root",
        SyntaxKind::NumberLiteralExpr => "Num",
        SyntaxKind::StringLiteralExpr => "Str",
        SyntaxKind::ErrorLiteralExpr => "Err",
        SyntaxKind::ArrayLiteralExpr => "Array",
        SyntaxKind::IdentifierExpr => "Id",
        SyntaxKind::QuotedIdentifierExpr => "QuotedId",
        SyntaxKind::QualifiedReferenceExpr => "Qualified",
        SyntaxKind::SheetSpan3DReferenceExpr => "SheetSpan",
        SyntaxKind::CallExpr => "Call",
        SyntaxKind::InvokeExpr => "Invoke",
        SyntaxKind::ArgumentList => "Args",
        SyntaxKind::BinaryExpr => "Binary",
        SyntaxKind::PrefixExpr => "Prefix",
        SyntaxKind::PostfixExpr => "Postfix",
        SyntaxKind::GroupingExpr => "Group",
        SyntaxKind::RangeExpr => "Range",
        SyntaxKind::IntersectionExpr => "Intersect",
        SyntaxKind::UnionExpr => "Union",
        SyntaxKind::OmittedArgExpr => "Omitted",
        SyntaxKind::MissingExpr => "Missing",
        SyntaxKind::UnsupportedExpr => "Unsupported",
    }
}

const ALL_SYNTAX_KINDS: [SyntaxKind; 22] = [
    SyntaxKind::FormulaRoot,
    SyntaxKind::NumberLiteralExpr,
    SyntaxKind::StringLiteralExpr,
    SyntaxKind::ErrorLiteralExpr,
    SyntaxKind::ArrayLiteralExpr,
    SyntaxKind::IdentifierExpr,
    SyntaxKind::QuotedIdentifierExpr,
    SyntaxKind::QualifiedReferenceExpr,
    SyntaxKind::SheetSpan3DReferenceExpr,
    SyntaxKind::CallExpr,
    SyntaxKind::InvokeExpr,
    SyntaxKind::ArgumentList,
    SyntaxKind::BinaryExpr,
    SyntaxKind::PrefixExpr,
    SyntaxKind::PostfixExpr,
    SyntaxKind::GroupingExpr,
    SyntaxKind::RangeExpr,
    SyntaxKind::IntersectionExpr,
    SyntaxKind::UnionExpr,
    SyntaxKind::OmittedArgExpr,
    SyntaxKind::MissingExpr,
    SyntaxKind::UnsupportedExpr,
];

const ALL_TOKEN_KINDS: [TokenKind; 33] = [
    TokenKind::Equals,
    TokenKind::NotEqual,
    TokenKind::Less,
    TokenKind::LessEqual,
    TokenKind::Greater,
    TokenKind::GreaterEqual,
    TokenKind::Number,
    TokenKind::Identifier,
    TokenKind::ErrorLiteral,
    TokenKind::QuotedIdentifier,
    TokenKind::BracketedQualifier,
    TokenKind::StringLiteral,
    TokenKind::LParen,
    TokenKind::RParen,
    TokenKind::Comma,
    TokenKind::Colon,
    TokenKind::Ampersand,
    TokenKind::Plus,
    TokenKind::Minus,
    TokenKind::Percent,
    TokenKind::Caret,
    TokenKind::Star,
    TokenKind::Slash,
    TokenKind::Dot,
    TokenKind::At,
    TokenKind::Hash,
    TokenKind::Bang,
    TokenKind::LBrace,
    TokenKind::RBrace,
    TokenKind::Semicolon,
    TokenKind::Whitespace,
    TokenKind::Unknown,
    TokenKind::Eof,
];

/// A leaf of the tree in source order: a token with the kind of the node that holds it, or a
/// node without children.
enum Leaf<'a> {
    Token {
        token: &'a Token,
        parent: SyntaxKind,
    },
    Empty(&'a SyntaxNode),
}

fn leaves<'a>(node: &'a SyntaxNode, out: &mut Vec<Leaf<'a>>) {
    if node.children().is_empty() {
        out.push(Leaf::Empty(node));
    }
    for child in node.children() {
        match child {
            SyntaxChild::Node(inner) => leaves(inner, out),
            SyntaxChild::Token(token) => out.push(Leaf::Token {
                token,
                parent: node.kind(),
            }),
        }
    }
}

fn leaf_span(leaf: &Leaf<'_>) -> TextSpan {
    match leaf {
        Leaf::Token { token, .. } => token.span(),
        Leaf::Empty(node) => node.span(),
    }
}

fn nodes<'a>(node: &'a SyntaxNode, out: &mut Vec<&'a SyntaxNode>) {
    out.push(node);
    for inner in node.child_nodes() {
        nodes(inner, out);
    }
}

/// The input text at `span`, cut from its UTF-16 code units.
fn utf16_slice(units: &[u16], span: TextSpan) -> String {
    assert!(
        span.end() <= units.len(),
        "span {span:?} runs past the input's {} UTF-16 code units",
        units.len()
    );
    String::from_utf16(&units[span.start..span.end()]).expect("a span cuts at code points")
}

/// Checks a tree against its input by the test's own walk, and returns the text the walk
/// rebuilt: the trivia and tokens must tile the input's UTF-16 code units from 0 to the end,
/// each piece equal to the input cut at its span; missing tokens and childless nodes are empty
/// at the cursor; the last token is the end of input; every node spans exactly from its first
/// leaf to its last.
fn check_lossless(source: &str, tree: &SyntaxTree) -> String {
    let units: Vec<u16> = source.encode_utf16().collect();
    let mut walk = Vec::new();
    leaves(tree.root(), &mut walk);
    let mut rebuilt = String::new();
    let mut cursor = 0usize;
    let mut last_token = None;
    for leaf in &walk {
        let token = match leaf {
            Leaf::Token { token, .. } => *token,
            Leaf::Empty(node) => {
                assert_eq!(
                    node.span(),
                    TextSpan::new(cursor, 0),
                    "{source:?}: empty {:?} off the cursor",
                    node.kind()
                );
                continue;
            }
        };
        for trivia in token.leading_trivia() {
            assert_eq!(trivia.span().start, cursor, "{source:?}: trivia gap");
            assert_eq!(
                utf16_slice(&units, trivia.span()),
                trivia.text(),
                "{source:?}"
            );
            assert!(
                trivia.text().chars().all(|c| c.is_ascii_whitespace()),
                "{source:?}: trivia {:?} is not whitespace",
                trivia.text()
            );
            rebuilt.push_str(trivia.text());
            cursor = trivia.span().end();
        }
        if token.is_missing() {
            assert_eq!(token.text(), "", "{source:?}");
            assert_eq!(token.span().len, 0, "{source:?}");
            assert_eq!(
                token.span().start,
                cursor,
                "{source:?}: missing token off the cursor"
            );
        } else {
            assert_eq!(
                token.span().start,
                cursor,
                "{source:?}: gap before {token:?}"
            );
            assert_eq!(
                utf16_slice(&units, token.span()),
                token.text(),
                "{source:?}"
            );
            rebuilt.push_str(token.text());
            cursor = token.span().end();
        }
        last_token = Some(token);
    }
    assert_eq!(
        cursor,
        units.len(),
        "{source:?}: the tokens stop short of the input"
    );
    let last = last_token.expect("a tree has the end-of-input token");
    assert_eq!(last.kind(), TokenKind::Eof, "{source:?}");
    assert_eq!(last.span(), TextSpan::new(units.len(), 0), "{source:?}");

    let mut all_nodes = Vec::new();
    nodes(tree.root(), &mut all_nodes);
    for node in all_nodes {
        let mut own = Vec::new();
        leaves(node, &mut own);
        let first = leaf_span(own.first().expect("a node has a leaf"));
        let last = leaf_span(own.last().expect("a node has a leaf"));
        assert_eq!(node.span().start, first.start, "{source:?}: {node:?}");
        assert_eq!(node.span().end(), last.end(), "{source:?}: {node:?}");
    }

    assert_eq!(rebuilt, source, "{source:?}: the test's own walk");
    assert_eq!(
        tree.source_text(),
        source,
        "{source:?}: SyntaxTree::source_text"
    );
    rebuilt
}

/// A compact rendering of an expression: `Kind(child child ...)`, tokens as their text (a
/// missing token as `?`), trivia left out.
fn shape(node: &SyntaxNode) -> String {
    let parts: Vec<String> = node
        .children()
        .iter()
        .map(|child| match child {
            SyntaxChild::Node(inner) => shape(inner),
            SyntaxChild::Token(token) if token.is_missing() => "?".to_string(),
            SyntaxChild::Token(token) => token.text().to_string(),
        })
        .collect();
    format!("{}({})", kind_name(node.kind()), parts.join(" "))
}

fn expression_shape(tree: &SyntaxTree) -> String {
    shape(tree.expression().expect("a root has an expression"))
}

fn record_kinds(
    tree: &SyntaxTree,
    syntax: &mut BTreeSet<&'static str>,
    tokens: &mut BTreeSet<String>,
) {
    let mut all = Vec::new();
    nodes(tree.root(), &mut all);
    for node in all {
        syntax.insert(kind_name(node.kind()));
    }
    for token in tree.tokens() {
        tokens.insert(format!("{:?}", token.kind()));
    }
    // Whitespace that is trivia is still a lexed whitespace token.
    if tree
        .tokens()
        .iter()
        .any(|token| !token.leading_trivia().is_empty())
    {
        tokens.insert(format!("{:?}", TokenKind::Whitespace));
    }
}

// ---------------------------------------------------------------------------------------------
// cfml_m1_1__lossless_roundtrip

/// Excel trim references, which this grammar does not parse: the formula, the trim operator the
/// one unsupported-syntax diagnostic must span, and the expression shape around it. Each still
/// reprints exactly.
const TRIM_REFERENCES: &[(&str, &str, &str)] = &[
    (
        "=SUM(A1.:B10)",
        ".:",
        "Call(SUM Args(( Unsupported(Id(A1) . : Id(B10)) )))",
    ),
    (
        "=SUM(A1:.B10)",
        ":.",
        "Call(SUM Args(( Unsupported(Id(A1) : . Id(B10)) )))",
    ),
    (
        "=SUM(A1.:.B10)",
        ".:.",
        "Call(SUM Args(( Unsupported(Id(A1) . : . Id(B10)) )))",
    ),
];

/// Well-formed formulas: each reprints exactly, parses without diagnostics and has the expression
/// shape given (Excel precedence, calls, literals, A1 references and sheet qualifiers).
const WELL_FORMED: &[(&str, &str)] = &[
    ("=1+2*3", "Binary(Num(1) + Binary(Num(2) * Num(3)))"),
    ("=-2^2", "Binary(Prefix(- Num(2)) ^ Num(2))"),
    ("=50%^2", "Binary(Postfix(Num(50) %) ^ Num(2))"),
    ("=1&2=3", "Binary(Binary(Num(1) & Num(2)) = Num(3))"),
    ("=10/4-1", "Binary(Binary(Num(10) / Num(4)) - Num(1))"),
    ("=-A1:B2", "Prefix(- Range(Id(A1) : Id(B2)))"),
    ("=A1<>B1", "Binary(Id(A1) <> Id(B1))"),
    (
        "=(A1<B1)+(A1<=B1)+(A1>B1)+(A1>=B1)",
        "Binary(Binary(Binary(Group(( Binary(Id(A1) < Id(B1)) )) + Group(( Binary(Id(A1) <= Id(B1)) ))) + Group(( Binary(Id(A1) > Id(B1)) ))) + Group(( Binary(Id(A1) >= Id(B1)) )))",
    ),
    (
        "=SUM(A1,Sheet2!B2)",
        "Call(SUM Args(( Id(A1) , Qualified(Sheet2 ! Id(B2)) )))",
    ),
    (
        "=  SUM( A1 ,\tSheet2!B2 )  ",
        "Call(SUM Args(( Id(A1) , Qualified(Sheet2 ! Id(B2)) )))",
    ),
    (
        "=IF(A1>0,\"say \"\"hi\"\"\",#DIV/0!)",
        "Call(IF Args(( Binary(Id(A1) > Num(0)) , Str(\"say \"\"hi\"\"\") , Err(#DIV/0!) )))",
    ),
    (
        "='My Sheet'!A1+'O''Brien''s Data'!$B$2",
        "Binary(Qualified('My Sheet' ! Id(A1)) + Qualified('O''Brien''s Data' ! Id($B$2)))",
    ),
    (
        "=IF(A1,,2)",
        "Call(IF Args(( Id(A1) , Omitted() , Num(2) )))",
    ),
    ("=NOW()", "Call(NOW Args(( )))"),
    (
        "=SUM(A1:B2 B1:C3,(A1,C3))",
        "Call(SUM Args(( Intersect(Range(Id(A1) : Id(B2))   Range(Id(B1) : Id(C3))) , Group(( Union(Id(A1) , Id(C3)) )) )))",
    ),
    (
        "={1,2;3,\"x\"}",
        "Array({ Num(1) , Num(2) ; Num(3) , Str(\"x\") })",
    ),
    ("=Sheet1:Sheet3!A1", "SheetSpan(Sheet1 : Sheet3 ! Id(A1))"),
    (
        "=LAMBDA(x,x+1)(2)",
        "Invoke(Call(LAMBDA Args(( Id(x) , Binary(Id(x) + Num(1)) ))) Args(( Num(2) )))",
    ),
    ("=@A1:A3#", "Prefix(@ Range(Id(A1) : Postfix(Id(A3) #)))"),
    (
        "=[Book.xlsx]Sheet2!A1",
        "Qualified([Book.xlsx]Sheet2 ! Id(A1))",
    ),
    ("SUM(A1,2.5E-3)", "Call(SUM Args(( Id(A1) , Num(2.5E-3) )))"),
    ("=1\n+\r\n2", "Binary(Num(1) + Num(2))"),
    (
        "=\"\u{1F600}x\"&売上&Größe",
        "Binary(Binary(Str(\"\u{1F600}x\") & Id(売上)) & Id(Größe))",
    ),
    ("='Q2 Data'", "QuotedId('Q2 Data')"),
    ("=TRUE", "Id(TRUE)"),
    (
        "=SUM(Sheet1!A1:Sheet1!B2)",
        "Call(SUM Args(( Range(Qualified(Sheet1 ! Id(A1)) : Qualified(Sheet1 ! Id(B2))) )))",
    ),
    (
        "=SUM(Sheet1!A:Sheet1!C)",
        "Call(SUM Args(( Range(Qualified(Sheet1 ! Id(A)) : Qualified(Sheet1 ! Id(C))) )))",
    ),
    (
        "='Annual Data'!A:'Annual Data'!C",
        "Range(Qualified('Annual Data' ! Id(A)) : Qualified('Annual Data' ! Id(C)))",
    ),
    ("=#REF!A1", "Qualified(#REF! Id(A1))"),
    (
        "=SUM(#REF!A1:B2)",
        "Call(SUM Args(( Range(Qualified(#REF! Id(A1)) : Id(B2)) )))",
    ),
];

/// Malformed or incomplete input: each reprints exactly and carries at least one diagnostic,
/// with the expression shape given.
const INCOMPLETE: &[(&str, &str)] = &[
    ("=SUM(A1,", "Call(SUM Args(( Id(A1) , ?))"),
    ("=SUM(", "Call(SUM Args(( ?))"),
    ("=SUM(A1 ", "Call(SUM Args(( Id(A1) ?))"),
    ("=\"abc", "Str(\"abc)"),
    ("=\"abc\"\"", "Str(\"abc\"\")"),
    ("='My Sheet", "QuotedId('My Sheet)"),
    ("=(1+", "Group(( Binary(Num(1) + Missing()) ?)"),
    ("=", "Missing()"),
    ("", "Missing()"),
    ("=Sheet1!", "Qualified(Sheet1 ! Missing())"),
    ("={1,2;", "Array({ Num(1) , Num(2) ; ?)"),
    ("=1+2)", "Binary(Num(1) + Num(2))"),
    ("=\u{1F600}*2", "Binary(Missing(\u{1F600}) * Num(2))"),
    ("=A1 : A3", "Range(Id(A1) : Id(A3))"),
    ("=#", "Missing(#)"),
];

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfml_m1_1__lossless_roundtrip() {
    let mut syntax_seen = BTreeSet::new();
    let mut tokens_seen = BTreeSet::new();
    let mut diagnostic_kinds = BTreeSet::new();

    for (source, expected) in WELL_FORMED {
        let tree = parse_formula(source);
        check_lossless(source, &tree);
        assert!(
            tree.diagnostics().is_empty(),
            "{source:?}: {:?}",
            tree.diagnostics()
        );
        assert_eq!(expression_shape(&tree), *expected, "{source:?}");
        record_kinds(&tree, &mut syntax_seen, &mut tokens_seen);
    }

    for (source, expected) in INCOMPLETE {
        let tree = parse_formula(source);
        check_lossless(source, &tree);
        assert!(
            !tree.diagnostics().is_empty(),
            "{source:?} is malformed but parsed without a diagnostic"
        );
        for diagnostic in tree.diagnostics() {
            assert_ne!(
                diagnostic.kind,
                SyntaxDiagnosticKind::UnsupportedSyntax,
                "{source:?}: Excel input reported as unsupported syntax"
            );
            diagnostic_kinds.insert(
                format!("{:?}", diagnostic.kind)
                    .split([' ', '{'])
                    .next()
                    .unwrap()
                    .to_string(),
            );
        }
        assert_eq!(expression_shape(&tree), *expected, "{source:?}");
        record_kinds(&tree, &mut syntax_seen, &mut tokens_seen);
    }

    // Unterminated quoted text is named as such, at the token's span.
    for (source, message, quoted) in [
        ("=\"abc", "unterminated string literal", "\"abc"),
        ("='My Sheet", "unterminated quoted name", "'My Sheet"),
    ] {
        let tree = parse_formula(source);
        let units: Vec<u16> = source.encode_utf16().collect();
        let found = tree
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.message == message)
            .unwrap_or_else(|| panic!("{source:?}: no {message:?} in {:?}", tree.diagnostics()));
        assert_eq!(utf16_slice(&units, found.span), quoted, "{source:?}");
    }

    // Trim references reprint exactly and get one unsupported-syntax diagnostic naming them,
    // spanning exactly the trim operator.
    for (source, operator, expected) in TRIM_REFERENCES {
        let tree = parse_formula(source);
        check_lossless(source, &tree);
        let units: Vec<u16> = source.encode_utf16().collect();
        assert_eq!(
            tree.diagnostics().len(),
            1,
            "{source:?}: {:?}",
            tree.diagnostics()
        );
        let diagnostic = &tree.diagnostics()[0];
        assert_eq!(
            diagnostic.kind,
            SyntaxDiagnosticKind::UnsupportedSyntax,
            "{source:?}"
        );
        assert!(
            diagnostic.message.contains("trim references"),
            "{source:?}: {}",
            diagnostic.message
        );
        assert_eq!(
            utf16_slice(&units, diagnostic.span),
            *operator,
            "{source:?}"
        );
        assert_eq!(expression_shape(&tree), *expected, "{source:?}");
    }

    // The qualifier of `#REF!A1` is the `#REF!` error literal, naming the #REF! error.
    let tree = parse_formula("=#REF!A1");
    let qualified = tree.expression().unwrap();
    assert_eq!(qualified.kind(), SyntaxKind::QualifiedReferenceExpr);
    match &qualified.children()[0] {
        SyntaxChild::Token(token) => {
            assert_eq!(token.kind(), TokenKind::ErrorLiteral);
            assert_eq!(token.error_code(), Some(WorksheetErrorCode::Ref));
        }
        SyntaxChild::Node(node) => panic!("the qualifier is a node: {node:?}"),
    }

    // The incomplete call keeps its tokens and reports the missing ')' where it belongs.
    let tree = parse_formula("=SUM(A1,");
    let missing: Vec<&Token> = tree
        .tokens()
        .into_iter()
        .filter(|token| token.is_missing())
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].kind(), TokenKind::RParen);
    assert_eq!(missing[0].span(), TextSpan::new(8, 0));
    assert_eq!(tree.diagnostics().len(), 1);
    assert_eq!(tree.diagnostics()[0].message, "expected ')'");
    assert_eq!(tree.diagnostics()[0].span, TextSpan::new(8, 0));

    // UTF-16 offsets: the emoji is two code units, so `&` after `"😀x"` starts at 6, not 5 (its
    // char offset) or 8 (its UTF-8 byte offset).
    let source = "=\"\u{1F600}x\"&売上&Größe";
    let tree = parse_formula(source);
    let ampersand = tree
        .tokens()
        .into_iter()
        .find(|token| token.kind() == TokenKind::Ampersand)
        .unwrap();
    assert_eq!(ampersand.span(), TextSpan::new(6, 1));
    let name = tree
        .tokens()
        .into_iter()
        .find(|token| token.text() == "Größe")
        .unwrap();
    assert_eq!(name.span(), TextSpan::new(10, 5));
    let string_node = tree
        .expression()
        .unwrap()
        .child_nodes()
        .next()
        .unwrap()
        .child_nodes()
        .next()
        .unwrap();
    assert_eq!(string_node.kind(), SyntaxKind::StringLiteralExpr);
    assert_eq!(string_node.span(), TextSpan::new(1, 5));

    // Each of the fourteen worksheet errors lexes from its spelling, in any ASCII case, as one
    // error literal naming its code.
    let spellings: [(&str, WorksheetErrorCode); 14] = [
        ("#NULL!", WorksheetErrorCode::Null),
        ("#DIV/0!", WorksheetErrorCode::Div0),
        ("#VALUE!", WorksheetErrorCode::Value),
        ("#REF!", WorksheetErrorCode::Ref),
        ("#NAME?", WorksheetErrorCode::Name),
        ("#NUM!", WorksheetErrorCode::Num),
        ("#N/A", WorksheetErrorCode::NA),
        ("#BUSY!", WorksheetErrorCode::Busy),
        ("#GETTING_DATA", WorksheetErrorCode::GettingData),
        ("#SPILL!", WorksheetErrorCode::Spill),
        ("#CALC!", WorksheetErrorCode::Calc),
        ("#FIELD!", WorksheetErrorCode::Field),
        ("#BLOCKED!", WorksheetErrorCode::Blocked),
        ("#CONNECT!", WorksheetErrorCode::Connect),
    ];
    let codes: BTreeSet<String> = spellings
        .iter()
        .map(|(_, code)| format!("{code:?}"))
        .collect();
    let all: BTreeSet<String> = WorksheetErrorCode::all()
        .iter()
        .map(|code| format!("{code:?}"))
        .collect();
    assert_eq!(codes, all, "the spellings cover every worksheet error code");
    for (spelling, code) in spellings {
        for text in [spelling.to_string(), spelling.to_ascii_lowercase()] {
            let source = format!("=IFERROR(1/0,{text})");
            let tree = parse_formula(&source);
            check_lossless(&source, &tree);
            assert!(tree.diagnostics().is_empty(), "{source:?}");
            let literal: Vec<&Token> = tree
                .tokens()
                .into_iter()
                .filter(|token| token.kind() == TokenKind::ErrorLiteral)
                .collect();
            assert_eq!(literal.len(), 1, "{source:?}");
            assert_eq!(literal[0].text(), text, "{source:?}");
            assert_eq!(literal[0].error_code(), Some(code), "{source:?}");
            record_kinds(&tree, &mut syntax_seen, &mut tokens_seen);
        }
    }

    // Coverage: every Excel node kind and every token kind except the `.` selector token
    // (exercised by cfml_m1_1__tree_syntax_rejected), and both Excel diagnostic kinds.
    let expected_syntax: BTreeSet<&str> = ALL_SYNTAX_KINDS
        .iter()
        .filter(|kind| **kind != SyntaxKind::UnsupportedExpr)
        .map(|kind| kind_name(*kind))
        .collect();
    assert_eq!(syntax_seen, expected_syntax, "node kinds exercised");
    let expected_tokens: BTreeSet<String> = ALL_TOKEN_KINDS
        .iter()
        .filter(|kind| **kind != TokenKind::Dot)
        .map(|kind| format!("{kind:?}"))
        .collect();
    assert_eq!(tokens_seen, expected_tokens, "token kinds exercised");
    assert_eq!(
        diagnostic_kinds,
        BTreeSet::from(["Message".to_string(), "SpacedRangeColon".to_string()]),
        "diagnostic kinds exercised"
    );

    note!(
        "ACCEPT-NOTE: cfml_m1_1__lossless_roundtrip: {} well-formed and {} malformed or incomplete formulas plus {} error-literal formulas reprinted exactly with UTF-16 spans tiling the input; {} trim-reference formulas reprinted exactly with one unsupported-syntax diagnostic each; {} node kinds and {} token kinds exercised",
        WELL_FORMED.len(),
        INCOMPLETE.len(),
        spellings.len() * 2,
        TRIM_REFERENCES.len(),
        syntax_seen.len(),
        tokens_seen.len()
    );
}

// ---------------------------------------------------------------------------------------------
// cfml_m1_1__tree_syntax_rejected

/// Legacy host-reference (tree) syntax: the formula, the exact source text the one
/// unsupported-syntax diagnostic must span, and the expression shape around it.
const TREE_SYNTAX: &[(&str, &str, &str)] = &[
    ("=^", "^", "Unsupported(^)"),
    ("=^^", "^^", "Unsupported(^ ^)"),
    ("=^.Value", "^.Value", "Unsupported(^ . Value)"),
    (
        "=^^.Total*2",
        "^^.Total",
        "Binary(Unsupported(^ ^ . Total) * Num(2))",
    ),
    ("=.Children", ".Children", "Unsupported(. Children)"),
    ("=.@Kids", ".@Kids", "Unsupported(. @ Kids)"),
    ("=. @ Kids", ". @ Kids", "Unsupported(. @ Kids)"),
    (
        "=Base.@CHILDREN",
        ".@CHILDREN",
        "Unsupported(Id(Base) . @ CHILDREN)",
    ),
    ("=Base.*", ".*", "Unsupported(Id(Base) . *)"),
    (
        "=(A1).Value",
        ".Value",
        "Unsupported(Group(( Id(A1) )) . Value)",
    ),
    (
        "=SUM(^.Value,1)",
        "^.Value",
        "Call(SUM Args(( Unsupported(^ . Value) , Num(1) )))",
    ),
    (
        "=SUM(A1:A3)+^.Cost",
        "^.Cost",
        "Binary(Call(SUM Args(( Range(Id(A1) : Id(A3)) ))) + Unsupported(^ . Cost))",
    ),
];

/// Excel syntax next to those tokens: each parses without any diagnostic and without an
/// unsupported node.
const EXCEL_NEIGHBOURS: &[(&str, &str)] = &[
    ("=2^3", "Binary(Num(2) ^ Num(3))"),
    ("=2^-1", "Binary(Num(2) ^ Prefix(- Num(1)))"),
    ("=@A1:A3", "Prefix(@ Range(Id(A1) : Id(A3)))"),
    ("=@Parent.Value", "Prefix(@ Id(Parent.Value))"),
    ("=Base.Value", "Id(Base.Value)"),
    ("=.5+1", "Binary(Num(.5) + Num(1))"),
    ("=Sheet1!A1#", "Postfix(Qualified(Sheet1 ! Id(A1)) #)"),
];

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfml_m1_1__tree_syntax_rejected() {
    for (source, rejected, expected) in TREE_SYNTAX {
        let tree = parse_formula(source);
        check_lossless(source, &tree);
        let units: Vec<u16> = source.encode_utf16().collect();

        assert_eq!(
            tree.diagnostics().len(),
            1,
            "{source:?}: {:?}",
            tree.diagnostics()
        );
        let diagnostic = &tree.diagnostics()[0];
        assert_eq!(
            diagnostic.kind,
            SyntaxDiagnosticKind::UnsupportedSyntax,
            "{source:?}"
        );
        assert!(
            diagnostic.message.starts_with("unsupported syntax: "),
            "{source:?}: {}",
            diagnostic.message
        );
        assert_eq!(
            utf16_slice(&units, diagnostic.span),
            *rejected,
            "{source:?}"
        );
        assert_eq!(diagnostic.suggested_correction(), None);
        assert_eq!(expression_shape(&tree), *expected, "{source:?}");

        // Every token inside the rejected span is a direct, unstructured token of an
        // unsupported node: none is wrapped in a name, reference or operator node.
        let mut walk = Vec::new();
        leaves(tree.root(), &mut walk);
        let mut inside = 0;
        for leaf in &walk {
            if let Leaf::Token { token, parent } = leaf
                && token.span().len > 0
                && token.span().start >= diagnostic.span.start
                && token.span().end() <= diagnostic.span.end()
            {
                assert_eq!(
                    *parent,
                    SyntaxKind::UnsupportedExpr,
                    "{source:?}: {token:?}"
                );
                inside += 1;
            }
        }
        assert!(inside >= 1, "{source:?}");
    }

    for (source, expected) in EXCEL_NEIGHBOURS {
        let tree = parse_formula(source);
        check_lossless(source, &tree);
        assert!(
            tree.diagnostics().is_empty(),
            "{source:?}: {:?}",
            tree.diagnostics()
        );
        assert_eq!(expression_shape(&tree), *expected, "{source:?}");
        let mut all = Vec::new();
        nodes(tree.root(), &mut all);
        assert!(
            all.iter()
                .all(|node| node.kind() != SyntaxKind::UnsupportedExpr),
            "{source:?}"
        );
    }

    note!(
        "ACCEPT-NOTE: cfml_m1_1__tree_syntax_rejected: {} legacy selector formulas each refused with one unsupported-syntax diagnostic spanning exactly the selector tokens, held unstructured in an UnsupportedExpr; {} neighbouring Excel formulas parsed without a diagnostic",
        TREE_SYNTAX.len(),
        EXCEL_NEIGHBOURS.len()
    );
}

// ---------------------------------------------------------------------------------------------
// cfml-m1.2: typed reference binding, placement and reads.

/// The cell an A1 address names, decoded here without calc-fml.
fn cell_at(sheet: SheetId, address: &str) -> CellKey {
    let digits = address
        .find(|ch: char| ch.is_ascii_digit())
        .expect("an address has a row");
    let (letters, row) = address.split_at(digits);
    let column = letters.bytes().fold(0u32, |number, letter| {
        number * 26 + u32::from(letter - b'A' + 1)
    });
    CellKey::new(sheet, row.parse().expect("a row number"), column).expect("a cell of the grid")
}

fn area_of(sheet: SheetId, first: &str, last: &str) -> GridArea {
    GridArea::new(cell_at(sheet, first), cell_at(sheet, last)).expect("one sheet")
}

fn one_cell(sheet: SheetId, address: &str) -> GridArea {
    GridArea::cell(cell_at(sheet, address))
}

fn anchor_at(sheet: SheetId, address: &str) -> FormulaAnchor {
    FormulaAnchor::new(cell_at(sheet, address))
}

/// A catalogue of named sheets that logs every request it receives. It matches names regardless
/// of case, as Excel matches sheet names.
struct LoggingCatalog {
    sheets: Vec<(&'static str, SheetId)>,
    requests: RefCell<Vec<String>>,
}

impl LoggingCatalog {
    fn new(names: &[&'static str]) -> Self {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        LoggingCatalog {
            sheets: names
                .iter()
                .map(|name| (*name, ids.issue().expect("a sheet id")))
                .collect(),
            requests: RefCell::new(Vec::new()),
        }
    }

    fn id(&self, name: &str) -> SheetId {
        self.sheets
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, id)| *id)
            .expect("a fixture sheet")
    }
}

impl ReferenceCatalog for LoggingCatalog {
    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        self.requests.borrow_mut().push(name.to_owned());
        self.sheets
            .iter()
            .find(|(known, _)| known.to_lowercase() == name.to_lowercase())
            .map(|(_, id)| *id)
    }
}

type Bound = Vec<(String, Result<ReferenceOperand, BindError>)>;

/// Binds `formula` and gives each site's source text, cut from the formula at the site's UTF-16
/// span, with its binding.
fn sites(
    formula: &str,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
) -> Bound {
    let tree = parse_formula(formula);
    assert!(
        tree.diagnostics().is_empty(),
        "{formula}: {:?}",
        tree.diagnostics()
    );
    tree_sites(formula, &tree, notation, anchor, catalog)
}

/// Binds the tree parsed from `formula`, which may hold diagnostics, as [`sites`] does.
fn tree_sites(
    formula: &str,
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
) -> Bound {
    let units: Vec<u16> = formula.encode_utf16().collect();
    bind_references(tree, notation, anchor, catalog)
        .into_iter()
        .map(|site| (utf16_slice(&units, site.span), site.binding))
        .collect()
}

fn texts(bound: &Bound) -> Vec<&str> {
    bound.iter().map(|(text, _)| text.as_str()).collect()
}

fn operands(bound: &Bound) -> Vec<ReferenceOperand> {
    bound
        .iter()
        .map(|(text, binding)| match binding {
            Ok(operand) => *operand,
            Err(refused) => panic!("{text} was refused: {refused:?}"),
        })
        .collect()
}

fn cell(row: AxisRef, column: AxisRef) -> ReferenceShape {
    ReferenceShape::Cell(CellRef { row, column })
}

fn on(sheet: SheetTarget, shape: ReferenceShape) -> ReferenceOperand {
    ReferenceOperand { sheet, shape }
}

/// The whole-column area from row 1 to the last row.
fn columns_of(sheet: SheetId, first: &str, last: &str) -> GridArea {
    area_of(sheet, &format!("{first}1"), &format!("{last}{MAX_ROWS}"))
}

/// The whole-row area from column A to the last column (XFD).
fn rows_of(sheet: SheetId, first: u32, last: u32) -> GridArea {
    area_of(sheet, &format!("A{first}"), &format!("XFD{last}"))
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfml_m1_2__relative_absolute() {
    use AxisRef::{Absolute as abs, Relative as rel};
    use ReferenceNotation::{A1, R1C1};
    let catalog = LoggingCatalog::new(&["Sheet1", "Sheet2", "Sheet3"]);
    let (s1, s2, s3) = (
        catalog.id("Sheet1"),
        catalog.id("Sheet2"),
        catalog.id("Sheet3"),
    );
    assert_eq!(cell_at(s1, "XFD1").column(), MAX_COLUMNS);
    let c5 = anchor_at(s1, "C5");
    let f10 = anchor_at(s1, "F10");
    let mut placements = 0;
    let mut refusals = 0;

    // Mixed dollar cells entered in C5: each relative row or column is an offset from C5.
    let mixed = sites("=B3+$B3+B$3+$B$3", A1, c5, &catalog);
    assert_eq!(texts(&mixed), ["B3", "$B3", "B$3", "$B$3"]);
    let mixed = operands(&mixed);
    assert_eq!(
        mixed,
        [
            on(SheetTarget::Anchor, cell(rel(-2), rel(-1))),
            on(SheetTarget::Anchor, cell(rel(-2), abs(2))),
            on(SheetTarget::Anchor, cell(abs(3), rel(-1))),
            on(SheetTarget::Anchor, cell(abs(3), abs(2))),
        ]
    );

    // Two callers: C5, where it was entered, and F10, five rows down and three columns right,
    // where the relative parts move with the caller and the absolute parts stay.
    for (caller, expected) in [
        (c5, ["B3", "B3", "B3", "B3"]),
        (f10, ["E8", "B8", "E3", "B3"]),
    ] {
        for (operand, address) in mixed.iter().zip(expected) {
            assert_eq!(operand.instantiate(caller), Ok(one_cell(s1, address)));
            placements += 1;
        }
    }

    // The text Excel shows for the copy in F10 binds there to the same operands, and so does the
    // R1C1 spelling at either caller; the original A1 text bound at F10 is another reference.
    let copied = sites("=E8+$B8+E$3+$B$3", A1, f10, &catalog);
    assert_eq!(operands(&copied), mixed);
    for caller in [c5, f10] {
        let r1c1 = sites("=R[-2]C[-1]+R[-2]C2+R3C[-1]+R3C2", R1C1, caller, &catalog);
        assert_eq!(operands(&r1c1), mixed);
    }
    let moved = sites("=B3", A1, f10, &catalog);
    assert_eq!(
        operands(&moved),
        [on(SheetTarget::Anchor, cell(rel(-7), rel(-4)))]
    );

    // On another sheet, an unqualified reference reads the caller's sheet and a qualified one
    // stays on its own.
    let qualified = operands(&sites("=Sheet2!B3+B3", A1, c5, &catalog));
    assert_eq!(
        qualified,
        [
            on(SheetTarget::Sheet(s2), cell(rel(-2), rel(-1))),
            on(SheetTarget::Anchor, cell(rel(-2), rel(-1))),
        ]
    );
    let sheet3_f10 = anchor_at(s3, "F10");
    assert_eq!(qualified[0].instantiate(sheet3_f10), Ok(one_cell(s2, "E8")));
    assert_eq!(qualified[1].instantiate(sheet3_f10), Ok(one_cell(s3, "E8")));
    placements += 2;

    // A rectangle, whole rows and whole columns, each with a relative and an absolute end, which
    // the operand holds absolute end first. At F10 the relative corner E8 passes the absolute D7,
    // and the area is D7:E8.
    let shapes = operands(&sites("=B3:$D$7+2:$4+B:$D", A1, c5, &catalog));
    assert_eq!(
        shapes,
        [
            on(
                SheetTarget::Anchor,
                ReferenceShape::Area {
                    first: CellRef {
                        row: abs(7),
                        column: abs(4)
                    },
                    last: CellRef {
                        row: rel(-2),
                        column: rel(-1)
                    },
                }
            ),
            on(
                SheetTarget::Anchor,
                ReferenceShape::Rows {
                    first: abs(4),
                    last: rel(-3)
                }
            ),
            on(
                SheetTarget::Anchor,
                ReferenceShape::Columns {
                    first: abs(4),
                    last: rel(-1)
                }
            ),
        ]
    );
    for (caller, expected) in [
        (
            c5,
            [
                area_of(s1, "B3", "D7"),
                rows_of(s1, 2, 4),
                columns_of(s1, "B", "D"),
            ],
        ),
        (
            f10,
            [
                area_of(s1, "D7", "E8"),
                rows_of(s1, 4, 7),
                columns_of(s1, "D", "E"),
            ],
        ),
    ] {
        for (operand, area) in shapes.iter().zip(expected) {
            assert_eq!(operand.instantiate(caller), Ok(area));
            placements += 1;
        }
    }

    // An operand does not depend on the anchor it was bound at. R[-2]C[-1]:R7C4 is B3:$D$7 at
    // C5, where its relative corner lies above and left of the absolute one, and $D$7:E8 at F10,
    // where it lies below and right; R[-3]:R4 is 2:$4 at C5 and $4:7 at F10. Bound at either
    // caller, in R1C1 or in either A1 order, each is one operand, which places at each caller.
    let at_c5 = operands(&sites("=R[-2]C[-1]:R7C4+R[-3]:R4", R1C1, c5, &catalog));
    let at_f10 = operands(&sites("=R[-2]C[-1]:R7C4+R[-3]:R4", R1C1, f10, &catalog));
    assert_eq!(at_c5, at_f10);
    assert_eq!(at_c5, shapes[..2]);
    for (formula, caller) in [
        ("=B3:$D$7+2:$4", c5),
        ("=$D$7:B3+$4:2", c5),
        ("=E8:$D$7+7:$4", f10),
        ("=$D$7:E8+$4:7", f10),
    ] {
        assert_eq!(
            operands(&sites(formula, A1, caller, &catalog)),
            at_c5,
            "{formula}"
        );
    }
    for (operands, caller, expected) in [
        (&at_c5, c5, [area_of(s1, "B3", "D7"), rows_of(s1, 2, 4)]),
        (&at_f10, f10, [area_of(s1, "D7", "E8"), rows_of(s1, 4, 7)]),
    ] {
        for (operand, area) in operands.iter().zip(expected) {
            assert_eq!(operand.instantiate(caller), Ok(area));
            placements += 1;
        }
    }

    // Sheet-qualified R1C1: a cell, a rectangle and a whole row on Sheet2, each bound at C5 and
    // placed at C5, at F10 and on Sheet3 at F10. The relative parts move with the caller, and the
    // sheet stays Sheet2.
    let qualified_r1c1 = [
        (
            "=Sheet2!R3C2",
            ReferenceShape::Cell(CellRef {
                row: abs(3),
                column: abs(2),
            }),
            [one_cell(s2, "B3"), one_cell(s2, "B3"), one_cell(s2, "B3")],
        ),
        (
            "=Sheet2!R[-1]C:R1C1",
            ReferenceShape::Area {
                first: CellRef {
                    row: abs(1),
                    column: abs(1),
                },
                last: CellRef {
                    row: rel(-1),
                    column: rel(0),
                },
            },
            [
                area_of(s2, "A1", "C4"),
                area_of(s2, "A1", "F9"),
                area_of(s2, "A1", "F9"),
            ],
        ),
        (
            "=Sheet2!R3",
            ReferenceShape::Rows {
                first: abs(3),
                last: abs(3),
            },
            [rows_of(s2, 3, 3), rows_of(s2, 3, 3), rows_of(s2, 3, 3)],
        ),
    ];
    for (formula, shape, areas) in qualified_r1c1 {
        let bound = sites(formula, R1C1, c5, &catalog);
        assert_eq!(texts(&bound), [&formula[1..]]);
        let operand = operands(&bound);
        assert_eq!(operand, [on(SheetTarget::Sheet(s2), shape)], "{formula}");
        for (caller, area) in [c5, f10, sheet3_f10].into_iter().zip(areas) {
            assert_eq!(operand[0].instantiate(caller), Ok(area), "{formula}");
            placements += 1;
        }
    }

    // R1C1 whole rows and columns, placed at F10.
    let axes = operands(&sites("=R[-1]:R[1]+C2:C[1]+R+C", R1C1, c5, &catalog));
    assert_eq!(
        axes,
        [
            on(
                SheetTarget::Anchor,
                ReferenceShape::Rows {
                    first: rel(-1),
                    last: rel(1)
                }
            ),
            on(
                SheetTarget::Anchor,
                ReferenceShape::Columns {
                    first: abs(2),
                    last: rel(1)
                }
            ),
            on(
                SheetTarget::Anchor,
                ReferenceShape::Rows {
                    first: rel(0),
                    last: rel(0)
                }
            ),
            on(
                SheetTarget::Anchor,
                ReferenceShape::Columns {
                    first: rel(0),
                    last: rel(0)
                }
            ),
        ]
    );
    for (operand, area) in axes.iter().zip([
        rows_of(s1, 9, 11),
        columns_of(s1, "B", "G"),
        rows_of(s1, 10, 10),
        columns_of(s1, "F", "F"),
    ]) {
        assert_eq!(operand.instantiate(f10), Ok(area));
        placements += 1;
    }

    // The `#REF!` qualifier binds, and placing it anywhere is #REF!.
    let deleted = operands(&sites("=#REF!B3+#REF!$B$3:C4", A1, c5, &catalog));
    assert_eq!(deleted.len(), 2);
    for operand in &deleted {
        assert_eq!(operand.sheet, SheetTarget::Deleted);
        for caller in [c5, f10] {
            let refused = operand.instantiate(caller);
            assert_eq!(refused, Err(PlacementError::DeletedSheet));
            assert_eq!(
                refused.unwrap_err().worksheet_error(),
                WorksheetErrorCode::Ref
            );
            refusals += 1;
        }
    }
    // `Sheet2!#REF!`, which Excel writes for a reference whose cells were deleted, binds on Sheet2
    // as a deleted reference, and placing it anywhere is #REF!. A range between two `#REF!`
    // qualified ends is one reference on the deleted sheet.
    let deleted_cells = sites("=Sheet2!#REF!+#REF!A1:#REF!B2", A1, c5, &catalog);
    assert_eq!(texts(&deleted_cells), ["Sheet2!#REF!", "#REF!A1:#REF!B2"]);
    let deleted_cells = operands(&deleted_cells);
    assert_eq!(
        deleted_cells,
        [
            on(SheetTarget::Sheet(s2), ReferenceShape::Deleted),
            on(
                SheetTarget::Deleted,
                ReferenceShape::Area {
                    first: CellRef {
                        row: rel(-4),
                        column: rel(-2)
                    },
                    last: CellRef {
                        row: rel(-3),
                        column: rel(-1)
                    },
                }
            ),
        ]
    );
    for (operand, expected) in deleted_cells
        .iter()
        .zip([PlacementError::DeletedCells, PlacementError::DeletedSheet])
    {
        for caller in [c5, f10, sheet3_f10] {
            let refused = operand.instantiate(caller);
            assert_eq!(refused, Err(expected), "{operand:?} at {caller:?}");
            assert_eq!(
                refused.unwrap_err().worksheet_error(),
                WorksheetErrorCode::Ref
            );
            refusals += 1;
        }
    }

    // Placed where a relative part leaves the grid, a reference is refused as #REF!: three of the
    // mixed cells and the three shapes at A1, D7 (entered in C5) at the last row and the last
    // column, and R[-1]C at row 1.
    let a1 = anchor_at(s1, "A1");
    let d7 = operands(&sites("=D7", A1, c5, &catalog))[0];
    let r1c1_up = operands(&sites("=R[-1]C", R1C1, anchor_at(s1, "D1"), &catalog))[0];
    let crossing = [
        (mixed[0], a1, PlacementError::RowOffGrid { resolved: -1 }),
        (mixed[1], a1, PlacementError::RowOffGrid { resolved: -1 }),
        (mixed[2], a1, PlacementError::ColumnOffGrid { resolved: 0 }),
        (shapes[0], a1, PlacementError::RowOffGrid { resolved: -1 }),
        (shapes[1], a1, PlacementError::RowOffGrid { resolved: -2 }),
        (shapes[2], a1, PlacementError::ColumnOffGrid { resolved: 0 }),
        (
            d7,
            anchor_at(s1, &format!("XFD{MAX_ROWS}")),
            PlacementError::RowOffGrid {
                resolved: i64::from(MAX_ROWS) + 2,
            },
        ),
        (
            d7,
            anchor_at(s1, "XFD1"),
            PlacementError::ColumnOffGrid {
                resolved: i64::from(MAX_COLUMNS) + 1,
            },
        ),
        (
            r1c1_up,
            anchor_at(s1, "D1"),
            PlacementError::RowOffGrid { resolved: 0 },
        ),
    ];
    for (operand, caller, expected) in crossing {
        let refused = operand.instantiate(caller);
        assert_eq!(refused, Err(expected), "{operand:?} at {caller:?}");
        assert_eq!(
            refused.unwrap_err().worksheet_error(),
            WorksheetErrorCode::Ref
        );
        refusals += 1;
    }
    // The absolute cell, and the same references one step inside the grid, still place.
    assert_eq!(mixed[3].instantiate(a1), Ok(one_cell(s1, "B3")));
    assert_eq!(
        r1c1_up.instantiate(anchor_at(s1, "D2")),
        Ok(one_cell(s1, "D1"))
    );
    assert_eq!(
        d7.instantiate(anchor_at(s1, &format!("XFC{}", MAX_ROWS - 2))),
        Ok(one_cell(s1, &format!("XFD{MAX_ROWS}")))
    );
    placements += 3;

    // Text that no caller puts on the grid is refused when it is bound.
    let outside_a1 = sites(
        "=$XFE$1+A$1048577+0:3+1:1048577+$XFE:$XFE",
        A1,
        c5,
        &catalog,
    );
    let outside_r1c1 = sites(
        "=R1048577C1+R0+C16385+R[1048576]C+R1C1:R1C16385",
        R1C1,
        c5,
        &catalog,
    );
    assert_eq!(
        texts(&outside_a1),
        ["$XFE$1", "A$1048577", "0:3", "1:1048577", "$XFE:$XFE"]
    );
    assert_eq!(
        texts(&outside_r1c1),
        ["R1048577C1", "R0", "C16385", "R[1048576]C", "R1C1:R1C16385"]
    );
    for (text, binding) in outside_a1.iter().chain(&outside_r1c1) {
        assert_eq!(binding, &Err(BindError::OutsideGrid), "{text}");
        refusals += 1;
    }
    // The longest offset the grid allows binds, and places only where it lands on the grid.
    let longest = operands(&sites("=R[-1048575]C", R1C1, c5, &catalog))[0];
    assert_eq!(
        longest.instantiate(anchor_at(s1, &format!("C{MAX_ROWS}"))),
        Ok(one_cell(s1, "C1"))
    );
    assert_eq!(
        longest.instantiate(c5),
        Err(PlacementError::RowOffGrid {
            resolved: 5 - 1_048_575
        })
    );
    // Without a `$`, cell-like text outside the grid is a name, which this binding leaves alone.
    assert!(sites("=XFE1+Sales2023+A0+XFE:XFE", A1, c5, &catalog).is_empty());
    // A number is no reference, even where its digits would name a row outside the grid: each of
    // these formulas gives the A1 site only.
    for formula in ["=A1+0", "=IF(A1>0,1,2)", "=A1*1048577"] {
        assert_eq!(
            sites(formula, A1, c5, &catalog),
            [(
                "A1".to_owned(),
                Ok(on(SheetTarget::Anchor, cell(rel(-4), rel(-2))))
            )],
            "{formula}"
        );
    }
    // In A1 notation a row or column alone is no reference, and with a `$` it is no name or
    // number either: each is refused, one outside the grid as such, without asking the catalogue.
    for (formula, refused) in [
        ("=$B", BindError::LoneRowOrColumn),
        ("=$3", BindError::LoneRowOrColumn),
        ("=$XFE", BindError::OutsideGrid),
        ("=Sheet2!$B", BindError::LoneRowOrColumn),
        ("=Sheet2!$3", BindError::LoneRowOrColumn),
    ] {
        assert_eq!(
            sites(formula, A1, c5, &catalog),
            [(formula[1..].to_owned(), Err(refused))],
            "{formula}"
        );
        refusals += 1;
    }
    // Only the Sheet2 qualifiers reached the catalogue, once per qualified reference bound.
    assert_eq!(*catalog.requests.borrow(), ["Sheet2"; 5]);

    note!(
        "ACCEPT-NOTE: cfml_m1_2__relative_absolute: {placements} placements at two or more callers gave the areas decoded by the case; {refusals} references refused (#REF! qualifier or deleted cells, off-grid placement, or outside the grid or a lone $ row or column at binding)"
    );
}

/// What a read's visit received for one run: each value with the address it was lent at, or a
/// count of blank or missing cells.
#[derive(Debug, PartialEq)]
enum Seen {
    Values(Vec<(usize, CalcValue)>),
    Blank(u64),
    Missing(u64),
}

/// A reader over stored values and unresolved cells; every other cell of a sheet is blank. It
/// logs each area it is asked for, lends stored values in place, and reports runs in row-major
/// order: one per stored or unresolved cell and one per stretch of blanks between them.
#[derive(Default)]
struct LoggingReader {
    values: BTreeMap<CellKey, CalcValue>,
    unresolved: BTreeSet<CellKey>,
    requests: Vec<GridArea>,
}

impl LoggingReader {
    fn store(&mut self, sheet: SheetId, address: &str, value: CalcValue) {
        self.values.insert(cell_at(sheet, address), value);
    }

    fn leave_unresolved(&mut self, sheet: SheetId, address: &str) {
        self.unresolved.insert(cell_at(sheet, address));
    }

    /// Where the reader keeps the value of a cell.
    fn address(&self, sheet: SheetId, address: &str) -> usize {
        &self.values[&cell_at(sheet, address)] as *const CalcValue as usize
    }
}

impl ReferenceReader for LoggingReader {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        self.requests.push(area);
        let first = area.first();
        let width = u64::from(area.column_count());
        let offset = |key: &CellKey| {
            u64::from(key.row() - first.row()) * width + u64::from(key.column() - first.column())
        };
        let mut marks: Vec<(u64, Option<&CalcValue>)> = self
            .values
            .iter()
            .filter(|(key, _)| area.contains(**key))
            .map(|(key, value)| (offset(key), Some(value)))
            .chain(
                self.unresolved
                    .iter()
                    .filter(|key| area.contains(**key))
                    .map(|key| (offset(key), None)),
            )
            .collect();
        marks.sort_by_key(|(at, _)| *at);
        let mut next = 0;
        for (at, mark) in marks {
            if at > next && sink(CellRun::Blank(at - next)).is_break() {
                return;
            }
            let run = match mark {
                Some(value) => CellRun::Values(std::slice::from_ref(value)),
                None => CellRun::Missing(1),
            };
            if sink(run).is_break() {
                return;
            }
            next = at + 1;
        }
        if next < area.cell_count() {
            let _ = sink(CellRun::Blank(area.cell_count() - next));
        }
    }
}

/// A reader over one block of stored values, kept row-major from `first` across `width`
/// columns, that lends them in runs of up to `chunk` values: a run continues from the end of one
/// row of the area to the start of the next. It serves areas across the block's columns.
struct BlockReader {
    first: CellKey,
    width: u32,
    values: Vec<CalcValue>,
    chunk: usize,
    requests: Vec<GridArea>,
}

impl ReferenceReader for BlockReader {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        self.requests.push(area);
        let start = area.first();
        assert_eq!(
            (start.sheet(), start.column(), area.column_count()),
            (self.first.sheet(), self.first.column(), self.width),
            "an area across the block's columns"
        );
        let skip = ((start.row() - self.first.row()) * self.width) as usize;
        let cells = &self.values[skip..skip + area.cell_count() as usize];
        for run in cells.chunks(self.chunk) {
            if sink(CellRun::Values(run)).is_break() {
                return;
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfml_m1_2__provider_reads() {
    let catalog = LoggingCatalog::new(&["Sheet1", "Sheet2", "Q2 Data", "It's"]);
    let (s1, s2) = (catalog.id("Sheet1"), catalog.id("Sheet2"));
    let (q2, its) = (catalog.id("Q2 Data"), catalog.id("It's"));
    let anchor = anchor_at(s1, "D10");

    let formula = "=SUM(sheet2!$B$2:C3,'Q2 Data'!A1,'It''s'!B2:B3,B2:B4,Sheet2!B:B,Missing!A1,[Book2.xlsx]Sheet1!A1,Sheet1:Sheet3!A1,'Sheet 1:Sheet 3'!A1,Sheet2!TaxRate)";
    let bound = sites(formula, ReferenceNotation::A1, anchor, &catalog);

    // The catalogue was asked for sheets only, by the names the qualifiers spell, unquoted and
    // as written: never for reference text, and not for the other workbook, the runs of sheets
    // or the sheet-qualified name.
    assert_eq!(
        *catalog.requests.borrow(),
        ["sheet2", "Q2 Data", "It's", "Sheet2", "Missing"]
    );
    assert_eq!(
        texts(&bound),
        [
            "sheet2!$B$2:C3",
            "'Q2 Data'!A1",
            "'It''s'!B2:B3",
            "B2:B4",
            "Sheet2!B:B",
            "Missing!A1",
            "[Book2.xlsx]Sheet1!A1",
            "Sheet1:Sheet3!A1",
            "'Sheet 1:Sheet 3'!A1",
        ]
    );
    let sheets: Vec<Result<SheetTarget, BindError>> = bound
        .iter()
        .map(|(_, binding)| binding.clone().map(|operand| operand.sheet))
        .collect();
    assert_eq!(
        sheets,
        [
            Ok(SheetTarget::Sheet(s2)),
            Ok(SheetTarget::Sheet(q2)),
            Ok(SheetTarget::Sheet(its)),
            Ok(SheetTarget::Anchor),
            Ok(SheetTarget::Sheet(s2)),
            Err(BindError::UnknownSheet {
                name: "Missing".to_owned()
            }),
            Err(BindError::ExternalWorkbook),
            Err(BindError::SheetSpan),
            Err(BindError::SheetSpan),
        ]
    );

    // A qualifier belongs to its whole operand. Before an operand that is no reference endpoint
    // it is refused as one site spanning both, without a catalogue request, so nothing inside
    // binds as a reference of the anchor's sheet or of an inner qualifier's.
    let asked = catalog.requests.borrow().len();
    for formula in [
        "=Sheet2!(A1)",
        "=[Book2.xlsx]Sheet1!(A1)",
        "=Sheet1:Sheet3!(A1)",
        "=Nope!(A1)",
        "=Sheet2!Sheet3!A1",
        "=#REF!(A1)",
        "=#REF!SUM(A1)",
        "=Sheet2!SUM(A1)",
        "=Sheet2!3",
    ] {
        assert_eq!(
            sites(formula, ReferenceNotation::A1, anchor, &catalog),
            [(
                formula[1..].to_owned(),
                Err(BindError::QualifiedNonReference)
            )],
            "{formula}"
        );
    }
    // A range whose ends share a qualifier naming another workbook, or whose first end is a run
    // of sheets, is one refused site.
    for (formula, refused) in [
        ("=[B.xlsx]S!A1:[B.xlsx]S!B2", BindError::ExternalWorkbook),
        ("=Sheet1:Sheet3!A1:Sheet3!B2", BindError::SheetSpan),
    ] {
        assert_eq!(
            sites(formula, ReferenceNotation::A1, anchor, &catalog),
            [(formula[1..].to_owned(), Err(refused))],
            "{formula}"
        );
    }
    // An array constant holds constants only: each reference inside one is refused, a qualified
    // one without asking the catalogue, and the constants and the name give no site.
    assert_eq!(
        sites("={A1}", ReferenceNotation::A1, anchor, &catalog),
        [("A1".to_owned(), Err(BindError::InArrayConstant))]
    );
    assert_eq!(
        sites(
            "=SUM({1,Sheet2!B3;A1:B2,TRUE},Sheet2!TaxRate)+C1",
            ReferenceNotation::A1,
            anchor,
            &catalog
        ),
        [
            ("Sheet2!B3".to_owned(), Err(BindError::InArrayConstant)),
            ("A1:B2".to_owned(), Err(BindError::InArrayConstant)),
            (
                "C1".to_owned(),
                Ok(on(
                    SheetTarget::Anchor,
                    cell(AxisRef::Relative(-9), AxisRef::Relative(-1))
                ))
            ),
        ]
    );
    // Syntax the parser refused is one refused site, and the references inside it are not bound:
    // neither Sheet1!A1 nor B10 of the trim range, nor A1 under the member selector. C1 binds.
    for (formula, refused, after) in [
        ("=SUM(Sheet1!A1.:B10)+C1", "Sheet1!A1.:B10", "C1"),
        ("=(A1).Value+C1", "(A1).Value", "C1"),
    ] {
        let tree = parse_formula(formula);
        let kinds: Vec<SyntaxDiagnosticKind> = tree
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.kind.clone())
            .collect();
        assert_eq!(
            kinds,
            [SyntaxDiagnosticKind::UnsupportedSyntax],
            "{formula}"
        );
        assert_eq!(
            tree_sites(formula, &tree, ReferenceNotation::A1, anchor, &catalog),
            [
                (refused.to_owned(), Err(BindError::UnsupportedSyntax)),
                (
                    after.to_owned(),
                    Ok(on(
                        SheetTarget::Anchor,
                        cell(AxisRef::Relative(-9), AxisRef::Relative(-1))
                    ))
                ),
            ],
            "{formula}"
        );
    }
    assert_eq!(catalog.requests.borrow().len(), asked);

    // Between two sheet qualifiers each is asked of the catalogue once. On one sheet the range is
    // one site; across two sheets, or where the catalogue knows no sheet of one qualifier, each
    // end is a site of its own, bound or refused alone.
    let d10 = |row: i32, column: i32| cell(AxisRef::Relative(row), AxisRef::Relative(column));
    let two_ends: [(&str, [&str; 2], Bound); 3] = [
        (
            "=Sheet2!B3:sheet2!D7",
            ["Sheet2", "sheet2"],
            vec![(
                "Sheet2!B3:sheet2!D7".to_owned(),
                Ok(on(
                    SheetTarget::Sheet(s2),
                    ReferenceShape::Area {
                        first: CellRef {
                            row: AxisRef::Relative(-7),
                            column: AxisRef::Relative(-2),
                        },
                        last: CellRef {
                            row: AxisRef::Relative(-3),
                            column: AxisRef::Relative(0),
                        },
                    },
                )),
            )],
        ),
        (
            "=Sheet2!A1:Sheet1!B2",
            ["Sheet2", "Sheet1"],
            vec![
                (
                    "Sheet2!A1".to_owned(),
                    Ok(on(SheetTarget::Sheet(s2), d10(-9, -3))),
                ),
                (
                    "Sheet1!B2".to_owned(),
                    Ok(on(SheetTarget::Sheet(s1), d10(-8, -2))),
                ),
            ],
        ),
        (
            "=Nope!B3:Sheet2!D7",
            ["Nope", "Sheet2"],
            vec![
                (
                    "Nope!B3".to_owned(),
                    Err(BindError::UnknownSheet {
                        name: "Nope".to_owned(),
                    }),
                ),
                (
                    "Sheet2!D7".to_owned(),
                    Ok(on(SheetTarget::Sheet(s2), d10(-3, 0))),
                ),
            ],
        ),
    ];
    for (formula, requests, expected) in two_ends {
        let before = catalog.requests.borrow().len();
        assert_eq!(
            sites(formula, ReferenceNotation::A1, anchor, &catalog),
            expected,
            "{formula}"
        );
        assert_eq!(catalog.requests.borrow()[before..], requests, "{formula}");
    }

    // Sheet2: B2 and C2 hold values, B3 is blank, C3 is an input left unresolved, and B5 holds a
    // value further down column B. Sheet1's B4 is blank; It's B3 is unresolved.
    let mut reader = LoggingReader::default();
    reader.store(s2, "B2", CalcValue::number(1.0));
    reader.store(
        s2,
        "C2",
        CalcValue::text(ExcelText::from_interop_assignment("x")),
    );
    reader.leave_unresolved(s2, "C3");
    reader.store(s2, "B5", CalcValue::number(5.0));
    reader.store(q2, "A1", CalcValue::logical(true));
    reader.store(its, "B2", CalcValue::number(2.0));
    reader.leave_unresolved(its, "B3");
    reader.store(s1, "B2", CalcValue::number(10.0));
    reader.store(s1, "B3", CalcValue::number(20.0));

    // Each bound reference, placed at the anchor and read through calc-fml.
    let mut reads: Vec<Vec<(CellKey, Seen)>> = Vec::new();
    for (text, binding) in &bound {
        let Ok(operand) = binding else { continue };
        let area = operand
            .instantiate(anchor)
            .unwrap_or_else(|refused| panic!("{text}: {refused:?}"));
        let mut runs = Vec::new();
        read_cells(&mut reader, area, &mut |start, run| {
            runs.push((
                start,
                match run {
                    CellRun::Values(values) => Seen::Values(
                        values
                            .iter()
                            .map(|value| (value as *const CalcValue as usize, value.clone()))
                            .collect(),
                    ),
                    CellRun::Blank(count) => Seen::Blank(count),
                    CellRun::Missing(count) => Seen::Missing(count),
                },
            ));
            ControlFlow::Continue(())
        })
        .unwrap_or_else(|broken| panic!("{text}: {broken:?}"));
        reads.push(runs);
    }

    // The reader was asked for typed areas, one per bound reference.
    assert_eq!(
        reader.requests,
        [
            area_of(s2, "B2", "C3"),
            one_cell(q2, "A1"),
            area_of(its, "B2", "B3"),
            area_of(s1, "B2", "B4"),
            columns_of(s2, "B", "B"),
        ]
    );

    // Values arrive lent from the reader's own storage; blank and unresolved cells arrive as
    // counts, kept apart; every run starts at the cell calc-fml reports for it.
    let lent = |sheet: SheetId, address: &str, value: CalcValue| {
        Seen::Values(vec![(reader.address(sheet, address), value)])
    };
    let expected = vec![
        vec![
            (cell_at(s2, "B2"), lent(s2, "B2", CalcValue::number(1.0))),
            (
                cell_at(s2, "C2"),
                lent(
                    s2,
                    "C2",
                    CalcValue::text(ExcelText::from_interop_assignment("x")),
                ),
            ),
            (cell_at(s2, "B3"), Seen::Blank(1)),
            (cell_at(s2, "C3"), Seen::Missing(1)),
        ],
        vec![(cell_at(q2, "A1"), lent(q2, "A1", CalcValue::logical(true)))],
        vec![
            (cell_at(its, "B2"), lent(its, "B2", CalcValue::number(2.0))),
            (cell_at(its, "B3"), Seen::Missing(1)),
        ],
        vec![
            (cell_at(s1, "B2"), lent(s1, "B2", CalcValue::number(10.0))),
            (cell_at(s1, "B3"), lent(s1, "B3", CalcValue::number(20.0))),
            (cell_at(s1, "B4"), Seen::Blank(1)),
        ],
        // The whole column in five runs, its last 1048571 blank cells as one count.
        vec![
            (cell_at(s2, "B1"), Seen::Blank(1)),
            (cell_at(s2, "B2"), lent(s2, "B2", CalcValue::number(1.0))),
            (cell_at(s2, "B3"), Seen::Blank(2)),
            (cell_at(s2, "B5"), lent(s2, "B5", CalcValue::number(5.0))),
            (cell_at(s2, "B6"), Seen::Blank(u64::from(MAX_ROWS) - 5)),
        ],
    ];
    assert_eq!(reads, expected);

    // Every cell is accounted for: values, blanks and missing cells add up to each area's size.
    let mut counted = [0u64; 3];
    for (runs, area) in reads.iter().zip(&reader.requests) {
        let mut cells = 0u64;
        for (_, seen) in runs {
            let (slot, count) = match seen {
                Seen::Values(values) => (0, values.len() as u64),
                Seen::Blank(count) => (1, *count),
                Seen::Missing(count) => (2, *count),
            };
            counted[slot] += count;
            cells += count;
        }
        assert_eq!(cells, area.cell_count());
    }

    // A reader that lends several values per run. Sheet1's B7:D9 holds 1 to 9 row by row, lent
    // four at a time, so runs continue from the end of one row of the area to the start of the
    // next. Each run starts at the cell calc-fml reports, and the case places each value at its
    // own cell from that start by its own row-major count.
    let mut block = BlockReader {
        first: cell_at(s1, "B7"),
        width: 3,
        values: (1..=9).map(|n| CalcValue::number(f64::from(n))).collect(),
        chunk: 4,
        requests: Vec::new(),
    };
    let wrapped = operands(&sites(
        "=B7:D9+B8:D9",
        ReferenceNotation::A1,
        anchor,
        &catalog,
    ));
    let mut wrapped_reads = Vec::new();
    for operand in &wrapped {
        let area = operand.instantiate(anchor).expect("on the grid at D10");
        let first = area.first();
        let mut starts = Vec::new();
        let mut cells = Vec::new();
        read_cells(&mut block, area, &mut |start, run| {
            let CellRun::Values(values) = run else {
                panic!("the block reader lends values only: {run:?}");
            };
            starts.push((start, values.len()));
            let offset = (start.row() - first.row()) * 3 + (start.column() - first.column());
            for (index, value) in values.iter().enumerate() {
                let at = offset + index as u32;
                let cell =
                    CellKey::new(first.sheet(), first.row() + at / 3, first.column() + at % 3)
                        .expect("a cell of the area");
                cells.push((cell, value as *const CalcValue as usize, value.clone()));
            }
            ControlFlow::Continue(())
        })
        .unwrap_or_else(|broken| panic!("{operand:?}: {broken:?}"));
        wrapped_reads.push((starts, cells));
    }
    assert_eq!(
        block.requests,
        [area_of(s1, "B7", "D9"), area_of(s1, "B8", "D9")]
    );
    let lent_at = |addresses: &[&str], from: usize| -> Vec<(CellKey, usize, CalcValue)> {
        addresses
            .iter()
            .enumerate()
            .map(|(index, address)| {
                let value = &block.values[from + index];
                (
                    cell_at(s1, address),
                    value as *const CalcValue as usize,
                    value.clone(),
                )
            })
            .collect()
    };
    assert_eq!(
        wrapped_reads,
        [
            (
                vec![
                    (cell_at(s1, "B7"), 4),
                    (cell_at(s1, "C8"), 4),
                    (cell_at(s1, "D9"), 1)
                ],
                lent_at(&["B7", "C7", "D7", "B8", "C8", "D8", "B9", "C9", "D9"], 0),
            ),
            (
                vec![(cell_at(s1, "B8"), 4), (cell_at(s1, "C9"), 2)],
                lent_at(&["B8", "C8", "D8", "B9", "C9", "D9"], 3),
            ),
        ]
    );

    note!(
        "ACCEPT-NOTE: cfml_m1_2__provider_reads: the catalogue was asked for {} sheet names and the reader for {} typed areas; {} values lent in place, {} blank and {} missing cells counted apart; {} values lent in {} runs of up to four cells that continue across row ends",
        catalog.requests.borrow().len(),
        reader.requests.len(),
        counted[0],
        counted[1],
        counted[2],
        wrapped_reads
            .iter()
            .map(|(_, cells)| cells.len())
            .sum::<usize>(),
        wrapped_reads
            .iter()
            .map(|(starts, _)| starts.len())
            .sum::<usize>()
    );
}

// ---------------------------------------------------------------------------------------------
// cfml-m1.3: compiled plans, placed instances and evaluation frames.

fn text_value(text: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(text))
}

/// Compiles `formula`, entered at `anchor` in A1 notation, through calc-fml's production route.
fn compiled(
    formula: &str,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
) -> Arc<CompiledPlan> {
    compile(
        &parse_formula(formula),
        ReferenceNotation::A1,
        anchor,
        catalog,
        cache,
    )
    .unwrap_or_else(|refused| panic!("{formula}: {refused:?}"))
}

/// The cfml-m1.3 cases' reader: stored values and unresolved cells; every other cell of a sheet
/// is blank. It logs each area it is asked for, and each run of values it reports. Runs follow
/// row-major order over the area: each stretch of consecutive stored cells is one run of values,
/// which crosses from the end of a row of the area to the start of the next when both are stored;
/// each unresolved cell is a missing run of one, and each stretch of blanks a blank run.
#[derive(Default)]
struct RunReader {
    values: BTreeMap<CellKey, CalcValue>,
    unresolved: BTreeSet<CellKey>,
    requests: Vec<GridArea>,
    /// Each run of values reported: the area asked for, the run's first cell and its values.
    value_runs: Vec<(GridArea, CellKey, Vec<CalcValue>)>,
}

impl RunReader {
    fn store(&mut self, sheet: SheetId, address: &str, value: CalcValue) {
        self.values.insert(cell_at(sheet, address), value);
    }

    fn leave_unresolved(&mut self, sheet: SheetId, address: &str) {
        self.unresolved.insert(cell_at(sheet, address));
    }
}

impl ReferenceReader for RunReader {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        self.requests.push(area);
        let first = area.first();
        let width = u64::from(area.column_count());
        let offset = |key: &CellKey| {
            u64::from(key.row() - first.row()) * width + u64::from(key.column() - first.column())
        };
        let cell_at_offset = |at: u64| {
            let row = first.row() + u32::try_from(at / width).expect("a row");
            let column = first.column() + u32::try_from(at % width).expect("a column");
            CellKey::new(first.sheet(), row, column).expect("a cell of the area")
        };
        let mut marks: Vec<(u64, Option<CalcValue>)> = self
            .values
            .iter()
            .filter(|(key, _)| area.contains(**key))
            .map(|(key, value)| (offset(key), Some(value.clone())))
            .chain(
                self.unresolved
                    .iter()
                    .filter(|key| area.contains(**key))
                    .map(|key| (offset(key), None)),
            )
            .collect();
        marks.sort_by_key(|(at, _)| *at);
        let mut next = 0;
        let mut index = 0;
        while let Some((at, mark)) = marks.get(index) {
            let at = *at;
            if at > next && sink(CellRun::Blank(at - next)).is_break() {
                return;
            }
            if mark.is_none() {
                index += 1;
                next = at + 1;
                if sink(CellRun::Missing(1)).is_break() {
                    return;
                }
                continue;
            }
            let mut run = Vec::new();
            while let Some((cell, Some(value))) = marks.get(index) {
                if *cell != at + run.len() as u64 {
                    break;
                }
                run.push(value.clone());
                index += 1;
            }
            next = at + run.len() as u64;
            self.value_runs
                .push((area, cell_at_offset(at), run.clone()));
            if sink(CellRun::Values(&run)).is_break() {
                return;
            }
        }
        if next < area.cell_count() {
            let _ = sink(CellRun::Blank(area.cell_count() - next));
        }
    }
}

/// One evaluation of `instance` in a frame made for it, under the Fast numeric mode, with the
/// reader's logs cleared first: the result and the areas the reader was asked for. The runs of
/// values the reader reported stay in its log.
fn evaluated(
    instance: &FormulaInstance,
    reader: &mut RunReader,
    scratch: &mut EvalScratch,
) -> (Result<CalcValue, EvalFault>, Vec<GridArea>) {
    reader.requests.clear();
    reader.value_runs.clear();
    let mode = NumericMode::Fast;
    let result = evaluate(
        EvalFrame::new(instance, scratch),
        &mut EvaluationServices::new(&mode, reader),
    );
    (result, std::mem::take(&mut reader.requests))
}

/// An expected evaluation: the result and the areas read, in order.
type Expected = (Result<CalcValue, EvalFault>, Vec<GridArea>);

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfml_m1_3__reuse_plan() {
    // A plan and its instances are shared values: Send + Sync, checked by the compiler here.
    fn shared<T: Send + Sync>() {}
    shared::<CompiledPlan>();
    shared::<FormulaInstance>();

    let catalog = LoggingCatalog::new(&["Sheet1", "Sheet2"]);
    let (s1, s2) = (catalog.id("Sheet1"), catalog.id("Sheet2"));
    let c10 = anchor_at(s1, "C10");
    let mut cache = PlanCache::new();
    let mut scratch = EvalScratch::new();
    let mut reader = RunReader::default();

    // The provider's first state. Sheet1!A4 and Sheet2!B3 are blank.
    reader.store(s1, "A1", CalcValue::number(1.0));
    reader.store(s1, "A2", CalcValue::number(2.0));
    reader.store(s1, "A3", CalcValue::number(3.0));
    reader.store(s1, "B1", CalcValue::number(5.0));
    reader.store(s2, "A1", text_value("a"));
    reader.store(s2, "B1", CalcValue::number(10.0));
    reader.store(s2, "B2", CalcValue::number(20.0));

    // Each formula with its hand-derived result in the first state and in the second, where
    // Sheet1!A2 is 10, Sheet1!B1 is -1, Sheet1!A4 holds "t", Sheet2!A1 holds "b" and Sheet2!B2
    // is 25.
    let a1_to_a3 = area_of(s1, "A1", "A3");
    let power = catalogue()
        .resolve_surface(Notation::Infix, "^")
        .expect("the catalogue holds ^");
    let table: Vec<(&str, Expected, Expected)> = vec![
        (
            "=1+2*3",
            (Ok(CalcValue::number(7.0)), vec![]),
            (Ok(CalcValue::number(7.0)), vec![]),
        ),
        (
            "=SUM(A1:A3)*2",
            (Ok(CalcValue::number(12.0)), vec![a1_to_a3]),
            (Ok(CalcValue::number(28.0)), vec![a1_to_a3]),
        ),
        (
            "=IF(B1>0,\"yes\",1/0)",
            (Ok(text_value("yes")), vec![one_cell(s1, "B1")]),
            (
                Ok(CalcValue::error(WorksheetErrorCode::Div0)),
                vec![one_cell(s1, "B1")],
            ),
        ),
        (
            "=Sheet2!A1&\"x\"",
            (Ok(text_value("ax")), vec![one_cell(s2, "A1")]),
            (Ok(text_value("bx")), vec![one_cell(s2, "A1")]),
        ),
        (
            "=SUM(Sheet2!B1:B3)-A1",
            (
                Ok(CalcValue::number(29.0)),
                vec![area_of(s2, "B1", "B3"), one_cell(s1, "A1")],
            ),
            (
                Ok(CalcValue::number(34.0)),
                vec![area_of(s2, "B1", "B3"), one_cell(s1, "A1")],
            ),
        ),
        (
            "=A4",
            (Ok(CalcValue::number(0.0)), vec![one_cell(s1, "A4")]),
            (Ok(text_value("t")), vec![one_cell(s1, "A4")]),
        ),
        // calc-func does not model 5^0.5 and refuses it; -1^0.5 is #NUM!.
        (
            "=B1^0.5",
            (
                Err(EvalFault::Function(FunctionFault::NotImplemented {
                    id: power.id(),
                    argument: None,
                })),
                vec![one_cell(s1, "B1")],
            ),
            (
                Ok(CalcValue::error(WorksheetErrorCode::Num)),
                vec![one_cell(s1, "B1")],
            ),
        ),
        // A reference to three cells where + needs one value: implicit intersection is refused.
        (
            "=A1:A3+1",
            (Err(EvalFault::AreaAsValue { area: a1_to_a3 }), vec![]),
            (Err(EvalFault::AreaAsValue { area: a1_to_a3 }), vec![]),
        ),
    ];

    // Compile each formula once and place it at its anchor.
    let plans: Vec<Arc<CompiledPlan>> = table
        .iter()
        .map(|(formula, _, _)| compiled(formula, c10, &catalog, &mut cache))
        .collect();
    let instances: Vec<FormulaInstance> = plans.iter().map(|plan| instantiate(plan, c10)).collect();
    // Compilation asked the catalogue for the two Sheet2 qualifiers.
    let asked_by_compile = catalog.requests.borrow().clone();
    assert_eq!(asked_by_compile, ["Sheet2", "Sheet2"]);
    // The instance places each operand at the anchor: SUM(Sheet2!B1:B3)-A1 reads these areas.
    assert_eq!(
        instances[4].areas(),
        [Ok(area_of(s2, "B1", "B3")), Ok(one_cell(s1, "A1"))]
    );
    assert_eq!(plans[4].references().len(), 2);

    // Two evaluations of every instance, with the provider's data changed between them.
    let mut first = Vec::new();
    for (instance, (formula, expected, _)) in instances.iter().zip(&table) {
        let seen = evaluated(instance, &mut reader, &mut scratch);
        assert_eq!(&seen, expected, "first evaluation of {formula}");
        first.push(seen.0);
    }
    reader.store(s1, "A2", CalcValue::number(10.0));
    reader.store(s1, "B1", CalcValue::number(-1.0));
    reader.store(s1, "A4", text_value("t"));
    reader.store(s2, "A1", text_value("b"));
    reader.store(s2, "B2", CalcValue::number(25.0));
    let mut changed = 0;
    for ((instance, (formula, _, expected)), before) in instances.iter().zip(&table).zip(&first) {
        let seen = evaluated(instance, &mut reader, &mut scratch);
        assert_eq!(&seen, expected, "second evaluation of {formula}");
        if &seen.0 != before {
            changed += 1;
        }
    }
    // Six of the eight read data that changed, and each of them gave a different result.
    assert_eq!(changed, 6);

    // Neither evaluation compiled. That rests on the API's structure: a CompiledPlan holds only
    // its instruction blocks and reference operands, and evaluate receives an instance, a frame
    // and the services (a numeric mode and a reader), with no formula text, syntax tree,
    // catalogue or PlanCache to compile from. What the case observes agrees with it: the
    // catalogue, which compiling a qualified reference asks, was not asked again, and each
    // instance still runs the plan compile returned, one allocation held by the cache, the case's
    // handle and the instance.
    assert_eq!(*catalog.requests.borrow(), asked_by_compile);
    for (plan, instance) in plans.iter().zip(&instances) {
        assert!(Arc::ptr_eq(plan, instance.plan()));
        assert_eq!(Arc::strong_count(plan), 3);
    }

    // The reader reports consecutive stored cells as one run of values, and the kernels receive
    // them value by value through the CellReader bridge. SUM(A1:A3) reads one run of three values
    // down a column, crossing two row ends: 1, 10 and 3 in the second state, whose sum 14 (28
    // doubled) shows each was visited once.
    let (result, _) = evaluated(&instances[1], &mut reader, &mut scratch);
    assert_eq!(result, Ok(CalcValue::number(28.0)));
    assert_eq!(
        reader.value_runs,
        [(
            a1_to_a3,
            cell_at(s1, "A1"),
            vec![
                CalcValue::number(1.0),
                CalcValue::number(10.0),
                CalcValue::number(3.0)
            ]
        )]
    );
    // A two-column area whose stored cells run across a row end: E1, F1, E2 and F2 hold 1, 2, 4
    // and 8, one run of four values; E3 is blank and F3 holds 16, a run of one. Each power of two
    // reaches the sum once. In H1:I2 the run of four holds 1, #DIV/0!, #N/A and 2: SUM's first
    // error in row-major order is #DIV/0! (in column order it would be #N/A).
    for (address, value) in [
        ("E1", CalcValue::number(1.0)),
        ("F1", CalcValue::number(2.0)),
        ("E2", CalcValue::number(4.0)),
        ("F2", CalcValue::number(8.0)),
        ("F3", CalcValue::number(16.0)),
        ("H1", CalcValue::number(1.0)),
        ("I1", CalcValue::error(WorksheetErrorCode::Div0)),
        ("H2", CalcValue::error(WorksheetErrorCode::NA)),
        ("I2", CalcValue::number(2.0)),
    ] {
        reader.store(s1, address, value);
    }
    let walks = [
        (
            "=SUM(E1:F3)",
            Ok(CalcValue::number(31.0)),
            vec![(cell_at(s1, "E1"), 4), (cell_at(s1, "F3"), 1)],
        ),
        (
            "=SUM(H1:I2)",
            Ok(CalcValue::error(WorksheetErrorCode::Div0)),
            vec![(cell_at(s1, "H1"), 4)],
        ),
    ];
    for (formula, expected, runs) in &walks {
        let instance = instantiate(&compiled(formula, c10, &catalog, &mut cache), c10);
        let (result, _) = evaluated(&instance, &mut reader, &mut scratch);
        assert_eq!(&result, expected, "{formula}");
        let reported: Vec<(CellKey, usize)> = reader
            .value_runs
            .iter()
            .map(|(_, first, values)| (*first, values.len()))
            .collect();
        assert_eq!(&reported, runs, "{formula}");
    }
    note!(
        "ACCEPT-NOTE: cfml_m1_3__reuse_plan: the reader reported runs of several values that cross row ends (A1:A3 as one run of 3; E1:F2 as one run of 4 within E1:F3; H1:I2 as one run of 4); SUM's results through the CellReader bridge (28; 31, the sum of five distinct powers of two; #DIV/0!, met before #N/A in row-major order) show each value reached the kernel once, in row-major order"
    );

    // The near-zero rows of cfunc-m1.2's oracle sheet mixed_operands, whose C3 and C4 hold what
    // this reader's Sheet1!C3 and C4 hold. Each expected value is the evaluator's current answer,
    // not Excel's: a final + or - whose binary64 result lies in the chosen band is refused, the
    // same difference under a final * is published, and =1E15+1-1E15, exactly 1, is refused too.
    // Bead dc-r4ci measures Excel's rule against the captured rows and replaces the refusal.
    reader.store(s1, "C3", CalcValue::number(123_456_789_012.34));
    reader.store(s1, "C4", CalcValue::number(123_456_789_012.33));
    let near_zero = |result: f64| Err(EvalFault::NearZeroFinalSum { result });
    let near_zero_rows: [(&str, Expected); 7] = [
        ("=0.5-0.4-0.1", (near_zero(-2.7755575615628914e-17), vec![])),
        (
            "=1*(0.5-0.4-0.1)",
            (Ok(CalcValue::number(-2.7755575615628914e-17)), vec![]),
        ),
        (
            "=1.333+1.225-1.333-1.225",
            (near_zero(-2.220446049250313e-16), vec![]),
        ),
        ("=1E15+1-1E15", (near_zero(1.0), vec![])),
        ("=0.1+0.2-0.3", (near_zero(5.551115123125783e-17), vec![])),
        ("=1+1E-15-1", (near_zero(1.1102230246251565e-15), vec![])),
        (
            "=C3-C4",
            (
                near_zero(0.0099945068359375),
                vec![one_cell(s1, "C3"), one_cell(s1, "C4")],
            ),
        ),
    ];
    for (formula, expected) in &near_zero_rows {
        let instance = instantiate(&compiled(formula, c10, &catalog, &mut cache), c10);
        let seen = evaluated(&instance, &mut reader, &mut scratch);
        assert_eq!(&seen, expected, "{formula}");
        let answer = match &seen.0 {
            Ok(value) => value
                .as_number()
                .map_or_else(|| format!("{value:?}"), |n| format!("number {n:?}")),
            Err(fault) => format!("{fault:?}"),
        };
        note!(
            "ACCEPT-NOTE: cfml_m1_3__reuse_plan: near-zero row {formula} -> {answer} (the evaluator's current answer, not Excel's; Excel's rule is measured by dc-r4ci)"
        );
    }

    // References to deleted cells (dc-4r8u's binding): =Sheet2!#REF! and =#REF!A1 evaluate to
    // #REF! without asking the reader for any area, and the error propagates through an operator.
    for formula in ["=Sheet2!#REF!", "=#REF!A1", "=Sheet2!#REF!+1"] {
        let instance = instantiate(&compiled(formula, c10, &catalog, &mut cache), c10);
        let seen = evaluated(&instance, &mut reader, &mut scratch);
        assert_eq!(
            seen,
            (Ok(CalcValue::error(WorksheetErrorCode::Ref)), vec![]),
            "{formula}"
        );
    }

    // The cache keys plans by normal form. The same formula copied one row down, or written with
    // other case and spacing, is the same plan; absolute references or another literal are not.
    let sum = &plans[1];
    let copied = compiled("=SUM(A2:A4)*2", anchor_at(s1, "C11"), &catalog, &mut cache);
    let respelled = compiled("=sum(a1:a3) * 2", c10, &catalog, &mut cache);
    let absolute = compiled("=SUM($A$1:$A$3)*2", c10, &catalog, &mut cache);
    let tripled = compiled("=SUM(A1:A3)*3", c10, &catalog, &mut cache);
    assert!(Arc::ptr_eq(sum, &copied));
    assert!(Arc::ptr_eq(sum, &respelled));
    assert!(!Arc::ptr_eq(sum, &absolute));
    assert!(!Arc::ptr_eq(sum, &tripled));

    // One plan placed at three more anchors: one row down it reads A2:A4 (SUM skips A4's text:
    // (10+3)*2), at F20 the blank D11:D13, and at C3 its rows lie above the grid, so it is #REF!
    // without a read.
    let placements = [
        (
            anchor_at(s1, "C11"),
            (Ok(CalcValue::number(26.0)), vec![area_of(s1, "A2", "A4")]),
        ),
        (
            anchor_at(s1, "F20"),
            (Ok(CalcValue::number(0.0)), vec![area_of(s1, "D11", "D13")]),
        ),
        (
            anchor_at(s1, "C3"),
            (Ok(CalcValue::error(WorksheetErrorCode::Ref)), vec![]),
        ),
    ];
    for (anchor, expected) in &placements {
        let instance = instantiate(sum, *anchor);
        assert_eq!(
            &evaluated(&instance, &mut reader, &mut scratch),
            expected,
            "{:?}",
            anchor.cell()
        );
    }
    assert_eq!(
        instantiate(sum, anchor_at(s1, "C3")).areas(),
        [Err(PlacementError::RowOffGrid { resolved: -6 })]
    );

    // What compile does not lower it refuses, typed, without a plan.
    let refusals = [
        ("=1+", "syntax"),
        ("=Nope!A1", "reference"),
        ("=TaxRate*2", "unsupported cfml-m3.1"),
        ("=SUM({1,2})", "unsupported cfml-m3.4"),
        ("=NOSUCH(1)", "unknown function"),
        ("=IF(1)", "arity"),
        ("=1234567890123456", "number literal"),
    ];
    for (formula, expected) in refusals {
        let refusal = compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            c10,
            &catalog,
            &mut cache,
        )
        .map(|plan| format!("{plan:?}"));
        let kind = match &refusal {
            Err(CompileError::Syntax { .. }) => "syntax".to_owned(),
            Err(CompileError::Reference { .. }) => "reference".to_owned(),
            Err(CompileError::Unsupported { owner, .. }) => format!("unsupported {owner}"),
            Err(CompileError::UnknownFunction { .. }) => "unknown function".to_owned(),
            Err(CompileError::Arity { .. }) => "arity".to_owned(),
            Err(CompileError::NumberLiteral { .. }) => "number literal".to_owned(),
            Ok(plan) => panic!("{formula} compiled: {plan}"),
        };
        assert_eq!(kind, expected, "{formula}");
    }

    note!(
        "ACCEPT-NOTE: cfml_m1_3__reuse_plan: {} formulas compiled once and each evaluated twice; {} results changed with the provider's data; {} catalogue requests, all made by compile; one plan shared by three formula texts and placed at four anchors; {} near-zero rows evaluated; {} formulas refused by compile, typed",
        table.len(),
        changed,
        asked_by_compile.len(),
        near_zero_rows.len(),
        refusals.len()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfml_m1_3__lazy_if() {
    let catalog = LoggingCatalog::new(&["Sheet1", "Sheet2"]);
    let (s1, s2) = (catalog.id("Sheet1"), catalog.id("Sheet2"));
    let c10 = anchor_at(s1, "C10");
    let mut cache = PlanCache::new();
    let mut scratch = EvalScratch::new();
    let mut reader = RunReader::default();

    // B1 decides. D1 is an input the reader cannot supply: reading it is a fault, so a branch
    // that reads it shows in the reader's log and in the result whenever it is evaluated.
    reader.store(s1, "B1", CalcValue::number(5.0));
    reader.store(s1, "A1", CalcValue::number(4.0));
    reader.store(s1, "A2", CalcValue::number(6.0));
    reader.store(s1, "C1", CalcValue::number(7.0));
    reader.leave_unresolved(s1, "D1");
    reader.store(s1, "D2", CalcValue::number(1.0));
    reader.store(s1, "E1", CalcValue::error(WorksheetErrorCode::NA));
    reader.store(s2, "A1", CalcValue::number(1.0));
    reader.store(s2, "A2", CalcValue::number(2.0));
    reader.store(s2, "A3", CalcValue::number(3.0));

    let cell = |address: &str| one_cell(s1, address);
    let missing_d1 = Err(EvalFault::MissingInput {
        cell: cell_at(s1, "D1"),
    });
    // Each formula with its hand-derived result and reads while B1 is 5, then while B1 is -1.
    let table: Vec<(&str, Expected, Expected)> = vec![
        // The unselected error branch C1/0 is not read while B1 is 5.
        (
            "=IF(B1>0,A1*2,C1/0)",
            (Ok(CalcValue::number(8.0)), vec![cell("B1"), cell("A1")]),
            (
                Ok(CalcValue::error(WorksheetErrorCode::Div0)),
                vec![cell("B1"), cell("C1")],
            ),
        ),
        // The unselected branch reads the unresolved D1 and divides by zero: while B1 is 5 it is
        // neither read nor a fault.
        (
            "=IF(B1>0,\"yes\",D1/0)",
            (Ok(text_value("yes")), vec![cell("B1")]),
            (missing_d1.clone(), vec![cell("B1"), cell("D1")]),
        ),
        // A function in the unselected branch is not called: SUM would read D1:D3.
        (
            "=IF(B1>0,A1,SUM(D1:D3))",
            (Ok(CalcValue::number(4.0)), vec![cell("B1"), cell("A1")]),
            (
                missing_d1.clone(),
                vec![cell("B1"), area_of(s1, "D1", "D3")],
            ),
        ),
        // Nested: the inner IF runs only on the outer's TRUE branch, and only its chosen branch.
        (
            "=IF(B1>0,IF(A1>5,A1,A2),SUM(Sheet2!A1:A3))",
            (
                Ok(CalcValue::number(6.0)),
                vec![cell("B1"), cell("A1"), cell("A2")],
            ),
            (
                Ok(CalcValue::number(6.0)),
                vec![cell("B1"), area_of(s2, "A1", "A3")],
            ),
        ),
        // A FALSE condition without a third argument asks for nothing more.
        (
            "=IF(B1>0,A1)",
            (Ok(CalcValue::number(4.0)), vec![cell("B1"), cell("A1")]),
            (Ok(CalcValue::logical(false)), vec![cell("B1")]),
        ),
        // A condition that is an error is the result; neither branch is asked for.
        (
            "=IF(E1,A1,A2)",
            (
                Ok(CalcValue::error(WorksheetErrorCode::NA)),
                vec![cell("E1")],
            ),
            (
                Ok(CalcValue::error(WorksheetErrorCode::NA)),
                vec![cell("E1")],
            ),
        ),
        // Controls: an eager function reads every argument, and reading D1 is a fault.
        (
            "=SUM(B1,A1,C1)",
            (
                Ok(CalcValue::number(16.0)),
                vec![cell("B1"), cell("A1"), cell("C1")],
            ),
            (
                Ok(CalcValue::number(10.0)),
                vec![cell("B1"), cell("A1"), cell("C1")],
            ),
        ),
        (
            "=D1/0",
            (missing_d1.clone(), vec![cell("D1")]),
            (missing_d1.clone(), vec![cell("D1")]),
        ),
    ];

    let instances: Vec<FormulaInstance> = table
        .iter()
        .map(|(formula, _, _)| instantiate(&compiled(formula, c10, &catalog, &mut cache), c10))
        .collect();
    // The plan holds the references of both branches; only evaluation chooses between them.
    assert_eq!(instances[1].areas(), [Ok(cell("B1")), Ok(cell("D1"))]);

    let (mut evaluations, mut unrequested) = (0, 0);
    for b1 in [5.0, -1.0] {
        reader.store(s1, "B1", CalcValue::number(b1));
        for (instance, (formula, while_positive, while_negative)) in instances.iter().zip(&table) {
            let expected = if b1 > 0.0 {
                while_positive
            } else {
                while_negative
            };
            let seen = evaluated(instance, &mut reader, &mut scratch);
            assert_eq!(&seen, expected, "{formula} with B1 = {b1}");
            evaluations += 1;
            // The plan's reference operands, as placed, whose area this evaluation did not
            // request; an operand another evaluation requests counts here all the same.
            unrequested += instance
                .areas()
                .iter()
                .filter(|area| matches!(area, Ok(area) if !seen.1.contains(area)))
                .count();
        }
    }
    assert_eq!((evaluations, unrequested), (16, 14));
    // The catalogue was asked once, for Sheet2, when the nested formula compiled.
    assert_eq!(*catalog.requests.borrow(), ["Sheet2"]);

    note!(
        "ACCEPT-NOTE: cfml_m1_3__lazy_if: {} formulas evaluated with B1 = 5 and with B1 = -1; each of the {} evaluations asked the reader for exactly the hand-derived areas (for IF, the condition's and then only the chosen branch's); counted in each evaluation, the plan's reference operands that evaluation did not request (those of unselected branches, and of both branches where the condition is an error) are {} in all; the catalogue holds no volatile function, so no unselected volatile branch is exercised, and an unresolved input stands in as the branch whose evaluation would show",
        table.len(),
        evaluations,
        unrequested
    );
}

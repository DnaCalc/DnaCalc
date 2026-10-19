//! Acceptance cases owned by the calc-fml package, each registered by its exact name in
//! `crates/calc-fml/acceptance.json`. Every case is `#[test]` natively and `#[wasm_bindgen_test]`
//! on wasm32, with no browser or Node.js configuration of its own: `cargo xtask accept` forces the
//! browser with `WASM_BINDGEN_USE_BROWSER`.
//!
//! The cases reach the parser only through calc-fml's public API (`calc_fml::parse_formula` and
//! the `calc_fml::syntax` types). They check the tree against the input text by their own walk:
//! the UTF-16 code units of the input are cut at each token's and trivia's span and compared with
//! its text, and those pieces must tile the input from offset 0 to its end with nothing left out.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_fml::parse_formula;
use calc_fml::syntax::{
    SyntaxChild, SyntaxDiagnosticKind, SyntaxKind, SyntaxNode, SyntaxTree, TextSpan, Token,
    TokenKind,
};
use calc_func::value::WorksheetErrorCode;
use std::collections::BTreeSet;

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

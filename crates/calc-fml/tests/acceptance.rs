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
//! None of the cfml-m1.3 expected values has been compared with Excel here. They are of two
//! kinds:
//! - values derived by hand from Excel's documented operator, function and reference rules
//!   (=1+2*3 is 7; a reference to deleted cells is #REF!);
//! - every other expected value is the implementation's current answer, where Excel's behaviour
//!   is undocumented, not measured or not modelled: each typed refusal (CompileError, EvalFault
//!   such as AreaAsValue, MissingInput and NearZeroFinalSum, FunctionFault such as
//!   NotImplemented), the binary64 result the evaluator publishes for a near-zero row it does not
//!   refuse (=1*(0.5-0.4-0.1)), and the choices made by hand: SUM's first error in row-major
//!   order, 0 for a formula whose value is a blank cell, and #REF! for a reference placed off the
//!   grid (Excel may wrap on some routes; dc-ds2z measures it).
//!
//! cfunc-m1.2's oracle capture is the Excel evidence for the operators and functions; bead
//! dc-r4ci reads it for the near-zero rows.
//!
//! The dc-2mlz case runs formulas of Excel's maximum length (8,192 characters) and its deepest
//! nesting of functions (64) through the same public route, parse_formula, bind_references,
//! compile, instantiate and evaluate, natively on a thread spawned with a 1 MB stack (the size of
//! the Windows main thread's) and on wasm32 on the browser's main thread. Both limits are Excel's
//! documented specifications, not measured here. Its expected values are derived by hand (4,096
//! ones sum to 4,096), and the depth of each tree is counted by the case's own walk, which keeps
//! its own stack. A stack overflow aborts the native test process, so a native run that reports
//! the case passed shows that the 1 MB stack sufficed.
//!
//! The dc-besk case compiles function names as a file spells them (`_xlfn.MAXIFS`,
//! `_xlfn._xlws.MAXIFS`) through `calc_fml::compile` and evaluates them through
//! `calc_fml::evaluate`, beside the same formula as a cell entry spells it: the two must share one
//! plan from the `PlanCache`, which holds one plan per distinct set of catalogue entries, and
//! give the value derived by hand from the functions' documented rules. Which spellings compile
//! reads as a function, and which as an unknown name, is the rule `file_unprefixed` in
//! `src/eval/compile.rs` states; the case holds no value read from Excel.
//!
//! The dc-ibp2 cases run IFERROR and IFNA through the same public route (parse_formula, compile,
//! instantiate, evaluate), over a reader that logs the areas it is asked for and an
//! EvaluationObserver that records the calls the evaluator shows it, so a fallback that is not
//! evaluated shows as areas never requested and calls never run. Each oracle row's expected value
//! is derived by hand and labelled: `documented` when Microsoft's IFERROR or IFNA page (or the
//! operator and function rules the row nests them in) gives it, `unmeasured choice` with the
//! choice named otherwise, including where the implementation departs from the page's text. The
//! rows' formulas are column D of the sheet error_fallback of `tests/fixtures/dc_ibp2/oracle.xlsx`,
//! written by the committed generator `tests/fixtures/make-fixtures.ps1`; Excel's capture of that
//! workbook (`tests/captures/dc_ibp2/oracle.capture.json`, written by `cargo xtask capture`) is the
//! Excel evidence, judged row by row through calc_verdict::judge. Column F holds
//! source formulas calc-fml does not compile (FILTER, SEQUENCE, FIELDVALUE), whose errors
//! (#CALC!, #SPILL!, #FIELD!) some rows read: the case's reader holds the error each is assumed to
//! give, and the comparison checks that assumption against Excel's capture of the source cell.

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
    CompileError, CompiledPlan, EvalFault, EvalFrame, EvalScratch, EvaluationObserver,
    EvaluationReport, EvaluationServices, ExecutedCall, FormulaInstance, PlanCache, compile,
    evaluate, instantiate,
};
use calc_fml::{ReferenceCatalog, ReferenceReader, bind_references, read_cells};
use calc_func::value::{
    CalcValue, CellKey, CoreValue, ExcelText, GridArea, IdAllocator, MAX_COLUMNS, MAX_ROWS,
    NumericMode, SheetId, WorksheetErrorCode,
};
use calc_func::{FunctionFault, Notation, catalogue};
use calc_verdict::{
    Acquisition, CalculationContext, CaptureRecord, CaptureSession, CellAddress, CellObservation,
    CellResult, Evaluation as VerdictEvaluation, FormulaCell, FormulaKind, InstrumentRecord,
    InstrumentedEvaluation, MissingCapture, Observation, Provenance, ResultSet, RunIdentity,
    STAGE_ENVIRONMENT, STAGE_INVENTORY, STAGE_ISOLATION, SheetEntry, SheetVisibility, Verdict,
    VerdictPolicy, VerdictReport, judge, sha256_hex,
};
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
    // without a read. That #REF! is cfml-m1.2's choice, not measured: Excel may wrap a relative
    // reference around the grid on some routes (dc-ds2z measures it).
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

// ---------------------------------------------------------------------------------------------
// dc-2mlz: formulas of Excel's maximum length, on a 1 MB stack.

/// Excel's maximum formula length, in characters (its specifications and limits).
const EXCEL_MAX_FORMULA_CHARS: usize = 8192;

/// The stack the dc-2mlz case runs on natively: 1 MB, the size of the Windows main thread's.
#[cfg(not(target_arch = "wasm32"))]
const ONE_MEGABYTE: usize = 1 << 20;

/// Runs `body` natively on a thread spawned with a 1 MB stack, passing on its panic; on wasm32 on
/// the browser's main thread, where the case runs.
fn on_a_one_megabyte_stack(body: fn()) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let thread = std::thread::Builder::new()
            .name("dc_2mlz__maximum_length".to_owned())
            .stack_size(ONE_MEGABYTE)
            .spawn(body)
            .expect("a thread with a 1 MB stack");
        if let Err(panic) = thread.join() {
            std::panic::resume_unwind(panic);
        }
    }
    #[cfg(target_arch = "wasm32")]
    body();
}

/// The depth of the tree under `node` in nodes, and the deepest nesting of calls in it, counted
/// on a stack of the nodes still to visit: a recursive walk is what the case must not need.
fn nesting(node: &SyntaxNode) -> (usize, usize) {
    let (mut depth, mut calls) = (0, 0);
    let mut pending = vec![(node, 1, 0)];
    while let Some((node, at, calls_above)) = pending.pop() {
        let calls_here = calls_above + usize::from(node.kind() == SyntaxKind::CallExpr);
        depth = depth.max(at);
        calls = calls.max(calls_here);
        pending.extend(node.child_nodes().map(|child| (child, at + 1, calls_here)));
    }
    (depth, calls)
}

/// `levels` nested IFs: level k is `IF(A1>=k, level k+1, -k)`, and the innermost IF's TRUE
/// branch is the text "innermost". With A1 at least `levels`, every condition is TRUE, so the
/// value is "innermost"; a FALSE condition at level k would give -k.
fn nested_ifs(levels: usize) -> String {
    let mut formula = String::from("=");
    for level in 1..=levels {
        formula.push_str(&format!("IF(A1>={level},"));
    }
    formula.push_str("\"innermost\"");
    for level in (1..=levels).rev() {
        formula.push_str(&format!(",-{level})"));
    }
    formula
}

/// A value as the case's notes print it: a number, or text in quotes.
fn shown(value: &CalcValue) -> String {
    match (value.as_number(), value.as_text()) {
        (Some(number), _) => format!("the number {number}"),
        (_, Some(text)) => format!("the text {:?}", text.to_string_lossy()),
        _ => format!("{value:?}"),
    }
}

/// A refusal as the case compares it: its variant and, where it has one, its owner or bind error.
fn refusal_kind(refusal: &CompileError) -> String {
    match refusal {
        CompileError::Unsupported { owner, .. } => format!("Unsupported {owner}"),
        CompileError::Reference { error, .. } => format!("Reference {error:?}"),
        other => format!("{other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_2mlz__maximum_length() {
    on_a_one_megabyte_stack(maximum_length_formulas);
}

fn maximum_length_formulas() {
    let catalog = LoggingCatalog::new(&["Sheet1"]);
    let sheet = catalog.id("Sheet1");
    let c10 = anchor_at(sheet, "C10");
    let mut cache = PlanCache::new();
    let mut scratch = EvalScratch::new();
    let mut reader = RunReader::default();
    reader.store(sheet, "A1", CalcValue::number(64.0));
    let a1 = one_cell(sheet, "A1");

    // Each formula with the depth of its tree in nodes and its deepest nesting of calls, the
    // number of references binding finds, its value and the areas its evaluation reads, all
    // derived by hand. The first four are 8,192 characters long.
    type Calculated = (
        &'static str,
        String,
        (usize, usize),
        usize,
        CalcValue,
        Vec<GridArea>,
    );
    let calculated: Vec<Calculated> = vec![
        // 4,096 ones: a left-deep chain of 4,095 additions.
        (
            "left-deep chain =1+1+...+1",
            format!("=1{}", "+1".repeat(4095)),
            (4096, 0),
            0,
            CalcValue::number(4096.0),
            vec![],
        ),
        // Each addition's right operand is the parenthesised rest: 2,047 groups and 2,049 ones.
        (
            "right-deep chain =1+(1+(...(1+1)...))",
            format!("={}1+1{}", "1+(".repeat(2047), ")".repeat(2047)),
            (4096, 0),
            0,
            CalcValue::number(2049.0),
            vec![],
        ),
        // One literal inside 4,095 parentheses.
        (
            "parenthesised group =((...(1)...))",
            format!("={}1{}", "(".repeat(4095), ")".repeat(4095)),
            (4096, 0),
            0,
            CalcValue::number(1.0),
            vec![],
        ),
        // 8,190 prefix minus signs, an even number of them.
        (
            "prefix chain =--...-1",
            format!("={}1", "-".repeat(8190)),
            (8191, 0),
            0,
            CalcValue::number(1.0),
            vec![],
        ),
        // 64 nested IFs (each a call and its argument list, two nodes), the innermost argument
        // list holding the condition A1>=64 (a binary operator over two leaves). Each condition
        // reads A1, which holds 64.
        (
            "64 nested IFs",
            nested_ifs(64),
            (130, 64),
            64,
            text_value("innermost"),
            vec![a1; 64],
        ),
    ];
    for (name, formula, ..) in &calculated[..4] {
        assert_eq!(
            formula.encode_utf16().count(),
            EXCEL_MAX_FORMULA_CHARS,
            "{name}"
        );
    }
    for (name, formula, shape, references, value, reads) in &calculated {
        let tree = parse_formula(formula);
        assert!(
            tree.diagnostics().is_empty(),
            "{name}: {:?}",
            tree.diagnostics().first()
        );
        assert!(
            tree.source_text() == *formula,
            "{name}: the tree does not reprint the formula"
        );
        let expression = tree.expression().expect("a formula's expression");
        assert_eq!(nesting(expression), *shape, "{name}");
        assert!(tree.clone() == tree, "{name}: a copy of the tree differs");
        let sites = bind_references(&tree, ReferenceNotation::A1, c10, &catalog);
        assert_eq!(sites.len(), *references, "{name}");
        let plan = compile(&tree, ReferenceNotation::A1, c10, &catalog, &mut cache)
            .unwrap_or_else(|refused| panic!("{name}: {refused:?}"));
        let instance = instantiate(&plan, c10);
        let (result, read) = evaluated(&instance, &mut reader, &mut scratch);
        assert_eq!(result.as_ref(), Ok(value), "{name}");
        assert_eq!(&read, reads, "{name}");
        note!(
            "ACCEPT-NOTE: dc_2mlz__maximum_length: {name}: {} characters, a tree {} nodes deep with calls nested {} deep; parsed without diagnostics and reprinted, {} references bound, compiled, and evaluated to {} with {} reads",
            formula.encode_utf16().count(),
            shape.0,
            shape.1,
            sites.len(),
            shown(value),
            read.len()
        );
    }

    // The other directions of nesting, at 8,192 characters: compile refuses each, typed, after
    // parsing and binding it without diagnostics. Each with the depth of its tree, its number of
    // binding sites and its refusal.
    let refused: Vec<(&str, String, usize, usize, &str)> = vec![
        (
            "prefix @ chain",
            format!("={}A1", "@".repeat(8189)),
            8190,
            1,
            "Unsupported cfml-m3.3",
        ),
        (
            "qualifier chain",
            format!("={}A10", "S!".repeat(4094)),
            4095,
            1,
            "Reference QualifiedNonReference",
        ),
        (
            "range operator chain",
            format!("=1{}", ":1".repeat(4095)),
            4096,
            1,
            "Unsupported cfml-m3.3",
        ),
        (
            "union chain",
            format!("=(1{})", ",1".repeat(4094)),
            4096,
            0,
            "Unsupported cfml-m3.3",
        ),
        (
            "intersection chain",
            format!("=1{}", " 1".repeat(4095)),
            4096,
            0,
            "Unsupported cfml-m3.3",
        ),
        (
            "nested array constants",
            format!("={}1{}", "{".repeat(4095), "}".repeat(4095)),
            4096,
            0,
            "Unsupported cfml-m3.4",
        ),
        (
            "invocation chain",
            format!("=A{}", "(1)".repeat(2730)),
            2732,
            0,
            "Unsupported cfml-m2.3",
        ),
    ];
    for (name, formula, depth, references, refusal) in &refused {
        assert_eq!(
            formula.encode_utf16().count(),
            EXCEL_MAX_FORMULA_CHARS,
            "{name}"
        );
        let tree = parse_formula(formula);
        assert!(
            tree.diagnostics().is_empty(),
            "{name}: {:?}",
            tree.diagnostics().first()
        );
        assert!(
            tree.source_text() == *formula,
            "{name}: the tree does not reprint the formula"
        );
        assert_eq!(
            nesting(tree.expression().expect("a formula's expression")).0,
            *depth,
            "{name}"
        );
        let sites = bind_references(&tree, ReferenceNotation::A1, c10, &catalog);
        assert_eq!(sites.len(), *references, "{name}");
        let seen = compile(&tree, ReferenceNotation::A1, c10, &catalog, &mut cache)
            .map(|_| "compiled".to_owned())
            .unwrap_or_else(|refused| refusal_kind(&refused));
        assert_eq!(seen, *refusal, "{name}");
        note!(
            "ACCEPT-NOTE: dc_2mlz__maximum_length: {name}: 8192 characters, a tree {depth} nodes deep, {references} binding sites; compile refused it: {seen}"
        );
    }

    // One IF more than Excel's 64 nested levels of functions: the parser refuses the 65th call,
    // and compile refuses the formula at that call's name.
    let too_deep = nested_ifs(65);
    let tree = parse_formula(&too_deep);
    let at = too_deep.find("IF(A1>=65,").expect("the 65th IF");
    let name = TextSpan::new(at, 2);
    assert_eq!(tree.diagnostics().len(), 1, "{:?}", tree.diagnostics());
    let diagnostic = &tree.diagnostics()[0];
    assert_eq!(diagnostic.span, name);
    assert_eq!(diagnostic.kind, SyntaxDiagnosticKind::Message);
    assert!(
        diagnostic
            .message
            .starts_with("more than 64 nested levels of functions"),
        "{}",
        diagnostic.message
    );
    assert_eq!(nesting(tree.expression().expect("an expression")).1, 65);
    assert_eq!(
        compile(&tree, ReferenceNotation::A1, c10, &catalog, &mut cache).err(),
        Some(CompileError::Syntax { span: name })
    );
    note!(
        "ACCEPT-NOTE: dc_2mlz__maximum_length: 65 nested IFs ({} characters): one parse diagnostic, at the 65th IF ({}..{}): {:?}; compile refused it as CompileError::Syntax at that span",
        too_deep.len(),
        name.start,
        name.end(),
        diagnostic.message
    );
}

// ---------------------------------------------------------------------------------------------
// dc-besk: function names as a file spells them, with Excel's `_xlfn.` prefix.

/// What compiling and evaluating a formula of the dc-besk case gives: its value, or the compile
/// refusal it meets.
#[derive(Clone, Debug, PartialEq)]
enum Outcome {
    Value(CalcValue),
    Refused(CompileError),
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_besk__prefixed_calls_evaluate() {
    const CASE: &str = "dc_besk__prefixed_calls_evaluate";
    let catalog = LoggingCatalog::new(&["Sheet1"]);
    let sheet = catalog.id("Sheet1");
    let e1 = anchor_at(sheet, "E1");
    let mut cache = PlanCache::new();
    let mut scratch = EvalScratch::new();
    let mut reader = RunReader::default();
    for (address, value) in [
        ("A1", CalcValue::number(1.0)),
        ("A2", CalcValue::number(5.0)),
        ("A3", CalcValue::number(3.0)),
        ("B1", text_value("x")),
        ("B2", text_value("y")),
        ("B3", text_value("x")),
        ("C1", CalcValue::error(WorksheetErrorCode::NA)),
    ] {
        reader.store(sheet, address, value);
    }
    let compiled_at = |formula: &str, cache: &mut PlanCache| {
        compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            e1,
            &catalog,
            cache,
        )
    };
    // The refusal of `formula` as an unknown function written `spelling`, at that spelling.
    let unknown = |formula: &str, spelling: &str| {
        let start = formula
            .find(spelling)
            .expect("the spelling is in the formula");
        CompileError::UnknownFunction {
            span: TextSpan::new(start, spelling.len()),
            spelling: spelling.to_owned(),
        }
    };
    // The refusal of `formula`, one call after its `=`, for supplying `supplied` arguments.
    let arity = |formula: &str, supplied: usize| CompileError::Arity {
        span: TextSpan::new(1, formula.len() - 1),
        supplied,
    };
    let value = |value: CalcValue| (Outcome::Value(value.clone()), Outcome::Value(value));

    // IFNA is catalogued by dc-ibp2. Until it lands, its file spelling is refused as an unknown
    // function with that spelling, as its plain spelling is with its own.
    let ifna_catalogued = catalogue().resolve_surface(Notation::Call, "IFNA").is_ok();
    let ifna = if ifna_catalogued {
        value(CalcValue::number(7.0))
    } else {
        (
            Outcome::Refused(unknown("=_xlfn.IFNA(C1,7)", "_xlfn.IFNA")),
            Outcome::Refused(unknown("=IFNA(C1,7)", "IFNA")),
        )
    };

    // Each formula as a file spells it, the same formula as a cell entry spells it (its plain
    // twin), and what each gives. The values are derived by hand from the documented rules of
    // MAXIFS, MINIFS, SUM, IF and IFNA over A1:A3 = 1, 5, 3, B1:B3 = "x", "y", "x" and C1 = #N/A.
    let maxifs = r#"=MAXIFS(A1:A3,B1:B3,"x")"#;
    let rows: Vec<(&str, &str, (Outcome, Outcome))> = vec![
        (
            r#"=_xlfn.MAXIFS(A1:A3,B1:B3,"x")"#,
            maxifs,
            value(CalcValue::number(3.0)),
        ),
        (
            r#"=_xlfn.MINIFS(A1:A3,B1:B3,"x")"#,
            r#"=MINIFS(A1:A3,B1:B3,"x")"#,
            value(CalcValue::number(1.0)),
        ),
        (
            r#"=_xlfn._xlws.MAXIFS(A1:A3,B1:B3,"x")"#,
            maxifs,
            value(CalcValue::number(3.0)),
        ),
        // The prefixes and the name match in any ASCII case.
        (
            r#"=_XLFN.MAXIFS(A1:A3,B1:B3,"x")"#,
            maxifs,
            value(CalcValue::number(3.0)),
        ),
        (
            r#"=_xlfn.maxifs(A1:A3,B1:B3,"x")"#,
            maxifs,
            value(CalcValue::number(3.0)),
        ),
        (
            r#"=_XLFN._XLWS.MAXIFS(A1:A3,B1:B3,"x")"#,
            maxifs,
            value(CalcValue::number(3.0)),
        ),
        (
            r#"=_XlFn.MAXIFS(A1:A3,B1:B3,"x")"#,
            maxifs,
            value(CalcValue::number(3.0)),
        ),
        (
            r#"=_xLfN._xLwS.MAXIFS(A1:A3,B1:B3,"x")"#,
            maxifs,
            value(CalcValue::number(3.0)),
        ),
        // A function Excel never stores with a prefix, read through one.
        (
            "=_xlfn.SUM(A1:A3)",
            "=SUM(A1:A3)",
            value(CalcValue::number(9.0)),
        ),
        // A lazy entry through the prefix.
        (
            r#"=_xlfn.IF(A2,"big","small")"#,
            r#"=IF(A2,"big","small")"#,
            value(text_value("big")),
        ),
        // Prefixed calls inside arithmetic: 3*10+1.
        (
            r#"=_xlfn.MAXIFS(A1:A3,B1:B3,"x")*10+_xlfn.MINIFS(A1:A3,B1:B3,"x")"#,
            r#"=MAXIFS(A1:A3,B1:B3,"x")*10+MINIFS(A1:A3,B1:B3,"x")"#,
            value(CalcValue::number(31.0)),
        ),
        ("=_xlfn.IFNA(C1,7)", "=IFNA(C1,7)", ifna),
        // The entry's arity applies to the prefixed spelling as to the plain one.
        (
            "=_xlfn.MAXIFS(A1:A3,B1:B3)",
            "=MAXIFS(A1:A3,B1:B3)",
            (
                Outcome::Refused(arity("=_xlfn.MAXIFS(A1:A3,B1:B3)", 2)),
                Outcome::Refused(arity("=MAXIFS(A1:A3,B1:B3)", 2)),
            ),
        ),
    ];
    let mut evaluated_rows = 0;
    for (file, entered, (file_expected, entered_expected)) in &rows {
        let file_plan = compiled_at(file, &mut cache);
        let entered_plan = compiled_at(entered, &mut cache);
        let mut outcome = |plan: &Result<Arc<CompiledPlan>, CompileError>| match plan {
            Ok(plan) => Outcome::Value(
                evaluated(&instantiate(plan, e1), &mut reader, &mut scratch)
                    .0
                    .unwrap_or_else(|fault| panic!("{file}: {fault:?}")),
            ),
            Err(refused) => Outcome::Refused(refused.clone()),
        };
        assert_eq!(&outcome(&file_plan), file_expected, "{file}");
        assert_eq!(&outcome(&entered_plan), entered_expected, "{entered}");
        match (&file_plan, &entered_plan) {
            (Ok(file_plan), Ok(entered_plan)) => {
                assert!(
                    Arc::ptr_eq(file_plan, entered_plan),
                    "{file} and {entered} compile to one shared plan: the same catalogue entries"
                );
                evaluated_rows += 1;
            }
            (Err(_), Err(_)) => {}
            _ => panic!("{file} and {entered}: one compiled and the other did not"),
        }
        note!(
            "ACCEPT-NOTE: {CASE}: {file} gives {}, {entered} gives {}{}",
            described(file_expected),
            described(entered_expected),
            if file_plan.is_ok() {
                ", on one shared plan"
            } else {
                ""
            }
        );
    }

    // Prefixes compile does not take off (`_xlws.` alone, a second `_xlfn.`, `_xlws.` before
    // `_xlfn.`), and names no catalogue holds after the prefixes: each is refused as an unknown
    // function, at its span, with its spelling as written.
    let refused = [
        ("=_xlfn.NOSUCHFUNCTION(A1)", "_xlfn.NOSUCHFUNCTION"),
        (r#"=_xlws.MAXIFS(A1:A3,B1:B3,"x")"#, "_xlws.MAXIFS"),
        (
            r#"=_xlfn._xlfn.MAXIFS(A1:A3,B1:B3,"x")"#,
            "_xlfn._xlfn.MAXIFS",
        ),
        (
            r#"=_xlws._xlfn.MAXIFS(A1:A3,B1:B3,"x")"#,
            "_xlws._xlfn.MAXIFS",
        ),
        (
            "=_xlfn._xlws.NOSUCHFUNCTION(A1)",
            "_xlfn._xlws.NOSUCHFUNCTION",
        ),
        ("=1+_xlfn.NOSUCHFUNCTION(A1)", "_xlfn.NOSUCHFUNCTION"),
    ];
    for (formula, spelling) in refused {
        assert_eq!(
            compiled_at(formula, &mut cache).err(),
            Some(unknown(formula, spelling)),
            "{formula}"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} formulas refused as CompileError::UnknownFunction at the name, with its spelling as written: {}",
        refused.len(),
        refused.map(|(formula, _)| formula).join(" ")
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {evaluated_rows} prefixed formulas evaluated through calc_fml::evaluate, each on the plan of its unprefixed spelling; IFNA catalogued: {ifna_catalogued}; every expected value derived by hand from the functions' documented rules"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: counted divergence, owned by bead dc-xrxb: compile reads {maxifs} as MAXIFS whether the text came from a file or from a cell entry, while a file that spells MAXIFS or IFNA without its prefix loads in Excel 16.0 as an unknown name (#NAME?); dc-xrxb gives compile the file prefix of each catalogued function and a load-origin input"
    );
}

/// An outcome as the dc-besk case's notes print it.
fn described(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Value(value) => shown(value),
        Outcome::Refused(refused) => format!("the refusal {refused:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// dc-ibp2: IFERROR and IFNA, evaluated lazily through calc_fml::evaluate, and their Excel
// comparison through calc_verdict::judge.

/// The oracle workbook, as committed (its bytes are kept across checkouts by `.gitattributes`).
const FALLBACK_WORKBOOK: &[u8] = include_bytes!("fixtures/dc_ibp2/oracle.xlsx");
const FALLBACK_WORKBOOK_PATH: &str = "crates/calc-fml/tests/fixtures/dc_ibp2/oracle.xlsx";
/// Excel's capture of the oracle workbook, or the absent-capture marker until it is taken.
const FALLBACK_CAPTURE: &[u8] = include_bytes!("captures/dc_ibp2/oracle.capture.json");
/// The capture worker, whose content hash a capture's tool revision must name.
const CAPTURE_WORKER: &[u8] =
    include_bytes!("../../../platform/calc-oracle-excel/worker/capture-worker.ps1");
/// The case the capture is recorded under, and the command that writes it.
const FALLBACK_CASE: &str = "dc_ibp2__error_fallback";
const FALLBACK_CAPTURE_COMMAND: &str = "cargo xtask capture --fixture crates/calc-fml/tests/fixtures/dc_ibp2/oracle.xlsx --case dc_ibp2__error_fallback --out crates/calc-fml/tests/captures/dc_ibp2/oracle.capture.json";
/// The oracle workbook's sheet, and its column (D) of formulas, one per oracle row from row 1.
const FALLBACK_SHEET: &str = "error_fallback";
const FALLBACK_COLUMN: u32 = 4;
/// The sheet of the cells only the rows that are not compared with Excel read.
const ELSEWHERE: &str = "elsewhere";
/// The authored revision the rows calculate: the oracle workbook as committed.
const FALLBACK_REVISION: u64 = 0;

/// Why a row's expected value is what it is.
#[derive(Clone, Copy, Debug)]
enum Basis {
    /// Microsoft's IFERROR or IFNA page, or the operator and function rules the row nests them
    /// in, gives the value.
    Documented,
    /// The pages give the value under a condition about the oracle workbook, which the
    /// comparison checks against the capture.
    DocumentedWhen(Condition),
    /// The pages do not give the value, or say otherwise: the implementation's choice, named.
    Unmeasured(&'static str),
}

const EVERY_ERROR: &str = "IFERROR catches every error; its page lists seven (#N/A, #VALUE!, #REF!, #DIV/0!, #NUM!, #NAME?, #NULL!), not #GETTING_DATA";
const BLANK_VALUE: &str = "a blank cell's value becomes the result as the empty value, which the formula's result shows as 0; the pages say an empty cell is treated as empty text";
const BLANK_OPERAND: &str = "the empty value IFERROR returns for a blank cell reads as 0 in arithmetic and equals 0; the pages' empty text would give #VALUE! and FALSE";
const OMITTED: &str = "an omitted argument that becomes the result is the empty value, which the formula's result shows as 0; the pages do not say";
const OMITTED_OPERAND: &str = "an omitted argument that IFERROR returns is the empty value, which concatenates as empty text (calc-func's IF returns 0 for an omitted branch, which would give \"0|\"); the pages do not say";
const NOT_A_REFERENCE: &str = "IFERROR's value parameter takes a value, so a reference reaches it as its cell's value and IFERROR returns no reference; the page does not say";
const IF_REFERENCE: &str = "IF returns its chosen branch as written, a reference here, whose text SUM skips (cfml-m1.3's choice): the contrast with the row before";
const NEAR_ZERO: &str = "the check of a final + or - does not follow IFERROR's arguments, so the binary64 result is published; Excel's adjustment of a final sum near zero (KB 78113) is not documented inside a function's argument";
const NOT_IN_WORKBOOK: &str = "IFERROR catches every error, these too; these rows read them from cells of the sheet elsewhere, which the oracle workbook does not hold, so they are not compared with Excel (whether Excel takes any of these errors as a formula literal or a cell constant is not measured); #CALC!, #SPILL! and #FIELD! are compared through the source formulas of column F, while for #BLOCKED!, #CONNECT! and #BUSY! catching is an unmeasured choice, since the functions that give them in Excel (IMAGE, STOCKHISTORY, PY and other external-data routes) are ones the capture refuses";
const SOURCE_CAUGHT: &str = "IFERROR catches the error of the source formula of column F, which Microsoft's page, listing seven errors, does not name; the reader holds the error the case assumes Excel gives the source";
const SOURCE_PASSED: &str = "IFNA passes on the error of the source formula of column F, as its page says of every error but #N/A; the reader holds the error the case assumes Excel gives the source";

/// A condition about the oracle workbook under which the pages give a row's value, checked by
/// [`judge_excel_capture`] against the capture.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Condition {
    /// Excel loads the generator's empty inline string at A5 as empty text, not as a blank cell:
    /// the row's raw value in the capture is empty text, where the row over the blank A4 gives 0.
    EmptyTextCell,
    /// The workbook holds the formula in its legacy form: the capture's formula cell is no
    /// dynamic-array formula, and Excel reports it with the implicit-intersection operator `@`.
    LegacyForm,
}

impl Condition {
    fn text(self) -> &'static str {
        match self {
            Condition::EmptyTextCell => {
                "Excel loads the generator's empty inline string at A5 as empty text, not as a blank cell"
            }
            Condition::LegacyForm => {
                "the oracle workbook holds the formula in its legacy form, without dynamic-array cell metadata, and the value holds for that form; its dynamic-array form, whose range argument is array evaluation (cfml-m3.4), is not compared"
            }
        }
    }
}

/// An oracle row: its formula as entered, in column D of the sheet error_fallback at its position
/// (the first row is row 1), its hand-derived value and the value's basis, and, for a row that
/// reads a source cell of column F, that cell's address: the row's value is then conditional on
/// the error the reader holds for that cell, and the comparison publishes it as not modelled.
struct FallbackRow {
    formula: &'static str,
    expected: CalcValue,
    basis: Basis,
    source: Option<&'static str>,
}

fn fallback_rows() -> Vec<FallbackRow> {
    use Basis::{Documented as D, DocumentedWhen as DW, Unmeasured as U};
    use WorksheetErrorCode as W;
    let n = CalcValue::number;
    let e = CalcValue::error;
    let t = text_value;
    let row = |formula, expected, basis| FallbackRow {
        formula,
        expected,
        basis,
        source: None,
    };
    let reads = |source, formula, expected, basis| FallbackRow {
        formula,
        expected,
        basis,
        source: Some(source),
    };
    let near_zero = n(-2.7755575615628914e-17);
    vec![
        // The bead's four rows, then an error literal of each kind a cell can hold as a constant,
        // and #GETTING_DATA.
        row("=IFERROR(1/0,5)", n(5.0), D),
        row("=IFERROR(2,1/0)", n(2.0), D),
        row("=IFNA(#N/A,1)", n(1.0), D),
        row("=IFNA(#DIV/0!,1)", e(W::Div0), D),
        row("=IFERROR(#N/A,\"na\")", t("na"), D),
        row("=IFERROR(#VALUE!,1)", n(1.0), D),
        row("=IFERROR(#REF!,1)", n(1.0), D),
        row("=IFERROR(#NAME?,1)", n(1.0), D),
        row("=IFERROR(#NUM!,1)", n(1.0), D),
        row("=IFERROR(#NULL!,1)", n(1.0), D),
        row("=IFERROR(#DIV/0!,1)", n(1.0), D),
        row("=IFERROR(#GETTING_DATA,1)", n(1.0), U(EVERY_ERROR)),
        row("=IFNA(#GETTING_DATA,1)", e(W::GettingData), D),
        row("=IFNA(#VALUE!,1)", e(W::Value), D),
        row("=IFNA(#REF!,\"x\")", e(W::Ref), D),
        row("=IFNA(#NAME?,1)", e(W::Name), D),
        row("=IFNA(#NUM!,1)", e(W::Num), D),
        row("=IFNA(#NULL!,1)", e(W::Null), D),
        // References to error cells.
        row("=IFERROR(A2,\"caught\")", t("caught"), D),
        row("=IFNA(A2,\"caught\")", e(W::Div0), D),
        row("=IFNA(A3,\"caught\")", t("caught"), D),
        row("=IFERROR(A3,\"caught\")", t("caught"), D),
        row("=IFERROR(B1,1)", n(1.0), D),
        row("=IFERROR(B2,1)", n(1.0), D),
        row("=IFERROR(B3,1)", n(1.0), D),
        row("=IFERROR(B4,1)", n(1.0), D),
        row("=IFERROR(B5,1)", n(1.0), D),
        row("=IFNA(B1,1)", e(W::Ref), D),
        row("=IFNA(B5,1)", e(W::Null), D),
        // References to cells holding values, a blank cell and empty text among them, and
        // omitted arguments.
        row("=IFERROR(A1,0)", n(5.0), D),
        row("=IFNA(A1,0)", n(5.0), D),
        row("=IFERROR(A6,1)", t("a"), D),
        row("=IFERROR(A7,1)", CalcValue::logical(true), D),
        row("=IFERROR(A5,\"x\")", t(""), DW(Condition::EmptyTextCell)),
        row("=IFERROR(A4,\"x\")", n(0.0), U(BLANK_VALUE)),
        row("=IFNA(A4,\"x\")", n(0.0), U(BLANK_VALUE)),
        row("=IFERROR(#N/A,A4)", n(0.0), U(BLANK_VALUE)),
        row("=IFERROR(A4,1)&\"|\"", t("|"), D),
        row("=IFERROR(A4,1)+1", n(1.0), U(BLANK_OPERAND)),
        row(
            "=IFERROR(A4,1)=0",
            CalcValue::logical(true),
            U(BLANK_OPERAND),
        ),
        row("=IFERROR(,1)", n(0.0), U(OMITTED)),
        row("=IFERROR(#N/A,)", n(0.0), U(OMITTED)),
        row("=IFNA(,1)", n(0.0), U(OMITTED)),
        row("=IFERROR(#N/A,)&\"|\"", t("|"), U(OMITTED_OPERAND)),
        row("=IFERROR(,1)&\"|\"", t("|"), U(OMITTED_OPERAND)),
        // The fallback: read only on an error, and returned as it is.
        row("=IFERROR(A2,B7)", n(2.0), D),
        row("=IFERROR(A1,B7)", n(5.0), D),
        row("=IFERROR(A2,A3)", e(W::NA), D),
        row("=IFNA(#N/A,#DIV/0!)", e(W::Div0), D),
        row("=IFERROR(1/0,1/0)", e(W::Div0), D),
        row("=IFERROR(1,2^0.5)", n(1.0), D),
        row("=IFERROR(1,A1:A2)", n(1.0), DW(Condition::LegacyForm)),
        row("=IFNA(A1,2^0.5)", n(5.0), D),
        // Nested inside arithmetic and other calls.
        row("=1+IFERROR(1/0,5)", n(6.0), D),
        row("=IFERROR(A2,1)*10", n(10.0), D),
        row("=2*IFNA(A3,4)", n(8.0), D),
        row("=-IFERROR(A2,3)", n(-3.0), D),
        row("=IFERROR(IFERROR(1/0,#N/A),2)", n(2.0), D),
        row("=IFNA(IFERROR(A2,#N/A),\"x\")", t("x"), D),
        row("=IFERROR(\"a\"+1,\"bad\")", t("bad"), D),
        row("=IFERROR(A6*2,0)", n(0.0), D),
        row("=IFERROR(A1/B6,\"div\")", t("div"), D),
        row("=IFERROR(A1/B7,\"div\")", n(2.5), D),
        row("=IF(IFERROR(A2,FALSE),1,2)", n(2.0), D),
        row("=IFERROR(IF(A1>3,A2,1),\"x\")", t("x"), D),
        row("=SUM(IFERROR(A2,1),A1)", n(6.0), D),
        row("=IFERROR(SUM(A1:A3),0)", n(0.0), D),
        row("=SUM(IFERROR(A8,0))", n(2.0), U(NOT_A_REFERENCE)),
        row("=SUM(IF(TRUE,A8))", n(0.0), U(IF_REFERENCE)),
        row("=IFERROR(1/0,\"a\")&\"b\"", t("ab"), D),
        // A final sum near zero inside either argument, and two values as they are.
        row("=IFERROR(0.5-0.4-0.1,1)", near_zero.clone(), U(NEAR_ZERO)),
        row("=IFERROR(#N/A,0.5-0.4-0.1)", near_zero, U(NEAR_ZERO)),
        row("=IFERROR(TRUE,1)", CalcValue::logical(true), D),
        row("=IFERROR(\"\",1)", t(""), D),
        // The errors of the source formulas of column F, as the reader holds them: conditional
        // rows.
        reads("F1", "=IFERROR(F1,1)", n(1.0), U(SOURCE_CAUGHT)),
        reads("F1", "=IFNA(F1,1)", e(W::Calc), U(SOURCE_PASSED)),
        reads("F2", "=IFERROR(F2,1)", n(1.0), U(SOURCE_CAUGHT)),
        reads("F2", "=IFNA(F2,1)", e(W::Spill), U(SOURCE_PASSED)),
        reads("F4", "=IFERROR(F4,1)", n(1.0), U(SOURCE_CAUGHT)),
        reads("F4", "=IFNA(F4,1)", e(W::Field), U(SOURCE_PASSED)),
    ]
}

/// A source formula of column F of the sheet error_fallback: a call to a function that calc-fml
/// does not compile (calc-func's catalogue has no entry for it), which the oracle workbook holds as
/// a dynamic-array formula over its one cell. Rows read its error, which the case's reader holds
/// as `assumed`: the error the case assumes Excel gives the formula, not measured until the
/// capture, which the comparison checks against Excel's raw value of the cell.
struct SourceCell {
    address: &'static str,
    formula: &'static str,
    assumed: WorksheetErrorCode,
    /// The function calc-fml does not model, and what Excel is assumed to give.
    feature: &'static str,
}

fn fallback_sources() -> [SourceCell; 3] {
    use WorksheetErrorCode as W;
    [
        SourceCell {
            address: "F1",
            formula: "=FILTER(B6:B7,B6:B7>100)",
            assumed: W::Calc,
            feature: "FILTER (no row matches: #CALC!)",
        },
        SourceCell {
            address: "F2",
            formula: "=SEQUENCE(2)",
            assumed: W::Spill,
            feature: "SEQUENCE (its spill blocked by F3: #SPILL!)",
        },
        SourceCell {
            address: "F4",
            formula: "=FIELDVALUE(A1,\"x\")",
            assumed: W::Field,
            feature: "FIELDVALUE (A1 holds no data type: #FIELD!)",
        },
    ]
}

/// What an [`EvaluationObserver`] is shown of one evaluation: each call, as its notation and
/// spelling with ` *` when compile inserted it, and the report's cell and outcome.
#[derive(Default)]
struct CallLog {
    calls: Vec<String>,
    reported: Vec<(CellKey, Result<CalcValue, EvalFault>)>,
}

impl EvaluationObserver for CallLog {
    fn called(&mut self, call: ExecutedCall) {
        let target = call.target();
        self.calls.push(format!(
            "{:?} {}{}",
            target.notation(),
            target.spelling(),
            if call.synthetic() { " *" } else { "" }
        ));
    }

    fn returned(&mut self, report: EvaluationReport<'_>) {
        self.reported.push((
            report.anchor().cell(),
            report.outcome().cloned().map_err(Clone::clone),
        ));
    }
}

/// One evaluation as the case sees it: its outcome, the areas the reader was asked for in order,
/// and what the observer was shown.
struct Shown {
    outcome: Result<CalcValue, EvalFault>,
    reads: Vec<GridArea>,
    log: CallLog,
}

/// The cells the rows read, on the sheet error_fallback (as the oracle workbook holds them) and
/// on the sheet elsewhere (errors no oracle row can give, and an unresolved input), and the state
/// the rows are compiled and evaluated in.
struct FallbackBook {
    catalog: LoggingCatalog,
    sheet: SheetId,
    elsewhere: SheetId,
    reader: RunReader,
    cache: PlanCache,
    scratch: EvalScratch,
}

/// The constant cells of the sheet error_fallback, by address, as the generator writes them: A4
/// is blank, A5 empty text, F3 the text that blocks F2's spill.
fn fallback_cells() -> Vec<(&'static str, CalcValue)> {
    use WorksheetErrorCode as W;
    vec![
        ("A1", CalcValue::number(5.0)),
        ("A2", CalcValue::error(W::Div0)),
        ("A3", CalcValue::error(W::NA)),
        ("A5", text_value("")),
        ("A6", text_value("a")),
        ("A7", CalcValue::logical(true)),
        ("A8", text_value("2")),
        ("B1", CalcValue::error(W::Ref)),
        ("B2", CalcValue::error(W::Value)),
        ("B3", CalcValue::error(W::Num)),
        ("B4", CalcValue::error(W::Name)),
        ("B5", CalcValue::error(W::Null)),
        ("B6", CalcValue::number(0.0)),
        ("B7", CalcValue::number(2.0)),
        ("F3", text_value("block")),
    ]
}

/// The errors no constant cell of the oracle workbook holds, on the sheet elsewhere at A1 to A6.
const ELSEWHERE_ERRORS: [WorksheetErrorCode; 6] = [
    WorksheetErrorCode::Calc,
    WorksheetErrorCode::Spill,
    WorksheetErrorCode::Field,
    WorksheetErrorCode::Blocked,
    WorksheetErrorCode::Connect,
    WorksheetErrorCode::Busy,
];

impl FallbackBook {
    fn new() -> Self {
        let catalog = LoggingCatalog::new(&[FALLBACK_SHEET, ELSEWHERE]);
        let (sheet, elsewhere) = (catalog.id(FALLBACK_SHEET), catalog.id(ELSEWHERE));
        let mut reader = RunReader::default();
        for (address, value) in fallback_cells() {
            reader.store(sheet, address, value);
        }
        for source in fallback_sources() {
            reader.store(sheet, source.address, CalcValue::error(source.assumed));
        }
        for (row, code) in ELSEWHERE_ERRORS.iter().enumerate() {
            reader.store(elsewhere, &format!("A{}", row + 1), CalcValue::error(*code));
        }
        reader.leave_unresolved(elsewhere, "D1");
        FallbackBook {
            catalog,
            sheet,
            elsewhere,
            reader,
            cache: PlanCache::new(),
            scratch: EvalScratch::new(),
        }
    }

    /// The anchor of oracle row `row`: column D of the sheet error_fallback.
    fn anchor(&self, row: u32) -> FormulaAnchor {
        FormulaAnchor::new(CellKey::new(self.sheet, row, FALLBACK_COLUMN).expect("a cell"))
    }

    fn compile(
        &mut self,
        formula: &str,
        anchor: FormulaAnchor,
    ) -> Result<Arc<CompiledPlan>, CompileError> {
        compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &self.catalog,
            &mut self.cache,
        )
    }

    /// One evaluation of `instance` under Fast, observed, with the reader's logs cleared first.
    fn run(&mut self, instance: &FormulaInstance) -> Shown {
        self.reader.requests.clear();
        self.reader.value_runs.clear();
        let mode = NumericMode::Fast;
        let mut log = CallLog::default();
        let outcome = evaluate(
            EvalFrame::new(instance, &mut self.scratch),
            &mut EvaluationServices::new(&mode, &mut self.reader).observed_by(&mut log),
        );
        Shown {
            outcome,
            reads: std::mem::take(&mut self.reader.requests),
            log,
        }
    }

    /// `formula` compiled at `anchor` and evaluated once.
    fn evaluate(&mut self, formula: &str, anchor: FormulaAnchor) -> Shown {
        let plan = self
            .compile(formula, anchor)
            .unwrap_or_else(|refused| panic!("{formula}: {refused:?}"));
        self.run(&instantiate(&plan, anchor))
    }

    /// The areas these A1 texts name: a cell or `first:last`, on the sheet error_fallback unless
    /// qualified with `elsewhere!`.
    fn areas(&self, texts: &[&str]) -> Vec<GridArea> {
        texts
            .iter()
            .map(|text| {
                let (sheet, text) = match text.strip_prefix("elsewhere!") {
                    Some(rest) => (self.elsewhere, rest),
                    None => (self.sheet, *text),
                };
                match text.split_once(':') {
                    Some((first, last)) => area_of(sheet, first, last),
                    None => one_cell(sheet, text),
                }
            })
            .collect()
    }
}

/// A value as the dc-ibp2 notes show it.
fn value_shown(value: &CalcValue) -> String {
    match value.core() {
        CoreValue::Number(n) => format!("{n:?}"),
        CoreValue::Text(text) => format!("{:?}", text.to_string_lossy()),
        CoreValue::Logical(b) => (if *b { "TRUE" } else { "FALSE" }).to_owned(),
        CoreValue::Error(code) => format!("#{code:?}"),
        other => format!("{other:?}"),
    }
}

/// Whether two values are the same value, numbers bit for bit.
fn same_value(a: &CalcValue, b: &CalcValue) -> bool {
    match (a.as_number(), b.as_number()) {
        (Some(a), Some(b)) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}

/// An oracle row as one run evaluated it: its row, the evaluation, and the tick it ran at.
struct OracleRun {
    row: u32,
    formula: &'static str,
    expected: CalcValue,
    basis: Basis,
    source: Option<&'static str>,
    shown: Shown,
    tick: u64,
}

/// Every oracle row, evaluated in order at its cell, one tick each from tick 1.
fn fallback_run(book: &mut FallbackBook) -> Vec<OracleRun> {
    fallback_rows()
        .into_iter()
        .zip(1u32..)
        .map(|(row, at)| {
            let shown = book.evaluate(row.formula, book.anchor(at));
            OracleRun {
                row: at,
                formula: row.formula,
                expected: row.expected,
                basis: row.basis,
                source: row.source,
                shown,
                tick: u64::from(at),
            }
        })
        .collect()
}

/// Each source formula of column F with calc-fml's refusal to compile it at its own cell: the
/// diagnostic its not-modelled result carries.
fn source_refusals(book: &mut FallbackBook) -> Vec<(SourceCell, CompileError)> {
    fallback_sources()
        .into_iter()
        .map(|source| {
            let anchor = anchor_at(book.sheet, source.address);
            let refused = book
                .compile(source.formula, anchor)
                .err()
                .unwrap_or_else(|| panic!("{} compiles", source.formula));
            (source, refused)
        })
        .collect()
}

/// A formula in the form the oracle workbook stores it: without `=`, and with the prefix
/// `_xlfn.` that a package gives a function added to Excel after Excel 2007 (IFNA, Excel 2013, and
/// the source formulas' SEQUENCE and FIELDVALUE), and `_xlfn._xlws.` for FILTER.
fn file_form(formula: &str) -> String {
    formula[1..]
        .replace("IFNA(", "_xlfn.IFNA(")
        .replace("FILTER(", "_xlfn._xlws.FILTER(")
        .replace("SEQUENCE(", "_xlfn.SEQUENCE(")
        .replace("FIELDVALUE(", "_xlfn.FIELDVALUE(")
}

/// The parts of a ZIP package whose entries are stored (not compressed), as the generator writes
/// them, by name.
fn stored_parts(package: &[u8]) -> BTreeMap<String, &[u8]> {
    let u16_at = |at: usize| usize::from(u16::from_le_bytes([package[at], package[at + 1]]));
    let u32_at = |at: usize| {
        u32::from_le_bytes([
            package[at],
            package[at + 1],
            package[at + 2],
            package[at + 3],
        ]) as usize
    };
    let mut parts = BTreeMap::new();
    let mut at = 0;
    while at + 30 <= package.len() && u32_at(at) == 0x0403_4b50 {
        assert_eq!(
            u16_at(at + 8),
            0,
            "a part of the oracle workbook is compressed"
        );
        let size = u32_at(at + 18);
        let (name_length, extra_length) = (u16_at(at + 26), u16_at(at + 28));
        let name = String::from_utf8(package[at + 30..at + 30 + name_length].to_vec())
            .expect("an ASCII part name");
        let data = at + 30 + name_length + extra_length;
        assert!(
            parts.insert(name, &package[data..data + size]).is_none(),
            "a part repeats"
        );
        at = data + size;
    }
    assert_eq!(
        u32_at(at),
        0x0201_4b50,
        "the local entries of the oracle workbook end where its central directory starts"
    );
    parts
}

fn part_text<'p>(parts: &BTreeMap<String, &'p [u8]>, name: &str) -> &'p str {
    let bytes = parts
        .get(name)
        .unwrap_or_else(|| panic!("the oracle workbook has no part {name}"));
    std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The value of attribute `name` of a start tag.
fn attribute<'x>(tag: &'x str, name: &str) -> Option<&'x str> {
    let marker = format!(" {name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The error literals of the constant cells the generator writes.
const CELL_ERRORS: [(&str, WorksheetErrorCode); 7] = [
    ("#NULL!", WorksheetErrorCode::Null),
    ("#DIV/0!", WorksheetErrorCode::Div0),
    ("#VALUE!", WorksheetErrorCode::Value),
    ("#REF!", WorksheetErrorCode::Ref),
    ("#NAME?", WorksheetErrorCode::Name),
    ("#NUM!", WorksheetErrorCode::Num),
    ("#N/A", WorksheetErrorCode::NA),
];

/// A cell of the oracle sheet: a formula, in its file form, a dynamic-array formula over its one
/// cell, in its file form, or a constant.
#[derive(Debug)]
enum SheetCell {
    Formula(String),
    DynamicArray(String),
    Constant(CalcValue),
}

/// The cells of the oracle workbook's sheet error_fallback as the generator writes them,
/// `<c r="..." [t="..."]>` holding an `<f>`, a `<v>` or an inline string, or `<c r="..." cm="1">`
/// holding `<f t="array" ref="...">` over the cell itself, by address; the metadata part must mark
/// `cm="1"` as a dynamic-array formula.
fn workbook_cells(package: &[u8]) -> BTreeMap<String, SheetCell> {
    let parts = stored_parts(package);
    let workbook = part_text(&parts, "xl/workbook.xml");
    assert!(
        workbook.contains(&format!(
            "<sheet name=\"{FALLBACK_SHEET}\" sheetId=\"1\" r:id=\"rId1\"/>"
        )) && workbook.contains("<calcPr fullCalcOnLoad=\"1\"/>")
            && !workbook.contains("<workbookPr"),
        "the oracle workbook's workbook part: {workbook}"
    );
    let relationships = part_text(&parts, "xl/_rels/workbook.xml.rels");
    assert!(relationships.contains("Id=\"rId1\""));
    assert!(relationships.contains("Target=\"worksheets/sheet1.xml\""));
    assert!(relationships.contains("/relationships/sheetMetadata\" Target=\"metadata.xml\""));
    assert!(part_text(&parts, "[Content_Types].xml").contains(
        "<Override PartName=\"/xl/metadata.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml\"/>"
    ));
    let metadata = part_text(&parts, "xl/metadata.xml");
    assert!(
        metadata.contains("<metadataTypes count=\"1\"><metadataType name=\"XLDAPR\" ")
            && metadata.contains("<futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\"><xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/>")
            && metadata.contains("<cellMetadata count=\"1\"><bk><rc t=\"1\" v=\"0\"/></bk></cellMetadata>"),
        "the metadata part marks cm=\"1\" as a dynamic-array formula: {metadata}"
    );
    let xml = part_text(&parts, "xl/worksheets/sheet1.xml");
    let mut cells = BTreeMap::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<c ") {
        rest = &rest[at..];
        let tag_end = rest.find('>').expect("a cell's start tag ends");
        let tag = &rest[..tag_end];
        let close = rest.find("</c>").expect("a cell ends");
        let body = &rest[tag_end + 1..close];
        rest = &rest[close + 4..];
        let inner = |name: &str| -> Option<String> {
            let open = format!("<{name}>");
            let start = body.find(&open)? + open.len();
            let end = body[start..].find(&format!("</{name}>"))? + start;
            Some(unescape(&body[start..end]))
        };
        let address = attribute(tag, "r")
            .expect("a cell names its address")
            .to_owned();
        let value = |name: &str| inner(name).unwrap_or_else(|| panic!("{address} has no <{name}>"));
        let array_open = format!("<f t=\"array\" ref=\"{address}\">");
        let array = body.find(&array_open).map(|start| {
            let start = start + array_open.len();
            let end = body[start..].find("</f>").expect("an array formula ends") + start;
            unescape(&body[start..end])
        });
        let cell = match (inner("f"), array, attribute(tag, "t"), attribute(tag, "cm")) {
            (Some(formula), None, None, None) => SheetCell::Formula(formula),
            (None, Some(formula), None, Some("1")) => SheetCell::DynamicArray(formula),
            (None, None, None, None) => {
                SheetCell::Constant(CalcValue::number(value("v").parse().expect("a number")))
            }
            (None, None, Some("b"), None) => {
                SheetCell::Constant(CalcValue::logical(value("v") == "1"))
            }
            (None, None, Some("inlineStr"), None) => SheetCell::Constant(text_value(&value("t"))),
            (None, None, Some("e"), None) => {
                let literal = value("v");
                let code = CELL_ERRORS
                    .iter()
                    .find(|(known, _)| *known == literal)
                    .unwrap_or_else(|| panic!("{address}: {literal} is no error literal"))
                    .1;
                SheetCell::Constant(CalcValue::error(code))
            }
            (formula, array, kind, metadata) => panic!(
                "{address}: {formula:?}, array {array:?}, of type {kind:?} with cell metadata {metadata:?}"
            ),
        };
        assert!(
            cells.insert(address.clone(), cell).is_none(),
            "{address} repeats"
        );
    }
    cells
}

/// Runs the committed generator under Windows PowerShell into a new directory under the target
/// directory's temporary directory and returns the bytes of the workbook it wrote. The directory
/// is removed when the function returns or panics.
#[cfg(all(windows, not(target_arch = "wasm32")))]
fn generated_workbook() -> Vec<u8> {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    struct Removed(PathBuf);
    impl Drop for Removed {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let generator = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("make-fixtures.ps1");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("calc-fml-fixtures-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let _removed = Removed(out.clone());
    let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    let powershell = Path::new(&system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let ran = Command::new(&powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&generator)
        .arg("-OutDir")
        .arg(&out)
        .stdin(Stdio::null())
        .output()
        .expect("Windows PowerShell starts");
    assert!(
        ran.status.success(),
        "the generator failed ({:?}):\n{}\n{}",
        ran.status,
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let written =
        std::fs::read(out.join("dc_ibp2").join("oracle.xlsx")).expect("the generated workbook");
    let mut files = 0;
    let mut pending = vec![out.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("the generator's directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files += 1;
            }
        }
    }
    assert_eq!(files, 1, "the generator writes one workbook");
    written
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_ibp2__error_fallback() {
    const CASE: &str = "dc_ibp2__error_fallback";
    let mut book = FallbackBook::new();
    let run = fallback_run(&mut book);

    // Every oracle row evaluates to its hand-derived value, and the observer was shown exactly
    // that outcome, for the row's cell. A row reads a source cell of column F exactly when it is
    // marked conditional on that cell.
    let mut documented = 0;
    let mut conditions = Vec::new();
    let mut choices = Vec::new();
    let mut conditional = Vec::new();
    for row in &run {
        let shown = &row.shown;
        let sources_read: Vec<&str> = fallback_sources()
            .iter()
            .map(|source| source.address)
            .filter(|address| shown.reads.contains(&book.areas(&[*address])[0]))
            .collect();
        assert_eq!(
            sources_read,
            Vec::from_iter(row.source),
            "{}: the source cells of column F it reads",
            row.formula
        );
        if let Some(source) = row.source {
            conditional.push(format!("D{} {} reads {source}", row.row, row.formula));
        }
        assert!(
            matches!(&shown.outcome, Ok(value) if same_value(value, &row.expected)),
            "{} (D{}): {:?}, expected {:?} ({:?})",
            row.formula,
            row.row,
            shown.outcome,
            row.expected,
            row.basis
        );
        assert_eq!(
            shown.log.reported,
            [(book.anchor(row.row).cell(), shown.outcome.clone())],
            "{}",
            row.formula
        );
        match row.basis {
            Basis::Documented => documented += 1,
            Basis::DocumentedWhen(condition) => conditions.push(format!(
                "{} gives {} when {}",
                row.formula,
                value_shown(&row.expected),
                condition.text()
            )),
            Basis::Unmeasured(choice) => choices.push(format!(
                "{} gives {}: {choice}",
                row.formula,
                value_shown(&row.expected)
            )),
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} oracle rows evaluated through parse_formula, compile, instantiate and evaluate, each equal to its hand-derived value (not compared with Excel here): {} read only the workbook's constant cells and {} are conditional on the error the reader holds for the source cell of column F they read ({}), which the comparison publishes as not modelled; {documented} documented, {} documented under a condition about the workbook, {} unmeasured choices:",
        run.len(),
        run.len() - conditional.len(),
        conditional.len(),
        conditional.join(", "),
        conditions.len(),
        choices.len()
    );
    for condition in &conditions {
        note!("ACCEPT-NOTE: {CASE}: documented under a condition: {condition}");
    }
    for choice in &choices {
        note!("ACCEPT-NOTE: {CASE}: unmeasured choice: {choice}");
    }

    // The fallback is evaluated only on a caught error: the reader is asked for its areas, and
    // the calls it holds run, only then. `2^0.5` is refused by calc-func (NotImplemented) and
    // `A1:A2` where one value is needed by the evaluator (AreaAsValue), so a fallback holding
    // either would end the evaluation in a fault had it run.
    let by_formula = |formula: &str| -> &OracleRun {
        run.iter()
            .find(|row| row.formula == formula)
            .unwrap_or_else(|| panic!("{formula} is no oracle row"))
    };
    let lazy: [(&str, &[&str], &[&str]); 12] = [
        ("=IFERROR(1/0,5)", &[], &["Infix /", "Call IFERROR"]),
        ("=IFERROR(2,1/0)", &[], &["Call IFERROR"]),
        ("=IFNA(#N/A,1)", &[], &["Call IFNA"]),
        ("=IFNA(#DIV/0!,1)", &[], &["Call IFNA"]),
        ("=IFERROR(A2,B7)", &["A2", "B7"], &["Call IFERROR"]),
        ("=IFERROR(A1,B7)", &["A1"], &["Call IFERROR"]),
        ("=IFNA(A2,\"caught\")", &["A2"], &["Call IFNA"]),
        ("=IFERROR(1,2^0.5)", &[], &["Call IFERROR"]),
        ("=IFERROR(1,A1:A2)", &[], &["Call IFERROR"]),
        ("=IFNA(A1,2^0.5)", &["A1"], &["Call IFNA"]),
        (
            "=IFERROR(IFERROR(1/0,#N/A),2)",
            &[],
            &["Infix /", "Call IFERROR", "Call IFERROR"],
        ),
        (
            "=1+IFERROR(1/0,5)",
            &[],
            &[
                "Infix /",
                "Call IFERROR",
                "Infix +",
                "Prefix - *",
                "Prefix - *",
            ],
        ),
    ];
    for (formula, reads, calls) in lazy {
        let shown = &by_formula(formula).shown;
        assert_eq!(shown.reads, book.areas(reads), "{formula}: areas read");
        assert_eq!(shown.log.calls, calls, "{formula}: calls run");
    }
    // IFNA passes an error other than #N/A on without evaluating its fallback, whose read of B7
    // would show. Not an oracle row (the oracle workbook and its capture hold no IFNA over another
    // error with an observable fallback), so its hand-derived value, #DIV/0! (IFNA's page: every
    // error but #N/A is the result), is not compared with Excel.
    let (formula, reads, calls): (&str, &[&str], &[&str]) =
        ("=IFNA(A2,B7)", &["A2"], &["Call IFNA"]);
    let passed_on = book.evaluate(formula, book.anchor(100));
    assert_eq!(
        passed_on.outcome,
        Ok(CalcValue::error(WorksheetErrorCode::Div0)),
        "{formula}"
    );
    assert_eq!(passed_on.reads, book.areas(reads), "{formula}: areas read");
    assert_eq!(passed_on.log.calls, calls, "{formula}: calls run");
    // One plan, two evaluations: B7 is read once A1 holds an error, and not before.
    let anchor = book.anchor(by_formula("=IFERROR(A1,B7)").row);
    let plan = book.compile("=IFERROR(A1,B7)", anchor).expect("compiles");
    let instance = instantiate(&plan, anchor);
    let before = book.run(&instance);
    book.reader
        .store(book.sheet, "A1", CalcValue::error(WorksheetErrorCode::NA));
    let after = book.run(&instance);
    book.reader.store(book.sheet, "A1", CalcValue::number(5.0));
    assert_eq!(
        (before.outcome, before.reads),
        (Ok(CalcValue::number(5.0)), book.areas(&["A1"]))
    );
    assert_eq!(
        (after.outcome, after.reads),
        (Ok(CalcValue::number(2.0)), book.areas(&["A1", "B7"]))
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the reader logs and the observer show each fallback evaluated only on a caught error: {} rows checked area by area and call by call (a fallback of 2^0.5, which calc-func refuses, and of A1:A2, which the evaluator refuses where one value is needed, never runs); =IFNA(A2,B7), not an oracle row and not compared with Excel, gives #DIV/0! (hand-derived) reading A2 alone, so IFNA passes a non-#N/A error on without reading its fallback; one plan of =IFERROR(A1,B7) reads A1 alone while A1 is 5 and A1 then B7 while A1 is #N/A",
        lazy.len()
    );

    // Rows not compared with Excel: errors read from cells the oracle workbook does not hold, typed
    // refusals, and faults, each with the areas read.
    let caret = catalogue()
        .resolve_surface(Notation::Infix, "^")
        .expect("^")
        .id();
    let at = book.anchor(100);
    for (index, code) in ELSEWHERE_ERRORS.iter().enumerate() {
        let cell = format!("elsewhere!A{}", index + 1);
        let caught = book.evaluate(&format!("=IFERROR({cell},1)"), at);
        assert_eq!(
            caught.outcome,
            Ok(CalcValue::number(1.0)),
            "{code:?} ({NOT_IN_WORKBOOK})"
        );
        let passed = book.evaluate(&format!("=IFNA({cell},1)"), at);
        assert_eq!(
            passed.outcome,
            Ok(CalcValue::error(*code)),
            "IFNA passes {code:?} on"
        );
    }
    let missing_input = book.evaluate("=IFERROR(elsewhere!D1,A1)", at);
    assert_eq!(
        (
            missing_input.outcome,
            missing_input.reads,
            missing_input.log.calls
        ),
        (
            Err(EvalFault::MissingInput {
                cell: cell_at(book.elsewhere, "D1")
            }),
            book.areas(&["elsewhere!D1"]),
            vec!["Call IFERROR".to_owned()]
        ),
        "a value the reader cannot supply stops the call; the fallback is not read"
    );
    let unread = book.evaluate("=IFERROR(1,elsewhere!D1)", at);
    assert_eq!(
        (unread.outcome, unread.reads),
        (Ok(CalcValue::number(1.0)), vec![])
    );
    let refused = book.evaluate("=IFERROR(2^0.5,A1)", at);
    assert!(
        matches!(
            &refused.outcome,
            Err(EvalFault::Function(FunctionFault::NotImplemented { id, .. })) if *id == caret
        ),
        "{:?}",
        refused.outcome
    );
    assert_eq!(
        refused.reads,
        vec![],
        "a refused value: the fallback is not read"
    );
    assert_eq!(refused.log.calls, ["Infix ^", "Call IFERROR"]);
    for (formula, area) in [
        ("=IFERROR(A1:A2,0)", "A1:A2"),
        ("=IFNA(#N/A,A1:A2)", "A1:A2"),
        ("=SUM(IFERROR(A1:A3,0))", "A1:A3"),
    ] {
        assert_eq!(
            book.evaluate(formula, at).outcome,
            Err(EvalFault::AreaAsValue {
                area: book.areas(&[area])[0]
            }),
            "{formula}: a range where one value is needed (array lifting is cfml-m3.4's)"
        );
    }
    let compiled = |book: &mut FallbackBook, formula: &str| book.compile(formula, at).err();
    assert!(matches!(
        compiled(&mut book, "=IFERROR({1,#N/A},0)"),
        Some(CompileError::Unsupported {
            owner: "cfml-m3.4",
            ..
        })
    ));
    assert!(matches!(
        compiled(&mut book, "=IFERROR(1)"),
        Some(CompileError::Arity { supplied: 1, .. })
    ));
    assert!(matches!(
        compiled(&mut book, "=IFNA(1,2,3)"),
        Some(CompileError::Arity { supplied: 3, .. })
    ));
    // The package form of IFNA, as a workbook file stores it, compiles to IFNA (compile reads
    // `_xlfn.` names as the catalogued function, bead dc-besk) and gives IFNA's result.
    let packaged = book.evaluate("=_xlfn.IFNA(#N/A,1)", book.anchor(100));
    assert_eq!(
        packaged.outcome,
        Ok(CalcValue::number(1.0)),
        "=_xlfn.IFNA(#N/A,1)"
    );
    // The source formulas of column F are not compiled: calc-func's catalogue has none of their
    // functions, so the rows that read them read the error the reader holds for each.
    let sources = source_refusals(&mut book);
    for (source, refused) in &sources {
        let function = &source.formula[1..source.formula.find('(').expect("a call")];
        assert!(
            matches!(refused, CompileError::UnknownFunction { spelling, .. } if spelling == function),
            "{}: {refused:?}",
            source.formula
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the source formulas of column F are refused at compile as unknown functions ({}); the rows that read them read the errors the reader holds for them ({}), which the comparison checks against Excel's capture of those cells",
        sources
            .iter()
            .map(|(source, _)| format!("{} {}", source.address, source.formula))
            .collect::<Vec<_>>()
            .join(", "),
        sources
            .iter()
            .map(|(source, _)| format!(
                "{} {}",
                source.address,
                value_shown(&CalcValue::error(source.assumed))
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    note!(
        "ACCEPT-NOTE: {CASE}: not compared with Excel: IFERROR catches and IFNA passes on #CALC!, #SPILL!, #FIELD!, #BLOCKED!, #CONNECT! and #BUSY! read from cells of the sheet elsewhere (unmeasured choice: {NOT_IN_WORKBOOK}); a value that reads an unresolved input stops the call with MissingInput and a value calc-func refuses with NotImplemented, neither reading its fallback; a range as IFERROR's or IFNA's value or fallback, and IFERROR over a range inside SUM, is refused with AreaAsValue (array lifting is cfml-m3.4's); an array constant is refused at compile (cfml-m3.4); one and three arguments are refused at compile; the package form =_xlfn.IFNA(#N/A,1) compiles to IFNA (bead dc-besk) and gives 1"
    );

    // The oracle workbook's sheet holds exactly the rows' formulas, in their file form, in
    // column D in row order, and the cells the reader holds on that sheet.
    let cells = workbook_cells(FALLBACK_WORKBOOK);
    let mut expected: BTreeMap<String, SheetCell> = run
        .iter()
        .map(|row| {
            (
                format!("D{}", row.row),
                SheetCell::Formula(file_form(row.formula)),
            )
        })
        .collect();
    for (address, value) in fallback_cells() {
        expected.insert(address.to_owned(), SheetCell::Constant(value));
    }
    for source in fallback_sources() {
        expected.insert(
            source.address.to_owned(),
            SheetCell::DynamicArray(file_form(source.formula)),
        );
    }
    let differences: Vec<String> = expected
        .keys()
        .chain(cells.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(
            |address| match (expected.get(*address), cells.get(*address)) {
                (Some(SheetCell::Formula(a)), Some(SheetCell::Formula(b)))
                | (Some(SheetCell::DynamicArray(a)), Some(SheetCell::DynamicArray(b))) => a != b,
                (Some(SheetCell::Constant(a)), Some(SheetCell::Constant(b))) => !same_value(a, b),
                _ => true,
            },
        )
        .map(|address| {
            format!(
                "{address}: the case has {:?}, the workbook {:?}",
                expected.get(address),
                cells.get(address)
            )
        })
        .collect();
    assert!(
        differences.is_empty(),
        "the oracle workbook's sheet {FALLBACK_SHEET} differs from the case:\n{}",
        differences.join("\n")
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {FALLBACK_WORKBOOK_PATH} (sha256 {}): sheet {FALLBACK_SHEET} holds exactly the {} oracle rows' formulas in column D in row order (IFNA in its package form _xlfn.IFNA), the {} constant cells the rows read and the {} source formulas of column F as dynamic-array formulas (cell metadata naming XLDAPR)",
        sha256_hex(FALLBACK_WORKBOOK),
        run.len(),
        fallback_cells().len(),
        fallback_sources().len()
    );
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_workbook();
        assert!(
            generated == FALLBACK_WORKBOOK,
            "the committed generator writes a workbook (sha256 {}) that differs from the committed one (sha256 {})",
            sha256_hex(&generated),
            sha256_hex(FALLBACK_WORKBOOK)
        );
        note!(
            "ACCEPT-NOTE: {CASE}: the committed generator tests/fixtures/make-fixtures.ps1, run into a new directory under the target directory's temporary directory (removed when the generator's run returns or panics), wrote the committed workbook byte for byte"
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {CASE}: the check that the committed generator tests/fixtures/make-fixtures.ps1 writes the committed workbook byte for byte did not run on this target: the generator is a Windows PowerShell script, which the case runs only in its native Windows build"
        );
    }
}

/// The calculation context the rows calculate under: the oracle workbook declares no
/// `workbookPr` and a `calcPr` with no calculation setting, so every setting is at its
/// SpreadsheetML default (checked by [`workbook_cells`]).
fn fallback_context() -> CalculationContext {
    CalculationContext {
        date1904: false,
        iteration: false,
        max_iterations: 100,
        max_change: 0.001,
        precision_as_displayed: false,
    }
}

fn fallback_address(row: u32) -> CellAddress {
    CellAddress::new(FALLBACK_SHEET, row, FALLBACK_COLUMN).expect("a cell of the grid")
}

/// The address of a source cell of column F on the sheet error_fallback.
fn source_address(source: &SourceCell) -> CellAddress {
    let digits = source
        .address
        .find(|c: char| c.is_ascii_digit())
        .expect("a row");
    assert_eq!(
        &source.address[..digits],
        "F",
        "a source cell is in column F"
    );
    CellAddress::new(
        FALLBACK_SHEET,
        source.address[digits..].parse().expect("a row number"),
        6,
    )
    .expect("a cell of the grid")
}

/// What the run publishes, judged by calc_verdict: a result for each oracle row's cell, calculated
/// at its tick (or failed with its fault), a not-modelled result for each source cell of column F
/// with calc-fml's compile refusal, and the evaluation record made from what the observer was
/// shown, each evaluation with the outcome the evaluator reported. A row that reads a source cell
/// is not calculated from the workbook: the product route does not calculate its source, and the
/// row's value rests on the error the reader holds for it. It is published not modelled, with the
/// value it is conditional on in its diagnostic, never as calculated.
fn fallback_documents(
    run: &[OracleRun],
    sources: &[(SourceCell, CompileError)],
) -> (ResultSet, InstrumentRecord) {
    let source = format!("sha256:{}", sha256_hex(FALLBACK_WORKBOOK));
    let context = fallback_context().fingerprint();
    let mut results = Vec::new();
    let mut evaluations = Vec::new();
    for row in run {
        let cell = fallback_address(row.row);
        let (provenance, value) = match (&row.shown.outcome, row.source) {
            (Ok(value), None) => (
                Provenance::Calculated {
                    tick: row.tick,
                    revision: FALLBACK_REVISION,
                    context: context.clone(),
                },
                Some(value.clone()),
            ),
            (Ok(value), Some(read)) => {
                let (source, refused) = sources
                    .iter()
                    .find(|(source, _)| source.address == read)
                    .unwrap_or_else(|| panic!("{read} is no source cell"));
                (
                    Provenance::NotModeled {
                        feature: format!("reads {read}, {}", source.feature),
                        diagnostic: format!(
                            "conditional on Excel's value of {read}, whose formula {} calc_fml::compile refuses ({refused:?}): with the reader holding {} for {read}, calc_fml::evaluate gives {}",
                            source.formula,
                            value_shown(&CalcValue::error(source.assumed)),
                            value_shown(value)
                        ),
                    },
                    None,
                )
            }
            (Err(fault), _) => (
                Provenance::Failed {
                    fault: format!("{fault:?}"),
                },
                None,
            ),
        };
        results.push(CellResult {
            cell,
            provenance,
            value,
        });
        for (key, returned) in &row.shown.log.reported {
            assert_eq!(key.row(), row.row);
            assert_eq!(key.column(), FALLBACK_COLUMN);
            evaluations.push(InstrumentedEvaluation::new(
                VerdictEvaluation {
                    cell: CellAddress::new(FALLBACK_SHEET, key.row(), key.column())
                        .expect("a cell"),
                    tick: row.tick,
                    revision: FALLBACK_REVISION,
                    context: context.clone(),
                },
                returned.clone().map_err(|fault| format!("{fault:?}")),
            ));
        }
    }
    for (source, refused) in sources {
        results.push(CellResult {
            cell: source_address(source),
            provenance: Provenance::NotModeled {
                feature: source.feature.to_owned(),
                diagnostic: format!("calc_fml::compile refuses {}: {refused:?}", source.formula),
            },
            value: None,
        });
    }
    (
        ResultSet {
            case: FALLBACK_CASE.to_owned(),
            source: source.clone(),
            run: RunIdentity {
                revision: FALLBACK_REVISION,
                context,
            },
            results,
        },
        InstrumentRecord {
            case: FALLBACK_CASE.to_owned(),
            source,
            evaluations,
        },
    )
}

/// The verdict on `capture` of what `run` publishes, and what on it disagrees with the rows: a
/// formula cell of the sheet that no row or source has, a row that reads only constant cells whose
/// verdict is not `matched`, a conditional row whose verdict is not `not_modeled` or whose raw
/// value in the capture is not the value calc_fml::evaluate gave it, a source cell whose verdict
/// is not `not_modeled` or whose raw value in the capture is not the error the rows assume, and a
/// row whose cell the inventory lacks. A conditional row that agrees is counted apart from the
/// calculated rows: its agreement holds given Excel's value of its source cell, which the source
/// cell's check compares with the error the reader held.
fn fallback_comparison(
    capture: &CaptureRecord,
    run: &[OracleRun],
    sources: &[(SourceCell, CompileError)],
) -> Comparison {
    let (results, evaluations) = fallback_documents(run, sources);
    let policy = VerdictPolicy {
        numeric_mode: NumericMode::Fast,
        evidence: None,
    };
    let report = judge(capture, &results, &evaluations, &policy)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    let rows: BTreeMap<u32, &OracleRun> = run.iter().map(|row| (row.row, row)).collect();
    let mut problems = Vec::new();
    let mut conditional_agreed = 0;
    for cell in &report.cells {
        let address = &cell.formula.cell;
        let at = format!("{}!{}", address.sheet(), address.a1());
        let raw = capture
            .observations
            .iter()
            .find(|observed| observed.cell == *address)
            .map(|observed| &observed.observation);
        if let Some((source, _)) = sources
            .iter()
            .find(|(source, _)| source_address(source) == *address)
        {
            let assumed = CalcValue::error(source.assumed);
            match (&cell.verdict, raw) {
                (Verdict::NotModeled { .. }, Some(Observation::ExcelRaw(value)))
                    if same_value(value, &assumed) => {}
                (verdict, raw) => problems.push(format!(
                    "{at} {}: the source of rows that assume {}: {}, Excel's raw value {raw:?}",
                    source.formula,
                    value_shown(&assumed),
                    verdict.name()
                )),
            }
            continue;
        }
        let row = (address.sheet() == FALLBACK_SHEET && address.column() == FALLBACK_COLUMN)
            .then(|| rows.get(&address.row()))
            .flatten();
        match (row, &cell.verdict) {
            (None, verdict) => problems.push(format!(
                "{at}: a formula cell that no oracle row has ({})",
                verdict.name()
            )),
            (Some(row), verdict) if row.source.is_some() => {
                let read = row.source.unwrap_or_default();
                match (verdict, raw, &row.shown.outcome) {
                    (Verdict::NotModeled { .. }, Some(Observation::ExcelRaw(excel)), Ok(value))
                        if same_value(excel, value) =>
                    {
                        conditional_agreed += 1;
                    }
                    (verdict, raw, outcome) => problems.push(format!(
                        "{at} {}: conditional on {read}: {}, calc_fml::evaluate gave {outcome:?}, Excel's raw value {raw:?} ({:?})",
                        row.formula,
                        verdict.name(),
                        row.basis
                    )),
                }
            }
            (Some(_), Verdict::Matched { .. }) => {}
            (
                Some(row),
                Verdict::Mismatched {
                    expected, actual, ..
                },
            ) => problems.push(format!(
                "{at} {}: calc_fml::evaluate gave {actual:?}, Excel's raw value is {expected:?} ({:?})",
                row.formula, row.basis
            )),
            (Some(row), verdict) => problems.push(format!(
                "{at} {}: {}, not matched: {verdict:?}",
                row.formula,
                verdict.name()
            )),
        }
    }
    for outside in &report.outside_inventory {
        problems.push(format!(
            "{}!{}: the capture's inventory lacks this oracle row's cell",
            outside.cell.sheet(),
            outside.cell.a1()
        ));
    }
    Comparison {
        report,
        problems,
        conditional_agreed,
    }
}

/// The verdict on a capture, what disagrees with the rows, and how many conditional rows agree.
struct Comparison {
    report: VerdictReport,
    problems: Vec<String>,
    conditional_agreed: usize,
}

/// A capture record that observes, for each row, the value `observe` gives it, and for each source
/// cell of column F, as a dynamic-array formula, the value `source_raw` gives it: the shape of a
/// version 2 live capture of the oracle workbook as committed, by the committed worker. It is
/// made here, so it is evidence about the comparison's documents, not about Excel.
fn made_capture(
    run: &[OracleRun],
    sources: &[(SourceCell, CompileError)],
    observe: impl Fn(&OracleRun) -> CalcValue,
    source_raw: impl Fn(&SourceCell) -> CalcValue,
) -> CaptureRecord {
    let source = format!("sha256:{}", sha256_hex(FALLBACK_WORKBOOK));
    CaptureRecord {
        case: FALLBACK_CASE.to_owned(),
        source: source.clone(),
        // This record is built by the test and is not Excel's. calc_verdict has no acquisition
        // kind for a made record: a FileCacheReplay record may hold no raw observation (and
        // calc_verdict::judge refuses one that does), and LegacyEvidence names an entry of a legacy
        // evidence record.
        acquisition: Acquisition::LiveCapture,
        environment: None,
        sheets: vec![SheetEntry {
            name: FALLBACK_SHEET.to_owned(),
            visibility: SheetVisibility::Visible,
        }],
        formulas: run
            .iter()
            .map(|row| FormulaCell {
                cell: fallback_address(row.row),
                kind: FormulaKind::Ordinary,
                formula: Some(row.formula.to_owned()),
                dynamic_array: false,
            })
            .chain(sources.iter().map(|(source, _)| FormulaCell {
                cell: source_address(source),
                kind: FormulaKind::Ordinary,
                formula: Some(source.formula.to_owned()),
                dynamic_array: true,
            }))
            .collect(),
        observations: run
            .iter()
            .map(|row| CellObservation {
                cell: fallback_address(row.row),
                observation: Observation::ExcelRaw(observe(row)),
            })
            .chain(sources.iter().map(|(source, _)| CellObservation {
                cell: source_address(source),
                observation: Observation::ExcelRaw(source_raw(source)),
            }))
            .collect(),
        session: Some(CaptureSession {
            revision: FALLBACK_REVISION,
            operations: Vec::new(),
            context: fallback_context().fingerprint(),
            tool: format!("worker sha256:{}", sha256_hex(CAPTURE_WORKER)),
            input_package: source,
            output_package: None,
            failures: Vec::new(),
            shared_ranges: Vec::new(),
            saved: Vec::new(),
            reopened: Vec::new(),
        }),
    }
}

/// Judges an Excel capture of the oracle workbook against `run`: it must be a live capture of the
/// workbook as committed, by the committed worker, with Excel's environment and no failure that
/// leaves its raw values unobserved or untrusted (isolation, environment, inventory, value or
/// spill); each
/// row's cell must hold the row's formula as an ordinary formula and each source cell its formula
/// as a dynamic-array formula, and each condition a row's documented value rests on must hold;
/// every row that reads only constant cells must be calculated and matched, every conditional row
/// must be not modelled and agree with Excel given its source cell's value, and every source cell
/// must hold the error the rows assume. `what` says which capture it is, for the note.
fn judge_excel_capture(
    case: &str,
    what: &str,
    bytes: &[u8],
    run: &[OracleRun],
    sources: &[(SourceCell, CompileError)],
) {
    let capture = CaptureRecord::decode(bytes)
        .unwrap_or_else(|refusal| panic!("{what} does not decode: {refusal}"));
    assert_eq!(capture.case, FALLBACK_CASE, "{what}");
    assert_eq!(capture.acquisition, Acquisition::LiveCapture, "{what}");
    let workbook = format!("sha256:{}", sha256_hex(FALLBACK_WORKBOOK));
    assert_eq!(
        capture.source, workbook,
        "{what}: not of the oracle workbook as committed"
    );
    let session = capture.session.as_ref().expect("a version 2 record");
    assert_eq!(session.input_package, workbook, "{what}");
    let worker = format!("worker sha256:{}", sha256_hex(CAPTURE_WORKER));
    assert!(
        session.tool.split("; ").any(|part| part == worker),
        "{what}: the capture-tool revision {:?} does not name the committed worker ({worker})",
        session.tool
    );
    // A failure that leaves the raw values unobserved or untrusted fails the comparison; a failure
    // of a later stage (the saved cache, the value after reopening) concerns no raw value and is
    // reported. "value" and "spill" (a cell that spills gets no raw value) are the stage names of
    // calc-oracle-excel's conversion, a native-only package this portable package cannot depend on.
    let raw_stages = [
        STAGE_ISOLATION,
        STAGE_ENVIRONMENT,
        STAGE_INVENTORY,
        "value",
        "spill",
    ];
    let unobserved: Vec<_> = session
        .failures
        .iter()
        .filter(|failure| raw_stages.contains(&failure.stage.as_str()))
        .collect();
    assert!(
        unobserved.is_empty(),
        "{what} records failures of its raw values: {unobserved:?}"
    );
    let later: Vec<String> = session
        .failures
        .iter()
        .map(|failure| {
            let at = failure
                .cell
                .as_ref()
                .map(|cell| format!(" {}!{}", cell.sheet(), cell.a1()))
                .unwrap_or_default();
            format!("[{}]{at}: {}", failure.stage, failure.message)
        })
        .collect();
    let environment = capture
        .environment
        .as_ref()
        .unwrap_or_else(|| panic!("{what} records no Excel environment"));

    // The capture is of the rows: each row's cell holds its formula as Excel reports it, an
    // ordinary formula, and each source cell its formula as a dynamic-array formula. The conditions
    // the pages' values rest on hold.
    let formula_cell = |address: &CellAddress| {
        capture
            .formulas
            .iter()
            .find(|formula| formula.cell == *address)
            .unwrap_or_else(|| panic!("{what}: no formula cell {}", address.a1()))
    };
    let raw_value = |address: &CellAddress| {
        capture
            .observations
            .iter()
            .find(|observed| observed.cell == *address)
            .map(|observed| &observed.observation)
    };
    let mut conditions_held = Vec::new();
    for row in run {
        let address = fallback_address(row.row);
        let cell = formula_cell(&address);
        let condition = match row.basis {
            Basis::DocumentedWhen(condition) => Some(condition),
            _ => None,
        };
        let reported = match condition {
            Some(Condition::LegacyForm) => format!("=@{}", &row.formula[1..]),
            _ => row.formula.to_owned(),
        };
        assert_eq!(
            (cell.formula.as_deref(), cell.dynamic_array),
            (Some(reported.as_str()), false),
            "{what}: D{} {}: Excel's formula text and dynamic-array flag",
            row.row,
            row.formula
        );
        match condition {
            Some(Condition::EmptyTextCell) => {
                assert!(
                    matches!(raw_value(&address), Some(Observation::ExcelRaw(value)) if same_value(value, &text_value(""))),
                    "{what}: D{} {}: {}: Excel's raw value {:?}",
                    row.row,
                    row.formula,
                    Condition::EmptyTextCell.text(),
                    raw_value(&address)
                );
                conditions_held.push(format!(
                    "D{} {} is empty text in Excel (A5 loads as empty text; the blank A4 gives 0)",
                    row.row, row.formula
                ));
            }
            Some(Condition::LegacyForm) => conditions_held.push(format!(
                "D{} {} is no dynamic-array formula in the capture and Excel reports it as {reported}",
                row.row, row.formula
            )),
            None => {}
        }
    }
    for (source, _) in sources {
        let cell = formula_cell(&source_address(source));
        assert_eq!(
            (cell.formula.as_deref(), cell.dynamic_array),
            (Some(source.formula), true),
            "{what}: {} {}",
            source.address,
            source.formula
        );
    }

    let Comparison {
        report,
        problems,
        conditional_agreed,
    } = fallback_comparison(&capture, run, sources);
    assert!(
        problems.is_empty(),
        "{} problems in the verdict on {what} by Excel {} ({}):\n{}",
        problems.len(),
        environment.target.excel_build(),
        report.summary(),
        problems.join("\n")
    );
    let conditional = run.iter().filter(|row| row.source.is_some()).count();
    let calculated = run.len() - conditional;
    assert_eq!(conditional_agreed, conditional);
    assert_eq!(
        (
            report.counts.calculated,
            report.counts.matched,
            report.counts.mismatched
        ),
        (calculated as u64, calculated as u64, 0)
    );
    let unmeasured: Vec<String> = run
        .iter()
        .filter(|row| row.source.is_none() && matches!(row.basis, Basis::Unmeasured(_)))
        .map(|row| format!("D{} {}", row.row, row.formula))
        .collect();
    note!(
        "ACCEPT-NOTE: {case}: {what}, taken by Excel {}: through calc_verdict::judge, calculated {calculated} of {} formula cells, matched {} of {calculated} calculated, mismatched {}; the {conditional} conditional rows are not modelled (not counted as calculated) and each agrees with Excel's raw value given Excel's value of the source cell it reads; each of the {} source cells of column F is not modelled and holds the error the rows assume ({})",
        environment.target.excel_build(),
        report.counts.denominator,
        report.counts.matched,
        report.counts.mismatched,
        sources.len(),
        report.summary()
    );
    note!(
        "ACCEPT-NOTE: {case}: {what}: of the matched rows, {} were labelled unmeasured choices before the capture and agree with Excel: {}",
        unmeasured.len(),
        unmeasured.join(", ")
    );
    note!(
        "ACCEPT-NOTE: {case}: {what}: each row's cell holds the row's formula as an ordinary formula (the legacy-form row's with Excel's @) and each source cell its formula as a dynamic-array formula; conditions of the pages' values that hold in the capture: {}",
        conditions_held.join("; ")
    );
    note!(
        "ACCEPT-NOTE: {case}: {what}: COM add-ins connected during the capture {:?}; capture failures of stages after the raw values (counted, not compared): {}",
        report.com_add_ins_connected,
        if later.is_empty() {
            "none".to_owned()
        } else {
            later.join("; ")
        }
    );
}

/// A live capture of the oracle workbook through `cargo xtask capture`, the one capture route,
/// with a target directory of its own: the record's bytes, or why none was written. Its record,
/// work directory and `capture.log` (the command, its exit status and its full standard output and
/// standard error) go to a new directory under the workspace's `target/accept/dc-ibp2/excel-route/`,
/// which is kept and named in the result, so nothing is left in the temp directory. The command
/// runs from the workspace root with the paths relative to it, as the committed capture's command
/// was run: given the work directory as a verbatim (`\\?\`) path, which canonicalizing the root
/// gives on Windows, Excel's Workbooks.Open failed in the worker.
#[cfg(all(windows, not(target_arch = "wasm32")))]
fn live_capture() -> Result<Vec<u8>, String> {
    use std::path::Path;
    use std::process::{Command, Stdio};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root, two directories above calc-fml's manifest");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let place = format!(
        "target/accept/dc-ibp2/excel-route/live-capture-{}-{stamp}",
        std::process::id()
    );
    let directory = root.join(&place);
    std::fs::create_dir_all(&directory).expect("a new directory");
    let out = directory.join("capture.json");
    let log = directory.join("capture.log");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let ran = Command::new(cargo)
        .current_dir(root)
        .args([
            "xtask",
            "capture",
            "--fixture",
            FALLBACK_WORKBOOK_PATH,
            "--case",
            FALLBACK_CASE,
            "--out",
        ])
        .arg(format!("{place}/capture.json"))
        .arg("--work-dir")
        .arg(format!("{place}/work"))
        .env(
            "CARGO_TARGET_DIR",
            root.join("target").join("dc-ibp2-capture"),
        )
        .stdin(Stdio::null())
        .output();
    let transcript = match &ran {
        Ok(ran) => format!(
            "exit: {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}\n",
            ran.status.code(),
            String::from_utf8_lossy(&ran.stdout),
            String::from_utf8_lossy(&ran.stderr)
        ),
        Err(e) => format!("did not start: {e}\n"),
    };
    std::fs::write(
        &log,
        format!(
            "cargo xtask capture --fixture {FALLBACK_WORKBOOK_PATH} --case {FALLBACK_CASE} --out {place}/capture.json --work-dir {place}/work (CARGO_TARGET_DIR target/dc-ibp2-capture)\n{transcript}"
        ),
    )
    .expect("the capture log is written");
    let ran =
        ran.map_err(|e| format!("cargo xtask capture did not start ({place}/capture.log): {e}"))?;
    match std::fs::read(&out) {
        Ok(bytes) if ran.status.success() => Ok(bytes),
        _ => {
            let stderr = String::from_utf8_lossy(&ran.stderr);
            let lines: Vec<&str> = stderr
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();
            let last = lines
                .iter()
                .rev()
                .find(|line| line.starts_with("xtask capture:"))
                .or(lines.last())
                .copied()
                .unwrap_or("");
            Err(format!(
                "cargo xtask capture exited {:?} and wrote no record; its full output is in {place}/capture.log; its last line of standard error from xtask capture (or the last line, if it wrote none): {last}",
                ran.status.code()
            ))
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_ibp2__excel_comparison() {
    const CASE: &str = "dc_ibp2__excel_comparison";
    let mut book = FallbackBook::new();
    let run = fallback_run(&mut book);
    let sources = source_refusals(&mut book);
    let assumed = |source: &SourceCell| CalcValue::error(source.assumed);

    // The comparison's documents, judged against captures made here (evidence about the
    // documents and the judge, not about Excel): observing every row's hand-derived value and
    // every source's assumed error, each row is matched and each source not modelled; observing
    // one row otherwise, that row alone is mismatched; observing one source otherwise, that source
    // alone is reported.
    let agreeing = made_capture(&run, &sources, |row| row.expected.clone(), assumed);
    let Comparison {
        report,
        problems,
        conditional_agreed,
    } = fallback_comparison(&agreeing, &run, &sources);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert_eq!(report.evaluation_record_version, 2);
    let conditional: Vec<&OracleRun> = run.iter().filter(|row| row.source.is_some()).collect();
    let calculated = run.len() - conditional.len();
    let not_modeled = report
        .cells
        .iter()
        .filter(|cell| matches!(cell.verdict, Verdict::NotModeled { .. }))
        .count();
    assert_eq!(
        (
            report.counts.calculated,
            report.counts.matched,
            not_modeled,
            conditional_agreed
        ),
        (
            calculated as u64,
            calculated as u64,
            sources.len() + conditional.len(),
            conditional.len()
        ),
        "{}",
        report.summary()
    );
    let changed_row = 1;
    let changed = made_capture(
        &run,
        &sources,
        |row| {
            if row.row == changed_row {
                CalcValue::number(6.0)
            } else {
                row.expected.clone()
            }
        },
        assumed,
    );
    let problems = fallback_comparison(&changed, &run, &sources).problems;
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("error_fallback!D1 =IFERROR(1/0,5): calc_fml::evaluate gave"));
    let first_conditional = conditional[0];
    let changed_conditional = made_capture(
        &run,
        &sources,
        |row| {
            if row.row == first_conditional.row {
                CalcValue::number(2.0)
            } else {
                row.expected.clone()
            }
        },
        assumed,
    );
    let problems = fallback_comparison(&changed_conditional, &run, &sources).problems;
    assert_eq!(problems.len(), 1, "{problems:?}");
    let conditional_problem = format!(
        "error_fallback!D{} {}: conditional on {}: not_modeled,",
        first_conditional.row,
        first_conditional.formula,
        first_conditional.source.unwrap_or_default()
    );
    assert!(
        problems[0].starts_with(&conditional_problem),
        "{problems:?}"
    );
    let changed_source = made_capture(
        &run,
        &sources,
        |row| row.expected.clone(),
        |source| {
            if source.address == "F1" {
                CalcValue::error(WorksheetErrorCode::NA)
            } else {
                assumed(source)
            }
        },
    );
    let problems = fallback_comparison(&changed_source, &run, &sources).problems;
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with(
            "error_fallback!F1 =FILTER(B6:B7,B6:B7>100): the source of rows that assume #Calc"
        ),
        "{problems:?}"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: against captures made by the case from the hand-derived values and the assumed source errors (not Excel's), calc_verdict::judge counts {calculated} rows calculated and matches them, and gives not_modeled to the {} conditional rows ({}), which the comparison counts apart as agreeing given their source cells' values, and to the {} source cells ({}); with D1 observed as 6 it mismatches D1 alone, with D{} observed as 2 the comparison reports D{} alone, and with F1 observed as #N/A it reports F1 alone; the evaluation record is version {} (each evaluation with the outcome the evaluator reported to the observer)",
        conditional.len(),
        conditional
            .iter()
            .map(|row| format!("D{} reads {}", row.row, row.source.unwrap_or_default()))
            .collect::<Vec<_>>()
            .join(", "),
        sources.len(),
        report.summary(),
        first_conditional.row,
        first_conditional.row,
        report.evaluation_record_version
    );

    // Excel's capture, as committed.
    match MissingCapture::recognize(FALLBACK_CAPTURE) {
        Some(missing) => {
            assert_eq!(
                (
                    missing.case.as_str(),
                    missing.fixture.as_str(),
                    missing.command.as_str()
                ),
                (
                    FALLBACK_CASE,
                    FALLBACK_WORKBOOK_PATH,
                    FALLBACK_CAPTURE_COMMAND
                ),
                "the absent-capture marker names this capture"
            );
            note!("ACCEPT-NOT-RUN: {CASE}: {missing}");
        }
        None => judge_excel_capture(
            CASE,
            "the committed capture",
            FALLBACK_CAPTURE,
            &run,
            &sources,
        ),
    }

    // On the excel route, a live capture taken now, judged the same way.
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    if std::env::var("DNACALC_ACCEPT_PLATFORM").as_deref() == Ok("excel") {
        match live_capture() {
            Ok(bytes) => judge_excel_capture(
                CASE,
                "a live capture taken on the excel route",
                &bytes,
                &run,
                &sources,
            ),
            Err(why) => note!("ACCEPT-NOT-RUN: {CASE}: the excel route's live capture: {why}"),
        }
    }
}

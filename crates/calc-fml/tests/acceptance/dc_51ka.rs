//! The bodies of the dc-51ka cases: INDIRECT resolving its text to a reference through calc_fml.
//!
//! Each oracle row is a formula of column D of the sheet indirect of the oracle workbook
//! `tests/fixtures/dc_51ka/oracle.xlsx`, which the committed generator
//! `tests/fixtures/make-fixtures.ps1` writes, and runs through the production route
//! (parse_formula, compile, instantiate, evaluate) with the case's own catalogue of the workbook's
//! 42 sheets (`EvaluationServices::with_catalog`; the workbook's two defined names are not bound),
//! a reader holding the workbook's constant cells and an observer that records the calls, the
//! areas INDIRECT resolved and the outcome. Each row's expected value is labelled `documented`
//! when Microsoft's INDIRECT page gives it, for it or for an analogue of one of the page's
//! examples (the label says which), and `measured` when it is Excel's value in the committed
//! capture of the oracle workbook
//! (`tests/captures/dc_51ka/oracle.capture.json`, written by `cargo xtask capture`), against which
//! `dc_51ka__excel_comparison` judges every row through calc_verdict::judge; exploratory probes in
//! Excel 16.0.20430, which are not committed, chose most rows before the capture. Rows calc_fml
//! does not model carry the refusal they expect and the bead that owns them, and are published as
//! not modelled under it.

use super::*;

/// The oracle workbook, as committed (its bytes are kept across checkouts by `.gitattributes`).
const WORKBOOK: &[u8] = include_bytes!("../fixtures/dc_51ka/oracle.xlsx");
const WORKBOOK_PATH: &str = "crates/calc-fml/tests/fixtures/dc_51ka/oracle.xlsx";
/// Excel's capture of the oracle workbook, as `CAPTURE_COMMAND` wrote it.
const CAPTURE: &[u8] = include_bytes!("../captures/dc_51ka/oracle.capture.json");
/// The case the capture is recorded under, and the command that writes it.
const CAPTURE_CASE: &str = "dc_51ka__indirect_text_references";
const CAPTURE_COMMAND: &str = "cargo xtask capture --fixture crates/calc-fml/tests/fixtures/dc_51ka/oracle.xlsx --case dc_51ka__indirect_text_references --out crates/calc-fml/tests/captures/dc_51ka/oracle.capture.json";
/// The sheet of the formulas, and their column (D), one per oracle row from row 1.
const SHEET: &str = "indirect";
const COLUMN: u32 = 4;
/// The authored revision the rows calculate: the oracle workbook as committed.
const REVISION: u64 = 0;

/// The oracle workbook's sheets, in its order: fourteen the rows name one by one, the 21 sheets
/// a<mark>b of [`MARKED`] and the seven of [`WIDE`].
const SHEETS: [&str; 42] = [
    "indirect", "data", "bench", "Q2 Data", "It's", "a-b", "1x", "A1", "a.b", ".x", "TRUE",
    "FALSE", "a_b", "\u{e9}", "a(b", "a)b", "a+b", "a&b", "a,b", "a%b", "a#b", "a$b", "a{b", "a~b",
    "a^b", "a=b", "a!b", "a;b", "a\"b", "a<b", "a>b", "a@b", "a`b", "a|b", "a}b", "\u{df}",
    "\u{d8}", "\u{f1}", "\u{3a9}", "\u{416}", "\u{4e2d}", "\u{20ac}",
];

/// The workbook-scoped defined names of the oracle workbook, in its order: name and formula.
const NAMES: [(&str, &str); 2] = [("Rate", "indirect!$A$2"), ("\\x", "indirect!$B$1")];

/// For each sheet a<mark>b, in the workbook's order: its name unquoted in INDIRECT's text, which
/// Excel gives #REF!, and quoted, which gives its A1 (3001, 3002, ... in this order). The marks are
/// the printable ASCII characters a sheet name may hold other than letters, digits, `_`, `.`, the
/// space, `'` and `-`, which other rows show.
const MARKED: [(&str, &str); 21] = [
    ("=INDIRECT(\"a(b!A1\")", "=INDIRECT(\"'a(b'!A1\")"),
    ("=INDIRECT(\"a)b!A1\")", "=INDIRECT(\"'a)b'!A1\")"),
    ("=INDIRECT(\"a+b!A1\")", "=INDIRECT(\"'a+b'!A1\")"),
    ("=INDIRECT(\"a&b!A1\")", "=INDIRECT(\"'a&b'!A1\")"),
    ("=INDIRECT(\"a,b!A1\")", "=INDIRECT(\"'a,b'!A1\")"),
    ("=INDIRECT(\"a%b!A1\")", "=INDIRECT(\"'a%b'!A1\")"),
    ("=INDIRECT(\"a#b!A1\")", "=INDIRECT(\"'a#b'!A1\")"),
    ("=INDIRECT(\"a$b!A1\")", "=INDIRECT(\"'a$b'!A1\")"),
    ("=INDIRECT(\"a{b!A1\")", "=INDIRECT(\"'a{b'!A1\")"),
    ("=INDIRECT(\"a~b!A1\")", "=INDIRECT(\"'a~b'!A1\")"),
    ("=INDIRECT(\"a^b!A1\")", "=INDIRECT(\"'a^b'!A1\")"),
    ("=INDIRECT(\"a=b!A1\")", "=INDIRECT(\"'a=b'!A1\")"),
    ("=INDIRECT(\"a!b!A1\")", "=INDIRECT(\"'a!b'!A1\")"),
    ("=INDIRECT(\"a;b!A1\")", "=INDIRECT(\"'a;b'!A1\")"),
    ("=INDIRECT(\"a\"\"b!A1\")", "=INDIRECT(\"'a\"\"b'!A1\")"),
    ("=INDIRECT(\"a<b!A1\")", "=INDIRECT(\"'a<b'!A1\")"),
    ("=INDIRECT(\"a>b!A1\")", "=INDIRECT(\"'a>b'!A1\")"),
    ("=INDIRECT(\"a@b!A1\")", "=INDIRECT(\"'a@b'!A1\")"),
    ("=INDIRECT(\"a`b!A1\")", "=INDIRECT(\"'a`b'!A1\")"),
    ("=INDIRECT(\"a|b!A1\")", "=INDIRECT(\"'a|b'!A1\")"),
    ("=INDIRECT(\"a}b!A1\")", "=INDIRECT(\"'a}b'!A1\")"),
];

/// Text naming no sheet the workbook can hold: an unquoted name with a character no sheet name may
/// hold.
const UNNAMEABLE: [&str; 4] = [
    "=INDIRECT(\"a*b!A1\")",
    "=INDIRECT(\"a/b!A1\")",
    "=INDIRECT(\"a?b!A1\")",
    "=INDIRECT(\"a\\b!A1\")",
];

/// The non-ASCII sheets, each named unquoted in INDIRECT's text, and what Excel gives: the six
/// letters' sheets their A1 (4001 to 4006), the euro sign's #REF!.
const WIDE: [(&str, Option<f64>); 7] = [
    ("=INDIRECT(\"\u{df}!A1\")", Some(4001.0)),
    ("=INDIRECT(\"\u{d8}!A1\")", Some(4002.0)),
    ("=INDIRECT(\"\u{f1}!A1\")", Some(4003.0)),
    ("=INDIRECT(\"\u{3a9}!A1\")", Some(4004.0)),
    ("=INDIRECT(\"\u{416}!A1\")", Some(4005.0)),
    ("=INDIRECT(\"\u{4e2d}!A1\")", Some(4006.0)),
    ("=INDIRECT(\"\u{20ac}!A1\")", None),
];

/// Tab, carriage return and no-break space after the text, before it, after the qualifier's `!`
/// and on either side of a range's `:`: Excel gives #REF! for each.
const BLANKS: [&str; 15] = [
    "=INDIRECT(\"A1\t\")",
    "=INDIRECT(\"\tA1\")",
    "=INDIRECT(\"data!\tA1\")",
    "=SUM(INDIRECT(\"A1\t:B2\"))",
    "=SUM(INDIRECT(\"A1:\tB2\"))",
    "=INDIRECT(\"A1\r\")",
    "=INDIRECT(\"\rA1\")",
    "=INDIRECT(\"data!\rA1\")",
    "=SUM(INDIRECT(\"A1\r:B2\"))",
    "=SUM(INDIRECT(\"A1:\rB2\"))",
    "=INDIRECT(\"A1\u{a0}\")",
    "=INDIRECT(\"\u{a0}A1\")",
    "=INDIRECT(\"data!\u{a0}A1\")",
    "=SUM(INDIRECT(\"A1\u{a0}:B2\"))",
    "=SUM(INDIRECT(\"A1:\u{a0}B2\"))",
];

/// The constant cells of each sheet, as the generator writes them: `(sheet, address, value)`.
fn constants() -> Vec<(&'static str, &'static str, CalcValue)> {
    let n = CalcValue::number;
    let t = text_value;
    let mut cells = vec![
        ("indirect", "A1", n(10.0)),
        ("indirect", "A2", n(20.0)),
        ("indirect", "B1", n(30.0)),
        ("indirect", "B2", n(40.0)),
        ("indirect", "A3", t("bench")),
        ("indirect", "A5", t("A1")),
        ("indirect", "A6", t("R1C1")),
        ("indirect", "A7", t("'Q2 Data'")),
        ("indirect", "B3", t("q")),
        ("indirect", "B4", t("y")),
        ("indirect", "B5", t("zz")),
        ("indirect", "A1048576", n(99.0)),
        ("indirect", "XFD1", n(98.0)),
        ("data", "A1", n(100.0)),
        ("data", "B1", t("x")),
        ("data", "C1", t("y")),
        ("data", "D1", t("z")),
        ("data", "A2", t("p")),
        ("data", "B2", n(1.0)),
        ("data", "C2", n(2.0)),
        ("data", "D2", n(3.0)),
        ("data", "A3", t("q")),
        ("data", "B3", n(4.0)),
        ("data", "C3", n(5.0)),
        ("data", "D3", n(6.0)),
        ("data", "A4", t("r")),
        ("data", "B4", n(7.0)),
        ("data", "C4", n(8.0)),
        ("data", "D4", n(9.0)),
    ];
    cells.extend([
        ("bench", "B1", t("x")),
        ("bench", "C1", t("y")),
        ("bench", "D1", t("z")),
        ("bench", "A2", t("Total_p")),
        ("bench", "B2", n(1.25)),
        ("bench", "C2", n(2.5)),
        ("bench", "D2", n(3.75)),
        ("bench", "A3", t("Total_q")),
        ("bench", "B3", n(4.125)),
        ("bench", "C3", n(12.3456)),
        ("bench", "D3", n(6.5)),
        ("bench", "A4", t("Total_r")),
        ("bench", "B4", n(7.777)),
        ("bench", "C4", n(8.25)),
        ("bench", "D4", n(9.5)),
        ("Q2 Data", "B2", n(5.0)),
        ("Q2 Data", "C3", n(333.0)),
        ("It's", "A1", n(444.0)),
        ("a-b", "A1", n(555.0)),
        ("1x", "A1", n(777.0)),
        ("A1", "B2", n(666.0)),
        ("a.b", "A1", n(888.0)),
        (".x", "A1", n(1020.0)),
        ("TRUE", "A1", n(999.0)),
        ("FALSE", "A1", n(1111.0)),
        ("a_b", "A1", n(2222.0)),
        ("\u{e9}", "A1", n(1003.0)),
    ]);
    // The sheets a<mark>b hold 3001, 3002, ... in A1, and the non-ASCII sheets 4001, 4002, ....
    for (sheet, value) in SHEETS[14..35].iter().zip(3001..) {
        cells.push((*sheet, "A1", n(f64::from(value))));
    }
    for (sheet, value) in SHEETS[35..].iter().zip(4001..) {
        cells.push((*sheet, "A1", n(f64::from(value))));
    }
    cells
}

/// Why a row's expected value is what it is.
#[derive(Clone, Copy, Debug)]
enum Basis {
    /// Microsoft's INDIRECT page gives the value: the statement named.
    Documented(&'static str),
    /// Excel's value, measured (see the module documentation).
    Measured,
    /// calc_fml does not model the row: why, and the bead that owns it.
    NotModelled {
        why: &'static str,
        owner: &'static str,
    },
}

const PAGE_A1: &str = "ref_text is read in A1 style when a1 is omitted";
const PAGE_R1C1: &str = "ref_text is read in R1C1 style when a1 is FALSE";
const PAGE_EXAMPLE_CELL: &str = "an analogue of the page's example =INDIRECT(A2), A2 holding the text B2, which gives B2's value: here A5 holds the text A1";
const PAGE_EXAMPLE_JOIN: &str = "an analogue of the page's example =INDIRECT(\"B\"&A5), which joins B to the number in A5 to name a cell of column B: here the number is the literal 1";
const PAGE_LIMITS: &str =
    "a reference outside the row limit 1,048,576 or the column limit XFD gives #REF!";
const PAGE_INVALID: &str = "text that is not a valid cell reference gives #REF!";
const NAME: &str = "the text could be a defined name, which INDIRECT resolves when the workbook defines it; names are not bound";
const DEFINED: &str = "a defined name of the workbook, which Excel resolves; names are not bound";

/// An oracle row: its formula as entered at its row of column D, what calc_fml::evaluate must
/// give, and the basis.
struct Row {
    formula: &'static str,
    expected: Result<CalcValue, EvalFault>,
    basis: Basis,
}

/// INDIRECT's catalogue entry.
fn indirect_target() -> calc_func::FunctionCallTarget<'static> {
    catalogue()
        .resolve_surface(Notation::Call, "INDIRECT")
        .expect("INDIRECT is catalogued")
}

/// INDIRECT's refusal as not modelled, for argument `argument`.
fn refused(argument: usize) -> EvalFault {
    EvalFault::Function(FunctionFault::NotImplemented {
        id: indirect_target().id(),
        argument: Some(argument),
    })
}

fn rows(_book: &Book) -> Vec<Row> {
    use self::Basis::{Documented as D, Measured as M};
    use WorksheetErrorCode as W;
    let n = |x: f64| Ok(CalcValue::number(x));
    let e = |code: W| Ok(CalcValue::error(code));
    let row = |formula, expected, basis| Row {
        formula,
        expected,
        basis,
    };
    let not_modelled = |formula, fault, why, owner| Row {
        formula,
        expected: Err(fault),
        basis: Basis::NotModelled { why, owner },
    };
    let mut rows = vec![
        // R1C1 offsets from the formula's cell, wrapping around the grid's edge (rows 1 to 6): up
        // from row 1, left from column D, down from row 5 by 1048575 rows to row 4 (B4 holds the
        // text y), and a range from row 6 whose first corner wraps to the last row while its
        // second does not (A2:B1048576: 20 + 40 + 99).
        row("=INDIRECT(\"R[-1]C1\",FALSE)", n(99.0), M),
        row("=INDIRECT(\"R[-1]C[-3]\",FALSE)", n(10.0), M),
        row("=INDIRECT(\"R1C[-4]\",FALSE)", n(98.0), M),
        row("=INDIRECT(\"R[1048576]C1\",FALSE)", e(W::Ref), M),
        row(
            "=INDIRECT(\"R[1048575]C[-2]\",FALSE)",
            Ok(text_value("y")),
            M,
        ),
        row("=SUM(INDIRECT(\"R[-6]C1:R[-4]C2\",FALSE))", n(159.0), M),
        // A1 cells of the formula's own sheet.
        row("=INDIRECT(\"A1\")", n(10.0), D(PAGE_A1)),
        row("=INDIRECT(\"$A$1\")", n(10.0), M),
        row("=INDIRECT(\"b2\")", n(40.0), M),
        row("=INDIRECT(\"A$2\")", n(20.0), M),
        row("=INDIRECT(A5)", n(10.0), D(PAGE_EXAMPLE_CELL)),
        row("=INDIRECT(\"B\"&1)", n(30.0), D(PAGE_EXAMPLE_JOIN)),
        row("=INDIRECT(\"A01\")", n(10.0), M),
        row("=INDIRECT(\"A1   \")", n(10.0), M),
        row("=INDIRECT(\"XFD1048576\")", n(0.0), M),
        row("=INDIRECT(\"$A$1048577\")", e(W::Ref), D(PAGE_LIMITS)),
        row("=INDIRECT(\"$XFE$1\")", e(W::Ref), D(PAGE_LIMITS)),
        // Sheet qualifiers, quoted and not.
        row("=INDIRECT(\"data!A1\")", n(100.0), M),
        row("=INDIRECT(\"DATA!a1\")", n(100.0), M),
        row("=INDIRECT(\"'data'!A1\")", n(100.0), M),
        row("=INDIRECT(\"data! A1\")", n(100.0), M),
        row("=INDIRECT(\"'data'! A1\")", n(100.0), M),
        row("=INDIRECT(\"data !A1\")", e(W::Ref), M),
        row("=INDIRECT(\"'data' !A1\")", e(W::Ref), M),
        row("=INDIRECT(\"'Q2 Data'!C3\")", n(333.0), M),
        row("=INDIRECT(\"Q2 Data!C3\")", e(W::Ref), M),
        row("=INDIRECT(\"'It''s'!A1\")", n(444.0), M),
        row("=INDIRECT(\"It's!A1\")", e(W::Ref), M),
        row("=INDIRECT(\"a-b!A1\")", e(W::Ref), M),
        row("=INDIRECT(\"'a-b'!A1\")", n(555.0), M),
        row("=INDIRECT(\"1x!A1\")", n(777.0), M),
        row("=INDIRECT(\"A1!B2\")", n(666.0), M),
        row("=INDIRECT(\"a.b!A1\")", n(888.0), M),
        row("=INDIRECT(\".x!A1\")", e(W::Ref), M),
        row("=INDIRECT(\"'.x'!A1\")", n(1020.0), M),
        row("=INDIRECT(\"TRUE!A1\")", e(W::Ref), M),
        row("=INDIRECT(\"true!A1\")", e(W::Ref), M),
        row("=INDIRECT(\"'TRUE'!A1\")", n(999.0), M),
        row("=INDIRECT(\"FALSE!A1\")", e(W::Ref), M),
        row("=INDIRECT(\"a_b!A1\")", n(2222.0), M),
        row("=INDIRECT(\"\u{e9}!A1\")", n(1003.0), M),
        row("=INDIRECT(\"nosuch!A1\")", e(W::Ref), M),
        row("=INDIRECT(A3&\"!B2\")", n(1.25), M),
        // Ranges, whole columns and rows.
        row("=SUM(INDIRECT(\"A1:B2\"))", n(100.0), M),
        row("=SUM(INDIRECT(\"B2:A1\"))", n(100.0), M),
        row("=SUM(INDIRECT(\"$A1 : B$2\"))", n(100.0), M),
        row("=SUM(INDIRECT(\"data!B2:B4\"))", n(12.0), M),
        row("=SUM(INDIRECT(\"'Q2 Data'!A1:C3\"))", n(338.0), M),
        row("=SUM(INDIRECT(\"data!B2:data!B4\"))", e(W::Ref), M),
        row("=SUM(INDIRECT(\"A1:data!B2\"))", e(W::Ref), M),
        row("=SUM(INDIRECT(\"A1:B2:C3\"))", e(W::Ref), M),
        row("=SUM(INDIRECT(\"A1 B1\"))", e(W::Ref), M),
        row("=SUM(INDIRECT(\"A:A\"))", n(129.0), M),
        row("=SUM(INDIRECT(\"$A:$B\"))", n(199.0), M),
        row("=SUM(INDIRECT(\"data!B:B\"))", n(12.0), M),
        row("=SUM(INDIRECT(\"data!2:2\"))", n(6.0), M),
        row("=SUM(INDIRECT(\"data!3:2\"))", n(21.0), M),
        row("=ROWS(INDIRECT(\"A1:B3\"))", n(3.0), M),
        row("=COLUMNS(INDIRECT(\"data!B1:D1\"))", n(3.0), M),
        // R1C1 text.
        row("=INDIRECT(\"R1C1\",FALSE)", n(10.0), D(PAGE_R1C1)),
        row("=INDIRECT(\"r2c2\",FALSE)", n(40.0), M),
        row("=INDIRECT(\"data!R1C1\",FALSE)", n(100.0), M),
        row("=SUM(INDIRECT(\"R1C1:R2C2\",FALSE))", n(100.0), M),
        row("=SUM(INDIRECT(\"data!R2C2:R4C2\",FALSE))", n(12.0), M),
        row("=SUM(INDIRECT(\"data!R2\",FALSE))", n(6.0), M),
        row("=SUM(INDIRECT(\"data!C2\",FALSE))", n(12.0), M),
        row("=INDIRECT(\"A1\",FALSE)", e(W::Ref), D(PAGE_INVALID)),
        row("=INDIRECT(\"R1C1\")", e(W::Ref), D(PAGE_INVALID)),
        row("=INDIRECT(\"R0C1\",FALSE)", e(W::Ref), D(PAGE_INVALID)),
        row("=INDIRECT(\"R1C1:R2\",FALSE)", e(W::Ref), M),
        // Text that names no reference.
        row("=INDIRECT(\"\")", e(W::Ref), M),
        row("=INDIRECT(\" A1\")", e(W::Ref), M),
        row("=INDIRECT(\"=A1\")", e(W::Ref), M),
        row("=INDIRECT(\"(A1)\")", e(W::Ref), M),
        row("=INDIRECT(\"A1+1\")", e(W::Ref), M),
        row("=INDIRECT(\"indirect:data!A1\")", e(W::Ref), M),
        row("=INDIRECT(\"data!#REF!\")", e(W::Ref), M),
        row("=INDIRECT(\"$A\")", e(W::Ref), M),
        row("=INDIRECT(\"1\")", e(W::Ref), M),
        // ref_text and a1 as values.
        row("=INDIRECT(1)", e(W::Ref), M),
        row("=INDIRECT(TRUE)", e(W::Ref), M),
        row("=INDIRECT(#N/A)", e(W::NA), M),
        row("=INDIRECT(A4)", e(W::Ref), M),
        row("=INDIRECT(\"A1\",1)", n(10.0), M),
        row("=INDIRECT(\"A1\",0)", e(W::Ref), M),
        row("=INDIRECT(\"A1\",2)", n(10.0), M),
        row("=INDIRECT(\"A1\",)", e(W::Ref), M),
        row("=INDIRECT(\"R1C1\",)", n(10.0), M),
        row("=INDIRECT(\"R1C1\",\"FALSE\")", n(10.0), M),
        row("=INDIRECT(\"A1\",\"true\")", n(10.0), M),
        row("=INDIRECT(\"A1\",\"x\")", e(W::Value), M),
        row("=INDIRECT(\"R1C1\",A4)", n(10.0), M),
        row("=INDIRECT(\"A1\",#N/A)", e(W::NA), M),
        row("=INDIRECT(#DIV/0!,#N/A)", e(W::Div0), M),
        row("=INDIRECT(1,#N/A)", e(W::NA), M),
        row("=INDIRECT(A6,FALSE)", n(10.0), M),
        // INDIRECT nested in INDEX and MATCH as the Ofgem price-cap workbook nests it, and in
        // other calls.
        row(OFGEM_SHAPE, n(12.3456), M),
        row(OFGEM_ROUNDED, n(12.35), M),
        row(
            "=IFERROR(ROUND(INDEX(INDIRECT(A3&\"!$B$2:$D$4\"),MATCH(\"Total_\"&B5,INDIRECT(A3&\"!$A$2:$A$4\"),0),MATCH(B4,INDIRECT(A3&\"!$B$1:$D$1\"),0)),2),\"-\")",
            Ok(text_value("-")),
            M,
        ),
        row("=INDEX(INDIRECT(A7&\"!A1:C3\"),2,2)", n(5.0), M),
        row("=MATCH(7.777,INDIRECT(\"bench!B2:B4\"),0)", n(3.0), M),
        row("=SUM(INDIRECT(\"A1\"),INDIRECT(\"B2\"))", n(50.0), M),
        row("=INDIRECT(\"A1\")+INDIRECT(\"data!A1\")", n(110.0), M),
        row("=INDIRECT(INDIRECT(\"A5\"))", n(10.0), M),
    ];
    // Each mark in an unquoted sheet name, then the same sheet's name quoted; then four characters
    // no sheet name may hold in an unquoted one.
    for (&(unquoted, quoted), value) in MARKED.iter().zip(3001..) {
        rows.push(row(unquoted, e(W::Ref), M));
        rows.push(row(quoted, n(f64::from(value)), M));
    }
    for formula in UNNAMEABLE {
        rows.push(row(formula, e(W::Ref), M));
    }
    // Unquoted non-ASCII sheet names, and the euro sign's quoted.
    for (formula, value) in WIDE {
        rows.push(row(formula, value.map_or(e(W::Ref), n), M));
    }
    rows.push(row("=INDIRECT(\"'\u{20ac}'!A1\")", n(4007.0), M));
    rows.push(not_modelled(
        "=INDIRECT(\"\\x\")",
        refused(0),
        DEFINED,
        "dc-10y",
    ));
    // Tab, carriage return and no-break space before, inside and after the text.
    for formula in BLANKS {
        rows.push(row(formula, e(W::Ref), M));
    }
    rows.extend([
        // Rows calc_fml does not model.
        not_modelled("=INDIRECT(\"NoSuchName\")", refused(0), NAME, "dc-10y"),
        not_modelled("=INDIRECT(\"data!Foo\")", refused(0), NAME, "dc-10y"),
        not_modelled("=INDIRECT(\"XFE1\")", refused(0), NAME, "dc-10y"),
        not_modelled("=INDIRECT(\"Rate\")", refused(0), DEFINED, "dc-10y"),
        // A qualifier naming another workbook, closed (dc-ylr): #REF!, as Excel gives it.
        row("=INDIRECT(\"[Book1]data!A1\")", e(W::Ref), M),
        // The legacy formula intersects the range with its own row, which A1:B2 does not reach
        // (Excel writes it `=@INDIRECT("A1:B2")`; dc-mzk).
        row("=INDIRECT(\"A1:B2\")", e(W::Value), M),
        // a1 as digit text is no logical: #VALUE! (dc-ip6).
        row("=INDIRECT(\"A1\",\"1\")", e(W::Value), M),
        not_modelled(
            "=SUM(INDIRECT(\"A1:B\"))",
            refused(0),
            "a range whose end could be a defined name (B), which is not bound",
            "dc-10y",
        ),
        // A line feed after the text reads as a space: A1 = 10 (dc-0k9).
        row("=INDIRECT(\"A1\n\")", n(10.0), M),
    ]);
    rows
}

/// The shape of the 252 INDIRECT cells of the Ofgem workbook Default-tariff-cap-level (bead
/// dc-3t1b), without its ROUND and IFERROR.
const OFGEM_SHAPE: &str = "=INDEX(INDIRECT(A3&\"!$B$2:$D$4\"),MATCH(\"Total_q\",INDIRECT(A3&\"!$A$2:$A$4\"),0),MATCH(\"y\",INDIRECT(A3&\"!$B$1:$D$1\"),0))";
/// The same, as the Ofgem cells write it.
const OFGEM_ROUNDED: &str = "=IFERROR(ROUND(INDEX(INDIRECT(A3&\"!$B$2:$D$4\"),MATCH(\"Total_\"&B3,INDIRECT(A3&\"!$A$2:$A$4\"),0),MATCH(B4,INDIRECT(A3&\"!$B$1:$D$1\"),0)),2),\"-\")";

/// What an observer is shown of one evaluation: each call, the areas INDIRECT resolved, in the
/// order shown, and the report's cell and outcome.
#[derive(Default)]
struct Watch {
    calls: Vec<String>,
    resolved: Vec<GridArea>,
    /// The calls that resolved each area, by spelling.
    resolvers: Vec<String>,
    reported: Vec<(CellKey, Result<CalcValue, EvalFault>)>,
}

impl EvaluationObserver for Watch {
    fn called(&mut self, call: ExecutedCall) {
        let target = call.target();
        self.calls.push(format!(
            "{:?} {}{}",
            target.notation(),
            target.spelling(),
            if call.synthetic() { " *" } else { "" }
        ));
    }

    fn resolved(&mut self, reference: calc_fml::ResolvedReference) {
        self.resolved.push(reference.area());
        self.resolvers
            .push(reference.target().spelling().to_owned());
    }

    fn returned(&mut self, report: EvaluationReport<'_>) {
        self.reported.push((
            report.anchor().cell(),
            report.outcome().cloned().map_err(Clone::clone),
        ));
    }
}

/// One evaluation as the case sees it: its outcome, the areas the reader was asked for, the
/// areas the plan places and what the observer was shown.
struct Shown {
    outcome: Result<CalcValue, EvalFault>,
    reads: Vec<GridArea>,
    placed: Vec<GridArea>,
    watch: Watch,
}

/// The oracle workbook as the case holds it: the catalogue of its sheets, a reader of its
/// constant cells, and the state rows are compiled and evaluated in.
struct Book {
    catalog: LoggingCatalog,
    reader: RunReader,
    cache: PlanCache,
    scratch: EvalScratch,
}

impl Book {
    fn new() -> Self {
        let catalog = LoggingCatalog::new(&SHEETS);
        let mut reader = RunReader::default();
        for (sheet, address, value) in constants() {
            reader.store(catalog.id(sheet), address, value);
        }
        Book {
            catalog,
            reader,
            cache: PlanCache::new(),
            scratch: EvalScratch::new(),
        }
    }

    fn anchor(&self, row: u32) -> FormulaAnchor {
        FormulaAnchor::new(CellKey::new(self.catalog.id(SHEET), row, COLUMN).expect("a cell"))
    }

    fn area(&self, sheet: &str, first: &str, last: &str) -> GridArea {
        area_of(self.catalog.id(sheet), first, last)
    }

    fn compile(&mut self, formula: &str, anchor: FormulaAnchor) -> Arc<CompiledPlan> {
        compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &self.catalog,
            &mut self.cache,
        )
        .unwrap_or_else(|refused| panic!("{formula}: {refused:?}"))
    }

    /// One evaluation of `instance` under Fast, observed, with the catalogue given unless
    /// `without_catalog`.
    fn run(&mut self, instance: &FormulaInstance, without_catalog: bool) -> Shown {
        self.reader.requests.clear();
        self.reader.value_runs.clear();
        let mode = NumericMode::Fast;
        let mut watch = Watch::default();
        let outcome = {
            let services = EvaluationServices::new(&mode, &mut self.reader);
            let services = if without_catalog {
                services
            } else {
                services.with_catalog(&self.catalog)
            };
            evaluate(
                EvalFrame::new(instance, &mut self.scratch),
                &mut services.observed_by(&mut watch),
            )
        };
        Shown {
            outcome,
            reads: std::mem::take(&mut self.reader.requests),
            placed: instance.areas().iter().filter_map(|a| a.ok()).collect(),
            watch,
        }
    }

    fn evaluate(&mut self, formula: &str, row: u32) -> Shown {
        let anchor = self.anchor(row);
        let plan = self.compile(formula, anchor);
        self.run(&instantiate(&plan, anchor), false)
    }
}

/// Whether `inner` lies inside `outer`.
fn within(inner: GridArea, outer: GridArea) -> bool {
    outer.contains(inner.first()) && outer.contains(inner.last())
}

/// Every row evaluated at its cell, in order from row 1.
fn run_rows(book: &mut Book) -> Vec<(u32, Row, Shown)> {
    rows(book)
        .into_iter()
        .zip(1u32..)
        .map(|(row, at)| {
            let shown = book.evaluate(row.formula, at);
            (at, row, shown)
        })
        .collect()
}

/// `CASE`'s first part: every row, its value or refusal, its basis, its observer's account, and
/// its reads inside the areas its plan places or INDIRECT resolved.
pub(super) fn indirect_text_references() {
    const CASE: &str = "dc_51ka__indirect_text_references";
    let mut book = Book::new();
    let run = run_rows(&mut book);
    let indirect = indirect_target();
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut refusals = Vec::new();
    for (at, row, shown) in &run {
        let matches = match (&shown.outcome, &row.expected) {
            (Ok(value), Ok(expected)) => same_value(value, expected),
            (outcome, expected) => outcome == expected,
        };
        assert!(
            matches,
            "D{at} {}: calc_fml::evaluate gave {:?}, expected {:?} ({:?})",
            row.formula, shown.outcome, row.expected, row.basis
        );
        assert_eq!(
            shown.watch.reported,
            [(book.anchor(*at).cell(), shown.outcome.clone())],
            "{}: the observer is shown the outcome evaluate returned",
            row.formula
        );
        // Every area shown resolved was resolved by INDIRECT, never more areas than INDIRECT calls
        // ran; every cell the evaluation read lies in an area the plan places or INDIRECT
        // resolved, so the resolved areas and the plan's references cover its reads.
        assert!(
            shown
                .watch
                .resolvers
                .iter()
                .all(|spelling| spelling == "INDIRECT"),
            "{}",
            row.formula
        );
        assert!(
            shown.watch.resolved.len()
                <= shown
                    .watch
                    .calls
                    .iter()
                    .filter(|call| *call == "Call INDIRECT")
                    .count(),
            "{}: more areas resolved than INDIRECT calls run",
            row.formula
        );
        for read in &shown.reads {
            assert!(
                shown.placed.iter().any(|area| within(*read, *area))
                    || shown.watch.resolved.iter().any(|area| within(*read, *area)),
                "{}: read {read:?} outside its plan's areas {:?} and the areas resolved {:?}",
                row.formula,
                shown.placed,
                shown.watch.resolved
            );
        }
        let label = match row.basis {
            Basis::Documented(_) => "documented",
            Basis::Measured => "measured",
            Basis::NotModelled { why, owner } => {
                refusals.push(format!(
                    "D{at} {} refused ({:?}): {why}, bead {owner}",
                    one_line(row.formula),
                    shown.outcome
                ));
                "not modelled"
            }
        };
        *counts.entry(label).or_default() += 1;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} oracle rows evaluated through parse_formula, compile, instantiate and evaluate with the catalogue of the workbook's {} sheets: {} documented (Microsoft's INDIRECT page), {} measured (Excel's value, judged against the committed capture in dc_51ka__excel_comparison), {} not modelled; every row's reads lie inside the areas its plan places or INDIRECT resolved",
        run.len(),
        SHEETS.len(),
        counts.get("documented").copied().unwrap_or_default(),
        counts.get("measured").copied().unwrap_or_default(),
        counts.get("not modelled").copied().unwrap_or_default()
    );
    for (_, row, _) in run
        .iter()
        .filter(|(_, row, _)| matches!(row.basis, Basis::Documented(_)))
    {
        if let Basis::Documented(statement) = row.basis {
            note!(
                "ACCEPT-NOTE: {CASE}: documented: {} ({statement})",
                row.formula
            );
        }
    }
    for refusal in &refusals {
        note!("ACCEPT-NOTE: {CASE}: not modelled: {refusal}");
    }

    // The read footprint: the areas INDIRECT resolved, as the observer is shown them, decoded here
    // from A1 addresses without calc-fml. The plan holds none of them: only the references the
    // formula writes.
    let by_formula = |formula: &str| -> &Shown {
        &run.iter()
            .find(|(_, row, _)| row.formula == formula)
            .unwrap_or_else(|| panic!("{formula} is no oracle row"))
            .2
    };
    let b = &book;
    let footprints: [(&str, Vec<GridArea>); 17] = [
        (
            "=INDIRECT(\"R[-1]C1\",FALSE)",
            vec![b.area(SHEET, "A1048576", "A1048576")],
        ),
        (
            "=INDIRECT(\"R1C[-4]\",FALSE)",
            vec![b.area(SHEET, "XFD1", "XFD1")],
        ),
        ("=INDIRECT(\"R[1048576]C1\",FALSE)", vec![]),
        (
            "=INDIRECT(\"R[1048575]C[-2]\",FALSE)",
            vec![b.area(SHEET, "B4", "B4")],
        ),
        (
            "=SUM(INDIRECT(\"R[-6]C1:R[-4]C2\",FALSE))",
            vec![b.area(SHEET, "A2", &format!("B{MAX_ROWS}"))],
        ),
        (
            "=INDIRECT(\"'\u{20ac}'!A1\")",
            vec![b.area("\u{20ac}", "A1", "A1")],
        ),
        ("=INDIRECT(\"\\x\")", vec![]),
        ("=INDIRECT(\"A1\n\")", vec![b.area(SHEET, "A1", "A1")]),
        ("=INDIRECT(\"data!A1\")", vec![b.area("data", "A1", "A1")]),
        (
            "=INDIRECT(\"'Q2 Data'!C3\")",
            vec![b.area("Q2 Data", "C3", "C3")],
        ),
        ("=INDIRECT(\"nosuch!A1\")", vec![]),
        (
            "=SUM(INDIRECT(\"A:A\"))",
            vec![b.area(SHEET, "A1", &format!("A{MAX_ROWS}"))],
        ),
        (
            "=SUM(INDIRECT(\"data!3:2\"))",
            vec![b.area("data", "A2", "XFD3")],
        ),
        (
            "=SUM(INDIRECT(\"data!C2\",FALSE))",
            vec![b.area("data", "B1", &format!("B{MAX_ROWS}"))],
        ),
        (
            OFGEM_SHAPE,
            vec![
                b.area("bench", "B2", "D4"),
                b.area("bench", "A2", "A4"),
                b.area("bench", "B1", "D1"),
            ],
        ),
        (
            "=INDIRECT(INDIRECT(\"A5\"))",
            vec![b.area(SHEET, "A5", "A5"), b.area(SHEET, "A1", "A1")],
        ),
        ("=INDIRECT(\"NoSuchName\")", vec![]),
    ];
    for (formula, expected) in &footprints {
        let shown = by_formula(formula);
        assert_eq!(&shown.watch.resolved, expected, "{formula}: areas resolved");
        for area in expected {
            assert!(
                !shown.placed.contains(area),
                "{formula}: the plan does not hold {area:?}"
            );
        }
    }
    let ofgem = by_formula(OFGEM_SHAPE);
    assert_eq!(
        ofgem.watch.calls,
        [
            "Infix &",
            "Call INDIRECT",
            "Infix &",
            "Call INDIRECT",
            "Call MATCH",
            "Infix &",
            "Call INDIRECT",
            "Call MATCH",
            "Call INDEX"
        ],
        "{OFGEM_SHAPE}: calls run"
    );
    assert_eq!(
        ofgem.placed,
        vec![b.area(SHEET, "A3", "A3"); 3],
        "{OFGEM_SHAPE}: the plan places A3 three times and nothing on bench"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the observer is shown each area INDIRECT resolved, as INDIRECT returns: {} rows checked area by area against A1 addresses decoded by the case (the Ofgem shape {OFGEM_SHAPE} resolves bench!B2:D4, bench!A2:A4 and bench!B1:D1, while its plan places only indirect!A3, three times); text that names no reference and a refused text resolve nothing",
        footprints.len()
    );

    // One plan, two texts: the footprint follows the text, and the value with it.
    let switched = "=INDIRECT(A3&\"!B2\")";
    let anchor = book.anchor(200);
    let plan = book.compile(switched, anchor);
    let instance = instantiate(&plan, anchor);
    let before = book.run(&instance, false);
    let sheet = book.catalog.id(SHEET);
    book.reader.store(sheet, "A3", text_value("data"));
    let after = book.run(&instance, false);
    book.reader.store(sheet, "A3", text_value("bench"));
    assert_eq!(
        (before.outcome, before.watch.resolved),
        (
            Ok(CalcValue::number(1.25)),
            vec![book.area("bench", "B2", "B2")]
        )
    );
    assert_eq!(
        (after.outcome, after.watch.resolved),
        (
            Ok(CalcValue::number(1.0)),
            vec![book.area("data", "B2", "B2")]
        )
    );
    note!(
        "ACCEPT-NOTE: {CASE}: one plan of {switched}, not an oracle row: with A3 holding bench it gives 1.25 and resolves bench!B2, with A3 holding data it gives 1 and resolves data!B2 (hand-derived from the reader's cells)"
    );

    // Without a catalogue INDIRECT is refused as not modelled after its call runs, and resolves
    // nothing; the services' other routes are unchanged.
    let anchor = book.anchor(201);
    let plan = book.compile("=INDIRECT(\"A1\")+1", anchor);
    let bare = book.run(&instantiate(&plan, anchor), true);
    assert_eq!(bare.outcome, Err(refused(0)));
    assert_eq!(bare.watch.calls, ["Call INDIRECT"]);
    assert!(bare.watch.resolved.is_empty());
    assert_eq!(
        indirect.parameter_class(0),
        Some(calc_func::ParameterClass::Value)
    );
    note!(
        "ACCEPT-NOTE: {CASE}: without EvaluationServices::with_catalog, =INDIRECT(\"A1\")+1 is refused (NotImplemented of INDIRECT, argument 0) once INDIRECT's call runs, and nothing is resolved"
    );

    // The oracle workbook holds exactly the rows' formulas in column D, in row order, and the
    // constant cells the reader holds, on its 42 sheets in order, and its two defined names.
    let sheets = workbook_sheets(WORKBOOK);
    assert_eq!(
        sheets
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        SHEETS,
        "the oracle workbook's sheets"
    );
    let mut expected: BTreeMap<(String, String), SheetCell> = run
        .iter()
        .map(|(at, row, _)| {
            (
                (SHEET.to_owned(), format!("D{at}")),
                SheetCell::Formula(row.formula[1..].to_owned()),
            )
        })
        .collect();
    for (sheet, address, value) in constants() {
        expected.insert(
            (sheet.to_owned(), address.to_owned()),
            SheetCell::Constant(value),
        );
    }
    let actual: BTreeMap<(String, String), SheetCell> = sheets
        .into_iter()
        .flat_map(|(sheet, cells)| {
            cells
                .into_iter()
                .map(move |(address, cell)| ((sheet.clone(), address), cell))
        })
        .collect();
    let differences: Vec<String> = expected
        .keys()
        .chain(actual.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|key| match (expected.get(*key), actual.get(*key)) {
            (Some(SheetCell::Formula(a)), Some(SheetCell::Formula(b))) => a != b,
            (Some(SheetCell::Constant(a)), Some(SheetCell::Constant(b))) => !same_value(a, b),
            _ => true,
        })
        .map(|key| {
            format!(
                "{}!{}: the case has {:?}, the workbook {:?}",
                key.0,
                key.1,
                expected.get(key),
                actual.get(key)
            )
        })
        .collect();
    assert!(
        differences.is_empty(),
        "the oracle workbook differs from the case:\n{}",
        differences.join("\n")
    );
    let names = workbook_names(WORKBOOK);
    assert_eq!(
        names
            .iter()
            .map(|(name, formula)| (name.as_str(), formula.as_str()))
            .collect::<Vec<_>>(),
        NAMES,
        "the oracle workbook's defined names"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {WORKBOOK_PATH} (sha256 {}) holds exactly the {} rows' formulas in column D of the sheet {SHEET} in row order and the {} constant cells the reader holds, on its {} sheets, and the defined names {:?}",
        sha256_hex(WORKBOOK),
        run.len(),
        constants().len(),
        SHEETS.len(),
        NAMES
    );
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_workbooks(CASE);
        assert!(
            generated.get("dc_51ka/oracle.xlsx").map(Vec::as_slice) == Some(WORKBOOK),
            "the committed generator writes dc_51ka/oracle.xlsx as committed"
        );
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        for (name, bytes) in &generated {
            let committed = std::fs::read(fixtures.join(name))
                .unwrap_or_else(|e| panic!("the committed fixture {name}: {e}"));
            assert!(
                &committed == bytes,
                "the committed generator writes {name} (sha256 {}), which differs from the committed one (sha256 {})",
                sha256_hex(bytes),
                sha256_hex(&committed)
            );
        }
        note!(
            "ACCEPT-NOTE: {CASE}: the committed generator tests/fixtures/make-fixtures.ps1, run into a new directory under the target directory's temporary directory, wrote {} workbooks ({}), each equal to the committed fixture at its path byte for byte, this case's among them",
            generated.len(),
            generated.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    note!(
        "ACCEPT-NOTE: {CASE}: the check that the committed generator tests/fixtures/make-fixtures.ps1 writes the committed workbook byte for byte did not run on this target: the generator is a Windows PowerShell script, which the case runs only in its native Windows build"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: what this case does not show: calc-engine and DNA OneCalc do not give calc_fml::evaluate a catalogue yet, so INDIRECT through them is refused as not modelled (dc-m3u); the engine is not shown the areas INDIRECT resolves and does not re-dirty a formula when a cell inside one changes (dc-m3u for INDIRECT and for dynamic reads in general); text naming a sheet the workbook lacks gives #REF! and resolves no area, so nothing records the dependency and creating that sheet later re-dirties nothing (dc-m3u); the catalogue declares no volatility, so INDIRECT is not marked volatile (cfunc-m4.1, whose volatility declaration owns it); OFFSET and the reference operators (dc-mzk) are covered by its own tests, not here"
    );
}

/// The workbook-scoped defined names of the oracle workbook, in its order: name and formula.
fn workbook_names(package: &[u8]) -> Vec<(String, String)> {
    let parts = stored_parts(package);
    let workbook = part_text(&parts, "xl/workbook.xml");
    let mut names = Vec::new();
    let mut rest = workbook;
    while let Some(at) = rest.find("<definedName ") {
        rest = &rest[at..];
        let tag_end = rest.find('>').expect("a defined name's start tag ends");
        let close = rest.find("</definedName>").expect("a defined name ends");
        let name = unescape(attribute(&rest[..tag_end], "name").expect("a name"));
        names.push((name, unescape(&rest[tag_end + 1..close])));
        rest = &rest[close..];
    }
    names
}

/// `formula` on one line, for a note: a tab, carriage return or line feed written as `\t`, `\r`
/// or `\n`.
fn one_line(formula: &str) -> String {
    formula
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

/// `formula` as Excel reports it, without the implicit-intersection operators Excel adds before
/// calls (an `@` outside the formula's text literals), and whether it held one.
fn without_implicit_intersection(formula: &str) -> (String, bool) {
    let mut plain = String::with_capacity(formula.len());
    let mut in_text = false;
    let mut removed = false;
    for c in formula.chars() {
        if c == '"' {
            in_text = !in_text;
        }
        if c == '@' && !in_text {
            removed = true;
            continue;
        }
        plain.push(c);
    }
    (plain, removed)
}

/// A cell of the oracle workbook: a formula, in its file form, or a constant.
#[derive(Debug)]
enum SheetCell {
    Formula(String),
    Constant(CalcValue),
}

/// The sheets of the oracle workbook in order, each with its cells by address, as the generator
/// writes them: `<c r="...">` holding an `<f>`, a `<v>` or an inline string.
fn workbook_sheets(package: &[u8]) -> Vec<(String, BTreeMap<String, SheetCell>)> {
    let parts = stored_parts(package);
    let workbook = part_text(&parts, "xl/workbook.xml");
    assert!(
        workbook.contains("<calcPr fullCalcOnLoad=\"1\"/>") && !workbook.contains("<workbookPr"),
        "the oracle workbook declares no calculation setting: {workbook}"
    );
    let mut sheets = Vec::new();
    let mut rest = workbook;
    while let Some(at) = rest.find("<sheet ") {
        rest = &rest[at..];
        let end = rest.find("/>").expect("a sheet entry ends");
        let tag = &rest[..end];
        rest = &rest[end..];
        let name = unescape(attribute(tag, "name").expect("a sheet name"));
        let index = sheets.len() + 1;
        assert_eq!(
            attribute(tag, "r:id"),
            Some(format!("rId{index}").as_str()),
            "{name}"
        );
        let xml = part_text(&parts, &format!("xl/worksheets/sheet{index}.xml"));
        sheets.push((name, sheet_cells(xml)));
    }
    let relationships = part_text(&parts, "xl/_rels/workbook.xml.rels");
    for index in 1..=sheets.len() {
        assert!(relationships.contains(&format!(
            "Id=\"rId{index}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{index}.xml\""
        )));
    }
    sheets
}

fn sheet_cells(xml: &str) -> BTreeMap<String, SheetCell> {
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
            // The generator writes a tab, carriage return and line feed of a formula as character
            // references; each is decoded before the entities, so `&amp;#9;` stays text.
            let decoded = body[start..end]
                .replace("&#9;", "\t")
                .replace("&#13;", "\r")
                .replace("&#10;", "\n");
            Some(unescape(&decoded))
        };
        let address = attribute(tag, "r").expect("an address").to_owned();
        let cell = match (inner("f"), attribute(tag, "t")) {
            (Some(formula), None) => SheetCell::Formula(formula),
            (None, None) => SheetCell::Constant(CalcValue::number(
                inner("v").expect("a value").parse().expect("a number"),
            )),
            (None, Some("inlineStr")) => {
                SheetCell::Constant(text_value(&inner("t").expect("a text")))
            }
            (formula, kind) => panic!("{address}: {formula:?} of type {kind:?}"),
        };
        assert!(
            cells.insert(address.clone(), cell).is_none(),
            "{address} repeats"
        );
    }
    cells
}

fn address(row: u32) -> CellAddress {
    CellAddress::new(SHEET, row, COLUMN).expect("a cell of the grid")
}

/// What the run publishes, judged by calc_verdict: a result for each row's cell, calculated at
/// its tick when calc_fml::evaluate gave a value, and not modelled, naming the bead that owns it,
/// when it refused; and the evaluation record of what the observer was shown.
fn documents(run: &[(u32, Row, Shown)]) -> (ResultSet, InstrumentRecord) {
    let source = format!("sha256:{}", sha256_hex(WORKBOOK));
    let context = fallback_context().fingerprint();
    let mut results = Vec::new();
    let mut evaluations = Vec::new();
    for (at, row, shown) in run {
        let tick = u64::from(*at);
        let (provenance, value) = match (&shown.outcome, row.basis) {
            (Ok(value), Basis::Documented(_) | Basis::Measured) => (
                Provenance::Calculated {
                    tick,
                    revision: REVISION,
                    context: context.clone(),
                    external_links: false,
                },
                Some(value.clone()),
            ),
            (Err(fault), Basis::NotModelled { why, owner }) => (
                Provenance::NotModeled {
                    feature: format!("{why} (bead {owner})"),
                    diagnostic: format!("calc_fml::evaluate refuses {}: {fault:?}", row.formula),
                },
                None,
            ),
            (outcome, basis) => panic!("{}: {outcome:?} under {basis:?}", row.formula),
        };
        results.push(CellResult {
            cell: address(*at),
            provenance,
            value,
        });
        for (key, returned) in &shown.watch.reported {
            assert_eq!((key.row(), key.column()), (*at, COLUMN));
            evaluations.push(InstrumentedEvaluation::new(
                VerdictEvaluation {
                    cell: address(*at),
                    tick,
                    revision: REVISION,
                    context: context.clone(),
                },
                returned.clone().map_err(|fault| format!("{fault:?}")),
            ));
        }
    }
    (
        ResultSet {
            case: CAPTURE_CASE.to_owned(),
            source: source.clone(),
            run: RunIdentity {
                revision: REVISION,
                context,
            },
            results,
        },
        InstrumentRecord {
            case: CAPTURE_CASE.to_owned(),
            source,
            evaluations,
        },
    )
}

/// The verdict on `capture` of what the run publishes, and what on it disagrees with the rows: a
/// formula cell no row has, a calculated row not matched, a not-modelled row whose verdict is not
/// `not_modeled`, and a row whose cell the inventory lacks. Gives each not-modelled row's raw value
/// in the capture.
fn comparison(
    capture: &CaptureRecord,
    run: &[(u32, Row, Shown)],
) -> (VerdictReport, Vec<String>, Vec<String>) {
    let (results, evaluations) = documents(run);
    let policy = VerdictPolicy {
        numeric_mode: NumericMode::Fast,
        evidence: None,
    };
    let report = judge(capture, &results, &evaluations, &policy)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    let rows: BTreeMap<u32, &Row> = run.iter().map(|(at, row, _)| (*at, row)).collect();
    let mut problems = Vec::new();
    let mut excel_for_refused = Vec::new();
    for cell in &report.cells {
        let at = &cell.formula.cell;
        let row = (at.sheet() == SHEET && at.column() == COLUMN)
            .then(|| rows.get(&at.row()))
            .flatten();
        let raw = capture
            .observations
            .iter()
            .find(|observed| observed.cell == *at)
            .map(|observed| &observed.observation);
        match (row, &cell.verdict) {
            (None, verdict) => problems.push(format!(
                "{}!{}: a formula cell no row has ({})",
                at.sheet(),
                at.a1(),
                verdict.name()
            )),
            (Some(row), Verdict::NotModeled { .. })
                if matches!(row.basis, Basis::NotModelled { .. }) =>
            {
                let excel = match raw {
                    Some(Observation::ExcelRaw(value)) => value_shown(value),
                    other => format!("{other:?}"),
                };
                excel_for_refused.push(format!(
                    "D{} {}: Excel gives {excel}",
                    at.row(),
                    one_line(row.formula)
                ));
            }
            (Some(row), Verdict::Matched { .. })
                if !matches!(row.basis, Basis::NotModelled { .. }) => {}
            (Some(row), verdict) => problems.push(format!(
                "D{} {}: {}: {verdict:?} ({:?})",
                at.row(),
                row.formula,
                verdict.name(),
                row.basis
            )),
        }
    }
    for outside in &report.outside_inventory {
        problems.push(format!(
            "{}!{}: the capture's inventory lacks this row's cell",
            outside.cell.sheet(),
            outside.cell.a1()
        ));
    }
    (report, problems, excel_for_refused)
}

/// A capture record made by the case, observing `observe` for each row: the shape of a live
/// capture of the oracle workbook by the committed worker. Evidence about the comparison's
/// documents and the judge, not about Excel.
fn made_capture(
    run: &[(u32, Row, Shown)],
    observe: impl Fn(u32, &Row, &Shown) -> CalcValue,
) -> CaptureRecord {
    let source = format!("sha256:{}", sha256_hex(WORKBOOK));
    CaptureRecord {
        case: CAPTURE_CASE.to_owned(),
        source: source.clone(),
        // calc_verdict has no acquisition kind for a record a test makes (see dc_ibp2's
        // made_capture).
        acquisition: Acquisition::LiveCapture,
        environment: None,
        sheets: SHEETS
            .iter()
            .map(|name| SheetEntry {
                name: (*name).to_owned(),
                visibility: SheetVisibility::Visible,
            })
            .collect(),
        formulas: run
            .iter()
            .map(|(at, row, _)| FormulaCell {
                cell: address(*at),
                kind: FormulaKind::Ordinary,
                formula: Some(row.formula.to_owned()),
                dynamic_array: false,
            })
            .collect(),
        observations: run
            .iter()
            .map(|(at, row, shown)| CellObservation {
                cell: address(*at),
                observation: Observation::ExcelRaw(observe(*at, row, shown)),
            })
            .collect(),
        session: Some(CaptureSession {
            revision: REVISION,
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

/// The committed capture, judged against the rows.
pub(super) fn excel_comparison() {
    const CASE: &str = "dc_51ka__excel_comparison";
    let mut book = Book::new();
    let run = run_rows(&mut book);
    let refused_rows = run
        .iter()
        .filter(|(_, row, _)| matches!(row.basis, Basis::NotModelled { .. }))
        .count();
    let calculated = run.len() - refused_rows;

    // The documents and the judge, against captures the case makes (not Excel's): observing
    // every computed row's value, each is matched and each refused row not modelled; observing
    // one row otherwise, that row alone is reported.
    let value_of = |_: u32, _: &Row, shown: &Shown| {
        shown
            .outcome
            .clone()
            .unwrap_or_else(|_| CalcValue::error(WorksheetErrorCode::Ref))
    };
    let (report, problems, _) = comparison(&made_capture(&run, value_of), &run);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert_eq!(report.evaluation_record_version, 2);
    assert_eq!(
        (
            report.counts.calculated,
            report.counts.matched,
            report.counts.not_modeled
        ),
        (calculated as u64, calculated as u64, refused_rows as u64),
        "{}",
        report.summary()
    );
    let changed = made_capture(&run, |at, row, shown| {
        if row.formula == OFGEM_SHAPE {
            CalcValue::number(12.3457)
        } else {
            value_of(at, row, shown)
        }
    });
    let (_, problems, _) = comparison(&changed, &run);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains(OFGEM_SHAPE) && problems[0].contains("mismatched"),
        "{problems:?}"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: against captures made by the case from calc_fml's own values (not Excel's), calc_verdict::judge counts {calculated} rows calculated and matched and {refused_rows} not modelled; with the Ofgem-shape row observed as 12.3457 it reports that row alone, mismatched"
    );

    // Excel's capture, as committed.
    if let Some(missing) = MissingCapture::recognize(CAPTURE) {
        assert_eq!(
            (
                missing.case.as_str(),
                missing.fixture.as_str(),
                missing.command.as_str()
            ),
            (CAPTURE_CASE, WORKBOOK_PATH, CAPTURE_COMMAND)
        );
        note!("ACCEPT-NOT-RUN: {CASE}: {missing}");
        return;
    }
    let capture = CaptureRecord::decode(CAPTURE)
        .unwrap_or_else(|refusal| panic!("the committed capture does not decode: {refusal}"));
    assert_eq!(capture.case, CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let workbook = format!("sha256:{}", sha256_hex(WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let session = capture.session.as_ref().expect("a version 2 record");
    assert!(
        session.failures.is_empty(),
        "the capture records failures: {:?}",
        session.failures
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    // Each row's cell holds its formula as an ordinary formula: Excel reports a legacy formula
    // with the implicit-intersection operator @ before each call whose result could be a range
    // where one value is needed, which the row's formula does not write.
    let mut with_at = 0;
    for (at, row, _) in &run {
        let cell = capture
            .formulas
            .iter()
            .find(|formula| formula.cell == address(*at))
            .unwrap_or_else(|| panic!("no formula cell D{at}"));
        let (reported, had_at) =
            without_implicit_intersection(cell.formula.as_deref().unwrap_or_default());
        assert_eq!(
            (reported, cell.dynamic_array),
            (row.formula.to_owned(), false),
            "D{at}: Excel's formula text and dynamic-array flag"
        );
        if had_at {
            with_at += 1;
        }
    }
    let (report, problems, excel_for_refused) = comparison(&capture, &run);
    assert!(
        problems.is_empty(),
        "{} problems in the verdict on the committed capture by Excel {} ({}):\n{}",
        problems.len(),
        environment.target.excel_build(),
        report.summary(),
        problems.join("\n")
    );
    assert_eq!(
        (
            report.counts.calculated,
            report.counts.matched,
            report.counts.mismatched,
            report.counts.not_modeled
        ),
        (calculated as u64, calculated as u64, 0, refused_rows as u64),
        "{}",
        report.summary()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the committed capture ({}), taken by Excel {}: through calc_verdict::judge, calculated {calculated} of {} formula cells, matched {} of {calculated}, mismatched {}, not modeled {}; {with_at} rows Excel reports with its implicit-intersection @ ({})",
        CAPTURE_COMMAND,
        environment.target.excel_build(),
        report.counts.denominator,
        report.counts.matched,
        report.counts.mismatched,
        report.counts.not_modeled,
        report.summary()
    );
    for refused in &excel_for_refused {
        note!("ACCEPT-NOTE: {CASE}: not modelled, not compared: {refused}");
    }
    note!(
        "ACCEPT-NOTE: {CASE}: COM add-ins connected during the capture: {:?}",
        report.com_add_ins_connected
    );
}

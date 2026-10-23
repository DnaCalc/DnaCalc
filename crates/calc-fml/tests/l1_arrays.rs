//! L1 (dc-mzk): array constants, lifting over arrays and ranges, and array results, through
//! `calc_fml::evaluate`.
//!
//! The operator rows are OxFml's `operator_array_arithmetic_cases.json` (OxFml 15daac5,
//! `crates/oxfml_core/tests/fixtures`) and its evaluator tests
//! (`evaluator_lifts_*`, `evaluator_ifs_array_condition_*`, `evaluator_choose_array_index_*`);
//! the legacy rows are Excel 16.0's, measured through COM on 2026-10-23 (`Range.Formula` entry
//! over A1:A3 = 1, 2, 3 and B1:B3 = 10, 20, 30; see the `legacy` module of calc-fml).

mod l1_support;

use calc_fml::{CompileError, EvalFault, FormulaForm};
use calc_func::value::{CalcValue, WorksheetErrorCode as W};
use l1_support::{Book, b, e, n, numbers, rows, t};

fn data() -> Book {
    let mut book = Book::new();
    for (row, value) in [(1, 1.0), (2, 2.0), (3, 3.0)] {
        book.number(&format!("A{row}"), value);
        book.number(&format!("B{row}"), value * 10.0);
    }
    book
}

#[test]
fn array_constants_are_values() {
    let mut book = Book::new();
    assert_eq!(
        book.dynamic("={1,2;3,4}"),
        numbers(&[&[1.0, 2.0], &[3.0, 4.0]])
    );
    assert_eq!(
        book.dynamic("={1,\"a\";TRUE,#N/A}"),
        rows(&[&[n(1.0), t("a")], &[b(true), e(W::NA)]])
    );
    assert_eq!(book.dynamic("={-1,+2,-0}"), numbers(&[&[-1.0, 2.0, 0.0]]));
    assert_eq!(book.dynamic("=SUM({1,2;3,4})"), n(10.0));
    assert_eq!(book.dynamic("=ROWS({1;2;3})"), n(3.0));
    // Rows of different lengths, and a reference or an expression inside, are no array constant.
    for refused in ["={1,2;3}", "={1+1}", "={A1}"] {
        assert!(
            matches!(
                book.eval_in(FormulaForm::Dynamic, refused, "Z100"),
                Err(CompileError::Syntax { .. } | CompileError::Reference { .. })
            ),
            "{refused}"
        );
    }
}

#[test]
fn operators_lift_over_arrays_as_oxfml_fixture_rows() {
    let mut book = Book::new();
    let cases: [(&str, _); 8] = [
        (
            "={1,2,3;2,3,4}*-1",
            numbers(&[&[-1.0, -2.0, -3.0], &[-2.0, -3.0, -4.0]]),
        ),
        ("=2*{1,2;3,4}", numbers(&[&[2.0, 4.0], &[6.0, 8.0]])),
        ("=10-{1,2;3,4}", numbers(&[&[9.0, 8.0], &[7.0, 6.0]])),
        (
            "={1,2;3,4}+{10,20;30,40}",
            numbers(&[&[11.0, 22.0], &[33.0, 44.0]]),
        ),
        (
            "={8,6;4,2}/{2,0;1,2}",
            rows(&[&[n(4.0), e(W::Div0)], &[n(4.0), n(1.0)]]),
        ),
        (
            "={1,2;3,4}+{10,20,30}",
            rows(&[&[n(11.0), n(22.0), e(W::NA)], &[n(13.0), n(24.0), e(W::NA)]]),
        ),
        (
            "={\"2\",3;\"4\",\"5\"}+1",
            numbers(&[&[3.0, 4.0], &[5.0, 6.0]]),
        ),
        ("={1,\"bad\"}+1", rows(&[&[n(2.0), e(W::Value)]])),
    ];
    for (formula, expected) in cases {
        assert_eq!(book.dynamic(formula), expected, "{formula}");
    }
    assert_eq!(
        book.dynamic("=-{1,2,3;2,3,4}"),
        numbers(&[&[-1.0, -2.0, -3.0], &[-2.0, -3.0, -4.0]])
    );
    assert_eq!(
        book.dynamic("={1;2}&\"x\""),
        rows(&[&[t("1x")], &[t("2x")]])
    );
    assert_eq!(
        book.dynamic("={1,2,3}>=2"),
        rows(&[&[b(false), b(true), b(true)]])
    );
    assert_eq!(book.dynamic("={1,2}%"), numbers(&[&[0.01, 0.02]]));
}

#[test]
fn functions_of_single_values_lift() {
    let mut book = Book::new();
    assert_eq!(
        book.dynamic("=ROUND({1.25,2.75},1)"),
        numbers(&[&[1.3, 2.8]])
    );
    assert_eq!(
        book.dynamic("=ROUND(2.345,{0;1;2})"),
        numbers(&[&[2.0], &[2.3], &[2.35]])
    );
    assert_eq!(
        book.dynamic("=ISNUMBER({1,\"a\"})"),
        rows(&[&[b(true), b(false)]])
    );
    assert_eq!(
        book.dynamic("=NOT({TRUE,FALSE})"),
        rows(&[&[b(false), b(true)]])
    );
    // An aggregate takes the array whole.
    assert_eq!(book.dynamic("=SUM({1,2,3}*2)"), n(12.0));
    assert_eq!(book.dynamic("=SUMPRODUCT({1,2},{3,4})"), n(11.0));
}

#[test]
fn lazy_selectors_lift_over_an_array_that_decides() {
    let mut book = Book::new();
    assert_eq!(
        book.dynamic("=IF({TRUE,FALSE},1,2)"),
        numbers(&[&[1.0, 2.0]])
    );
    assert_eq!(
        book.dynamic("=IF({1,0,1},{10,20,30},\"n\")"),
        rows(&[&[n(10.0), t("n"), n(30.0)]])
    );
    assert_eq!(
        book.dynamic("=CHOOSE({1,2},\"a\",\"b\")"),
        rows(&[&[t("a"), t("b")]])
    );
    assert_eq!(
        book.dynamic("=IFERROR({1,#N/A,3},0)"),
        numbers(&[&[1.0, 0.0, 3.0]])
    );
    assert_eq!(
        book.dynamic("=IFNA({#N/A,#DIV/0!},\"x\")"),
        rows(&[&[t("x"), e(W::Div0)]])
    );
    // A scalar decision stays lazy: the untaken branch does not run.
    assert_eq!(book.dynamic("=IF(TRUE,1,1/0)"), n(1.0));
}

#[test]
fn dynamic_formulas_lift_over_ranges_and_spill() {
    let mut book = data();
    assert_eq!(book.dynamic("=A1:A3*2"), numbers(&[&[2.0], &[4.0], &[6.0]]));
    assert_eq!(
        book.dynamic("=A1:A3+B1:B3"),
        numbers(&[&[11.0], &[22.0], &[33.0]])
    );
    assert_eq!(book.dynamic("=SUM(A1:A3*B1:B3)"), n(140.0));
    assert_eq!(
        book.dynamic("=A1:B2"),
        numbers(&[&[1.0, 10.0], &[2.0, 20.0]])
    );
    assert_eq!(book.dynamic("=SUM(IF(A1:A3>1,1,0))"), n(2.0));
    assert_eq!(
        book.dynamic("=A1:A4"),
        rows(&[&[n(1.0)], &[n(2.0)], &[n(3.0)], &[CalcValue::empty()]])
    );
}

/// The legacy rows measured in Excel 16.0 (see the module documentation), each entered in row 2
/// unless it says otherwise.
#[test]
fn legacy_formulas_intersect_where_excel_writes_at() {
    let mut book = data();
    let cases: [(&str, &str, _); 18] = [
        ("=A1:A3*1", "I2", n(2.0)),
        ("={1,2}*2", "I2", n(2.0)),
        ("=ABS({-1,-2})", "I4", n(1.0)),
        ("={4,5,6}", "I6", n(4.0)),
        ("=IFERROR(A1:A3,0)", "I2", n(2.0)),
        ("=A1:A3", "I3", n(3.0)),
        ("=A1:A3", "I5", e(W::Value)),
        ("=SUM(A1:A3*2)", "I2", n(4.0)),
        ("=SUM(ABS(A1:A3))", "I2", n(2.0)),
        ("=SUM({1,2}*2)", "I2", n(6.0)),
        ("=SUM(ABS({-1,-2}))", "I2", n(3.0)),
        ("=SUM({1,2}*A1:A2)", "I2", n(6.0)),
        ("=SUMPRODUCT(A1:A3*2)", "I2", n(12.0)),
        ("=SUMPRODUCT((A1:A3>1)*B1:B3)", "I2", n(50.0)),
        ("=SUMPRODUCT(ABS(A1:A3))", "I2", n(6.0)),
        ("=SUMPRODUCT(IFERROR(A1:A3,0))", "I2", n(2.0)),
        ("=SUM(IF(TRUE,A1:A3))", "I2", n(6.0)),
        ("=SUM(IFERROR(A1:A3,0))", "I2", n(2.0)),
    ];
    for (formula, at, expected) in cases {
        assert_eq!(book.legacy_at(formula, at), expected, "{formula} in {at}");
    }
    assert_eq!(book.legacy_at("=IF(A1:A3>1,1,0)", "I7"), e(W::Value));
    assert_eq!(book.legacy_at("=SUMPRODUCT(IF(A1:A3>1,1,0))", "I2"), n(1.0));
    assert_eq!(book.legacy_at("=COUNT(A1:A3*1)", "I2"), n(1.0));
    assert_eq!(book.legacy_at("=MAX(A1:A3*1)", "I2"), n(2.0));
    assert_eq!(
        book.legacy_at("=LOOKUP(2,1/(A1:A3=2),B1:B3)", "I2"),
        n(20.0)
    );
}

/// Where the measurements do not say what a legacy formula does, a range at a single-value place
/// is refused, never guessed.
#[test]
fn legacy_formulas_refuse_unmeasured_places() {
    let mut book = data();
    let outcome = book
        .eval_in(FormulaForm::Legacy, "=XLOOKUP(2,A1:A3*1,B1:B3)", "I2")
        .unwrap();
    assert!(
        matches!(outcome, Err(EvalFault::AreaAsValue { .. })),
        "{outcome:?}"
    );
}

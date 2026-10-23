//! L1 (dc-mzk): the range, union and intersection operators, implicit intersection (`@`), spill
//! references (`#`) and OFFSET, through `calc_fml::evaluate`.
//!
//! Ported in kind from OxFml's evaluator tests (`evaluator_dispatches_range_and_intersection_*`,
//! `evaluator_runs_indirect_offset_and_iferror`, OxFml 15daac5), which ran without a grid; the
//! values here are Excel 16.0's, measured through COM on 2026-10-23 where a row says so.

mod l1_support;

use calc_fml::{EvalFault, FormulaForm};
use calc_func::value::{CalcValue, WorksheetErrorCode as W};
use l1_support::{Book, e, n, numbers};

fn grid() -> Book {
    let mut book = Book::new();
    for row in 1..=10u32 {
        for (col, letter) in ["A", "B", "C", "D", "E"].iter().enumerate() {
            book.number(
                &format!("{letter}{row}"),
                f64::from(100 * row) + col as f64 + 1.0,
            );
        }
    }
    book
}

#[test]
fn the_range_operator_spans_its_operands() {
    let mut book = grid();
    // A1:B2:C4 is A1:C4 (Excel: =ROWS(A1:B2:C4) is 4).
    assert_eq!(book.dynamic("=ROWS(A1:B2:C4)"), n(4.0));
    assert_eq!(book.dynamic("=COLUMNS(A1:B2:C4)"), n(3.0));
    assert_eq!(
        book.dynamic("=SUM((A1):(B2))"),
        n(101.0 + 102.0 + 201.0 + 202.0)
    );
    assert_eq!(
        book.dynamic("=SUM(A1:INDEX(A1:A10,3))"),
        n(101.0 + 201.0 + 301.0)
    );
    assert_eq!(book.dynamic("=ROWS(Sheet1!A1:Sheet1!A4)"), n(4.0));
}

#[test]
fn the_intersection_operator_takes_shared_cells() {
    let mut book = grid();
    assert_eq!(
        book.dynamic("=A1:C3 B2:D4"),
        numbers(&[&[202.0, 203.0], &[302.0, 303.0]])
    );
    assert_eq!(
        book.dynamic("=SUM(A1:C3 B2:D4)"),
        n(202.0 + 203.0 + 302.0 + 303.0)
    );
    // Excel: =SUM((A1:A2 B1:B2)) is #NULL!; areas on two sheets are #VALUE!.
    assert_eq!(book.dynamic("=SUM((A1:A2 B1:B2))"), e(W::Null));
    assert_eq!(
        book.dynamic("=SUM((Sheet1!A1:A2 Sheet2!A1:A2))"),
        e(W::Value)
    );
}

#[test]
fn the_union_operator_joins_areas() {
    let mut book = grid();
    assert_eq!(book.dynamic("=SUM((A1:A2,C3))"), n(101.0 + 201.0 + 303.0));
    assert_eq!(book.dynamic("=COUNT((A1:A2,C3,A1))"), n(4.0));
    // Excel: =SUM((S1!A1,S2!A1)) is #VALUE!.
    assert_eq!(book.dynamic("=SUM((Sheet1!A1,Sheet2!A1))"), e(W::Value));
    // A union where one value is needed is #VALUE!.
    assert_eq!(book.dynamic("=ABS((A1,B1))"), e(W::Value));
}

#[test]
fn implicit_intersection_takes_one_value() {
    let mut book = grid();
    // Excel: =@S1!C1:E10 entered on another sheet in D5 is S1!D5; in F5 and D11 #VALUE!.
    assert_eq!(book.dynamic_at("=@Sheet1!C1:E10", "Sheet2!D5"), n(504.0));
    assert_eq!(book.dynamic_at("=@Sheet1!C1:E10", "Sheet2!F5"), e(W::Value));
    assert_eq!(
        book.dynamic_at("=@Sheet1!C1:E10", "Sheet2!D11"),
        e(W::Value)
    );
    // A column takes the formula's row; a blank cell is 0 (Excel: =@S1!A1:A10 over a blank).
    assert_eq!(book.dynamic_at("=@Sheet1!B1:B10", "Sheet2!Z7"), n(702.0));
    assert_eq!(book.dynamic_at("=@Sheet1!F1:F10", "Sheet2!A5"), n(0.0));
    // Excel: =@{7,8} is 7; the file form _xlfn.SINGLE is the same operator.
    assert_eq!(book.dynamic("=@{7,8}"), n(7.0));
    assert_eq!(
        book.dynamic_at("=_xlfn.SINGLE(Sheet1!C1:E10)", "Sheet2!D5"),
        n(504.0)
    );
    // Excel: =@A1:A3 entered in H22 is #VALUE!.
    assert_eq!(book.dynamic_at("=@A1:A3", "H22"), e(W::Value));
}

#[test]
fn spill_references_read_the_spilled_range() {
    let mut book = grid();
    let spilled = book.area("A1", "A3");
    let anchor = book.at("A1");
    book.spills.insert(anchor, spilled);
    assert_eq!(book.dynamic("=SUM(A1#)"), n(101.0 + 201.0 + 301.0));
    assert_eq!(
        book.dynamic("=SUM(_xlfn.ANCHORARRAY(A1))"),
        n(101.0 + 201.0 + 301.0)
    );
    assert_eq!(book.dynamic("=ROWS(B1#)"), e(W::Ref));
    // A reader that does not know spills refuses the reference, naming L2.
    book.knows_spills = false;
    let outcome = book
        .eval_in(FormulaForm::Dynamic, "=SUM(A1#)", "Z100")
        .unwrap();
    assert_eq!(
        outcome,
        Err(EvalFault::NotModelled {
            construct: "a spilled range reference (#) the reader does not know",
            owner: "dc-0k9"
        })
    );
}

#[test]
fn offset_shifts_references() {
    let mut book = grid();
    assert_eq!(book.dynamic("=OFFSET(A1,2,1)"), n(302.0));
    assert_eq!(
        book.dynamic("=SUM(OFFSET(A1,1,1,2,2))"),
        n(202.0 + 203.0 + 302.0 + 303.0)
    );
    // Excel: an omitted or blank offset is 0.
    assert_eq!(book.dynamic("=OFFSET(A1,,1)"), n(102.0));
    assert_eq!(book.dynamic("=OFFSET(A1,Z50,0)"), n(101.0));
    assert_eq!(book.dynamic("=OFFSET(A1,-1,0)"), e(W::Ref));
    assert_eq!(book.dynamic("=ROWS(OFFSET(A5,0,0,-2))"), n(2.0));
    assert_eq!(
        book.dynamic("=OFFSET(A1,0,0,2,1)"),
        numbers(&[&[101.0], &[201.0]])
    );
    // Lifted, OFFSET gives references, which are not modelled (Excel: =OFFSET(A1,{0;1},0) is
    // #VALUE! in a cell, yet =SUM(OFFSET(A1,{0;1;2},0)) is 6).
    let lifted = book
        .eval_in(FormulaForm::Dynamic, "=OFFSET(A1,{0;1},0)", "Z100")
        .unwrap();
    assert!(
        matches!(lifted, Err(EvalFault::NotModelled { .. })),
        "{lifted:?}"
    );
    // In a legacy formula OFFSET's result is intersected.
    assert_eq!(book.legacy_at("=OFFSET(A1,0,0,3,1)", "Z2"), n(201.0));
}

#[test]
fn ranges_meeting_values_follow_the_form() {
    let mut book = grid();
    let _ = CalcValue::empty();
    assert_eq!(book.legacy_at("=A1:A10+0", "Z4"), n(401.0));
    assert_eq!(book.dynamic("=ROWS(A1:A10+0)"), n(10.0));
}

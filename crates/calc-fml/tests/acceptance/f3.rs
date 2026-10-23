//! F3 (dc-728): the dynamic-array functions (SEQUENCE, FILTER, SORT, SORTBY, UNIQUE, the stacking
//! and shaping functions) and the LAMBDA helpers (MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY),
//! through `calc_fml::evaluate` as dynamic-array formulas on L1's workbook ([`Book`]). Expected
//! values are Microsoft's documented examples and the rules OxFunc's callable_helpers.rs and
//! dynamic_array_reshape_family.rs record (OxFunc 6fca3518).

use super::l1::{Book, e, n, numbers, rows, t};
use calc_func::value::WorksheetErrorCode as W;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

/// A1:A3 hold 1, 2 and 3; B1:B3 hold "b", "A" and "c".
fn data() -> Book {
    let mut book = Book::new();
    for (row, value) in [(1, 1.0), (2, 2.0), (3, 3.0)] {
        book.number(&format!("A{row}"), value);
    }
    book.set("B1", t("b")).set("B2", t("A")).set("B3", t("c"));
    book
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn sequence_filter_sort_unique() {
    let mut book = data();
    assert_eq!(book.dynamic("=SEQUENCE(2,3)"), numbers(&[&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]]));
    assert_eq!(book.dynamic("=SEQUENCE(3,1,10,-2)"), numbers(&[&[10.0], &[8.0], &[6.0]]));
    assert_eq!(book.dynamic("=SEQUENCE(0)"), e(W::Calc));
    assert_eq!(book.dynamic("=SUM(SEQUENCE(10))"), n(55.0));
    assert_eq!(book.dynamic("=_xlfn.SEQUENCE(2)"), numbers(&[&[1.0], &[2.0]]));
    assert_eq!(book.dynamic("=FILTER(A1:A3,A1:A3>1)"), numbers(&[&[2.0], &[3.0]]));
    assert_eq!(book.dynamic("=FILTER(A1:A3,A1:A3>5)"), e(W::Calc));
    assert_eq!(book.dynamic("=FILTER(A1:A3,A1:A3>5,\"none\")"), t("none"));
    assert_eq!(book.dynamic("=_xlfn._xlws.FILTER(B1:B3,A1:A3<>2)"), rows(&[&[t("b")], &[t("c")]]));
    assert_eq!(book.dynamic("=SORT({3;1;2})"), numbers(&[&[1.0], &[2.0], &[3.0]]));
    assert_eq!(book.dynamic("=SORT({3;1;2},,-1)"), numbers(&[&[3.0], &[2.0], &[1.0]]));
    // Text sorts ignoring case.
    assert_eq!(book.dynamic("=SORT(B1:B3)"), rows(&[&[t("A")], &[t("b")], &[t("c")]]));
    assert_eq!(book.dynamic("=SORT({1,\"b\";2,\"a\"},2)"), rows(&[&[n(2.0), t("a")], &[n(1.0), t("b")]]));
    assert_eq!(book.dynamic("=SORT({3,1,2},,,TRUE)"), numbers(&[&[1.0, 2.0, 3.0]]));
    assert_eq!(book.dynamic("=SORT({1;2},3)"), e(W::Value));
    assert_eq!(
        book.dynamic("=SORTBY({\"a\";\"b\";\"c\"},{3;1;2})"),
        rows(&[&[t("b")], &[t("c")], &[t("a")]])
    );
    assert_eq!(
        book.dynamic("=SORTBY({\"a\";\"b\";\"c\"},{1;1;2},-1,{1;2;3},1)"),
        rows(&[&[t("c")], &[t("a")], &[t("b")]])
    );
    assert_eq!(book.dynamic("=UNIQUE({1;2;1;3})"), numbers(&[&[1.0], &[2.0], &[3.0]]));
    assert_eq!(book.dynamic("=UNIQUE({1;2;1;3},,TRUE)"), numbers(&[&[2.0], &[3.0]]));
    assert_eq!(book.dynamic("=UNIQUE({\"a\";\"A\"})"), rows(&[&[t("a")]]));
    assert_eq!(book.dynamic("=UNIQUE({1,1,2},TRUE)"), numbers(&[&[1.0, 2.0]]));
    assert_eq!(book.dynamic("=UNIQUE({1;1},,TRUE)"), e(W::Calc));
    assert_eq!(book.dynamic("=ROWS(UNIQUE(A1:A3))"), n(3.0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn taking_choosing_stacking_and_wrapping() {
    let mut book = data();
    let grid = "{1,2;3,4;5,6}";
    assert_eq!(book.dynamic(&format!("=TAKE({grid},2)")), numbers(&[&[1.0, 2.0], &[3.0, 4.0]]));
    assert_eq!(book.dynamic(&format!("=TAKE({grid},-1)")), numbers(&[&[5.0, 6.0]]));
    assert_eq!(book.dynamic(&format!("=TAKE({grid},,1)")), numbers(&[&[1.0], &[3.0], &[5.0]]));
    assert_eq!(book.dynamic(&format!("=DROP({grid},1,1)")), numbers(&[&[4.0], &[6.0]]));
    assert_eq!(book.dynamic(&format!("=DROP({grid},3)")), e(W::Calc));
    assert_eq!(book.dynamic("=TAKE(A1:A3,2)"), numbers(&[&[1.0], &[2.0]]));
    assert_eq!(book.dynamic(&format!("=CHOOSEROWS({grid},3,1)")), numbers(&[&[5.0, 6.0], &[1.0, 2.0]]));
    assert_eq!(book.dynamic(&format!("=CHOOSEROWS({grid},-1)")), numbers(&[&[5.0, 6.0]]));
    assert_eq!(book.dynamic(&format!("=CHOOSEROWS({grid},4)")), e(W::Value));
    assert_eq!(book.dynamic("=CHOOSECOLS({1,2,3},{3,1})"), numbers(&[&[3.0, 1.0]]));
    assert_eq!(book.dynamic("=VSTACK({1,2},{3})"), rows(&[&[n(1.0), n(2.0)], &[n(3.0), e(W::NA)]]));
    assert_eq!(book.dynamic("=HSTACK({1;2},3)"), rows(&[&[n(1.0), n(3.0)], &[n(2.0), e(W::NA)]]));
    assert_eq!(book.dynamic("=VSTACK(A1:A2,A3)"), numbers(&[&[1.0], &[2.0], &[3.0]]));
    assert_eq!(book.dynamic("=TOCOL({1,2;3,4})"), numbers(&[&[1.0], &[2.0], &[3.0], &[4.0]]));
    assert_eq!(book.dynamic("=TOCOL({1,2;3,4},,TRUE)"), numbers(&[&[1.0], &[3.0], &[2.0], &[4.0]]));
    assert_eq!(book.dynamic("=TOROW({1;2})"), numbers(&[&[1.0, 2.0]]));
    assert_eq!(book.dynamic("=TOROW({1,#N/A,2},2)"), numbers(&[&[1.0, 2.0]]));
    assert_eq!(
        book.dynamic("=WRAPROWS({1,2,3,4,5},2)"),
        rows(&[&[n(1.0), n(2.0)], &[n(3.0), n(4.0)], &[n(5.0), e(W::NA)]])
    );
    assert_eq!(book.dynamic("=WRAPCOLS({1,2,3},2,0)"), numbers(&[&[1.0, 3.0], &[2.0, 0.0]]));
    assert_eq!(book.dynamic("=EXPAND({1,2},2,3,0)"), numbers(&[&[1.0, 2.0, 0.0], &[0.0, 0.0, 0.0]]));
    assert_eq!(book.dynamic("=EXPAND({1,2},1,1)"), e(W::Value));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn lambda_helpers_call_their_lambda() {
    let mut book = data();
    assert_eq!(book.dynamic("=MAP({1,2,3},LAMBDA(x,x*2))"), numbers(&[&[2.0, 4.0, 6.0]]));
    assert_eq!(book.dynamic("=MAP({1,2},{10,20},LAMBDA(a,b,a+b))"), numbers(&[&[11.0, 22.0]]));
    assert_eq!(book.dynamic("=MAP(A1:A3,LAMBDA(x,x+1))"), numbers(&[&[2.0], &[3.0], &[4.0]]));
    assert_eq!(book.dynamic("=MAP({1,2},LAMBDA(x,x+A1))"), numbers(&[&[2.0, 3.0]]));
    assert_eq!(book.dynamic("=LET(f,LAMBDA(x,x^2),MAP({1,2,3},f))"), numbers(&[&[1.0, 4.0, 9.0]]));
    assert_eq!(book.dynamic("=SUM(MAP({1,2,3},LAMBDA(x,x*2)))"), n(12.0));
    assert_eq!(
        book.dynamic("=_xlfn.MAP({1,2},_xlfn.LAMBDA(_xlpm.x,_xlpm.x*3))"),
        numbers(&[&[3.0, 6.0]])
    );
    assert_eq!(book.dynamic("=REDUCE(0,{1,2,3},LAMBDA(a,b,a+b))"), n(6.0));
    assert_eq!(book.dynamic("=REDUCE(1,A1:A3,LAMBDA(a,b,a*b))"), n(6.0));
    assert_eq!(book.dynamic("=REDUCE(\"\",{\"a\",\"b\"},LAMBDA(acc,x,acc&x))"), t("ab"));
    // The accumulator may be an array.
    assert_eq!(
        book.dynamic("=REDUCE(0,{1,2},LAMBDA(a,b,VSTACK(a,b)))"),
        numbers(&[&[0.0], &[1.0], &[2.0]])
    );
    assert_eq!(book.dynamic("=SCAN(0,{1,2,3},LAMBDA(a,b,a+b))"), numbers(&[&[1.0, 3.0, 6.0]]));
    assert_eq!(book.dynamic("=BYROW({1,2;3,4},LAMBDA(r,SUM(r)))"), numbers(&[&[3.0], &[7.0]]));
    assert_eq!(book.dynamic("=BYCOL({1,2;3,4},LAMBDA(c,MAX(c)))"), numbers(&[&[3.0, 4.0]]));
    assert_eq!(
        book.dynamic("=MAKEARRAY(2,3,LAMBDA(r,c,r*10+c))"),
        numbers(&[&[11.0, 12.0, 13.0], &[21.0, 22.0, 23.0]])
    );
    // A call that returns an array, a value that is no LAMBDA, a LAMBDA of the wrong arity, and
    // a dimension that is not a positive integer.
    assert_eq!(book.dynamic("=MAP({1,2},LAMBDA(x,{1,2}))"), e(W::Calc));
    assert_eq!(book.dynamic("=BYROW({1,2;3,4},LAMBDA(r,r))"), e(W::Calc));
    assert_eq!(book.dynamic("=MAP({1,2},5)"), e(W::Value));
    assert_eq!(book.dynamic("=MAP({1,2},#N/A)"), e(W::NA));
    assert_eq!(book.dynamic("=MAP({1,2},LAMBDA(x,y,x))"), e(W::Value));
    assert_eq!(book.dynamic("=MAKEARRAY(0,1,LAMBDA(r,c,1))"), e(W::Value));
    assert_eq!(book.dynamic("=MAKEARRAY(1.5,1,LAMBDA(r,c,1))"), e(W::Value));
    // Helpers nest, and an error inside a LAMBDA is the cell's value.
    assert_eq!(
        book.dynamic("=MAP({1,2},LAMBDA(x,REDUCE(0,SEQUENCE(x),LAMBDA(a,b,a+b))))"),
        numbers(&[&[1.0, 3.0]])
    );
    assert_eq!(book.dynamic("=MAP({1,0},LAMBDA(x,1/x))"), rows(&[&[n(1.0), e(W::Div0)]]));
}

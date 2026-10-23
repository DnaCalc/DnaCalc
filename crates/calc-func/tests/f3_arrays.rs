//! F3 (dc-728): the dynamic-array functions and the LAMBDA helpers called through
//! `calc_func::invoke` alone: a reference argument read as the array of its cells, and the
//! refusal of a LAMBDA helper whose services carry no caller of LAMBDA values. The formula route
//! (calc-fml's evaluator, which supplies that caller) is tested in calc-fml's `acceptance/f3.rs`.

mod f1_support;

use calc_func::value::WorksheetErrorCode::{Calc, Value};
use f1_support::*;

#[test]
fn references_are_read_as_arrays() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(3.0), n(1.0), n(2.0)]);
    let range = sheet.area(1, 1, 3, 1);
    assert_eq!(array(call_on(&mut sheet, "SORT", vec![range.clone()])), (3, 1, vec![1.0, 2.0, 3.0]));
    // TAKE and DROP of a range give the reference to the cells they keep; of an array, the array.
    let kept = call_on(&mut sheet, "TAKE", vec![range.clone(), n(-2.0)]).expect("a reference");
    assert_eq!(kept, sheet.area(2, 1, 3, 1));
    let dropped = call_on(&mut sheet, "DROP", vec![range.clone(), n(1.0)]).expect("a reference");
    assert_eq!(dropped, sheet.area(2, 1, 3, 1));
    error(call_on(&mut sheet, "DROP", vec![range.clone(), n(3.0)]), Calc);
    assert_eq!(array(call("TAKE", vec![col(&[3.0, 1.0, 2.0]), n(-2.0)])), (2, 1, vec![1.0, 2.0]));
    assert_eq!(array(call_on(&mut sheet, "TOROW", vec![range.clone()])), (1, 3, vec![3.0, 1.0, 2.0]));
    // A blank cell of the range is an empty value, which TOCOL's ignore 1 leaves out.
    let wider = sheet.area(1, 1, 4, 1);
    assert_eq!(
        array(call_on(&mut sheet, "TOCOL", vec![wider, n(1.0)])),
        (3, 1, vec![3.0, 1.0, 2.0])
    );
    error(call_on(&mut sheet, "FILTER", vec![range.clone(), arr(vec![vec![0.0], vec![0.0], vec![0.0]])]), Calc);
    assert_eq!(array(call("SEQUENCE", vec![n(2.0), n(2.0), n(0.0), n(0.5)])), (2, 2, vec![0.0, 0.5, 1.0, 1.5]));
    error(call("SEQUENCE", vec![n(-1.0)]), Value);
}

/// A callable value of one parameter whose identity no evaluator knows.
#[derive(Debug)]
struct Opaque;

impl calc_func::value::OpaqueCallable for Opaque {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn a_lambda(params: usize) -> calc_func::value::CalcValue {
    calc_func::value::CalcValue::callable(calc_func::value::CallableValue::new(
        calc_func::value::CallableArityShape::exact(params),
        "LAMBDA",
        std::sync::Arc::new(Opaque),
    ))
}

#[test]
fn a_lambda_helper_without_a_caller_is_refused() {
    refused(call("MAP", vec![arr(vec![vec![1.0]]), a_lambda(1)]));
    refused(call("MAKEARRAY", vec![n(1.0), n(1.0), a_lambda(2)]));
    // What the helper decides before any call stays a worksheet error: a value that is no LAMBDA,
    // a LAMBDA of the wrong arity, a dimension that is not a positive integer.
    error(call("MAP", vec![arr(vec![vec![1.0]]), n(5.0)]), Value);
    error(call("REDUCE", vec![n(0.0), arr(vec![vec![1.0]]), a_lambda(1)]), Value);
    error(call("MAKEARRAY", vec![n(0.0), n(1.0), a_lambda(2)]), Value);
}

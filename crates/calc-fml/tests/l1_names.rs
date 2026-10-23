//! L1 (dc-mzk): defined names at workbook and sheet scope, named LAMBDAs, and the refusal of a
//! name a catalogue cannot supply, through `calc_fml::evaluate`.
//!
//! Ported from OxFml's evaluator tests (OxFml 15daac5, `crates/oxfml_core/tests/
//! evaluator_tests.rs`: `evaluator_uses_defined_name_bindings_for_sum`,
//! `evaluator_direct_deref_of_unresolved_name_yields_name_error_value`,
//! `evaluator_invokes_defined_name_callable_binding`,
//! `evaluator_executes_bounded_recursive_defined_name_callable`,
//! `evaluator_executes_bounded_mutual_recursive_defined_name_callables`), with the names held by
//! the test workbook's catalogue.

mod l1_support;

use calc_fml::{CompileError, EvalFault, FormulaForm};
use calc_func::value::WorksheetErrorCode as W;
use l1_support::{Book, b, e, n, numbers};

#[test]
fn workbook_names_hold_values_and_references() {
    let mut book = Book::new();
    book.number("Sheet1!B2", 40.0)
        .number("Sheet1!B3", 2.0)
        .name("InputValue", "=40")
        .name("TaxRate", "0.25")
        .name("Inputs", "Sheet1!$B$2:$B$3")
        .name("Total", "=SUM(Inputs)*(1+TaxRate)");
    assert_eq!(book.dynamic("=SUM(InputValue,2)"), n(42.0));
    assert_eq!(book.dynamic("=taxrate*4"), n(1.0));
    assert_eq!(book.dynamic("=SUM(Inputs)"), n(42.0));
    assert_eq!(book.dynamic("=ROWS(Inputs)"), n(2.0));
    assert_eq!(book.dynamic("=Total"), n(52.5));
    assert_eq!(book.dynamic("=Inputs"), numbers(&[&[40.0], &[2.0]]));
    // A legacy formula intersects a name's range as it does a written one.
    assert_eq!(book.legacy_at("=Inputs", "Sheet2!C3"), n(2.0));
}

#[test]
fn sheet_names_shadow_workbook_names() {
    let mut book = Book::new();
    book.name("Rate", "1").local_name("Sheet2", "Rate", "2");
    assert_eq!(book.dynamic_at("=Rate", "Sheet1!A1"), n(1.0));
    assert_eq!(book.dynamic_at("=Rate", "Sheet2!A1"), n(2.0));
    assert_eq!(book.dynamic_at("=Sheet2!Rate*10", "Sheet1!A1"), n(20.0));
    // Sheet1 holds no name of its own called Rate.
    assert_eq!(book.dynamic_at("=Sheet1!Rate", "Sheet2!A1"), e(W::Name));
}

#[test]
fn unknown_names_are_name_errors_and_unknown_catalogues_refuse() {
    let mut book = Book::new();
    assert_eq!(book.dynamic("=Missing"), e(W::Name));
    assert_eq!(book.dynamic("=IFERROR(Missing,2)"), n(2.0));
    assert_eq!(book.dynamic("=ISERROR(NoSuchFunction(1))"), b(true));
    // A catalogue that does not carry names: every name is refused, naming D1.
    book.knows_names = false;
    match book.eval_in(FormulaForm::Dynamic, "=TaxRate*2", "Z100") {
        Err(CompileError::Unsupported {
            construct, owner, ..
        }) => assert_eq!((construct, owner), ("a defined name", "dc-ndi")),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        book.eval_in(FormulaForm::Dynamic, "=NoSuchFunction(1)", "Z100"),
        Err(CompileError::UnknownFunction { .. })
    ));
}

#[test]
fn names_with_relative_references_are_refused() {
    let mut book = Book::new();
    book.name("Left", "Sheet1!A1");
    match book.eval_in(FormulaForm::Dynamic, "=Left", "Z100") {
        Err(CompileError::Unsupported { owner, .. }) => assert_eq!(owner, "dc-mzk"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn named_lambdas_are_called_and_may_recurse() {
    // OxFml's rows call these Fact, IsEven and IsOdd, which are Excel's built-in FACT, ISEVEN and
    // ISODD: Excel calls the built-in for such a call, so they are renamed here.
    let mut book = Book::new();
    book.name("AddOne", "LAMBDA(x,x+1)")
        .name("MyFact", "=LAMBDA(n,IF(n<=1,1,n*MyFact(n-1)))")
        .name("EvenQ", "LAMBDA(n,IF(n=0,TRUE,OddQ(n-1)))")
        .name("OddQ", "LAMBDA(n,IF(n=0,FALSE,EvenQ(n-1)))");
    assert_eq!(book.dynamic("=AddOne(2)"), n(3.0));
    assert_eq!(book.dynamic("=MyFact(5)"), n(120.0));
    assert_eq!(book.dynamic("=EvenQ(10)"), b(true));
    assert_eq!(book.dynamic("=EvenQ(7)"), b(false));
    // A named LAMBDA reached as a value is #CALC! in a cell: the evaluator returns the LAMBDA.
    let value = book.dynamic("=AddOne");
    assert!(value.callable_value().is_some());
    assert_eq!(value.as_error(), Some(W::Calc));
    // A built-in name calls the built-in, never a name of that spelling.
    book.name("SUM", "LAMBDA(x,0)");
    assert_eq!(book.dynamic("=SUM(1,2)"), n(3.0));
}

#[test]
fn recursion_deeper_than_the_evaluator_nests_is_refused() {
    let mut book = Book::new();
    book.name("CountDown", "LAMBDA(n,IF(n<=0,0,1+CountDown(n-1)))");
    assert_eq!(book.dynamic("=CountDown(40)"), n(40.0));
    let deep = book
        .eval_in(FormulaForm::Dynamic, "=CountDown(5000)", "Z100")
        .unwrap();
    assert!(
        matches!(
            deep,
            Err(EvalFault::NotModelled {
                owner: "dc-2mlz",
                ..
            })
        ),
        "{deep:?}"
    );
}

/// The deepest recursion the evaluator runs fits a 1 MB stack (the main thread's on Windows),
/// in an unoptimised build: each level is a named LAMBDA call through IF, a lazy call.
#[test]
fn the_deepest_recursion_fits_a_one_megabyte_stack() {
    let depth = calc_fml::MAX_CALL_DEPTH - 1;
    let outcome = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let mut book = Book::new();
            book.name("CountDown", "LAMBDA(n,IF(n<=0,0,1+CountDown(n-1)))");
            book.dynamic(&format!("=CountDown({})", depth - 1))
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(outcome, n((depth - 1) as f64));
}

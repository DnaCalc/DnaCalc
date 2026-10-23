//! L1 (dc-mzk): LET and LAMBDA, through `calc_fml::evaluate`.
//!
//! Ported from OxFml's evaluator tests (OxFml 15daac5, `crates/oxfml_core/tests/
//! evaluator_tests.rs`: `evaluator_runs_let_*`, `evaluator_runs_immediate_lambda_invocation`,
//! `evaluator_runs_helper_bound_lambda_*`, `evaluator_uses_lexical_not_dynamic_scope_*`,
//! `evaluator_executes_*_returned_lambda_*`, `evaluator_*_optional_bracket_parameter`,
//! `evaluator_matches_excel_builtin_colliding_let_recursive_name_frontier_ftc_0443`,
//! `evaluator_preserves_non_builtin_let_recursive_name_self_application`,
//! `evaluator_projects_direct_helper_local_self_recursion_as_name_error`, and
//! `ftc_1013_case_insensitive_shadowing_tests.rs`), and Excel 16.0 rows measured through COM on
//! 2026-10-23.

mod l1_support;

use calc_fml::{CompileError, FormulaForm};
use calc_func::value::WorksheetErrorCode as W;
use l1_support::{Book, b, e, n, numbers};

#[test]
fn let_binds_names_in_order() {
    let mut book = Book::new();
    book.number("A1", 5.0);
    assert_eq!(book.dynamic("=LET(x,1,x+2)"), n(3.0));
    assert_eq!(book.dynamic("=LET(x,2,y,x*3,x+y)"), n(8.0));
    assert_eq!(book.dynamic("=LET(r,A1,SUM(r,2))"), n(7.0));
    assert_eq!(book.dynamic("=LET(x,1,LET(x,2,x)+x)"), n(3.0));
    assert_eq!(book.dynamic("=LET(X,4,x*2)"), n(8.0));
    // The file form: Excel writes _xlfn.LET and _xlpm. before each name.
    assert_eq!(book.dynamic("=_xlfn.LET(_xlpm.x,3,_xlpm.x*2)"), n(6.0));
    assert_eq!(book.dynamic("=LET(a,{1,2,3},SUM(a*a))"), n(14.0));
    // A name that is no name, and an even number of arguments, do not compile.
    for refused in ["=LET(A1,1,A1)", "=LET(1,1,2)", "=LET(x,1)"] {
        assert!(
            matches!(
                book.eval_in(FormulaForm::Dynamic, refused, "Z100"),
                Err(CompileError::Syntax { .. } | CompileError::Arity { .. })
            ),
            "{refused}"
        );
    }
}

#[test]
fn lambdas_are_called_where_they_are_written_or_bound() {
    let mut book = Book::new();
    assert_eq!(book.dynamic("=LAMBDA(x,x+1)(2)"), n(3.0));
    assert_eq!(book.dynamic("=LAMBDA(200)()"), n(200.0));
    assert_eq!(book.dynamic("=LET(f,LAMBDA(x,x+1),f(2))"), n(3.0));
    assert_eq!(book.dynamic("=LET(f,LAMBDA(x,x^2),f(3))"), n(9.0));
    assert_eq!(book.dynamic("=LET(a,3,f,LAMBDA(x,x^2),f(a))"), n(9.0));
    // Lexical scope: f sees the x of its own LET.
    assert_eq!(
        book.dynamic("=LET(x,10,f,LAMBDA(y,x+y),LET(x,20,f(2)))"),
        n(12.0)
    );
    // A LAMBDA a LAMBDA returns keeps what it captured. (OxFml's rows name it `add5`, which is a
    // cell reference in Excel's grid, column ADD; Excel refuses it as a LET name.)
    assert_eq!(
        book.dynamic("=LET(adder,LAMBDA(n,LAMBDA(x,x+n)),addfive,adder(5),addfive(10))"),
        n(15.0)
    );
    assert_eq!(book.dynamic("=LAMBDA(n,LAMBDA(x,x+n))(5)(10)"), n(15.0));
    assert_eq!(
        book.dynamic(
            "=LET(base,100,adder,LAMBDA(n,LAMBDA(x,x+n+base)),addfive,adder(5),addfive(10))"
        ),
        n(115.0)
    );
    // A parameter shadows an outer name regardless of case (ftc-1013).
    assert_eq!(book.dynamic("=LET(N,4,LAMBDA(n,1/N)(2))"), n(0.5));
    assert_eq!(book.dynamic("=LET(N,4,LAMBDA(n,1/N)(0))"), e(W::Div0));
    // Lifting inside a body.
    assert_eq!(
        book.dynamic("=LAMBDA(v,v*2)({1,2})"),
        numbers(&[&[2.0, 4.0]])
    );
    // The file form.
    assert_eq!(
        book.dynamic("=_xlfn.LET(_xlpm.f,_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1),_xlpm.f(2))"),
        n(3.0)
    );
}

#[test]
fn optional_parameters_and_arity() {
    let mut book = Book::new();
    assert_eq!(
        book.dynamic("=LAMBDA(x,[y],IF(ISOMITTED(y),x*2,x+y))(5)"),
        n(10.0)
    );
    assert_eq!(
        book.dynamic("=LAMBDA(x,[y],IF(ISOMITTED(y),x*2,x+y))(5,3)"),
        n(8.0)
    );
    assert_eq!(
        book.dynamic("=LET(f,LAMBDA(x,[y],IF(ISOMITTED(y),x*2,x+y)),f(5))"),
        n(10.0)
    );
    assert_eq!(
        book.dynamic(
            "=_xlfn.LAMBDA(_xlpm.x,_xlop.y,_xlpm.x+IF(_xlfn.ISOMITTED(_xlpm.y),0,_xlpm.y))(1)"
        ),
        n(1.0)
    );
    assert_eq!(book.dynamic("=LAMBDA(a,b,ISOMITTED(b))(1,)"), b(true));
    assert_eq!(book.dynamic("=LAMBDA(a,ISOMITTED(a))(3)"), b(false));
    assert_eq!(book.dynamic("=ISOMITTED(1)"), b(false));
    // Excel 16.0: too few or too many arguments are #VALUE!.
    assert_eq!(book.dynamic("=LAMBDA(x,y,x)(1)"), e(W::Value));
    assert_eq!(book.dynamic("=LAMBDA(x,x)(1,2)"), e(W::Value));
}

#[test]
fn names_resolve_as_excel_resolves_them() {
    let mut book = Book::new();
    book.number("A1", 1.0);
    // A built-in function wins over a LET name of its spelling (Excel 16.0: 12, GCD's). Until
    // the catalogue holds GCD the call is refused as unknown, never resolved to the LET name.
    match book.eval_in(
        FormulaForm::Dynamic,
        "=LET(gcd,LAMBDA(a,b,a+b),gcd(48,36))",
        "Z100",
    ) {
        Ok(Ok(value)) => assert_eq!(value, n(12.0)),
        Err(CompileError::UnknownFunction { spelling, .. }) => assert_eq!(spelling, "gcd"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        book.dynamic(
            "=LET(zzgcd,LAMBDA(self,a,b,IF(b=0,a,self(self,b,MOD(a,b)))),zzgcd(zzgcd,48,36))"
        ),
        n(12.0)
    );
    // A LET name is not in scope inside its own value: F there is the call of a name no defined
    // name holds, which Excel gives #NAME? (OxFml's row) and which is refused here, since a VBA
    // or add-in function of that name is not ruled out.
    assert!(matches!(
        book.eval_in(
            FormulaForm::Dynamic,
            "=LET(F,LAMBDA(n,IF(n<=0,0,1+F(n-1))),F(5))",
            "Z100"
        ),
        Err(CompileError::UnknownFunction { .. })
    ));
    assert_eq!(
        book.dynamic("=LET(f,LAMBDA(self,n,IF(n<=0,0,1+self(self,n-1))),f(f,3))"),
        n(3.0)
    );
    // Excel 16.0: calling what is not a LAMBDA.
    assert_eq!(book.dynamic("=(A1)(2)"), e(W::Ref));
    assert_eq!(book.dynamic("=LET(x,1,x(2))"), e(W::Value));
    // A LAMBDA left as the result is the callable, #CALC! in a cell.
    assert_eq!(book.dynamic("=LAMBDA(x,x)").as_error(), Some(W::Calc));
}

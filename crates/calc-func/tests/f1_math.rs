#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the math and trigonometry functions ported from OxFunc, called through
//! `calc_func::invoke` as the formula evaluator calls them. The expected values are Excel's
//! documented results and the seed rows of OxFunc's tests (OxFunc 6fca3518, functions/).

mod f1_support;

use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Value};
use f1_support::*;

const PI: f64 = std::f64::consts::PI;

#[test]
fn circular_functions() {
    close(call("SIN", vec![n(1.0)]), 0.8414709848078965, 1e-15);
    exact(call("COS", vec![n(0.0)]), 1.0);
    close(call("TAN", vec![n(PI / 4.0)]), 1.0, 1e-15);
    // Excel's circular functions give #NUM! from 2^27 on.
    error(call("SIN", vec![n(134_217_728.0)]), Num);
    error(call("COS", vec![n(-134_217_728.0)]), Num);
    close(call("SIN", vec![n(134_217_727.0)]), (134_217_727.0f64).sin(), 1e-9);
    close(call("COT", vec![n(1.0)]), 1.0 / 1.0f64.tan(), 1e-15);
    error(call("COT", vec![n(0.0)]), Div0);
    close(call("SEC", vec![n(1.0)]), 1.0 / 1.0f64.cos(), 1e-15);
    close(call("CSC", vec![n(1.0)]), 1.0 / 1.0f64.sin(), 1e-15);
    error(call("CSC", vec![n(0.0)]), Div0);
}

#[test]
fn inverse_circular_functions() {
    close(call("ASIN", vec![n(0.5)]), PI / 6.0, 1e-15);
    error(call("ASIN", vec![n(2.0)]), Num);
    close(call("ACOS", vec![n(0.5)]), PI / 3.0, 1e-15);
    error(call("ACOS", vec![n(-1.5)]), Num);
    close(call("ATAN", vec![n(1.0)]), PI / 4.0, 1e-15);
    close(call("ATAN2", vec![n(1.0), n(1.0)]), PI / 4.0, 1e-15);
    close(call("ATAN2", vec![n(-1.0), n(0.0)]), PI, 1e-15);
    error(call("ATAN2", vec![n(0.0), n(0.0)]), Div0);
    close(call("ACOT", vec![n(1.0)]), PI / 4.0, 1e-15);
}

#[test]
fn hyperbolic_functions() {
    close(call("SINH", vec![n(1.0)]), 1.1752011936438014, 1e-15);
    close(call("COSH", vec![n(1.0)]), 1.5430806348152437, 1e-15);
    close(call("TANH", vec![n(0.5)]), 0.46211715726000974, 1e-15);
    exact(call("TANH", vec![n(800.0)]), 1.0);
    error(call("COSH", vec![n(1000.0)]), Num);
    close(call("ASINH", vec![n(1.0)]), 0.881373587019543, 1e-15);
    close(call("ACOSH", vec![n(2.0)]), 1.3169578969248166, 1e-15);
    error(call("ACOSH", vec![n(0.5)]), Num);
    close(call("ATANH", vec![n(0.5)]), 0.5493061443340548, 1e-15);
    error(call("ATANH", vec![n(1.0)]), Num);
    close(call("ACOTH", vec![n(2.0)]), 0.5493061443340548, 1e-15);
    close(call("COTH", vec![n(1.0)]), 1.3130352854993312, 1e-15);
    exact(call("COTH", vec![n(800.0)]), 1.0);
    close(call("SECH", vec![n(1.0)]), 0.6480542736638855, 1e-15);
    close(call("CSCH", vec![n(1.0)]), 0.8509181282393216, 1e-15);
}

#[test]
fn angles_and_pi() {
    exact(call("PI", vec![]), PI);
    close(call("DEGREES", vec![n(PI)]), 180.0, 1e-15);
    close(call("RADIANS", vec![n(180.0)]), PI, 1e-15);
    error(call("DEGREES", vec![n(1e307)]), Num);
}

#[test]
fn exponentials_logarithms_and_roots() {
    close(call("EXP", vec![n(1.0)]), std::f64::consts::E, 1e-15);
    error(call("EXP", vec![n(1000.0)]), Num);
    close(call("LN", vec![n(10.0)]), std::f64::consts::LN_10, 1e-15);
    error(call("LN", vec![n(0.0)]), Num);
    exact(call("LOG", vec![n(100.0)]), 2.0);
    close(call("LOG", vec![n(8.0), n(2.0)]), 3.0, 1e-15);
    exact(call("LOG10", vec![n(1000.0)]), 3.0);
    error(call("LOG10", vec![n(0.0)]), Num);
    exact(call("SQRT", vec![n(16.0)]), 4.0);
    error(call("SQRT", vec![n(-1.0)]), Num);
    close(call("SQRTPI", vec![n(1.0)]), 1.772453850905516, 1e-15);
    error(call("SQRTPI", vec![n(-1.0)]), Num);
}

#[test]
fn power_shares_the_operator_rule() {
    exact(call("POWER", vec![n(3.0), n(2.0)]), 9.0);
    error(call("POWER", vec![n(0.0), n(0.0)]), Num);
    error(call("POWER", vec![n(0.0), n(-1.0)]), Div0);
    error(call("POWER", vec![n(-8.0), n(0.5)]), Num);
    // The powers `^` refuses for want of a pinned power algorithm, POWER computes by OxFunc's
    // power kernel.
    exact(call("POWER", vec![n(2.0), n(3.0)]), 8.0);
    exact(call("POWER", vec![n(2.0), n(0.5)]), std::f64::consts::SQRT_2);
    exact(call("POWER", vec![n(-8.0), n(3.0)]), -512.0);
    exact(call("POWER", vec![n(10.0), n(-2.0)]), 0.01);
}

#[test]
fn factorials_and_combinatorics() {
    exact(call("FACT", vec![n(5.0)]), 120.0);
    exact(call("FACT", vec![n(5.9)]), 120.0);
    error(call("FACT", vec![n(171.0)]), Num);
    error(call("FACT", vec![n(-1.0)]), Num);
    exact(call("FACTDOUBLE", vec![n(7.0)]), 105.0);
    exact(call("COMBIN", vec![n(10.0), n(3.0)]), 120.0);
    error(call("COMBIN", vec![n(3.0), n(10.0)]), Num);
    exact(call("COMBINA", vec![n(4.0), n(3.0)]), 20.0);
    exact(call("PERMUT", vec![n(10.0), n(3.0)]), 720.0);
    exact(call("PERMUTATIONA", vec![n(3.0), n(2.0)]), 9.0);
    // Excel's own result, as OxFunc's machine witness rows pin it.
    exact(call("MULTINOMIAL", vec![n(2.0), n(3.0), n(4.0)]), 1_259.999_999_999_999);
    error(call("MULTINOMIAL", vec![n(-1.0)]), Num);
}

#[test]
fn gcd_and_lcm() {
    exact(call("GCD", vec![n(24.0), n(36.0)]), 12.0);
    exact(call("GCD", vec![n(5.0), n(2.0)]), 1.0);
    exact(call("LCM", vec![n(4.0), n(6.0)]), 12.0);
    exact(call("GCD", vec![arr(vec![vec![12.0, 18.0]]), n(30.0)]), 6.0);
    error(call("GCD", vec![n(-1.0)]), Num);
    error(call("GCD", vec![b(true)]), Value);
    error(call("GCD", vec![missing(), n(4.0)]), NA);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(8.0), n(12.0)]);
    let range = sheet.area(1, 1, 2, 1);
    exact(call_on(&mut sheet, "GCD", vec![range]), 4.0);
}

#[test]
fn numeral_systems() {
    text(call("ROMAN", vec![n(499.0)]), "CDXCIX");
    text(call("ROMAN", vec![n(499.0), n(1.0)]), "LDVLIV");
    text(call("ROMAN", vec![n(499.0), b(false)]), "ID");
    error(call("ROMAN", vec![n(4000.0)]), Value);
    exact(call("ARABIC", vec![t("MCMXII")]), 1912.0);
    exact(call("ARABIC", vec![t("")]), 0.0);
    error(call("ARABIC", vec![t("ABC")]), Value);
    text(call("BASE", vec![n(7.0), n(2.0)]), "111");
    text(call("BASE", vec![n(15.0), n(16.0), n(4.0)]), "000F");
    error(call("BASE", vec![n(7.0), n(1.0)]), Num);
    exact(call("DECIMAL", vec![t("FF"), n(16.0)]), 255.0);
    exact(call("DECIMAL", vec![t("111"), n(2.0)]), 7.0);
    error(call("DECIMAL", vec![t("FF"), n(10.0)]), Num);
}

#[test]
fn series_and_paired_sums() {
    exact(
        call("SERIESSUM", vec![n(2.0), n(0.0), n(1.0), arr(vec![vec![1.0, 2.0, 3.0]])]),
        17.0,
    );
    let x = arr(vec![vec![2.0, 3.0]]);
    let y = arr(vec![vec![1.0, 2.0]]);
    exact(call("SUMX2MY2", vec![x.clone(), y.clone()]), 8.0);
    exact(call("SUMX2PY2", vec![x.clone(), y.clone()]), 18.0);
    exact(call("SUMXMY2", vec![x.clone(), y]), 2.0);
    error(call("SUMXMY2", vec![x, arr(vec![vec![1.0]])]), NA);
}

#[test]
fn matrix_functions() {
    close(call("MDETERM", vec![arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]])]), -2.0, 1e-15);
    let (rows, cols, cells) = array(call("MINVERSE", vec![arr(vec![vec![4.0, 7.0], vec![2.0, 6.0]])]));
    assert_eq!((rows, cols), (2, 2));
    for (got, want) in cells.iter().zip([0.6, -0.7, -0.2, 0.4]) {
        assert!((got - want).abs() < 1e-15, "{got} vs {want}");
    }
    error(call("MINVERSE", vec![arr(vec![vec![1.0, 2.0], vec![2.0, 4.0]])]), Num);
    let (rows, cols, cells) = array(call(
        "MMULT",
        vec![arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]]), arr(vec![vec![5.0], vec![6.0]])],
    ));
    assert_eq!((rows, cols, cells), (2, 1, vec![17.0, 39.0]));
    error(call("MMULT", vec![arr(vec![vec![1.0, 2.0]]), arr(vec![vec![1.0, 2.0]])]), Value);
    let (rows, cols, cells) = array(call("MUNIT", vec![n(2.0)]));
    assert_eq!((rows, cols, cells), (2, 2, vec![1.0, 0.0, 0.0, 1.0]));
    error(call("MUNIT", vec![n(0.0)]), Value);
}

#[test]
fn arguments_are_read_by_worksheet_coercion() {
    close(call("SIN", vec![t("1")]), 0.8414709848078965, 1e-15);
    exact(call("SQRT", vec![b(true)]), 1.0);
    error(call("SQRT", vec![t("abc")]), Value);
    error(call("LN", vec![e(Div0)]), Div0);
    // An array given to a function of single values lifts over it, which is not modelled.
    refused(call("SIN", vec![arr(vec![vec![1.0, 2.0]])]));
}

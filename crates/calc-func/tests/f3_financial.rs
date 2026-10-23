#![allow(clippy::excessive_precision)]

//! F3 (dc-728): the financial functions ported from OxFunc, called through `calc_func::invoke`
//! as the formula evaluator calls them. Expected values are Microsoft's documented examples
//! (rounded as the documentation shows them, so compared within that rounding) or OxFunc's tests
//! (OxFunc 6fca3518, functions/).

mod f1_support;

use calc_func::locale::CalcEnvironment;
use calc_func::locale::DateSystem::{System1900, System1904};
use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Value};
use f1_support::*;

/// `spelling` in a 1900-system workbook.
fn dated(spelling: &str, args: Vec<calc_func::value::CalcValue>) -> Result<calc_func::value::CalcValue, calc_func::FunctionFault> {
    call_in(&CalcEnvironment::dates(System1900), None, None, spelling, args)
}

#[test]
fn time_value_of_money() {
    close(call("PMT", vec![n(0.08 / 12.0), n(10.0), n(10000.0)]), -1037.032089, 1e-9);
    close(call("PMT", vec![n(0.06 / 12.0), n(18.0 * 12.0), n(0.0), n(50000.0)]), -129.0811609, 1e-9);
    close(call("PV", vec![n(0.08 / 12.0), n(240.0), n(500.0), n(0.0), n(0.0)]), -59777.14585, 1e-9);
    close(call("FV", vec![n(0.06 / 12.0), n(10.0), n(-200.0), n(-500.0), n(1.0)]), 2581.403374, 1e-9);
    close(call("FV", vec![n(0.12 / 12.0), n(12.0), n(-1000.0)]), 12682.50301, 1e-9);
    close(call("NPER", vec![n(0.12 / 12.0), n(-100.0), n(-1000.0), n(10000.0), n(1.0)]), 59.6738657, 1e-8);
    close(call("RATE", vec![n(48.0), n(-200.0), n(8000.0)]), 0.00770147, 1e-5);
    close(call("IPMT", vec![n(0.1 / 12.0), n(1.0), n(36.0), n(8000.0)]), -66.66666667, 1e-9);
    close(call("PPMT", vec![n(0.1 / 12.0), n(1.0), n(24.0), n(2000.0)]), -75.62318601, 1e-9);
    close(call("ISPMT", vec![n(0.1 / 12.0), n(1.0), n(36.0), n(8000000.0)]), -64814.8148, 1e-9);
    close(call("PDURATION", vec![n(0.025), n(2000.0), n(2200.0)]), 3.859866163, 1e-9);
    close(call("RRI", vec![n(96.0), n(10000.0), n(11000.0)]), 0.0009933, 1e-4);
    close(call("NOMINAL", vec![n(0.053543), n(4.0)]), 0.05250032, 1e-7);
    close(call("EFFECT", vec![n(0.0525), n(4.0)]), 0.053542667, 1e-8);
    // The payment type is any number truncated to non-zero; an omitted fv is 0.
    close(call("PMT", vec![n(0.01), n(12.0), n(1000.0), missing(), n(1.0)]), -87.96909770, 1e-9);
    error(call("NOMINAL", vec![n(0.0), n(4.0)]), Num);
    error(call("PDURATION", vec![n(0.0), n(1.0), n(2.0)]), Num);
    error(call("PMT", vec![n(0.1), n(0.0), n(1000.0)]), Num);
}

#[test]
fn cash_flows() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(-10000.0), n(3000.0), n(4200.0), n(6800.0), t("x"), b(true)]);
    let range = sheet.area(1, 1, 6, 1);
    close(
        call("NPV", vec![n(0.1), n(-10000.0), n(3000.0), n(4200.0), n(6800.0)]),
        1188.443412,
        1e-9,
    );
    // A reference's text and logicals are skipped; direct arguments are coerced.
    close(
        call_on(&mut sheet, "NPV", vec![n(0.1), range]),
        1188.443412,
        1e-9,
    );
    close(
        call(
            "MIRR",
            vec![col(&[-120000.0, 39000.0, 30000.0, 21000.0, 37000.0, 46000.0]), n(0.1), n(0.12)],
        ),
        0.126094,
        1e-5,
    );
    close(call("FVSCHEDULE", vec![n(1.0), arr(vec![vec![0.09, 0.11, 0.1]])]), 1.33089, 1e-12);
    close(
        call("IRR", vec![col(&[-70000.0, 12000.0, 15000.0, 18000.0, 21000.0, 26000.0])]),
        0.086630948,
        1e-7,
    );
    close(
        call("IRR", vec![col(&[-70000.0, 12000.0, 15000.0]), n(-0.1)]),
        -0.443506941,
        1e-7,
    );
    error(call("IRR", vec![col(&[1.0, 2.0])]), Num);
    let values = col(&[-10000.0, 2750.0, 4250.0, 3250.0, 2750.0]);
    let dates = col(&[39448.0, 39508.0, 39751.0, 39859.0, 39904.0]);
    close(call("XNPV", vec![n(0.09), values.clone(), dates.clone()]), 2086.647602, 1e-9);
    close(call("XIRR", vec![values.clone(), dates.clone()]), 0.373362535, 1e-7);
    error(call("XNPV", vec![n(0.09), values, col(&[39448.0, 39508.0])]), Num);
    close(
        call("CUMIPMT", vec![n(0.09 / 12.0), n(360.0), n(125000.0), n(13.0), n(24.0), n(0.0)]),
        -11135.23213,
        1e-9,
    );
    close(
        call("CUMPRINC", vec![n(0.09 / 12.0), n(360.0), n(125000.0), n(13.0), n(24.0), n(0.0)]),
        -934.1071234,
        1e-9,
    );
    error(
        call("CUMIPMT", vec![n(0.09 / 12.0), n(360.0), n(125000.0), n(13.0), n(24.0), n(2.0)]),
        Num,
    );
}

#[test]
fn depreciation() {
    exact(call("SLN", vec![n(30000.0), n(7500.0), n(10.0)]), 2250.0);
    error(call("SLN", vec![n(30000.0), n(7500.0), n(0.0)]), Div0);
    close(call("SYD", vec![n(30000.0), n(7500.0), n(10.0), n(1.0)]), 4090.909091, 1e-9);
    close(call("DB", vec![n(1000000.0), n(100000.0), n(6.0), n(1.0), n(7.0)]), 186083.3333, 1e-9);
    close(call("DB", vec![n(1000000.0), n(100000.0), n(6.0), n(7.0), n(7.0)]), 15845.10, 1e-6);
    close(call("DDB", vec![n(2400.0), n(300.0), n(3650.0), n(1.0)]), 1.315068493, 1e-9);
    close(call("DDB", vec![n(2400.0), n(300.0), n(10.0), n(2.0)]), 384.0, 1e-12);
    close(call("VDB", vec![n(2400.0), n(300.0), n(3650.0), n(0.0), n(1.0)]), 1.315068493, 1e-9);
    close(call("VDB", vec![n(2400.0), n(300.0), n(120.0), n(6.0), n(18.0), n(1.5)]), 311.81, 1e-4);
    close(call("VDB", vec![n(2400.0), n(300.0), n(120.0), n(6.0), n(18.0)]), 396.31, 1e-5);
}

#[test]
fn amortisation_needs_the_date_system() {
    let args = || vec![n(2400.0), n(39679.0), n(39813.0), n(300.0), n(1.0), n(0.15), n(1.0)];
    exact(dated("AMORDEGRC", args()), 776.0);
    exact(dated("AMORLINC", args()), 360.0);
    // Without a workbook date system the call is refused, not given a 1900-system answer.
    refused(call("AMORLINC", args()));
    // The same dates in a 1904 workbook are 1462 lower.
    let shifted = vec![n(2400.0), n(39679.0 - 1462.0), n(39813.0 - 1462.0), n(300.0), n(1.0), n(0.15), n(1.0)];
    exact(call_in(&CalcEnvironment::dates(System1904), None, None, "AMORLINC", shifted), 360.0);
}

#[test]
fn dollar_fractions() {
    close(call("DOLLARDE", vec![n(1.02), n(16.0)]), 1.125, 1e-15);
    close(call("DOLLARDE", vec![n(1.1), n(32.0)]), 1.3125, 1e-15);
    close(call("DOLLARFR", vec![n(1.125), n(16.0)]), 1.02, 1e-15);
    close(call("DOLLARFR", vec![n(1.125), n(32.0)]), 1.04, 1e-15);
    // OxFunc: DOLLARDE(3.69, 10) keeps the fraction, as the denominator is a power of ten.
    close(call("DOLLARDE", vec![n(3.69), n(10.0)]), 3.69, 1e-15);
    error(call("DOLLARDE", vec![n(1.02), n(0.0)]), Div0);
    error(call("DOLLARDE", vec![n(1.02), n(-1.0)]), Num);
    error(call("DOLLARDE", vec![b(true), n(16.0)]), Value);
    error(call("DOLLARFR", vec![n(1.0), missing()]), NA);
}

#[test]
fn bonds_and_coupons() {
    close(
        dated("PRICE", vec![n(39493.0), n(43054.0), n(0.0575), n(0.065), n(100.0), n(2.0), n(0.0)]),
        94.63436,
        1e-6,
    );
    close(
        dated("YIELD", vec![n(39493.0), n(42689.0), n(0.0575), n(95.04287), n(100.0), n(2.0), n(0.0)]),
        0.065,
        1e-6,
    );
    let coupon = |name: &str| dated(name, vec![n(40568.0), n(40862.0), n(2.0), n(1.0)]);
    exact(coupon("COUPDAYBS"), 71.0);
    exact(coupon("COUPDAYS"), 181.0);
    exact(coupon("COUPDAYSNC"), 110.0);
    exact(coupon("COUPNCD"), 40678.0);
    exact(coupon("COUPNUM"), 2.0);
    exact(coupon("COUPPCD"), 40497.0);
    // In a 1904 workbook the dates, and the returned date, are 1462 lower.
    exact(
        call_in(&CalcEnvironment::dates(System1904), None, None, "COUPNCD", vec![n(40568.0 - 1462.0), n(40862.0 - 1462.0), n(2.0), n(1.0)]),
        40678.0 - 1462.0,
    );
    refused(call("COUPNUM", vec![n(40568.0), n(40862.0), n(2.0), n(1.0)]));
    error(dated("COUPNUM", vec![n(40568.0), n(40862.0), n(3.0), n(1.0)]), Num);
    error(dated("COUPNUM", vec![n(40862.0), n(40568.0), n(2.0), n(1.0)]), Num);
    close(
        dated("ACCRINT", vec![n(39508.0), n(39691.0), n(39569.0), n(0.1), n(1000.0), n(2.0), n(0.0)]),
        16.66666667,
        1e-9,
    );
    close(
        dated("ACCRINTM", vec![n(39539.0), n(39614.0), n(0.1), n(1000.0), n(3.0)]),
        20.54794521,
        1e-9,
    );
    close(
        dated("PRICEMAT", vec![n(39493.0), n(39551.0), n(39397.0), n(0.061), n(0.061), n(0.0)]),
        99.98449888,
        1e-9,
    );
    close(
        dated("YIELDMAT", vec![n(39522.0), n(39755.0), n(39394.0), n(0.0625), n(100.0123), n(0.0)]),
        0.060954,
        1e-5,
    );
    close(
        dated("YIELDDISC", vec![n(39494.0), n(39508.0), n(99.795), n(100.0), n(2.0)]),
        0.052823,
        1e-5,
    );
}

#[test]
fn discounted_securities_and_bills() {
    close(
        dated("PRICEDISC", vec![n(39494.0), n(39508.0), n(0.0525), n(100.0), n(2.0)]),
        99.79583333,
        1e-9,
    );
    close(
        dated("INTRATE", vec![n(39493.0), n(39583.0), n(1000000.0), n(1014420.0), n(2.0)]),
        0.05768,
        1e-4,
    );
    close(
        dated("RECEIVED", vec![n(39493.0), n(39583.0), n(1000000.0), n(0.0575), n(2.0)]),
        1014584.654,
        1e-9,
    );
    close(
        dated("DISC", vec![n(43282.0), n(54058.0), n(97.975), n(100.0), n(1.0)]),
        0.000686,
        1e-2,
    );
    close(dated("TBILLPRICE", vec![n(39538.0), n(39600.0), n(0.09)]), 98.45, 1e-9);
    close(dated("TBILLYIELD", vec![n(39538.0), n(39600.0), n(98.45)]), 0.091417, 1e-4);
    close(dated("TBILLEQ", vec![n(39538.0), n(39600.0), n(0.0914)]), 0.094151, 1e-5);
}

#[test]
fn odd_period_bonds() {
    close(
        dated(
            "ODDFPRICE",
            vec![n(39763.0), n(44256.0), n(39736.0), n(39873.0), n(0.0785), n(0.0625), n(100.0), n(2.0), n(1.0)],
        ),
        113.597717,
        1e-8,
    );
    close(
        dated(
            "ODDFYIELD",
            vec![n(39763.0), n(44256.0), n(39736.0), n(39873.0), n(0.0575), n(84.5), n(100.0), n(2.0), n(0.0)],
        ),
        0.0772,
        1e-3,
    );
    close(
        dated(
            "ODDLPRICE",
            vec![n(39485.0), n(39614.0), n(39370.0), n(0.0375), n(0.0405), n(100.0), n(2.0), n(0.0)],
        ),
        99.87829,
        1e-7,
    );
    close(
        dated(
            "ODDLYIELD",
            vec![n(39558.0), n(39614.0), n(39440.0), n(0.0375), n(99.875), n(100.0), n(2.0), n(0.0)],
        ),
        0.045192,
        1e-4,
    );
}

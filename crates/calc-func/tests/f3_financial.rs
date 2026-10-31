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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

/// Excel 16.0 build 20228, answers-irr-exact-graph-discovery-20260809.json,
/// d16-root / d16-far_p010 and d11-root. These run through the application call path.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn irr_near_zero_plateau_terminates_and_nonconvergence_is_num() {
    let flows = col(&[-1000.0, 999.999999, 1.1e-5]);
    for guess in [f64::from_bits(0x3e45798eddbe9730), 0.1] {
        exact(call("IRR", vec![flows.clone(), n(guess)]), f64::from_bits(0x3e45798ee0000000));
    }
    error(
        call("IRR", vec![
            col(&[
                f64::from_bits(0xdf138d352e5096af),
                f64::from_bits(0x5eef485516e7577f),
                f64::from_bits(0x5ef7763fd12d819f),
                f64::from_bits(0x5f0b5f4a740a6c8f),
            ]),
            n(f64::from_bits(0x3fb4413a15562ce0)),
        ]),
        Num,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

/// Recorded IRR discovery row d12-far_n025: a nonzero tiny derivative still polishes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn irr_polishes_cashflows_with_a_small_nonzero_derivative() {
    let flows = col(&[
        f64::from_bits(0xa0ca_2fe7_6a3f_9475),
        f64::from_bits(0x20aa_2fe7_6a3f_9475),
        f64::from_bits(0x20b1_0589_9ea9_53b2),
        f64::from_bits(0x20c1_ad22_9aea_ea9c),
    ]);
    exact(call("IRR", vec![flows, n(-0.25)]), f64::from_bits(0x3fb9_f899_7cbe_f010));
}

/// Recorded interest-only balances avoid cancellation and do not depend on the period count.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn pmt_equal_opposite_balances_are_interest_only() {
    // Excel build 20131, oc28619 and oc28633: the same balance at one and four periods.
    for periods in [1.0, 4.0] {
        exact(call("PMT", vec![n(f64::from_bits(0xbeb9_9d33_5565_e403)),
            n(periods), n(1.0), n(-1.0), n(0.0)]),
            f64::from_bits(0x3eb9_9d33_5565_e403));
    }
    // oc14794 covers the discount arm; oc107780 distinguishes timing order.
    exact(call("PMT", vec![n(f64::from_bits(0x3eb2_eee1_6da1_925e)),
        n(1.0), n(1.0), n(-1.0), n(0.0)]), f64::from_bits(0xbeb2_eee1_6da1_925e));
    exact(call("PMT", vec![n(0.05), n(12.0), n(1.0), n(-1.0), n(1.0)]),
        f64::from_bits(0xbfa8_6186_1861_8618));
}

/// Mathematical overflow controls, not additional Excel readings.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn pmt_interest_only_keeps_a_finite_factored_payment() {
    let rate = f64::from_bits(0x7fe8_0458_d015_75b0);
    for periods in [1.0, 4.0] {
        // For beginning payments, MAX*rate/(1+rate) rounds to MAX: its
        // exact difference from MAX is less than two, far below half an ULP.
        exact(call("PMT", vec![n(rate), n(periods), n(f64::MAX), n(-f64::MAX), n(1.0)]),
            -f64::MAX);
        // End payments have no timing divisor; this product really overflows.
        error(call("PMT", vec![n(rate), n(periods), n(f64::MAX), n(-f64::MAX), n(0.0)]), Num);
    }
}

/// Recorded PMT stores in the exponential correction and timing division.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn pmt_uses_recorded_exponential_and_timing_stores() {
    // Excel build 20131 PMT bank, oc72460 and oc36619: the growth arm's
    // first quotient is stored from PC64, with and without a future value.
    exact(call("PMT", vec![
        n(f64::from_bits(0xbf4a_dfce_e2c3_cdfe)), n(96.0), n(97.0),
    ]), f64::from_bits(0xbfef_105d_90b7_fd3b));
    exact(call("PMT", vec![
        n(f64::from_bits(0xbf71_2e25_12f6_bd04)), n(360.0), n(1.0), n(1.0),
    ]), f64::from_bits(0xbf7a_e23b_ddd2_dba3));
    // oc1134: the discount arm stores the final PC64 product.
    exact(call("PMT", vec![
        n(f64::from_bits(0x3eb6_b288_9103_0aed)), n(256.0), n(1.0),
    ]), f64::from_bits(0xbf70_00b6_4c87_af22));
    // oc30542: the growth arm retains the native product.
    exact(call("PMT", vec![
        n(f64::from_bits(0xbee8_ffdb_60af_1ab5)), n(192.0), n(1.0), n(-1000.0), n(1.0),
    ]), f64::from_bits(0x4014_d624_c57a_7851));
    exact(call("PMT", vec![n(f64::from_bits(0x3e70000000000000)), n(f64::from_bits(0x4018000000000000)), n(f64::from_bits(0x3ff0000000000000)), n(f64::from_bits(0x0000000000000000)), n(f64::from_bits(0x0000000000000000))]), f64::from_bits(0xbfc55555a000003d));
    exact(call("PMT", vec![n(f64::from_bits(0x3e70000000000000)), n(f64::from_bits(0x4018000000000000)), n(f64::from_bits(0x3ff0000000000001)), n(f64::from_bits(0x0000000000000000)), n(f64::from_bits(0x0000000000000000))]), f64::from_bits(0xbfc55555a000003f));
}

/// Recorded d00-m46 distinguishes q-space publication; irr-B-g0.0 needs
/// the equal-residual Newton step when the initial secant points coincide.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn irr_two_cashflows_publish_the_q_fixpoint() {
    exact(call("IRR", vec![
        col(&[f64::from_bits(0xc08f_2800_0000_0000), f64::from_bits(0x4091_3c00_0000_0000)]),
        n(f64::from_bits(0x3fba_f7b8_1ab4_40bd)),
    ]), f64::from_bits(0x3fbb_37b8_1ab4_40c0));
    exact(call("IRR", vec![col(&[-1000.0, 1210.0]), n(0.0)]),
        f64::from_bits(0x3fca_e147_ae14_7ae0));
}

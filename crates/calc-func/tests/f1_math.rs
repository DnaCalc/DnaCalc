#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the math and trigonometry functions ported from OxFunc, called through
//! `calc_func::invoke` as the formula evaluator calls them. The expected values are Excel's
//! documented results and the seed rows of OxFunc's tests (OxFunc 6fca3518, functions/).

mod f1_support;

use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Value};
use f1_support::*;

const PI: f64 = std::f64::consts::PI;

/// Excel 20430 integer-threshold bank: neighbors and signs around two binade boundaries.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn int_negative_fraction_adjustment_rounds_before_truncation() {
    for (input, expected) in [
        (0xc30f_ffff_ffff_fffe, 0xc310_0000_0000_0000),
        (0xc30f_ffff_ffff_ffff, 0xc310_0000_0000_0004),
        (0xc310_0000_0000_0000, 0xc310_0000_0000_0000),
        (0xc310_0000_0000_0001, 0xc310_0000_0000_0004),
        (0xc31f_ffff_ffff_fffe, 0xc320_0000_0000_0000),
        (0xc31f_ffff_ffff_ffff, 0xc320_0000_0000_0002),
        (0xc320_0000_0000_0000, 0xc320_0000_0000_0000),
        (0x430f_ffff_ffff_ffff, 0x430f_ffff_ffff_fff8),
    ] {
        exact(
            call("INT", vec![n(f64::from_bits(input))]),
            f64::from_bits(expected),
        );
    }
}

/// Excel 20430's W111 binomial inverse boundaries, also exercised through the compatibility alias.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_inverse_advances_on_measured_equal_boundaries() {
    for function in ["BINOM.INV", "CRITBINOM"] {
        for (trials, alpha, expected) in [(1.0, 0.5, 1.0), (2.0, 0.25, 1.0), (7.0, 0.5, 4.0)] {
            exact(call(function, vec![n(trials), n(0.5), n(alpha)]), expected);
        }
    }
    // The W111 control near probability one remains in the lower count.
    exact(
        call(
            "BINOM.INV",
            vec![
                n(2.0),
                n(f64::from_bits(0x3fef_ffff_ffff_ffff)),
                n(f64::from_bits(0x3cb0_0000_0000_0000)),
            ],
        ),
        1.0,
    );
}

/// Touchstone Excel 16.0.20625 rejects trial counts at and above the signed 32-bit sentinel.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_inverse_rejects_out_of_range_trials() {
    for function in ["BINOM.INV", "CRITBINOM"] {
        for trials in [2_147_483_647.0, 9_007_199_254_740_992.0, 1e100] {
            error(call(function, vec![n(trials), n(1e-9), n(0.5)]), Num);
        }
    }
}

/// Touchstone Excel 16.0.20625 controls at the largest admitted trial count.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_inverse_searches_large_admitted_trial_counts() {
    for function in ["BINOM.INV", "CRITBINOM"] {
        for (p, alpha, expected) in [
            (0.5, 0.5, 1_073_741_823.0),
            (0.25, 0.75, 536_884_446.0),
            (1e-9, 0.5, 2.0),
        ] {
            exact(
                call(function, vec![n(2_147_483_646.75), n(p), n(alpha)]),
                expected,
            );
        }
    }
}

/// Excel20527: the directed unit at10^126 rounds through PC64, then binary64.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn roundup_publishes_its_power_of_ten_unit_from_pc64() {
    for sign in [1.0, -1.0] {
        exact(
            call("ROUNDUP", vec![n(sign), n(-126.0)]),
            sign * f64::from_bits(0x5a17_a2ec_c414_a040),
        );
        exact(call("ROUNDUP", vec![n(sign), n(-125.0)]), sign * 1e125);
        exact(call("ROUNDUP", vec![n(sign), n(-127.0)]), sign * 1e127);
    }
}

/// Fresh Excel20527 controls distinguish normalized radix16 scaling from both
/// direct decimal conversion and an integer coefficient padded to15 digits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rounding_publishes_normalized_decimal_coefficients_in_radix16_steps() {
    for function in ["ROUND", "ROUNDUP", "ROUNDDOWN", "TRUNC"] {
        for (input, expected) in [
            (0x00cf_63b0_3545_19d2u64, 0x00cf_63b0_3545_19d1),
            (0x123f_0cf4_0240_a183, 0x123f_0cf4_0240_a182),
            (0x80cf_63b0_3545_19d2, 0x80cf_63b0_3545_19d1),
        ] {
            exact(
                call(function, vec![n(f64::from_bits(input)), n(32000.0)]),
                f64::from_bits(expected),
            );
        }
    }
}

/// Original Excel20430 observations oc1418, oc1065 and oc1762.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn asinh_stores_each_identity_operation_from_pc64() {
    for (input, expected) in [
        (0x3e73_4f4b_587c_834b, 0x3e73_4f4b_5859_1206),
        (0x3ed0_51e2_e324_9a58, 0x3ed0_51e2_e326_b736),
        (0xbeb0_32f7_6bce_6f24, 0xbeb0_32f7_6bdb_73b3),
    ] {
        exact(
            call("ASINH", vec![n(f64::from_bits(input))]),
            f64::from_bits(expected),
        );
    }
}

/// Every subnormal has x*x rounding to zero and 1+abs(x) rounding to one.
/// Both signs therefore reach LN(1), whose published result is positive zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn asinh_admits_subnormal_inputs_whose_identity_is_exact_zero() {
    for input in [
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x000f_ffff_ffff_ffff,
        0x800f_ffff_ffff_ffff,
    ] {
        exact(call("ASINH", vec![n(f64::from_bits(input))]), 0.0);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn circular_functions() {
    close(call("SIN", vec![n(1.0)]), 0.8414709848078965, 1e-15);
    exact(call("COS", vec![n(0.0)]), 1.0);
    close(call("TAN", vec![n(PI / 4.0)]), 1.0, 1e-15);
    // Excel's circular functions give #NUM! from 2^27 on.
    error(call("SIN", vec![n(134_217_728.0)]), Num);
    error(call("COS", vec![n(-134_217_728.0)]), Num);
    close(
        call("SIN", vec![n(134_217_727.0)]),
        (134_217_727.0f64).sin(),
        1e-9,
    );
    close(call("COT", vec![n(1.0)]), 1.0 / 1.0f64.tan(), 1e-15);
    error(call("COT", vec![n(0.0)]), Div0);
    close(call("SEC", vec![n(1.0)]), 1.0 / 1.0f64.cos(), 1e-15);
    close(call("CSC", vec![n(1.0)]), 1.0 / 1.0f64.sin(), 1e-15);
    error(call("CSC", vec![n(0.0)]), Div0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

/// Recorded build-20228 admission witnesses: the ported kernels implement DAZ,
/// and the catalogue must let those inputs reach them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn measured_subnormal_admission_reaches_the_kernel() {
    for bits in [1, 0x8000_0000_0000_0001] {
        exact(call("ATANH", vec![n(f64::from_bits(bits))]), 0.0);
        exact(call("COMBIN", vec![n(f64::from_bits(bits)), n(0.0)]), 1.0);
        exact(call("COMBIN", vec![n(1.0), n(f64::from_bits(bits))]), 1.0);
        exact(call("COMBINA", vec![n(1.0), n(f64::from_bits(bits))]), 1.0);
    }
    error(call("ATANH", vec![t("not a number")]), Value);
    error(call("COMBIN", vec![n(-f64::MIN_POSITIVE), n(0.0)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn angles_and_pi() {
    exact(call("PI", vec![]), PI);
    close(call("DEGREES", vec![n(PI)]), 180.0, 1e-15);
    close(call("RADIANS", vec![n(180.0)]), PI, 1e-15);
    error(call("DEGREES", vec![n(1e307)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn power_shares_the_operator_rule() {
    exact(call("POWER", vec![n(3.0), n(2.0)]), 9.0);
    assert_excel_capture(
        call("POWER", vec![n(0.0), n(0.0)]),
        include_bytes!("captures/catalogue/sections/f1_power.capture.json"),
        "f1_power",
        2,
        4,
    );
    assert_excel_capture(
        call("POWER", vec![n(0.0), n(-1.0)]),
        include_bytes!("captures/catalogue/sections/f1_power.capture.json"),
        "f1_power",
        4,
        4,
    );
    error(call("POWER", vec![n(-8.0), n(0.5)]), Num);
    // The powers that need a power algorithm, `^` and POWER compute by OxFunc's power kernel.
    assert_excel_capture(
        call("POWER", vec![n(2.0), n(3.0)]),
        include_bytes!("captures/catalogue/sections/f1_power.capture.json"),
        "f1_power",
        7,
        4,
    );
    exact(
        call("POWER", vec![n(2.0), n(0.5)]),
        std::f64::consts::SQRT_2,
    );
    exact(call("POWER", vec![n(-8.0), n(3.0)]), -512.0);
    assert_excel_capture(
        call("POWER", vec![n(10.0), n(-2.0)]),
        include_bytes!("captures/catalogue/sections/f1_power.capture.json"),
        "f1_power",
        11,
        4,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn factorials_and_combinatorics() {
    assert_excel_capture(
        call("FACT", vec![n(5.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        81,
        1,
    );
    exact(call("FACT", vec![n(5.9)]), 120.0);
    error(call("FACT", vec![n(171.0)]), Num);
    error(call("FACT", vec![n(-1.0)]), Num);
    assert_excel_capture(
        call("FACTDOUBLE", vec![n(7.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        82,
        1,
    );
    exact(call("COMBIN", vec![n(10.0), n(3.0)]), 120.0);
    error(call("COMBIN", vec![n(3.0), n(10.0)]), Num);
    assert_excel_capture(
        call("COMBINA", vec![n(4.0), n(3.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        45,
        1,
    );
    exact(call("PERMUT", vec![n(10.0), n(3.0)]), 720.0);
    assert_excel_capture(
        call("PERMUTATIONA", vec![n(3.0), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        180,
        1,
    );
    // Excel's own result, as OxFunc's machine witness rows pin it.
    exact(
        call("MULTINOMIAL", vec![n(2.0), n(3.0), n(4.0)]),
        1_259.999_999_999_999,
    );
    error(call("MULTINOMIAL", vec![n(-1.0)]), Num);
    for args in [
        vec![n(0.0), n(1e100)],
        vec![n(171.0)],
        vec![n(170.0), n(1.0)],
    ] {
        error(call("MULTINOMIAL", args), Num);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn multinomial_distinguishes_blank_cells_from_blank_ranges() {
    let mut sheet = Sheet::new();
    let blank = sheet.area(1, 1, 1, 1);
    let blank2 = sheet.area(1, 2, 1, 2);
    let range = sheet.area(1, 1, 2, 1);
    error(call_on(&mut sheet, "MULTINOMIAL", vec![blank.clone()]), Value);
    error(call_on(&mut sheet, "MULTINOMIAL", vec![blank.clone(), blank2]), Value);
    exact(call_on(&mut sheet, "MULTINOMIAL", vec![range.clone()]), 1.0);
    exact(call_on(&mut sheet, "MULTINOMIAL", vec![blank.clone(), range]), 1.0);
    exact(call_on(&mut sheet, "MULTINOMIAL", vec![blank.clone(), n(0.0)]), 1.0);
    error(call_on(&mut sheet, "MULTINOMIAL", vec![blank, e(Div0)]), Div0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gcd_and_lcm() {
    exact(call("GCD", vec![n(24.0), n(36.0)]), 12.0);
    exact(call("GCD", vec![n(5.0), n(2.0)]), 1.0);
    assert_excel_capture(
        call("LCM", vec![n(4.0), n(6.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        140,
        1,
    );
    exact(call("GCD", vec![arr(vec![vec![12.0, 18.0]]), n(30.0)]), 6.0);
    error(call("GCD", vec![n(-1.0)]), Num);
    assert_excel_capture(
        call("GCD", vec![b(true)]),
        include_bytes!("captures/jnt/sections/jnt_w111_rounding.capture.json"),
        "jnt_w111_rounding",
        5754,
        4,
    );
    error(call("GCD", vec![missing(), n(4.0)]), NA);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(8.0), n(12.0)]);
    let range = sheet.area(1, 1, 2, 1);
    exact(call_on(&mut sheet, "GCD", vec![range]), 4.0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn numeral_systems() {
    assert_excel_capture(
        call("ROMAN", vec![n(499.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        197,
        1,
    );
    text(call("ROMAN", vec![n(499.0), n(1.0)]), "LDVLIV");
    text(call("ROMAN", vec![n(499.0), b(false)]), "ID");
    error(call("ROMAN", vec![n(4000.0)]), Value);
    exact(call("ARABIC", vec![t("MCMXII")]), 1912.0);
    exact(call("ARABIC", vec![t("")]), 0.0);
    error(call("ARABIC", vec![t("ABC")]), Value);
    assert_excel_capture(
        call("BASE", vec![n(7.0), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        13,
        1,
    );
    text(call("BASE", vec![n(15.0), n(16.0), n(4.0)]), "000F");
    error(call("BASE", vec![n(7.0), n(1.0)]), Num);
    assert_excel_capture(
        call("DECIMAL", vec![t("FF"), n(16.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        65,
        1,
    );
    exact(call("DECIMAL", vec![t("111"), n(2.0)]), 7.0);
    error(call("DECIMAL", vec![t("FF"), n(10.0)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn series_and_paired_sums() {
    exact(
        call(
            "SERIESSUM",
            vec![
                n(f64::from_bits(0x4084_daa4_0643_1804)),
                n(-0.5),
                n(10.0),
                n(f64::from_bits(0x3929_867d_9c7b_0918)),
            ],
        ),
        f64::from_bits(0x38df_9e86_63cc_7c72),
    );
    // Excel 20527: powers are staged before coefficient multiplication.
    exact(
        call("SERIESSUM", vec![n(-2.0), n(0.5), n(1.0), n(1.0)]),
        0.0,
    );
    exact(
        call("SERIESSUM", vec![n(1e-10), n(32.0), n(1.0), n(1e300)]),
        0.0,
    );
    exact(call("SERIESSUM", vec![n(2.0), n(1.0), n(1.0), t("1")]), 2.0);
    error(call("SERIESSUM", vec![n(0.0), n(0.0), n(1.0), n(0.0)]), Num);

    exact(
        call(
            "SERIESSUM",
            vec![n(2.0), n(0.0), n(1.0), arr(vec![vec![1.0, 2.0, 3.0]])],
        ),
        17.0,
    );
    let x = arr(vec![vec![2.0, 3.0]]);
    let y = arr(vec![vec![1.0, 2.0]]);
    exact(call("SUMX2MY2", vec![x.clone(), y.clone()]), 8.0);
    exact(call("SUMX2PY2", vec![x.clone(), y.clone()]), 18.0);
    exact(call("SUMXMY2", vec![x.clone(), y]), 2.0);
    error(call("SUMXMY2", vec![x, arr(vec![vec![1.0]])]), NA);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn minverse_accepts_tiny_nonzero_pivots() {
    let pivot = f64::from_bits(0x3d3c_25c2_6849_7680);
    let expected = f64::from_bits(0x42a2_309c_e540_0001);
    let diagonal = arr(vec![vec![pivot, 0.0], vec![0.0, 1.0]]);
    assert_eq!(
        array(call("MINVERSE", vec![diagonal])),
        (2, 2, vec![expected, 0.0, 0.0, 1.0]),
    );
    let swapped = arr(vec![vec![0.0, 1.0], vec![f64::MIN_POSITIVE, 0.0]]);
    assert_eq!(
        array(call("MINVERSE", vec![swapped])),
        (2, 2, vec![0.0, f64::from_bits(0x7fd0_0000_0000_0000), 1.0, 0.0]),
    );
    let near_dependent = arr(vec![vec![1.0, 1.0], vec![1.0, 1.0 + f64::EPSILON]]);
    let reciprocal = 4_503_599_627_370_496.0;
    assert_eq!(
        array(call("MINVERSE", vec![near_dependent])),
        (2, 2, vec![reciprocal + 1.0, -reciprocal, -reciprocal, reciprocal]),
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn minverse_pivot_must_remain_normal_after_elimination() {
    let smallest_normal = f64::MIN_POSITIVE;
    let twice_smallest_normal = 2.0 * smallest_normal;
    let matrix = |last| arr(vec![vec![1.0, 1.0], vec![smallest_normal, last]]);
    let normal_reciprocal = f64::from_bits(0x7fd0_0000_0000_0000);
    assert_eq!(
        array(call("MINVERSE", vec![matrix(twice_smallest_normal)])),
        (2, 2, vec![2.0, -normal_reciprocal, -1.0, normal_reciprocal]),
    );
    error(
        call("MINVERSE", vec![matrix(twice_smallest_normal.next_down())]),
        Num,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn matrix_functions() {
    for value in [1e-100, -1e-13, 0.0] {
        exact(call("MDETERM", vec![n(value)]), value);
    }
    exact(
        call(
            "MDETERM",
            vec![arr(vec![vec![1e-100, 0.0], vec![0.0, 2.0]])],
        ),
        2e-100,
    );

    close(
        call("MDETERM", vec![arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]])]),
        -2.0,
        1e-15,
    );
    let (rows, cols, cells) = array(call(
        "MINVERSE",
        vec![arr(vec![vec![4.0, 7.0], vec![2.0, 6.0]])],
    ));
    assert_eq!((rows, cols), (2, 2));
    for (got, want) in cells.iter().zip([0.6, -0.7, -0.2, 0.4]) {
        assert!((got - want).abs() < 1e-15, "{got} vs {want}");
    }
    error(
        call("MINVERSE", vec![arr(vec![vec![1.0, 2.0], vec![2.0, 4.0]])]),
        Num,
    );
    let (rows, cols, cells) = array(call(
        "MMULT",
        vec![
            arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]]),
            arr(vec![vec![5.0], vec![6.0]]),
        ],
    ));
    assert_eq!((rows, cols, cells), (2, 1, vec![17.0, 39.0]));
    error(
        call(
            "MMULT",
            vec![arr(vec![vec![1.0, 2.0]]), arr(vec![vec![1.0, 2.0]])],
        ),
        Value,
    );
    let (rows, cols, cells) = array(call("MUNIT", vec![n(2.0)]));
    assert_eq!((rows, cols, cells), (2, 2, vec![1.0, 0.0, 0.0, 1.0]));
    error(call("MUNIT", vec![n(0.0)]), Value);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn arguments_are_read_by_worksheet_coercion() {
    close(call("SIN", vec![t("1")]), 0.8414709848078965, 1e-15);
    exact(call("SQRT", vec![b(true)]), 1.0);
    error(call("SQRT", vec![t("abc")]), Value);
    error(call("LN", vec![e(Div0)]), Div0);
    // An array given to a function of single values lifts over it, which is not modelled.
    refused(call("SIN", vec![arr(vec![vec![1.0, 2.0]])]));
}

/// Excel 16.0.20527 fn1 ECMA.CEILING Value2 readings, including all sign/zero controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fn1_ecma_ceiling_matches_the_recorded_controls() {
    for (value, significance, expected) in [
        (1.0, 1.0, 1.0),
        (-4.3, 2.0, -4.0),
        (-4.3, -2.0, -6.0),
        (4.3, 0.0, 0.0),
        (0.0, 2.0, 0.0),
    ] {
        exact(
            call("ECMA.CEILING", vec![n(value), n(significance)]),
            expected,
        );
    }
    error(call("ECMA.CEILING", vec![n(4.3), n(-2.0)]), Num);
}

/// Recorded Excel answers (build 20430, OxFunc-Evidence scalar bank) for the rounding family's
/// digit-count, fifteen-digit reading, ROUNDUP assembly and MROUND half rules. Each row is
/// `(function, number bits, digits bits, Excel's answer bits)`; `None` stands for #NUM!.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rounding_family_matches_recorded_bank_rows() {
    #[rustfmt::skip]
    let rows: &[(&str, u64, u64, Option<u64>)] = &[
        // ROUND keeps digits + exponent + 1 significant digits in 32-bit arithmetic: a sum
        // that wraps below the 32-bit range keeps fifteen digits, one above it keeps none.
        ("ROUND", 0x9233_cf3f_bcba_bd40, 0xc1df_ffff_d60e_432a, Some(0x9233_cf3f_bcba_bd3c)),
        ("ROUND", 0xc2d9_945c_a262_0000, 0x41df_ffff_fcc0_0000, Some(0)),
        // A 16-digit tie read on the x87 unit at 64-bit precision: ROUND drops it.
        ("ROUND", 0xc316_a939_8069_56e4, 0x402c_0000_0000_0000, Some(0xc316_a939_8069_56d0)),
        // ROUNDDOWN keeps it.
        ("ROUNDDOWN", 0x42f0_168e_a699_71b8, 0, Some(0x42f0_168e_a699_71c0)),
        // The directed functions read num_digits as 16 bits of its magnitude below 2^32; a
        // magnitude with the 16-bit sign bit set, or 2^32 or more, counts as 100 places.
        ("ROUNDDOWN", 0x2b31_482f_e620_c5cf, 0x40e0_0000_0000_0000, Some(0x2b2b_ff2e_e48e_0530)),
        ("ROUNDDOWN", 0x2b9b_00ca_d793_3513, 0x40ef_ffe0_0000_0000, Some(0x2b9a_e737_0fa0_78fc)),
        ("ROUNDDOWN", 0xd4b2_d135_2447_6f3a, 0xc1df_ffff_c600_0000, Some(0xd4b2_49ad_2594_c37d)),
        // ROUNDUP adds no unit for such a positive count and has none to add for a negative one.
        ("ROUNDUP", 0x270b_bc24_37e4_5a41, 0x41df_ffff_afc0_0000, Some(0)),
        // -2147483748 is 100 below 2^31 in its low 16 bits: ROUNDUP to 1E+100.
        ("ROUNDUP", 0x166e_3d71_f622_be8f, 0xc1e0_0000_0c80_0000, Some(0x54b2_49ad_2594_c37d)),
        // ROUNDUP is the ROUNDDOWN result plus one unit, added in binary64.
        ("ROUNDUP", 0xbff2_0000_0000_0000, 0x4000_0000_0000_0000, Some(0xbff2_147a_e147_ae15)),
        ("ROUNDUP", 0x400c_47ae_147a_e148, 0x4000_0000_0000_0000, Some(0x400c_51eb_851e_b851)),
        // ROUNDUP: a positive reading under 2^-1026 above the truncation stays, a unit below the
        // normal range is not added, and otherwise the unit is added.
        ("ROUNDUP", 0x0031_fa98_2c40_c60e, 0x4073_3000_0000_0000, Some(0x0031_fa18_2c40_c60d)),
        ("ROUNDUP", 0x8035_fa18_2c40_c60e, 0x4073_4000_0000_0000, Some(0x8035_9283_684d_ba77)),
        ("ROUNDUP", 0x01a5_6e1f_c3c5_c026, 0x4073_3000_0000_0000, Some(0x01a5_6e1f_e6ed_23b2)),
        // MOD with a subnormal remainder of the other sign: added to the divisor below 2^-1026,
        // taken from it above, and #NUM! for a count of 2^26 or more or a divisor whose Veltkamp
        // low part is subnormal (the last row is Excel 16.0.20527).
        ("MOD", 0x0020_0000_0000_0001, 0x8020_0000_0000_0000, Some(0x8020_0000_0000_0001)),
        ("MOD", 0x0020_8000_0000_0000, 0x8020_0000_0000_0000, Some(0x801f_0000_0000_0000)),
        ("MOD", 0x01c0_0000_0000_0001, 0x8020_0000_0000_0000, None),
        ("MOD", 0x827a_28ed_c580_e50f, 0x027a_28ed_c580_e50e, Some(0x027a_28ed_c580_e50f)),
        // MROUND counts a quotient within 5E-15 below a half up.
        ("MROUND", 0x4037_7fff_ffff_ffff, 0x3ff0_0000_0000_0000, Some(0x4038_0000_0000_0000)),
        ("MROUND", 0xbfa9_9999_9999_9978, 0xbfb9_9999_9999_999a, Some(0xbfb9_9999_9999_999a)),
        ("MROUND", 0x2431_da05_80b5_7755, 0x20fc_dcb2_1a5b_96d6, Some(0x2431_da05_80b5_7756)),
    ];
    for &(function, number, digits, expected) in rows {
        let result = call(
            function,
            vec![n(f64::from_bits(number)), n(f64::from_bits(digits))],
        );
        match expected {
            Some(bits) => exact(result, f64::from_bits(bits)),
            None => error(result, Num),
        }
    }
    // MROUND(6.05,0.1) stays 6, the quotient 60.49999999999999 lying 7E-15 below the half.
    assert_excel_capture(
        call("MROUND", vec![n(6.05), n(0.1)]),
        include_bytes!("captures/catalogue/sections/dcj9k_math.capture.json"),
        "dcj9k_math",
        48,
        4,
    );
    assert_excel_capture(
        call("ROUND", vec![n(1.5), n(2_147_483_647.0)]),
        include_bytes!("captures/catalogue/sections/dch9u_functions.capture.json"),
        "dch9u_functions",
        79,
        4,
    );
    exact(call("ROUND", vec![n(1.5), n(2_147_483_646.0)]), 1.5);
    assert_excel_capture(
        call("TRUNC", vec![n(1.5), n(1e300)]),
        include_bytes!("captures/catalogue/sections/dch9u_functions.capture.json"),
        "dch9u_functions",
        82,
        4,
    );
    // Past DBL_MAX Excel keeps the significand and carries the exponent into the field's top
    // value: NaN bit patterns that the worksheet holds as numbers (Excel 16.0.20527, Value2).
    for (function, number, digits, bits) in [
        ("ROUND", f64::MAX, -308.0, 0x7ff1_ccf3_85eb_c8a0u64),
        ("ROUND", -f64::MAX, -308.0, 0xfff1_ccf3_85eb_c8a0),
        ("ROUND", f64::MAX, -307.0, 0x7ff0_0541_9221_015d),
        ("ROUND", f64::MAX, 0.0, 0x7ff0_0000_0000_000a),
        (
            "TRUNC",
            f64::from_bits(0x7fef_ffff_ffff_fffc),
            0.0,
            0x7ff0_0000_0000_000a,
        ),
    ] {
        exact(
            call(function, vec![n(number), n(digits)]),
            f64::from_bits(bits),
        );
    }
    error(call("ROUNDUP", vec![n(f64::MAX), n(-308.0)]), Num);
    assert_excel_capture(
        call("ROUNDUP", vec![n(1.5), n(1e300)]),
        include_bytes!("captures/catalogue/sections/dch9u_functions.capture.json"),
        "dch9u_functions",
        81,
        4,
    );
    error(call("ROUNDUP", vec![n(1.5), n(-1e300)]), Num);
}

/// dc-j9k.87 / dc-j9k.53 (Carbon cb16): unified 8087 FPTAN circular substrate for SIN and COS,
/// replaying Excel 16.0.20527.20196 rows captured on Windows x64, AMD Ryzen 7 4800H.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_j9k_87_cb16_unified_fptan_circular_substrate() {
    if !crate::x87_cpu::AMD_RYZEN_7_4800H.compares_on_active_backend() {
        return;
    }

    let unary_rows: &[(&str, u64, u64)] = &[
        // dc-j9k.53 five folded angles from ax1 run2-final-shared-leaf-backends.json
        // plus the two remaining full-bank SIN misses (ax1 + sx1).
        ("SIN", 0x3faa_14e4_96a7_f689, 0x3faa_1201_6ee0_4ef8),
        ("SIN", 0x3ff1_62fd_5784_1647, 0x3fec_5294_c765_1321),
        ("SIN", 0x3ff1_6ef7_e5a2_7363, 0x3fec_5db3_6ad4_dd88),
        ("SIN", 0x3ff3_92ac_71f2_c347, 0x3fee_1660_04fa_bb6e),
        ("SIN", 0x3ff6_b7c0_d24e_314c, 0x3fef_a2dd_682d_2345),
        ("SIN", 0x3fbd_41c1_3898_1b43, 0x3fbd_3176_2552_370a),
        ("SIN", 0x419a_77f4_5268_bd66, 0xbfc5_1722_5ca6_86a8),
        // Sole full-bank COS miss: COS(120031), q & 3 == 2.
        ("COS", 0x40fd_4df0_0000_0000, 0xbfef_8a9f_c355_34cd),
        // cb16 single-site discriminants (FPTAN vs FSIN/FCOS, sqrt-after-div, PC64 register chain).
        ("SIN", 0xbf71_deca_0271_8435, 0xbf71_dec6_4b4f_3b28),
        ("SIN", 0xc066_fed7_94bf_89cd, 0xbfef_7ab2_dddc_8361),
        ("SIN", 0xc015_9305_0458_72de, 0x3fe8_dbc4_9cd3_51fc),
        ("SIN", 0x3efe_9334_ba2e_6469, 0x3efe_9334_ba1b_c8a8),
        ("SIN", 0x3eb4_6fba_0f8f_af48, 0x3eb4_6fba_0f8f_a9b9),
        ("COS", 0xbfce_64ae_54ef_3c67, 0x3fef_1a25_2321_2fd2),
        ("COS", 0x40c2_8e38_77a2_bb0c, 0x3fee_e21c_0c58_f8b1),
        ("COS", 0x3ff2_9f9b_9d5a_9970, 0x3fd9_5324_a4d2_3916),
        // Seam and small-argument SIN controls.
        ("SIN", 0x3e5b_b67a_e858_4ca9, 0x3e5b_b67a_e858_4ca8),
        ("SIN", 0x3e5b_b67a_e858_4cab, 0x3e5b_b67a_e858_4caa),
        ("SIN", 0x3fe9_21fb_5444_2d15, 0x3fe6_a09e_667f_3bca),
        ("SIN", 0x3fe9_21fb_5444_2d1b, 0x3fe6_a09e_667f_3bce),
        ("SIN", 0x419f_ffff_fbff_fe00, 0xbfee_977a_a08c_ddea),
        // Holdout flips on SIN and COS.
        ("SIN", 0xc0dd_8b62_d67e_bcec, 0xbf7d_07e6_1c67_ad3b),
        ("COS", 0xc07b_a69f_2a7a_9549, 0xbfeb_44db_f5a9_d26f),
        // Downstream callers: CSC, SEC, reflected negative GAMMA (incl. 2-ULP jumps).
        ("CSC", 0xc0e6_d6be_5c18_51bd, 0xbff1_02d9_7e3f_a979),
        ("SEC", 0xbf0a_8428_7768_7595, 0x3ff0_0000_0057_e38f),
        ("GAMMA", 0xc033_9b9f_9ce3_3678, 0x3c54_8f99_9bf9_fcf8),
        ("GAMMA", 0xc048_2c64_db06_ec16, 0xb33e_85ee_67d6_9eb0),
        ("GAMMA", 0xc04e_b9d9_b1a5_ce6f, 0x2e7e_3191_a469_4d2c),
        ("GAMMA", 0xc058_4254_0408_002c, 0x20a9_6537_1264_8955),
        ("GAMMA", 0xc05b_2714_3653_5896, 0x9ba2_3aac_4d2a_cf55),
        ("GAMMA", 0xc05c_7ef6_55ba_2654, 0x199c_7e6e_784c_f8e6),
        ("GAMMA", 0xc03b_be4b_ff51_4192, 0x3a05_5ad6_6dcd_b279),
        ("GAMMA", 0xc05b_ee07_e248_f9fa, 0x1a54_9adb_119a_e212),
    ];
    for &(func, arg, expected) in unary_rows {
        let got = number(call(func, vec![n(f64::from_bits(arg))]));
        assert_eq!(
            got.to_bits(),
            expected,
            "{func}({arg:#018x}): got {:#018x}, expected {expected:#018x}",
            got.to_bits()
        );
    }

    // Downstream complex circular callers.
    #[rustfmt::skip]
    let complex_rows = [
        ("IMCOT", "120031-0.1i", "4.32013081085074+2.61581898713197i"),
        ("IMCOT", "17445.25-0.1i", "-1.36893603420608+9.84093514703274i"),
        ("IMSIN", "115110.25-0.1i", "0.75266066425061+0.0663773356796942i"),
        ("IMCOS", "37723.25-0.1i", "0.547578037830135-0.0839930800440596i"),
    ];
    for (func, arg, expected) in complex_rows {
        text(call(func, vec![t(arg)]), expected);
    }

    // Downstream BESSELJ / BESSELY pure trig-substrate flips.
    #[rustfmt::skip]
    let bessel_rows = [
        ("BESSELJ", 0x40a6_b6c5_914b_2a58u64, 0.0, 0xbf88_8973_07bb_fbc4u64),
        ("BESSELY", 0x40f4_e59f_3784_cab8u64, 0.0, 0xbf54_633f_05f3_7d2bu64),
    ];
    for (func, x_bits, order, expected) in bessel_rows {
        let got = number(call(func, vec![n(f64::from_bits(x_bits)), n(order)]));
        assert_eq!(
            got.to_bits(),
            expected,
            "{func}({x_bits:#018x}, {order}): got {:#018x}, expected {expected:#018x}",
            got.to_bits()
        );
    }
    println!(
        "Matched {} cb16 Excel rows",
        unary_rows.len() + complex_rows.len() + bessel_rows.len()
    );
}

/// dc-j9k.88 (Carbon cb17): ROUND shares Excel's Q0.64 two-branch 15-digit reader
/// (`number_text::decimal`) with post-carry `reading.magnitude()` wraparound, and
/// COMPLEX/IM* share `directed_fifteen_x87` (`Excel 16.0.20527.20196`, portable integer arithmetic).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_j9k_88_cb17_round_q64_and_complex_text() {
    #[rustfmt::skip]
    let round_rows: &[(u64, u64, u64)] = &[
        // P1: Branch 1 (1 <= |x| < 2^16) exact dyadic ties round down.
        (0x3ff4_47a0_0000_0000, 0x402c_0000_0000_0000, 0x3ff4_479f_ffff_ffe9),
        (0xc030_ed24_0000_0000, 0x402a_0000_0000_0000, 0xc030_ed23_ffff_fff2),
        // P1 / H1: Branch 1 Q16.64 pre-addition flips above exact midpoints.
        (0x3ffc_33d8_5249_3ddf, 0x402c_0000_0000_0000, 0x3ffc_33d8_5249_3df6),
        (0x3ff9_be20_2d72_9743, 0x402c_0000_0000_0000, 0x3ff9_be20_2d72_975a),
        (0xbffc_85c3_4a96_bf57, 0x402c_0000_0000_0000, 0xbffc_85c3_4a96_bf6e),
        // P2 / H2 / full-bank ROUND.tsv: Branch 2 sawtooth & low-nibble PC64 scale.
        (0x40f4_c22b_c1ba_cb95, 0x4024_0000_0000_0000, 0x40f4_c22b_c1ba_cb92),
        (0x0491_263c_6b33_45a8, 0x4072_c000_0000_0000, 0x0491_263c_6b33_4593),
        (0x083b_a339_4b26_4876, 0x4071_b000_0000_0000, 0x083b_a339_4b26_487d),
        (0x201d_d231_4018_4ffa, 0x4065_0000_0000_0000, 0x201d_d231_4018_4ff2),
        // P3: Branch 2 Farey ceil(frexp_k / 3.3219) discriminators.
        (0x0345_33c7_8670_3714, 0x4073_3000_0000_0000, 0x0345_33c7_8670_3718),
        (0x834c_f61d_efd9_43ed, 0x4073_3000_0000_0000, 0x834c_f61d_efd9_43f1),
        // P3: Branch 2 ev >= 0 sliver (10^-1 correction before truncating shift).
        (0x219f_fb9c_4acf_e7ee, 0x4064_0000_0000_0000, 0x219f_fb9c_4acf_e7c1),
        (0xa19f_fa8f_8f4f_0396, 0x4064_0000_0000_0000, 0xa19f_fa8f_8f4f_0369),
        // P4A / P4B: Post-carry reading.magnitude() 32-bit wraparound vs pre-carry.
        (0x40c3_87ff_ffff_ffff, 0x41df_ffff_fec0_0000, 0x0000_0000_0000_0000),
        (0xc12e_847f_ffff_fffe, 0x41df_ffff_fe40_0000, 0x0000_0000_0000_0000),
        (0x40f0_0000_0000_0671, 0x41df_ffff_fe80_0000, 0x40f0_0000_0000_0671),
        // Pinned ROUND-00970 preserved (truncating shift into Q0.64).
        (0x02d6_c926_0001_5d5b, 0x4073_5000_0000_0000, 0x02d6_c926_0001_5d55),
        // Subnormal guard returns +0 regardless of num_digits.
        (0x0000_0000_0000_0001, 0x4074_4000_0000_0000, 0x0000_0000_0000_0000),
    ];
    for &(x_bits, digits_bits, expected) in round_rows {
        let got = number(call(
            "ROUND",
            vec![n(f64::from_bits(x_bits)), n(f64::from_bits(digits_bits))],
        ));
        assert_eq!(
            got.to_bits(),
            expected,
            "ROUND({x_bits:#018x}, {digits_bits:#018x}): got {:#018x}, expected {expected:#018x}",
            got.to_bits()
        );
    }

    // Holdout controls confirming COMPLEX and IM* retain directed_fifteen_x87 formatting.
    for &(re_bits, im_bits, expected) in &[
        (0x02b0_4039_5ff7_2d65u64, 0x0000_0000_0000_0000u64, "9.9394196903566E-296"),
        (0x5e32_1958_0b96_1a1b, 0x0000_0000_0000_0000, "5.65005261194644E+145"),
        (0x0000_0000_0000_0000, 0xa19f_ffdd_9f1d_a8e7, "-1.00102513759339E-146i"),
    ] {
        text(
            call("COMPLEX", vec![n(f64::from_bits(re_bits)), n(f64::from_bits(im_bits))]),
            expected,
        );
    }
    for &(func, a, b, expected) in &[
        ("IMDIV", "779496170723E-200+0i", "8381401", "9.3003087517588E-196"),
        ("IMPRODUCT", "217839000024E-212", "270856680823E-1", "5.90031485003021E-191"),
        ("IMSUM", "842235242468E-152", "519992646571E-160", "8.42235247667926E-141"),
        ("IMSUB", "939668388801E-182", "298196148651E-190", "9.39668385819039E-171"),
    ] {
        text(call(func, vec![t(a), t(b)]), expected);
    }
}

/// dc-ztvv.26.63 (cb28 S1e/S3b): ROUND's num_digits uses symmetric `date_integer_floor`
/// (`T22 = 2^-22 + 2^-33` on `|places|`, keeping sign), while ROUNDUP/ROUNDDOWN/TRUNC
/// keep plain truncation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ztvv_26_63_round_places_uses_symmetric_date_integer_floor() {
    #[rustfmt::skip]
    let rows: &[(u64, u64, u64)] = &[
        // fuzz-ROUND-r01-000136: ROUND(283, -1 + 2^-52) -> 280
        (0x4071_b000_0000_0000, 0xbfefffff_fffffffe, 0x4071_8000_0000_0000),
        // 1-ULP lock at +(1 - T22) in vs out:
        (0x4071_b70a_3d70_a3d7, 0x3fefffff_7ff00000, 0x4071_b666_6666_6666),
        (0x4071_b70a_3d70_a3d7, 0x3fefffff_7fefffff, 0x4071_b000_0000_0000),
        // 1-ULP lock at -(1 - T22) in vs out:
        (0x4071_b000_0000_0000, 0xbfefffff_7ff00000, 0x4071_8000_0000_0000),
        (0x4071_b000_0000_0000, 0xbfefffff_7fefffff, 0x4071_b000_0000_0000),
        // -100 + T22 and i32::MAX - T22:
        (0x5500_2c5b_86d5_f67c, 0xc058_ffff_ff80_0000, 0x5500_0077_80e2_2b0d),
        (0x3ff2_0000_0000_0000, 0x41df_ffff_ffbf_ffff, 0x0000_0000_0000_0000),
        // -T23 snaps toward 0, not -1:
        (0x4071_b000_0000_0000, 0xbe80_0200_0000_0000, 0x4071_b000_0000_0000),
    ];
    for &(x_bits, places_bits, expected) in rows {
        let got = number(call(
            "ROUND",
            vec![n(f64::from_bits(x_bits)), n(f64::from_bits(places_bits))],
        ));
        assert_eq!(
            got.to_bits(),
            expected,
            "ROUND({x_bits:#018x}, {places_bits:#018x}): got {:#018x}, expected {expected:#018x}",
            got.to_bits()
        );
    }
    // Sibling controls: ROUNDUP, ROUNDDOWN, TRUNC do not snap num_digits by T22.
    let near_neg_one = f64::from_bits(0xbfefffff_fffffffe);
    for func in ["ROUNDUP", "ROUNDDOWN", "TRUNC"] {
        exact(call(func, vec![n(283.0), n(near_neg_one)]), 283.0);
    }
}

/// Excel 16.0 build 20625 (Touchstone, dc-j9k.360; the whole grid 1-3999 x forms 0-4 was asked): forms 2-4 write
/// 450-489 and 950-989 with L.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn roman_forms_2_to_4_use_l_for_450_and_950_bands() {
    for form in [2.0, 3.0, 4.0] {
        text(call("ROMAN", vec![n(3981.0), n(form)]), "MMMLMXXXI");
        text(call("ROMAN", vec![n(450.0), n(form)]), "LD");
        text(call("ROMAN", vec![n(488.0), n(form)]), "LDXXXVIII");
        text(call("ROMAN", vec![n(1489.0), n(form)]), "MLDXXXIX");
        text(call("ROMAN", vec![n(449.0), n(form)]), "CDIL");
        text(call("ROMAN", vec![n(949.0), n(form)]), "CMIL");
    }
    text(call("ROMAN", vec![n(3981.0), n(1.0)]), "MMMLMXXXI");
}

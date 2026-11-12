#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the math and trigonometry functions ported from OxFunc, called through
//! `calc_func::invoke` as the formula evaluator calls them. The expected values are Excel's
//! documented results and the seed rows of OxFunc's tests (OxFunc 6fca3518, functions/).

mod f1_support;

use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Value};
use f1_support::*;

const PI: f64 = std::f64::consts::PI;

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
    error(call("POWER", vec![n(0.0), n(0.0)]), Num);
    error(call("POWER", vec![n(0.0), n(-1.0)]), Div0);
    error(call("POWER", vec![n(-8.0), n(0.5)]), Num);
    // The powers that need a power algorithm, `^` and POWER compute by OxFunc's power kernel.
    exact(call("POWER", vec![n(2.0), n(3.0)]), 8.0);
    exact(
        call("POWER", vec![n(2.0), n(0.5)]),
        std::f64::consts::SQRT_2,
    );
    exact(call("POWER", vec![n(-8.0), n(3.0)]), -512.0);
    exact(call("POWER", vec![n(10.0), n(-2.0)]), 0.01);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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
    exact(call("MROUND", vec![n(6.05), n(0.1)]), 6.0);
    exact(call("ROUND", vec![n(1.5), n(2_147_483_647.0)]), 0.0);
    exact(call("ROUND", vec![n(1.5), n(2_147_483_646.0)]), 1.5);
    exact(call("TRUNC", vec![n(1.5), n(1e300)]), 1.5);
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
    exact(call("ROUNDUP", vec![n(1.5), n(1e300)]), 1.5);
    error(call("ROUNDUP", vec![n(1.5), n(-1e300)]), Num);
}

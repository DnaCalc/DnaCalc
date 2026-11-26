#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the engineering functions ported from OxFunc, called through
//! `calc_func::invoke`. Expected values are Excel's documented examples.

mod f1_support;

/// Excel 20527 uses the worksheet exponential for these numeric IMEXP inputs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imexp_numeric_real_uses_excel_exponential() {
    use f1_support::{call, n, text};
    for (bits, expected) in [
        (0x403b_0000_0000_0001, "532048240601.801"),
        (0xc070_dbc7_9532_e44a, "7.16218213402738E-118"),
        (0x406e_0b28_596a_2a00, "2.41047970972278E+104"),
    ] {
        text(call("IMEXP", vec![n(f64::from_bits(bits))]), expected);
    }
}

/// Excel 20527 rejects empty complex text while a blank cell remains zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_functions_distinguish_empty_text_from_blank() {
    for name in ["IMEXP", "IMSEC", "IMABS", "IMCONJUGATE"] {
        for value in [t(""), t(" "), t("\t")] {
            error(call(name, vec![value]), Num);
        }
    }
    text(call("IMEXP", vec![calc_func::value::CalcValue::empty()]), "1");
    text(call("IMSEC", vec![calc_func::value::CalcValue::empty()]), "1");
    error(call("IMSUM", vec![t(""), n(1.0)]), Num);
    error(call("IMPRODUCT", vec![n(1.0), t("")]), Num);
}

/// Touchstone Excel 16.0.20625.20108, dc-j9k.522.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imabs_scales_by_the_larger_component() {
    for (input, bits) in [
        ("710+i", 0x4086_3001_7137_7ad9),
        ("1+710i", 0x4086_3001_7137_7ad9),
        ("710-1i", 0x4086_3001_7137_7ad9),
        ("1+5i", 0x4014_6565_5f12_2ff7),
        ("1e-308+1e-308i", 0),
    ] {
        exact(call("IMABS", vec![t(input)]), f64::from_bits(bits));
    }
}

/// Excel 20527 uses circular argument reduction and rejects angles above 2^27 - 1.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn impower_negative_axis_and_angle_limit() {
    for (exponent, expected) in [
        (1.0, "-1+1.22514845490862E-16i"),
        (2.0, "1-2.45029690981724E-16i"),
        (50.0, "1+9.79685083057902E-16i"),
        (365.0, "-1-4.40999233658479E-14i"),
    ] {
        text(call("IMPOWER", vec![n(-1.0), n(exponent)]), expected);
    }
    for (base, exponent, expected) in [
        (-2.0, 0.5, "8.66310780426106E-17+1.4142135623731i"),
        (-2.0, -10.0, "0.0009765625+1.1964340379967E-18i"),
        (-1.5, 25.0, "-25251.1682940423-1.2369096453729E-11i"),
        (-0.75, -15.0, "-74.8309138807576-4.03371279121925E-13i"),
    ] {
        text(call("IMPOWER", vec![n(base), n(exponent)]), expected);
    }
    text(
        call("IMPOWER", vec![n(-1.0), n(f64::from_bits(0x4184_5f30_6b3d_e275))]),
        "-0.293388403557722-0.955993328772671i",
    );
    error(call("IMPOWER", vec![n(-1.0), n(f64::from_bits(0x4184_5f30_6b3d_e276))]), Num);
    text(call("IMPOWER", vec![n(1.0), n(1e100)]), "1");
    text(call("IMPOWER", vec![n(-2.0), n(-1e9)]), "0");
    text(call("IMPOWER", vec![t("3+4i"), n(-1e9)]), "0");
    text(call("IMPOWER", vec![t("i"), n(2.0)]), "-1+1.22514845490862E-16i");
    text(call("IMPOWER", vec![t("-i"), n(3.0)]), "-1.83772268236293E-16+i");
    text(call("IMPOWER", vec![n(1.5), n(25.0)]), "25251.1682940423");
    text(call("IMPOWER", vec![n(0.75), n(-15.0)]), "74.8309138807576");
}

use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Value};
use f1_support::*;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn radix_conversions() {
    exact(call("BIN2DEC", vec![t("1100100")]), 100.0);
    exact(call("BIN2DEC", vec![t("1111111111")]), -1.0);
    assert_excel_capture(
        call("DEC2BIN", vec![n(9.0), n(4.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        62,
        1,
    );
    text(call("DEC2BIN", vec![n(-100.0)]), "1110011100");
    error(call("DEC2BIN", vec![n(9.0), n(2.0)]), Num);
    text(call("DEC2HEX", vec![n(-54.0)]), "FFFFFFFFCA");
    assert_excel_capture(
        call("DEC2HEX", vec![n(100.0), n(4.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        63,
        1,
    );
    exact(call("HEX2DEC", vec![t("FFFFFFFF9C")]), -100.0);
    assert_excel_capture(
        call("OCT2DEC", vec![n(54.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        170,
        1,
    );
    assert_excel_capture(
        call("HEX2BIN", vec![t("F"), n(8.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        106,
        1,
    );
    assert_excel_capture(
        call("OCT2HEX", vec![n(100.0), n(4.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        171,
        1,
    );
    assert_excel_capture(
        call("BIN2OCT", vec![n(1001.0), n(3.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        24,
        1,
    );
    error(call("BIN2DEC", vec![b(true)]), Value);
    error(call("DEC2BIN", vec![missing()]), NA);
    error(call("BIN2DEC", vec![t("102")]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bit_functions() {
    assert_excel_capture(
        call("BITAND", vec![n(13.0), n(25.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        29,
        1,
    );
    assert_excel_capture(
        call("BITOR", vec![n(23.0), n(10.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        31,
        1,
    );
    assert_excel_capture(
        call("BITXOR", vec![n(5.0), n(3.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        33,
        1,
    );
    assert_excel_capture(
        call("BITLSHIFT", vec![n(4.0), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        30,
        1,
    );
    assert_excel_capture(
        call("BITRSHIFT", vec![n(13.0), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        32,
        1,
    );
    error(call("BITAND", vec![n(1.5), n(1.0)]), Num);
    error(call("BITAND", vec![n(-1.0), n(1.0)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_numbers() {
    assert_excel_capture(
        call("COMPLEX", vec![n(3.0), n(4.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        46,
        1,
    );
    text(call("COMPLEX", vec![n(3.0), n(4.0), t("j")]), "3+4j");
    assert_excel_capture(
        call("COMPLEX", vec![n(0.0), n(1.0)]),
        include_bytes!("captures/jnt/sections/jnt_parity_complex.capture.json"),
        "jnt_parity_complex",
        585,
        4,
    );
    error(call("COMPLEX", vec![n(3.0), n(4.0), t("k")]), Value);
    assert_excel_capture(
        call("IMABS", vec![t("5+12i")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        111,
        1,
    );
    assert_excel_capture(
        call("IMREAL", vec![t("6-9i")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        127,
        1,
    );
    assert_excel_capture(
        call("IMAGINARY", vec![t("3+4i")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        112,
        1,
    );
    assert_excel_capture(
        call("IMCONJUGATE", vec![t("3+4i")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        114,
        1,
    );
    assert_excel_capture(
        call("IMSUM", vec![t("3+4i"), t("5-3i")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        134,
        1,
    );
    assert_excel_capture(
        call("IMSUB", vec![t("13+4i"), t("5+3i")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        133,
        1,
    );
    assert_excel_capture(
        call("IMPRODUCT", vec![t("3+4i"), t("5-3i")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        126,
        1,
    );
    assert_excel_capture(
        call("IMDIV", vec![t("-238+240i"), t("10+24i")]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        120,
        1,
    );
    text(call("IMPOWER", vec![t("2+3i"), n(2.0)]), "-5+12i");
    close(call("IMARGUMENT", vec![t("3+4i")]), 0.92729521800161, 1e-13);
    error(call("IMABS", vec![t("not complex")]), Num);
    error(call("IMSUM", vec![t("1+i"), t("1+j")]), Value);
}

/// Excel 20527 jnt_parity_complex D389, D407 and D408: the circular factors
/// retain Excel's SIN reduction even away from the real axis.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_tangent_and_cotangent_keep_the_excel_circular_factors() {
    assert_excel_capture(
        call("IMTAN", vec![t("3.141592653589793+1e-100i")]),
        include_bytes!("captures/jnt/sections/jnt_parity_complex.capture.json"),
        "jnt_parity_complex",
        389,
        4,
    );
    assert_excel_capture(
        call("IMCOT", vec![t("1.5707963267948966-0.1i")]),
        include_bytes!("captures/jnt/sections/jnt_parity_complex.capture.json"),
        "jnt_parity_complex",
        407,
        4,
    );
    assert_excel_capture(
        call("IMCOT", vec![t("-1.5707963267948966-0.1i")]),
        include_bytes!("captures/jnt/sections/jnt_parity_complex.capture.json"),
        "jnt_parity_complex",
        408,
        4,
    );
}

/// Excel 20527 jnt_parity_complex D391 and D411, near the real-argument limit.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_tangent_and_cotangent_reduce_the_denominator_at_the_real_argument() {
    assert_excel_capture(
        call("IMTAN", vec![t("134217727+1e-100i")]),
        include_bytes!("captures/jnt/sections/jnt_parity_complex.capture.json"),
        "jnt_parity_complex",
        391,
        4,
    );
    assert_excel_capture(
        call("IMCOT", vec![t("134217727-0.1i")]),
        include_bytes!("captures/jnt/sections/jnt_parity_complex.capture.json"),
        "jnt_parity_complex",
        411,
        4,
    );
}

/// Touchstone Excel 16.0.20625 (dc-j9k.449): the real component is bounded at 2^27 - 1.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_sine_and_cosine_bound_the_real_component() {
    let boundary = 134_217_727.0_f64;
    let next = f64::from_bits(boundary.to_bits() + 1);
    for (function, bound_text) in [
        ("IMSIN", "-0.955993328772671"),
        ("IMCOS", "-0.293388403557722"),
    ] {
        text(call(function, vec![n(boundary)]), bound_text);
        for input in [n(next), n(-next), t("134217727.5"), t("-134217727.5+i")] {
            error(call(function, vec![input]), Num);
        }
    }
}

/// Excel 20527 distinguishes the stored leading product of the high J0 seed.
/// dc-iuwct.6-.9, Excel 16.0.20527 (Touchstone, 2026-11-20): complex coefficients take fifteen
/// digits through the nibble chain at every shift (decimal near-midpoints, where one rounded
/// power of ten gives the other neighbour), and the real-axis IMCOSH and IMSINH (from |x| = 1)
/// are the worksheet COSH and SINH.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_text_uses_the_nibble_chain_and_real_axis_hyperbolics_the_scalar_kernels() {
    for (bits, expected) in [
        (0x08a6_aa36_71bf_27b3_u64, "5.49145631004733E-267"),
        (0x5a7c_d1e9_e483_db23, "7.80354216159832E+127"),
        (0x157f_51f6_4d9e_c62a, "3.90217732321279E-205"),
        (0x1d1e_c47b_9e34_e502, "2.03814371988341E-168"),
        (0x10e5_6189_4bbe_3363, "2.82046893450868E-227"),
        (0x46bd_8fa6_beb2_0c93, "5.99571110793787E+32"),
    ] {
        text(call("COMPLEX", vec![n(f64::from_bits(bits)), n(0.0)]), expected);
    }
    text(call("IMCOSH", vec![n(f64::from_bits(0xc02e_4938_a826_0ece))]), "1885800.96126951");
    text(call("IMSINH", vec![n(f64::from_bits(0xc00f_5b65_9282_4c40))]), "-25.1809084647036");
}

/// dc-ymjf.2.128, Excel 16.0.20527.20196 (Touchstone, 2026-11-22): COMPLEX coefficients use
/// the directed 15-digit x87 reading (`ExactDecimal::fifteen_x87(x, true)`): `frexp`-based
/// `e_est = floor(k / 3.3219)` (48/48 vs 0/48 for `floor(k * log10(2))` on binade-4),
/// nibble-chain scaling + `*10` loop, exact half-up ties (10/10), and the `k = -485, -970`
/// sliver 16-digit binary64 re-scale with half-down (51/51 vs 15/51 for `*10^-1` or `/10`).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_text_uses_directed_fifteen_x87_including_binade4_and_slivers() {
    for (bits, expected) in [
        // Previously failing full-bank COMPLEX midpoint rows (dc-ymjf.2.128).
        (0x2946_7cda_51e6_16a3_u64, "7.48060549288466E-110"),
        (0x36c0_3e1a_b826_1b3a, "5.69018092922882E-45"),
        (0x4d65_1c8c_5ab6_b74b, "6.94781936777026E+64"),
        (0x5f44_951e_131e_24dd, "8.4218162443162E+150"),
        (0x7c86_d60c_9167_382e, "7.12144801799257E+291"),
        (0x0af3_9824_905d_51d2, "6.52489643647008E-256"),
        (0x49e1_f7e6_52d4_052d, "8.20649529430271E+47"),
        // Binade-4 discriminators confirming floor(frexp_k / 3.3219) over log10(2).
        (0x0340_911b_ebff_d95b, "5.18792517643478E-293"),
        (0x2191_6c8a_93e3_7090, "5.45066795064255E-147"),
        (0x5e38_5176_e7fd_3ec7, "7.59153699503465E+145"),
        (0x7c89_2187_8a20_ddde, "7.83709558821205E+291"),
        // 0.1% slivers at k = -485 and k = -970 confirming 16-digit f64 re-scale (half-down).
        (0x219f_f787_85c9_2417, "1.00000646001945E-146"),
        (0x034f_ef0f_31cd_bac7, "1.00001192003583E-292"),
        (0x034f_f04c_a142_9bce, "1.00016360049087E-292"),
        // Exact binary64 half-way ties round half up (unlike Q0.64 number_text).
        (0x3ff0_0020_0000_0000, "1.00003051757813"),
        (0x3ff0_0060_0000_0000, "1.00009155273438"),
        (100_000_000_000_000.5_f64.to_bits(), "100000000000001"),
        (100_000_000_000_001.5_f64.to_bits(), "100000000000002"),
    ] {
        text(
            call("COMPLEX", vec![n(f64::from_bits(bits)), n(0.0)]),
            expected,
        );
    }
}

/// Excel 16.0.20625, dc-j9k.401: the real-axis IMSEC reciprocal uses SEC's rounding.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imsec_real_axis_uses_excel_reciprocal_rounding() {
    for bits in [0x3e7a_d7f2_9abc_af48, 0xbe7a_d7f2_9abc_af48] {
        text(call("IMSEC", vec![n(f64::from_bits(bits))]), "1");
    }
    text(call("IMSEC", vec![n(2e-7)]), "1.00000000000002");
    error(call("IMSEC", vec![n(134_217_728.0)]), Num);
}

/// Excel 16.0.20625 (dc-j9k.397) applies the imaginary limit before reducing the real angle.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn circular_complex_reciprocals_zero_past_imaginary_limit() {
    for (function, positive, negative) in [
        ("IMCSC", "-2.43356150124685E-308i", "6.61510600727682E-308i"),
        ("IMSEC", "2.43356150124685E-308", "6.61510600727682E-308"),
    ] {
        for input in ["1e-5+2e5i", "0+709.000000000001i", "134217728-708.000000000001i"] {
            text(call(function, vec![t(input)]), "0");
        }
        text(call(function, vec![t("0+709i")]), positive);
        text(call(function, vec![t("0-708i")]), negative);
        error(call(function, vec![t("134217728+709i")]), Num);
    }
}

/// Excel 16.0.20625 (dc-j9k.398) retains the real axis and applies two off-axis limits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn hyperbolic_complex_reciprocal_large_real_limits() {
    for function in ["IMCSCH", "IMSECH"] {
        for input in ["3e150+4e150i", "709.000000000001+1e-5i", "-708.000000000001-1i"] {
            text(call(function, vec![t(input)]), "0");
        }
        text(call(function, vec![t("709.000000000001")]), "2.43356150124436E-308");
        error(call(function, vec![t("709+134217728i")]), Num);
    }
    for (input, expected) in [
        ("354.5+1i", "1.19199101546952E-154-1.85641601521147E-154i"),
        ("354.500000000001+1i", "-1.1919910154683E-154+1.85641601520957E-154i"),
        ("-354+1i", "-1.96526094168804E-154-3.06071257154752E-154i"),
        ("-354.000000000001+1i", "1.96526094168603E-154+3.06071257154439E-154i"),
    ] {
        text(call("IMCSCH", vec![t(input)]), expected);
    }
}

/// Excel 16.0.20625, dc-j9k.399: the off-axis denominator uses separate squares.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn hyperbolic_complex_reciprocal_separately_squares_factors() {
    text(call("IMCSCH", vec![t("1+i")]), "0.303931001628426-0.621518017170429i");
    text(call("IMSECH", vec![t("0.0001+1i")]), "1.85081566353496-0.000288247460169339i");
    text(call("IMCSCH", vec![t("1e-154+1e-155i")]), "1E+154-1E+153i");
    error(call("IMCSCH", vec![t("0+7.4583407312002e-155i")]), Num);
    text(call("IMCSCH", vec![t("0+7.45834073120021e-155i")]), "-1.34078079299426E+154i");
    text(call("IMCSCH", vec![t("0+7.458340883255e-155i")]), "-1.34078076565947E+154i");
}

/// dc-ymjf.2.139, Excel 16.0.20527.20196 (Touchstone, 2026-11-22): complex hyperbolics
/// (IMSINH, IMCOSH, IMCSCH, IMSECH) evaluate sinh/cosh via `em = EXP(-x)` and `1 / em`
/// for `|x| < 20` (with Taylor series `x + x^3/6 + x^5/120` for `|x| <= 0.01` and
/// `1 + x^2/2` for `|x| <= 0.0001`), `EXP(|x|) / 2` for `|x| >= 20`, and Excel's circular
/// reduction off-axis.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_hyperbolics_match_excel_neg_exp_and_cutoffs() {
    // 0.01 exact bit boundary for IMSINH/IMCSCH (<= 0.01 Taylor, > 0.01 (1/EXP(-x) - EXP(-x))/2).
    let b001 = 0.01_f64.to_bits();
    text(call("IMSINH", vec![n(f64::from_bits(b001))]), "0.0100001666675");
    text(call("IMCSCH", vec![n(f64::from_bits(b001))]), "99.9983333527776");
    text(call("IMSINH", vec![n(f64::from_bits(b001 + 1))]), "0.0100001666674999");
    text(call("IMCSCH", vec![n(f64::from_bits(b001 + 1))]), "99.9983333527784");
    text(call("IMSINH", vec![n(f64::from_bits(b001 + 2))]), "0.0100001666675001");
    text(call("IMCSCH", vec![n(f64::from_bits(b001 + 2))]), "99.9983333527767");

    // Taylor term ordering x + x^3/6 + x^5/120 (12/12 vs 0/12 factored).
    text(call("IMCSCH", vec![n(f64::from_bits(0x3f51_3e2e_bd7d_fff3))]), "950.187979483262");

    // 0.0001 cutoff for IMCOSH/IMSECH (<= 0.0001 uses 1 + x^2/2).
    text(call("IMCOSH", vec![n(f64::from_bits(0x3ed7_f1c1_1117_8ea4))]), "1.00000000001629");
    text(call("IMCOSH", vec![n(f64::from_bits(0x3f1a_36db_2ffc_2a11))]), "1.00000000499995");
    text(call("IMCOSH", vec![n(f64::from_bits(0x3f1a_3706_2303_69f7))]), "1.00000000500021");

    // 0.01 < |x| < 20 uses (1/EXP(-x) +- EXP(-x))/2.
    text(call("IMSINH", vec![n(f64::from_bits(0x3f85_0a53_0b77_0780))]), "0.0102737791934296");
    text(call("IMCSCH", vec![n(f64::from_bits(0x3f85_0a53_0b77_0780))]), "97.3351656846515");
    text(call("IMSINH", vec![n(f64::from_bits(0x3f87_9132_b2c7_3080))]), "0.0115076660051997");
    text(call("IMCSCH", vec![n(f64::from_bits(0x3f87_9132_b2c7_3080))]), "86.8985943412116");
    text(call("IMCOSH", vec![n(f64::from_bits(0x3ffa_daf5_dfdc_8320))]), "2.77197290065682");
    text(call("IMCOSH", vec![n(f64::from_bits(0x4030_30b0_e68d_b984))]), "5373833.6247373");
    text(call("IMSECH", vec![n(f64::from_bits(0x400e_2b70_4227_d160))]), "0.0460239667603411");

    // 20.0 boundary (< 20 uses 1/EXP(-x), >= 20 uses EXP(x)/2).
    text(call("IMSINH", vec![n(f64::from_bits(0x4033_ffff_ffff_fff1))]), "242582597.704882");
    text(call("IMSINH", vec![n(f64::from_bits(0x4034_0000_0000_000c))]), "242582597.704905");
    text(call("IMSINH", vec![n(f64::from_bits(0x4041_7e81_0b2e_7508))]), "783792881764184");
    text(call("IMCSCH", vec![n(f64::from_bits(0x4038_628a_34ef_59d7))]), "5.13798939901378E-11");

    // Off-axis uses the same hyperbolic helper and Excel's 2^27 circular reduction.
    text(call("IMSINH", vec![t("0.01+0.5i")]), "0.00877597188339536+0.479449510080894i");
    text(call("IMSINH", vec![t("0.010000000000001+0.5i")]), "0.00877597188339622+0.479449510080895i");
    text(call("IMSINH", vec![t("710+0.5i")]), "9.80257425068922E+307+5.35517072003032E+307i");
    text(call("IMSINH", vec![t("-709+1i")]), "-2.22021225102115E+307+3.45777571011361E+307i");
    text(call("IMCOSH", vec![t("-709+1i")]), "2.22021225102115E+307-3.45777571011361E+307i");
    text(call("IMSINH", vec![t("710+1i")]), "6.03516261727294E+307+9.39920887968891E+307i");
    error(call("IMSINH", vec![t("710+2i")]), Num);
    error(call("IMCOSH", vec![t("710-1i")]), Num);
    error(call("IMSINH", vec![t("1+134217728i")]), Num);
    error(call("IMCOSH", vec![t("1+134217728i")]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_high_j0_stores_its_leading_product() {
    for (x, order, expected) in [
        (108.43896102905273, 0.0, 0x3faa_3c33_7085_0555),
        (108.43896102905273, 2.0, 0xbfa9_b1ea_c889_83f1),
        (52.40333382782054, 0.0, 0x3f98_b5ce_293a_347d),
        (38.65895045944264, 1.0, 0x3f98_1035_45c9_d0a0),
    ] {
        exact(
            call("BESSELJ", vec![n(x), n(order)]),
            f64::from_bits(expected),
        );
    }
}

/// Excel 20527 full-bank cases distinguish the backward recurrence stores.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_backward_j_stores_its_recurrence() {
    for (x, order, expected) in [
        (0x3feb_9241_5272_776c, 0x4042_7fff_ffff_ffff, 0x3491_e664_d5a8_d495),
        (0x402f_ffff_ffff_ffff, 0x4059_0000_0000_0000, 0x31d3_f1f0_533b_5270),
        (0x4075_3f20_4cc0_563c, 0x4076_d000_0000_0000, 0x3f0e_284a_fd2a_41be),
    ] {
        exact(
            call("BESSELJ", vec![n(f64::from_bits(x)), n(f64::from_bits(order))]),
            f64::from_bits(expected),
        );
    }
}

/// Excel 20527 switches the two hyperbolic factors at different magnitudes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_cotangent_uses_signed_hyperbolic_factors() {
    for (input, expected) in [
        ("0-0.1i", "10.0333111322541i"),
        ("0.1-0.1i", "4.96671119559759+5.03337769295277i"),
        ("-0.1-0.1i", "-4.96671119559759+5.03337769295277i"),
        (
            "3.141592653589793-0.1i",
            "-3.22039035684448E-13+10.0333111322541i",
        ),
        ("6.339-0.095i", "4.57890469107014+7.856857549072i"),
        ("0+0.00005i", "-20000.0001548828i"),
        ("0+0.005i", "-200.001666664444i"),
        ("0+0.00500000000000035i", "-200.001666662682i"),
        (
            "-59177759.6817275-0.00649741821738437i",
            "6.16565874912588+0.253903609245275i",
        ),
    ] {
        text(call("IMCOT", vec![t(input)]), expected);
    }
}

/// Excel 20527 omits a subnormal real quotient and retains a normal quotient near underflow.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_cotangent_flushes_subnormal_real_quotients() {
    for (input, expected) in [
        ("0.1+354.0i", "-i"),
        ("3.141592653589793+340.0i", "-i"),
        ("0.7853981633974483+354.0i", "6.61510600727682E-308-i"),
    ] {
        text(call("IMCOT", vec![t(input)]), expected);
    }
}

/// At imaginary part 19, Excel 20527 rounds the denominator near the midpoint.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_cotangent_preserves_explicit_unit_coefficient_after_rounding() {
    for (input, expected) in [
        ("0.5+19.0i", "5.28297832393483E-17-i"),
        ("0.7853981633974483+19.0i", "6.27826558409606E-17-1i"),
        ("1+19.0i", "5.70881074054678E-17-1i"),
    ] {
        text(call("IMCOT", vec![t(input)]), expected);
    }
}

/// Excel 20527 omits a subnormal real quotient and retains a normal quotient near underflow.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_tangent_flushes_subnormal_real_quotients() {
    for (input, expected) in [
        ("0.1+354.0i", "i"),
        ("3.141592653589793+340.0i", "i"),
        ("0.7853981633974483+354.0i", "6.61510600727682E-308+i"),
    ] {
        text(call("IMTAN", vec![t(input)]), expected);
    }
}

/// Excel 20527 stores the hyperbolic sum before the circular subtraction.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_tangent_preserves_explicit_unit_coefficient_after_rounding() {
    for (input, expected) in [
        ("0+19.0i", "1i"),
        ("0.1+19.0i", "1.24729882214603E-17+1i"),
        ("0.5+19.0i", "5.28297832393483E-17+1i"),
        ("0.7853981633974483+19.0i", "6.27826558409606E-17+1i"),
        ("1+19.0i", "5.70881074054678E-17+i"),
    ] {
        text(call("IMTAN", vec![t(input)]), expected);
    }
}

/// Excel 20527 takes the signed imaginary limit before checking the circular
/// argument. The positive and negative boundaries differ and are both strict.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_tangent_and_cotangent_saturate_at_asymmetric_imaginary_limits() {
    for (input, tangent, cotangent) in [
        ("134217728+354.500000000001i", "i", "-i"),
        ("134217728-354.000000000001i", "-i", "i"),
        ("-1e275+1e280j", "j", "-j"),
        ("1.34-1e265i", "-i", "i"),
    ] {
        text(call("IMTAN", vec![t(input)]), tangent);
        text(call("IMCOT", vec![t(input)]), cotangent);
    }
    for input in ["134217728+354.5i", "134217728-354i"] {
        error(call("IMTAN", vec![t(input)]), Num);
        error(call("IMCOT", vec![t(input)]), Num);
    }
}

/// Excel 20527 Value2 controls and independent literal and complex-zero controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn impower_empty_text_and_zero_power() {
    for exponent in [n(0.0), n(1.0), e(Div0)] {
        error(call("IMPOWER", vec![t(""), exponent]), Num);
    }
    for base in [n(0.0), t("0i"), calc_func::value::CalcValue::empty()] {
        error(call("IMPOWER", vec![base, n(0.0)]), Num);
    }
    error(call("IMPOWER", vec![n(0.0), calc_func::value::CalcValue::empty()]), Num);
    error(call("IMPOWER", vec![n(0.0), e(Div0)]), Div0);
    text(call("IMPOWER", vec![n(0.0), n(0.5)]), "0");
    text(call("IMPOWER", vec![n(1.0), n(0.0)]), "1");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_functions() {
    // The original Excel reading is now matched by the published backward route.
    assert_excel_capture(
        call("BESSELJ", vec![n(1.9), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        15,
        1,
    );
    close(call("BESSELI", vec![n(1.5), n(1.0)]), 0.981666428, 1e-8);
    close(call("BESSELK", vec![n(1.5), n(1.0)]), 0.277387804, 1e-8);
    close(call("BESSELY", vec![n(2.5), n(1.0)]), 0.145918138, 1e-8);
    error(call("BESSELJ", vec![n(1.9), n(-1.0)]), Num);
}

/// Original dcztvv_r23/r24 workbook readings and R30 Value2/error-order controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn besselj_keeps_recorded_argument_kind_errors() {
    for logical in [false, true] {
        for order in [3.0, 50.0] {
            error(call("BESSELJ", vec![b(logical), n(order)]), Value);
        }
        for x in [1.1, 100.0] {
            error(call("BESSELJ", vec![n(x), b(logical)]), Value);
        }
    }
    for order in [3.0, 50.0] {
        error(call("BESSELJ", vec![missing(), n(order)]), NA);
    }
    for x in [1.1, 100.0] {
        error(call("BESSELJ", vec![n(x), missing()]), NA);
    }
    exact(
        call(
            "BESSELJ",
            vec![calc_func::value::CalcValue::empty(), n(3.0)],
        ),
        0.0,
    );
    exact(
        call(
            "BESSELJ",
            vec![n(1.1), calc_func::value::CalcValue::empty()],
        ),
        f64::from_bits(0x3fe7_0724_bfe3_f22a),
    );
    error(call("BESSELJ", vec![e(Div0), b(true)]), Div0);
    error(call("BESSELJ", vec![b(true), e(Div0)]), Value);
    error(call("BESSELJ", vec![b(true), n(-1.0)]), Value);
}

/// Excel 20527 cell and literal controls, with independent argument-error holdouts.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_family_rejects_logical_arguments_in_order() {
    for name in ["BESSELJ", "BESSELY", "BESSELI", "BESSELK"] {
        for logical in [false, true] {
            error(call(name, vec![b(logical), n(2.0)]), Value);
            error(call(name, vec![n(2.0), b(logical)]), Value);
            error(call(name, vec![b(logical), e(Div0)]), Value);
            error(call(name, vec![e(NA), b(logical)]), NA);
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_recorded_adjacent_phase_domain_boundaries() {
    // Original R19 Value2 readings, Excel build 20527, BESSELJ-4/BESSELY-3.
    // Each high endpoint and its predecessor were observed independently;
    // requested, submitted and read-back operands agree. These assertions
    // concern admission only; the remaining numeric J-body mismatches stay open.
    for (function, order, high, negative) in [
        ("BESSELJ", 0.0, 0x419fffffff243f6c_u64, false),
        ("BESSELJ", 0.0, 0x419fffffff243f6c, true),
        ("BESSELJ", 1.0, 0x41a0000002b65f21, false),
        ("BESSELJ", 1.0, 0x41a0000002b65f21, true),
        ("BESSELJ", 2.0, 0x419fffffff243f6c, false),
        ("BESSELJ", 2.0, 0x419fffffff243f6c, true),
        ("BESSELJ", 3.0, 0x419fffffff243f6c, false),
        ("BESSELJ", 4.0, 0x419fffffff243f6c, false),
        ("BESSELY", 0.0, 0x419fffffff243f6c, false),
        ("BESSELY", 1.0, 0x41a0000002b65f21, false),
        ("BESSELY", 2.0, 0x419fffffff243f6c, false),
    ] {
        let sign = if negative { -1.0 } else { 1.0 };
        let inside = sign * f64::from_bits(high - 1);
        let outside = sign * f64::from_bits(high);
        let value = call(function, vec![n(inside), n(order)]).unwrap();
        assert!(
            value.as_number().is_some_and(f64::is_finite),
            "{function}({inside},{order}) must remain numeric"
        );
        error(call(function, vec![n(outside), n(order)]), Num);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn besseli_recorded_adjacent_domain_boundary() {
    // R16 Value2 readings, excel-20527/r16-BESSELI-2.json: 018..029,
    // 054..059. The submitted and read-back binary64 operands agree.
    let boundary = 709.7_f64;
    let outside = f64::from_bits(boundary.to_bits() + 1);
    for order in [0.0, 1.0, 2.0, 3.0, 10.0, 100.0] {
        let value = call("BESSELI", vec![n(boundary), n(order)]).unwrap();
        assert!(
            matches!(value.core(), calc_func::value::CoreValue::Number(_)),
            "BESSELI({boundary}, {order}) must remain numeric: {value:?}"
        );
        error(call("BESSELI", vec![n(outside), n(order)]), Num);
    }
    for order in [0.0, 1.0, 2.0] {
        error(call("BESSELI", vec![n(-outside), n(order)]), Num);
    }
    exact(
        call("BESSELI", vec![n(boundary), n(0.0)]),
        f64::from_bits(0x7f8c_3d51_558e_e541),
    );
    exact(
        call("BESSELI", vec![n(-boundary), n(1.0)]),
        f64::from_bits(0xff8c_3839_94e0_48cc),
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn besseli_higher_orders_use_the_same_normalization_as_order_zero() {
    // Original catalogue dcj9k_bessel/engineering readings. These cover both
    // I0 approximation regions, negative arguments, and truncated order.
    for (x, order, expected) in [
        (2.0, 2.0, 0x3fe6_0bdd_9e3d_e5e0),
        (2.5, 5.0, 0x3fa0_d0dc_29ee_9cbe),
        (3.75, 3.0, 0x4003_d534_6d07_25af),
        (3.9, 10.0, 0x3f34_303a_16de_a877),
        (5.2, 10.0, 0x3f7d_06aa_c7a3_8c8e),
        (8.0, 2.0, 0x4074_7988_8e58_37d5),
        (-1.7, 3.0, 0xbfbf_4aa3_956a_9c7d),
        (-9.1, 2.0, 0x408d_c4a7_358f_8c59),
        (1.5, 2.9, 0x3fd5_9f15_1a07_2f72),
    ] {
        exact(
            call("BESSELI", vec![n(x), n(order)]),
            f64::from_bits(expected),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn besseli_recorded_normalization_underflow_returns_zero() {
    // Original R19 Value2 readings r19-endpoints-besseli-046/047 are zero.
    // The normal mathematical series value remains a control in bessel.rs;
    // the application must reproduce the captured Excel result.
    exact(call("BESSELI", vec![n(709.7), n(1570.0)]), 0.0);
    exact(call("BESSELI", vec![n(-709.7), n(1570.0)]), 0.0);
    exact(call("BESSELI", vec![n(5.0), n(10_000_000.0)]), 0.0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn delta_gestep_and_convert() {
    assert_excel_capture(
        call("DELTA", vec![n(5.0), n(4.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        67,
        1,
    );
    exact(call("DELTA", vec![n(5.0), n(5.0)]), 1.0);
    exact(call("DELTA", vec![n(0.0)]), 1.0);
    assert_excel_capture(
        call("GESTEP", vec![n(5.0), n(4.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        103,
        1,
    );
    exact(call("GESTEP", vec![n(-4.0)]), 0.0);
    close(
        call("CONVERT", vec![n(1.0), t("lbm"), t("kg")]),
        0.45359237,
        1e-12,
    );
    close(call("CONVERT", vec![n(68.0), t("F"), t("C")]), 20.0, 1e-12);
    error(call("CONVERT", vec![n(2.0), t("gal"), t("mi")]), NA);
}

/// Excel 20527's low Y0 seed just below eight and the order-two result at 1.5.
/// The latter still uses the usual forward recurrence and truncates order 2.9.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessely_low_seed_staging_matches_the_remaining_corpus_examples() {
    for (x, order, expected) in [
        (f64::from_bits(0x401f_ffff_ffff_ffff), 0.0, 0x3fcc_9c5a_10ee_6a8c),
        (1.5, 2.9, 0xbfed_d488_026d_9901),
        (f64::from_bits(0x401f_8e4e_1a5a_4f81), 0.0, 0x3fca_2d24_05bb_0e91),
    ] {
        let result = call("BESSELY", vec![n(x), n(order)]).unwrap();
        assert_eq!(result.as_number().unwrap().to_bits(), expected, "BESSELY({x}, {order})");
    }
}

/// The low J0 seed is also consumed by Y0. This Excel 20527 input
/// distinguishes the rounded Horner multiplications from binary64 alone.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn besselj_low_seed_staging_matches_excel() {
    let x = f64::from_bits(0x401f_8e4e_1a5a_4f81);
    let result = call("BESSELJ", vec![n(x), n(0.0)]).unwrap();
    assert_eq!(result.as_number().unwrap().to_bits(), 0x3fc9_2fbc_1c0b_efbc);
}

/// The low order-one seeds use stored operations through the rational
/// quotient and the logarithmic correction. Excel 20527, exact inputs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_order_one_seed_staging_matches_excel() {
    let x = f64::from_bits(0x401e_a32d_8a6b_7a56);
    let actual = ["BESSELJ", "BESSELY"].map(|function| {
        let result = call(function, vec![n(x), n(1.0)]).unwrap();
        result.as_number().unwrap().to_bits()
    });
    assert_eq!(actual, [0x3fc6_16b4_a079_a19e, 0xbfcd_b4bd_82a6_e1f7]);
}

/// Excel 20527 controls that distinguish stores in the shared forward recurrence.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_forward_stores_each_recurrence_operation() {
    for (function, x, order, expected) in [
        ("BESSELY", f64::from_bits(1.0_f64.to_bits() - 1), 10.0, 0xc19c_fef9_7b53_aa46),
        ("BESSELY", 479.0, 365.0, 0x3fa4_c801_8aad_87d2),
        ("BESSELY", 25_569.0, 5_569.0, 0xbf50_989b_ad6b_5918),
        ("BESSELJ", f64::from_bits(16.0_f64.to_bits() - 1), 10.0, 0xbfca_64f2_b129_529b),
    ] {
        let result = call(function, vec![n(x), n(order)]).unwrap();
        assert_eq!(result.as_number().unwrap().to_bits(), expected, "{function}({x}, {order})");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_low_rational_seeds_match_recorded_excel_bits() {
    // Existing catalogue capture dc96l3_bessel, Excel 16.0.20527.20088.
    // Original answers and operand bits are preserved; this is the invoke route.
    for (function, x, order, expected) in [
        (
            "BESSELY",
            0x4004000000000000,
            0x3ff0000000000000,
            0x3fc2ad720e3ee754,
        ),
        (
            "BESSELJ",
            0x3fd3333333333333,
            0x0000000000000000,
            0x3fef48b6d783ca43,
        ),
        (
            "BESSELJ",
            0x3fd3333333333333,
            0x3ff0000000000000,
            0x3fc2fc1c68ab31f5,
        ),
        (
            "BESSELJ",
            0x3ff199999999999a,
            0x0000000000000000,
            0x3fe70724bfe3f22a,
        ),
        (
            "BESSELJ",
            0x3ff199999999999a,
            0x3ff0000000000000,
            0x3fde2343cc4f390e,
        ),
        (
            "BESSELJ",
            0x4000000000000000,
            0x0000000000000000,
            0x3fcca87401162af6,
        ),
        (
            "BESSELJ",
            0x4000000000000000,
            0x3ff0000000000000,
            0x3fe2748795969b5b,
        ),
        (
            "BESSELJ",
            0x4004000000000000,
            0x0000000000000000,
            0xbfa8c5c21daa73f8,
        ),
        (
            "BESSELJ",
            0x4004000000000000,
            0x3ff0000000000000,
            0x3fdfd063c8529f24,
        ),
        (
            "BESSELJ",
            0x400e000000000000,
            0x0000000000000000,
            0xbfd9b0a303f8be57,
        ),
        (
            "BESSELJ",
            0x400e000000000000,
            0x3ff0000000000000,
            0x3fa1036fef5a6608,
        ),
        (
            "BESSELJ",
            0x400f333333333333,
            0x0000000000000000,
            0xbfd9b7847390a92a,
        ),
        (
            "BESSELJ",
            0x400f333333333333,
            0x3ff0000000000000,
            0xbf9be5dc8becdfb4,
        ),
        (
            "BESSELJ",
            0x4014cccccccccccd,
            0x0000000000000000,
            0xbfbc3bfe8f50de92,
        ),
        (
            "BESSELJ",
            0x4014cccccccccccd,
            0x3ff0000000000000,
            0xbfd5f75da16eefa8,
        ),
        (
            "BESSELJ",
            0x401ecccccccccccd,
            0x0000000000000000,
            0x3fce0608ac6a480b,
        ),
        (
            "BESSELJ",
            0x401ecccccccccccd,
            0x3ff0000000000000,
            0x3fc73541424d9cb2,
        ),
        (
            "BESSELJ",
            0xbffb333333333333,
            0x0000000000000000,
            0x3fd978957e320ded,
        ),
        (
            "BESSELJ",
            0xbffb333333333333,
            0x3ff0000000000000,
            0xbfe27d0d82ccb28e,
        ),
        (
            "BESSELY",
            0x3fd3333333333333,
            0x0000000000000000,
            0xbfe9d52f68d1fa7a,
        ),
        (
            "BESSELY",
            0x3fd3333333333333,
            0x3ff0000000000000,
            0xc0025847819115ec,
        ),
        (
            "BESSELY",
            0x3ff199999999999a,
            0x0000000000000000,
            0x3fc4c1c3996c05ca,
        ),
        (
            "BESSELY",
            0x3ff199999999999a,
            0x3ff0000000000000,
            0xbfe656fed4bf15cf,
        ),
        (
            "BESSELY",
            0x4000000000000000,
            0x0000000000000000,
            0x3fe054ff59a9e8c9,
        ),
        (
            "BESSELY",
            0x4000000000000000,
            0x3ff0000000000000,
            0xbfbb667a398a2868,
        ),
        (
            "BESSELY",
            0x4004000000000000,
            0x0000000000000000,
            0x3fdfe0627f28ac71,
        ),
        (
            "BESSELY",
            0x400e000000000000,
            0x0000000000000000,
            0x3fb5d362f8d268e4,
        ),
        (
            "BESSELY",
            0x400e000000000000,
            0x3ff0000000000000,
            0x3fda9d981445d3dc,
        ),
        (
            "BESSELY",
            0x400f333333333333,
            0x0000000000000000,
            0x3f97efda52684810,
        ),
        (
            "BESSELY",
            0x400f333333333333,
            0x3ff0000000000000,
            0x3fda19b92227a346,
        ),
        (
            "BESSELY",
            0x4014cccccccccccd,
            0x0000000000000000,
            0xbfd533371efa8668,
        ),
        (
            "BESSELY",
            0x4014cccccccccccd,
            0x3ff0000000000000,
            0x3fb445d16d3cb0e8,
        ),
        (
            "BESSELY",
            0x401ecccccccccccd,
            0x0000000000000000,
            0x3fc538fcd0fec429,
        ),
        (
            "BESSELY",
            0x401ecccccccccccd,
            0x3ff0000000000000,
            0xbfccb677d633cddc,
        ),
        (
            "BESSELJ",
            0x0000000000000000,
            0x0000000000000000,
            0x3ff0000000c292b1,
        ),
        (
            "BESSELJ",
            0x0000000000000000,
            0x3ff0000000000000,
            0x0000000000000000,
        ),
        (
            "BESSELJ",
            0x01a56e1fc2f8f359,
            0x3ff0000000000000,
            0x01956e1fc306f12b,
        ),
        (
            "BESSELY",
            0x01a56e1fc2f8f359,
            0x3ff0000000000000,
            0xfe2e6b6f21c268b8,
        ),
    ] {
        let x = f64::from_bits(x);
        let order = f64::from_bits(order);
        let actual = call(function, vec![n(x), n(order)]).unwrap();
        let calc_func::value::CoreValue::Number(actual) = actual.core() else {
            panic!("{function}({x}, {order}) returned {actual:?}");
        };
        assert_eq!(actual.to_bits(), expected, "{function}({x}, {order})");
    }
    assert_excel_capture(
        call("BESSELY", vec![n(0.0), n(0.0)]),
        include_bytes!("captures/catalogue/sections/dc96l3_bessel.capture.json"),
        "dc96l3_bessel",
        320,
        4,
    );
    assert_excel_capture(
        call("BESSELY", vec![n(0.0), n(1.0)]),
        include_bytes!("captures/catalogue/sections/dc96l3_bessel.capture.json"),
        "dc96l3_bessel",
        321,
        4,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_published_backward_and_high_y1_match_recorded_excel_bits() {
    // Unchanged catalogue dc96l3_bessel answers and existing R19 J/Y banks.
    // This table preserves exact operands and Excel answers through invoke.
    for (function, x, order, expected) in [
        (
            "BESSELJ",
            0x3ffe666666666666,
            0x4000000000000000,
            0x3fd51d81390f83eb,
        ),
        (
            "BESSELJ",
            0x3fd3333333333333,
            0x4000000000000000,
            0x3f86de209f391983,
        ),
        (
            "BESSELJ",
            0x3fd3333333333333,
            0x4008000000000000,
            0x3f42541c0c752fd9,
        ),
        (
            "BESSELJ",
            0x3fd3333333333333,
            0x4014000000000000,
            0x3ea5277765c0bf87,
        ),
        (
            "BESSELJ",
            0x3fd3333333333333,
            0x4024000000000000,
            0x3cdc916d1bb1a8f2,
        ),
        (
            "BESSELJ",
            0x3ff199999999999a,
            0x4000000000000000,
            0x3fc17aef27ec916f,
        ),
        (
            "BESSELJ",
            0x3ff199999999999a,
            0x4008000000000000,
            0x3f9a4faaa04edc20,
        ),
        (
            "BESSELJ",
            0x3ff199999999999a,
            0x4014000000000000,
            0x3f3a213de89794a0,
        ),
        (
            "BESSELJ",
            0x3ff199999999999a,
            0x4024000000000000,
            0x3e07550f46fef591,
        ),
        (
            "BESSELJ",
            0x4000000000000000,
            0x4000000000000000,
            0x3fd694d5ed8bbdd6,
        ),
        (
            "BESSELJ",
            0x4000000000000000,
            0x4008000000000000,
            0x3fc0813660d7b6fa,
        ),
        (
            "BESSELJ",
            0x4000000000000000,
            0x4014000000000000,
            0x3f7cd596393d19ff,
        ),
        (
            "BESSELJ",
            0x4000000000000000,
            0x4024000000000000,
            0x3e90e16696236e1b,
        ),
        (
            "BESSELJ",
            0x4004000000000000,
            0x4008000000000000,
            0x3fcbb98fdf2dd19a,
        ),
        (
            "BESSELJ",
            0x4004000000000000,
            0x4014000000000000,
            0x3f93f83be8afa94c,
        ),
        (
            "BESSELJ",
            0x4004000000000000,
            0x4024000000000000,
            0x3ec2a99161bcc46b,
        ),
        (
            "BESSELJ",
            0x400e000000000000,
            0x4014000000000000,
            0x3fbac6c6177fcf64,
        ),
        (
            "BESSELJ",
            0x400e000000000000,
            0x4024000000000000,
            0x3f1c0f2c434a1438,
        ),
        (
            "BESSELJ",
            0x400f333333333333,
            0x4014000000000000,
            0x3fbee75c306e54d3,
        ),
        (
            "BESSELJ",
            0x400f333333333333,
            0x4024000000000000,
            0x3f24379ee1e85100,
        ),
        (
            "BESSELJ",
            0x4014cccccccccccd,
            0x4024000000000000,
            0x3f60f31f5e704531,
        ),
        (
            "BESSELJ",
            0x401ecccccccccccd,
            0x4024000000000000,
            0x3fa8034ca8683f6a,
        ),
        (
            "BESSELJ",
            0x4020000000000000,
            0x4024000000000000,
            0x3faf1cdb1157547d,
        ),
        (
            "BESSELJ",
            0x4020cccccccccccd,
            0x4024000000000000,
            0x3fb54919429574dd,
        ),
        (
            "BESSELJ",
            0xbffb333333333333,
            0x4000000000000000,
            0x3fd20802e4c24e8e,
        ),
        (
            "BESSELJ",
            0xbffb333333333333,
            0x4008000000000000,
            0xbfb5cc62b7b46ca2,
        ),
        (
            "BESSELY",
            0x4020000000000000,
            0x3ff0000000000000,
            0xbfc43b53412fb1a7,
        ),
        (
            "BESSELY",
            0x4020cccccccccccd,
            0x3ff0000000000000,
            0xbfab62524eac4415,
        ),
        (
            "BESSELY",
            0x402899999999999a,
            0x3ff0000000000000,
            0xbfbe73670d39f087,
        ),
        (
            "BESSELY",
            0x403999999999999a,
            0x3ff0000000000000,
            0xbfc3449d67ecca79,
        ),
        (
            "BESSELJ",
            0x3ff8000000000000,
            0x4007333333333333,
            0x3fcdb50c8f979d2f,
        ),
        (
            "BESSELJ",
            0x3ddb7cdfd9d7bdbb,
            0x4000000000000000,
            0x3b979ca10c924224,
        ),
        (
            "BESSELY",
            0x41a0000000000000,
            0x3ff0000000000000,
            0x3ed7ff19f84f233c,
        ),
        (
            "BESSELJ",
            0x4014000000000000,
            0x416312d000000000,
            0x0000000000000000,
        ),
        (
            "BESSELJ",
            0x4020000000000000,
            0x4030000000000000,
            0x3f1472ebab6dc571,
        ),
        (
            "BESSELJ",
            0x4020000000000000,
            0x403f000000000000,
            0x3cb86ac063ddc7eb,
        ),
        (
            "BESSELJ",
            0x4020000000000000,
            0x404f800000000000,
            0x35a8fe5ab37b6cae,
        ),
        (
            "BESSELJ",
            0x4020800000000000,
            0x4024000000000000,
            0x3fb300d5dbaeb63b,
        ),
        (
            "BESSELJ",
            0x4020800000000000,
            0x4030000000000000,
            0x3f1f676a4bb3d2ba,
        ),
        (
            "BESSELJ",
            0x4020800000000000,
            0x403f000000000000,
            0x3cceaf9805c733c6,
        ),
        (
            "BESSELJ",
            0x4020800000000000,
            0x404f800000000000,
            0x35d55e0390366b87,
        ),
        (
            "BESSELJ",
            0x4021800000000000,
            0x4024000000000000,
            0x3fbb36a30de05451,
        ),
        (
            "BESSELJ",
            0x4021800000000000,
            0x4030000000000000,
            0x3f319d6978ce0ddd,
        ),
        (
            "BESSELJ",
            0x4021800000000000,
            0x403f000000000000,
            0x3cf6376c60480333,
        ),
        (
            "BESSELJ",
            0x4021800000000000,
            0x404f800000000000,
            0x362a4ddb6857c14f,
        ),
        (
            "BESSELJ",
            0x4022000000000000,
            0x4024000000000000,
            0x3fbfebf3bab3bc5b,
        ),
        (
            "BESSELJ",
            0x4022000000000000,
            0x4030000000000000,
            0x3f39c67e8d46212a,
        ),
        (
            "BESSELJ",
            0x4022000000000000,
            0x403f000000000000,
            0x3d09ada4beb68e5f,
        ),
        (
            "BESSELJ",
            0x4022000000000000,
            0x404f800000000000,
            0x36530f8f43c15be1,
        ),
        (
            "BESSELJ",
            0x4023000000000000,
            0x4024000000000000,
            0x3fc51f95d1a35329,
        ),
        (
            "BESSELJ",
            0x4023000000000000,
            0x4030000000000000,
            0x3f4a6cb36b1cabb6,
        ),
        (
            "BESSELJ",
            0x4023000000000000,
            0x403f000000000000,
            0x3d2fde8d1278a8e4,
        ),
        (
            "BESSELJ",
            0x4023000000000000,
            0x404f800000000000,
            0x36a151b1f68cd4e8,
        ),
        (
            "BESSELJ",
            0x4024000000000000,
            0x4024000000000000,
            0x3fca8ee79d2f09d3,
        ),
        (
            "BESSELJ",
            0x4024000000000000,
            0x4030000000000000,
            0x3f59ab73a6aa8562,
        ),
        (
            "BESSELJ",
            0x4024000000000000,
            0x403f000000000000,
            0x3d521244b37a53a8,
        ),
        (
            "BESSELJ",
            0x4024000000000000,
            0x404f800000000000,
            0x36ea5fd8b94e7c49,
        ),
        (
            "BESSELJ",
            0x4028000000000000,
            0x4030000000000000,
            0x3f8ca786ad8a55b4,
        ),
        (
            "BESSELJ",
            0x4028000000000000,
            0x403f000000000000,
            0x3dcc37f894eee87b,
        ),
        (
            "BESSELJ",
            0x4028000000000000,
            0x404f800000000000,
            0x37f07a3da3c055db,
        ),
        (
            "BESSELJ",
            0x402a000000000000,
            0x4030000000000000,
            0x3fa0c14d294d1bf8,
        ),
        (
            "BESSELJ",
            0x402a000000000000,
            0x403f000000000000,
            0x3e013639eb600d3a,
        ),
        (
            "BESSELJ",
            0x402a000000000000,
            0x404f800000000000,
            0x3862110bc762a44d,
        ),
        (
            "BESSELJ",
            0x4030000000000000,
            0x4030000000000000,
            0x3fc6b6c94748fc85,
        ),
        (
            "BESSELJ",
            0x4030000000000000,
            0x403f000000000000,
            0x3e847b19da378365,
        ),
        (
            "BESSELJ",
            0x4030000000000000,
            0x404f800000000000,
            0x3987705b6dcbff24,
        ),
        (
            "BESSELJ",
            0x4034000000000000,
            0x403f000000000000,
            0x3f07a2e88ef734b9,
        ),
        (
            "BESSELJ",
            0x4034000000000000,
            0x404f800000000000,
            0x3ac00bbb63905601,
        ),
        (
            "BESSELJ",
            0x4039000000000000,
            0x403f000000000000,
            0x3f781eed27ab5faf,
        ),
        (
            "BESSELJ",
            0x4039000000000000,
            0x404f800000000000,
            0x3bef7880f465f2ec,
        ),
        (
            "BESSELJ",
            0x403f000000000000,
            0x403f000000000000,
            0x3fc2393fcd78b7be,
        ),
        (
            "BESSELJ",
            0x403f000000000000,
            0x404f800000000000,
            0x3d0729f4256bffbd,
        ),
        (
            "BESSELJ",
            0x4040000000000000,
            0x404f800000000000,
            0x3d3074098ea667c4,
        ),
        (
            "BESSELJ",
            0x4044000000000000,
            0x404f800000000000,
            0x3e3b4423422ea97f,
        ),
        (
            "BESSELJ",
            0x4048000000000000,
            0x404f800000000000,
            0x3efaecc06a9efbcd,
        ),
        (
            "BESSELY",
            0x419ffffffc000000,
            0x3ff0000000000000,
            0x3f0fe64bf9c29c37,
        ),
        (
            "BESSELY",
            0x419ffffffe000000,
            0x3ff0000000000000,
            0x3f03e239764b5875,
        ),
        (
            "BESSELY",
            0x419fffffffffffff,
            0x3ff0000000000000,
            0x3ed7ff1a40469a0f,
        ),
        (
            "BESSELY",
            0x41a0000000000001,
            0x3ff0000000000000,
            0x3ed7ff1968603592,
        ),
        (
            "BESSELY",
            0x41a0000002000000,
            0x3ff0000000000000,
            0xbf0ca8838551ac39,
        ),
    ] {
        let x = f64::from_bits(x);
        let order = f64::from_bits(order);
        let actual = call(function, vec![n(x), n(order)]).unwrap();
        let calc_func::value::CoreValue::Number(actual) = actual.core() else {
            panic!("{function}({x}, {order}) returned {actual:?}");
        };
        assert_eq!(actual.to_bits(), expected, "{function}({x}, {order})");
    }
    // Existing Excel-labelled unit-test reading; original raw capture not located.
    exact(
        call("BESSELY", vec![n(100.0), n(1.0)]),
        f64::from_bits(0xbf94dc7ab5ff838f),
    );
    // Signed counterparts use Bessel parity, not newly captured readings.
    exact(
        call("BESSELJ", vec![n(-1.9), n(2.0)]),
        f64::from_bits(0x3fd51d81390f83eb),
    );
    exact(
        call("BESSELJ", vec![n(-0.3), n(3.0)]),
        -f64::from_bits(0x3f42541c0c752fd9),
    );
    assert_excel_capture(
        call("BESSELJ", vec![n(0.0), n(2.0)]),
        include_bytes!("captures/catalogue/sections/dc96l3_bessel.capture.json"),
        "dc96l3_bessel",
        311,
        4,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_high_y0_matches_recorded_excel_bits() {
    // Original catalogue dc96l3_bessel Value2 answers, unchanged.
    for (x, order, expected) in [
        (8.0, 0.0, 0x3fcc9c5a2723ea8b),
        (8.0, 2.0, 0xbfd0d5977bb7eb7a),
        (8.0, 3.0, 0x3f9b2dde2bbe3168),
        (8.4, 0.0, 0x3fd109c77154134d),
        (8.4, 2.0, 0xbfd1da6b6a86eaa9),
        (12.3, 0.0, 0xbfc96b7f9f5cfbe7),
        (25.6, 0.0, 0xbfa9a0803136e92f),
        (25.6, 10.0, 0xbfc4f5b48e80a043),
        (100.0, 50.0, 0x3fb395d956a51e83),
        (100000.0, 100.0, 0x3f5ccfd3e73ac451),
    ] {
        exact(
            call("BESSELY", vec![n(x), n(order)]),
            f64::from_bits(expected),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_itk_i_matches_recorded_excel_bits() {
    // Original catalogue and six w111extended bank answers, unchanged.
    // The latter distinguish strict arithmetic, the recurrence and the I0 helper.
    for (x, order, expected) in [
        (0x3ff8000000000000, 0x3ff0000000000000, 0x3fef69cfb6bc3907),
        (0x3fd3333333333333, 0x0000000000000000, 0x3ff05cadfd772c5b),
        (0x3fd3333333333333, 0x3ff0000000000000, 0x3fc36ab427bf7f0d),
        (0x3fd3333333333333, 0x4000000000000000, 0x3f873699f0fa6a4b),
        (0x3fd3333333333333, 0x4008000000000000, 0x3f4289319e1d41e3),
        (0x3fd3333333333333, 0x4014000000000000, 0x3ea5503c2ad9ea15),
        (0x3fd3333333333333, 0x4024000000000000, 0x3cdcaf67ed0c0be6),
        (0x3ff199999999999a, 0x0000000000000000, 0x3ff537f3b7d4a6aa),
        (0x3ff199999999999a, 0x3ff0000000000000, 0x3fe4664f0f352628),
        (0x3ff199999999999a, 0x4000000000000000, 0x3fc563304988da68),
        (0x3ff199999999999a, 0x4008000000000000, 0x3f9e9baab6102950),
        (0x3ff199999999999a, 0x4014000000000000, 0x3f3ce6f0f1d374dd),
        (0x3ff199999999999a, 0x4024000000000000, 0x3e08a6c7fa31aa71),
        (0x4000000000000000, 0x0000000000000000, 0x40023c9738ba3a74),
        (0x4000000000000000, 0x3ff0000000000000, 0x3ff9733fa21c214c),
        (0x4000000000000000, 0x4000000000000000, 0x3fe60bdd9e3de5e0),
        (0x4000000000000000, 0x4008000000000000, 0x3fcb3b1020bce0b5),
        (0x4000000000000000, 0x4014000000000000, 0x3f841f7c5b90b3e8),
        (0x4000000000000000, 0x4024000000000000, 0x3e943f1abfcf579f),
        (0x4004000000000000, 0x0000000000000000, 0x400a519733342993),
        (0x4004000000000000, 0x3ff0000000000000, 0x4004223c20067cd1),
        (0x4004000000000000, 0x4000000000000000, 0x3ff46c67c769054a),
        (0x4004000000000000, 0x4008000000000000, 0x3fde5c15b85c636b),
        (0x4004000000000000, 0x4014000000000000, 0x3fa0d0dc29ee9cbe),
        (0x4004000000000000, 0x4024000000000000, 0x3ec8cb693f9d11cb),
        (0x400e000000000000, 0x0000000000000000, 0x40223ce67d42a522),
        (0x400e000000000000, 0x3ff0000000000000, 0x401f1ebc4ab5d54b),
        (0x400e000000000000, 0x4000000000000000, 0x4013e0dfff01cb62),
        (0x400e000000000000, 0x4008000000000000, 0x4003d5346d0725af),
        (0x400e000000000000, 0x4014000000000000, 0x3fd5b6e93222147d),
        (0x400e000000000000, 0x4024000000000000, 0x3f2a97dcd155e20d),
        (0x400f333333333333, 0x0000000000000000, 0x4024bce80e0ed899),
        (0x400f333333333333, 0x3ff0000000000000, 0x4021d358e4503aca),
        (0x400f333333333333, 0x4000000000000000, 0x40173174f39bc785),
        (0x400f333333333333, 0x4008000000000000, 0x4007b9fd7669b7af),
        (0x400f333333333333, 0x4014000000000000, 0x3fdb9cd39a1ada03),
        (0x400f333333333333, 0x4024000000000000, 0x3f34303a16dea877),
        (0x4014cccccccccccd, 0x0000000000000000, 0x40404ab32d44b98a),
        (0x4014cccccccccccd, 0x3ff0000000000000, 0x403d411a69468826),
        (0x4014cccccccccccd, 0x4000000000000000, 0x403554f9b5e9a17b),
        (0x4014cccccccccccd, 0x4008000000000000, 0x4029b0b4c616a417),
        (0x4014cccccccccccd, 0x4014000000000000, 0x4006a15310058d91),
        (0x4014cccccccccccd, 0x4024000000000000, 0x3f7d06aac7a38c8e),
        (0x401ecccccccccccd, 0x0000000000000000, 0x4074316673d8b494),
        (0x401ecccccccccccd, 0x3ff0000000000000, 0x4072d4ff6e0297e5),
        (0x401ecccccccccccd, 0x4000000000000000, 0x406e9a62ce06ad92),
        (0x401ecccccccccccd, 0x4008000000000000, 0x4065c42f4c920204),
        (0x401ecccccccccccd, 0x4014000000000000, 0x404e5ec51e47adc4),
        (0x401ecccccccccccd, 0x4024000000000000, 0x3fe6b5860c3334c5),
        (0x4020000000000000, 0x0000000000000000, 0x407ab906a843d31a),
        (0x4020000000000000, 0x3ff0000000000000, 0x4078fdf85c2f64c5),
        (0x4020000000000000, 0x4000000000000000, 0x407479888e5837d5),
        (0x4020000000000000, 0x4008000000000000, 0x406d82684104a253),
        (0x4020000000000000, 0x4014000000000000, 0x4055624aaa2f58cf),
        (0x4020000000000000, 0x4024000000000000, 0x3ff25472f294f19a),
        (0x4020cccccccccccd, 0x0000000000000000, 0x40836f8d874731d8),
        (0x4020cccccccccccd, 0x3ff0000000000000, 0x40823d418fc196ec),
        (0x4020cccccccccccd, 0x4000000000000000, 0x407e2fa67acd89cf),
        (0x4020cccccccccccd, 0x4008000000000000, 0x40761aadda50a1f4),
        (0x4020cccccccccccd, 0x4014000000000000, 0x4060c8e84e4dffa2),
        (0x4020cccccccccccd, 0x4024000000000000, 0x4001100add69d30d),
        (0x402899999999999a, 0x0000000000000000, 0x40d8aa5f276c6544),
        (0x402899999999999a, 0x3ff0000000000000, 0x40d7a3fbc92ee813),
        (0x402899999999999a, 0x4000000000000000, 0x40d4d24f35720616),
        (0x402899999999999a, 0x4008000000000000, 0x40d0de8c9223eb3f),
        (0x402899999999999a, 0x4014000000000000, 0x40c15baacd896ec6),
        (0x402899999999999a, 0x4024000000000000, 0x407c9a554a0c3240),
        (0x403999999999999a, 0x0000000000000000, 0x42035d800732e295),
        (0x403999999999999a, 0x3ff0000000000000, 0x4202fbb02fd33fca),
        (0x403999999999999a, 0x4000000000000000, 0x4201e1d64369889b),
        (0x403999999999999a, 0x4008000000000000, 0x42003066e6936431),
        (0x403999999999999a, 0x4014000000000000, 0x41f792bef8254dc1),
        (0x403999999999999a, 0x4024000000000000, 0x41d5af846f57a917),
        (0xbffb333333333333, 0x0000000000000000, 0x3ffdd2ccf0c53a63),
        (0xbffb333333333333, 0x3ff0000000000000, 0xbff3243c4cadc562),
        (0xbffb333333333333, 0x4000000000000000, 0x3fdd3745241cba94),
        (0xbffb333333333333, 0x4008000000000000, 0xbfbf4aa3956a9c7d),
        (0xc022333333333333, 0x0000000000000000, 0x4092c6f05346f245),
        (0xc022333333333333, 0x3ff0000000000000, 0xc091b695d4730e10),
        (0xc022333333333333, 0x4000000000000000, 0x408dc4a7358f8c59),
        (0xc022333333333333, 0x4008000000000000, 0xc086576eeb47db74),
        (0x3ff8000000000000, 0x4007333333333333, 0x3fd59f151a072f72),
        (0x0000000000000000, 0x0000000000000000, 0x3ff0000000000000),
        (0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000),
        (0x0000000000000000, 0x4000000000000000, 0x0000000000000000),
        (0x01a56e1fc2f8f359, 0x3ff0000000000000, 0x01956e1fc2f8f359),
        (0x3ddb7cdfd9d7bdbb, 0x4000000000000000, 0x3b979ca10c924223),
        (0x4059000000000000, 0x4049000000000000, 0x478d056635fca6da),
        (0x40862d999999999a, 0x0000000000000000, 0x7f8c3d51558ee541),
        (0x3ffddbc50e25491c, 0x401c30cde189f75e, 0x3f21d7fd6fe380e0),
        (0x400a3164a4059a50, 0x40310694bb9f5604, 0x3daf3c93d805a673),
        (0x4015bae9d88a54dd, 0x401feafe01ffd6d6, 0x3fe0b33559c62928),
        (0x401a4f5bcf6959f0, 0x4030d0c272565d18, 0x3ef18b061ac42404),
        (0x40290760dd512b66, 0x400fe62ee82134c8, 0x40d4dc000ce65cba),
        (0x402af083a685968c, 0x40253c1ed2043b18, 0x409e95e8e2cdac05),
    ] {
        let x = f64::from_bits(x);
        let order = f64::from_bits(order);
        let actual = call("BESSELI", vec![n(x), n(order)]).unwrap();
        let calc_func::value::CoreValue::Number(actual) = actual.core() else {
            panic!("BESSELI({x}, {order}) returned {actual:?}");
        };
        assert_eq!(actual.to_bits(), expected, "BESSELI({x}, {order})");
    }
    for (x, order) in [
        (0x3ff0000000000000, 0xbff0000000000000),
        (0x3ff0000000000000, 0xbfe0000000000000),
        (0x4014000000000000, 0x416312d020000000),
        (0x40f86a0000000000, 0x4059000000000000),
        (0x40862d9999a70593, 0x0000000000000000),
    ] {
        error(
            call(
                "BESSELI",
                vec![n(f64::from_bits(x)), n(f64::from_bits(order))],
            ),
            Num,
        );
    }
}

/// Original Excel build20430, w111extended-2026092903-BESSELK-00698,
/// runs/w111-extended-numeric-20260929/answers/answers-besselk.json.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_k_original_answer_uses_stored_pc64() {
    exact(
        call(
            "BESSELK",
            vec![
                n(f64::from_bits(0x3ffd840e4ca60c0e)),
                n(f64::from_bits(0x3fc35a781fcf7430)),
            ],
        ),
        f64::from_bits(0x3fc1aa1445c5bb34),
    );
}

/// Original Excel20228 held-out Bessel bank and R16 Value2 captures.
/// R27 retains the complete remaining mismatch lists in its research evidence.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_high_j_matches_recorded_seeds_and_forward_results() {
    for (x, order, expected) in [
        (0x4020000000000000, 0x0000000000000000, 0x3fc5f8a755e1788c), // besj-ho-0004
        (0x4020000000000000, 0x3ff0000000000000, 0x3fce084d8f616cc8), // besj-ho-0005
        (0x4020000000000000, 0x4000000000000000, 0xbfbced27e4123ab4), // besj-ho-0006
        (0x4020000000000000, 0x401c000000000000, 0x3fd4845b9401abc6), // besj-ho-0009
        (0x4059000000000000, 0x0000000000000000, 0x3f94772bb4df490f), // besj-ho-0184
        (0x4059000000000000, 0x3ff0000000000000, 0xbfb3bfcc3be6ce4c), // besj-ho-0185
        (0x4059000000000000, 0x401c000000000000, 0x3fb1f6d664e0f46c), // besj-ho-0189
        (0xc19ffffffc000000, 0x0000000000000000, 0xbf0fe64bfb1961f0), // r16-besselj-065
        (0xc19ffffffc000000, 0x3ff0000000000000, 0x3f00eaf7037cd4d4), // r16-besselj-066
        (0xc19ffffffc000000, 0x4000000000000000, 0x3f0fe64bf6dea42f), // r16-besselj-067
    ] {
        let args = vec![n(f64::from_bits(x)), n(f64::from_bits(order))];
        let actual = call("BESSELJ", args).unwrap();
        let calc_func::value::CoreValue::Number(actual) = actual.core() else {
            panic!("BESSELJ returned {actual:?}");
        };
        assert_eq!(
            actual.to_bits(),
            expected,
            "BESSELJ x={x:016x}, order={order:016x}"
        );
    }
}

/// Original R25 exact-operand phase-window captures, with unchanged Excel bits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_high_correction_multiplies_ratio_before_polynomial() {
    for (function, x, order, expected) in [
        (
            "BESSELJ",
            0x40278fdb9f06da55,
            0x0000000000000000,
            0xbf641f37ffc90c13,
        ), // r25-bessel-00052
        (
            "BESSELJ",
            0x402ab41b098d9cef,
            0x3ff0000000000000,
            0x3f79150d1d69965a,
        ), // r25-bessel-00058
        (
            "BESSELJ",
            0x402ab41b098f5ff6,
            0x3ff0000000000000,
            0x3f79150d207b8262,
        ), // r25-bessel-00073
        (
            "BESSELJ",
            0x402ab41b098f5ff7,
            0x3ff0000000000000,
            0x3f79150d207b8420,
        ), // r25-bessel-00078
        (
            "BESSELJ",
            0x402dd85a74162294,
            0x0000000000000000,
            0x3f5c45f4c3bbd9ed,
        ), // r25-bessel-00097
        (
            "BESSELJ",
            0x402dd85a7417e599,
            0x0000000000000000,
            0x3f5c45f4b815478d,
        ), // r25-bessel-00102
        (
            "BESSELJ",
            0x402dd85a7417e59b,
            0x0000000000000000,
            0x3f5c45f4b8153a54,
        ), // r25-bessel-00112
        (
            "BESSELJ",
            0x4036c6cbc4605f61,
            0x3ff0000000000000,
            0xbf66898bb67bbd3d,
        ), // r25-bessel-00138
        (
            "BESSELJ",
            0x403858eb79a4a232,
            0x0000000000000000,
            0xbf4b2d36b9cba3de,
        ), // r25-bessel-00152
        (
            "BESSELJ",
            0x403858eb79a4a233,
            0x0000000000000000,
            0xbf4b2d36b9cb8f2a,
        ), // r25-bessel-00157
        (
            "BESSELJ",
            0x403858eb79a583b6,
            0x0000000000000000,
            0xbf4b2d36a78f0a97,
        ), // r25-bessel-00167
        (
            "BESSELJ",
            0x403858eb79a583b7,
            0x0000000000000000,
            0xbf4b2d36a78ef5e4,
        ), // r25-bessel-00172
        (
            "BESSELJ",
            0x4039eb0b2ee9c688,
            0x3ff0000000000000,
            0x3f62919085f01638,
        ), // r25-bessel-00188
        (
            "BESSELJ",
            0x404986834196890f,
            0x3ff0000000000000,
            0x3f4ae05ad9cd0f5d,
        ), // r25-bessel-00203
        (
            "BESSELJ",
            0x404a4f931c391b38,
            0x0000000000000000,
            0x3f311e97b4d1eee0,
        ), // r25-bessel-00227
        (
            "BESSELJ",
            0x404b18a2f6db3ca3,
            0x3ff0000000000000,
            0xbf4892e4e0532714,
        ), // r25-bessel-00248
        (
            "BESSELJ",
            0x40693b1d4f98c414,
            0x3ff0000000000000,
            0x3f1b59dd7eb7db96,
        ), // r25-bessel-00253
        (
            "BESSELJ",
            0x40693b1d4f98c416,
            0x3ff0000000000000,
            0x3f1b59dd7ebb73b6,
        ), // r25-bessel-00263
        (
            "BESSELJ",
            0x40696d614641689f,
            0x0000000000000000,
            0x3f0205e929ce7b06,
        ), // r25-bessel-00282
        (
            "BESSELJ",
            0x40699fa53ce9f0f9,
            0x3ff0000000000000,
            0xbf1ab985f37a8fee,
        ), // r25-bessel-00298
        (
            "BESSELY",
            0x40278fdb9f06da53,
            0x3ff0000000000000,
            0xbf7e3fe308f58e41,
        ), // r25-bessel-00045
        (
            "BESSELY",
            0x40278fdb9f06da55,
            0x3ff0000000000000,
            0xbf7e3fe308f591f9,
        ), // r25-bessel-00055
        (
            "BESSELY",
            0x402ab41b098d9cef,
            0x0000000000000000,
            0xbf60b15329482f2b,
        ), // r25-bessel-00059
        (
            "BESSELY",
            0x402dd85a74162292,
            0x3ff0000000000000,
            0x3f753bfd9a7f34ed,
        ), // r25-bessel-00090
        (
            "BESSELY",
            0x402dd85a74162293,
            0x3ff0000000000000,
            0x3f753bfd9a7f3694,
        ), // r25-bessel-00095
        (
            "BESSELY",
            0x402dd85a7417e599,
            0x3ff0000000000000,
            0x3f753bfd9d66fc3f,
        ), // r25-bessel-00105
        (
            "BESSELY",
            0x4036c6cbc4605f60,
            0x0000000000000000,
            0x3f4e081f27555146,
        ), // r25-bessel-00134
        (
            "BESSELY",
            0x4036c6cbc4605f61,
            0x0000000000000000,
            0x3f4e081f27553bde,
        ), // r25-bessel-00139
        (
            "BESSELY",
            0x4036c6cbc4605f62,
            0x0000000000000000,
            0x3f4e081f27552677,
        ), // r25-bessel-00144
        (
            "BESSELY",
            0x403858eb79a4a233,
            0x3ff0000000000000,
            0xbf6464a5daaf15ee,
        ), // r25-bessel-00160
        (
            "BESSELY",
            0x4039eb0b2ee9c687,
            0x0000000000000000,
            0xbf48bf26abd555dc,
        ), // r25-bessel-00184
        (
            "BESSELY",
            0x404986834196890f,
            0x0000000000000000,
            0xbf31ea5aa30f373d,
        ), // r25-bessel-00204
        (
            "BESSELY",
            0x404a4f931c391b38,
            0x3ff0000000000000,
            0x3f49aea16f73e30b,
        ), // r25-bessel-00230
        (
            "BESSELY",
            0x404b18a2f6db3ca1,
            0x0000000000000000,
            0x3f30617bb35c54da,
        ), // r25-bessel-00239
        (
            "BESSELY",
            0x404b18a2f6db3ca2,
            0x0000000000000000,
            0x3f30617bb35c1d5a,
        ), // r25-bessel-00244
        (
            "BESSELY",
            0x40696d6146414c6e,
            0x3ff0000000000000,
            0x3f1b08eb85ac3cf4,
        ), // r25-bessel-00270
        (
            "BESSELY",
            0x40696d614641689f,
            0x3ff0000000000000,
            0x3f1b08ebb823c8b1,
        ), // r25-bessel-00285
        (
            "BESSELY",
            0x40696d61464168a1,
            0x3ff0000000000000,
            0x3f1b08ebb8275d40,
        ), // r25-bessel-00295
        (
            "BESSELY",
            0x40699fa53ce9f0fa,
            0x0000000000000000,
            0x3f01d0fae3bcaccd,
        ), // r25-bessel-00304
    ] {
        let actual = call(
            function,
            vec![n(f64::from_bits(x)), n(f64::from_bits(order))],
        )
        .unwrap();
        let calc_func::value::CoreValue::Number(actual) = actual.core() else {
            panic!("{function} returned {actual:?}");
        };
        assert_eq!(
            actual.to_bits(),
            expected,
            "{function} x={x:016x}, order={order:016x}"
        );
    }
}

/// Carbon cb15 (dc-j9k.85, Excel 16.0.20527.20196, Windows x64, AMD Ryzen 7 4800H): stored-PC64 high
/// asymptotic seeds (`z = 8/|x|`, `y = z*z`, Horner, `J0` `(z*sin)*corr` vs
/// `J1/Y0/Y1` `(trig*corr)*z`, `excel_sqrt_staged(W/|x|)`) and the three
/// remaining stored-PC64 operations in `jn` backward recurrence (`B0/B1/B2`).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_j9k_85_cb15_bessel_high_asymptotic_and_backward_recurrence() {
    if !crate::x87_cpu::AMD_RYZEN_7_4800H.compares_on_active_backend() {
        return;
    }
    let rows = [
        // H1: J1 and Y1 trailing-product association `(trig * corr) * z`
        (
            "H1_J1_pos",
            "BESSELJ",
            0x403c_a0fd_c1e0_28b0,
            0x3ff0_0000_0000_0000,
            0x3fae_fbdd_8c2f_6896,
        ),
        (
            "H1_J1_neg",
            "BESSELJ",
            0xc033_b62a_5c36_6357,
            0x3ff0_0000_0000_0000,
            0xbf91_94a0_53a0_56c5,
        ),
        (
            "H1_Y1",
            "BESSELY",
            0x4021_92de_24d4_3c2c,
            0x3ff0_0000_0000_0000,
            0x3faa_13fb_d72d_fee7,
        ),
        // H2: J0 `(z * sin) * corr` vs Y0 `(cos * corr) * z`
        (
            "H2_J0",
            "BESSELJ",
            0x4046_6492_ad08_4087,
            0x0000_0000_0000_0000,
            0x3fbe_846e_744f_f5b9,
        ),
        (
            "H2_Y0",
            "BESSELY",
            0x4024_3ff9_ef09_930e,
            0x0000_0000_0000_0000,
            0x3f98_fb73_96fb_374b,
        ),
        // H3: high seed stored-PC64 sites (J0, Y0)
        (
            "H3_J0",
            "BESSELJ",
            0x4048_e11c_8f63_cfe1,
            0x0000_0000_0000_0000,
            0x3f9f_9299_388a_a30b,
        ),
        (
            "H3_Y0",
            "BESSELY",
            0x4096_4f25_d7ad_b2c2,
            0x0000_0000_0000_0000,
            0x3f8c_769e_a5e4_9948,
        ),
        // S2 (`y = z * z` vs `64 / (x * x)`) and S10 (`excel_sqrt_staged(W / x)`)
        (
            "S2_J0_y_zz",
            "BESSELJ",
            0x4025_6457_16f2_1e1c,
            0x0000_0000_0000_0000,
            0xbfcb_c47a_1708_e8d1,
        ),
        (
            "S2_Y1_y_zz",
            "BESSELY",
            0x402d_f84c_5be3_6af5,
            0x3ff0_0000_0000_0000,
            0x3f92_6eaa_598e_9a52,
        ),
        (
            "S10_J0_sqrt",
            "BESSELJ",
            0x4098_b07e_159c_0161,
            0x0000_0000_0000_0000,
            0xbf8a_1514_e864_1283,
        ),
        (
            "S10_Y0_sqrt",
            "BESSELY",
            0x4078_3ea0_b459_8d57,
            0x0000_0000_0000_0000,
            0xbf9b_1f55_1716_1096,
        ),
        // H4: high forward recurrence (J and Y)
        (
            "H4_J_fwd",
            "BESSELJ",
            0x4052_4fc3_75af_bd28,
            0x4041_8000_0000_0000,
            0x3fb0_34e0_d28e_0bf7,
        ),
        (
            "H4_Y_fwd",
            "BESSELY",
            0x4031_8c29_c46d_2e40,
            0x402c_0000_0000_0000,
            0x3fc3_dbc6_3235_189e,
        ),
        // H5: near-8 boundary and high-x cancellation zeros
        (
            "H5_near8_J1",
            "BESSELJ",
            0x4020_0000_0000_003b,
            0x3ff0_0000_0000_0000,
            0x3fce_084d_8f61_6ee3,
        ),
        (
            "H5_zero_J0",
            "BESSELJ",
            0x4035_362d_d176_951f,
            0x0000_0000_0000_0000,
            0x3d21_2906_6f0f_17b2,
        ),
        (
            "H5_zero_Y0",
            "BESSELY",
            0x4030_803c_7402_b8c6,
            0x0000_0000_0000_0000,
            0x3d3c_9a2c_64b8_3a22,
        ),
        // H6 / probe2: jn backward recurrence B0 (`index * twice_over_x`),
        // B1 (`total + current`), B2 (`2 * total - current`)
        (
            "H6_B0_idx_mul",
            "BESSELJ",
            0x40a0_0261_51c1_a8fb,
            0x40a0_ac00_0000_0000,
            0x3e18_ebd7_2671_b791,
        ),
        (
            "H6_B1_total_add",
            "BESSELJ",
            0x3f72_0a0b_f7fd_3105,
            0x4022_0000_0000_0000,
            0x39d1_0309_ee03_52d3,
        ),
        (
            "P2_B1_2ulp",
            "BESSELJ",
            0x3f7c_f02a_5d07_ea60,
            0x4000_0000_0000_0000,
            0x3eda_2b4d_7c43_66e6,
        ),
        (
            "H6_B2_two_tot_sub",
            "BESSELJ",
            0x4083_b565_d6b2_323c,
            0x4083_f000_0000_0000,
            0x3f92_9189_d2b9_73e5,
        ),
    ];
    for (label, function, x, order, expected) in rows {
        let actual = call(
            function,
            vec![n(f64::from_bits(x)), n(f64::from_bits(order))],
        )
        .unwrap();
        let calc_func::value::CoreValue::Number(actual) = actual.core() else {
            panic!("{label} {function} returned {actual:?}");
        };
        assert_eq!(
            actual.to_bits(),
            expected,
            "{label} {function} x={x:016x}, order={order:016x}"
        );
    }
    println!("Matched {} cb15 Excel rows", rows.len());
}

/// Live Excel 16.0.20625.20108 (dc-j9k.176): a pure imaginary argument takes IMLOG2's real-argument
/// route, `LN(|b|)` times the stored log2(e), so `IMLOG2("0+355i")` ends 205, not 204.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imlog2_of_a_pure_imaginary_uses_the_real_route() {
    text(call("IMLOG2", vec![t("0+355i")]), "8.47167521439205+2.2661800709136i");
}

/// Live Excel 16.0.20625.20108 (dc-j9k.175/.176, Touchstone 2026-11-25): IMLOG2 and the other IMLOG
/// functions of imaginary-heavy arguments (rows Excel answered and the engine already matched).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imlog_family_matches_live_excel_probes() {
    for (name, arg, expected) in [
        ("IMLOG2", "0-355i", "8.47167521439205-2.2661800709136i"),
        ("IMLOG2", "0+3i", "1.58496250072116+2.2661800709136i"),
        ("IMLOG10", "0+355i", "2.55022835305509+0.682188176920921i"),
        ("IMLN", "0+355i", "5.87211778947542+1.5707963267949i"),
        ("IMLOG2", "1e-10-1e9i", "29.8973528539863-2.2661800709136i"),
        ("IMLOG2", "0.000001-1000i", "9.96578428466209-2.2661800694709i"),
        ("IMLOG2", "1e-5-1e9i", "29.8973528539863-2.26618007091358i"),
        ("IMLOG2", "1e-300-1e5i", "16.6096404744368-2.2661800709136i"),
        ("IMLOG2", "1e-300-1e3i", "9.96578428466209-2.2661800709136i"),
        ("IMLOG2", "1e-3-1e9i", "29.8973528539863-2.26618007091215i"),
        ("IMLOG2", "1-1e9i", "29.8973528539863-2.2661800694709i"),
    ] {
        f1_support::text(f1_support::call(name, vec![f1_support::t(arg)]), expected);
    }
}

/// Live Excel 16.0.20625.20108 (dc-j9k.175, Touchstone 2026-11-25): the IMLN family answers #NUM!
/// (IMARGUMENT #DIV/0!) when im/re overflows and when the squares of both parts underflow.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imln_family_overflowing_ratio_and_underflowing_squares_are_num() {
    use calc_func::value::WorksheetErrorCode::{Div0, Num};
    let mut bad = Vec::new();
    for (name, arg, expected) in [
        ("IMLN", "1e-300-1e9i", Num),
        ("IMLOG10", "1e-300-1e9i", Num),
        ("IMLOG2", "1e-300-1e9i", Num),
        ("IMARGUMENT", "1e-300-1e9i", Div0),
        ("IMLOG2", "1e-300-1.8e8i", Num),
        ("IMLOG2", "1e-200-1e109i", Num),
        ("IMLOG2", "1e-310-1e-5i", Num),
        ("IMLOG2", "1e-300-1e-300i", Num),
        ("IMLOG2", "1e300-1e300i", Num),
        ("IMLN", "1e300-1e300i", Num),
        ("IMLOG2", "1e-320-1e-320i", Num),
        ("IMEXP", "1e-300-1e9i", Num),
    ] {
        let got = f1_support::call(name, vec![f1_support::t(arg)]);
        let ok = matches!(&got, Ok(v) if v.as_error() == Some(expected));
        if !ok {
            bad.push(format!("{name}({arg}) want {expected:?} got {got:?}"));
        }
    }
    for (name, arg, expected) in [
        ("IMLOG2", "1e-300-1.7e8i", "27.3409595054619-2.2661800709136i"),
        ("IMLOG2", "1e-300-1e8i", "26.5754247590989-2.2661800709136i"),
        ("IMLOG2", "1e-308-1i", "-2.2661800709136i"),
        ("IMLOG2", "1e-309-1i", "-2.2661800709136i"),
        ("IMLOG2", "1e-200-1e108i", "358.768234247835-2.2661800709136i"),
        ("IMLOG2", "1e9-1e-300i", "29.8973528539863"),
        ("IMSQRT", "1e-300-1e9i", "31622.7766016838"),
    ] {
        let got = f1_support::call(name, vec![f1_support::t(arg)]);
        let ok = matches!(&got, Ok(v) if matches!(v.core(), calc_func::value::CoreValue::Text(t) if t.to_string_lossy() == expected));
        if !ok {
            bad.push(format!("{name}({arg}) want {expected} got {got:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Live Excel 16.0.20625.20108 (dc-j9k.175): a nonzero complex coefficient below 1e-309 is #NUM!;
/// 1e-309 and up to the normal range reads as 0.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_coefficients_below_1e_309_are_num() {
    for arg in ["1e-310-1i", "9.99999999999999e-310-1i", "1e-320-1i", "1e-400-1i", "1e-310", "0-1e-400i"] {
        f1_support::error(f1_support::call("IMREAL", vec![f1_support::t(arg)]), calc_func::value::WorksheetErrorCode::Num);
    }
    for arg in ["1e-309-1i", "1.5e-309-1i", "1e-308-1i", "1e-309", "0-1e-309i"] {
        let r = f1_support::call("IMREAL", vec![f1_support::t(arg)]).unwrap();
        assert_eq!(r.core(), &calc_func::value::CoreValue::Number(0.0), "{arg}");
    }
}

/// Excel 16.0 build 20527 (Touchstone relay, 2026-11-25; dc-j9k.339-.347): IMPRODUCT multiplies argument by
/// argument. A step whose product overflows is #VALUE! (IMSUM's overflow is #NUM!), raised before a later
/// argument is read; a step below the normal range is flushed to 0 and stays 0.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn improduct_overflow_is_value_and_subnormal_steps_flush() {
    error(call("IMPRODUCT", vec![n(1e300), n(1e300)]), Value);
    error(call("IMPRODUCT", vec![n(1e300), n(1e300), n(0.0)]), Value);
    error(call("IMPRODUCT", vec![n(1e300), n(1e300), t("x")]), Value);
    error(call("IMPRODUCT", vec![n(1e300), t("x"), n(1e300)]), Num);
    error(call("IMPRODUCT", vec![t("1e200+1e200i"), t("1e200-1e200i")]), Value);
    text(call("IMPRODUCT", vec![n(3e-308), n(0.5), n(1e300)]), "0");
    text(call("IMPRODUCT", vec![n(1e-300), n(1e-10), n(1e300)]), "0");
    text(call("IMPRODUCT", vec![n(1e-300), n(1e-300), n(1e300)]), "0");
    // The sweep's representatives; Excel's answers are recorded on the beads.
    error(call("IMPRODUCT", vec![n(f64::from_bits(0x4059000000000000)), n(f64::from_bits(0xc0c3880000000000)), n(f64::from_bits(0x7e37e43c8800759c)), n(f64::from_bits(0x5850000000000000)), n(f64::from_bits(0x3f747ae147ae147b)), n(f64::from_bits(0x0000000000000000)), n(f64::from_bits(0x3f1a36e2eb1c432d)), n(f64::from_bits(0xf280000000000000)), n(f64::from_bits(0xbfefae147ae147ae))]), Value); // dc-j9k.339
    error(call("IMPRODUCT", vec![n(f64::from_bits(0x404e000000000000)), n(f64::from_bits(0xf320000000000000)), n(f64::from_bits(0xbff8000000000000)), n(f64::from_bits(0x433fffffffffffff)), n(f64::from_bits(0xd990000000000000)), n(f64::from_bits(0xbe7ad7f29abcaf48)), n(f64::from_bits(0xbf1a36e2eb1c432f)), n(f64::from_bits(0x4059000000000000)), n(f64::from_bits(0x403e000000000000)), n(f64::from_bits(0x4202a05f20000001)), n(f64::from_bits(0x7400000000000000)), n(f64::from_bits(0x3fb999999999999a))]), Value); // dc-j9k.340
    error(call("IMPRODUCT", vec![n(f64::from_bits(0x7e37e43c8800759c)), n(f64::from_bits(0xbd719799812dea11)), n(f64::from_bits(0x7e37e43c8800759c)), n(f64::from_bits(0xf000000000000000))]), Value); // dc-j9k.341
    error(call("IMPRODUCT", vec![n(f64::from_bits(0xbd950a664b973575)), n(f64::from_bits(0xe38234aa619d9595)), n(f64::from_bits(0x4d74893e2c2961aa)), n(f64::from_bits(0x4a560cd5cd140a24)), n(f64::from_bits(0xc90bf1d35366fdcf)), n(f64::from_bits(0x52d0ec5ae258cd09))]), Value); // dc-j9k.342
    error(call("IMPRODUCT", vec![n(f64::from_bits(0xc059000000000000)), n(f64::from_bits(0x7e37e43c8800759c)), n(f64::from_bits(0x3e7ad7f29abcaf48)), n(f64::from_bits(0xbf1a36e2eb1c432d)), n(f64::from_bits(0x3f1a36e2eb1c432d)), n(f64::from_bits(0x433ffffffffffffe)), n(f64::from_bits(0x4202a05f20000000))]), Value); // dc-j9k.343
    error(call("IMPRODUCT", vec![n(f64::from_bits(0x68a21322b2f4c432)), n(f64::from_bits(0xc1ac5122f89a090d)), n(f64::from_bits(0x3f8ac515f56a973d)), n(f64::from_bits(0x7957c156fe93319f)), n(f64::from_bits(0xc0f164c6ae689d25)), n(f64::from_bits(0xf2bbeb6508db43a7)), n(f64::from_bits(0x09d2821573cc8c78)), n(f64::from_bits(0xc44563c8d3c05b61)), n(f64::from_bits(0x528bc6856579d956)), n(f64::from_bits(0x9ec93cdf3cf68359))]), Value); // dc-j9k.344
    error(call("IMPRODUCT", vec![n(f64::from_bits(0x3fbda972202701e2)), n(f64::from_bits(0xb7b5b3f97974e8a2)), n(f64::from_bits(0x3a82cf19db1ea4b6)), n(f64::from_bits(0x6bf88da5bc42a253)), n(f64::from_bits(0xc08ed32d18c7cc82)), n(f64::from_bits(0xc08483e25282b5f5)), n(f64::from_bits(0xc277d41735725fe9)), n(f64::from_bits(0x6a3bd1dd535e9bcf)), n(f64::from_bits(0xbfc38c7d01c670a6)), n(f64::from_bits(0xc1d2bab6bf6fad05)), n(f64::from_bits(0x3dc47f680d66d959))]), Value); // dc-j9k.345
    error(call("IMPRODUCT", vec![n(f64::from_bits(0x4340000000000001)), n(f64::from_bits(0x4010000000000000)), n(f64::from_bits(0xfe37e43c8800759e)), n(f64::from_bits(0x8890000000000000)), n(f64::from_bits(0xbf50624dd2f1a9fc)), n(f64::from_bits(0x0010000000000001)), n(f64::from_bits(0xc023ffffffffffff)), n(f64::from_bits(0xe860000000000000))]), Value); // dc-j9k.346
    text(call("IMPRODUCT", vec![n(f64::from_bits(0x8010000000000000)), n(f64::from_bits(0xbf9999999999999a)), n(f64::from_bits(0xe740000000000000)), n(f64::from_bits(0x3f9999999999999a)), n(f64::from_bits(0x2b2bff2ee48e0530)), n(f64::from_bits(0x41c0000000000000)), n(f64::from_bits(0xbfe0000000000000)), n(f64::from_bits(0x4340000000000000))]), "0"); // dc-j9k.347
}

/// Excel 16.0 build 20625 (Touchstone relay, 2026-11-25; dc-j9k.348): operands with different suffixes
/// ("3-4j" and "i") are #NUM! in IMDIV and IMSUB, #VALUE! in IMSUM and IMPRODUCT.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mixed_suffix_error_depends_on_function() {
    for (a, b) in [("3-4j", "i"), ("3-4i", "j"), ("3-4i", "2j"), ("j", "i")] {
        error(call("IMDIV", vec![t(a), t(b)]), Num);
        error(call("IMSUB", vec![t(a), t(b)]), Num);
        error(call("IMSUM", vec![t(a), t(b)]), Value);
        error(call("IMPRODUCT", vec![t(a), t(b)]), Value);
    }
}

/// Excel 16.0 build 20625 (Touchstone relay, 2026-11-25; dc-j9k.334/.335): IMSUM and IMPRODUCT read their
/// arguments one at a time: an unreadable argument is #NUM!, a suffix that differs from an earlier one is
/// #VALUE!, an overflowing IMSUM step is #NUM!; whichever comes first in argument order wins.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imsum_improduct_report_the_first_problem_in_argument_order() {
    for name in ["IMSUM", "IMPRODUCT"] {
        error(call(name, vec![t("i"), t("x"), t("j")]), Num);
        error(call(name, vec![t("i"), t("j"), t("x")]), Value);
        error(call(name, vec![t("x"), t("i"), t("j")]), Num);
        error(call(name, vec![t("i"), t("j"), t("")]), Value);
    }
    error(call("IMSUM", vec![n(1.7e308), n(1.7e308), t("i"), t("j")]), Num);
    error(call("IMSUM", vec![n(1.7e308), t("i"), t("j"), n(1.7e308)]), Value);
    // The sweep representatives (dc-j9k.334, .335).
    let rep334 = ["1-i", "-1", "-j", "0", "3+4i"];
    let mut a: Vec<_> = rep334.iter().map(|s| t(s)).collect();
    a.extend([n(-1.0), t("0-354.5i"), calc_func::value::CalcValue::empty(), t("12521X."), t(",,Z3")]);
    error(call("IMSUM", a), Value);
    let a = ["-1", "134217727.5", "-j", "1e-300+1e109i", "134217728"]
        .iter().map(|s| t(s)).chain([calc_func::value::CalcValue::empty(), t("Y"), t("-i")]).collect();
    error(call("IMSUM", a), Value);
}

/// Excel 16.0 build 20625 (Touchstone relay, 2026-11-25; dc-j9k.357): the complex exponential saturates
/// exp(re) at the largest double before scaling by cos and sin; a component that is exactly the largest
/// double is #NUM! (IMEXP("710+1e-15i")), as is the real route (IMEXP("710")).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imexp_saturates_the_real_exponential() {
    for z in ["710+i", "711+i", "1000+i", "709.783+i"] {
        text(call("IMEXP", vec![t(z)]), "9.71297746009434E+307+1.51270661257499E+308i");
    }
    text(call("IMEXP", vec![t("710-i")]), "9.71297746009434E+307-1.51270661257499E+308i");
    text(call("IMEXP", vec![t("710+0.1i")]), "1.78871215707972E+308+1.79469847735852E+307i");
    text(call("IMEXP", vec![t("710+1.5707963i")]), "4.81690016402968E+300+1.79769313486232E+308i");
    text(call("IMEXP", vec![t("710+3i")]), "-1.77970271470383E+308+2.53690469680938E+307i");
    text(call("IMEXP", vec![t("-745+i")]), "0");
    for z in ["710+1e-15i", "710+1.57079632679i", "710+3.14159265358979i", "710+0i", "710"] {
        error(call("IMEXP", vec![t(z)]), Num);
    }
    error(call("IMEXP", vec![n(710.0)]), Num);
}

/// Excel 16.0 build 20625 (Touchstone, dc-j9k.357): IMEXP flushes a component below the normal range.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imexp_flushes_subnormal_components() {
    text(call("IMEXP", vec![t("-708+i")]), "2.78320988327593E-308i");
    text(call("IMEXP", vec![t("-709+i")]), "0");
    text(call("IMEXP", vec![t("-708+1.5707963i")]), "3.30755300363841E-308i");
}

/// Excel 16.0 build 20625 (Touchstone, dc-j9k.358): IMEXP's circular factors are the worksheet COS and SIN.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imexp_uses_the_worksheet_circular_factors() {
    text(call("IMEXP", vec![t("1e-5+2e5i")]), "0.997454021377453-0.0714526097350478i");
}

/// Excel 16.0 build 20625 (Touchstone, dc-j9k.357 sweep of 1,200 IMEXP inputs): past |im| 2^27 both circular
/// factors are the largest double, so the result is exp(re) times it in both components; an overflow is #NUM!.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imexp_past_2_pow_27_scales_the_largest_double() {
    for (z, expected) in [
        ("-1.986408992-189136055.9i", "2.4662045569442E+307+2.4662045569442E+307i"),
        ("-91.62273316+285604541.8i", "2.90715462570676E+268+2.90715462570676E+268i"),
        ("-1.402125125e-07+158312174.9i", "1.79769288280326E+308+1.79769288280326E+308i"),
        ("333.0136229-22257579.08i", "-4.81186746991685E+143+4.1989990459965E+144i"),
        ("383.203825+600.0815279i", "-2.64852424685757E+166-9.89181844300722E+164i"),
        ("-97.09285634-0.03653430818i", "6.80484686395056E-43-2.48721042901288E-44i"),
    ] {
        text(call("IMEXP", vec![t(z)]), expected);
    }
    error(call("IMEXP", vec![t("338.501785-232076421.2i")]), Num);
    error(call("IMEXP", vec![t("1-1e9i")]), Num);
}

/// Excel 16.0 build 20625 (Touchstone, dc-j9k.289/.290): COMPLEX reads the suffix first.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_reads_the_suffix_before_the_coefficients() {
    use calc_func::value::WorksheetErrorCode::{Null, Ref};
    error(call("COMPLEX", vec![e(Null), n(120.0), n(5.0)]), Value);
    error(call("COMPLEX", vec![n(1.0), e(Ref), n(5.0)]), Value);
    error(call("COMPLEX", vec![e(Ref), e(Div0), t("k")]), Value);
    error(call("COMPLEX", vec![t("x"), n(1.0), e(Ref)]), Ref);
    error(call("COMPLEX", vec![n(1.0), n(2.0), e(Ref)]), Ref);
    error(call("COMPLEX", vec![e(Ref), n(2.0), t("i")]), Ref);
    error(call("COMPLEX", vec![e(Value), e(Ref), t("i")]), Value);
    error(call("COMPLEX", vec![e(Ref), e(Value), t("i")]), Ref);
    error(call("COMPLEX", vec![e(Div0), n(1.0)]), Div0);
}

/// Excel 16.0 build 20625 (Touchstone, dc-j9k.586): COMPLEX does not trim whitespace on its
/// suffix argument; only "", "i", "j" (or omitted/blank) are valid, and any whitespace suffix
/// returns #VALUE! before formatting coefficients (which would otherwise overflow to #NUM!).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_j9k_586_complex_rejects_whitespace_suffixes() {
    error(call("COMPLEX", vec![n(f64::MAX), n(10.0), t("\n")]), Value);
    error(call("COMPLEX", vec![n(1.0), n(2.0), t("\n")]), Value);
    error(call("COMPLEX", vec![n(1.0), n(2.0), t(" ")]), Value);
    error(call("COMPLEX", vec![n(1.0), n(2.0), t("\t")]), Value);
    error(call("COMPLEX", vec![n(1.0), n(2.0), t(" i")]), Value);
    error(call("COMPLEX", vec![n(1.0), n(2.0), t("i ")]), Value);
    error(call("COMPLEX", vec![n(1.0), n(2.0), t("I")]), Value);
    text(call("COMPLEX", vec![n(1.0), n(2.0), t("")]), "1+2i");
    text(call("COMPLEX", vec![n(1.0), n(2.0), t("i")]), "1+2i");
    text(call("COMPLEX", vec![n(1.0), n(2.0), t("j")]), "1+2j");
    error(call("COMPLEX", vec![n(f64::MAX), n(10.0), t("i")]), Num);
}

/// Excel 16.0.20625 (Touchstone, dc-shqi1): IMLOG10 flushes subnormal imaginary parts after
/// dividing by ln(10) and uses the real x87 LOG10_E route for pure-imaginary arguments.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imlog10_flushes_subnormal_imaginary_and_uses_real_route_for_pure_imaginary() {
    text(call("IMLOG10", vec![t("1e9-3e-299i")]), "9");
    text(call("IMLOG10", vec![t("1e9+3e-299i")]), "9");
    text(call("IMLOG10", vec![t("0+8i")]), "0.903089986991944+0.682188176920921i");
    text(call("IMLOG10", vec![t("0+11i")]), "1.04139268515823+0.682188176920921i");
    text(call("IMLOG10", vec![t("0+166i")]), "2.22010808804006+0.682188176920921i");
}

/// Excel 16.0.20625 (Touchstone, dc-shqi1): IMEXP flushes subnormal exp(re) to 0 before scaling by
/// circular factors, even when |im| >= 2^27 where sin/cos saturate at f64::MAX.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn imexp_flushes_subnormal_real_exp_before_scaling_past_2_pow_27() {
    text(call("IMEXP", vec![t("-709+1e9i")]), "0");
    text(call("IMEXP", vec![t("-709-1e9i")]), "0");
    text(call("IMEXP", vec![t("-709+134217728i")]), "0");
    text(call("IMEXP", vec![t("-708+1e9i")]), "5.945965327834+5.945965327834i");
}

/// dc-j9k.490 (Excel 20625): IMPOWER forms the squared modulus of every operand except a positive
/// real and a pure imaginary one, so a squared modulus beyond the binary64 range is #NUM!.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn impower_squared_modulus_overflow_off_axis_and_negative_real() {
    use calc_func::value::WorksheetErrorCode::Num;
    for power in [1.0, 0.5, -0.025] {
        error(call("IMPOWER", vec![n(-1e155), n(power)]), Num);
        error(call("IMPOWER", vec![t("-1E+155"), n(power)]), Num);
        error(call("IMPOWER", vec![t("1E+155+1i"), n(power)]), Num);
        error(call("IMPOWER", vec![t("1+1E+155i"), n(power)]), Num);
        error(call("IMPOWER", vec![t("-1E+155-1E+155i"), n(power)]), Num);
        // a positive real and a pure imaginary operand are formed without the square
        text(call("IMPOWER", vec![t("1E+155"), n(1.0)]), "1E+155");
    }
    text(call("IMPOWER", vec![t("1E+155i"), n(1.0)]), "6.1257422745431E+138+1E+155i");
    text(call("IMPOWER", vec![n(-1.3e154), n(1.0)]), "-1.3E+154+1.59269299138121E+138i");
}

/// Excel 20625 (Touchstone 2026-11-26, dc-j9k.556): CONVERT reads its arguments in order and a logical
/// at any of them is #VALUE! (an error earlier in the list wins); a number as a unit is #N/A.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn convert_logical_arguments_are_value_in_argument_order() {
    use calc_func::value::WorksheetErrorCode::{Div0, NA, Value};
    error(call("CONVERT", vec![n(1.5), b(true), n(0.125)]), Value);
    error(call("CONVERT", vec![n(1.0), b(true), t("m")]), Value);
    error(call("CONVERT", vec![n(1.0), t("m"), b(true)]), Value);
    error(call("CONVERT", vec![b(true), t("xyz"), t("m")]), Value);
    error(call("CONVERT", vec![n(1.0), b(true), e(NA)]), Value);
    error(call("CONVERT", vec![n(1.0), e(NA), b(true)]), NA);
    error(call("CONVERT", vec![e(Div0), b(true), t("m")]), Div0);
    error(call("CONVERT", vec![n(1.0), b(true), t("xyz")]), Value);
    error(call("CONVERT", vec![n(1.0), n(5.0), t("m")]), NA);
    error(call("CONVERT", vec![n(1.0), t("m"), n(5.0)]), NA);
}

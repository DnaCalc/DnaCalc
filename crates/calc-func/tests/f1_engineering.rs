#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the engineering functions ported from OxFunc, called through
//! `calc_func::invoke`. Expected values are Excel's documented examples.

mod f1_support;

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
}

use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Value};
use f1_support::*;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn radix_conversions() {
    exact(call("BIN2DEC", vec![t("1100100")]), 100.0);
    exact(call("BIN2DEC", vec![t("1111111111")]), -1.0);
    text(call("DEC2BIN", vec![n(9.0), n(4.0)]), "1001");
    text(call("DEC2BIN", vec![n(-100.0)]), "1110011100");
    error(call("DEC2BIN", vec![n(9.0), n(2.0)]), Num);
    text(call("DEC2HEX", vec![n(-54.0)]), "FFFFFFFFCA");
    text(call("DEC2HEX", vec![n(100.0), n(4.0)]), "0064");
    exact(call("HEX2DEC", vec![t("FFFFFFFF9C")]), -100.0);
    exact(call("OCT2DEC", vec![n(54.0)]), 44.0);
    text(call("HEX2BIN", vec![t("F"), n(8.0)]), "00001111");
    text(call("OCT2HEX", vec![n(100.0), n(4.0)]), "0040");
    text(call("BIN2OCT", vec![n(1001.0), n(3.0)]), "011");
    error(call("BIN2DEC", vec![b(true)]), Value);
    error(call("DEC2BIN", vec![missing()]), NA);
    error(call("BIN2DEC", vec![t("102")]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bit_functions() {
    exact(call("BITAND", vec![n(13.0), n(25.0)]), 9.0);
    exact(call("BITOR", vec![n(23.0), n(10.0)]), 31.0);
    exact(call("BITXOR", vec![n(5.0), n(3.0)]), 6.0);
    exact(call("BITLSHIFT", vec![n(4.0), n(2.0)]), 16.0);
    exact(call("BITRSHIFT", vec![n(13.0), n(2.0)]), 3.0);
    error(call("BITAND", vec![n(1.5), n(1.0)]), Num);
    error(call("BITAND", vec![n(-1.0), n(1.0)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn complex_numbers() {
    text(call("COMPLEX", vec![n(3.0), n(4.0)]), "3+4i");
    text(call("COMPLEX", vec![n(3.0), n(4.0), t("j")]), "3+4j");
    text(call("COMPLEX", vec![n(0.0), n(1.0)]), "i");
    error(call("COMPLEX", vec![n(3.0), n(4.0), t("k")]), Value);
    exact(call("IMABS", vec![t("5+12i")]), 13.0);
    exact(call("IMREAL", vec![t("6-9i")]), 6.0);
    exact(call("IMAGINARY", vec![t("3+4i")]), 4.0);
    text(call("IMCONJUGATE", vec![t("3+4i")]), "3-4i");
    text(call("IMSUM", vec![t("3+4i"), t("5-3i")]), "8+i");
    text(call("IMSUB", vec![t("13+4i"), t("5+3i")]), "8+i");
    text(call("IMPRODUCT", vec![t("3+4i"), t("5-3i")]), "27+11i");
    text(call("IMDIV", vec![t("-238+240i"), t("10+24i")]), "5+12i");
    text(call("IMPOWER", vec![t("2+3i"), n(2.0)]), "-5+12i");
    close(call("IMARGUMENT", vec![t("3+4i")]), 0.92729521800161, 1e-13);
    error(call("IMABS", vec![t("not complex")]), Num);
    error(call("IMSUM", vec![t("1+i"), t("1+j")]), Value);
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
    exact(
        call("BESSELJ", vec![n(1.9), n(2.0)]),
        f64::from_bits(0x3fd5_1d81_390f_83eb),
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
    exact(call("DELTA", vec![n(5.0), n(4.0)]), 0.0);
    exact(call("DELTA", vec![n(5.0), n(5.0)]), 1.0);
    exact(call("DELTA", vec![n(0.0)]), 1.0);
    exact(call("GESTEP", vec![n(5.0), n(4.0)]), 1.0);
    exact(call("GESTEP", vec![n(-4.0)]), 0.0);
    close(
        call("CONVERT", vec![n(1.0), t("lbm"), t("kg")]),
        0.45359237,
        1e-12,
    );
    close(call("CONVERT", vec![n(68.0), t("F"), t("C")]), 20.0, 1e-12);
    error(call("CONVERT", vec![n(2.0), t("gal"), t("mi")]), NA);
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
    error(call("BESSELY", vec![n(0.0), n(0.0)]), Num);
    error(call("BESSELY", vec![n(0.0), n(1.0)]), Num);
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
    exact(call("BESSELJ", vec![n(0.0), n(2.0)]), 0.0);
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

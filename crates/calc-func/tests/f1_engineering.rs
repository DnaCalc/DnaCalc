#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the engineering functions ported from OxFunc, called through
//! `calc_func::invoke`. Expected values are Excel's documented examples.

mod f1_support;

use calc_func::value::WorksheetErrorCode::{NA, Num, Value};
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_functions() {
    // Preserve the Excel reading separately from the generic library calculation.
    recorded_calculation(call("BESSELJ", vec![n(1.9), n(2.0)]),
        f64::from_bits(0x3fd5_1d81_390f_83eb), f64::from_bits(0x3fd5_1d80_cca3_0f39));
    close(call("BESSELI", vec![n(1.5), n(1.0)]), 0.981666428, 1e-8);
    close(call("BESSELK", vec![n(1.5), n(1.0)]), 0.277387804, 1e-8);
    close(call("BESSELY", vec![n(2.5), n(1.0)]), 0.145918138, 1e-8);
    error(call("BESSELJ", vec![n(1.9), n(-1.0)]), Num);
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
        assert!(matches!(value.core(), calc_func::value::CoreValue::Number(_)),
            "BESSELI({boundary}, {order}) must remain numeric: {value:?}");
        error(call("BESSELI", vec![n(outside), n(order)]), Num);
    }
    for order in [0.0, 1.0, 2.0] {
        error(call("BESSELI", vec![n(-outside), n(order)]), Num);
    }
    exact(call("BESSELI", vec![n(boundary), n(0.0)]),
        f64::from_bits(0x7f8c_3d51_558e_e541));
    exact(call("BESSELI", vec![n(-boundary), n(1.0)]),
        f64::from_bits(0xff8c_3839_94e0_48cc));
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
        exact(call("BESSELI", vec![n(x), n(order)]), f64::from_bits(expected));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn besseli_normalization_keeps_a_normal_result_when_its_ratio_underflows() {
    // Mathematical series controls at 100 and 200 decimal digits. Both are
    // normal binary64 results although the I_n/I0 ratio or I0 itself is not.
    for (x, order, expected) in [
        (709.7, 1570.0, 6.766516533174238e-301),
    ] {
        close(call("BESSELI", vec![n(x), n(order)]), expected, 2e-11);
    }
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
    close(call("CONVERT", vec![n(1.0), t("lbm"), t("kg")]), 0.45359237, 1e-12);
    close(call("CONVERT", vec![n(68.0), t("F"), t("C")]), 20.0, 1e-12);
    error(call("CONVERT", vec![n(2.0), t("gal"), t("mi")]), NA);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_low_rational_seeds_match_recorded_excel_bits() {
    // Existing catalogue capture dc96l3_bessel, Excel 16.0.20527.20088.
    // Original answers and operand bits are preserved; this is the invoke route.
    for (function, x, order, expected) in [
        ("BESSELY", 0x4004000000000000, 0x3ff0000000000000, 0x3fc2ad720e3ee754),
        ("BESSELJ", 0x3fd3333333333333, 0x0000000000000000, 0x3fef48b6d783ca43),
        ("BESSELJ", 0x3fd3333333333333, 0x3ff0000000000000, 0x3fc2fc1c68ab31f5),
        ("BESSELJ", 0x3ff199999999999a, 0x0000000000000000, 0x3fe70724bfe3f22a),
        ("BESSELJ", 0x3ff199999999999a, 0x3ff0000000000000, 0x3fde2343cc4f390e),
        ("BESSELJ", 0x4000000000000000, 0x0000000000000000, 0x3fcca87401162af6),
        ("BESSELJ", 0x4000000000000000, 0x3ff0000000000000, 0x3fe2748795969b5b),
        ("BESSELJ", 0x4004000000000000, 0x0000000000000000, 0xbfa8c5c21daa73f8),
        ("BESSELJ", 0x4004000000000000, 0x3ff0000000000000, 0x3fdfd063c8529f24),
        ("BESSELJ", 0x400e000000000000, 0x0000000000000000, 0xbfd9b0a303f8be57),
        ("BESSELJ", 0x400e000000000000, 0x3ff0000000000000, 0x3fa1036fef5a6608),
        ("BESSELJ", 0x400f333333333333, 0x0000000000000000, 0xbfd9b7847390a92a),
        ("BESSELJ", 0x400f333333333333, 0x3ff0000000000000, 0xbf9be5dc8becdfb4),
        ("BESSELJ", 0x4014cccccccccccd, 0x0000000000000000, 0xbfbc3bfe8f50de92),
        ("BESSELJ", 0x4014cccccccccccd, 0x3ff0000000000000, 0xbfd5f75da16eefa8),
        ("BESSELJ", 0x401ecccccccccccd, 0x0000000000000000, 0x3fce0608ac6a480b),
        ("BESSELJ", 0x401ecccccccccccd, 0x3ff0000000000000, 0x3fc73541424d9cb2),
        ("BESSELJ", 0xbffb333333333333, 0x0000000000000000, 0x3fd978957e320ded),
        ("BESSELJ", 0xbffb333333333333, 0x3ff0000000000000, 0xbfe27d0d82ccb28e),
        ("BESSELY", 0x3fd3333333333333, 0x0000000000000000, 0xbfe9d52f68d1fa7a),
        ("BESSELY", 0x3fd3333333333333, 0x3ff0000000000000, 0xc0025847819115ec),
        ("BESSELY", 0x3ff199999999999a, 0x0000000000000000, 0x3fc4c1c3996c05ca),
        ("BESSELY", 0x3ff199999999999a, 0x3ff0000000000000, 0xbfe656fed4bf15cf),
        ("BESSELY", 0x4000000000000000, 0x0000000000000000, 0x3fe054ff59a9e8c9),
        ("BESSELY", 0x4000000000000000, 0x3ff0000000000000, 0xbfbb667a398a2868),
        ("BESSELY", 0x4004000000000000, 0x0000000000000000, 0x3fdfe0627f28ac71),
        ("BESSELY", 0x400e000000000000, 0x0000000000000000, 0x3fb5d362f8d268e4),
        ("BESSELY", 0x400e000000000000, 0x3ff0000000000000, 0x3fda9d981445d3dc),
        ("BESSELY", 0x400f333333333333, 0x0000000000000000, 0x3f97efda52684810),
        ("BESSELY", 0x400f333333333333, 0x3ff0000000000000, 0x3fda19b92227a346),
        ("BESSELY", 0x4014cccccccccccd, 0x0000000000000000, 0xbfd533371efa8668),
        ("BESSELY", 0x4014cccccccccccd, 0x3ff0000000000000, 0x3fb445d16d3cb0e8),
        ("BESSELY", 0x401ecccccccccccd, 0x0000000000000000, 0x3fc538fcd0fec429),
        ("BESSELY", 0x401ecccccccccccd, 0x3ff0000000000000, 0xbfccb677d633cddc),
        ("BESSELJ", 0x0000000000000000, 0x0000000000000000, 0x3ff0000000c292b1),
        ("BESSELJ", 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000),
        ("BESSELJ", 0x01a56e1fc2f8f359, 0x3ff0000000000000, 0x01956e1fc306f12b),
        ("BESSELY", 0x01a56e1fc2f8f359, 0x3ff0000000000000, 0xfe2e6b6f21c268b8),
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

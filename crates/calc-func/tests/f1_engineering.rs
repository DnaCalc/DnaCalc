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
    // The original Excel reading is now matched by the published backward route.
    exact(call("BESSELJ", vec![n(1.9), n(2.0)]),
        f64::from_bits(0x3fd5_1d81_390f_83eb));
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn bessel_published_backward_and_high_y1_match_recorded_excel_bits() {
    // Unchanged catalogue dc96l3_bessel answers and existing R19 J/Y banks.
    // This table preserves exact operands and Excel answers through invoke.
    for (function, x, order, expected) in [
        ("BESSELJ", 0x3ffe666666666666, 0x4000000000000000, 0x3fd51d81390f83eb),
        ("BESSELJ", 0x3fd3333333333333, 0x4000000000000000, 0x3f86de209f391983),
        ("BESSELJ", 0x3fd3333333333333, 0x4008000000000000, 0x3f42541c0c752fd9),
        ("BESSELJ", 0x3fd3333333333333, 0x4014000000000000, 0x3ea5277765c0bf87),
        ("BESSELJ", 0x3fd3333333333333, 0x4024000000000000, 0x3cdc916d1bb1a8f2),
        ("BESSELJ", 0x3ff199999999999a, 0x4000000000000000, 0x3fc17aef27ec916f),
        ("BESSELJ", 0x3ff199999999999a, 0x4008000000000000, 0x3f9a4faaa04edc20),
        ("BESSELJ", 0x3ff199999999999a, 0x4014000000000000, 0x3f3a213de89794a0),
        ("BESSELJ", 0x3ff199999999999a, 0x4024000000000000, 0x3e07550f46fef591),
        ("BESSELJ", 0x4000000000000000, 0x4000000000000000, 0x3fd694d5ed8bbdd6),
        ("BESSELJ", 0x4000000000000000, 0x4008000000000000, 0x3fc0813660d7b6fa),
        ("BESSELJ", 0x4000000000000000, 0x4014000000000000, 0x3f7cd596393d19ff),
        ("BESSELJ", 0x4000000000000000, 0x4024000000000000, 0x3e90e16696236e1b),
        ("BESSELJ", 0x4004000000000000, 0x4008000000000000, 0x3fcbb98fdf2dd19a),
        ("BESSELJ", 0x4004000000000000, 0x4014000000000000, 0x3f93f83be8afa94c),
        ("BESSELJ", 0x4004000000000000, 0x4024000000000000, 0x3ec2a99161bcc46b),
        ("BESSELJ", 0x400e000000000000, 0x4014000000000000, 0x3fbac6c6177fcf64),
        ("BESSELJ", 0x400e000000000000, 0x4024000000000000, 0x3f1c0f2c434a1438),
        ("BESSELJ", 0x400f333333333333, 0x4014000000000000, 0x3fbee75c306e54d3),
        ("BESSELJ", 0x400f333333333333, 0x4024000000000000, 0x3f24379ee1e85100),
        ("BESSELJ", 0x4014cccccccccccd, 0x4024000000000000, 0x3f60f31f5e704531),
        ("BESSELJ", 0x401ecccccccccccd, 0x4024000000000000, 0x3fa8034ca8683f6a),
        ("BESSELJ", 0x4020000000000000, 0x4024000000000000, 0x3faf1cdb1157547d),
        ("BESSELJ", 0x4020cccccccccccd, 0x4024000000000000, 0x3fb54919429574dd),
        ("BESSELJ", 0xbffb333333333333, 0x4000000000000000, 0x3fd20802e4c24e8e),
        ("BESSELJ", 0xbffb333333333333, 0x4008000000000000, 0xbfb5cc62b7b46ca2),
        ("BESSELY", 0x4020000000000000, 0x3ff0000000000000, 0xbfc43b53412fb1a7),
        ("BESSELY", 0x4020cccccccccccd, 0x3ff0000000000000, 0xbfab62524eac4415),
        ("BESSELY", 0x402899999999999a, 0x3ff0000000000000, 0xbfbe73670d39f087),
        ("BESSELY", 0x403999999999999a, 0x3ff0000000000000, 0xbfc3449d67ecca79),
        ("BESSELJ", 0x3ff8000000000000, 0x4007333333333333, 0x3fcdb50c8f979d2f),
        ("BESSELJ", 0x3ddb7cdfd9d7bdbb, 0x4000000000000000, 0x3b979ca10c924224),
        ("BESSELY", 0x41a0000000000000, 0x3ff0000000000000, 0x3ed7ff19f84f233c),
        ("BESSELJ", 0x4014000000000000, 0x416312d000000000, 0x0000000000000000),
        ("BESSELJ", 0x4020000000000000, 0x4030000000000000, 0x3f1472ebab6dc571),
        ("BESSELJ", 0x4020000000000000, 0x403f000000000000, 0x3cb86ac063ddc7eb),
        ("BESSELJ", 0x4020000000000000, 0x404f800000000000, 0x35a8fe5ab37b6cae),
        ("BESSELJ", 0x4020800000000000, 0x4024000000000000, 0x3fb300d5dbaeb63b),
        ("BESSELJ", 0x4020800000000000, 0x4030000000000000, 0x3f1f676a4bb3d2ba),
        ("BESSELJ", 0x4020800000000000, 0x403f000000000000, 0x3cceaf9805c733c6),
        ("BESSELJ", 0x4020800000000000, 0x404f800000000000, 0x35d55e0390366b87),
        ("BESSELJ", 0x4021800000000000, 0x4024000000000000, 0x3fbb36a30de05451),
        ("BESSELJ", 0x4021800000000000, 0x4030000000000000, 0x3f319d6978ce0ddd),
        ("BESSELJ", 0x4021800000000000, 0x403f000000000000, 0x3cf6376c60480333),
        ("BESSELJ", 0x4021800000000000, 0x404f800000000000, 0x362a4ddb6857c14f),
        ("BESSELJ", 0x4022000000000000, 0x4024000000000000, 0x3fbfebf3bab3bc5b),
        ("BESSELJ", 0x4022000000000000, 0x4030000000000000, 0x3f39c67e8d46212a),
        ("BESSELJ", 0x4022000000000000, 0x403f000000000000, 0x3d09ada4beb68e5f),
        ("BESSELJ", 0x4022000000000000, 0x404f800000000000, 0x36530f8f43c15be1),
        ("BESSELJ", 0x4023000000000000, 0x4024000000000000, 0x3fc51f95d1a35329),
        ("BESSELJ", 0x4023000000000000, 0x4030000000000000, 0x3f4a6cb36b1cabb6),
        ("BESSELJ", 0x4023000000000000, 0x403f000000000000, 0x3d2fde8d1278a8e4),
        ("BESSELJ", 0x4023000000000000, 0x404f800000000000, 0x36a151b1f68cd4e8),
        ("BESSELJ", 0x4024000000000000, 0x4024000000000000, 0x3fca8ee79d2f09d3),
        ("BESSELJ", 0x4024000000000000, 0x4030000000000000, 0x3f59ab73a6aa8562),
        ("BESSELJ", 0x4024000000000000, 0x403f000000000000, 0x3d521244b37a53a8),
        ("BESSELJ", 0x4024000000000000, 0x404f800000000000, 0x36ea5fd8b94e7c49),
        ("BESSELJ", 0x4028000000000000, 0x4030000000000000, 0x3f8ca786ad8a55b4),
        ("BESSELJ", 0x4028000000000000, 0x403f000000000000, 0x3dcc37f894eee87b),
        ("BESSELJ", 0x4028000000000000, 0x404f800000000000, 0x37f07a3da3c055db),
        ("BESSELJ", 0x402a000000000000, 0x4030000000000000, 0x3fa0c14d294d1bf8),
        ("BESSELJ", 0x402a000000000000, 0x403f000000000000, 0x3e013639eb600d3a),
        ("BESSELJ", 0x402a000000000000, 0x404f800000000000, 0x3862110bc762a44d),
        ("BESSELJ", 0x4030000000000000, 0x4030000000000000, 0x3fc6b6c94748fc85),
        ("BESSELJ", 0x4030000000000000, 0x403f000000000000, 0x3e847b19da378365),
        ("BESSELJ", 0x4030000000000000, 0x404f800000000000, 0x3987705b6dcbff24),
        ("BESSELJ", 0x4034000000000000, 0x403f000000000000, 0x3f07a2e88ef734b9),
        ("BESSELJ", 0x4034000000000000, 0x404f800000000000, 0x3ac00bbb63905601),
        ("BESSELJ", 0x4039000000000000, 0x403f000000000000, 0x3f781eed27ab5faf),
        ("BESSELJ", 0x4039000000000000, 0x404f800000000000, 0x3bef7880f465f2ec),
        ("BESSELJ", 0x403f000000000000, 0x403f000000000000, 0x3fc2393fcd78b7be),
        ("BESSELJ", 0x403f000000000000, 0x404f800000000000, 0x3d0729f4256bffbd),
        ("BESSELJ", 0x4040000000000000, 0x404f800000000000, 0x3d3074098ea667c4),
        ("BESSELJ", 0x4044000000000000, 0x404f800000000000, 0x3e3b4423422ea97f),
        ("BESSELJ", 0x4048000000000000, 0x404f800000000000, 0x3efaecc06a9efbcd),
        ("BESSELY", 0x419ffffffc000000, 0x3ff0000000000000, 0x3f0fe64bf9c29c37),
        ("BESSELY", 0x419ffffffe000000, 0x3ff0000000000000, 0x3f03e239764b5875),
        ("BESSELY", 0x419fffffffffffff, 0x3ff0000000000000, 0x3ed7ff1a40469a0f),
        ("BESSELY", 0x41a0000000000001, 0x3ff0000000000000, 0x3ed7ff1968603592),
        ("BESSELY", 0x41a0000002000000, 0x3ff0000000000000, 0xbf0ca8838551ac39),
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
    exact(call("BESSELY", vec![n(100.0), n(1.0)]), f64::from_bits(0xbf94dc7ab5ff838f));
    // Signed counterparts use Bessel parity, not newly captured readings.
    exact(call("BESSELJ", vec![n(-1.9), n(2.0)]), f64::from_bits(0x3fd51d81390f83eb));
    exact(call("BESSELJ", vec![n(-0.3), n(3.0)]), -f64::from_bits(0x3f42541c0c752fd9));
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
        exact(call("BESSELY", vec![n(x), n(order)]), f64::from_bits(expected));
    }
}

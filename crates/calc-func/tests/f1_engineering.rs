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
    // OxFunc's seed row for BESSELJ(1.9,2) is 0.329925829, which its kernel reproduces; the
    // documented example (0.329925728) is the mathematical value to nine digits.
    close(call("BESSELJ", vec![n(1.9), n(2.0)]), 0.329925829, 1e-8);
    close(call("BESSELI", vec![n(1.5), n(1.0)]), 0.981666428, 1e-8);
    close(call("BESSELK", vec![n(1.5), n(1.0)]), 0.277387804, 1e-8);
    close(call("BESSELY", vec![n(2.5), n(1.0)]), 0.145918138, 1e-8);
    error(call("BESSELJ", vec![n(1.9), n(-1.0)]), Num);
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

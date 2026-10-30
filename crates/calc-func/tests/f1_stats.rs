#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the descriptive statistics, the distributions, SUBTOTAL and AGGREGATE, called
//! through `calc_func::invoke`. Expected values are Excel's documented examples (the function
//! pages' sample data) and values computed from the definitions.

mod f1_support;

use calc_func::SubtotalExclusions;
use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Value};
use f1_support::*;

fn data() -> CalcValue {
    arr(vec![vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]])
}

use calc_func::value::CalcValue;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn deviations_and_means() {
    exact(call("AVEDEV", vec![data()]), 1.5);
    exact(call("DEVSQ", vec![data()]), 32.0);
    close(call("GEOMEAN", vec![arr(vec![vec![1.0, 2.0, 4.0]])]), 2.0, 1e-15);
    close(call("HARMEAN", vec![arr(vec![vec![1.0, 2.0, 4.0]])]), 12.0 / 7.0, 1e-15);
    error(call("GEOMEAN", vec![n(-1.0)]), Num);
    exact(call("MEDIAN", vec![data()]), 4.5);
    exact(call("MEDIAN", vec![n(3.0), n(1.0), n(2.0)]), 2.0);
    error(call("MEDIAN", vec![arr_values(vec![vec![t("a")]])]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn variances() {
    exact(call("STDEV.P", vec![data()]), 2.0);
    exact(call("STDEVP", vec![data()]), 2.0);
    close(call("STDEV.S", vec![data()]), 2.138089935299395, 1e-15);
    close(call("STDEV", vec![data()]), 2.138089935299395, 1e-15);
    exact(call("VAR.P", vec![data()]), 4.0);
    close(call("VAR.S", vec![data()]), 4.571428571428571, 1e-15);
    error(call("VAR.S", vec![n(1.0)]), Div0);
    // In an array the A-suffixed forms pass over text and logicals, as the plain forms do
    // (dc-bd5: Excel 16.0.20527 gives VARPA({1,TRUE,3}) 1 and STDEVPA({1,"a",3}) 1).
    let mixed = arr_values(vec![vec![n(1.0), b(true), t("x")]]);
    exact(call("VARPA", vec![mixed.clone()]), 0.0);
    exact(call("VARP", vec![mixed]), 0.0);
    let measured = arr_values(vec![vec![n(1.0), b(true), n(3.0)]]);
    exact(call("VARPA", vec![measured]), 1.0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn order_statistics() {
    exact(call("MODE.SNGL", vec![data()]), 4.0);
    exact(call("MODE", vec![n(1.0), n(2.0), n(2.0), n(1.0)]), 1.0);
    error(call("MODE.SNGL", vec![arr(vec![vec![1.0, 2.0]])]), NA);
    let (rows, cols, cells) = array(call("MODE.MULT", vec![arr(vec![vec![1.0, 3.0, 2.0, 3.0, 2.0]])]));
    assert_eq!((rows, cols, cells), (2, 1, vec![3.0, 2.0]));
    exact(call("LARGE", vec![data(), n(2.0)]), 7.0);
    exact(call("SMALL", vec![data(), n(2.0)]), 4.0);
    error(call("LARGE", vec![data(), n(9.0)]), Num);
    let five = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]);
    close(call("PERCENTILE.INC", vec![five.clone(), n(0.3)]), 2.2, 1e-15);
    close(call("PERCENTILE", vec![five.clone(), n(0.3)]), 2.2, 1e-15);
    close(call("PERCENTILE.EXC", vec![five.clone(), n(0.3)]), 1.8, 1e-15);
    exact(call("QUARTILE.INC", vec![five.clone(), n(1.0)]), 2.0);
    error(call("QUARTILE.EXC", vec![five.clone(), n(4.0)]), Num);
    exact(call("PERCENTRANK.INC", vec![five.clone(), n(2.0)]), 0.25);
    exact(call("TRIMMEAN", vec![arr(vec![(1..=10).map(f64::from).collect()]), n(0.2)]), 5.5);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ranks_read_a_reference() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0].map(n).to_vec());
    let range = sheet.area(1, 1, 8, 1);
    exact(call_on(&mut sheet, "RANK.EQ", vec![n(4.0), range.clone()]), 5.0);
    exact(call_on(&mut sheet, "RANK", vec![n(4.0), range.clone()]), 5.0);
    exact(call_on(&mut sheet, "RANK.AVG", vec![n(4.0), range.clone()]), 6.0);
    exact(call_on(&mut sheet, "RANK.EQ", vec![n(4.0), range.clone(), n(1.0)]), 2.0);
    error(call_on(&mut sheet, "RANK.EQ", vec![n(3.0), range]), NA);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn paired_statistics() {
    let x = arr(vec![vec![1.0, 2.0, 3.0]]);
    let y = arr(vec![vec![2.0, 4.0, 7.0]]);
    close(call("CORREL", vec![x.clone(), y.clone()]), 0.9933992677987828, 1e-14);
    close(call("PEARSON", vec![x.clone(), y.clone()]), 0.9933992677987828, 1e-14);
    close(call("RSQ", vec![y.clone(), x.clone()]), 0.9868421052631579, 1e-14);
    close(call("SLOPE", vec![y.clone(), x.clone()]), 2.5, 1e-15);
    close(call("INTERCEPT", vec![y.clone(), x.clone()]), -2.0 / 3.0, 1e-14);
    close(call("FORECAST", vec![n(4.0), y.clone(), x.clone()]), 28.0 / 3.0, 1e-14);
    close(call("FORECAST.LINEAR", vec![n(4.0), y.clone(), x.clone()]), 28.0 / 3.0, 1e-14);
    close(call("COVARIANCE.P", vec![x.clone(), y.clone()]), 5.0 / 3.0, 1e-15);
    close(call("COVAR", vec![x.clone(), y.clone()]), 5.0 / 3.0, 1e-15);
    close(call("COVARIANCE.S", vec![x.clone(), y.clone()]), 2.5, 1e-15);
    error(call("CORREL", vec![x, arr(vec![vec![1.0, 2.0]])]), NA);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn moments_and_transforms() {
    let d = arr(vec![vec![3.0, 4.0, 5.0, 2.0, 3.0, 4.0, 5.0, 6.0, 4.0, 7.0]]);
    close(call("KURT", vec![d.clone()]), -0.15179963720841538, 1e-12);
    close(call("SKEW", vec![d]), 0.3595430714067975, 1e-12);
    close(call("STANDARDIZE", vec![n(42.0), n(40.0), n(1.5)]), 4.0 / 3.0, 1e-15);
    error(call("STANDARDIZE", vec![n(42.0), n(40.0), n(0.0)]), Num);
    close(call("FISHER", vec![n(0.75)]), 0.9729550745276566, 1e-15);
    close(call("FISHERINV", vec![n(0.9729550745276566)]), 0.75, 1e-15);
    error(call("FISHER", vec![n(1.0)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn prob_and_frequency() {
    let xs = arr(vec![vec![0.0, 1.0, 2.0, 3.0]]);
    let ps = arr(vec![vec![0.2, 0.3, 0.1, 0.4]]);
    close(call("PROB", vec![xs.clone(), ps.clone(), n(2.0)]), 0.1, 1e-15);
    close(call("PROB", vec![xs, ps, n(1.0), n(3.0)]), 0.8, 1e-15);
    let (rows, cols, cells) = array(call(
        "FREQUENCY",
        vec![arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]), arr(vec![vec![2.0, 4.0]])],
    ));
    assert_eq!((rows, cols, cells), (3, 1, vec![2.0, 2.0, 1.0]));
    // Unsorted bins: each counts the numbers above the next smaller bin.
    let (_, _, cells) = array(call(
        "FREQUENCY",
        vec![arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]), arr(vec![vec![4.0, 2.0]])],
    ));
    assert_eq!(cells, vec![2.0, 2.0, 1.0]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regressions() {
    let y = arr(vec![vec![1.0], vec![2.0], vec![3.0]]);
    let x = arr(vec![vec![1.0], vec![2.0], vec![3.0]]);
    let (rows, cols, cells) = array(call("LINEST", vec![y.clone(), x.clone()]));
    assert_eq!((rows, cols), (1, 2));
    assert!((cells[0] - 1.0).abs() < 1e-14 && cells[1].abs() < 1e-14, "{cells:?}");
    close(call("TREND", vec![y, x.clone(), n(4.0)]), 4.0, 1e-14);
    let g = arr(vec![vec![2.0], vec![4.0], vec![8.0]]);
    close(call("GROWTH", vec![g.clone(), x.clone(), n(4.0)]), 16.0, 1e-13);
    let (_, _, cells) = array(call("LOGEST", vec![g, x]));
    assert!((cells[0] - 2.0).abs() < 1e-13 && (cells[1] - 1.0).abs() < 1e-13, "{cells:?}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn distributions_match_excel_examples() {
    close(call("NORM.S.DIST", vec![n(1.0), b(true)]), 0.8413447460685429, 1e-12);
    close(call("NORMSDIST", vec![n(1.0)]), 0.8413447460685429, 1e-12);
    close(call("NORM.DIST", vec![n(42.0), n(40.0), n(1.5), b(true)]), 0.9087887802741321, 1e-12);
    close(call("NORM.DIST", vec![n(42.0), n(40.0), n(1.5), b(false)]), 0.10934004978399575, 1e-12);
    close(call("NORM.INV", vec![n(0.908789), n(40.0), n(1.5)]), 42.000002, 1e-7);
    close(call("NORM.S.INV", vec![n(0.908789)]), 1.3333347, 1e-6);
    error(call("NORM.S.INV", vec![n(1.0)]), Num);
    close(call("LOGNORM.DIST", vec![n(4.0), n(3.5), n(1.2), b(true)]), 0.0390835557068005, 1e-10);
    close(call("CHISQ.DIST.RT", vec![n(18.307), n(10.0)]), 0.0500006, 1e-5);
    close(call("CHIDIST", vec![n(18.307), n(10.0)]), 0.0500006, 1e-5);
    close(call("T.DIST.2T", vec![n(1.959999998), n(60.0)]), 0.054644930, 1e-7);
    close(call("TDIST", vec![n(1.959999998), n(60.0), n(2.0)]), 0.054644930, 1e-7);
    close(call("F.DIST.RT", vec![n(15.2068649), n(6.0), n(4.0)]), 0.01, 1e-6);
    close(call("BINOM.DIST", vec![n(6.0), n(10.0), n(0.5), b(false)]), 0.205078125, 1e-15);
    close(call("POISSON.DIST", vec![n(2.0), n(5.0), b(false)]), 0.084224337, 1e-8);
    close(call("EXPON.DIST", vec![n(0.2), n(10.0), b(true)]), 0.864664716763387, 1e-12);
    close(call("WEIBULL.DIST", vec![n(105.0), n(20.0), n(100.0), b(true)]), 0.929581390069277, 1e-12);
    close(call("HYPGEOM.DIST", vec![n(1.0), n(4.0), n(8.0), n(20.0), b(false)]), 0.363261093911249, 1e-12);
    close(call("NEGBINOM.DIST", vec![n(10.0), n(5.0), n(0.25), b(false)]), 0.055048660375178, 1e-12);
    close(call("BETA.DIST", vec![n(2.0), n(8.0), n(10.0), b(true), n(1.0), n(3.0)]), 0.685470581, 1e-9);
    close(call("GAMMA.DIST", vec![n(10.00001131), n(9.0), n(2.0), b(true)]), 0.068094, 1e-5);
    exact(call("GAMMA", vec![n(5.0)]), 24.0);
    close(call("GAMMALN", vec![n(4.0)]), 6.0f64.ln(), 1e-15);
    close(call("CONFIDENCE.NORM", vec![n(0.05), n(2.5), n(50.0)]), 0.692951912174839, 1e-12);
    close(call("GAUSS", vec![n(2.0)]), 0.477249868051821, 1e-12);
    close(call("PHI", vec![n(0.75)]), 0.301137432154804, 1e-12);
    close(call("ERF", vec![n(1.0)]), 0.8427007929497149, 1e-14);
    close(call("ERFC", vec![n(1.0)]), 0.1572992070502851, 1e-14);
    close(call("ERF", vec![n(0.0), n(1.0)]), 0.8427007929497149, 1e-14);
    // A cumulative flag is read as a number.
    error(call("NORM.S.DIST", vec![n(1.0), t("yes")]), Value);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn tests_of_samples() {
    let a1 = arr(vec![vec![3.0, 4.0, 5.0, 8.0, 9.0, 1.0, 2.0, 4.0, 5.0]]);
    let a2 = arr(vec![vec![6.0, 19.0, 3.0, 2.0, 14.0, 4.0, 5.0, 17.0, 1.0]]);
    close(call("T.TEST", vec![a1.clone(), a2.clone(), n(2.0), n(1.0)]), 0.196015785, 1e-8);
    close(call("F.TEST", vec![arr(vec![vec![6.0, 7.0, 9.0, 15.0, 21.0]]), arr(vec![vec![20.0, 28.0, 31.0, 38.0, 40.0]])]), 0.648317846786174, 1e-12);
    error(call("T.TEST", vec![a1, a2, n(2.0), n(4.0)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn subtotal_needs_the_readers_exclusions() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(1.0), n(2.0), n(3.0), n(10.0)]);
    let range = sheet.area(1, 1, 4, 1);
    // A reader that cannot say which cells hold a SUBTOTAL formula: refused, not guessed.
    refused(call_on(&mut sheet, "SUBTOTAL", vec![n(9.0), range.clone()]));
    // A4 holds a nested SUBTOTAL; row 2 is hidden by hand.
    let a4 = sheet.key(4, 1);
    sheet.exclusions = Some(SubtotalExclusions {
        nested_subtotals: vec![a4],
        hidden_rows: vec![2],
        filtered_rows: vec![],
    });
    exact(call_on(&mut sheet, "SUBTOTAL", vec![n(9.0), range.clone()]), 6.0);
    exact(call_on(&mut sheet, "SUBTOTAL", vec![n(109.0), range.clone()]), 4.0);
    exact(call_on(&mut sheet, "SUBTOTAL", vec![n(1.0), range.clone()]), 2.0);
    exact(call_on(&mut sheet, "SUBTOTAL", vec![n(4.0), range.clone()]), 3.0);
    exact(call_on(&mut sheet, "SUBTOTAL", vec![n(103.0), range.clone()]), 2.0);
    error(call_on(&mut sheet, "SUBTOTAL", vec![n(12.0), range.clone()]), Value);
    // AGGREGATE's option 4 leaves nothing out, so it needs no exclusions.
    sheet.exclusions = None;
    exact(call_on(&mut sheet, "AGGREGATE", vec![n(9.0), n(4.0), range.clone()]), 16.0);
    refused(call_on(&mut sheet, "AGGREGATE", vec![n(9.0), n(0.0), range]));
    // The array form with option 6 leaves out errors.
    let with_error = arr_values(vec![vec![n(1.0), e(Div0), n(3.0)]]);
    exact(call("AGGREGATE", vec![n(14.0), n(6.0), with_error.clone(), n(1.0)]), 3.0);
    error(call("AGGREGATE", vec![n(14.0), n(4.0), with_error, n(1.0)]), Div0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rounding_to_a_significance() {
    exact(call("CEILING.MATH", vec![n(-8.1), n(2.0)]), -8.0);
    exact(call("CEILING.MATH", vec![n(-5.5), n(2.0), n(-1.0)]), -6.0);
    exact(call("CEILING.MATH", vec![n(24.3), n(5.0)]), 25.0);
    exact(call("FLOOR.MATH", vec![n(-8.1), n(2.0)]), -10.0);
    exact(call("FLOOR.MATH", vec![n(-5.5), n(2.0), n(-1.0)]), -4.0);
    exact(call("CEILING.PRECISE", vec![n(4.3)]), 5.0);
    exact(call("CEILING.PRECISE", vec![n(-4.3), n(2.0)]), -4.0);
    exact(call("ISO.CEILING", vec![n(-4.3), n(2.0)]), -4.0);
    exact(call("FLOOR.PRECISE", vec![n(-3.2), n(-1.0)]), -4.0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn random_numbers_draw_from_the_services_source() {
    refused(call("RAND", vec![]));
    exact(call_random(&mut Fixed(vec![0.25]), "RAND", vec![]), 0.25);
    refused(call_random(&mut Fixed(vec![1.0]), "RAND", vec![]));
    exact(call_random(&mut Fixed(vec![0.5]), "RANDBETWEEN", vec![n(1.0), n(10.0)]), 6.0);
    exact(call_random(&mut Fixed(vec![0.0]), "RANDBETWEEN", vec![n(1.5), n(10.0)]), 2.0);
    error(call_random(&mut Fixed(vec![0.5]), "RANDBETWEEN", vec![n(10.0), n(1.0)]), Num);
    let (rows, cols, cells) = array(call_random(
        &mut Fixed(vec![0.0, 0.5, 0.75, 0.99]),
        "RANDARRAY",
        vec![n(2.0), n(2.0), n(1.0), n(4.0), b(true)],
    ));
    assert_eq!((rows, cols, cells), (2, 2, vec![1.0, 3.0, 4.0, 4.0]));
}

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
fn gamma_small_inputs_record_known_gaps() {
    // Original recorded answers: one gain from each grouped-input half and a
    // matched control three ULP from the unconditional Taylor-series result.
    for (input, expected, calculated) in [
        (0x3f1a_529c_35ef_4724, 0x40c3_7323_fd56_1ac3, 0x40c37323fd561ac3),
        (0x3f1b_2638_e4e6_7c49, 0x40c2_db88_6b75_1adc, 0x40c2db886b751adc),
        (0x3f8f_e551_0015_81b1, 0x404f_d2d4_c62f_1d4e, 0x404fd2d4c62f1d4b),
    ] {
        recorded_calculation(call("GAMMA", vec![n(f64::from_bits(input))]), f64::from_bits(expected), f64::from_bits(calculated));
    }
}

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
fn regression_discards_a_predictor_below_the_dot_product_roundoff_bound() {
    // Recorded catalogue dcj9k_regression: this second predictor is nearly
    // twice the first. Excel drops the first after pivoting to the larger
    // column, leaving three residual degrees of freedom without a constant.
    let y = arr(vec![vec![1.0], vec![2.0], vec![4.0], vec![7.0]]);
    let x = arr(vec![vec![1.0, 2.0], vec![2.0, 4.0], vec![5.0, 10.0], vec![4.0, 8.000001]]);
    for function in ["LINEST", "LOGEST"] {
        let result = call(function, vec![y.clone(), x.clone(), b(false), b(true)]).unwrap();
        let cells = result.as_array().expect("regression statistics array");
        assert_eq!((cells.shape().rows, cells.shape().cols), (5, 3));
        exact(Ok(cells.get(3, 1).unwrap().clone()), 3.0);
        exact(Ok(cells.get(0, 1).unwrap().clone()), if function == "LOGEST" { 1.0 } else { 0.0 });
        exact(Ok(cells.get(1, 1).unwrap().clone()), 0.0);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regression_keeps_scaled_independent_predictors_from_r16() {
    // Original R16 capture rows649..672. Changing the units of the second
    // predictor does not make this recorded design rank deficient in Excel.
    let y = arr(vec![vec![1.0], vec![2.0], vec![4.0], vec![7.0]]);
    for factor in [2.0_f64.powi(-20), 2.0_f64.powi(20)] {
        let x = arr([1.0, 2.0, 5.0, 4.0].into_iter().enumerate().map(|(i, v)| {
            vec![v, (2.0*v + if i == 3 { 2.0_f64.powi(-16) } else { 0.0 })*factor]
        }).collect());
        for constant in [false, true] {
            for function in ["LINEST", "LOGEST"] {
                let result = call(function, vec![y.clone(), x.clone(), b(constant), b(true)]).unwrap();
                let cells = result.as_array().expect("regression statistics array");
                exact(Ok(cells.get(3, 1).unwrap().clone()), if constant { 1.0 } else { 2.0 });
            }
        }
    }
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
    // fresh-negbinom.dist-00113-fresh-endpoints: equality uses the deviance series.
    exact(call("NEGBINOM.DIST", vec![n(4.0), n(11.0), n(0.75), b(true)]),
        f64::from_bits(0x3fe5_f7b1_5d80_0000));
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

/// Recorded build-20131 rows oc1540/oc35142/oc35121 and W111 product-overflow rows.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn exponential_distribution_uses_the_recorded_arithmetic_stores() {
    for (x, lambda, cumulative, expected) in [
        (0x3e9f46de57161763, 0x3ff0000000000000, true, 0x3e9f46dddcce69b3),
        (0x3eeaf5c72d71a9d8, 0x3fc2294a8d5b7317, true, 0x3ebe9a11ac30f36e),
        (0x3ec7bfe8f913eebb, 0x3ff406b01029c9a0, false, 0x3ff406ab699425f4),
    ] {
        exact(call("EXPON.DIST", vec![n(f64::from_bits(x)), n(f64::from_bits(lambda)), b(cumulative)]), f64::from_bits(expected));
    }
    for (x, lambda, cumulative) in [
        (0x418d8b21f25875a1, 0x7f685d78a25c9e53, true),
        (0x42577d4b99895077, 0x7db09552fcb9586c, false),
    ] {
        error(call("EXPON.DIST", vec![n(f64::from_bits(x)), n(f64::from_bits(lambda)), b(cumulative)]), Num);
    }
    // The shape-one gamma CDF calls the same exponential kernel (recorded row oc61017).
    exact(call("GAMMA.DIST", vec![n(f64::from_bits(0x3e54df1215fc525b)), n(1.0), n(1.0), b(true)]), f64::from_bits(0x3e54df12129517f4));
}

/// Recorded GAUSS subnormal-power rows and ERFC tail controls (W109/W111).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn normal_error_functions_publish_recorded_underflows() {
    for bits in [1, 2, 0x0008_0000_0000_0000, 0x8008_0000_0000_0000] {
        exact(call("GAUSS", vec![n(f64::from_bits(bits))]), 0.0);
    }
    for function in ["ERFC", "ERFC.PRECISE"] {
        for bits in [0x403a_c3c9_6948_f741, 0x403b_0000_0000_0000] {
            exact(call(function, vec![n(f64::from_bits(bits))]), 0.0);
        }
    }
}

/// Recorded ERFC dependencies and normal tails; original answers and known gaps.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn error_function_tails_record_known_gaps() {
    // These ERFC arguments belong to different halves of the frozen split.
    for function in ["ERFC", "ERFC.PRECISE"] {
        recorded_calculation(call(function, vec![n(f64::from_bits(0x4021_0c8a_deec_196e))]), f64::from_bits(0x3922_d992_80b1_9df9), f64::from_bits(0x3922d99280b19e1e));
        recorded_calculation(call(function, vec![n(f64::from_bits(0x4024_2bc8_1cf3_8194))]), f64::from_bits(0x3680_fa89_4f89_d6df), f64::from_bits(0x3680fa894f89d6ff));
        // Residuals between 2^-49 and 2^-48, from both split halves.
        recorded_calculation(call(function, vec![n(f64::from_bits(0x4019_3db4_8c78_fb1d))]), f64::from_bits(0x3c20_95a4_42f6_d57d), f64::from_bits(0x3c2095a442f6d571));
        recorded_calculation(call(function, vec![n(f64::from_bits(0x401c_c218_61f7_e239))]), f64::from_bits(0x3b0a_c48e_8af8_7271), f64::from_bits(0x3b0ac48e8af87259));
        // Residuals between 2^-50 and 2^-49, from both split halves.
        recorded_calculation(call(function, vec![n(f64::from_bits(0x4010_5ad1_5827_f62f))]), f64::from_bits(0x3e3f_a6da_a7f6_9fce), f64::from_bits(0x3e3fa6daa7f69fc5));
        recorded_calculation(call(function, vec![n(f64::from_bits(0x4010_836a_af1b_be42))]), f64::from_bits(0x3e36_a4ab_c77f_b3ec), f64::from_bits(0x3e36a4abc77fb3f7));
    }
    recorded_calculation(call("NORM.S.DIST", vec![
        n(f64::from_bits(0xc026_a496_5d40_5589)), b(true),
    ]), f64::from_bits(0x39da_0ca4_2246_3668), f64::from_bits(0x39da0ca422463696));
    recorded_calculation(call("NORM.S.DIST", vec![
        n(f64::from_bits(0xc020_0893_4d02_54c4)), b(true),
    ]), f64::from_bits(0x3cc3_8f68_673d_153e), f64::from_bits(0x3cc38f68673d1530));
    recorded_calculation(call("NORM.S.DIST", vec![
        n(f64::from_bits(0xc016_a24f_cfb0_a98c)), b(true),
    ]), f64::from_bits(0x3e40_653f_58c2_faf3), f64::from_bits(0x3e40653f58c2faeb));
    recorded_calculation(call("NORM.S.DIST", vec![
        n(f64::from_bits(0xc016_aa4e_776d_04bd)), b(true),
    ]), f64::from_bits(0x3e3f_5501_63f6_fedc), f64::from_bits(0x3e3f550163f6fee8));
    // Recorded oc10307 distinguishes the PC64 square in the residual.
    recorded_calculation(call("NORM.S.DIST", vec![n(-17.145244843201656), b(true)]), f64::from_bits(0x3256_fcaf_346e_9729), f64::from_bits(0x3256fcaf346e96cd));
    // The same staging reaches the normal wrappers (recorded oc2613).
    for function in ["NORM.DIST", "NORMDIST"] {
        recorded_calculation(call(function, vec![n(-33.26358352947733), n(-3.934034287489425),
            n(3.7542580713919134), b(true)]), f64::from_bits(0x3ce9_47e8_34b5_2568), f64::from_bits(0x3ce947e834b5255b));
    }
}

/// Original GAMMA answers from both former fitting halves, with known gaps.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_generic_body_records_known_gaps() {
    for (input, expected, calculated) in [
        (0x400d_1554_af11_39b0, 0x400e_f676_6ccb_fa92, 0x400ef6766ccbfa92),
        (0x400d_4f88_34b9_3ec3, 0x400f_fe1e_2ca0_074c, 0x400ffe1e2ca0074c),
        (0x4044_be47_5963_23ad, 0x4a0b_1f25_9b34_f3ae, 0x4a0b1f259b34f3af),
        (0x405c_03e8_db5e_238e, 0x6562_21fb_163a_ffa9, 0x656221fb163affa9),
        // Positive tgamma refinement, from both grouped-argument halves.
        (0x3f21_ee1e_5eb2_9a49, 0x40bc_8d91_582d_997d, 0x40bc8d91582d997d),
        (0x3f1f_f366_0a69_738e, 0x40c0_0605_98b3_6e3b, 0x40c0060598b36e3a),
        // The same bound admits reduced reflection and the negative fallback.
        (0xc060_bad0_74be_eaa3, 0x10c5_b33f_ccd8_3432, 0x10c5b33fccd83432),
        (0xbfff_b384_bc86_8d32, 0x403b_41c1_68e1_0426, 0x403b41c168e10425),
        // Old matches differ from the rational by one ULP; keep their route.
        (0x400c_14f6_1d18_79ae, 0x400a_e3a0_184f_4257, 0x400ae3a0184f4255),
        (0x400d_7b7b_3b82_609e, 0x4010_6635_56ba_cf83, 0x4010663556bacf82),
        (0xc02d_5a3f_39dd_c52f, 0xbd9e_116c_a682_f364, 0xbd9e116ca682f364),
        // Negative inputs retain the larger bound.
        (0xbff1_1541_05b4_a44e, 0x402c_e385_d327_12a8, 0x402ce385d32712a8),
    ] {
        recorded_calculation(call("GAMMA", vec![n(f64::from_bits(input))]), f64::from_bits(expected), f64::from_bits(calculated));
    }
}

/// Recorded densities retain the gaps exposed by removing normalizer selection.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_density_records_known_gaps() {
    for (x, alpha, expected, calculated) in [
        (0.12417919307712058, 6.423680034363982, 0x3e67_0863_f2ee_9357, 0x3e670863f2ee9357),
        (0.8537609680018344, 7.171703572068208, 0x3f25_2274_a7b8_6ecb, 0x3f252274a7b86ecc),
        (9.563436515180983, 6.15888078874286, 0x3faa_1da4_7b5c_d5ed, 0x3faa1da47b5cd5e7),
        // A lower scalar-Gamma bound would lose these original PDF matches.
        (0.0003421170430624811, 0.943075920351938, 0x3ff8_4f92_2b67_1613, 0x3ff84f922b671613),
        (0.007825340264827977, 0.622814265063597, 0x4011_2efe_0987_a993, 0x40112efe0987a994),
    ] {
        recorded_calculation(call("GAMMA.DIST", vec![n(x), n(alpha), n(1.0), b(false)]), f64::from_bits(expected), f64::from_bits(calculated));
    }
}

/// Recorded GAMMA.DIST density row; the shape-one log normalizer is exactly zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_density_uses_the_exact_shape_one_normalizer() {
    exact(call("GAMMA.DIST", vec![
        n(f64::from_bits(0x3f0b11ad0237a590)), n(1.0),
        n(f64::from_bits(0x3fce00f3acb82fad)), b(false),
    ]), f64::from_bits(0x4011_0f90_28e4_bd60));
}

/// Recorded MOD tiny-result and subnormal-admission rows from W111.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mod_reports_a_subnormal_remainder_as_num() {
    // W111 discriminator 24222: the raw remainder underflows before sign adjustment.
    error(call("MOD", vec![
        n(f64::from_bits(0x0014_c742_6ee3_166b)),
        n(f64::from_bits(0x8014_0ff4_c89d_5c9a)),
    ]), Num);
    error(call("MOD", vec![n(f64::from_bits(0x0010_0000_0000_0000)), n(f64::from_bits(0x8010_0000_0000_0001))]), Num);
    error(call("MOD", vec![n(f64::from_bits(1)), n(0.0)]), Div0);
    exact(call("MOD", vec![n(f64::from_bits(1)), n(0.1)]), 0.0);
    // W111 tiny-00005 and refinement-00006/01014 separate both binary boundaries: a remainder
    // below 2^-1026 is added to the divisor, and one above it is taken from it.
    exact(
        call("MOD", vec![n(f64::from_bits(0x0010_0000_0000_0001)), n(f64::from_bits(0x8010_0000_0000_0000))]),
        f64::from_bits(0x8010_0000_0000_0001),
    );
    error(call("MOD", vec![n(f64::from_bits(0x0011_0000_0000_0000)), n(f64::from_bits(0x8010_0000_0000_0000))]), Num);
    error(call("MOD", vec![n(f64::from_bits(0x01b0_0000_0000_0001)), n(f64::from_bits(0x8010_0000_0000_0000))]), Num);
    // A binary-power divisor preserves a tiny intermediate with a normal final remainder.
    exact(call("MOD", vec![n(f64::from_bits(0x0020800000000000)), n(f64::from_bits(0x8020000000000000))]), f64::from_bits(0x801f_0000_0000_0000));
}

/// Recorded subnormal inputs reach the measured policies of these functions.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rounding_and_log_gamma_handle_recorded_subnormal_inputs() {
    for bits in [1, 0x8000_0000_0000_0001] {
        for function in ["ROUND", "ROUNDUP", "ROUNDDOWN", "TRUNC"] {
            exact(call(function, vec![n(f64::from_bits(bits)), n(0.0)]), 0.0);
        }
        for function in ["GAMMALN", "GAMMALN.PRECISE"] {
            error(call(function, vec![n(f64::from_bits(bits))]), Num);
        }
    }
}

/// W111 zero-count Poisson exponential-boundary readings retain the exponential's bits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_zero_count_retains_recorded_subnormal_probabilities() {
    for function in ["POISSON.DIST", "POISSON"] {
        for cumulative in [false, true] {
            exact(call(function, vec![n(0.0), n(f64::from_bits(0x4086_232b_dd7a_bcd3)), b(cumulative)]), f64::from_bits(0x000f_ffff_ffff_fe7c));
        }
    }
}

/// One recorded semantic input, three count spellings; oc30003 is an old match.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_first_mass_tiny_mean_matches_recorded_probability() {
    let mean = f64::from_bits(0x3d70_0000_0000_0000);
    let expected = f64::from_bits(0x3d6f_ffff_ffff_e000);
    for function in ["POISSON.DIST", "POISSON"] {
        for count_bits in [0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0001, 0x3ffc_0000_0000_0000] {
            exact(call(function, vec![n(f64::from_bits(count_bits)), n(mean), b(false)]), expected);
        }
        exact(call(function, vec![n(1.0), n(f64::from_bits(0x3eb0_f2c7_411e_5587)), b(false)]),
            f64::from_bits(0x3eb0_f2c6_21df_3991));
        exact(call(function, vec![n(1.0), n(mean), b(true)]), 1.0);
    }
}

/// Exponential-series mathematical controls, not additional Excel readings.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_first_mass_tiny_mean_has_analytic_rounding() {
    // For normal lambda <= 2^-54 the true probability rounds to lambda.
    for function in ["POISSON.DIST", "POISSON"] {
        for mean_bits in [0x0010_0000_0000_0000, 0x3af0_0000_0000_0000,
                          0x3c80_0000_0000_0000, 0x3c8f_ffff_ffff_ffff, 0x3c90_0000_0000_0000] {
            let mean = f64::from_bits(mean_bits);
            exact(call(function, vec![n(1.0), n(mean), b(false)]), mean);
        }
    }
}

/// Recorded cumulative gamma row: the final series product has an x87 store.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_cumulative_stores_the_final_series_product() {
    exact(call("GAMMA.DIST", vec![n(f64::from_bits(0x3ee4e69846c122d8)), n(f64::from_bits(0x4000000000000000)), n(f64::from_bits(0x3ff0000000000000)), b(true)]), f64::from_bits(0x3dcb4d6c2d100466));
}

/// Original catalogue dc96l3_discrete!D178: POISSON.DIST(33,33.3,TRUE).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_cumulative_retains_the_temme_correction() {
    for function in ["POISSON.DIST", "POISSON"] {
        exact(call(function, vec![n(33.0), n(33.3), b(true)]), f64::from_bits(0x3fe0_cfa4_9a24_03b8));
    }
}

/// Recorded lower-series gains and protected controls from both argument halves.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_cumulative_records_known_subtraction_gaps() {
    for (x, shape, expected, calculated) in [
        // oc820 (excluded half 1), oc837 (fitting half 0).
        (0x3e45_bfed_3f9f_5d2c, 2.0, 0x3c8d_90cd_01b5_dad1, 0x3c8d90cd01b5dad1),
        (0x3e46_4f60_ae25_7bea, 3.0, 0x3acc_eb10_4395_7fa3, 0x3acceb1043957fa4),
        // oc41045 / oc40295: unbounded correction would lose these matches.
        (0x3fb0_eec1_cb3a_8a9b, 3.0, 0x3f08_1056_fc2c_909a, 0x3f081056fc2c9099),
        (0x3fb0_e0e8_3a02_c89a, 3.0, 0x3f07_d671_a72d_b3af, 0x3f07d671a72db3ae),
    ] {
        for function in ["GAMMA.DIST", "GAMMADIST"] {
            recorded_calculation(call(function, vec![n(f64::from_bits(x)), n(shape), n(1.0), b(true)]),
                f64::from_bits(expected), f64::from_bits(calculated));
        }
    }
}

/// Recorded oc3066 distinguishes the logarithm composed into the lognormal CDF.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn lognormal_uses_its_internal_logarithm() {
    let x = n(f64::from_bits(0x3fe2_57a6_22fc_740d));
    let expected = f64::from_bits(0x3fd2_7dc8_2b76_c882);
    exact(call("LOGNORM.DIST", vec![x.clone(), n(0.0), n(1.0), b(true)]), expected);
    exact(call("LOGNORMDIST", vec![x, n(0.0), n(1.0)]), expected);
}

/// Build-20131 rows oc66883 and oc69092 distinguish the interior mass's logarithm.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_mass_uses_the_internal_normalization_logarithm() {
    for (k, p, expected) in [
        (2.0, 0x3ebd_5eba_c7b2_a5ce, 0x3e1d_88cb_f5d2_8843),
        (32.0, 0x3fef_ffe3_e30c_d521, 0x3e7b_0ce6_d3d6_58f5),
    ] {
        for function in ["BINOM.DIST", "BINOMDIST"] {
            exact(call(function, vec![n(k), n(34.0), n(f64::from_bits(p)), b(false)]),
                f64::from_bits(expected));
        }
    }
    // oc16921 distinguishes the asymptotic leading Stirling coefficient.
    for function in ["BINOM.DIST", "BINOMDIST"] {
        exact(call(function, vec![n(3.0), n(16.0), n(f64::from_bits(0x3f81_4b09_a96b_5f50)), b(false)]),
            f64::from_bits(0x3f33_c9f8_44fe_e7e4));
    }
    // oc48161 distinguishes the initial deviance-series product store.
    for function in ["BINOM.DIST", "BINOMDIST"] {
        exact(call(function, vec![n(3.0), n(48.0), n(f64::from_bits(0x3fb2_6666_6666_6763)), b(false)]),
            f64::from_bits(0x3fcc_a757_9b2b_c297));
    }
}

/// Recorded tiny ERF answers: the stored-product route below 2^-27 (dc-j9k.6) reproduces them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn tiny_erf_matches_recorded_answers() {
    for (input, expected) in [
        (0x3d70_5e80_0000_0000, 0x3d72_7879_0ff4_a4d4),
        (0x3d73_5c29_021a_4811, 0x3d75_d86d_8b33_b7f4),
    ] {
        exact(call("ERF.PRECISE", vec![n(f64::from_bits(input))]), f64::from_bits(expected));
        exact(call("ERF", vec![n(f64::from_bits(input))]), f64::from_bits(expected));
    }
    // gauss-discovery-00059: the tiny wrapper uses the same ERF calculation.
    exact(call("GAUSS", vec![n(f64::from_bits(0x005f_e189_b32b_8147))]), f64::from_bits(0x0049_6ff7_0f27_3fff));
}

/// Recorded b17-s20-03582 and b16-g25-01840: the linear route below 2^-16 (dc-j9k.6).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn small_erf_matches_recorded_answers() {
    for (input, expected) in [
        (0x3eb4_5231_7b1d_dc40, 0x3eb6_ee0b_e91e_09f3),
        (0x3e61_cc00_0000_0000, 0x3e64_14e5_3e86_1661),
    ] {
        for function in ["ERF.PRECISE", "ERF"] {
            exact(call(function, vec![n(f64::from_bits(input))]), f64::from_bits(expected));
        }
    }
}

/// Recorded oc1302 and smallz-0018 retain the removed model's curvature clues.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn small_erf_records_known_curvature_gaps() {
    for (input, expected, calculated) in [
        (0x3f1c_0000_0000_0000, 0x3f1f_9838_ca70_8460, 0x3f1f9838ca70845f),
        (0x3f83_0000_0000_0000, 0x3f85_7047_602b_fba1, 0x3f857047602bfba0),
    ] {
        for function in ["ERF.PRECISE", "ERF"] {
            recorded_calculation(call(function, vec![n(f64::from_bits(input))]), f64::from_bits(expected), f64::from_bits(calculated));
        }
    }
}

/// Recorded beta rows oc1582/oc640 and adjacent FDIST refinement inputs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_and_f_tails_keep_the_recorded_arithmetic_stores() {
    for (x, alpha, beta, expected) in [
        (0x3fd2_d8a7_e6f4_345c, 0x4000_0000_0000_0000, 0x4014_0000_0000_0000, 0x3fe2_2b76_1604_a6c4),
        (0x3fef_8f5e_328d_786c, 0x4049_84b8_81a3_be4a, 0x3fa9_9999_9999_999a, 0x3f93_d7f8_5447_ca8d),
    ] {
        exact(call("BETA.DIST", vec![n(f64::from_bits(x)), n(f64::from_bits(alpha)), n(f64::from_bits(beta)), b(true)]),
            f64::from_bits(expected));
    }
    for x in [0x3ff3_72a3_6987_efc6, 0x3ff3_72a3_6987_efc7] {
        exact(call("FDIST", vec![n(f64::from_bits(x)), n(5.0), n(6.0)]),
            f64::from_bits(0x3fd9_d035_ed43_a9d5));
    }
}

/// Recorded oc162/oc2552 distinguish the carries; fcdf-discovery-162 rejects an ALNREL carry.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_normalizer_and_fraction_carry_selected_products_into_the_sum() {
    exact(call("BETA.DIST", vec![n(0.75), n(50.0), n(0.2), b(true)]),
        f64::from_bits(0x3e50_c82c_2ab0_a678));
    exact(call("BETA.DIST", vec![
        n(f64::from_bits(0x3ffb_33f9_cca7_6a46)),
        n(f64::from_bits(0x400d_2340_d24a_6dd6)),
        n(f64::from_bits(0x4010_8821_4494_de74)), b(true),
        n(f64::from_bits(0xbfec_78fe_89bf_57d0)),
        n(f64::from_bits(0x4010_2ad2_a17e_4f43)),
    ]), f64::from_bits(0x3fe4_0a1a_47f6_ea10));
    exact(call("F.DIST", vec![n(f64::from_bits(0x3ff5_3020_c3d3_d95f)), n(4.0), n(11.0), b(true)]),
        f64::from_bits(0x3fe5_bb90_637b_4cd0));
    // oc2325 distinguishes the earlier Horner product in GAMln1's small-a arm.
    exact(call("BETA.DIST", vec![
        n(f64::from_bits(0xbfe4_e784_cb53_6e3c)),
        n(f64::from_bits(0x3ff4_5e5f_705d_d315)),
        n(f64::from_bits(0x3ff3_9d61_280e_fe34)), b(true),
        n(f64::from_bits(0xbfff_d2b1_1995_df92)),
        n(f64::from_bits(0x4006_c206_44e9_7df9)),
    ]), f64::from_bits(0x3fce_646a_71b7_08b3));
    // oc114: the ALNREL boundary includes magnitude 0.375.
    exact(call("BETA.DIST", vec![n(0.375), n(3.0), n(7.0), b(true)]),
        f64::from_bits(0x3fe6_fc1b_b000_0000));
    // oc58 and fcdf-retired_wrapper-022 distinguish beta parameter 2.
    exact(call("FDIST", vec![n(2.7), n(8.0), n(4.0)]),
        f64::from_bits(0x3fc6_94c2_ffff_fffe));
    exact(call("F.DIST", vec![n(4.0), n(4.0), n(8.0), b(true)]),
        f64::from_bits(0x3fee_8d2b_3183_aff0));
}

/// Original catalogue answers, including every recorded selection of both fits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn linest_stored_reflector_matches_original_excel_selections() {
    {
        // corpus:calc-func_catalogue_oracle.xlsx:dcj9k_linest_stats:1091:4 and the other original selections of this fit.
        let result = call("LINEST", vec![arr(vec![vec![-17.979], vec![-0.126], vec![8.0], vec![-5.0], vec![1.7], vec![37.0], vec![12.4], vec![14.46], vec![-8.0], vec![24.2]]),
            arr(vec![vec![20.8, 50.0], vec![34.0, 23.41], vec![38.875, -17.7], vec![4.863, 36.9], vec![42.1, 6.0], vec![-14.55, 44.41], vec![-4.57, -12.96], vec![-9.909, 2.589], vec![47.549, 42.0], vec![54.9, -17.63]]), b(true), b(true)]).expect("LINEST array");
        let values = result.as_array().expect("LINEST array");
        for (row, column, expected) in [
            (1, 1, "0xbfd27fdb7ef796a7"),
            (1, 2, "0xbfd1a33c9c5095e3"),
            (1, 3, "0x40311a7710d73964"),
            (2, 1, "0x3fc863a97e13b089"),
            (2, 2, "0x3fc9963b81442b03"),
            (2, 3, "0x401dc5ac009d3792"),
            (3, 1, "0x3fd5207539f192f3"),
            (3, 2, "0x402e02041aa086c9"),
            (3, 3, "error:NA"),
            (4, 1, "0x3ffb98672c7a3bc3"),
            (4, 2, "0x401c000000000000"),
            (4, 3, "error:NA"),
            (5, 1, "0x4088443548feaaee"),
            (5, 2, "0x40989f4ed822bc97"),
            (5, 3, "error:NA"),
        ] {
            let actual = values.get(row - 1, column - 1).expect("recorded selection");
            assert_eq!(calc_verdict::encode_result(actual).as_deref(), Some(expected), "row={row} column={column}");
        }
    }
    {
        // corpus:calc-func_catalogue_oracle.xlsx:dcj9k_linest_stats:1176:4 and the other original selections of this fit.
        let result = call("LINEST", vec![arr(vec![vec![31.78], vec![5.61], vec![-9.09]]),
            arr(vec![vec![53.56, 30.22, 32.0], vec![-9.0, -17.977, 47.17], vec![9.4, -11.83, 39.39]]), b(false), b(true)]).expect("LINEST array");
        let values = result.as_array().expect("LINEST array");
        for (row, column, expected) in [
            (1, 1, "0x3fe9f2f302983561"),
            (1, 2, "0x4003a2d805c5bf11"),
            (1, 3, "0xbff46aa36a83eafe"),
            (1, 4, "0x0000000000000000"),
            (2, 1, "0x0000000000000000"),
            (2, 2, "0x0000000000000000"),
            (2, 3, "0x0000000000000000"),
            (2, 4, "error:NA"),
            (3, 1, "0x3ff0000000000000"),
            (3, 2, "0x0000000000000000"),
            (3, 3, "error:NA"),
            (3, 4, "error:NA"),
            (4, 1, "error:Num"),
            (4, 2, "0x0000000000000000"),
            (4, 3, "error:NA"),
            (4, 4, "error:NA"),
            (5, 1, "0x409190463f141206"),
            (5, 2, "0x0000000000000000"),
            (5, 3, "error:NA"),
            (5, 4, "error:NA"),
        ] {
            let actual = values.get(row - 1, column - 1).expect("recorded selection");
            assert_eq!(calc_verdict::encode_result(actual).as_deref(), Some(expected), "row={row} column={column}");
        }
    }
}

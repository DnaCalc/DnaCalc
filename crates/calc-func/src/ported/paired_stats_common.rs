use crate::value::WorksheetErrorCode;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CovarianceDivisor {
    Population,
    Sample,
}

fn means(pairs: &[(f64, f64)]) -> Result<(f64, f64), WorksheetErrorCode> {
    means_using(pairs, |a, b| a + b, |a, b| a / b)
}

fn means_using(
    pairs: &[(f64, f64)],
    add: fn(f64, f64) -> f64,
    divide: fn(f64, f64) -> f64,
) -> Result<(f64, f64), WorksheetErrorCode> {
    if pairs.is_empty() {
        return Err(WorksheetErrorCode::Div0);
    }
    let (sum_x, sum_y) = pairs
        .iter()
        .fold((0.0, 0.0), |(ax, ay), (x, y)| (add(ax, *x), add(ay, *y)));
    let n = pairs.len() as f64;
    Ok((divide(sum_x, n), divide(sum_y, n)))
}

pub fn covariance_from_pairs(
    pairs: &[(f64, f64)],
    divisor: CovarianceDivisor,
) -> Result<f64, WorksheetErrorCode> {
    if pairs.is_empty() {
        return Err(WorksheetErrorCode::Div0);
    }
    if matches!(divisor, CovarianceDivisor::Sample) && pairs.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }

    use crate::ported::excel_numeric::{
        excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul, excel_x87_sub as sub,
    };
    let (mean_x, mean_y) = means_using(pairs, add, div)?;
    let sum = pairs
        .iter()
        .fold(0.0, |acc, (x, y)| add(acc, mul(sub(*x, mean_x), sub(*y, mean_y))));
    if !sum.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    let denom = match divisor {
        CovarianceDivisor::Population => pairs.len() as f64,
        CovarianceDivisor::Sample => (pairs.len() - 1) as f64,
    };
    Ok(div(sum, denom))
}

pub fn correlation_from_pairs(pairs: &[(f64, f64)]) -> Result<f64, WorksheetErrorCode> {
    correlation_with_divisor(pairs, CovarianceDivisor::Population)
}

fn correlation_with_divisor(
    pairs: &[(f64, f64)],
    divisor: CovarianceDivisor,
) -> Result<f64, WorksheetErrorCode> {
    use crate::ported::excel_numeric::{
        excel_sqrt_staged as sqrt, excel_x87_add as add, excel_x87_div as div,
        excel_x87_mul as mul, excel_x87_sub as sub,
    };
    if pairs.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }
    let (mean_x, mean_y) = means_using(pairs, add, div)?;
    let mut sum_xy = 0.0;
    let mut sum_x2 = 0.0;
    let mut sum_y2 = 0.0;
    for (x, y) in pairs {
        let dx = sub(*x, mean_x);
        let dy = sub(*y, mean_y);
        sum_xy = add(sum_xy, mul(dx, dy));
        sum_x2 = add(sum_x2, mul(dx, dx));
        sum_y2 = add(sum_y2, mul(dy, dy));
    }
    if !(sum_xy.is_finite() && sum_x2.is_finite() && sum_y2.is_finite()) {
        return Err(WorksheetErrorCode::Num);
    }
    if sum_x2 == 0.0 || sum_y2 == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    // Excel normalises each centred sum by n before taking the two roots.
    // Using n-1 or cancelling the divisions algebraically changes the last bits
    // (dc-wli1, correlation-arithmetic-2026-11-12.md).
    // RSQ retains sample normalisation, unlike CORREL and PEARSON.
    let denom = match divisor {
        CovarianceDivisor::Population => pairs.len() as f64,
        CovarianceDivisor::Sample => (pairs.len() - 1) as f64,
    };
    let covariance = div(sum_xy, denom);
    let variance_x = div(sum_x2, denom);
    let variance_y = div(sum_y2, denom);
    Ok(div(covariance, mul(sqrt(variance_x), sqrt(variance_y))))
}

pub fn slope_from_pairs(pairs: &[(f64, f64)]) -> Result<f64, WorksheetErrorCode> {
    if pairs.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }
    use crate::ported::excel_numeric::{
        excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul, excel_x87_sub as sub,
    };
    let (mean_x, mean_y) = means_using(pairs, add, div)?;
    let mut sum_xy = 0.0;
    let mut sum_x2 = 0.0;
    for (x, y) in pairs {
        let dx = sub(*x, mean_x);
        sum_xy = add(sum_xy, mul(dx, sub(*y, mean_y)));
        sum_x2 = add(sum_x2, mul(dx, dx));
    }
    if !(sum_xy.is_finite() && sum_x2.is_finite()) {
        return Err(WorksheetErrorCode::Num);
    }
    if sum_x2 == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    Ok(div(sum_xy, sum_x2))
}

pub fn intercept_from_pairs(pairs: &[(f64, f64)]) -> Result<f64, WorksheetErrorCode> {
    let slope = slope_from_pairs(pairs)?;
    let (mean_x, mean_y) = means(pairs)?;
    Ok(mean_y - slope * mean_x)
}

pub fn rsq_from_pairs(pairs: &[(f64, f64)]) -> Result<f64, WorksheetErrorCode> {
    correlation_with_divisor(pairs, CovarianceDivisor::Sample).map(|corr| corr * corr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bits(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} vs {expected}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn correlation_seed_lane_matches_excel_probe() {
        let pairs = vec![(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0)];
        let got = correlation_from_pairs(&pairs).unwrap();
        assert!((got - 1.0).abs() < 1e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn covariance_population_seed_lane_matches_excel_probe() {
        let pairs = vec![(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0)];
        let got = covariance_from_pairs(&pairs, CovarianceDivisor::Population).unwrap();
        assert!((got - 2.5).abs() < 1e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn slope_intercept_and_rsq_seed_lanes_match_excel_probe() {
        let pairs = vec![(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0)];
        assert!((slope_from_pairs(&pairs).unwrap() - 2.0).abs() < 1e-12);
        assert!(intercept_from_pairs(&pairs).unwrap().abs() < 1e-12);
        assert!((rsq_from_pairs(&pairs).unwrap() - 1.0).abs() < 1e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn correl_and_rsq_exactness_witness_rows_match_excel_targets() {
        let positive_pairs = vec![(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0), (5.0, 10.0)];
        let negative_pairs = vec![(1.0, 10.0), (2.0, 8.0), (3.0, 6.0), (4.0, 4.0), (5.0, 2.0)];

        let correl_positive = correlation_from_pairs(&positive_pairs).expect("correl positive");
        let correl_negative = correlation_from_pairs(&negative_pairs).expect("correl negative");
        let rsq_positive = rsq_from_pairs(&positive_pairs).expect("rsq positive");

        assert_bits(correl_positive, 0.9999999999999998_f64);
        assert_bits(correl_negative, -0.9999999999999998_f64);
        assert_bits(rsq_positive, 0.9999999999999996_f64);
    }

    /// Live Excel 16.0.20625.20108 (dc-j9k.167): a sum of centred squares or products that overflows
    /// binary64 is #NUM!, not a quotient by infinity.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn overflowing_centred_sums_are_num() {
        let wide = vec![(-1e155, 1.0), (1e155, 2.0)]; // (x, y) pairs for SLOPE/CORREL
        assert_eq!(slope_from_pairs(&wide), Err(WorksheetErrorCode::Num));
        assert_eq!(intercept_from_pairs(&wide), Err(WorksheetErrorCode::Num));
        assert_eq!(correlation_from_pairs(&wide), Err(WorksheetErrorCode::Num));
        assert_eq!(rsq_from_pairs(&wide), Err(WorksheetErrorCode::Num));
        let edge = vec![(-1e153, 1.0), (1e153, 2.0)];
        assert!(slope_from_pairs(&edge).is_ok());
        // Products overflow although every square is finite.
        let product = vec![(1e155, -1e155), (-1e155, 1e155)];
        assert_eq!(covariance_from_pairs(&product, CovarianceDivisor::Population), Err(WorksheetErrorCode::Num));
        let ok = vec![(-1e155, 1.0), (1e155, 2.0)];
        assert!(covariance_from_pairs(&ok, CovarianceDivisor::Population).is_ok());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn steyx_overflowing_sums_are_num() {
        let wide = vec![(-1e155, 1.0), (1e155, 2.0), (3.0, 4.0)];
        assert_eq!(
            crate::ported::moment_stats_family::steyx_kernel(&wide),
            Err(WorksheetErrorCode::Num)
        );
    }

    /// Live Excel 16.0.20625.20108 (dc-j9k.168): SLOPE and COVARIANCE run the centred sums in
    /// double-rounded x87 steps, as CORREL does; plain binary64 arithmetic is one ulp out.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn slope_and_covariance_use_x87_steps() {
        let b = f64::from_bits;
        // (x, y) pairs
        let pairs = vec![
            (b(0xbcd2_03af_9ee7_5616), b(0xd4b2_49ad_2594_c37d)),
            (b(0x4024_0000_0000_0000), b(0xbf50_624d_b774_ca22)),
        ];
        assert_bits(slope_from_pairs(&pairs).unwrap(), b(0x547d_42ae_a287_9f2d));
        let slope = vec![
            (b(0x3f31_17a9_f5a6_8320), b(0x3ea2_7ae3_59de_b400)),
            (b(0x3f6d_06f2_92f4_3720), b(0x3f8b_ce87_7977_27ec)),
        ];
        assert_bits(slope_from_pairs(&slope).unwrap(), b(0x4010_8b61_5284_34eb));
        assert_bits(covariance_from_pairs(&slope, CovarianceDivisor::Sample).unwrap(), b(0x3ef7_5dad_27bf_c16c));
        assert_bits(covariance_from_pairs(&slope, CovarianceDivisor::Population).unwrap(), b(0x3ee7_5dad_27bf_c16c));
    }
}

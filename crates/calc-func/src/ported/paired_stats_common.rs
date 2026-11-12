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

    let (mean_x, mean_y) = means(pairs)?;
    let sum = pairs
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>();
    let denom = match divisor {
        CovarianceDivisor::Population => pairs.len() as f64,
        CovarianceDivisor::Sample => (pairs.len() - 1) as f64,
    };
    Ok(sum / denom)
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
    let (mean_x, mean_y) = means(pairs)?;
    let mut sum_xy = 0.0;
    let mut sum_x2 = 0.0;
    for (x, y) in pairs {
        let dx = *x - mean_x;
        sum_xy += dx * (*y - mean_y);
        sum_x2 += dx * dx;
    }
    if sum_x2 == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    Ok(sum_xy / sum_x2)
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
}

use crate::value::WorksheetErrorCode;
use super::excel_numeric::{excel_x87_add as add, excel_x87_sub as sub,
    excel_x87_mul as mul, excel_x87_div as div, excel_sqrt_staged};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarianceInclusionPolicy {
    AverageLike,
    AverageALike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarianceDivisor {
    Population,
    Sample,
}

pub fn variance_from_values(
    values: &[f64],
    divisor: VarianceDivisor,
) -> Result<f64, WorksheetErrorCode> {
    if values.is_empty() {
        return Err(WorksheetErrorCode::Div0);
    }
    if matches!(divisor, VarianceDivisor::Sample) && values.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }

    // dc-j9k.13: the raw-moment identities have different store boundaries
    // for sample and population. Every arithmetic assignment stores from
    // x87 PC64, including the squares and the final division. Six original
    // bank controls distinguish these steps from ordinary binary64.
    let count = values.len() as f64;
    let total = values.iter().fold(0.0, |s, &x| add(s, x));
    let squares = values.iter().fold(0.0, |q, &x| add(q, mul(x, x)));
    let square_total = mul(total, total);
    let (left, correction) = match divisor {
        VarianceDivisor::Sample => (squares, div(square_total, count)),
        VarianceDivisor::Population => (mul(count, squares), square_total),
    };
    let numerator = sub(left, correction);
    let denom = match divisor {
        VarianceDivisor::Population => count,
        VarianceDivisor::Sample => (values.len() - 1) as f64,
    };
    let raw_divisor = match divisor {
        VarianceDivisor::Population => mul(count, denom),
        VarianceDivisor::Sample => denom,
    };
    let variance = div(numerator, raw_divisor);
    let mean = div(total, count);
    // R25 scalar-cell bisections distinguish these stored comparisons from
    // numerator<0.01*squares, a resident product, and strict variance*1000<mean.
    // The original negative-mean controls also require the absolute mean.
    // Range/array transfer at the new edges remains a separate Excel question.
    // Nonfinite raw moments take the centered pass to retain finite centered
    // results when an intermediate square or count*squares overflows.
    let cancellation = div(div(square_total, count), squares) > 0.99;
    let small_variance = mul(variance, 1000.0) <= mean.abs();
    if !variance.is_finite() || cancellation || small_variance {
        let centered = values.iter().fold(0.0, |q, &x| {
            let delta = sub(x, mean);
            add(q, mul(delta, delta))
        });
        Ok(div(centered, denom))
    } else {
        Ok(variance)
    }
}

pub fn stdev_from_values(
    values: &[f64],
    divisor: VarianceDivisor,
) -> Result<f64, WorksheetErrorCode> {
    variance_from_values(values, divisor).map(excel_sqrt_staged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn variance_population_and_sample_split_basic_lane() {
        let values = vec![2.0, 3.0, 4.0];
        let pop = variance_from_values(&values, VarianceDivisor::Population).unwrap();
        let sample = variance_from_values(&values, VarianceDivisor::Sample).unwrap();
        assert!((pop - (2.0 / 3.0)).abs() < 1e-12);
        assert!((sample - 1.0).abs() < 1e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sample_variance_requires_two_values() {
        let err = variance_from_values(&[1.0], VarianceDivisor::Sample).unwrap_err();
        assert_eq!(err, WorksheetErrorCode::Div0);
    }
}

use crate::value::WorksheetErrorCode;

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

    // Excel's variance (dc-j9k), with s the forward sum and q the forward sum of squares: the
    // one-pass form, (q - s*s/n)/(n - 1) for a sample and (n*q - s*s)/(n*n) for a population,
    // unless its numerator is below 1% of q (n*q for a population), where it cancels; then the
    // two-pass form, the forward sum of (x - mean)^2 with mean = s/n, over n - 1 or n. Measured
    // on VAR.S and VAR.P readings of the catalogue oracle's sheet dcj9k_statistics: every
    // reading whose forms differ follows the one-pass form at a numerator of 1.0006e-2 of q
    // and above and the two-pass form at 9.963e-3 and below.
    let n = values.len() as f64;
    let (sum, squares) = values
        .iter()
        .fold((0.0, 0.0), |(s, q), value| (s + value, q + value * value));
    let two_pass = || {
        let mean = sum / n;
        values.iter().fold(0.0, |total, value| {
            let delta = value - mean;
            total + delta * delta
        })
    };
    match divisor {
        VarianceDivisor::Sample => {
            let numerator = squares - sum * sum / n;
            if numerator < squares * 0.01 {
                Ok(two_pass() / (n - 1.0))
            } else {
                Ok(numerator / (n - 1.0))
            }
        }
        VarianceDivisor::Population => {
            let numerator = n * squares - sum * sum;
            if numerator < n * squares * 0.01 {
                Ok(two_pass() / n)
            } else {
                Ok(numerator / (n * n))
            }
        }
    }
}

pub fn stdev_from_values(
    values: &[f64],
    divisor: VarianceDivisor,
) -> Result<f64, WorksheetErrorCode> {
    variance_from_values(values, divisor).map(f64::sqrt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variance_population_and_sample_split_basic_lane() {
        let values = vec![2.0, 3.0, 4.0];
        let pop = variance_from_values(&values, VarianceDivisor::Population).unwrap();
        let sample = variance_from_values(&values, VarianceDivisor::Sample).unwrap();
        assert!((pop - (2.0 / 3.0)).abs() < 1e-12);
        assert!((sample - 1.0).abs() < 1e-12);
    }

    #[test]
    fn sample_variance_requires_two_values() {
        let err = variance_from_values(&[1.0], VarianceDivisor::Sample).unwrap_err();
        assert_eq!(err, WorksheetErrorCode::Div0);
    }
}

use crate::value::WorksheetErrorCode;
use std::collections::BTreeMap;

const PROB_SUM_TOLERANCE: f64 = 1e-7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VectorOrientation {
    Row,
    Column,
}

fn sorted_non_decreasing(values: &[f64]) -> bool {
    values.windows(2).all(|w| w[0] <= w[1])
}

pub(crate) fn prob_kernel(
    x_values: &[f64],
    probabilities: &[f64],
    lower: f64,
    upper: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    if x_values.len() != probabilities.len() || x_values.is_empty() {
        return Err(WorksheetErrorCode::NA);
    }
    if probabilities
        .iter()
        .any(|p| !p.is_finite() || *p < 0.0 || *p > 1.0)
    {
        return Err(WorksheetErrorCode::Num);
    }
    let sum = probabilities.iter().sum::<f64>();
    if (sum - 1.0).abs() > PROB_SUM_TOLERANCE {
        return Err(WorksheetErrorCode::Num);
    }
    let upper = upper.unwrap_or(lower);
    if upper < lower || !lower.is_finite() || !upper.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    let mut total = 0.0;
    for (x, p) in x_values.iter().zip(probabilities.iter()) {
        if *x >= lower && *x <= upper {
            total += *p;
        }
    }
    Ok(total)
}

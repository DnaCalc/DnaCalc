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
    // Excel 20527's singleton sum check accepts the four predecessors of 1 through its 22nd
    // successor. A reversed interval has no members; multi-item sums keep their existing route.
    if let ([x], [p]) = (x_values, probabilities)
    {
        let upper = upper.unwrap_or(lower);
        if x.is_finite() && p.is_finite() && lower.is_finite() && upper.is_finite() {
            if (1.0_f64.to_bits() - 4..=1.0_f64.to_bits() + 22).contains(&p.to_bits()) {
                return Ok(if *x >= lower && *x <= upper { *p } else { 0.0 });
            }
            return Err(WorksheetErrorCode::Num);
        }
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

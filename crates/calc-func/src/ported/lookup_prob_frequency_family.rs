use crate::value::WorksheetErrorCode;
use std::collections::BTreeMap;

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
    // Excel 20527 validates the stored sum in input order. It accepts the four predecessors
    // of 1 through its 22nd successor, including sums of compensating out-of-range probabilities.
    let sum = probabilities.iter().sum::<f64>();
    let one = 1.0_f64.to_bits();
    if !(one - 4..=one + 22).contains(&sum.to_bits()) {
        return Err(WorksheetErrorCode::Num);
    }
    let upper = upper.unwrap_or(lower);
    if !lower.is_finite() || !upper.is_finite() {
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

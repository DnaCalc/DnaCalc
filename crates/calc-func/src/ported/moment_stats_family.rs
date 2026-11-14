use crate::ported::excel_numeric::{
    excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul, excel_x87_sub as sub,
};
use crate::value::WorksheetErrorCode;

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// The stored mean and the stored two-pass sum of squared deviations.
fn stored_moments(values: &[f64]) -> (f64, f64) {
    let avg = div(
        values.iter().fold(0.0, |sum, &value| add(sum, value)),
        values.len() as f64,
    );
    let sumsq = values.iter().fold(0.0, |sum, &value| {
        let delta = sub(value, avg);
        add(sum, mul(delta, delta))
    });
    (avg, sumsq)
}

pub(crate) fn kurt_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() < 4 {
        return Err(WorksheetErrorCode::Div0);
    }
    let (avg, sumsq) = stored_moments(values);
    if sumsq == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    let n = values.len() as f64;
    // Every operation is stored from the x87: the two-pass sum of squares, its square root, each
    // z, the square of z squared, the running sum, and the coefficient before the product. All
    // 4,508 bank rows; the division after the product holds 3,212.
    let deviation = crate::ported::excel_numeric::excel_sqrt_staged(div(sumsq, n - 1.0));
    let fourths = values.iter().fold(0.0, |sum, &value| {
        let z = div(sub(value, avg), deviation);
        let square = mul(z, z);
        add(sum, mul(square, square))
    });
    let coefficient = div(mul(n, n + 1.0), mul(mul(n - 1.0, n - 2.0), n - 3.0));
    let offset = div(mul(3.0, mul(n - 1.0, n - 1.0)), mul(n - 2.0, n - 3.0));
    Ok(sub(mul(coefficient, fourths), offset))
}

pub(crate) fn skew_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() < 3 {
        return Err(WorksheetErrorCode::Div0);
    }
    let (avg, sumsq) = stored_moments(values);
    if sumsq == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    let n = values.len() as f64;
    let deviation = crate::ported::excel_numeric::excel_sqrt_staged(div(sumsq, n - 1.0));
    let cubes = values.iter().fold(0.0, |sum, &value| {
        let z = div(sub(value, avg), deviation);
        add(sum, mul(mul(z, z), z))
    });
    // F1 (dc-8dq): the factor n/((n-1)(n-2)) first, then times the sum, as Excel's SKEW gives
    // SKEW(1,2,3,5) (the catalogue oracle's capture); n*sum/((n-1)(n-2)) is one ulp above it.
    Ok(mul(div(n, mul(n - 1.0, n - 2.0)), cubes))
}

pub(crate) fn skew_p_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() < 3 {
        return Err(WorksheetErrorCode::Div0);
    }
    let n = values.len();
    let avg = mean(values);
    let mut sumsq = 0.0;
    let mut deltas = Vec::with_capacity(n);
    for value in values {
        let delta = *value - avg;
        sumsq += delta * delta;
        deltas.push(delta);
    }
    if sumsq == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }

    let pop_std = (sumsq / n as f64).sqrt();
    let standardized_sum = deltas
        .iter()
        .map(|delta| {
            let z = *delta / pop_std;
            z * z * z
        })
        .sum::<f64>();
    Ok(standardized_sum / n as f64)
}

pub(crate) fn steyx_kernel(pairs: &[(f64, f64)]) -> Result<f64, WorksheetErrorCode> {
    if pairs.len() < 3 {
        return Err(WorksheetErrorCode::Div0);
    }
    let n = pairs.len() as f64;
    let mean_x = pairs.iter().map(|(x, _)| *x).sum::<f64>() / n;
    let mean_y = pairs.iter().map(|(_, y)| *y).sum::<f64>() / n;
    let mut sum_x2 = 0.0;
    let mut sum_y2 = 0.0;
    let mut sum_xy = 0.0;
    for (x, y) in pairs {
        let dx = *x - mean_x;
        let dy = *y - mean_y;
        sum_x2 += dx * dx;
        sum_y2 += dy * dy;
        sum_xy += dx * dy;
    }
    if sum_x2 == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    let sse = sum_y2 - (sum_xy * sum_xy) / sum_x2;
    let residual = if sse < 0.0 && sse.abs() < 1.0e-12 {
        0.0
    } else {
        sse
    };
    Ok((residual / (n - 2.0)).sqrt())
}

pub(crate) fn trimmean_kernel(values: &mut [f64], percent: f64) -> Result<f64, WorksheetErrorCode> {
    if !percent.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    // A percent of 1 is #NUM! too, though Microsoft's page names only one above 1 (Excel
    // 16.0.20430, 2026-10-25, dc-6au: `=TRIMMEAN(A1:A3,1)` #NUM!, `=TRIMMEAN(A1:A3,0.9999999999)` 2).
    if !(0.0..1.0).contains(&percent) {
        return Err(WorksheetErrorCode::Num);
    }
    if values.is_empty() {
        return Err(WorksheetErrorCode::Num);
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let to_trim = (percent * values.len() as f64).floor() as usize;
    let each_side = to_trim / 2;
    if each_side * 2 >= values.len() {
        return Err(WorksheetErrorCode::Num);
    }
    // Excel adds the kept values in their own order, not the sorted order (dc-j9k: 20 of 20
    // TRIMMEAN readings in the catalogue oracle's sheet dcj9k_statistics; the sorted sum
    // matched 18): each of the `each_side` smallest and largest values is left out once.
    let mut trimmed: Vec<f64> = sorted[..each_side]
        .iter()
        .chain(sorted[sorted.len() - each_side..].iter())
        .copied()
        .collect();
    let mut total = 0.0;
    for value in values.iter() {
        if let Some(at) = trimmed.iter().position(|t| t == value) {
            trimmed.swap_remove(at);
        } else {
            total += value;
        }
    }
    Ok(total / (values.len() - 2 * each_side) as f64)
}

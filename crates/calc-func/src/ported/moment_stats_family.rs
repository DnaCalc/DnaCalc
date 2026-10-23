use crate::value::WorksheetErrorCode;

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

pub(crate) fn kurt_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() < 4 {
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

    let sample_std = (sumsq / (n as f64 - 1.0)).sqrt();
    let standardized_sum = deltas
        .iter()
        .map(|delta| {
            let z = *delta / sample_std;
            let z2 = z * z;
            z2 * z2
        })
        .sum::<f64>();
    let n = n as f64;
    Ok(
        (n * (n + 1.0) * standardized_sum) / ((n - 1.0) * (n - 2.0) * (n - 3.0))
            - (3.0 * (n - 1.0).powi(2)) / ((n - 2.0) * (n - 3.0)),
    )
}

pub(crate) fn skew_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
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

    let sample_std = (sumsq / (n as f64 - 1.0)).sqrt();
    let standardized_sum = deltas
        .iter()
        .map(|delta| {
            let z = *delta / sample_std;
            z * z * z
        })
        .sum::<f64>();
    let n = n as f64;
    // F1 (dc-8dq): the factor n/((n-1)(n-2)) first, then times the sum, as Excel's SKEW gives
    // SKEW(1,2,3,5) (the catalogue oracle's capture); n*sum/((n-1)(n-2)) is one ulp above it.
    Ok(n / ((n - 1.0) * (n - 2.0)) * standardized_sum)
}

pub(crate) fn skew_p_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.is_empty() {
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
    if !(0.0..=1.0).contains(&percent) {
        return Err(WorksheetErrorCode::Num);
    }
    if values.is_empty() {
        return Err(WorksheetErrorCode::Num);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let to_trim = (percent * values.len() as f64).floor() as usize;
    let each_side = to_trim / 2;
    if each_side * 2 >= values.len() {
        return Err(WorksheetErrorCode::Num);
    }
    let kept = &values[each_side..values.len() - each_side];
    Ok(kept.iter().sum::<f64>() / kept.len() as f64)
}

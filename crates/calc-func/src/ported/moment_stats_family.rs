use crate::ported::excel_numeric::{
    excel_x87_add as add, excel_x87_div as div, excel_x87_mul as mul, excel_x87_sub as sub,
};
use crate::value::WorksheetErrorCode;

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// The stored mean, the stored two-pass sum of squared deviations, and whether any
/// arithmetic step overflowed.
///
/// Live Excel 16.0 b20625 (dc-j9k.576..580, 30/30 across SKEW, SKEW.P, KURT):
/// when an intermediate addition, subtraction, or squaring overflows during Pass 1
/// or Pass 2, Excel substitutes 0.0 and latches overflow, continuing the loop.
/// After dividing by the variance divisor, a zero or subnormal variance returns
/// `#DIV/0!` ahead of the latched overflow; otherwise the latched overflow returns
/// `#NUM!`.
fn stored_moments(values: &[f64]) -> (f64, f64, bool) {
    let mut overflowed = false;
    let mut latch = |v: f64| -> f64 {
        if v.is_finite() {
            v
        } else {
            overflowed = true;
            0.0
        }
    };
    let sum = values.iter().fold(0.0, |sum, &value| latch(add(sum, value)));
    let avg = div(sum, values.len() as f64);
    let sumsq = values.iter().fold(0.0, |sum, &value| {
        let delta = latch(sub(value, avg));
        let sq = latch(mul(delta, delta));
        latch(add(sum, sq))
    });
    (avg, sumsq, overflowed)
}

fn moment_variance(sumsq: f64, divisor: f64, overflowed: bool) -> Result<f64, WorksheetErrorCode> {
    let variance = div(sumsq, divisor);
    if !variance.is_normal() {
        Err(WorksheetErrorCode::Div0)
    } else if overflowed {
        Err(WorksheetErrorCode::Num)
    } else {
        Ok(variance)
    }
}

pub(crate) fn kurt_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() < 4 {
        return Err(WorksheetErrorCode::Div0);
    }
    let (avg, sumsq, overflowed) = stored_moments(values);
    let n = values.len() as f64;
    // Every operation is stored from the x87: the two-pass sum of squares, its square root, each
    // z, the square of z squared, the running sum, and the coefficient before the product. All
    // 4,508 bank rows; the division after the product holds 3,212.
    let deviation =
        crate::ported::excel_numeric::excel_sqrt_staged(moment_variance(sumsq, n - 1.0, overflowed)?);
    let fourths = values.iter().fold(0.0, |sum, &value| {
        let z = div(sub(value, avg), deviation);
        let square = mul(z, z);
        add(sum, mul(square, square))
    });
    let coefficient = div(mul(n, n + 1.0), mul(mul(n - 1.0, n - 2.0), n - 3.0));
    let offset = div(mul(3.0, mul(n - 1.0, n - 1.0)), mul(n - 2.0, n - 3.0));
    let result = sub(mul(coefficient, fourths), offset);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

pub(crate) fn skew_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() < 3 {
        return Err(WorksheetErrorCode::Div0);
    }
    let (avg, sumsq, overflowed) = stored_moments(values);
    let n = values.len() as f64;
    let deviation =
        crate::ported::excel_numeric::excel_sqrt_staged(moment_variance(sumsq, n - 1.0, overflowed)?);
    let cubes = values.iter().fold(0.0, |sum, &value| {
        let z = div(sub(value, avg), deviation);
        add(sum, mul(mul(z, z), z))
    });
    // F1 (dc-8dq): the factor n/((n-1)(n-2)) first, then times the sum, as Excel's SKEW gives
    // SKEW(1,2,3,5) (the catalogue oracle's capture); n*sum/((n-1)(n-2)) is one ulp above it.
    let result = mul(div(n, mul(n - 1.0, n - 2.0)), cubes);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(WorksheetErrorCode::Num)
    }
}

pub(crate) fn skew_p_kernel(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() < 3 {
        return Err(WorksheetErrorCode::Div0);
    }
    // The same stored x87 stages as SKEW, with the population divisor n (dc-j9k.520).
    let (avg, sumsq, overflowed) = stored_moments(values);
    let n = values.len() as f64;
    let deviation =
        crate::ported::excel_numeric::excel_sqrt_staged(moment_variance(sumsq, n, overflowed)?);
    let cubes = values.iter().fold(0.0, |sum, &value| {
        let z = div(sub(value, avg), deviation);
        add(sum, mul(mul(z, z), z))
    });
    let result = div(cubes, n);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(WorksheetErrorCode::Num)
    }
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
    if !(sum_x2.is_finite() && sum_y2.is_finite() && sum_xy.is_finite()) {
        return Err(WorksheetErrorCode::Num);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_j9k_576_580_moment_overflow_reports_num_when_remaining_variance_is_normal() {
        // Live Excel 16.0 b20625 (Touchstone ng6, 30/30 across SKEW, SKEW.P, KURT):
        // Pass 1/Pass 2 overflows substitute 0.0 and latch overflow; if the final
        // variance is subnormal/zero Excel returns #DIV/0!, otherwise #NUM!.
        for (vals, expected) in [
            (vec![1e308, 1e308, 1.0, 1.0], Err(WorksheetErrorCode::Num)),
            (vec![1e308, 1e308, 0.0, 0.0], Err(WorksheetErrorCode::Div0)),
            (vec![1e308, 1e308, 1e-154, -1e-154], Err(WorksheetErrorCode::Div0)),
            (vec![1e308, 1e308, 2.2e-154, -2.2e-154], Err(WorksheetErrorCode::Num)),
            (vec![1e154, -1e154, 0.0, 3e-154], Err(WorksheetErrorCode::Div0)),
            (vec![1e154, -1e154, 0.0, 1e-153], Err(WorksheetErrorCode::Num)),
            (vec![1e154, -1e154, 1e153, 1e153], Err(WorksheetErrorCode::Num)),
            (vec![0.0, 1.0, 1e154, -1e154], Err(WorksheetErrorCode::Div0)),
            (vec![1e154, -1e154, 0.0, 1.0], Err(WorksheetErrorCode::Num)),
            (vec![-1e308, 1e308, 1e308, 1e308], Err(WorksheetErrorCode::Div0)),
        ] {
            assert_eq!(skew_kernel(&vals), expected, "SKEW({vals:?})");
            assert_eq!(skew_p_kernel(&vals), expected, "SKEW.P({vals:?})");
            assert_eq!(kurt_kernel(&vals), expected, "KURT({vals:?})");
        }
        // Exact divisor boundary: 8e-308 / 3 is normal (#NUM! for SKEW/KURT) while
        // 8e-308 / 4 is subnormal (#DIV/0! for SKEW.P) in live Excel 16.0 b20625.
        let split = [1e308, 1e308, 2e-154, -2e-154];
        assert_eq!(skew_kernel(&split), Err(WorksheetErrorCode::Num));
        assert_eq!(kurt_kernel(&split), Err(WorksheetErrorCode::Num));
        assert_eq!(skew_p_kernel(&split), Err(WorksheetErrorCode::Div0));
        // Representative from dc-j9k.577 (arity=3):
        let rep_577 = [
            f64::from_bits(0xffefffffffffffff),
            f64::from_bits(0x3f50624dd2f1a9fc),
            f64::from_bits(0x2b2bff2ee48e0530),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x54b249ad2594c37d),
            f64::from_bits(0x7e37e43c88007689),
            f64::from_bits(0xbff0000000000000),
            f64::from_bits(0x3f1a36e2eb1c432d),
            f64::from_bits(0xffefffffffffffff),
            f64::from_bits(0x3f50624dd2f1a9fc),
        ];
        assert_eq!(skew_kernel(&rep_577), Err(WorksheetErrorCode::Num));
    }
}

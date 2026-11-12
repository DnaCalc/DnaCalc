use crate::ported::chi_f_t_family::chisq_dist_rt_kernel;
use crate::ported::special_math_common::bratio;
use crate::value::WorksheetErrorCode;

fn sample_mean(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.is_empty() {
        return Err(WorksheetErrorCode::Div0);
    }
    Ok(values.iter().sum::<f64>() / values.len() as f64)
}

fn sample_variance(values: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if values.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }
    let mean = sample_mean(values)?;
    Ok(values
        .iter()
        .map(|v| {
            let d = *v - mean;
            d * d
        })
        .sum::<f64>()
        / (values.len() - 1) as f64)
}

fn validate_positive_df(df: f64) -> Result<f64, WorksheetErrorCode> {
    if !df.is_finite() || df <= 0.0 {
        Err(WorksheetErrorCode::Num)
    } else {
        Ok(df)
    }
}

fn t_probability(abs_t: f64, df: f64, tails: i32) -> Result<f64, WorksheetErrorCode> {
    if !abs_t.is_finite() || abs_t < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let v = validate_positive_df(df)?;
    // W109 identified staging: x = df/den, y = t^2/den (accurate complement).
    let t2 = abs_t * abs_t;
    let den = v + t2;
    let two_tail = bratio(df / 2.0, 0.5, v / den, t2 / den).0;
    match tails {
        1 => Ok(two_tail / 2.0),
        2 => Ok(two_tail),
        _ => Err(WorksheetErrorCode::Num),
    }
}

pub(crate) fn f_test_kernel(array1: &[f64], array2: &[f64]) -> Result<f64, WorksheetErrorCode> {
    if array1.len() < 2 || array2.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }
    let var1 = sample_variance(array1)?;
    let var2 = sample_variance(array2)?;
    if var1 == 0.0 || var2 == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    let (ratio, df1, df2) = if var1 >= var2 {
        (
            var1 / var2,
            (array1.len() - 1) as f64,
            (array2.len() - 1) as f64,
        )
    } else {
        (
            var2 / var1,
            (array2.len() - 1) as f64,
            (array1.len() - 1) as f64,
        )
    };
    if !ratio.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    // Two-tailed: 2 * min(P(F > f), P(F <= f)), not min(2 * P(F > f), 1). With unequal degrees
    // of freedom the right tail of the larger-first ratio can pass 1/2 (even at f = 1), where
    // the old clamp returned 1 (13 gross F.TEST bank rows). Both tails come from the one
    // bratio call of F.DIST.RT's staging; taking its own complement output, not 1 - p, holds
    // two more bank rows (dc-ymjf.6). The argument pair is this one, `d2 / (d2 + n)` and
    // `n / (d2 + n)`, not FDIST's (`f_tails`; dc-6332.8: 62 of the 78 bank rows against 48, and
    // FDIST's form on the two variances or on the sums of squares holds 39 to 45).
    let num = df1 * ratio;
    let den = df2 + num;
    let (p, q) = bratio(df2 / 2.0, df1 / 2.0, df2 / den, num / den);
    Ok(2.0 * p.min(q))
}

pub(crate) fn t_test_paired_kernel(
    pairs: &[(f64, f64)],
    tails: i32,
) -> Result<f64, WorksheetErrorCode> {
    if pairs.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }
    let diffs: Vec<f64> = pairs.iter().map(|(x, y)| x - y).collect();
    let mean = sample_mean(&diffs)?;
    let se = (sample_variance(&diffs)? / diffs.len() as f64).sqrt();
    if se == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    t_probability((mean / se).abs(), (diffs.len() - 1) as f64, tails)
}

pub(crate) fn t_test_equal_variance_kernel(
    array1: &[f64],
    array2: &[f64],
    tails: i32,
) -> Result<f64, WorksheetErrorCode> {
    if array1.len() < 2 || array2.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }
    let mean1 = sample_mean(array1)?;
    let mean2 = sample_mean(array2)?;
    let var1 = sample_variance(array1)?;
    let var2 = sample_variance(array2)?;
    let df = (array1.len() + array2.len() - 2) as f64;
    let pooled = (((array1.len() - 1) as f64) * var1 + ((array2.len() - 1) as f64) * var2) / df;
    let se = (pooled * (1.0 / array1.len() as f64 + 1.0 / array2.len() as f64)).sqrt();
    if se == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    t_probability(((mean1 - mean2) / se).abs(), df, tails)
}

pub(crate) fn t_test_unequal_variance_kernel(
    array1: &[f64],
    array2: &[f64],
    tails: i32,
) -> Result<f64, WorksheetErrorCode> {
    if array1.len() < 2 || array2.len() < 2 {
        return Err(WorksheetErrorCode::Div0);
    }
    let mean1 = sample_mean(array1)?;
    let mean2 = sample_mean(array2)?;
    let var1 = sample_variance(array1)?;
    let var2 = sample_variance(array2)?;
    let term1 = var1 / array1.len() as f64;
    let term2 = var2 / array2.len() as f64;
    let se = (term1 + term2).sqrt();
    if se == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    let denom =
        term1 * term1 / (array1.len() - 1) as f64 + term2 * term2 / (array2.len() - 1) as f64;
    if denom == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    let df = (term1 + term2) * (term1 + term2) / denom;
    t_probability(((mean1 - mean2) / se).abs(), df, tails)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Excel's F.TEST answers (build 20228, `work/w109/G3-06-ftest-variance/
    /// answers-ftest-wrapper-heldout-v1.json`) for equal variances with unequal sizes, where the
    /// right tail of the larger-first ratio exceeds 1/2; the old `min(2 * p, 1)` returned 1.0.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn f_test_two_tailed_takes_the_smaller_tail() {
        let rows: [(&[f64], &[f64], u64); 5] = [
            (&[-1.0, -1.0, 0.0, 1.0, 1.0], &[-1.0, 0.0, 1.0], 0x3fec71c71c71c71c),
            (&[-1.0, -1.0, -1.0, 0.0, 1.0, 1.0, 1.0], &[-1.0, 0.0, 1.0], 0x3feb000000000001),
            (
                &[-1.0, -1.0, -1.0, 0.0, 1.0, 1.0, 1.0],
                &[-1.0, -1.0, 0.0, 1.0, 1.0],
                0x3fee69ad42c3c9ee,
            ),
            (
                &[-1.0, -1.0, -1.0, -1.0, 0.0, 1.0, 1.0, 1.0, 1.0],
                &[-1.0, 0.0, 1.0],
                0x3fea36e2eb1c432e,
            ),
            (
                &[-1.0, -1.0, -1.0, -1.0, -1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0],
                &[-1.0, 0.0, 1.0],
                0x3fe9b85cb5339f15,
            ),
        ];
        for (a, b, bits) in rows {
            assert_eq!(f_test_kernel(a, b).unwrap().to_bits(), bits, "{a:?} {b:?}");
        }
        // ft-0 and oc3 (same source family), right tail below 1/2: unchanged.
        let ft0 = f_test_kernel(&[6.0, 7.0, 9.0, 15.0, 21.0], &[20.0, 28.0, 31.0, 38.0, 40.0]);
        assert_eq!(ft0.unwrap().to_bits(), 0x3fe4bf0511ab8322);
        let oc3 = f_test_kernel(&[100.0, 101.0, 102.0], &[50.0, 60.0, 70.0, 80.0]);
        assert_eq!(oc3.unwrap().to_bits(), 0x3f88742515bcb009);
    }
}

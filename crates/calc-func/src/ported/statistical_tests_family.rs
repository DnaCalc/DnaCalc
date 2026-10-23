use crate::value::WorksheetErrorCode;
use crate::ported::chi_f_t_family::{chisq_dist_rt_kernel, f_dist_rt_kernel};
use crate::ported::special_math_common::bratio;

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
    Ok((2.0 * f_dist_rt_kernel(ratio, df1, df2)?).min(1.0))
}

pub(crate) fn t_test_paired_kernel(pairs: &[(f64, f64)], tails: i32) -> Result<f64, WorksheetErrorCode> {
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

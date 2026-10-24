use crate::value::WorksheetErrorCode;
pub fn percentile_inc_kernel(values: &mut [f64], k: f64) -> Result<f64, WorksheetErrorCode> {
    if values.is_empty() {
        return Err(WorksheetErrorCode::Num);
    }
    if !(0.0..=1.0).contains(&k) {
        return Err(WorksheetErrorCode::Num);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if values.len() == 1 {
        return Ok(values[0]);
    }
    let rank = 1.0 + k * (values.len() as f64 - 1.0);
    interpolate_sorted(values, rank)
}

pub fn percentile_exc_kernel(values: &mut [f64], k: f64) -> Result<f64, WorksheetErrorCode> {
    if values.is_empty() {
        return Err(WorksheetErrorCode::Num);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = k * (values.len() as f64 + 1.0);
    if rank < 1.0 || rank > values.len() as f64 {
        return Err(WorksheetErrorCode::Num);
    }
    interpolate_sorted(values, rank)
}

fn interpolate_sorted(values: &[f64], rank: f64) -> Result<f64, WorksheetErrorCode> {
    let lower_index = rank.floor() as usize;
    let upper_index = rank.ceil() as usize;
    if lower_index == 0
        || upper_index == 0
        || lower_index > values.len()
        || upper_index > values.len()
    {
        return Err(WorksheetErrorCode::Num);
    }
    if lower_index == upper_index {
        return Ok(values[lower_index - 1]);
    }
    let lower = values[lower_index - 1];
    let upper = values[upper_index - 1];
    Ok(lower + (rank - lower_index as f64) * (upper - lower))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn percentile_inc_interpolates_seed_lane() {
        let mut values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile_inc_kernel(&mut values, 0.3), Ok(2.2));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn percentile_exc_interpolates_seed_lane() {
        let mut values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let got = percentile_exc_kernel(&mut values, 0.3).unwrap();
        assert!((got - 1.8).abs() < 1e-12);
    }
}

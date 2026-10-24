use crate::value::WorksheetErrorCode;

pub fn standardize_kernel(x: f64, mean: f64, stdev: f64) -> Result<f64, WorksheetErrorCode> {
    if stdev <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let difference = x - mean;
    let result = difference / stdev;
    if !difference.is_finite() || !result.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(if result.abs() < f64::MIN_POSITIVE { 0.0 } else { result })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn standardize_kernel_seed_lane_matches_excel_probe() {
        assert_eq!(standardize_kernel(42.0, 40.0, 1.5), Ok(4.0 / 3.0));
        assert_eq!(
            standardize_kernel(42.0, 40.0, 0.0),
            Err(WorksheetErrorCode::Num)
        );
    }
}

use crate::ported::gcd_lcm_common::lcm_int;
use crate::value::WorksheetErrorCode;

pub fn lcm_kernel(items: &[i64]) -> Result<f64, WorksheetErrorCode> {
    if items.contains(&0) {
        return Ok(0.0);
    }
    // Excel reduces the flattened values in reverse worksheet/argument order.
    // Rounded intermediates at 2^53 make this order observable.
    items
        .iter()
        .rev()
        .copied()
        .try_fold(1, lcm_int)
        .map(|n| n as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn lcm_kernel_matches_excel_seed_rows() {
        assert_eq!(lcm_kernel(&[6, 8]), Ok(24.0));
        assert_eq!(lcm_kernel(&[0, 5]), Ok(0.0));
        assert_eq!(lcm_kernel(&[0, 0]), Ok(0.0));
        assert_eq!(
            lcm_kernel(&[3, 3, 3_002_399_751_580_331]),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            lcm_kernel(&[3_002_399_751_580_331, 3, 3]),
            Ok(9_007_199_254_740_992.0)
        );
        assert_eq!(
            lcm_kernel(&[0, 9_007_199_254_740_991, 9_007_199_254_740_990]),
            Ok(0.0)
        );
    }
}

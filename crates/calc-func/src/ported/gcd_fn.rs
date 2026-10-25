use crate::ported::gcd_lcm_common::gcd_int;
use crate::value::WorksheetErrorCode;

pub fn gcd_kernel(items: &[i64]) -> f64 {
    items.iter().copied().fold(0, gcd_int) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gcd_kernel_matches_excel_seed_rows() {
        assert_eq!(gcd_kernel(&[24, 36]), 12.0);
        assert_eq!(gcd_kernel(&[0, 5]), 5.0);
        assert_eq!(gcd_kernel(&[0, 0]), 0.0);
    }
}

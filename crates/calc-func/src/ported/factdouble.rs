use crate::ported::real_policy::ExcelRealPolicy;
use crate::ported::factorial_common::{double_factorial_of_int, trunc_nonnegative_or_minus_one};
use crate::value::WorksheetErrorCode;

pub fn factdouble_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    let truncated = trunc_nonnegative_or_minus_one(n)?;
    ExcelRealPolicy::FINITE
        .publish(n, double_factorial_of_int(truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bits(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} vs {expected}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn factdouble_kernel_matches_excel_negative_boundary() {
        assert_eq!(factdouble_kernel(6.9), Ok(48.0));
        assert_eq!(factdouble_kernel(-1.0), Ok(1.0));
        assert_eq!(factdouble_kernel(-1.1), Err(WorksheetErrorCode::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn factdouble_exact_publication_controls_remain_exact() {
        assert_bits(factdouble_kernel(9.0).expect("factdouble(9)"), 945.0_f64);
        assert_bits(factdouble_kernel(6.0).expect("factdouble(6)"), 48.0_f64);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn factdouble_reverse_product_matches_live_excel_large_n() {
        assert_eq!(
            factdouble_kernel(50.0).unwrap().to_bits(),
            0x46b9a940c33f6120
        );
        assert_eq!(
            factdouble_kernel(200.0).unwrap().to_bits(),
            0x66fb30964ec395de
        );
    }
}

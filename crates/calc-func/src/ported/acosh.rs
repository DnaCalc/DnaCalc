use crate::value::WorksheetErrorCode;

pub fn acosh_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    if n < 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Inverse-problem identity, live Excel 16.0 b20228 Range.Value2:
    // ACOSH(x) == LN(x+SQRT(x*x-1)) on 29/29 including 1, 1+1ulp, and
    // large finite x. SQRT((x-1)*(x+1)) is not the graph (18/24).
    // When x*x overflows, Excel publishes #NUM! (ACOSH(1e200)), the same
    // intermediate overflow ASINH uses.
    if !(n * n).is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(crate::ported::excel_numeric::excel_log(
        n + (n * n - 1.0).sqrt(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acosh_kernel_rejects_values_below_one() {
        assert_eq!(acosh_kernel(0.5), Err(WorksheetErrorCode::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acosh_just_above_one_is_nonzero_matching_excel() {
        // BUG-FUNC-027 C5: the "Excel collapses to 0" near-1 witness was a
        // formula-literal artifact (the parser rounded 1+1e-15 down to 1.0).
        // Range.Value2 on Excel 16.0 b20228 publishes the LN identity at
        // 1+1ulp (0x3e56a09e67ffffff) and at 1+1e-15 (0x3e694c5839fffffe).
        let one_ulp = f64::from_bits(0x3ff0000000000001);
        assert_eq!(acosh_kernel(one_ulp).unwrap().to_bits(), 0x3e56a09e67ffffff);
        let y = acosh_kernel(1.0 + 1e-15).expect("non-zero just above 1");
        assert_eq!(y.to_bits(), 0x3e694c5839fffffe);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn acosh_follows_worksheet_ln_identity() {
        for x in [1.0, 1.0 + f64::EPSILON, 1.1, 2.0, 10.0, 1e6, 1e150] {
            let got = acosh_kernel(x).unwrap();
            let expect = crate::ported::excel_numeric::excel_log(x + (x * x - 1.0).sqrt());
            assert_eq!(got.to_bits(), expect.to_bits(), "x={x}");
        }
        assert_eq!(acosh_kernel(1e200), Err(WorksheetErrorCode::Num));
    }
}

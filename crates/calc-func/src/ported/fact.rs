use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

/// Live Excel 16.0 b20625 adds this exact constant (2^-22 + 2^-33) before truncating
/// non-negative FACT arguments, verified across all 170 integer boundaries n=2..=171.
const EXCEL_FACT_NEAR_INT_EPS: f64 = f64::from_bits(0x3e90020000000000);

fn fact_reverse_product(n: i64) -> f64 {
    // Live Excel 16.0 b20326: FACT(n) is the reverse native product
    // n*(n-1)*...*2 (empty product 1 for n<=1). Forward product diverges
    // (FACT(170) forward misses). FACT(171) still overflows to #NUM!.
    if n <= 1 {
        return 1.0;
    }
    let mut acc = 1.0;
    for k in (2..=n).rev() {
        acc *= k as f64;
    }
    acc
}

pub fn fact_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    // Excel checks the original sign before snapping/truncation: FACT(-0.1) and
    // FACT(-eps) are #NUM!, while FACTDOUBLE has a different negative-domain rule
    // and does not apply this near-integer snap.
    if !n.is_finite() || n < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let adjusted = n + EXCEL_FACT_NEAR_INT_EPS;
    if adjusted >= 171.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let truncated = adjusted.trunc() as i64;
    ExcelRealPolicy::FINITE.publish(n, fact_reverse_product(truncated))
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
    fn fact_kernel_truncates_and_rejects_negative() {
        assert_eq!(fact_kernel(5.9), Ok(120.0));
        assert_eq!(fact_kernel(-1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(fact_kernel(-0.1), Err(WorksheetErrorCode::Num));
        assert_eq!(
            fact_kernel(-f64::MIN_POSITIVE),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(fact_kernel(-0.0), Ok(1.0));
    }

    // BUG-FUNC-027 / oxf-vgxs: live Excel FACT(171)=#NUM!, FACT(170) finite.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fact_kernel_overflow_maps_to_num() {
        assert_eq!(fact_kernel(171.0), Err(WorksheetErrorCode::Num));
        assert_eq!(fact_kernel(f64::MAX), Err(WorksheetErrorCode::Num));
        assert!(fact_kernel(170.0).is_ok());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fact_exact_publication_controls_remain_exact() {
        assert_bits(fact_kernel(9.0).expect("fact(9)"), 362880.0_f64);
        assert_bits(fact_kernel(2.0).expect("fact(2)"), 2.0_f64);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fact_reverse_product_matches_live_excel_large_n() {
        assert_eq!(fact_kernel(25.0).unwrap().to_bits(), 0x4529a940c33f6120);
        assert_eq!(fact_kernel(100.0).unwrap().to_bits(), 0x60bb30964ec395de);
        assert_eq!(fact_kernel(170.0).unwrap().to_bits(), 0x7fa4ab786441863d);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_j9k_568_fact_snaps_within_excel_eps_below_171_to_num() {
        // Live Excel 16.0 b20625 (Touchstone ng6): FACT snaps inputs within
        // 0x3e90020000000000 (2^-22 + 2^-33) below an integer up to that integer.
        // At n=171 this produces #NUM! down to 0x40655fffff7ff000 inclusive,
        // and 170! at 1 ULP below (0x40655fffff7fefff).
        assert_eq!(
            fact_kernel(f64::from_bits(0x40655ffffffffc73)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            fact_kernel(f64::from_bits(0x40655ffffffffffe)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            fact_kernel(f64::from_bits(0x40655fffff7ff000)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            fact_kernel(f64::from_bits(0x40655fffff7fefff))
                .unwrap()
                .to_bits(),
            0x7fa4ab786441863d
        );
        // Negative values within eps below 0 still return #NUM! before snap.
        assert_eq!(
            fact_kernel(f64::from_bits(0xbe90020000000000)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            fact_kernel(f64::from_bits(0xbc9cd2b297d889bc)),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_j9k_569_fact_snaps_near_integers_across_all_boundaries() {
        // Live Excel 16.0 b20625 (Touchstone ng6): verified all 170 integer
        // boundaries n=2..=171 transition at exactly n - 0x3e90020000000000.
        assert_eq!(
            fact_kernel(f64::from_bits(0x405cbffffffffffe))
                .unwrap()
                .to_bits(),
            0x6710ce83f7f82d2c
        );
        let eps = f64::from_bits(0x3e90020000000000);
        for n in 2..=170_i64 {
            let boundary_bits = (n as f64 - eps).to_bits();
            let exact = fact_kernel(n as f64).unwrap().to_bits();
            let prev = fact_kernel((n - 1) as f64).unwrap().to_bits();
            assert_eq!(
                fact_kernel(f64::from_bits(boundary_bits))
                    .unwrap()
                    .to_bits(),
                exact,
                "n={n} at boundary 0x{boundary_bits:016x}"
            );
            assert_eq!(
                fact_kernel(f64::from_bits(boundary_bits - 1))
                    .unwrap()
                    .to_bits(),
                prev,
                "n={n} at 1 ULP below boundary 0x{:016x}",
                boundary_bits - 1
            );
        }
    }
}

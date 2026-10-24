use crate::value::WorksheetErrorCode;

pub fn sqrtpi_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    if n < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Excel computes SQRTPI(n) = `(n·π)^0.5` via its `pow` routine — NOT the IEEE
    // correctly-rounded `sqrt` — and publishes `#NUM!` once the `n·π` intermediate overflows
    // f64. Reproduce BOTH: form the same `n*π` product, reject (`#NUM!`) when it is non-finite
    // (the overflow flips at exactly Excel's input, n > f64::MAX/π ≈ 5.7222122209853366e307,
    // 0x7fd45f306dc9c882↔0x7fd45f306dc9c883), and take `powf(0.5)` rather than `sqrt`. `powf`
    // matches Excel bit-exactly across the whole range INCLUDING the scattered 1-ULP points
    // near overflow where correctly-rounded `sqrt` differs from Excel's `pow` — e.g. the
    // n*π==f64::MAX input 0x7fd45f306dc9c882: `sqrt`→0x5fefffffffffffff, but Excel and `powf`
    // both →0x5ff0000000000000. (This is `pow` vs `sqrt`, not x87/extended precision: 64-bit
    // Excel and Rust are both SSE2; the difference is the non-correctly-rounded pow path.)
    // Verified live Excel 16.0 build 20026 (.tmp/sqrtpi-broad-oracle.ps1 + sqrtpi-boundary-
    // oracle.ps1): 30/30 sampled inputs, `powf(0.5)` == Excel; `sqrt` missed the 2 near-overflow
    // points. Resolves oxf-quxx.
    let product = n * std::f64::consts::PI;
    if !product.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(product.powf(0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqrtpi_kernel_negative_is_num() {
        assert_eq!(sqrtpi_kernel(-1.0), Err(WorksheetErrorCode::Num));
    }

    /// Excel publishes `#NUM!` once the internal `n·π` intermediate overflows f64, and the
    /// finite square root below that. The boundary is the EXACT `n*π`-overflow point,
    /// `n = f64::MAX/π` which rounds to `0x7fd45f306dc9c882`; the next f64
    /// (`0x7fd45f306dc9c883`) overflows. Probed live against Excel 16.0 build 20026
    /// (.tmp/asinh-sqrtpi-oracle4.ps1): Excel returns finite at and below `0x7fd45f306dc9c882`
    /// and `#NUM!` at and above `0x7fd45f306dc9c883`. OxFunc must flip at the SAME input.
    #[test]
    fn sqrtpi_kernel_matches_excel_num_on_internal_n_pi_overflow() {
        // Last finite input (n*π == f64::MAX, finite). Excel publishes 0x5ff0000000000000 here
        // via its `(n·π)^0.5` pow path; `powf(0.5)` matches it bit-exactly, where IEEE `sqrt`
        // gives 0x5fefffffffffffff (1 ULP low). Fixed in oxf-quxx.
        let last_finite = f64::from_bits(0x7fd4_5f30_6dc9_c882);
        assert!((last_finite * std::f64::consts::PI).is_finite());
        assert!(sqrtpi_kernel(last_finite).unwrap().is_finite());

        // First input whose `n*π` overflows -> Excel #NUM!.
        let first_num = f64::from_bits(0x7fd4_5f30_6dc9_c883);
        assert!(!(first_num * std::f64::consts::PI).is_finite());
        assert_eq!(sqrtpi_kernel(first_num), Err(WorksheetErrorCode::Num));

        // The range OxFunc historically published +Inf for (Excel #NUM!): 5.8e307 .. f64::MAX.
        for n in [5.8e307, 1.0e308, f64::MAX] {
            assert_eq!(
                sqrtpi_kernel(n),
                Err(WorksheetErrorCode::Num),
                "SQRTPI({n:e})"
            );
        }

        // Finite values below the threshold stay finite and bit-exact to Excel (oracle2.ps1).
        assert_eq!(
            sqrtpi_kernel(1.0e10).unwrap().to_bits(),
            0x4105_a2eb_14aa_5ae9
        );
        assert_eq!(
            sqrtpi_kernel(1.0e300).unwrap().to_bits(),
            0x5f21_53bf_f724_c583
        );
        assert_eq!(
            sqrtpi_kernel(1.0e307).unwrap().to_bits(),
            0x5fda_c128_1685_1ee0
        );
        assert_eq!(
            sqrtpi_kernel(5.0e307).unwrap().to_bits(),
            0x5fed_e997_0174_6551
        );
    }

    /// At the last finite input Excel's `(n·π)^0.5` gives 0x5ff0000000000000, as the MSVC
    /// runtime's `pow` does. glibc's `pow` is correctly rounded there and gives the `sqrt` bits
    /// 0x5fefffffffffffff (Linux sweep, dc-ns0), so off MSVC this kernel is one ULP from Excel at
    /// this input; dc-sbo owns a portable form of Excel's `pow`.
    #[cfg(target_env = "msvc")]
    #[test]
    fn sqrtpi_kernel_matches_excel_bits_at_the_last_finite_input() {
        let last_finite = f64::from_bits(0x7fd4_5f30_6dc9_c882);
        assert_eq!(
            sqrtpi_kernel(last_finite).unwrap().to_bits(),
            0x5ff0_0000_0000_0000
        );
    }
}

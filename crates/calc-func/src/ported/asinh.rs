use crate::ported::excel_numeric::{excel_sqrt_staged, excel_x87_add, excel_x87_mul};
use crate::value::WorksheetErrorCode;

pub fn asinh_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    // Excel computes `ln(x + sqrt(x² + 1))` and overflows when the `x²` intermediate
    // overflows f64, publishing `#NUM!` rather than the (finite, representable) analytic
    // value for such large |x|. Reproduce that boundary *bit-exactly* by forming the same
    // `x*x` intermediate Excel forms and rejecting when it is non-finite: this flips to
    // `#NUM!` at exactly the input Excel does (|x| > sqrt(f64::MAX) = 1.3407807929942596e154,
    // verified live against Excel 16.0 build 20026 around 0x5fefffffffffffff↔0x5ff0000000000000).
    // No guessed magic constant — identical f64 arithmetic yields Excel's exact threshold.
    if !(n * n).is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    // Inverse-problem identity, live Excel 16.0 b20228 Range.Value2:
    // ASINH(x) == SIGN(x)*LN(ABS(x)+SQRT(x*x+1)) on 23/23 signed rows.
    // The unsigned LN(x+SQRT(x*x+1)) misses negatives. Worksheet HYPOT
    // is not the graph. When x*x+1 overflows (x*x == f64::MAX), SQRT
    // cannot be formed; the published finite value is LN(2*|x|).
    let a = n.abs();
    // The recorded identity stores each operation from PC64, including SQRT.
    // Retaining the entire expression in the registers loses recorded matches.
    let sq = excel_x87_mul(a, a);
    let radicand = excel_x87_add(sq, 1.0);
    let inner = if !radicand.is_finite() {
        excel_x87_add(a, a)
    } else {
        excel_x87_add(a, excel_sqrt_staged(radicand))
    };
    Ok(n.signum() * crate::ported::excel_numeric::excel_log(inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn asinh_kernel_matches_worksheet_ln_identity() {
        for x in [0.0_f64, 1e-10, 0.5, 1.0, -1.0, 2.0, 10.0] {
            let a = x.abs();
            let expect =
                x.signum() * crate::ported::excel_numeric::excel_log(a + (a * a + 1.0).sqrt());
            assert_eq!(
                asinh_kernel(x).unwrap().to_bits(),
                expect.to_bits(),
                "x={x}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn asinh_kernel_matches_pinned_excel_publication_on_disputed_rows() {
        assert_eq!(asinh_kernel(1.0), Ok(0.8813735870195429));
        assert_eq!(asinh_kernel(-1.0), Ok(-0.8813735870195429));
        assert_eq!(asinh_kernel(1.0e-10), Ok(1.000000082690371e-10));
        assert_eq!(asinh_kernel(-1.0e-10), Ok(-1.000000082690371e-10));
    }

    /// Excel publishes `#NUM!` once the internal `x²` intermediate overflows f64, and the
    /// finite analytic value below that. The boundary is the EXACT `x*x`-overflow point,
    /// `|x| = sqrt(f64::MAX) = 0x5fefffffffffffff`; the next f64 (`0x5ff0000000000000`)
    /// overflows. Probed live against Excel 16.0 build 20026 (.tmp/asinh-sqrtpi-oracle4.ps1):
    /// Excel returns finite at and below `0x5fefffffffffffff` and `#NUM!` at and above
    /// `0x5ff0000000000000`, on both signs. OxFunc must flip at the SAME input.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn asinh_kernel_matches_excel_num_on_internal_x_squared_overflow() {
        // Last finite input (|x| = sqrt(f64::MAX)); `x*x` is finite (== f64::MAX).
        let last_finite = f64::from_bits(0x5fef_ffff_ffff_ffff);
        assert_eq!(last_finite, last_finite.abs());
        assert!((last_finite * last_finite).is_finite());
        // Finite, and bit-exact to Excel's published value at this boundary input.
        assert_eq!(asinh_kernel(last_finite), Ok(355.58450362725193));
        assert_eq!(
            asinh_kernel(last_finite).unwrap().to_bits(),
            0x4076_395a_2079_b70c
        );
        assert_eq!(asinh_kernel(-last_finite), Ok(-355.58450362725193));

        // First input whose `x*x` overflows -> Excel #NUM!, both signs.
        let first_num = f64::from_bits(0x5ff0_0000_0000_0000);
        assert!(!(first_num * first_num).is_finite());
        assert_eq!(asinh_kernel(first_num), Err(WorksheetErrorCode::Num));
        assert_eq!(asinh_kernel(-first_num), Err(WorksheetErrorCode::Num));

        // The wide range the previous `|x| + hypot(x,1)` guard drifted across (Excel #NUM!,
        // OxFunc historically finite/+Inf): 1e155 .. f64::MAX.
        for x in [1.0e155, 1.0e200, 1.0e300, 1.0e308, f64::MAX] {
            assert_eq!(
                asinh_kernel(x),
                Err(WorksheetErrorCode::Num),
                "ASINH({x:e})"
            );
            assert_eq!(
                asinh_kernel(-x),
                Err(WorksheetErrorCode::Num),
                "ASINH(-{x:e})"
            );
        }

        // Mid-range finite values stay finite and bit-exact to Excel (oracle2.ps1).
        assert_eq!(
            asinh_kernel(1.0e150).unwrap().to_bits(),
            0x4075_a14b_6977_fcd2
        );
        assert_eq!(
            asinh_kernel(1.3e154).unwrap().to_bits(),
            0x4076_38db_9c34_9fe9
        );
    }
}

use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

/// PERMUTATIONA: n to the power k, both truncated, by repeated squaring (10^k by decimal
/// conversion); #NUM! outside the admitted domain.
pub fn permutationa_kernel(n: f64, k: f64) -> Result<f64, WorksheetErrorCode> {
    // W111 distinguishes raw-base admission from exponent truncation:
    // a negative fractional base is invalid, but -1 < k < 0 truncates to zero.
    if !n.is_finite()
        || !k.is_finite()
        || n < 0.0
        || n >= 2147483647.0
        || k >= 2147483647.0
        || k.trunc() < 0.0
    {
        return Err(WorksheetErrorCode::Num);
    }
    let n = n.trunc();
    let mut k = k.trunc() as u32;
    // The base-ten branch agrees with decimal power conversion, distinguishable
    // from repeated squaring at 10^99, 10^100 and later powers. Fresh validation
    // covers the full finite decimal exponent range before any parity promotion.
    let result = if n == 10.0 {
        if k > 308 {
            f64::INFINITY
        } else {
            format!("1e{k}").parse().unwrap_or(f64::INFINITY)
        }
    } else {
        let mut result = 1.0;
        let mut base = n;
        while k > 0 {
            if k & 1 != 0 {
                result = crate::ported::excel_numeric::excel_x87_mul(result, base);
            }
            k >>= 1;
            if k > 0 {
                base = crate::ported::excel_numeric::excel_x87_mul(base, base);
            }
        }
        result
    };
    ExcelRealPolicy::FINITE.publish(n, result)
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

    #[test]
    fn permutationa_kernel_matches_excel_lanes() {
        assert_eq!(permutationa_kernel(3.0, 2.0), Ok(9.0));
        assert_eq!(permutationa_kernel(3.9, 2.1), Ok(9.0));
        assert_eq!(permutationa_kernel(0.0, 0.0), Ok(1.0));
        assert_eq!(permutationa_kernel(0.0, 1.0), Ok(0.0));
        assert_eq!(permutationa_kernel(-1.0, 1.0), Err(WorksheetErrorCode::Num));
        // BUG-FUNC-027 CLASS-A5: live Excel 16.0 b20026 PERMUTATIONA(163,150)=#NUM!.
        assert_eq!(
            permutationa_kernel(163.0, 150.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[test]
    fn permutationa_exact_publication_controls_remain_exact() {
        assert_bits(
            permutationa_kernel(3.0, 2.0).expect("permutationa(3,2)"),
            9.0_f64,
        );
        assert_bits(
            permutationa_kernel(4.0, 3.0).expect("permutationa(4,3)"),
            64.0_f64,
        );
    }
}

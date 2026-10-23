use crate::ported::factorial_common::trunc_nonnegative;
use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

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
    // Excel checks the original sign before truncation: FACT(-0.1) is #NUM!,
    // while FACTDOUBLE deliberately has a different negative-domain rule.
    // Bound the product before converting to an integer, including huge inputs.
    if !n.is_finite() || n >= 171.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let truncated = trunc_nonnegative(n)?;
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

    #[test]
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
    #[test]
    fn fact_kernel_overflow_maps_to_num() {
        assert_eq!(fact_kernel(171.0), Err(WorksheetErrorCode::Num));
        assert_eq!(fact_kernel(f64::MAX), Err(WorksheetErrorCode::Num));
        assert!(fact_kernel(170.0).is_ok());
    }

    #[test]
    fn fact_exact_publication_controls_remain_exact() {
        assert_bits(fact_kernel(9.0).expect("fact(9)"), 362880.0_f64);
        assert_bits(fact_kernel(2.0).expect("fact(2)"), 2.0_f64);
    }

    #[test]
    fn fact_reverse_product_matches_live_excel_large_n() {
        assert_eq!(fact_kernel(25.0).unwrap().to_bits(), 0x4529a940c33f6120);
        assert_eq!(fact_kernel(100.0).unwrap().to_bits(), 0x60bb30964ec395de);
        assert_eq!(fact_kernel(170.0).unwrap().to_bits(), 0x7fa4ab786441863d);
    }
}

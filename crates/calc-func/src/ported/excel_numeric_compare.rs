use crate::value::WorksheetErrorCode;
use std::cmp::Ordering;

const EXCEL_COMPARE_SIGNIFICANT_DIGITS: usize = 15;

fn truncate_to_significant_digits(value: f64, digits: usize) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }

    let exponent = value.abs().log10().floor();
    let scale = 10_f64.powf((digits as f64) - exponent - 1.0);
    (value * scale).trunc() / scale
}

fn normalize_excel_compare_number(value: f64) -> f64 {
    truncate_to_significant_digits(value, EXCEL_COMPARE_SIGNIFICANT_DIGITS)
}

pub fn compare_excel_numbers(lhs: f64, rhs: f64) -> Ordering {
    let lhs = normalize_excel_compare_number(lhs);
    let rhs = normalize_excel_compare_number(rhs);
    lhs.partial_cmp(&rhs).unwrap_or(Ordering::Equal)
}

pub fn excel_numbers_equal(lhs: f64, rhs: f64) -> bool {
    compare_excel_numbers(lhs, rhs) == Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excel_numeric_compare_matches_observed_near_equal_lanes() {
        assert!(excel_numbers_equal(0.1 + 0.2, 0.3));
        assert!(excel_numbers_equal(5.0 + 1.0e-15, 5.0));
        assert!(excel_numbers_equal(5.0 + 2.0e-15, 5.0));
        assert!(!excel_numbers_equal(1.0 + 1.0e-14, 1.0));
    }

    #[test]
    fn excel_numeric_compare_preserves_order_when_values_are_not_near_equal() {
        assert_eq!(compare_excel_numbers(0.1 + 0.2, 0.3), Ordering::Equal);
        assert_eq!(compare_excel_numbers(1.0 + 1.0e-14, 1.0), Ordering::Greater);
        assert_eq!(compare_excel_numbers(-1.0 - 1.0e-14, -1.0), Ordering::Less);
    }

    #[test]
    fn excel_numeric_compare_matches_arithmetic_generated_boundary_pairs() {
        let lhs = ((123_456_789_012_345_f64 * 10.0) + 5.0) / 1.0e25;
        let rhs = ((123_456_789_012_345_f64 * 10.0) + 4.0) / 1.0e25;
        assert_eq!(compare_excel_numbers(lhs, rhs), Ordering::Equal);

        let neg_lhs = ((-123_456_789_012_345_f64 * 10.0) - 5.0) / 1.0e25;
        let neg_rhs = ((-123_456_789_012_345_f64 * 10.0) - 4.0) / 1.0e25;
        assert_eq!(compare_excel_numbers(neg_lhs, neg_rhs), Ordering::Equal);
    }
}

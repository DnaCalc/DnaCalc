use crate::value::WorksheetErrorCode;
pub const BIT_MAX: u64 = 281_474_976_710_655;
pub const BIT_SHIFT_MAX: i32 = 53;

pub fn coerce_bit_operand(n: f64) -> Result<u64, WorksheetErrorCode> {
    // Excel's bit operands must already be integers. Truncation is only
    // permitted for a shift count, not for the number being shifted/masked.
    if !n.is_finite() || n < 0.0 || n > BIT_MAX as f64 || n != n.trunc() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(n as u64)
}

pub fn coerce_shift_count(n: f64) -> Result<i32, WorksheetErrorCode> {
    let truncated = n.trunc();
    if !n.is_finite() || truncated.abs() > BIT_SHIFT_MAX as f64 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(truncated as i32)
}

/// Common BITLSHIFT/BITRSHIFT graph observed in Excel 16.0 build 20430, CV2.
/// A zero operand bypasses the finite shift-count range check; an admitted
/// truncated count with magnitude 49..=53 publishes zero in either direction.
/// These are empirical exceptions to the published <=53/48-bit description.
/// Evidence: w111-broad-20260929 BITLSHIFT/BITRSHIFT Value2 witness sets.
pub fn bit_shift_kernel(number: f64, shift: f64, left: bool) -> Result<f64, WorksheetErrorCode> {
    let number = coerce_bit_operand(number)?;
    if !shift.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    if number == 0 {
        return Ok(0.0);
    }
    let shift = coerce_shift_count(shift)?;
    if shift.abs() > 48 {
        return Ok(0.0);
    }
    let shift = if left { shift } else { -shift };
    let result = if shift >= 0 {
        // checked_shl only checks whether the count is >=64: it does not
        // detect discarded high bits. Check the Excel 48-bit bound first.
        if number > BIT_MAX >> shift {
            return Err(WorksheetErrorCode::Num);
        }
        number << shift
    } else {
        number >> -shift
    };
    Ok(result as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_common_bounds_match_excel_seed() {
        assert_eq!(coerce_bit_operand(281_474_976_710_655.0), Ok(BIT_MAX));
        assert_eq!(
            coerce_bit_operand(281_474_976_710_656.0),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(coerce_shift_count(53.0), Ok(53));
        assert_eq!(coerce_shift_count(54.0), Err(WorksheetErrorCode::Num));
    }

    #[test]
    fn bit_operands_reject_fractional_values_instead_of_truncating() {
        for n in [0.9, 1.5, 33_072_894_177_336.25, 188_572_911_003_774.5] {
            assert_eq!(coerce_bit_operand(n), Err(WorksheetErrorCode::Num));
        }
        assert_eq!(coerce_shift_count(-13.5), Ok(-13));
        assert_eq!(coerce_shift_count(53.5), Ok(53));
    }

    #[test]
    fn bit_shift_bounds_and_empirical_shortcuts_match_value2_witnesses() {
        assert_eq!(bit_shift_kernel(0.0, 54.0, true), Ok(0.0));
        assert_eq!(bit_shift_kernel(1.0, 53.0, true), Ok(0.0));
        assert_eq!(bit_shift_kernel(1.0, -53.0, false), Ok(0.0));
        assert_eq!(
            bit_shift_kernel(1.0, 48.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            bit_shift_kernel(1.0, -48.0, false),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            bit_shift_kernel(140_737_488_355_328.0, 47.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            bit_shift_kernel(140_737_488_355_328.0, -48.0, false),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            bit_shift_kernel(197_029_275_778_236.0, -13.5, true),
            Ok(24_051_425_265.0)
        );
    }
}

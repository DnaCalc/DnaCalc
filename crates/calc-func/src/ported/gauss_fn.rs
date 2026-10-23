use crate::ported::normal_log_family::identified_gauss;
use crate::value::WorksheetErrorCode;

pub fn gauss_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(identified_gauss(x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauss_matches_exact_excel_value_witness() {
        let got = gauss_kernel(1.0).unwrap();
        assert!(
            (got - 0.341_344_746_068_543_04).abs() <= 1e-15,
            "expected exact Excel witness, got {got}"
        );
    }

    #[test]
    fn gauss_wrapper_pins_live_excel_20228() {
        // Live Excel 16.0 b20228 Value2. GAUSS(1) is bit-exact through the
        // G-F3 complement path even while the ERFC body is 1 ULP high at
        // z = RN(1/√2): `1 - 0.5*Q` absorbs that ULP. GAUSS(-1) still
        // inherits the body miss (same z as CHIDIST(1,1)); this dispatch
        // claims the wrapper, not the ERFC body.
        assert_eq!(gauss_kernel(1.0).unwrap().to_bits(), 0x3fd5d897a241a6fc);
        let gauss_neg1 = gauss_kernel(-1.0).unwrap();
        assert!(
            gauss_neg1.to_bits().abs_diff(0xbfd5d897a241a6fc) <= 1,
            "G-F3 should not add error beyond the ERFC body; got {:#x}",
            gauss_neg1.to_bits()
        );
        assert_eq!(gauss_kernel(0.0).unwrap().to_bits(), 0);
        // Inclusive tiny-direct: 2^-50 is below 1e-15 and transports PHI(0).
        let tiny = f64::from_bits(0x3cd0000000000000);
        assert_eq!(gauss_kernel(tiny).unwrap().to_bits(), 0x3cb9884533d43651);
        assert_eq!(gauss_kernel(-tiny).unwrap().to_bits(), 0xbcb9884533d43651);
        // 2^-49 is above 1e-15: ordinary wrapper, RNE 6-ulp-of-1/2 subtract.
        let just_above = f64::from_bits(0x3ce0000000000000);
        assert_eq!(
            gauss_kernel(just_above).unwrap().to_bits(),
            0x3cc8000000000000
        );
    }
}

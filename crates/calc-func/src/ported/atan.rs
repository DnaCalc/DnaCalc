use crate::value::WorksheetErrorCode;

pub fn atan_kernel(n: f64) -> f64 {
    crate::ported::excel_numeric::excel_atan_reduced(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atan_kernel_matches_std() {
        assert_eq!(atan_kernel(1.0), 1.0f64.atan());
    }

    #[test]
    fn atan_matches_live_excel_pins() {
        assert_eq!(atan_kernel(0.5).to_bits(), 0x3fddac670561bb4f);
        assert_eq!(atan_kernel(1.0).to_bits(), 0x3fe921fb54442d18);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn atan_retains_extended_inverse_angle_and_pi() {
        assert_eq!(atan_kernel(f64::from_bits(0xc0000d685e592174)).to_bits(), 0xbff1bc3aef431d2a);
        assert_eq!(atan_kernel(f64::from_bits(0x40001dc1079c3c90)).to_bits(), 0x3ff1c2b6c60d2617);
    }
}

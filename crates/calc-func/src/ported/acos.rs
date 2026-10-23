use crate::value::WorksheetErrorCode;

pub fn acos_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    // W111 signed controls distinguish the staged subtraction near half an
    // output ULP. ASIN's reused-factor graph is shared without re-evaluation.
    match crate::ported::asin::asin_kernel(n) {
        Ok(a) => Ok(crate::ported::excel_numeric::excel_x87_sub(
            std::f64::consts::FRAC_PI_2,
            a,
        )),
        Err(_) => Err(WorksheetErrorCode::Num),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acos_kernel_rejects_out_of_domain() {
        assert_eq!(acos_kernel(2.0), Err(WorksheetErrorCode::Num));
    }

    #[test]
    fn acos_matches_live_excel_pi2_asin_pins() {
        assert_eq!(acos_kernel(0.5).unwrap().to_bits(), 0x3ff0c152382d7365);
        assert_eq!(acos_kernel(0.75).unwrap().to_bits(), 0x3fe720a392c1d954);
        assert_eq!(acos_kernel(0.0).unwrap().to_bits(), 0x3ff921fb54442d18);
        assert_eq!(acos_kernel(1.0).unwrap().to_bits(), 0x0000000000000000);
        assert_eq!(acos_kernel(-1.0).unwrap().to_bits(), 0x400921fb54442d18);
        assert_eq!(acos_kernel(-0.5).unwrap().to_bits(), 0x4000c152382d7366);
    }
}

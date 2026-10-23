use crate::value::WorksheetErrorCode;

/// W109 G4-01: the fFTAN π/2-quadrant chain (see `excel_numeric::excel_tan`).
pub fn tan_kernel(n: f64) -> f64 {
    crate::ported::excel_numeric::excel_tan(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tan_kernel_matches_std() {
        assert_eq!(tan_kernel(1.0), 1.0f64.tan());
    }
}

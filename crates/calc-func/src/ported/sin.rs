use crate::value::WorksheetErrorCode;

/// W109 G4-01: the SIN chain (see `excel_numeric::excel_sin`),
/// bit-exact to live Excel below the 2^27 guard.
pub fn sin_kernel(n: f64) -> f64 {
    crate::ported::excel_numeric::excel_sin(n)
}

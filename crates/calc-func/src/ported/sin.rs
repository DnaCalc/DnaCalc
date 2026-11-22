use crate::value::WorksheetErrorCode;

/// dc-j9k.87 / cb16: the unified 8087 `FPTAN` circular chain (see `excel_numeric::excel_sin`),
/// bit-exact to live Excel below the 2^27 guard.
pub fn sin_kernel(n: f64) -> f64 {
    crate::ported::excel_numeric::excel_sin(n)
}

use crate::value::WorksheetErrorCode;

pub fn delta_kernel(lhs: f64, rhs: f64) -> Result<f64, WorksheetErrorCode> {
    Ok(if lhs == rhs { 1.0 } else { 0.0 })
}

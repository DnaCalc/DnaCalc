use crate::value::WorksheetErrorCode;

pub fn gestep_kernel(number: f64, step: f64) -> Result<f64, WorksheetErrorCode> {
    Ok(if number >= step { 1.0 } else { 0.0 })
}

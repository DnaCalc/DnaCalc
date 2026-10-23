use crate::ported::bit_common::bit_shift_kernel;
use crate::value::WorksheetErrorCode;

pub fn bitlshift_kernel(number: f64, shift: f64) -> Result<f64, WorksheetErrorCode> {
    bit_shift_kernel(number, shift, true)
}

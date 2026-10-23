use crate::ported::bit_common::{coerce_bit_operand};
use crate::value::WorksheetErrorCode;

pub fn bitxor_kernel(lhs: f64, rhs: f64) -> Result<f64, WorksheetErrorCode> {
    Ok((coerce_bit_operand(lhs)? ^ coerce_bit_operand(rhs)?) as f64)
}

use crate::ported::normal_dist_common::phi_kernel as phi_kernel_impl;
use crate::value::WorksheetErrorCode;

pub fn phi_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    // W111 square-overflow observations distinguish worksheet density admission
    // from the unguarded internal primitive used in other numerical graphs.
    if !crate::ported::excel_numeric::excel_x87_mul(x, x).is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(phi_kernel_impl(x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phi_matches_exact_excel_value_witness() {
        let got = phi_kernel(0.0).unwrap();
        assert!(
            (got - 0.398_942_280_401_432_7).abs() <= 1e-15,
            "expected exact Excel witness, got {got}"
        );
    }
}

use crate::value::WorksheetErrorCode;

pub fn radians_kernel(n: f64) -> f64 {
    let result = crate::ported::excel_numeric::excel_x87_mul(n, std::f64::consts::PI / 180.0);
    if result.abs() < f64::MIN_POSITIVE { 0.0 } else { result }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radians_kernel_maps_180_to_pi() {
        assert_eq!(radians_kernel(180.0), std::f64::consts::PI);
    }
}

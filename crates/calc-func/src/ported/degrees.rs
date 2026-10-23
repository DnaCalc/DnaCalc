use crate::ported::real_policy::ExcelRealPolicy;
use crate::value::WorksheetErrorCode;

pub fn degrees_kernel(n: f64) -> f64 {
    n * (180.0 / std::f64::consts::PI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degrees_kernel_maps_pi_to_180() {
        assert_eq!(degrees_kernel(std::f64::consts::PI), 180.0);
    }

    // BUG-FUNC-027 / oxf-vgxs: live Excel DEGREES(1E307)=#NUM! (overflow guard on surface).
    #[test]
    fn degrees_overflow_maps_to_num() {
        assert_eq!(
            ExcelRealPolicy::FINITE.publish(1e307, degrees_kernel(1e307)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            ExcelRealPolicy::FINITE
                .publish(std::f64::consts::PI, degrees_kernel(std::f64::consts::PI)),
            Ok(180.0)
        );
    }
}

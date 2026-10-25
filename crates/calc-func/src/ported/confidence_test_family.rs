use crate::ported::chi_f_t_family::t_inv_2t_kernel;
use crate::ported::variance_common::{VarianceDivisor, stdev_from_values};
use crate::value::WorksheetErrorCode;

pub fn confidence_t_kernel(
    alpha: f64,
    standard_dev: f64,
    size: f64,
) -> Result<f64, WorksheetErrorCode> {
    if !alpha.is_finite() || !standard_dev.is_finite() || !size.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    let n = size.trunc();
    if !(0.0 < alpha && alpha < 1.0) || standard_dev <= 0.0 || n < 2.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(t_inv_2t_kernel(alpha, n - 1.0)? * standard_dev / n.sqrt())
}

pub fn z_test_kernel(
    values: &[f64],
    x: f64,
    sigma: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    if values.is_empty() || !x.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let sigma = match sigma {
        Some(value) => value,
        None => stdev_from_values(values, VarianceDivisor::Sample)?,
    };
    if sigma <= 0.0 || !sigma.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    // Live Excel 16.0 b20326: Z.TEST(array,x) with omitted sigma is
    // NORMSDIST((x-AVERAGE)/(STDEV.S/SQRT(n))), not 1-NORMSDIST((mean-x)/se)
    // and not STDEV.P. The two NORMSDIST complements are not bit-identical.
    // F1 (dc-8dq): (x - mean) * sqrt(n) / sigma, as Excel's Z.TEST gives Z.TEST({1,5,3},2) (the
    // catalogue oracle's capture); dividing by sigma/sqrt(n) is one ulp away there.
    let z = (x - mean) * (values.len() as f64).sqrt() / sigma;
    crate::ported::normal_log_family::norm_s_dist_kernel(z, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn confidence_t_matches_known_lane() {
        let got = confidence_t_kernel(0.05, 2.5, 50.0).unwrap();
        assert!((got - 0.7104921387393248).abs() < 1e-9);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn confidence_t_rejects_invalid_domain() {
        assert_eq!(
            confidence_t_kernel(1.0, 2.0, 10.0),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            confidence_t_kernel(0.05, 0.0, 10.0),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            confidence_t_kernel(0.05, 2.0, 1.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn z_test_matches_seed_lane_with_supplied_sigma() {
        let values = [3.0, 6.0, 7.0, 8.0, 6.0];
        let got = z_test_kernel(&values, 4.0, Some(1.5)).unwrap();
        assert!((got - 0.0014345563960383074).abs() < 1e-12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn z_test_omitted_sigma_matches_normsdist_stdev_s_identity() {
        // Live Excel 16.0 b20326 Value2: Z.TEST = NORMSDIST((x-mean)/(STDEV.S/√n)).
        assert_eq!(
            z_test_kernel(&[1.0, 2.0, 3.0, 4.0, 5.0], 4.0, None)
                .unwrap()
                .to_bits(),
            0x3fed7bb3d3a08445
        );
        assert_eq!(
            z_test_kernel(&[10.0, 12.0, 15.0, 11.0], 13.0, None)
                .unwrap()
                .to_bits(),
            0x3fea53ce6913b61a
        );
        assert_eq!(
            z_test_kernel(&[1.0, 2.0, 3.0, 4.0, 5.0], 4.0, Some(2.0))
                .unwrap()
                .to_bits(),
            0x3febc87d3294a339
        );
    }
}

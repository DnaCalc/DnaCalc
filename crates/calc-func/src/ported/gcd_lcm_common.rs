use crate::value::WorksheetErrorCode;

pub(crate) const GCD_LCM_LIMIT: f64 = 9_007_199_254_740_992.0;

pub fn gcd_int(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.abs()
}

pub fn lcm_int(a: i64, b: i64) -> Result<i64, WorksheetErrorCode> {
    if a == 0 || b == 0 {
        Ok(0)
    } else {
        // Each admitted operand is at most 2^53, so the exact product fits
        // u128. Publish its binary64 rounding before checking the next step.
        // In particular, 2^53 + 1 rounds to the admitted boundary 2^53.
        let product = (a / gcd_int(a, b)) as u128 * b as u128;
        let rounded = product as f64;
        if rounded > GCD_LCM_LIMIT {
            Err(WorksheetErrorCode::Num)
        } else {
            Ok(rounded as i64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gcd_and_lcm_basic_lanes() {
        assert_eq!(gcd_int(24, 36), 12);
        assert_eq!(gcd_int(0, 5), 5);
        assert_eq!(lcm_int(6, 8), Ok(24));
        assert_eq!(lcm_int(0, 5), Ok(0));
        assert_eq!(lcm_int(3, 3_002_399_751_580_331), Ok(9_007_199_254_740_992));
        assert_eq!(
            lcm_int(9_007_199_254_740_991, 9_007_199_254_740_990),
            Err(WorksheetErrorCode::Num)
        );
    }
}

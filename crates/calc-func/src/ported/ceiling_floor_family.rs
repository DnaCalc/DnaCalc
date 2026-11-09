use crate::value::WorksheetErrorCode;

/// The quotient number/significance as the CEILING and FLOOR family counts multiples from it:
/// rounded to 15 significant digits first, so a quotient within that rounding of an integer is
/// that integer (dc-j9k: 736 of 736 CEILING, FLOOR, .MATH and .PRECISE readings in the catalogue
/// oracle's sheet dcj9k_math; CEILING(1*(1+2^-48),1) is 1 and CEILING(1*(1+2^-47),1) is 2,
/// FLOOR(1*(1-2^-51),1) is 1 and FLOOR(1*(1-2^-50),1) is 0).
pub(crate) fn excel_quotient(number: f64, significance: f64) -> f64 {
    let quotient = number / significance;
    if quotient.is_subnormal() {
        return 0.0;
    }
    if !quotient.is_finite() || quotient == 0.0 {
        return quotient;
    }
    format!("{quotient:.14e}").parse().unwrap_or(quotient)
}

fn normalize_zero(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { x }
}

pub fn ceiling_kernel(number: f64, significance: f64) -> Result<f64, WorksheetErrorCode> {
    if significance == 0.0 {
        return Ok(0.0);
    }
    if number > 0.0 && significance < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(normalize_zero(
        significance * excel_quotient(number, significance).ceil(),
    ))
}

pub fn floor_kernel(number: f64, significance: f64) -> Result<f64, WorksheetErrorCode> {
    if significance == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    if number > 0.0 && significance < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(normalize_zero(
        significance * excel_quotient(number, significance).floor(),
    ))
}

pub fn ceiling_math_kernel(
    number: f64,
    significance: f64,
    mode: f64,
) -> Result<f64, WorksheetErrorCode> {
    let significance = significance.abs();
    if significance == 0.0 {
        return Ok(0.0);
    }
    let result = if number >= 0.0 {
        significance * excel_quotient(number, significance).ceil()
    } else if mode != 0.0 {
        -significance * excel_quotient(-number, significance).ceil()
    } else {
        -significance * excel_quotient(-number, significance).floor()
    };
    Ok(normalize_zero(result))
}

pub fn floor_math_kernel(
    number: f64,
    significance: f64,
    mode: f64,
) -> Result<f64, WorksheetErrorCode> {
    let significance = significance.abs();
    if significance == 0.0 {
        return Ok(0.0);
    }
    let result = if number >= 0.0 {
        significance * excel_quotient(number, significance).floor()
    } else if mode != 0.0 {
        -significance * excel_quotient(-number, significance).floor()
    } else {
        -significance * excel_quotient(-number, significance).ceil()
    };
    Ok(normalize_zero(result))
}

pub fn ceiling_precise_kernel(number: f64, significance: f64) -> Result<f64, WorksheetErrorCode> {
    ceiling_math_kernel(number, significance, 0.0)
}

pub fn iso_ceiling_kernel(number: f64, significance: f64) -> Result<f64, WorksheetErrorCode> {
    ceiling_math_kernel(number, significance, 0.0)
}

pub fn floor_precise_kernel(number: f64, significance: f64) -> Result<f64, WorksheetErrorCode> {
    floor_math_kernel(number, significance, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ceiling_legacy_seed_lanes_match_excel_probe() {
        assert_eq!(ceiling_kernel(4.3, 2.0), Ok(6.0));
        assert_eq!(ceiling_kernel(-4.3, 2.0), Ok(-4.0));
        assert_eq!(ceiling_kernel(-4.3, -2.0), Ok(-6.0));
        assert_eq!(ceiling_kernel(4.3, -2.0), Err(WorksheetErrorCode::Num));
        assert_eq!(ceiling_kernel(5.0, 0.0), Ok(0.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn floor_legacy_seed_lanes_match_excel_probe() {
        assert_eq!(floor_kernel(4.3, 2.0), Ok(4.0));
        assert_eq!(floor_kernel(-4.3, 2.0), Ok(-6.0));
        assert_eq!(floor_kernel(-4.3, -2.0), Ok(-4.0));
        assert_eq!(floor_kernel(4.3, -2.0), Err(WorksheetErrorCode::Num));
        assert_eq!(floor_kernel(5.0, 0.0), Err(WorksheetErrorCode::Div0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn math_and_precise_seed_lanes_match_excel_probe() {
        assert_eq!(ceiling_math_kernel(-4.3, 2.0, 0.0), Ok(-4.0));
        assert_eq!(ceiling_math_kernel(-4.3, 2.0, 1.0), Ok(-6.0));
        assert_eq!(ceiling_math_kernel(4.3, -2.0, 0.0), Ok(6.0));
        assert_eq!(ceiling_math_kernel(5.0, 0.0, 0.0), Ok(0.0));
        assert_eq!(floor_math_kernel(-4.3, 2.0, 0.0), Ok(-6.0));
        assert_eq!(floor_math_kernel(-4.3, 2.0, 1.0), Ok(-4.0));
        assert_eq!(floor_math_kernel(4.3, -2.0, 0.0), Ok(4.0));
        assert_eq!(floor_math_kernel(5.0, 0.0, 0.0), Ok(0.0));
        assert_eq!(ceiling_precise_kernel(-4.3, 2.0), Ok(-4.0));
        assert_eq!(floor_precise_kernel(-4.3, 2.0), Ok(-6.0));
        assert_eq!(iso_ceiling_kernel(-4.3, 2.0), Ok(-4.0));
    }
}

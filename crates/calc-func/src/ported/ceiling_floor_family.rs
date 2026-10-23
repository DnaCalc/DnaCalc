use crate::value::WorksheetErrorCode;

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
        significance * (number / significance).ceil(),
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
        significance * (number / significance).floor(),
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
        significance * (number / significance).ceil()
    } else if mode != 0.0 {
        -significance * ((-number) / significance).ceil()
    } else {
        -significance * ((-number) / significance).floor()
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
        significance * (number / significance).floor()
    } else if mode != 0.0 {
        -significance * ((-number) / significance).floor()
    } else {
        -significance * ((-number) / significance).ceil()
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

    #[test]
    fn ceiling_legacy_seed_lanes_match_excel_probe() {
        assert_eq!(ceiling_kernel(4.3, 2.0), Ok(6.0));
        assert_eq!(ceiling_kernel(-4.3, 2.0), Ok(-4.0));
        assert_eq!(ceiling_kernel(-4.3, -2.0), Ok(-6.0));
        assert_eq!(ceiling_kernel(4.3, -2.0), Err(WorksheetErrorCode::Num));
        assert_eq!(ceiling_kernel(5.0, 0.0), Ok(0.0));
    }

    #[test]
    fn floor_legacy_seed_lanes_match_excel_probe() {
        assert_eq!(floor_kernel(4.3, 2.0), Ok(4.0));
        assert_eq!(floor_kernel(-4.3, 2.0), Ok(-6.0));
        assert_eq!(floor_kernel(-4.3, -2.0), Ok(-4.0));
        assert_eq!(floor_kernel(4.3, -2.0), Err(WorksheetErrorCode::Num));
        assert_eq!(floor_kernel(5.0, 0.0), Err(WorksheetErrorCode::Div0));
    }

    #[test]
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

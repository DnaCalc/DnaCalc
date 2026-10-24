use crate::value::WorksheetErrorCode;

fn digit_value(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some((c as u8 - b'0') as u32),
        'A'..='Z' => Some((c as u8 - b'A') as u32 + 10),
        _ => None,
    }
}

pub fn decimal_kernel(text: &str, radix: f64) -> Result<f64, WorksheetErrorCode> {
    let radix = radix.trunc();
    if !(2.0..=36.0).contains(&radix) {
        return Err(WorksheetErrorCode::Num);
    }
    let radix = radix as u32;
    let normalized = text.trim_start().to_ascii_uppercase();
    if normalized.is_empty() {
        return Ok(0.0);
    }

    let mut value: u128 = 0;
    for ch in normalized.chars() {
        let Some(digit) = digit_value(ch) else {
            return Err(WorksheetErrorCode::Num);
        };
        if digit >= radix {
            return Err(WorksheetErrorCode::Num);
        }
        value = value
            .checked_mul(radix as u128)
            .and_then(|v| v.checked_add(digit as u128))
            .ok_or(WorksheetErrorCode::Num)?;
    }
    Ok(value as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn decimal_kernel_matches_excel_seed_rows() {
        assert_eq!(decimal_kernel("FF", 16.0), Ok(255.0));
        assert_eq!(decimal_kernel("111", 2.0), Ok(7.0));
        assert_eq!(decimal_kernel("Z", 36.0), Ok(35.0));
        assert_eq!(decimal_kernel("G", 16.0), Err(WorksheetErrorCode::Num));
        assert_eq!(decimal_kernel("10", 1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(decimal_kernel("10", 37.0), Err(WorksheetErrorCode::Num));
        assert_eq!(decimal_kernel("", 16.0), Ok(0.0));
        assert_eq!(decimal_kernel("ff", 16.0), Ok(255.0));
        assert_eq!(decimal_kernel("10.5", 2.0), Err(WorksheetErrorCode::Num));
        assert_eq!(decimal_kernel("  10", 2.0), Ok(2.0));
        assert_eq!(decimal_kernel("10\t", 2.0), Err(WorksheetErrorCode::Num));
        assert_eq!(decimal_kernel("10", 2.9), Ok(2.0));
    }
}

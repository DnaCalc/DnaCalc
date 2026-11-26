use crate::value::ExcelText;
use crate::value::WorksheetErrorCode;

fn roman_value(ch: char) -> Option<i64> {
    match ch {
        'I' => Some(1),
        'V' => Some(5),
        'X' => Some(10),
        'L' => Some(50),
        'C' => Some(100),
        'D' => Some(500),
        'M' => Some(1000),
        _ => None,
    }
}

pub fn arabic_kernel(text: &ExcelText) -> Result<f64, WorksheetErrorCode> {
    let units = text.utf16_code_units();
    if units.len() > 255 {
        return Err(WorksheetErrorCode::Value);
    }
    let text = String::from_utf16_lossy(units);
    let trimmed = text.trim_matches([' ', '\t', '\r', '\n']);
    let (sign, numerals) = match trimmed.strip_prefix('-') {
        Some("") => return Err(WorksheetErrorCode::Num),
        Some(numerals) => (-1, numerals),
        None => (1, trimmed),
    };
    let mut total = 0i64;
    let mut prev = 0i64;
    for ch in numerals.chars().rev() {
        let value = roman_value(ch.to_ascii_uppercase()).ok_or(WorksheetErrorCode::Value)?;
        if value < prev {
            total -= value;
        } else {
            total += value;
            prev = value;
        }
    }
    Ok((total * sign) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(s: &str) -> ExcelText {
        ExcelText::from_utf16_code_units(s.encode_utf16().collect())
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn arabic_kernel_matches_excel_seed_rows() {
        assert_eq!(arabic_kernel(&txt("LVII")), Ok(57.0));
        assert_eq!(arabic_kernel(&txt("mcmxii")), Ok(1912.0));
        assert_eq!(arabic_kernel(&txt("")), Ok(0.0));
        assert_eq!(arabic_kernel(&txt("IV")), Ok(4.0));
        assert_eq!(arabic_kernel(&txt("ABC")), Err(WorksheetErrorCode::Value));
    }
}

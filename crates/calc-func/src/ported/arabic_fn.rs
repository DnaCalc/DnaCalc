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
    let s = String::from_utf16_lossy(text.utf16_code_units()).to_uppercase();
    if s.is_empty() {
        return Ok(0.0);
    }
    let mut total = 0i64;
    let mut prev = 0i64;
    for ch in s.chars().rev() {
        let value = roman_value(ch).ok_or(WorksheetErrorCode::Value)?;
        if value < prev {
            total -= value;
        } else {
            total += value;
            prev = value;
        }
    }
    Ok(total as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(s: &str) -> ExcelText {
        ExcelText::from_utf16_code_units(s.encode_utf16().collect())
    }

    #[test]
    fn arabic_kernel_matches_excel_seed_rows() {
        assert_eq!(arabic_kernel(&txt("LVII")), Ok(57.0));
        assert_eq!(arabic_kernel(&txt("mcmxii")), Ok(1912.0));
        assert_eq!(arabic_kernel(&txt("")), Ok(0.0));
        assert_eq!(arabic_kernel(&txt("IV")), Ok(4.0));
        assert_eq!(arabic_kernel(&txt("ABC")), Err(WorksheetErrorCode::Value));
    }
}

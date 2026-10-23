use crate::value::ExcelText;
use crate::value::WorksheetErrorCode;

fn int_to_base(mut value: i64, radix: i64) -> String {
    const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if value == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while value > 0 {
        let digit = (value % radix) as usize;
        out.push(DIGITS[digit] as char);
        value /= radix;
    }
    out.into_iter().rev().collect()
}

pub fn base_kernel(
    number: f64,
    radix: f64,
    min_length: Option<f64>,
) -> Result<ExcelText, WorksheetErrorCode> {
    // Value2 captures on Excel 20430/CV2: sign and 2^53 admission precede
    // truncation, while the optional length is truncated within 0..255.
    if !number.is_finite()
        || !(0.0..9_007_199_254_740_992.0).contains(&number)
        || !radix.is_finite()
        || min_length.is_some_and(|n| !n.is_finite() || n < 0.0)
    {
        return Err(WorksheetErrorCode::Num);
    }
    let number = number.trunc();
    let radix = radix.trunc();
    let min_length = min_length.unwrap_or(1.0).trunc();
    if !(2.0..=36.0).contains(&radix) || min_length > 255.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // An explicit zero length differs from omitting the optional argument.
    let mut out = if number == 0.0 && min_length == 0.0 {
        String::new()
    } else {
        int_to_base(number as i64, radix as i64)
    };
    while out.len() < min_length as usize {
        out.insert(0, '0');
    }
    Ok(ExcelText::from_utf16_code_units(
        out.encode_utf16().collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_kernel_matches_excel_seed_rows() {
        let plain = base_kernel(31.0, 16.0, None).unwrap();
        assert_eq!(String::from_utf16_lossy(plain.utf16_code_units()), "1F");
        let padded = base_kernel(31.9, 16.0, Some(4.8)).unwrap();
        assert_eq!(String::from_utf16_lossy(padded.utf16_code_units()), "001F");
        assert_eq!(base_kernel(-1.0, 16.0, None), Err(WorksheetErrorCode::Num));
        assert_eq!(base_kernel(31.0, 1.0, None), Err(WorksheetErrorCode::Num));
    }
}

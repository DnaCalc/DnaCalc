use crate::value::ExcelText;
use crate::ported::base_fn::base_kernel;
use crate::value::WorksheetErrorCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RadixSpec {
    radix: u32,
    bits: u32,
    max_chars: usize,
}

const BIN_SPEC: RadixSpec = RadixSpec {
    radix: 2,
    bits: 10,
    max_chars: 10,
};
const OCT_SPEC: RadixSpec = RadixSpec {
    radix: 8,
    bits: 30,
    max_chars: 10,
};
const HEX_SPEC: RadixSpec = RadixSpec {
    radix: 16,
    bits: 40,
    max_chars: 10,
};

fn upper_ascii_digit_value(ch: char) -> Option<u32> {
    match ch {
        '0'..='9' => Some((ch as u8 - b'0') as u32),
        'A'..='Z' => Some((ch as u8 - b'A') as u32 + 10),
        _ => None,
    }
}

fn normalize_radix_text(text: &str) -> String {
    text.to_ascii_uppercase()
}

fn parse_radix_text(text: &str, source: RadixSpec) -> Result<i64, WorksheetErrorCode> {
    let normalized = normalize_radix_text(text);
    if normalized.is_empty() {
        return Ok(0);
    }
    if normalized.len() > source.max_chars {
        return Err(WorksheetErrorCode::Num);
    }

    let mut raw: u64 = 0;
    for ch in normalized.chars() {
        let Some(digit) = upper_ascii_digit_value(ch) else {
            return Err(WorksheetErrorCode::Num);
        };
        if digit >= source.radix {
            return Err(WorksheetErrorCode::Num);
        }
        raw = raw
            .checked_mul(source.radix as u64)
            .and_then(|value| value.checked_add(digit as u64))
            .ok_or(WorksheetErrorCode::Num)?;
    }

    if normalized.len() == source.max_chars {
        let sign_mask = 1_u64 << (source.bits - 1);
        if raw & sign_mask != 0 {
            return Ok(raw as i64 - (1_i64 << source.bits));
        }
    }

    Ok(raw as i64)
}

pub(crate) fn normalize_places(places: Option<f64>) -> Result<Option<usize>, WorksheetErrorCode> {
    match places {
        None => Ok(None),
        Some(raw) => {
            let truncated = raw.trunc();
            if !raw.is_finite() || !(1.0..=10.0).contains(&truncated) {
                Err(WorksheetErrorCode::Num)
            } else {
                Ok(Some(truncated as usize))
            }
        }
    }
}

fn min_signed_value(spec: RadixSpec) -> i64 {
    -(1_i64 << (spec.bits - 1))
}

fn max_signed_value(spec: RadixSpec) -> i64 {
    (1_i64 << (spec.bits - 1)) - 1
}

fn encode_signed_value(
    value: i64,
    target: RadixSpec,
    places: Option<f64>,
) -> Result<ExcelText, WorksheetErrorCode> {
    if value < min_signed_value(target) || value > max_signed_value(target) {
        return Err(WorksheetErrorCode::Num);
    }

    // Excel validates an explicitly supplied width even when a negative value
    // always publishes its full ten-character two's-complement encoding.
    let places = normalize_places(places)?;

    if value < 0 {
        let raw = ((1_i128 << target.bits) + i128::from(value)) as f64;
        return base_kernel(raw, target.radix as f64, Some(target.max_chars as f64));
    }

    let digits = base_kernel(value as f64, target.radix as f64, None)?;
    let digits_string = digits.to_string_lossy();

    match places {
        Some(min_length) => {
            if digits_string.len() > min_length {
                Err(WorksheetErrorCode::Num)
            } else {
                let padded = format!("{digits_string:0>width$}", width = min_length);
                Ok(ExcelText::from_utf16_code_units(
                    padded.encode_utf16().collect(),
                ))
            }
        }
        None => Ok(digits),
    }
}

fn trunc_i64(number: f64) -> i64 {
    number.trunc() as i64
}

pub(crate) fn dec_to_target_kernel(
    number: f64,
    places: Option<f64>,
    target: RadixSpec,
) -> Result<ExcelText, WorksheetErrorCode> {
    encode_signed_value(trunc_i64(number), target, places)
}

pub(crate) fn source_to_decimal_kernel(
    source_text: &str,
    source: RadixSpec,
) -> Result<f64, WorksheetErrorCode> {
    parse_radix_text(source_text, source).map(|value| value as f64)
}

pub(crate) fn source_to_target_kernel(
    source_text: &str,
    places: Option<f64>,
    source: RadixSpec,
    target: RadixSpec,
) -> Result<ExcelText, WorksheetErrorCode> {
    let decimal = parse_radix_text(source_text, source)?;
    encode_signed_value(decimal, target, places)
}

pub fn dec2bin_kernel(number: f64, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    dec_to_target_kernel(number, places, BIN_SPEC)
}

pub fn dec2hex_kernel(number: f64, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    dec_to_target_kernel(number, places, HEX_SPEC)
}

pub fn dec2oct_kernel(number: f64, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    dec_to_target_kernel(number, places, OCT_SPEC)
}

pub fn bin2dec_kernel(number: &str) -> Result<f64, WorksheetErrorCode> {
    source_to_decimal_kernel(number, BIN_SPEC)
}

pub fn bin2hex_kernel(number: &str, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    source_to_target_kernel(number, places, BIN_SPEC, HEX_SPEC)
}

pub fn bin2oct_kernel(number: &str, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    source_to_target_kernel(number, places, BIN_SPEC, OCT_SPEC)
}

pub fn hex2bin_kernel(number: &str, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    source_to_target_kernel(number, places, HEX_SPEC, BIN_SPEC)
}

pub fn hex2dec_kernel(number: &str) -> Result<f64, WorksheetErrorCode> {
    source_to_decimal_kernel(number, HEX_SPEC)
}

pub fn hex2oct_kernel(number: &str, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    source_to_target_kernel(number, places, HEX_SPEC, OCT_SPEC)
}

pub fn oct2bin_kernel(number: &str, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    source_to_target_kernel(number, places, OCT_SPEC, BIN_SPEC)
}

pub fn oct2dec_kernel(number: &str) -> Result<f64, WorksheetErrorCode> {
    source_to_decimal_kernel(number, OCT_SPEC)
}

pub fn oct2hex_kernel(number: &str, places: Option<f64>) -> Result<ExcelText, WorksheetErrorCode> {
    source_to_target_kernel(number, places, OCT_SPEC, HEX_SPEC)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_text_eq(result: Result<ExcelText, WorksheetErrorCode>, expected: &str) {
        let actual = result.unwrap().to_string_lossy();
        assert_eq!(actual, expected);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn decimal_to_engineering_radix_matches_seed_rows() {
        assert_text_eq(dec2bin_kernel(9.0, Some(4.0)), "1001");
        assert_text_eq(dec2bin_kernel(-100.0, Some(2.0)), "1110011100");
        assert_text_eq(dec2hex_kernel(100.0, Some(4.0)), "0064");
        assert_text_eq(dec2hex_kernel(-54.0, Some(3.0)), "FFFFFFFFCA");
        assert_text_eq(dec2oct_kernel(58.0, Some(3.0)), "072");
        assert_text_eq(dec2oct_kernel(-100.0, None), "7777777634");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn decimal_to_engineering_radix_enforces_ranges_and_places() {
        assert_eq!(dec2bin_kernel(512.0, None), Err(WorksheetErrorCode::Num));
        assert_eq!(
            dec2hex_kernel(549_755_813_888.0, None),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            dec2oct_kernel(58.0, Some(1.0)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(dec2bin_kernel(1.0, Some(0.0)), Err(WorksheetErrorCode::Num));
        assert_text_eq(dec2hex_kernel(255.0, Some(4.8)), "00FF");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn radix_to_decimal_matches_signed_ten_digit_rules() {
        assert_eq!(bin2dec_kernel("1100100"), Ok(100.0));
        assert_eq!(bin2dec_kernel("1111111111"), Ok(-1.0));
        assert_eq!(hex2dec_kernel("3DA408B9"), Ok(1_034_160_313.0));
        assert_eq!(hex2dec_kernel("FFFFFFFF5B"), Ok(-165.0));
        assert_eq!(oct2dec_kernel("54"), Ok(44.0));
        assert_eq!(oct2dec_kernel("7777777533"), Ok(-165.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cross_radix_text_conversions_match_seed_rows() {
        assert_text_eq(bin2hex_kernel("11111011", Some(4.0)), "00FB");
        assert_text_eq(bin2hex_kernel("1111111111", Some(1.0)), "FFFFFFFFFF");
        assert_text_eq(bin2oct_kernel("1001", Some(3.0)), "011");
        assert_text_eq(hex2bin_kernel("F", Some(8.0)), "00001111");
        assert_text_eq(hex2oct_kernel("FFFFFFFF5B", Some(1.0)), "7777777533");
        assert_text_eq(oct2bin_kernel("3", Some(4.0)), "0011");
        assert_text_eq(oct2hex_kernel("100", Some(4.0)), "0040");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cross_radix_text_conversions_reject_invalid_inputs() {
        assert_eq!(bin2hex_kernel("102", None), Err(WorksheetErrorCode::Num));
        assert_eq!(
            bin2oct_kernel("11111111111", None),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            hex2bin_kernel("1000000000", None),
            Err(WorksheetErrorCode::Num)
        );
        assert_text_eq(hex2oct_kernel("", None), "0");
        assert_eq!(hex2oct_kernel(" 1", None), Err(WorksheetErrorCode::Num));
        assert_eq!(oct2bin_kernel("8", None), Err(WorksheetErrorCode::Num));
    }
}

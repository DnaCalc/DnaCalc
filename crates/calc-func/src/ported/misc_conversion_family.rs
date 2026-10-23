use crate::value::ExcelText;
use crate::value::WorksheetErrorCode;
use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul};

const THAI_DIGITS: [&str; 10] = [
    "ศูนย์",
    "หนึ่ง",
    "สอง",
    "สาม",
    "สี่",
    "ห้า",
    "หก",
    "เจ็ด",
    "แปด",
    "เก้า",
];
const THAI_PLACES: [&str; 6] = ["", "สิบ", "ร้อย", "พัน", "หมื่น", "แสน"];

fn render_thai_under_million(mut value: u32, has_higher_digits: bool) -> String {
    if value == 0 {
        return String::new();
    }
    let mut digits = [0_u32; 6];
    for idx in 0..6 {
        digits[idx] = value % 10;
        value /= 10;
    }
    let mut out = String::new();
    for pos in (0..6).rev() {
        let digit = digits[pos];
        if digit == 0 {
            continue;
        }
        match pos {
            1 if digit == 1 => out.push_str("สิบ"),
            1 if digit == 2 => out.push_str("ยี่สิบ"),
            1 => {
                out.push_str(THAI_DIGITS[digit as usize]);
                out.push_str("สิบ");
            }
            // "เอ็ด" for a final 1 after any higher digit, including a higher million group:
            // 1,000,001 is "หนึ่งล้านเอ็ด" (live Excel 20430, W111-5 G8-07).
            0 if digit == 1 && (has_higher_digits || digits[1..].iter().any(|d| *d != 0)) => {
                out.push_str("เอ็ด")
            }
            0 => out.push_str(THAI_DIGITS[digit as usize]),
            _ => {
                out.push_str(THAI_DIGITS[digit as usize]);
                out.push_str(THAI_PLACES[pos]);
            }
        }
    }
    out
}

fn render_thai_integer(mut value: u128) -> String {
    if value == 0 {
        return "ศูนย์".to_string();
    }
    let mut groups = Vec::new();
    while value > 0 {
        groups.push((value % 1_000_000) as u32);
        value /= 1_000_000;
    }
    let mut out = String::new();
    for (idx, group) in groups.iter().enumerate().rev() {
        if *group == 0 {
            continue;
        }
        let has_higher_groups = groups[idx + 1..].iter().any(|g| *g != 0);
        out.push_str(&render_thai_under_million(*group, has_higher_groups));
        for _ in 0..idx {
            out.push_str("ล้าน");
        }
    }
    out
}

/// The amount in satang (hundredths): the value is first rounded to 15 significant digits,
/// then to two decimals half away from zero in decimal, so 1.005 is 1 baht 1 satang, 0.995 is
/// one baht, and 9,999,999,999,999,998 reads as 10^16 (live Excel 20430, W111-5 G8-07).
fn bahttext_satang(magnitude: f64) -> u128 {
    if magnitude == 0.0 {
        return 0;
    }
    // 15 significant digits: d.dddddddddddddd e X
    let text = format!("{magnitude:.14e}");
    let (mantissa, exponent) = text.split_once('e').expect("exponent form");
    let digits: u128 = mantissa.replace('.', "").parse().expect("15 digits");
    let exponent: i32 = exponent.parse().expect("exponent");
    // value = digits * 10^(exponent - 14); satang = value * 100 = digits * 10^(exponent - 12)
    let shift = exponent - 12;
    if shift >= 0 {
        return digits * 10u128.pow(shift as u32);
    }
    let divisor = 10u128.pow((-shift) as u32);
    let (quotient, remainder) = (digits / divisor, digits % divisor);
    if remainder * 2 >= divisor { quotient + 1 } else { quotient }
}

pub fn bahttext_kernel(value: f64) -> Result<ExcelText, WorksheetErrorCode> {
    if !value.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    // Negative amounts are spelled with a leading "ลบ" (minus), and an amount under one baht
    // omits the "ศูนย์บาท" (zero baht) part: BAHTTEXT(-5) is "ลบห้าบาทถ้วน", BAHTTEXT(0.25) is
    // "ยี่สิบห้าสตางค์" (live Excel 20430, W111-5 G8-07).
    let magnitude = value.abs();
    if magnitude > 9_999_999_999_999_999.0 {
        return Err(WorksheetErrorCode::Num);
    }
    let satang_total = bahttext_satang(magnitude);
    let baht = satang_total / 100;
    let satang = (satang_total % 100) as u32;
    let mut rendered = String::new();
    // Any negative value is prefixed, even one that rounds to zero ("ลบศูนย์บาทถ้วน").
    if value < 0.0 {
        rendered.push_str("ลบ");
    }
    if baht > 0 || satang == 0 {
        rendered.push_str(&render_thai_integer(baht));
        rendered.push_str("บาท");
    }
    if satang == 0 {
        rendered.push_str("ถ้วน");
    } else {
        rendered.push_str(&render_thai_under_million(satang, false));
        rendered.push_str("สตางค์");
    }
    Ok(ExcelText::from_utf16_code_units(
        rendered.encode_utf16().collect(),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConvertCategory {
    Length,
    Mass,
    Time,
    Pressure,
    Volume,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LinearUnit {
    category: ConvertCategory,
    factor_to_base: f64,
    prefix_exponent: i32,
}

fn convert_direct_linear_unit(unit: &str) -> Option<LinearUnit> {
    match unit {
        "m" => Some(LinearUnit {
            category: ConvertCategory::Length,
            factor_to_base: 10_000_000_000.0,
            prefix_exponent: 0,
        }),
        "in" => Some(LinearUnit {
            category: ConvertCategory::Length,
            factor_to_base: 254_000_000.0,
            prefix_exponent: 0,
        }),
        "ft" => Some(LinearUnit {
            category: ConvertCategory::Length,
            factor_to_base: 3_048_000_000.0,
            prefix_exponent: 0,
        }),
        "yd" => Some(LinearUnit {
            category: ConvertCategory::Length,
            factor_to_base: 9_144_000_000.0,
            prefix_exponent: 0,
        }),
        "mi" => Some(LinearUnit {
            category: ConvertCategory::Length,
            factor_to_base: 16_093_440_000_000.0,
            prefix_exponent: 0,
        }),
        "Nmi" => Some(LinearUnit {
            category: ConvertCategory::Length,
            factor_to_base: 18_520_000_000_000.0,
            prefix_exponent: 0,
        }),
        "g" => Some(LinearUnit {
            category: ConvertCategory::Mass,
            factor_to_base: 1.0,
            prefix_exponent: 0,
        }),
        "lbm" => Some(LinearUnit {
            category: ConvertCategory::Mass,
            factor_to_base: 453.59237,
            prefix_exponent: 0,
        }),
        "ozm" => Some(LinearUnit {
            category: ConvertCategory::Mass,
            factor_to_base: 28.349_523_125,
            prefix_exponent: 0,
        }),
        "sec" => Some(LinearUnit {
            category: ConvertCategory::Time,
            factor_to_base: 1.0,
            prefix_exponent: 0,
        }),
        "mn" => Some(LinearUnit {
            category: ConvertCategory::Time,
            factor_to_base: 60.0,
            prefix_exponent: 0,
        }),
        "hr" => Some(LinearUnit {
            category: ConvertCategory::Time,
            factor_to_base: 3600.0,
            prefix_exponent: 0,
        }),
        "day" => Some(LinearUnit {
            category: ConvertCategory::Time,
            factor_to_base: 86_400.0,
            prefix_exponent: 0,
        }),
        "Pa" => Some(LinearUnit {
            category: ConvertCategory::Pressure,
            factor_to_base: 1.0,
            prefix_exponent: 0,
        }),
        "atm" => Some(LinearUnit {
            category: ConvertCategory::Pressure,
            factor_to_base: 101_325.0,
            prefix_exponent: 0,
        }),
        "psi" => Some(LinearUnit {
            category: ConvertCategory::Pressure,
            // Reciprocal of the public units-per-pascal table entry
            // 1.4503773773020920E-04, independently rounded to binary64.
            factor_to_base: f64::from_bits(0x40ba_eec1_ddf7_0f99),
            prefix_exponent: 0,
        }),
        "l" => Some(LinearUnit {
            category: ConvertCategory::Volume,
            factor_to_base: 1.0,
            prefix_exponent: 0,
        }),
        "tsp" => Some(LinearUnit {
            category: ConvertCategory::Volume,
            factor_to_base: 0.004_928_921_593_75,
            prefix_exponent: 0,
        }),
        "tbs" => Some(LinearUnit {
            category: ConvertCategory::Volume,
            factor_to_base: 0.014_786_764_781_25,
            prefix_exponent: 0,
        }),
        "oz" => Some(LinearUnit {
            category: ConvertCategory::Volume,
            factor_to_base: 0.029_573_529_562_5,
            prefix_exponent: 0,
        }),
        "cup" => Some(LinearUnit {
            category: ConvertCategory::Volume,
            factor_to_base: 0.236_588_236_5,
            prefix_exponent: 0,
        }),
        "pt" => Some(LinearUnit {
            category: ConvertCategory::Volume,
            factor_to_base: 0.473_176_473,
            prefix_exponent: 0,
        }),
        "qt" => Some(LinearUnit {
            category: ConvertCategory::Volume,
            factor_to_base: 0.946_352_946,
            prefix_exponent: 0,
        }),
        "gal" => Some(LinearUnit {
            category: ConvertCategory::Volume,
            factor_to_base: 3.785_411_784,
            prefix_exponent: 0,
        }),
        _ => None,
    }
}

fn convert_prefix_exponent(prefix: &str) -> Option<i32> {
    match prefix {
        "Y" => Some(24),
        "Z" => Some(21),
        "E" => Some(18),
        "P" => Some(15),
        "T" => Some(12),
        "G" => Some(9),
        "M" => Some(6),
        "k" => Some(3),
        "h" => Some(2),
        "da" => Some(1),
        "d" => Some(-1),
        "c" => Some(-2),
        "m" => Some(-3),
        "u" => Some(-6),
        "n" => Some(-9),
        "p" => Some(-12),
        "f" => Some(-15),
        _ => None,
    }
}

fn convert_linear_unit(unit: &str) -> Option<LinearUnit> {
    if let Some(direct) = convert_direct_linear_unit(unit) {
        return Some(direct);
    }
    for base in ["sec", "Pa", "m", "g", "l"] {
        if let Some(prefix) = unit.strip_suffix(base) {
            let mut resolved = convert_direct_linear_unit(base)?;
            resolved.prefix_exponent = convert_prefix_exponent(prefix)?;
            return Some(resolved);
        }
    }
    None
}

const CONVERT_DECIMAL_POW10: [f64; 79] = [
    1.0e-39, 1.0e-38, 1.0e-37, 1.0e-36, 1.0e-35, 1.0e-34, 1.0e-33, 1.0e-32, 1.0e-31, 1.0e-30,
    1.0e-29, 1.0e-28, 1.0e-27, 1.0e-26, 1.0e-25, 1.0e-24, 1.0e-23, 1.0e-22, 1.0e-21, 1.0e-20,
    1.0e-19, 1.0e-18, 1.0e-17, 1.0e-16, 1.0e-15, 1.0e-14, 1.0e-13, 1.0e-12, 1.0e-11, 1.0e-10,
    1.0e-9, 1.0e-8, 1.0e-7, 1.0e-6, 1.0e-5, 1.0e-4, 1.0e-3, 1.0e-2, 1.0e-1, 1.0e0, 1.0e1, 1.0e2,
    1.0e3, 1.0e4, 1.0e5, 1.0e6, 1.0e7, 1.0e8, 1.0e9, 1.0e10, 1.0e11, 1.0e12, 1.0e13, 1.0e14,
    1.0e15, 1.0e16, 1.0e17, 1.0e18, 1.0e19, 1.0e20, 1.0e21, 1.0e22, 1.0e23, 1.0e24, 1.0e25, 1.0e26,
    1.0e27, 1.0e28, 1.0e29, 1.0e30, 1.0e31, 1.0e32, 1.0e33, 1.0e34, 1.0e35, 1.0e36, 1.0e37, 1.0e38,
    1.0e39,
];

fn convert_decimal_power10(exponent: i32) -> f64 {
    debug_assert!((-39..=39).contains(&exponent));
    CONVERT_DECIMAL_POW10[(exponent + 39) as usize]
}

fn convert_temperature(value: f64, from: &str, to: &str) -> Option<f64> {
    match (from, to) {
        (left, right) if left == right && matches!(left, "K" | "C" | "F") => Some(value),
        ("K", "C") => Some(value - 273.15),
        ("C", "K") => Some(value + 273.15),
        ("K", "F") => Some((value - 273.15) * 1.8 + 32.0),
        ("F", "K") => Some((value - 32.0) / 1.8 + 273.15),
        ("C", "F") => Some(value * 1.8 + 32.0),
        ("F", "C") => Some((value - 32.0) / 1.8),
        _ => None,
    }
}

pub fn convert_kernel(
    number: f64,
    from_unit: &str,
    to_unit: &str,
) -> Result<f64, WorksheetErrorCode> {
    if !number.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    let from = from_unit.trim();
    let to = to_unit.trim();
    if let Some(converted) = convert_temperature(number, from, to) {
        return Ok(converted);
    }
    let from = convert_linear_unit(from).ok_or(WorksheetErrorCode::NA)?;
    let to = convert_linear_unit(to).ok_or(WorksheetErrorCode::NA)?;
    if from.category != to.category {
        return Err(WorksheetErrorCode::NA);
    }
    // Frozen W109 G4-05 graph, selected before and exact on the disjoint
    // 10,418-row publication check. Every arithmetic site is an x87 PC64 op
    // followed by a binary64 store; prefix scaling is a separate final site.
    let product = excel_x87_mul(number, from.factor_to_base);
    let core = excel_x87_div(product, to.factor_to_base);
    let prefix_delta = convert_decimal_power10(from.prefix_exponent - to.prefix_exponent);
    Ok(excel_x87_mul(core, prefix_delta))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EuroCurrency {
    rate: f64,
    decimals: i32,
}

fn euro_currency(code: &str) -> Option<EuroCurrency> {
    match code {
        "EUR" => Some(EuroCurrency {
            rate: 1.0,
            decimals: 2,
        }),
        "ATS" => Some(EuroCurrency {
            rate: 13.7603,
            decimals: 2,
        }),
        "BEF" => Some(EuroCurrency {
            rate: 40.3399,
            decimals: 0,
        }),
        "DEM" => Some(EuroCurrency {
            rate: 1.95583,
            decimals: 2,
        }),
        "ESP" => Some(EuroCurrency {
            rate: 166.386,
            decimals: 0,
        }),
        "FIM" => Some(EuroCurrency {
            rate: 5.94573,
            decimals: 2,
        }),
        "FRF" => Some(EuroCurrency {
            rate: 6.55957,
            decimals: 2,
        }),
        "GRD" => Some(EuroCurrency {
            rate: 340.75,
            decimals: 0,
        }),
        "IEP" => Some(EuroCurrency {
            rate: 0.787564,
            decimals: 2,
        }),
        "ITL" => Some(EuroCurrency {
            rate: 1936.27,
            decimals: 0,
        }),
        "LUF" => Some(EuroCurrency {
            rate: 40.3399,
            decimals: 0,
        }),
        "NLG" => Some(EuroCurrency {
            rate: 2.20371,
            decimals: 2,
        }),
        "PTE" => Some(EuroCurrency {
            rate: 200.482,
            decimals: 2,
        }),
        _ => None,
    }
}

fn round_to_decimals(value: f64, decimals: i32) -> f64 {
    let scale = 10_f64.powi(decimals);
    (value * scale).round() / scale
}

fn round_to_significant_digits(value: f64, digits: usize) -> f64 {
    if value == 0.0 {
        return 0.0;
    }
    let exponent = value.abs().log10().floor();
    let scale = 10_f64.powf((digits as f64) - exponent - 1.0);
    (value * scale).round() / scale
}

pub fn euroconvert_kernel(
    number: f64,
    source: &str,
    target: &str,
    full_precision: Option<bool>,
    triangulation_precision: Option<f64>,
) -> Result<f64, WorksheetErrorCode> {
    if !number.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    let source = source.trim().to_ascii_uppercase();
    let target = target.trim().to_ascii_uppercase();
    let source_currency = euro_currency(&source).ok_or(WorksheetErrorCode::NA)?;
    let target_currency = euro_currency(&target).ok_or(WorksheetErrorCode::NA)?;
    let tri_digits = match triangulation_precision {
        None => None,
        Some(raw) if !raw.is_finite() => return Err(WorksheetErrorCode::Num),
        Some(raw) => {
            let digits = raw.trunc() as i64;
            if !(3..=15).contains(&digits) {
                return Err(WorksheetErrorCode::Num);
            }
            Some(digits as usize)
        }
    };
    let mut euro_intermediate = if source == "EUR" {
        number
    } else {
        number / source_currency.rate
    };
    if source != "EUR" && target != "EUR" {
        if let Some(digits) = tri_digits {
            euro_intermediate = round_to_significant_digits(euro_intermediate, digits);
        }
    }
    let converted = if target == "EUR" {
        euro_intermediate
    } else {
        euro_intermediate * target_currency.rate
    };
    Ok(if full_precision.unwrap_or(false) {
        converted
    } else {
        round_to_decimals(converted, target_currency.decimals)
    })
}

pub fn percentof_kernel(subset_sum: f64, total_sum: f64) -> Result<f64, WorksheetErrorCode> {
    if !subset_sum.is_finite() || !total_sum.is_finite() {
        return Err(WorksheetErrorCode::Value);
    }
    if total_sum == 0.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    Ok(subset_sum / total_sum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta <= tolerance,
            "expected {expected}, got {actual}, delta {delta}"
        );
    }

    #[test]
    fn bahttext_renders_integer_and_satang_lanes() {
        assert_eq!(
            bahttext_kernel(1234.0).unwrap().to_string_lossy(),
            "หนึ่งพันสองร้อยสามสิบสี่บาทถ้วน"
        );
        assert_eq!(
            bahttext_kernel(1234.56).unwrap().to_string_lossy(),
            "หนึ่งพันสองร้อยสามสิบสี่บาทห้าสิบหกสตางค์"
        );
        assert_eq!(
            bahttext_kernel(21.01).unwrap().to_string_lossy(),
            "ยี่สิบเอ็ดบาทหนึ่งสตางค์"
        );
        // Excel spells negatives with a leading "ลบ" (live Excel 20430, W111-5 G8-07); this
        // assertion used to expect #NUM!.
        assert_eq!(
            bahttext_kernel(-1.0).unwrap().to_string_lossy(),
            "ลบหนึ่งบาทถ้วน"
        );
    }

    #[test]
    fn convert_matches_bounded_catalog_examples() {
        assert_close(
            convert_kernel(1.0, "lbm", "kg").unwrap(),
            0.453_592_37,
            1.0e-12,
        );
        assert_close(convert_kernel(68.0, "F", "C").unwrap(), 20.0, 1.0e-12);
        assert_close(convert_kernel(3.5, "km", "m").unwrap(), 3500.0, 1.0e-9);
        assert_close(convert_kernel(500.0, "ml", "l").unwrap(), 0.5, 1.0e-12);
        assert_eq!(
            convert_kernel(2.5, "ft", "sec"),
            Err(WorksheetErrorCode::NA)
        );
        assert_eq!(
            convert_kernel(1.0, "stone", "kg"),
            Err(WorksheetErrorCode::NA)
        );
    }

    #[test]
    fn convert_v3_graph_matches_banked_refinement_and_publication_bit_pins() {
        // W109 G4-05: discovery/retired-bank, retired-v2, refinement-only v3,
        // and frozen disjoint publication witnesses. The publication rows pin
        // both the first-product and quotient PC64-to-f64 store sites.
        let rows = [
            // Discovery bank: prefix-final staging.
            (0x3ff0_0000_0000_0000, "El", "Gl", 0x41cd_cd65_0000_0000),
            // Retired v1: independently rounded reciprocal pressure factor.
            (0xc3a1_b121_9d59_f352, "atm", "psi", 0xc3e0_3ffe_1b65_b53b),
            // Retired v2's sole miss, later used only as refinement evidence.
            (0x457b_c2d0_0cc5_6eb2, "nm", "Pm", 0x4080_c7cd_ff92_ed2e),
            // Refinement-only cross-pair replay of the exposed mantissa.
            (0x457b_c2d0_0cc5_6eaa, "fm", "Gm", 0x4080_c7cd_ff92_ed29),
            // Frozen publication: first and second length sites.
            (0x410e_b5cd_ddec_4445, "Pm", "um", 0x456a_0335_0818_112b),
            (0xc5ac_d796_2a1d_b89f, "um", "ft", 0xc488_ce45_a4fc_f02a),
            // Frozen publication: first and second mass sites.
            (0x3336_e4da_7fdb_ec42, "ozm", "pg", 0x3602_7255_d603_9ff2),
            (0x3e42_df4c_87b1_8c3c, "Yg", "lbm", 0x42b1_9ef4_6fc2_69d0),
            // Frozen publication: first and second pressure sites.
            (0xbb5c_7a76_8454_e9e2, "atm", "TPa", 0xb9e8_34b7_65b6_e8c4),
            (0x3100_f06d_910e_68f2, "TPa", "atm", 0x3273_edd3_686e_d878),
            // Frozen publication: first and second volume sites.
            (0xc2d4_2d58_985e_b212, "tbs", "gal", 0xc254_2d58_985e_b213),
            (0xc9da_49b8_6a4f_4ebc, "El", "qt", 0xcd98_17ff_2f5c_fdc9),
            // Time has no candidate-disagreement rows, so pin independent
            // all-pair prefix coverage from the same publication check.
            (0x34cd_87bc_9caa_c616, "Esec", "Gsec", 0x36ab_808d_b2b1_b804),
        ];
        for (number_bits, from, to, expected_bits) in rows {
            let actual = convert_kernel(f64::from_bits(number_bits), from, to)
                .unwrap_or_else(|error| panic!("{from}->{to} unexpectedly failed: {error:?}"));
            assert_eq!(actual.to_bits(), expected_bits, "{from}->{to}");
        }
    }

    #[test]
    fn convert_v3_pins_direct_temperature_routes_and_unsupported_bar() {
        // The direct C->F route differs by three ULP from composition through
        // Kelvin for this frozen publication witness.
        let celsius = f64::from_bits(0xbfb8_e95e_2dd6_11af);
        assert_eq!(
            convert_kernel(celsius, "C", "F").unwrap().to_bits(),
            0x403f_d328_bce0_b1e0
        );
        assert_eq!(
            convert_kernel(1.0, "bar", "Pa"),
            Err(WorksheetErrorCode::NA)
        );
        assert_eq!(
            convert_kernel(1.0, "Pa", "bar"),
            Err(WorksheetErrorCode::NA)
        );
    }

    #[test]
    fn euroconvert_supports_default_rounding_and_full_precision() {
        assert_close(
            euroconvert_kernel(10.0, "DEM", "EUR", None, None).unwrap(),
            5.11,
            1.0e-12,
        );
        assert_close(
            euroconvert_kernel(10.0, "DEM", "EUR", Some(true), None).unwrap(),
            10.0 / 1.95583,
            1.0e-12,
        );
        assert_close(
            euroconvert_kernel(10.0, "DEM", "FRF", Some(true), Some(6.0)).unwrap(),
            round_to_significant_digits(10.0 / 1.95583, 6) * 6.55957,
            1.0e-12,
        );
        assert_eq!(
            euroconvert_kernel(10.0, "DEM", "USD", None, None),
            Err(WorksheetErrorCode::NA)
        );
        assert_eq!(
            euroconvert_kernel(10.0, "DEM", "FRF", None, Some(2.0)),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[test]
    fn percentof_maps_ratio_and_div0() {
        assert_close(percentof_kernel(15.0, 60.0).unwrap(), 0.25, 1.0e-12);
        assert_eq!(percentof_kernel(1.0, 0.0), Err(WorksheetErrorCode::Div0));
    }

    /// W111-5 G8-07, live Excel 20430 strings: negatives, amounts under one baht, decimal
    /// half-up satang after 15-digit rounding, and "เอ็ด" across million groups.
    #[test]
    fn bahttext_matches_excel_edge_strings() {
        let t = |v: f64| bahttext_kernel(v).unwrap().to_string_lossy();
        assert_eq!(t(-5.0), "ลบห้าบาทถ้วน");
        assert_eq!(t(-0.001), "ลบศูนย์บาทถ้วน");
        assert_eq!(t(-0.0), "ศูนย์บาทถ้วน");
        assert_eq!(t(0.25), "ยี่สิบห้าสตางค์");
        assert_eq!(t(1.005), "หนึ่งบาทหนึ่งสตางค์");
        assert_eq!(t(2.675), "สองบาทหกสิบแปดสตางค์");
        assert_eq!(t(0.995), "หนึ่งบาทถ้วน");
        assert_eq!(t(9999999999999998.0), "หนึ่งหมื่นล้านล้านบาทถ้วน");
        assert_eq!(t(1000001.0), "หนึ่งล้านเอ็ดบาทถ้วน");
        assert_eq!(t(21.0), "ยี่สิบเอ็ดบาทถ้วน");
    }
}

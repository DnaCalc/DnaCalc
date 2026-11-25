use crate::value::WorksheetErrorCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PercentRankMode {
    Inclusive,
    Exclusive,
}

pub fn percentrank(
    values: &mut [f64],
    x: f64,
    significance: i64,
    mode: PercentRankMode,
) -> Result<f64, WorksheetErrorCode> {
    if values.is_empty() {
        return Err(WorksheetErrorCode::NA);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let first = values[0];
    let last = values[values.len() - 1];
    if x < first || x > last {
        return Err(WorksheetErrorCode::NA);
    }
    // Excel 20527 returns 1 for the only numeric member before checking
    // significance, in both inclusive and exclusive modes.
    if values.len() == 1 {
        return Ok(1.0);
    }
    if significance <= 0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Excel 20527 (dc-j9k.309, .354): 10^significance overflows from 309 places on; both rank
    // modes and the legacy name answer #DIV/0! there, after the range and single-member checks.
    if significance > 308 {
        return Err(WorksheetErrorCode::Div0);
    }

    let base =
        match values.binary_search_by(|v| v.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal)) {
            Ok(idx) => exact_rank(idx, values.len(), mode),
            Err(upper) => {
                if upper == 0 || upper >= values.len() {
                    return Err(WorksheetErrorCode::NA);
                }
                let lower = upper - 1;
                let lower_x = values[lower];
                let upper_x = values[upper];
                let lower_rank = exact_rank(lower, values.len(), mode);
                let upper_rank = exact_rank(upper, values.len(), mode);
                lower_rank + (x - lower_x) * (upper_rank - lower_rank) / (upper_x - lower_x)
            }
        };
    publish_truncated(base, significance)
}

fn exact_rank(index: usize, len: usize, mode: PercentRankMode) -> f64 {
    match mode {
        PercentRankMode::Inclusive => index as f64 / (len as f64 - 1.0),
        PercentRankMode::Exclusive => (index as f64 + 1.0) / (len as f64 + 1.0),
    }
}

/// The rank truncated to `places` decimal places, as Excel publishes it (Excel 20527,
/// dc-j9k.310/.311/.312/.355/.356; 270 of 308 readings at 15 to 200 places on the stored rank, and
/// all at 1 to 14): the rank is read to 15 significant digits (the rounding functions' reading),
/// truncated at `places` decimals, and published as `N * POWER(10, places - k) / POWER(10, places)`
/// for the 15-digit integer `N` with `k` decimals, each operation stored to binary64. `places`
/// counts decimal places, not significant digits (301 of 301 PERCENTRANK readings in the
/// catalogue oracle's sheet dcj9k_statistics; PERCENTRANK.INC(...,5.93) is 0.037).
fn publish_truncated(value: f64, places: i64) -> Result<f64, WorksheetErrorCode> {
    use crate::decimal::{Direction, ExactDecimal};
    if value == 0.0 || !value.is_finite() {
        return Ok(value);
    }
    let places = places.min(308) as i32;
    let reading = ExactDecimal::fifteen_x87(value, true).unwrap_or_else(|| ExactDecimal::fifteen(value).0);
    let mut kept = reading.round_at(-places, Direction::TowardZero).unwrap_or(reading);
    // Pad to the 15 digits the reading carries.
    let length = kept.coefficient.digits().len() as i32;
    if length < 15 && kept.exponent - (15 - length) >= -places {
        kept.coefficient.times_pow10((15 - length) as u32);
        kept.exponent -= 15 - length;
    }
    let digits: f64 = kept.coefficient.digits().parse().unwrap_or(f64::NAN);
    let decimals = -kept.exponent;
    if decimals < 0 || !digits.is_finite() {
        return Ok(value);
    }
    let power = |exponent: i32| crate::ported::power_fn::power_kernel(10.0, f64::from(exponent));
    let numerator = if places > decimals {
        // The integer scaled by the 64-bit decimal chain the rounding functions publish with.
        let scaled = ExactDecimal { negative: false, coefficient: kept.coefficient.clone(), exponent: places - decimals };
        scaled.scaled_pc64_at_exponent().text().parse::<f64>().unwrap_or(f64::NAN)
    } else {
        digits
    };
    Ok(numerator / power(places)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn percentrank_inc_seed_lane() {
        let mut values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(
            percentrank(&mut values, 3.5, 3, PercentRankMode::Inclusive),
            Ok(0.625)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn percentrank_exc_seed_lane() {
        let mut values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(
            percentrank(&mut values, 3.5, 6, PercentRankMode::Exclusive),
            Ok(0.583333)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn percentrank_truncates_default_significance_like_excel() {
        let mut values = vec![1.0, 2.0, 2.0, 4.0, 5.0];
        assert_eq!(
            percentrank(&mut values, 3.5, 3, PercentRankMode::Inclusive),
            Ok(0.687)
        );
    }
}

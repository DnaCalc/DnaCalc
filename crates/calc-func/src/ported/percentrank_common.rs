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
    if significance <= 0 {
        return Err(WorksheetErrorCode::Num);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let first = values[0];
    let last = values[values.len() - 1];
    if x < first || x > last {
        return Err(WorksheetErrorCode::NA);
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
    Ok(truncate_to_decimal_places(base, significance))
}

fn exact_rank(index: usize, len: usize, mode: PercentRankMode) -> f64 {
    match mode {
        PercentRankMode::Inclusive => index as f64 / (len as f64 - 1.0),
        PercentRankMode::Exclusive => (index as f64 + 1.0) / (len as f64 + 1.0),
    }
}

/// The rank truncated to `places` decimal places, trunc(value*10^places)/10^places: Excel's
/// `significance` counts decimal places, not significant digits (dc-j9k: 301 of 301
/// PERCENTRANK, PERCENTRANK.INC and PERCENTRANK.EXC readings in the catalogue oracle's sheet
/// dcj9k_statistics; PERCENTRANK.INC(...,5.93) is 0.037, where three significant digits
/// gave 0.0373).
fn truncate_to_decimal_places(value: f64, places: i64) -> f64 {
    let scale = 10f64.powi(places.min(308) as i32);
    (value * scale).trunc() / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentrank_inc_seed_lane() {
        let mut values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(
            percentrank(&mut values, 3.5, 3, PercentRankMode::Inclusive),
            Ok(0.625)
        );
    }

    #[test]
    fn percentrank_exc_seed_lane() {
        let mut values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(
            percentrank(&mut values, 3.5, 6, PercentRankMode::Exclusive),
            Ok(0.583333)
        );
    }

    #[test]
    fn percentrank_truncates_default_significance_like_excel() {
        let mut values = vec![1.0, 2.0, 2.0, 4.0, 5.0];
        assert_eq!(
            percentrank(&mut values, 3.5, 3, PercentRankMode::Inclusive),
            Ok(0.687)
        );
    }
}

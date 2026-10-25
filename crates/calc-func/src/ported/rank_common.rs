use crate::value::WorksheetErrorCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankOrder {
    Descending,
    Ascending,
}

pub fn rank_eq(number: f64, values: &[f64], order: RankOrder) -> Result<f64, WorksheetErrorCode> {
    if values.is_empty() {
        return Err(WorksheetErrorCode::NA);
    }
    let matches = values.iter().filter(|value| **value == number).count();
    if matches == 0 {
        return Err(WorksheetErrorCode::NA);
    }

    let rank = match order {
        RankOrder::Descending => values.iter().filter(|value| **value > number).count() + 1,
        RankOrder::Ascending => values.iter().filter(|value| **value < number).count() + 1,
    };
    Ok(rank as f64)
}

pub fn rank_avg(number: f64, values: &[f64], order: RankOrder) -> Result<f64, WorksheetErrorCode> {
    let first = rank_eq(number, values, order)?;
    let matches = values.iter().filter(|value| **value == number).count() as f64;
    Ok(first + (matches - 1.0) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rank_eq_descending_basic_lane() {
        let values = vec![30.0, 20.0, 20.0, 10.0];
        let got = rank_eq(20.0, &values, RankOrder::Descending).unwrap();
        assert_eq!(got, 2.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rank_avg_descending_duplicate_lane() {
        let values = vec![30.0, 20.0, 20.0, 10.0];
        let got = rank_avg(20.0, &values, RankOrder::Descending).unwrap();
        assert_eq!(got, 2.5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rank_eq_ascending_basic_lane() {
        let values = vec![30.0, 20.0, 20.0, 10.0];
        let got = rank_eq(20.0, &values, RankOrder::Ascending).unwrap();
        assert_eq!(got, 2.0);
    }
}

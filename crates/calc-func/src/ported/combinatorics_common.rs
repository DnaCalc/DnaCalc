use crate::value::WorksheetErrorCode;

pub fn combinations_of_int(n: i64, k: i64) -> Result<f64, WorksheetErrorCode> {
    if n < 0 || k < 0 || n < k {
        return Err(WorksheetErrorCode::Num);
    }
    let k = k.min(n - k);
    let mut acc = 1.0;
    for i in 1..=k {
        acc *= (n - k + i) as f64;
        acc /= i as f64;
    }
    Ok(acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinations_of_int_matches_basic_lanes() {
        assert_eq!(combinations_of_int(5, 2), Ok(10.0));
        assert_eq!(combinations_of_int(0, 0), Ok(1.0));
        assert_eq!(combinations_of_int(5, 6), Err(WorksheetErrorCode::Num));
    }
}

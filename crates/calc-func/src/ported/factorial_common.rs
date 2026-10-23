use crate::value::WorksheetErrorCode;

pub fn trunc_nonnegative_or_minus_one(n: f64) -> Result<i64, WorksheetErrorCode> {
    if n < -1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(n.trunc() as i64)
}

pub fn trunc_nonnegative(n: f64) -> Result<i64, WorksheetErrorCode> {
    if n < 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(n.trunc() as i64)
}

pub fn factorial_of_int(n: i64) -> f64 {
    if n <= 1 {
        return 1.0;
    }
    (2..=n).fold(1.0, |acc, item| acc * item as f64)
}

pub fn double_factorial_of_int(n: i64) -> f64 {
    // Live Excel 16.0 b20326: FACTDOUBLE is the reverse native odd/even
    // product n*(n-2)*... (not the forward fold). Forward diverges by n=50.
    if n <= 1 {
        return 1.0;
    }
    let mut acc = 1.0;
    let mut k = n;
    while k >= 2 {
        acc *= k as f64;
        k -= 2;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trunc_nonnegative_or_minus_one_allows_minus_one_and_zero() {
        assert_eq!(trunc_nonnegative_or_minus_one(-1.0), Ok(-1));
        assert_eq!(trunc_nonnegative_or_minus_one(-0.1), Ok(0));
    }

    #[test]
    fn trunc_nonnegative_or_minus_one_rejects_below_minus_one() {
        assert_eq!(
            trunc_nonnegative_or_minus_one(-1.1),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            trunc_nonnegative_or_minus_one(-2.0),
            Err(WorksheetErrorCode::Num)
        );
    }

    #[test]
    fn trunc_nonnegative_rejects_negative_and_truncates_positive() {
        assert_eq!(trunc_nonnegative(5.9), Ok(5));
        assert_eq!(trunc_nonnegative(-0.1), Err(WorksheetErrorCode::Num));
    }
}

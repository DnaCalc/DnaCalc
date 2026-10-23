use crate::value::WorksheetErrorCode;

pub fn log10_kernel(n: f64) -> Result<f64, WorksheetErrorCode> {
    if n <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Excel's LOG10 is the x87 `fldlg2` + `fyl2x` fused op; match it bit-exact.
    Ok(crate::ported::excel_numeric::excel_log10(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log10_kernel_zero_is_num() {
        assert_eq!(log10_kernel(0.0), Err(WorksheetErrorCode::Num));
    }
}

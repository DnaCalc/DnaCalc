use crate::value::WorksheetErrorCode;

pub fn fisher_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    if x.abs() >= 1.0 {
        return Err(WorksheetErrorCode::Num);
    }
    // Inverse-problem identity, live Excel 16.0 b20228: FISHER(x) ==
    // 0.5*LN((1+x)/(1-x)) on 33/33 signed rows including tiny and near-1.
    // FISHER is not ATANH (21/33): ATANH keeps a cubic small-x body.
    // Split LN(1+x)-LN(1-x) is not the graph (10/33).
    // W111's small-input neighbors distinguish all three arithmetic stores.
    let numerator = crate::ported::excel_numeric::excel_x87_add(1.0, x);
    let denominator = crate::ported::excel_numeric::excel_x87_sub(1.0, x);
    let ratio = crate::ported::excel_numeric::excel_x87_div(numerator, denominator);
    Ok(0.5 * crate::ported::excel_numeric::excel_log(ratio))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fisher_follows_worksheet_ln_ratio_identity() {
        for x in [0.0, 1e-8, 0.1, 0.5, -0.5, 0.9] {
            let got = fisher_kernel(x).unwrap();
            let expect = 0.5 * crate::ported::excel_numeric::excel_log((1.0 + x) / (1.0 - x));
            assert_eq!(got.to_bits(), expect.to_bits(), "x={x}");
        }
        assert_eq!(fisher_kernel(1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(fisher_kernel(-1.0), Err(WorksheetErrorCode::Num));
    }
}

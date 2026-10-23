use crate::value::WorksheetErrorCode;
pub fn log_kernel(number: f64, base: f64) -> Result<f64, WorksheetErrorCode> {
    if number <= 0.0 || base <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }
    if base == 1.0 {
        return Err(WorksheetErrorCode::Div0);
    }
    // The observed explicit-base LOG graph is a staged quotient of two stored logarithms.
    // (both `ln`s are the x87 backend). This was confirmed by a live-Excel sweep
    // of 218 rows across bases 2, 10, and arbitrary — all bit-exact, including
    // Excel's own imprecision (e.g. LOG(1000,10) = 2.9999999999999996, NOT 3).
    // Do NOT special-case base 2 or 10 to a dedicated log2/log10: Excel's
    // dedicated LOG10() worksheet function returns an exact 3 for LOG10(1000),
    // but LOG(1000,10) differs. W111's paired capture also distinguishes omitted
    // LOG(1000), which agrees with LOG10 and is selected in the surface below.
    let value = crate::ported::excel_numeric::excel_x87_div(
        crate::ported::excel_numeric::excel_log(number),
        crate::ported::excel_numeric::excel_log(base),
    );
    // W111: LOG(1, base<1) publishes positive zero. The neighboring-one
    // discriminator also distinguishes the staged divide from native division.
    Ok(if value == 0.0 { 0.0 } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_kernel_seed_lanes_match_excel_probe() {
        assert_eq!(log_kernel(8.0, 2.0), Ok(3.0));
        assert_eq!(log_kernel(8.0, 1.0), Err(WorksheetErrorCode::Div0));
        assert_eq!(log_kernel(8.0, -2.0), Err(WorksheetErrorCode::Num));
    }

    // W108 Phase-A: LOG(x, base) = ln(x)/ln(base) for EVERY base (x87 ln),
    // pinned to live Excel 16.0 build 20131 (Value2 cell-ref plumbing). The
    // decisive rows are LOG(1000,10) and LOG(0.1,10): Excel's dedicated LOG10()
    // returns an exact 3 / -1, but LOG(_,10) is a different code path that
    // returns the imprecise ln/ln value — so LOG must NOT use a dedicated log10.
    #[test]
    fn log_kernel_pins_live_excel_ln_over_ln_all_bases() {
        // base 10 — the "not-log10" witnesses (imprecise, and that IS Excel).
        assert_eq!(
            log_kernel(1000.0, 10.0),
            Ok(f64::from_bits(0x4007ffffffffffff))
        ); // 2.9999999999999996
        assert_eq!(
            log_kernel(0.1, 10.0),
            Ok(f64::from_bits(0xbfeffffffffffffe))
        ); // -0.9999999999999998
        assert_eq!(log_kernel(100.0, 10.0), Ok(2.0));
        // base 2 — powers of two stay exact (log2 is exact there).
        assert_eq!(log_kernel(8.0, 2.0), Ok(3.0));
        assert_eq!(log_kernel(0.5, 2.0), Ok(-1.0));
        // arbitrary base 3 — a non-round witness.
        assert_eq!(
            log_kernel(
                f64::from_bits(0x41090f848340ac40),
                f64::from_bits(0x4008000000000000)
            ),
            Ok(f64::from_bits(0x402644badd30854a))
        );
    }
}

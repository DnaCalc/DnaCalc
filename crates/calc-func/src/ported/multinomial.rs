use crate::ported::excel_numeric::excel_log10;
use crate::ported::power_fn::power_kernel;
use crate::value::WorksheetErrorCode;

/// `MULTINOMIAL` kernel (dc-iuwct.47 / cb4, Excel 16.0.20527):
/// accumulates `excel_log10(2..=total)`, subtracts `excel_log10(2..=item)` in
/// reverse flattened argument order, and stages `POWER(10.0, acc)` through
/// [`power_kernel`].
pub fn multinomial_kernel(items: &[i64]) -> Result<f64, WorksheetErrorCode> {
    // Excel rejects a factorial numerator above 170!, even when cancellation
    // would leave a finite multinomial (including a single positive argument).
    // Bound the total before either loop or integer summation.
    let mut total = 0i64;
    for &item in items {
        if item < 0 || item > 170 - total {
            return Err(WorksheetErrorCode::Num);
        }
        total += item;
    }

    let mut acc = 0.0_f64;
    for k in 2..=total {
        acc += excel_log10(k as f64);
    }
    for &item in items.iter().rev() {
        for k in 2..=item {
            acc -= excel_log10(k as f64);
        }
    }
    power_kernel(10.0, acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bits(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} (0x{:016x}) vs {expected} (0x{:016x})",
            actual.to_bits(),
            expected.to_bits()
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn multinomial_kernel_matches_exact_control_rows() {
        // Live Excel 16.0.20527 words (verified via Touchstone).
        for (items, expected) in [
            (&[1, 2][..], f64::from_bits(0x4008_0000_0000_0001)),
            (&[1, 3][..], 4.0_f64),
            (&[1, 2, 3][..], f64::from_bits(0x404d_ffff_ffff_fff6)),
            (&[1, 1, 1, 1][..], f64::from_bits(0x4038_0000_0000_0001)),
            (&[5][..], f64::from_bits(0x3fef_ffff_ffff_fffc)),
            (&[0, 0][..], 1.0_f64),
        ] {
            let actual = multinomial_kernel(items).expect("multinomial exact control");
            assert_bits(actual, expected);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn multinomial_exactness_witness_matches_excel_target() {
        let actual = multinomial_kernel(&[2, 3, 4]).expect("multinomial witness");
        let prior_local = 1260.0_f64;
        let excel_target = 1259.9999999999991_f64;

        assert_bits(actual, excel_target);
        assert_ne!(actual.to_bits(), prior_local.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn multinomial_widened_empirical_machine_witness_rows_match_excel_targets() {
        let anchor_target = 1259.9999999999991_f64;
        for (items, expected) in [
            (&[2, 3, 4][..], anchor_target),
            (&[4, 2, 3][..], anchor_target),
            (&[0, 2, 3, 4][..], anchor_target),
            (&[0, 2, 3][..], 9.999999999999998_f64),
            (&[2, 7][..], 35.99999999999998_f64),
            (&[7, 2][..], 35.99999999999998_f64),
            (&[1, 2, 3, 4][..], 12599.999999999995_f64),
            (&[2, 3, 4, 5][..], 2522520.0000000005_f64),
        ] {
            let actual = multinomial_kernel(items).expect("multinomial empirical witness");
            assert_bits(actual, expected);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn multinomial_ordering_and_zero_padding_invariance_hold_for_widened_rows() {
        let anchor = multinomial_kernel(&[2, 3, 4]).expect("anchor");
        let four_arg = multinomial_kernel(&[1, 2, 3, 4]).expect("four arg");
        let two_positive = multinomial_kernel(&[2, 7]).expect("two positive");

        assert_bits(
            multinomial_kernel(&[3, 4, 2]).expect("anchor permutation"),
            anchor,
        );
        assert_bits(
            multinomial_kernel(&[0, 2, 3, 4]).expect("anchor zero padded"),
            anchor,
        );
        assert_bits(
            multinomial_kernel(&[4, 3, 2, 1]).expect("four arg permutation"),
            four_arg,
        );
        assert_bits(
            multinomial_kernel(&[0, 7, 2]).expect("two positive zero padded"),
            two_positive,
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn multinomial_reverse_denominator_order_matches_excel_asymmetry() {
        // Live Excel 16.0.20527 (cb4): reverse denominator subtraction order
        // distinguishes MULTINOMIAL(8, 100) from MULTINOMIAL(100, 8).
        assert_bits(
            multinomial_kernel(&[8, 100]).expect("8, 100"),
            f64::from_bits(0x4254_7d97_34ae_c006),
        );
        assert_bits(
            multinomial_kernel(&[100, 8]).expect("100, 8"),
            f64::from_bits(0x4254_7d97_34ae_c02f),
        );
    }
}

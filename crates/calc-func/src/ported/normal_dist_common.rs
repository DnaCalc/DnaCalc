use crate::value::WorksheetErrorCode;
pub const SQRT_2PI: f64 = 2.506_628_274_631_000_7;

/// `RN(1/sqrt(2π))` — PHI multiplies by this reciprocal constant (the
/// divide-by-`SQRT_2PI` staging is ruled out; live PHI(0) publishes exactly
/// these bits).
const INV_SQRT_2PI: f64 = f64::from_bits(0x3fd9884533d43651);

/// Worksheet `PHI` numerical graph observed on the reference Excel host.
/// W111 retains a fresh 9,130-row exact numerical replay; surface overflow
/// admission and contextual coercion remain separate obligations.
///
/// ```text
/// sq  = RN53(RN64(x·x))            (x87 double-rounded square)
/// t   = -(sq/2)                    (exact halving + negation)
/// e   = EXP(t)                     (the x87 exponential, `excel_exp`)
/// v   = RN53(RN64(e · RN(1/sqrt(2π))))
/// PHI = +0.0 if |v| < DBL_MIN      (live-pinned publication flush;
///        normal values publish unchanged — boundary probed at bit level)
/// ```
pub fn phi_kernel(x: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_exp, excel_x87_mul};
    let sq = excel_x87_mul(x, x);
    let e = excel_exp(-(sq / 2.0));
    let v = excel_x87_mul(e, INV_SQRT_2PI);
    if v.abs() < f64::MIN_POSITIVE { 0.0 } else { v }
}

pub fn erf_approx(x: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul};
    const P0: f64 = f64::from_bits(0x3ff1_df5e_70c7_956b);
    const Q0: f64 = f64::from_bits(0x3fef_ada1_5778_ece6);
    if x.abs() < 1.0e-8 {
        // In this measured linear regime, the recorded answers distinguish two
        // PC64 operations with binary64 stores: (x * numerator) / denominator.
        // The global coefficient pair was identified from arithmetic constraints
        // and checked against all 248,904 recorded tiny ERF.PRECISE inputs.
        // Both stores matter; direct binary64 multiply or divide loses matches.
        return excel_x87_div(excel_x87_mul(x, P0), Q0);
    }
    if x.abs() < 1.0e-2 {
        // dc-j9k.6: extend the same rational model in t=x*x. The coefficient
        // constraints come from erf(x)/x = (2/sqrt(pi))*(1-t/3+t*t/10-t*t*t/42+...),
        // DLMF 7.6.1, https://dlmf.nist.gov/7.6.E1. Recorded rounding intervals
        // constrain Q1/Q2/Q3; P1/P2/P3 follow those Taylor constraints. This is one
        // surviving coefficient set, not a unique identification of Excel's code.
        // All 612,353 inputs below this cutoff in the original calibration bank
        // match. Independent build-20527 readings matched 3,908/4,096; those
        // fresh answers still expose 188 misses. The cutoff bounds the fitted
        // model, not an identified Excel split.
        // The two outer additions require PC64-to-binary64 stores too.
        const P1: f64 = f64::from_bits(0x3fc2_01d8_627a_8c9c);
        const P2: f64 = f64::from_bits(0x3fa6_f28a_1efb_4082);
        const P3: f64 = f64::from_bits(0x3f5b_4fa7_6abc_0af4);
        const Q1: f64 = f64::from_bits(0x3fdd_191a_ca8d_2b6f);
        const Q2: f64 = f64::from_bits(0x3fb7_9f85_dcc9_753c);
        const Q3: f64 = f64::from_bits(0x3f85_2da6_0d43_50b1);
        let t = x * x;
        let numerator = excel_x87_add(P0, (P1 + (P2 + P3 * t) * t) * t);
        let denominator = excel_x87_add(Q0, (Q1 + (Q2 + Q3 * t) * t) * t);
        return excel_x87_div(excel_x87_mul(x, numerator), denominator);
    }
    libm::erf(x)
}

/// Binary64 reciprocal-square-root constant used by the normal wrappers.
pub const FRAC_1_SQRT_2_BITS: u64 = 0x3fe6a09e667f3bcd;

/// Inclusive tiny-direct GAUSS predicate: `abs(x) <= 1e-15` is the odd
/// stored-z route; the successor double is the ordinary G-F3 wrapper.
pub const GAUSS_TINY_MAX_BITS: u64 = 0x3cd203af9ee75616;

/// Native-multiply stored-z route for the distinct tiny GAUSS branch.
/// W111 independently distinguishes the ordinary normal-CDF wrapper's staged
/// RN64 multiply followed by a binary64 store; that wrapper does not call this.
pub fn stored_normal_z(x: f64) -> f64 {
    x.abs() * f64::from_bits(FRAC_1_SQRT_2_BITS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W109 PHI identification pins — live Excel 16.0 build 20131 (unique
    /// surviving staging; 764/764 answered rows replay via
    /// `verify_phi_promotion`).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phi_matches_live_excel_pinned_witnesses() {
        // PHI(0) publishes the reciprocal constant bits exactly.
        assert_eq!(phi_kernel(0.0).to_bits(), 0x3fd9884533d43651);
        // Subnormal-flush boundary (bit-pinned): x=37.6157 still publishes a
        // normal value; the next band flushes to +0.
        let x_norm = 37.6157f64;
        assert!(phi_kernel(x_norm) >= f64::MIN_POSITIVE);
        assert_eq!(phi_kernel(37.8511319f64), 0.0);
        assert_eq!(phi_kernel(39.0), 0.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phi_matches_excel_probe_lanes() {
        assert!((phi_kernel(0.0) - 0.398942280401433).abs() < 1e-12);
        assert!((phi_kernel(1.0) - 0.241970724519143).abs() < 1e-12);
    }
}

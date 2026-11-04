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

/// Constant term `N(0)` of the numerator of Excel's small-argument erf rational
/// `x * N(t) / D(t)`, `t = x*x`, as a binary64 (measured from recorded answers; see
/// [`erf_approx`]). `N(0) / D(0)` is `2/sqrt(pi)` to binary64 accuracy. The published table
/// these come from is not identified: Cody's 1969 table (SPECFUN CALERF) has the same
/// constant-term ratio, as every accurate table must, but its normalised linear terms differ
/// from Excel's.
const ERF_TINY_P0: f64 = f64::from_bits(0x3ff1_df5e_70c7_956b);
/// Constant term `D(0)` of the denominator, as a binary64.
const ERF_TINY_Q0: f64 = f64::from_bits(0x3fef_ada1_5778_ece6);
/// `2^-27`. Below it `t < 2^-54`. Excel stores `N(t)` and `D(t)` to binary64 before using
/// them, and their linear terms (measured: about `0.1407 t` and `0.4547 t`) are then below
/// half a unit in the last place of `N(0)` (`2^-53`) and of `D(0)` (`2^-54`), so the stored
/// values are exactly `N(0)` and `D(0)`, with a margin: any linear terms with `|N'(0)| < 1`
/// and `|D'(0)| < 1/2` give the same bound.
const ERF_TINY_MAX: f64 = f64::from_bits(0x3e40_0000_0000_0000);

/// The shared ERF body (ERF, ERF.PRECISE, the below-half ERFC complement and GAUSS's tiny
/// route).
///
/// Excel evaluates `|x| <= 0.5` as `x * N(t) / D(t)` on the x87 at precision 64, with the two
/// polynomials stored to binary64, then the product stored, then the quotient stored:
///
/// ```text
/// p = RN53(RN64(x * N))
/// y = RN53(RN64(p / D))
/// ```
///
/// Below `2^-27`, `N` and `D` are their constant terms ([`ERF_TINY_MAX`]). This reproduces
/// all 249,514 recorded ERF.PRECISE answers with `0 < |x| < 2^-27`; the comb that no single
/// constant fits is the product stored before the divide. The higher coefficients are not
/// identified yet, so `|x| >= 2^-27` stays on `libm::erf`; subnormal arguments have no
/// recording and also stay there.
pub fn erf_approx(x: f64) -> f64 {
    if x.is_normal() && x.abs() < ERF_TINY_MAX {
        use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul};
        return excel_x87_div(excel_x87_mul(x, ERF_TINY_P0), ERF_TINY_Q0);
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

    /// Recorded Excel `ERF.PRECISE` answers below `2^-27` (`OxFunc-Evidence`: the build-20131
    /// oracle cache, `readings/excel-20527/ERF.PRECISE.json`, W109 banks b16, b17 and
    /// b26-zcross). Each pair is `(argument bits, Excel result bits)`. The first group is
    /// mismatched by `libm::erf`; the second already matched and must stay matched.
    const ERF_TINY_RECORDED: [(u64, u64); 26] = [
        // libm::erf misses these by one unit in the last place.
        (0x3d70000000000000, 0x3d720dd750429b6e),
        (0x3d7cfde8d4cc4149, 0x3d805b5d30f4cc06),
        (0x3d7ab28000000000, 0x3d7e1fe84c83a3a8),
        (0x3d735c41b35a3054, 0x3d75d88967f7ead4),
        (0xbd768d3d87b932e7, 0xbd79726775b984dd),
        (0x3d966c42d2efd513, 0x3d994d30e2d5cb8e),
        (0xbdb06772d52c3fe8, 0xbdb28291fd7f949a),
        (0x3ddba00000000000, 0x3ddf2be5c093005b),
        (0x3df9e0d03f41c4b8, 0x3dfd334d2f1b80bd),
        (0x3e13bb4000000000, 0x3e1643b9ab266415),
        (0x3e1fee0a3d70a456, 0x3e2203b54e22aea5),
        (0x3e20000000000000, 0x3e220dd750429b6e),
        (0xbe223ab45be40eee, 0xbe2491cfea3435ea),
        (0x3e30000000000001, 0x3e320dd750429b6f),
        // libm::erf already matches these.
        (0x3d7024d3d9e093a8, 0x3d7237658117c44d),
        (0x3d7d758000000000, 0x3d809ed5f357116e),
        (0xbd7a3e0fafbe3c36, 0xbd7d9c85380b2536),
        (0x3d92ac4fd88c57a8, 0x3d9512011d9a0deb),
        (0x3ddde00000000000, 0x3de0daec03ee2f1b),
        (0xbdf3f40396a8a23e, 0xbdf683c6cf6f7a58),
        (0x3e14a4f4d4a539bc, 0x3e174b6f464f49c3),
        (0x3e1fec1eb851eba4, 0x3e22029ffea81b13),
        (0x3e2c000000000000, 0x3e2f9838cc748fff),
        (0x3e20079999999c92, 0x3e22166ab0225804),
        (0xbe2f490522e27e6c, 0xbe31a69b0ec7c0e8),
        (0x3e30d00000000000, 0x3e32f88b3f55fd52),
    ];

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn erf_tiny_route_reproduces_recorded_excel_answers() {
        for (x, excel) in ERF_TINY_RECORDED {
            let actual = erf_approx(f64::from_bits(x)).to_bits();
            assert_eq!(actual, excel, "ERF.PRECISE({x:#018x}): calculated {actual:#018x}, Excel {excel:#018x}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn erf_tiny_constant_ratio_is_two_over_root_pi() {
        // RN(2/sqrt(pi)) = 0x3ff20dd750429b6d; the stored pair's ratio is within an ulp of it.
        let ratio = ERF_TINY_P0 / ERF_TINY_Q0;
        assert!((ratio - f64::from_bits(0x3ff2_0dd7_5042_9b6d)).abs() <= f64::EPSILON * 2.0);
        // Below the cut, t < 2^-54: a linear term under 1*t stays below half an ulp of P0
        // (2^-53) and one under t/2 below half an ulp of Q0 (2^-54).
        let t_max = ERF_TINY_MAX * ERF_TINY_MAX;
        assert_eq!(t_max, 2f64.powi(-54));
        assert_eq!(ERF_TINY_P0.to_bits() >> 52, 0x3ff);
        assert_eq!(ERF_TINY_Q0.to_bits() >> 52, 0x3fe);
        assert!(t_max < 2f64.powi(-53) && 0.5 * t_max < 2f64.powi(-54));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phi_matches_excel_probe_lanes() {
        assert!((phi_kernel(0.0) - 0.398942280401433).abs() < 1e-12);
        assert!((phi_kernel(1.0) - 0.241970724519143).abs() < 1e-12);
    }
}

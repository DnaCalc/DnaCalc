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
/// `x * N(t) / D(t)`, `t = x*x`. Measured from recorded answers, as are the three constants
/// below (see [`erf_approx`]). The published table is not identified: Cody's 1969 table
/// (SPECFUN CALERF) shares only the constant-term ratio, as every accurate table must.
const ERF_N0: f64 = f64::from_bits(0x3ff1_df5e_70c7_956b);
/// Constant term `D(0)` of the denominator. `N(0) / D(0)` is `2/sqrt(pi)` to binary64 accuracy.
const ERF_D0: f64 = f64::from_bits(0x3fef_ada1_5778_ece6);
/// Linear coefficient of `N`: the only binary64 inside the bracket of the edge where stored `N`
/// first leaves `N(0)` (Excel, between `x = 0x3e5e280f6c1e72dc` and its successor).
const ERF_N1: f64 = f64::from_bits(0x3fc2_01d8_627a_7d2d);
/// Linear coefficient of `D`, from the edge where stored `D` first leaves `D(0)` (Excel, between
/// `x = 0x3e47bc15e9263272` and its successor); this bracket holds this binary64 and the next,
/// and no recorded answer tells them apart.
const ERF_D1: f64 = f64::from_bits(0x3fdd_191a_ca8d_2496);
/// `2^-16`. Below it `t^2 < 2^-64`, which is under half a unit of the 64-bit register for `N`
/// and about that for `D`, so the terms past the linear ones (not measured yet) are invisible.
const ERF_LINEAR_MAX: f64 = f64::from_bits(0x3ef0_0000_0000_0000);

/// The shared ERF body (ERF, ERF.PRECISE, the below-half ERFC complement and GAUSS's tiny
/// route).
///
/// Excel evaluates small arguments as `x * N(t) / D(t)` on the x87 at precision 64. Both
/// polynomials are stored to binary64, then the product is stored, then the quotient:
///
/// ```text
/// N = RN53(RN64(N(t)))     D = RN53(RN64(D(t)))
/// p = RN53(RN64(x * N))    y = RN53(RN64(p / D))
/// ```
///
/// The stores are shown by Excel directly: the stored `D` steps at sharp edges that bisection
/// found to one input ulp. The two `D` edges measured place `D'(0)` in brackets that agree to
/// 1e-15 only when each edge carries the 64-bit register's half unit, which is the double
/// rounding above. The measured coefficients give `N'(0)/N(0) - D'(0)/D(0) = -1/3` to 2e-16,
/// the Taylor coefficient of erf, which nothing in the measurement imposed.
///
/// Below `2^-16` this reproduces all 499,814 recorded ERF.PRECISE answers with nonzero
/// argument. Above it the higher coefficients are not measured, so the body stays on
/// `libm::erf`; subnormal arguments cannot be written to a cell (Excel reads them back as
/// zero) and also stay there.
pub fn erf_approx(x: f64) -> f64 {
    if x.is_normal() && x.abs() < ERF_LINEAR_MAX {
        use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul, excel_x87_mul_add};
        let t = excel_x87_mul(x, x);
        let numerator = excel_x87_mul_add(ERF_N1, t, ERF_N0);
        let denominator = excel_x87_mul_add(ERF_D1, t, ERF_D0);
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

    /// Recorded Excel `ERF.PRECISE` answers below `2^-16` (`OxFunc-Evidence`: the build-20131
    /// oracle cache, `readings/excel-20527/ERF.PRECISE.json`, W109 banks b16, b17 and
    /// b26-zcross). Each pair is `(argument bits, Excel result bits)`. The first group is
    /// mismatched by `libm::erf`; the second already matched and must stay matched.
    const ERF_TINY_RECORDED: [(u64, u64); 29] = [
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
        (0x3e80000000000000, 0x3e820dd750429b56),
        (0x3ee0000000000000, 0x3ee20dd750411a47),
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
        (0x3e70000000000000, 0x3e720dd750429b67),
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
    fn erf_measured_coefficients_reproduce_the_taylor_series() {
        // N(0)/D(0) = 2/sqrt(pi) and N'(0)/N(0) - D'(0)/D(0) = -1/3: erf(x)/x's first two Taylor
        // coefficients, which the measurements did not impose.
        let ratio = ERF_N0 / ERF_D0;
        assert!((ratio - f64::from_bits(0x3ff2_0dd7_5042_9b6d)).abs() <= 2.0 * f64::EPSILON);
        assert!((ERF_N1 / ERF_N0 - ERF_D1 / ERF_D0 + 1.0 / 3.0).abs() < 1e-15);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phi_matches_excel_probe_lanes() {
        assert!((phi_kernel(0.0) - 0.398942280401433).abs() < 1e-12);
        assert!((phi_kernel(1.0) - 0.241970724519143).abs() < 1e-12);
    }
}

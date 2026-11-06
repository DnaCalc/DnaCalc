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

/// `N(t)` of Excel's erf rational `x * N(t) / D(t)`, `t = x*x`, lowest power first. Measured from
/// Excel's answers (see [`erf_approx`]); the published table is not identified. Orders 0 and 1 are
/// exact. Orders 2 and above were read from runs of 20,000 consecutive doubles at a dozen sites,
/// so the last bits of order 3 and above can be off, which shows only above 0.25.
const ERF_N: [u64; 5] = [
    0x3ff1_df5e_70c7_956b,
    0x3fc2_01d8_627a_7d2c,
    0x3fa6_f289_d165_b303,
    0x3f5d_93a5_813e_372c,
    0x3f29_3276_7df2_64b2,
];
/// `D(t)`, lowest power first, measured as [`ERF_N`]. The degrees (4, 5) are measured: a fifth
/// term of `N` and a sixth of `D` fit to zero.
const ERF_D: [u64; 6] = [
    0x3fef_ada1_5778_ece6,
    0x3fdd_191a_ca8d_2499,
    0x3fb7_9f85_ba68_803e,
    0x3f85_6de5_da86_af12,
    0x3f46_0975_1b5b_6488,
    0x3ef4_f20a_473a_5ab0,
];

/// One Horner evaluation with every product and sum stored to binary64, as `erf_approx` needs.
fn stored_horner(coefficients: &[u64], t: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul};
    let mut value = 0.0;
    for (i, &bits) in coefficients.iter().enumerate().rev() {
        let c = f64::from_bits(bits);
        value = if i + 1 == coefficients.len() { c } else { excel_x87_add(excel_x87_mul(value, t), c) };
    }
    value
}

/// The shared ERF body (ERF, ERF.PRECISE, the below-half ERFC complement and GAUSS's tiny
/// route).
///
/// For `|x| < 0.5` Excel evaluates a (4, 5) rational `x * N(t) / D(t)` as compiled double
/// arithmetic on the x87 at precision 64: every operation is `RN53(RN64(..))`, that is, its
/// result is stored to binary64 before the next one reads it, in the order
///
/// ```text
/// t = x*x;  N = Horner(t);  D = Horner(t);  p = x*N;  y = p/D
/// ```
///
/// Excel shows this directly. Stored `N` and `D` step at edges found by bisection to one input
/// ulp, and the two `D` edges measured agree to 1e-15 only with the 64-bit register's half unit on
/// each. Of the store placements raced on 2,000 consecutive answers at each of about 20 sites up
/// to 0.5, all stores leaves 0.5% of the rows unexplained (the coefficients' last bits) and every
/// other placement more than twice that. The measured coefficients give `N1/N0 - D1/D0 = -1/3`
/// to 2e-16 and reproduce erf to a flat offset of 9e-17 up to 0.5, past which the error climbs at
/// once, which places the approximation's design interval.
///
/// Above 0.5 the magnitude is the stored-PC64 complement of the upper ERFC body. Independent
/// public calls agree on 3,328 adjacent inputs; exponent-edge controls distinguish the final
/// PC64 store from strict binary64 subtraction. ERFC's `F` remains a stand-in, so this measured
/// composition does not establish its last bits. Exactly 0.5 keeps its existing boundary body.
/// Subnormal arguments cannot be written to a cell (Excel reads them back as zero).
pub fn erf_approx(x: f64) -> f64 {
    if x.is_normal() && x.abs() < 0.5 {
        use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul};
        let t = excel_x87_mul(x, x);
        let numerator = stored_horner(&ERF_N, t);
        let denominator = stored_horner(&ERF_D, t);
        return excel_x87_div(excel_x87_mul(x, numerator), denominator);
    }
    if x.abs() > 0.5 {
        use crate::ported::excel_numeric::excel_x87_sub;
        return excel_x87_sub(1.0, erfc_upper(x.abs())).copysign(x);
    }
    libm::erf(x)
}

/// Cody's erfc rationals (Math. Comp. 23 (1969); the SPECFUN CALERF tables) for `x*exp(x*x)*erfc(x)`
/// and `exp(x*x)*erfc(x)`. They stand in for Excel's own `F` in [`erfc_upper`], which is not
/// identified: Excel's noise and steps show a different rational.
const ERFC_C: [f64; 9] = [
    5.64188496988670089e-1, 8.88314979438837594e0, 6.61191906371416295e1, 2.98635138197400131e2,
    8.81952221241769090e2, 1.71204761263407058e3, 2.05107837782607147e3, 1.23033935479799725e3,
    2.15311535474403846e-8,
];
const ERFC_D: [f64; 8] = [
    1.57449261107098347e1, 1.17693950891312499e2, 5.37181101862009858e2, 1.62138957456669019e3,
    3.29079923573345963e3, 4.36261909014324716e3, 3.43936767414372164e3, 1.23033935480374942e3,
];
const ERFC_P: [f64; 6] = [
    3.05326634961232344e-1, 3.60344899949804439e-1, 1.25781726111229246e-1, 1.60837851487422766e-2,
    6.58749161529837803e-4, 1.63153871373020978e-2,
];
const ERFC_Q: [f64; 5] = [
    2.56852019228982242e0, 1.87295284992346725e0, 5.27905102951428412e-1, 6.05183413124413191e-2,
    2.33520497626869185e-3,
];
/// `1/sqrt(pi)`.
const ERFC_SQRPI: f64 = 5.6418958354775628695e-1;

/// The upper branch of Excel's erfc for `x >= 0.5`: `Q = E * F`.
///
/// `E` is measured. `z = RN53(RN64(x*x))` and `E = exp(-z)` are stored to binary64 before use: over
/// 220,000 Excel answers up to `x = 26.5` the residual of `Q` against this `E` times the true
/// `exp(x*x)*erfc(x)` stays within about 10 units of 2^-53, where the exact `exp(-x*x)` spreads by
/// hundreds of units. `z`'s double rounding shows directly: plain binary64 `x*x` leaves outliers of
/// up to 1,024 units.
///
/// `F` is a stand-in with the same store discipline. Excel's own `F` is not identified: its
/// noise lines are those of two degree-8 polynomials in `x`, close in shape to Cephes'. Last bits
/// differ until it is.
pub fn erfc_upper(x: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_exp, excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub};
    let step = |value: f64, c: f64, by: f64| excel_x87_mul(excel_x87_add(value, c), by);
    let z = excel_x87_mul(x, x);
    let e = excel_exp(-z);
    let f = if x <= 4.0 {
        let mut num = excel_x87_mul(ERFC_C[8], x);
        let mut den = x;
        for i in 0..7 {
            num = step(num, ERFC_C[i], x);
            den = step(den, ERFC_D[i], x);
        }
        excel_x87_div(excel_x87_add(num, ERFC_C[7]), excel_x87_add(den, ERFC_D[7]))
    } else {
        let u = excel_x87_div(1.0, z);
        let mut num = excel_x87_mul(ERFC_P[5], u);
        let mut den = u;
        for i in 0..4 {
            num = step(num, ERFC_P[i], u);
            den = step(den, ERFC_Q[i], u);
        }
        let r = excel_x87_div(excel_x87_mul(u, excel_x87_add(num, ERFC_P[4])), excel_x87_add(den, ERFC_Q[4]));
        excel_x87_div(excel_x87_sub(ERFC_SQRPI, r), x)
    };
    excel_x87_mul(e, f)
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

    /// Recorded Excel `ERF.PRECISE` answers below 0.5 (`OxFunc-Evidence`: the build-20131
    /// oracle cache, `readings/excel-20527/ERF.PRECISE.json`, W109 banks b16, b17 and
    /// b26-zcross). Each pair is `(argument bits, Excel result bits)`. The first group is
    /// mismatched by `libm::erf`; the second already matched and must stay matched.
    const ERF_TINY_RECORDED: [(u64, u64); 33] = [
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
        (0x3f30000000000000, 0x3f320dd74a3dfe55),
        (0x3f90000000000000, 0x3f920d77083f17a1),
        (0x3fb0000000000000, 0x3fb207d480e90659),
        (0x3fd0000000000000, 0x3fd1af54e232d60a),
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
    fn erf_rational_reproduces_recorded_excel_answers() {
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
        let (n0, n1) = (f64::from_bits(ERF_N[0]), f64::from_bits(ERF_N[1]));
        let (d0, d1) = (f64::from_bits(ERF_D[0]), f64::from_bits(ERF_D[1]));
        assert!((n0 / d0 - f64::from_bits(0x3ff2_0dd7_5042_9b6d)).abs() <= 2.0 * f64::EPSILON);
        assert!((n1 / n0 - d1 / d0 + 1.0 / 3.0).abs() < 1e-15);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phi_matches_excel_probe_lanes() {
        assert!((phi_kernel(0.0) - 0.398942280401433).abs() < 1e-12);
        assert!((phi_kernel(1.0) - 0.241970724519143).abs() < 1e-12);
    }
}

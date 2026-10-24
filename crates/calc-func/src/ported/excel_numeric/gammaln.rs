//! Excel `GAMMALN` / `GAMMALN.PRECISE` positive-argument kernel (W109 G3-02).
//!
//! This reproduces the exact op-graph identified for the *published* GAMMALN
//! worksheet surface — a Cody/SPECFUN-family piecewise rational with band-
//! specific arithmetic precision. It is **not** the shared internal `lgamma`
//! (`special_math_common::ln_gamma`), which is a different routine in Excel too;
//! only the GAMMALN / GAMMALN.PRECISE surfaces route here.
//!
//! Landing spec: `smart-fuzzer/work/w109/G3-02-gamma/agentL_landing_spec.md`.
//! Every constant below is the exact IEEE-754 double the reference scorer
//! (`agentL_composite3.py`) consumes, verified bit-for-bit
//! (`agentP_verify_constants.py`).
//!
//! Bands (dispatch on the positive argument `x`, all comparisons at the
//! boundary double):
//!   * `x < 0.7`         COMPOSED : `core(x+1) - ln(x)`, one double subtract.
//!   * `0.7 <= x < 1.5`  B1       : Cody-Hillstrom 1967 n=7 `(x-1)*N/D`, plain double.
//!   * `1.5 <= x < 4.0`  B2       : anchored rational, x87 **continuous** (one final round).
//!   * `4.0 <= x < 8.0`  B4       : SPECFUN P4/Q4, x87 **continuous** (one final round).
//!   * `x >= 8.0`        STIRLING : fdlibm w-tail, worksheet LN + x87 DR q.
//!
//! The B2/B4 bands use the real x87 80-bit hardware ops (`super::x87::raw`) on
//! `x86_64`; a plain-`f64` fallback keeps the crate building on other targets
//! (not bit-exact there — the identified host is `x86_64`).

// ---- band anchors (correctly-rounded transcendentals) --------------------
const D2: f64 = f64::from_bits(0x3FDB_0EE6_0720_93CE); // 1 - EulerGamma
const D4: f64 = f64::from_bits(0x3FFC_AB0B_FA2A_2002); // ln 6
const LS2PI: f64 = f64::from_bits(0x3FED_67F1_C864_BEB5); // ln(2*pi)/2

// ---- dispatch threshold ---------------------------------------------------
const THR07: f64 = f64::from_bits(0x3FE6_6666_6666_6666); // 0.7

// ---- B1 [0.7,1.5): Cody & Hillstrom 1967 Table II, n=m=7 (ascending i=0..7) --
const P1: [f64; 8] = [
    f64::from_bits(0xC001_ABB6_66D0_134A),
    f64::from_bits(0xC053_5A47_EFFC_8A03),
    f64::from_bits(0xC080_2D1B_5724_230B),
    f64::from_bits(0xC08C_D217_4AAA_2285),
    f64::from_bits(0xC070_5B8C_BB60_E8EC),
    f64::from_bits(0x406E_659B_984E_C72C),
    f64::from_bits(0x4055_6C26_056B_AD85),
    f64::from_bits(0x4010_7BBE_50EC_A47A),
];
const Q1: [f64; 8] = [
    f64::from_bits(0x3FDA_39D2_EC8B_FCDF),
    f64::from_bits(0x4038_6F69_0BC7_F9C5),
    f64::from_bits(0x4070_64EE_FD4A_C8D5),
    f64::from_bits(0x408A_709A_B2B9_6CB8),
    f64::from_bits(0x408D_BA96_BA61_5B30),
    f64::from_bits(0x4077_9D65_5EAA_2824),
    f64::from_bits(0x4046_D2C9_6BBB_64C7),
    f64::from_bits(0x3FF0_0000_0000_0000), // 1.0 (monic)
];

// ---- B2 [1.5,4.0): agent-S LM refit under the CONTINUOUS staging (W109
// lane 4, 2026-07-18) — held-blind selection (noise floor 1.077 vs the old
// spill model's 1.113), validated on the FRESH b32 corpus: 549/1,200 vs the
// gn2/spill landing's 518/1,200. (Ascending Horner order.)
const P2: [f64; 8] = [
    f64::from_bits(0x4013_E5FF_9A8E_0235),
    f64::from_bits(0x4080_F34F_95D4_686C),
    f64::from_bits(0x40CE_4978_25AD_2725),
    f64::from_bits(0x4106_8ECA_52D4_9415),
    f64::from_bits(0x4130_9ACC_C4FB_E930),
    f64::from_bits(0x4149_77D4_7BE6_FFCE),
    f64::from_bits(0x4153_7AF9_6B73_8BB2),
    f64::from_bits(0x4147_741E_8705_57A8),
];
const Q2: [f64; 8] = [
    f64::from_bits(0x4066_E10D_06A2_326E),
    f64::from_bits(0x40BE_550C_A051_7E45),
    f64::from_bits(0x4100_4233_0FF7_BEDC),
    f64::from_bits(0x4131_5841_D242_285C),
    f64::from_bits(0x4154_187F_0784_1A4C),
    f64::from_bits(0x4169_AFB0_D161_2A49),
    f64::from_bits(0x4171_0062_54DB_452A),
    f64::from_bits(0x4162_2ED4_F2F0_6395),
];

// ---- B4 [4.0,8.0): SPECFUN dlgama published P4/Q4 (ascending Horner order) --
const P4: [f64; 8] = [
    f64::from_bits(0x40CC_CC82_C5C6_4704),
    f64::from_bits(0x4142_83DE_AF4B_5720),
    f64::from_bits(0x419C_F647_959E_37B1),
    f64::from_bits(0x41E3_D818_2034_30F7),
    f64::from_bits(0x421B_6268_E3F9_61D5),
    f64::from_bits(0x4243_D256_DDE8_451B),
    f64::from_bits(0x425C_AC7F_DC02_6F8F),
    f64::from_bits(0x4260_50FB_AE6A_CCA5),
];
const Q4: [f64; 8] = [
    f64::from_bits(0x40A5_050F_7336_3548),
    f64::from_bits(0x4123_8339_2180_0ACC),
    f64::from_bits(0x4183_B856_FA6B_57F8),
    f64::from_bits(0x41D0_B3C9_AB67_6EF8),
    f64::from_bits(0x420B_BA43_6E36_4E1B),
    f64::from_bits(0x4237_AC9F_24E3_3E6B),
    f64::from_bits(0x4253_E46F_C45D_AF37),
    f64::from_bits(0x4259_FA9F_BBBD_7E2A),
];

// ---- STIRLING (x>=8): fdlibm e_lgamma_r w1..w6 verbatim -------------------
const W1: f64 = f64::from_bits(0x3FB5_5555_5555_553B);
const W2: f64 = f64::from_bits(0xBF66_C16C_16B0_2E5C);
const W3: f64 = f64::from_bits(0x3F4A_019F_98CF_38B6);
const W4: f64 = f64::from_bits(0xBF43_80CB_8C0F_E741);
const W5: f64 = f64::from_bits(0x3F4B_67BA_4CDA_D5D1);
const W6: f64 = f64::from_bits(0xBF5A_B89D_0B9E_43E4);

/// B1 band `[0.7, 1.5)`: `(x-1) * N(x)/D(x)`, every op plain double, Horner
/// descending over the ascending-stored coefficients.
fn b1(x: f64) -> f64 {
    let mut n = 0.0_f64;
    for &c in P1.iter().rev() {
        n = n * x + c;
    }
    let mut d = 0.0_f64;
    for &c in Q1.iter().rev() {
        d = d * x + c;
    }
    (x - 1.0) * (n / d)
}

/// STIRLING band `x >= 8`: fdlibm w-tail with worksheet LN and x87
/// PC64-to-binary64 stores on the q chain and the final add.
///
/// Identified (W109 G3-02 high-band, 2026-08-09 reconstructed graph, replayed
/// 2026-09-12 on 1711 unique x>=8 rows): `ln = LN87(x)`, then
/// `q = DR(DR(DR((x-0.5)*ln) - x) + LS2PI)`, native fdlibm `z*w` Horner,
/// `out = DR(q + corr)`. Scores 1709/1711 (max 1 ULP) versus production
/// native-ln 1701/1711 (max 2). The two remaining misses
/// (`x=0x40215bf4d43f4d44`, `x=0x40234ce3244e3245`) sit between adjacent
/// stored-log effects; they stay open.
fn stirl8(x: f64) -> f64 {
    let lg = super::excel_log(x);
    let q1 = super::excel_x87_mul(x - 0.5, lg);
    let q2 = super::excel_x87_sub(q1, x);
    let q = super::excel_x87_add(q2, LS2PI);
    let z = 1.0 / x;
    let y = z * z;
    let mut w = W6;
    w = w * y + W5;
    w = w * y + W4;
    w = w * y + W3;
    w = w * y + W2;
    w = w * y + W1;
    super::excel_x87_add(q, z * w)
}

// -------------------------------------------------------------------------
// B2 / B4 extended-precision bands.
// -------------------------------------------------------------------------

/// B2 band `[1.5, 4.0)`: `t*(D2 + t*(XNUM/XDEN))`, `t = x-2`. Everything
/// extended (x87 CONTINUOUS, one final round) — the same class as B4. The
/// earlier "spill each Horner store" tag is superseded (W109 lane 4: the
/// held-blind noise floor picks continuous, 1.077 vs 1.113, and the fresh
/// b32 check confirms 549 vs 518 / 1,200).
#[cfg(target_arch = "x86_64")]
fn b2(x: f64) -> f64 {
    use super::x87::raw::{CW_PC64_RN as CW, ext_add, ext_div, ext_from_f64, ext_mul, ext_to_f64};

    let t = ext_from_f64(x - 2.0);
    let mut xnum = ext_from_f64(0.0);
    let mut xden = ext_from_f64(1.0);
    for i in 0..8 {
        xnum = ext_add(&ext_mul(&xnum, &t, CW), &ext_from_f64(P2[i]), CW);
        xden = ext_add(&ext_mul(&xden, &t, CW), &ext_from_f64(Q2[i]), CW);
    }
    let r = ext_div(&xnum, &xden, CW);
    let t1 = ext_mul(&t, &r, CW); // t * r
    let t2 = ext_add(&ext_from_f64(D2), &t1, CW); // D2 + t*r
    let t3 = ext_mul(&t, &t2, CW); // t * (D2 + t*r)
    ext_to_f64(&t3, CW)
}

/// B4 band `[4.0, 8.0)`: `D4 + u*(XNUM/XDEN)`, `u = x-4`. Everything extended
/// (x87 continuous), one final round. `XDEN` leading coefficient is `-1`.
#[cfg(target_arch = "x86_64")]
fn b4(x: f64) -> f64 {
    use super::x87::raw::{CW_PC64_RN as CW, ext_add, ext_div, ext_from_f64, ext_mul, ext_to_f64};

    let u = ext_from_f64(x - 4.0);
    let mut xnum = ext_from_f64(0.0);
    let mut xden = ext_from_f64(-1.0);
    for i in 0..8 {
        xnum = ext_add(&ext_mul(&xnum, &u, CW), &ext_from_f64(P4[i]), CW);
        xden = ext_add(&ext_mul(&xden, &u, CW), &ext_from_f64(Q4[i]), CW);
    }
    let r = ext_div(&xnum, &xden, CW);
    let t1 = ext_mul(&u, &r, CW); // u * r
    let t2 = ext_add(&ext_from_f64(D4), &t1, CW); // D4 + u*r
    ext_to_f64(&t2, CW)
}

/// Portable fallback for non-`x86_64` targets: same op-graph in plain `f64`
/// (double-rounded, not the x87 extended path — not bit-exact off `x86_64`).
#[cfg(not(target_arch = "x86_64"))]
fn b2(x: f64) -> f64 {
    let t = x - 2.0;
    let mut xnum = 0.0_f64;
    let mut xden = 1.0_f64;
    for i in 0..8 {
        xnum = xnum * t + P2[i];
        xden = xden * t + Q2[i];
    }
    let r = xnum / xden;
    t * (D2 + t * r)
}

#[cfg(not(target_arch = "x86_64"))]
fn b4(x: f64) -> f64 {
    let u = x - 4.0;
    let mut xnum = 0.0_f64;
    let mut xden = -1.0_f64;
    for i in 0..8 {
        xnum = xnum * u + P4[i];
        xden = xden * u + Q4[i];
    }
    let r = xnum / xden;
    D4 + u * r
}

/// Direct core for `x` in `[0.7, 8.0)`: dispatch by band.
fn core(x: f64) -> f64 {
    if x < 1.5 {
        b1(x)
    } else if x < 4.0 {
        b2(x)
    } else {
        b4(x)
    }
}

/// Excel `GAMMALN` / `GAMMALN.PRECISE` for a positive, finite argument.
///
/// Callers guard the domain: `x` must be finite and `> 0` (the surface returns
/// `#NUM!` otherwise). `x = 1.0` and `x = 2.0` return exactly `0.0`.
pub(crate) fn gammaln_excel(x: f64) -> f64 {
    if x < THR07 {
        let xp = x + 1.0;
        let inner = if xp < 8.0 { core(xp) } else { stirl8(xp) };
        inner - x.ln()
    } else if x < 8.0 {
        core(x)
    } else {
        stirl8(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Analytic sanity (loose): the published surface must land near the true
    // values across every band. Bit-exactness is checked by the W109 racer
    // (`smart-fuzzer/tools/calc_graph_racer` bin `check_gammaln_port`).
    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "got {a}, want {b}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn zeros_at_1_and_2() {
        // Both are exactly 0 by construction. x=1.0 yields -0.0 (the (x-1)
        // prefactor times a negative N/D), exactly as the identified op-graph
        // and the reference model do; x=2.0 (B2, t=0) yields +0.0.
        assert_eq!(gammaln_excel(1.0), 0.0);
        assert_eq!(gammaln_excel(1.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(gammaln_excel(2.0).to_bits(), 0.0_f64.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn matches_reference_across_bands() {
        close(gammaln_excel(0.5), 0.5723649429247001, 1e-12); // composed -> B2
        close(gammaln_excel(1.2), -0.0853740900033158, 1e-12); // B1 (negative)
        close(gammaln_excel(3.0), 0.6931471805599453, 1e-12); // B2  (ln 2)
        close(gammaln_excel(5.0), 3.1780538303479458, 1e-12); // B4  (ln 24)
        close(gammaln_excel(10.0), 12.801827480081471, 1e-11); // Stirling (ln 362880)
        close(gammaln_excel(1e-300), 690.7755278982137, 1e-9); // composed, xp->1
    }

    /// Native-only: pins Excel's x87 bits; wasm32 has no x87 route yet (dc-8dg).
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn stirling_ln87_dr_matches_current_build_hits_and_keeps_two_open() {
        // Current-build discovery (build 20228, Value2). Native ln missed
        // old-resid-00/08; LN87+DR matches Excel.
        assert_eq!(
            gammaln_excel(f64::from_bits(0x4020e83b9403b940)).to_bits(),
            0x4022e7db126472c6
        );
        assert_eq!(
            gammaln_excel(f64::from_bits(0x4029800000000000)).to_bits(),
            0x40335bb50a8f4512
        );
        // Two remaining open rows: still exactly 1 ULP below Excel.
        assert_eq!(
            gammaln_excel(f64::from_bits(0x40215bf4d43f4d44)).to_bits(),
            0x4023d98694477878
        );
        assert_eq!(
            gammaln_excel(f64::from_bits(0x40234ce3244e3245)).to_bits(),
            0x40280a8dd6771c99
        );
    }
}

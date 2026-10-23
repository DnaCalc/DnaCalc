
#![allow(clippy::excessive_precision)]
// Staged for the financial/math kernels (W108 next step). Until those call
// sites land, the `pub(crate)` entry points and their backing tables have no
// in-crate caller, so silence dead_code here rather than crate-wide.
#![allow(dead_code)]

mod tables;
use tables::*;

/// The x87 hardware backend — the only site emitting x87 assembly, used on
/// `x86_64` to reproduce Excel bit-for-bit. See the module docs for provenance.
#[cfg(target_arch = "x86_64")]
mod x87;

/// Excel `GAMMALN` / `GAMMALN.PRECISE` positive-argument kernel (W109 G3-02).
/// Distinct from the shared internal `lgamma`; only the published GAMMALN
/// surfaces route here.
mod gammaln;
pub(crate) use gammaln::gammaln_excel;


const EXP_TABLE_BITS: u32 = 7;
const N_EXP: u64 = 1 << EXP_TABLE_BITS; // 128
const LOG_TABLE_BITS: u32 = 7;
const N_LOG: u64 = 1 << LOG_TABLE_BITS; // 128
const OFF: u64 = 0x3fe6000000000000;

#[inline]
fn asuint64(f: f64) -> u64 {
    f.to_bits()
}
#[inline]
fn asdouble(i: u64) -> f64 {
    f64::from_bits(i)
}
#[inline]
fn top12(x: f64) -> u32 {
    (asuint64(x) >> 52) as u32
}
#[inline]
fn top16(x: f64) -> u32 {
    (asuint64(x) >> 48) as u32
}

// exp poly coefficient aliases: poly[5 - EXP_POLY_ORDER=0..], EXP_POLY_ORDER=5.
// C2 = poly[0], C3 = poly[1], C4 = poly[2], C5 = poly[3].
#[inline]
fn c2() -> f64 {
    EXP_POLY[0]
}
#[inline]
fn c3() -> f64 {
    EXP_POLY[1]
}
#[inline]
fn c4() -> f64 {
    EXP_POLY[2]
}
#[inline]
fn c5() -> f64 {
    EXP_POLY[3]
}

// Restricted historical implementation removed (dc-96l.7).
/// Excel's internal expm1 (W109 identification, 17,992/18,000 on the
/// EXPON/GAMMA.DIST-a=1 windows): msvcr100 exports no C99 `expm1`, so Excel
/// builds it from its own primitives via Kahan's cancellation-free correction
/// in DOUBLE arithmetic — `u = exp(t)` (the x87 chain, RN53); `t` when
/// `u == 1`; `(u-1)*t/ln(u)` for `|t| < 1` (ln = the fyl2x chain, == CR on
/// everything probed); plain `u - 1` otherwise. The numerator is one double
/// product, then the divide (alternate orderings refuted at 82.9%/84.9%).
pub(crate) fn excel_expm1_internal(t: f64) -> f64 {
    let u = excel_exp(t);
    if u == 1.0 {
        return t;
    }
    if t.abs() < 1.0 {
        (u - 1.0) * t / excel_log(u)
    } else {
        u - 1.0
    }
}

// Restricted historical implementation removed (dc-96l.7).
/// Portable fallback for non-`x86_64` hosts (not bit-exact to Excel).
#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn excel_binom_pmf_exp(lc: f64, lf: f64) -> f64 {
    (lc - 0.5 * lf).exp()
}

// Restricted historical implementation removed (dc-96l.7).
/// Shared non-normal-domain result for the `fyl2x` logarithms, so `x87::ln`,
/// `x87::log10`, and `x87::log2` all agree with the portable core on `NaN`,
/// `+Inf`, `0`, and negative inputs. Returns `Some(result)` for those; `None`
/// means "compute the logarithm of this finite, positive `x`".
#[cfg(target_arch = "x86_64")]
#[inline]
fn log_domain_guard(x: f64) -> Option<f64> {
    if x.is_nan() {
        return Some(f64::NAN);
    }
    if x == f64::INFINITY {
        return Some(f64::INFINITY);
    }
    if x <= 0.0 {
        // ln(0) = -Inf (incl. -0.0); ln(negative) = NaN. The worksheet #NUM!
        // for x <= 0 is applied by the surface layer / kernel guards.
        return Some(if x == 0.0 {
            f64::NEG_INFINITY
        } else {
            f64::NAN
        });
    }
    None
}

// Restricted historical implementation removed (dc-96l.7).
/// The positive-power staging of Excel's `POWER(x, y)`: `p = exp(y · ln x)` via
/// the x87 `exp`/`ln`, with the product formed as the x87 **double-rounded**
/// `t = RN53(RN64(y · ln x))`. Bit-exact to 64-bit Excel on `x86_64`.
///
/// `base` must be finite and `> 0`, `exp` finite and `> 0`. The caller
/// (`power_fn`) owns the surrounding algorithm: sign of a negative base, the
/// `y < 0` reciprocal via [`excel_x87_recip`], the zero/integer dispatch, and
/// the worksheet error/subnormal publication rules. The returned `p` is the raw
/// positive power and may be `+Inf`, `+0`, or subnormal.
pub(crate) fn excel_pow_positive(base: f64, exp: f64) -> f64 {
    // Excel special-cases exponent 0.5 to the correctly-rounded hardware `sqrt`
    // (SSE2 SQRTSD), NOT `exp(0.5·ln x)` — the latter is 1 ULP off (e.g.
    // POWER(2, 0.5) = √2 exactly). `exp` here is `|y|`, so this also covers the
    // `y = -0.5` case, whose positive power is then reciprocated by the caller.
    // (Live-Excel confirmed 400/400; corrects the reciprocal-staging reference.)
    //
    // The 0.5 shortcut belongs to the POWER wrapper, NOT the underlying pow:
    // the distribution substrate's pow sites hit the raw chain even at
    // exponent 0.5 (W109 b24 s0p5: CR sqrt 412/800 vs chain 800/800).
    if exp == 0.5 {
        return base.sqrt();
    }
    excel_pow_chain(base, exp)
}

/// The raw CRT `pow` chain `exp(RN53(RN64(y · ln x)))` with NO special cases —
/// the distribution-substrate pow (W109 lane 1). Site-proven: BINOM `q^n`
/// (b24 11,045/11,045), WEIBULL cdf/pdf pow sites (b24+b27 all blocks 100%,
/// b27D product-staging discriminator 113/113 for the double-rounded product,
/// no `y == 0.5 → sqrt` and no integer-exponent shortcut at these sites).
/// `y` may be negative or zero (`exp(0·ln x) = 1` exactly, as Excel's chain).
/// `base` must be finite and `> 0`.
pub(crate) fn excel_pow_chain(base: f64, exp: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        let ln_base = x87::ln(base);
        let t = x87::mul(exp, ln_base); // RN53(RN64(exp * ln base))
        x87::exp(t)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        base.powf(exp)
    }
}

// Restricted historical implementation removed (dc-96l.7).
/// Excel `SIN` — bit-exact to 64-bit Excel on `x86_64` (W109 G4-01, validated
/// 1020/1020 live rows incl. held-out, build 20131): the legacy CRT `fFSIN`
/// chain — `FPREM1(x, FLDPI)` (the 64-bit ROM π, NOT the hardware's internal
/// 66-bit reduction), `FSIN` on the residue, `(-1)^Q` parity fixup from the
/// quotient low bit. This is the source of Excel's large-argument trig
/// "error": the reduction constant is the fldpi double-rounding of π.
pub(crate) fn excel_sin(x: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        use x87::raw::{
            CW_PC64_RN, ext_chs, ext_from_f64, ext_pi, ext_prem1_quo, ext_sin, ext_to_f64,
        };
        if !x.is_finite() {
            return f64::NAN;
        }
        let (r, q) = ext_prem1_quo(&ext_from_f64(x), &ext_pi(), CW_PC64_RN);
        let mut s = ext_sin(&r, CW_PC64_RN);
        if q & 1 == 1 {
            s = ext_chs(&s, CW_PC64_RN);
        }
        ext_to_f64(&s, CW_PC64_RN)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        x.sin()
    }
}

/// Excel `COS` — bit-exact on `x86_64` (W109 G4-01/G4-07, 2561/2561 across
/// the original corpus, a focused/randomized discovery battery, and a fresh
/// oracle-blind held-out): `|x| < 2^-26` publishes exactly `1.0` (live-probed;
/// `2^-26` itself takes the chain). Otherwise Excel reduces `|x|` with
/// `FPREM1(|x|, FLDPI/2)`. Even quadrants use `FCOS`; odd quadrants reconstruct
/// the sine magnitude as `sqrt(tan(r)^2 / (1 + tan(r)^2))` in continuous x87
/// PC64/RN arithmetic, then apply the residue and quadrant signs.
pub(crate) fn excel_cos(x: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        use x87::raw::{
            CW_PC64_RN, ext_abs, ext_add, ext_chs, ext_cos, ext_div, ext_from_f64, ext_mul,
            ext_one, ext_pi, ext_prem1_quo, ext_scale, ext_sqrt, ext_tan, ext_to_f64,
        };
        if !x.is_finite() {
            return f64::NAN;
        }
        if x.abs() < f64::from_bits(0x3E50000000000000) {
            // 2^-26
            return 1.0;
        }
        let cw = CW_PC64_RN;
        let minus_one = ext_chs(&ext_one(), cw);
        let pi_half = ext_scale(&ext_pi(), &minus_one, cw); // exact halving
        let xa = ext_abs(&ext_from_f64(x), cw);
        let (r, q) = ext_prem1_quo(&xa, &pi_half, cw);
        let odd_quadrant_sine = || {
            let tangent = ext_tan(&r, cw);
            let square = ext_mul(&tangent, &tangent, cw);
            let denominator = ext_add(&ext_one(), &square, cw);
            let ratio = ext_div(&square, &denominator, cw);
            let mut sine = ext_sqrt(&ratio, cw);
            if ext_to_f64(&tangent, cw).is_sign_negative() {
                sine = ext_chs(&sine, cw);
            }
            sine
        };
        let v = match q & 3 {
            0 => ext_cos(&r, cw),
            1 => ext_chs(&odd_quadrant_sine(), cw),
            2 => ext_chs(&ext_cos(&r, cw), cw),
            _ => odd_quadrant_sine(),
        };
        ext_to_f64(&v, cw)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        x.cos()
    }
}

/// Excel `TAN` — bit-exact on `x86_64` (W109 G4-01, 1020/1020): the `fFTAN`
/// π/2-quadrant chain — `FPREM1(x, FLDPI/2)`, `FPTAN` on the residue, and on
/// odd quadrants `-1/tan(r)` with the reciprocal taken in EXTENDED before the
/// single binary64 store.
pub(crate) fn excel_tan(x: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        use x87::raw::{
            CW_PC64_RN, ext_chs, ext_div, ext_from_f64, ext_one, ext_pi, ext_prem1_quo, ext_scale,
            ext_tan, ext_to_f64,
        };
        if !x.is_finite() {
            return f64::NAN;
        }
        let cw = CW_PC64_RN;
        let minus_one = ext_chs(&ext_one(), cw);
        let pi_half = ext_scale(&ext_pi(), &minus_one, cw);
        let (r, q) = ext_prem1_quo(&ext_from_f64(x), &pi_half, cw);
        let t = ext_tan(&r, cw);
        let v = if q & 1 == 0 {
            t
        } else {
            ext_chs(&ext_div(&ext_one(), &t, cw), cw)
        };
        ext_to_f64(&v, cw)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        x.tan()
    }
}

/// `RN53(RN64(a + b))` — the x87 double-rounded sum of Excel's legacy
/// x87-compiled function bodies (W109: XNPV's `1 + rate` and running-total
/// accumulation). Bit-exact to 64-bit Excel on `x86_64`; plain `a + b`
/// elsewhere (single-rounded — the documented portable fallback).
pub(crate) fn excel_x87_add(a: f64, b: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        x87::add(a, b)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        a + b
    }
}

/// `RN53(RN64(a - b))` — the x87 double-rounded difference (W109: NPER's
/// numerator). Bit-exact to 64-bit Excel on `x86_64`.
pub(crate) fn excel_x87_sub(a: f64, b: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        x87::sub(a, b)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        a - b
    }
}

/// `RN53(RN64(a * b))` — the x87 double-rounded product as a `pub(crate)`
/// spill-loop helper (the same op `POWER` uses internally via [`x87::mul`]).
pub(crate) fn excel_x87_mul(a: f64, b: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        x87::mul(a, b)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        a * b
    }
}

/// `RN53(RN64(a / b))` — the x87 double-rounded quotient (W109: XNPV's
/// per-term `value / pow`). Bit-exact to 64-bit Excel on `x86_64`.
pub(crate) fn excel_x87_div(a: f64, b: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        x87::div(a, b)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        a / b
    }
}

/// `RN53(RN64(1.0 / x))` — the x87 double-rounded reciprocal Excel uses to stage
/// `POWER(x, y)` for `y < 0` (positive power, stored to `binary64`, then one
/// reciprocal). Bit-exact to 64-bit Excel on `x86_64`.
pub(crate) fn excel_x87_recip(x: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        x87::recip(x)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        1.0 / x
    }
}

/// Direct public FPATAN operation retained for separately characterized callers
/// and research comparisons. W111's ATAN and revised imaginary-family callers
/// use the magnitude-reduced graph below. Alternate CPU/platform parity remains
/// a separate validation axis.
pub(crate) fn excel_atan(x: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    { x87::atan(x) }
    #[cfg(not(target_arch = "x86_64"))]
    { x.atan() }
}

/// Magnitude-reduced ATAN graph characterized by W111 live observations.
/// The non-x86-64 fallback remains a separately unvalidated arithmetic target.
pub(crate) fn excel_atan_reduced(x: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    { x87::atan_reduced(x) }
    #[cfg(not(target_arch = "x86_64"))]
    { x.atan() }
}

// Restricted historical implementation removed (dc-96l.7).
/// sqrt(x) — IEEE-754 correctly-rounded, deterministic (hardware SQRTSD).
#[inline]
pub(crate) fn excel_sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// Public worksheet SQRT candidate: extended square root followed by a
/// binary64 store. W111 discovery distinguishes this from a single binary64
/// rounding at 153 of 5,676 fresh normal/zero inputs. Other callers keep their
/// separately evidenced `excel_sqrt` graph until tested.
#[inline]
pub(crate) fn excel_sqrt_staged(x: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        use x87::raw::{CW_PC64_RN, ext_from_f64, ext_sqrt, ext_to_f64};
        ext_to_f64(&ext_sqrt(&ext_from_f64(x), CW_PC64_RN), CW_PC64_RN)
    }
    #[cfg(not(target_arch = "x86_64"))]
    { x.sqrt() }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Golden (x_bits, cr_bits) pulled from the W108-A corpus. These pin the
    // *portable* faithful core (`exp_portable`/`log_portable` and the
    // expm1/log1p/sqrt kernels) to the correctly-rounded value. Excel-parity of
    // the x87 EXP/LN backend is covered separately by
    // `x87::tests::matches_live_excel_bit_for_bit`.
    const GOLDEN: &[(&str, u64, u64)] = &[
        ("EXP", 0xc034000000000000, 0x3e21b48655f37267),
        ("EXP", 0xc034000000000010, 0x3e21b48655f3714c),
        ("EXP", 0xc033ffffffffffef, 0x3e21b48655f37394),
        ("EXP", 0xc034000000000029, 0x3e21b48655f36f91),
        ("LN", 0x3fe0000000000000, 0xbfe62e42fefa39ef),
        ("LN", 0x3fe000000000000e, 0xbfe62e42fefa39d3),
        ("LN", 0x3fe0000000000015, 0xbfe62e42fefa39c5),
        ("LN", 0x3fe02f71aaff02f7, 0xbfe5cfeb44399090),
        ("SQRT", 0x3eb0c6f7a0b5ed8d, 0x3f50624dd2f1a9fc),
        ("SQRT", 0x3eb0c6f7a0b5ed98, 0x3f50624dd2f1aa01),
        ("SQRT", 0x3eb0c6f7a0b5edad, 0x3f50624dd2f1aa0b),
        ("SQRT", 0x3eb0c6f7a0b5eda4, 0x3f50624dd2f1aa07),
        // expm1/log1p CR goldens (mpmath prec 240)
        ("EXPM1", 0x3fda53e707e19ead, 0x3fe048c94c677b63),
        ("EXPM1", 0x3fdb3061c79b5d80, 0x3fe0f03fea9c350d),
        ("EXPM1", 0x3fdc0cdc87551c53, 0x3fe199fb57f320f9),
        ("EXPM1", 0x3fdce957470edb26, 0x3fe2460372d441c9),
        ("LOG1P", 0xbf7e4c6824a105fb, 0xbf7e693c89601f4e),
        ("LOG1P", 0x3f7ab037b0fcada5, 0x3f7a9a0e2db5905f),
        ("LOG1P", 0xbf946ef81ba4d29a, 0xbf94a3dd68efff1c),
        ("LOG1P", 0xbf9462c5719f45e5, 0xbf94976b3dc504e8),
    ];

    #[test]
    fn golden_matches_cr() {
        for &(f, xb, crb) in GOLDEN {
            let x = f64::from_bits(xb);
            let got = match f {
                "EXP" => exp_portable(x),
                "LN" => log_portable(x),
                "SQRT" => excel_sqrt(x),
                "EXPM1" => excel_expm1(x),
                "LOG1P" => excel_log1p(x),
                _ => unreachable!(),
            }
            .to_bits();
            assert_eq!(
                got, crb,
                "{} x=0x{:016x}: got 0x{:016x} want 0x{:016x}",
                f, xb, got, crb
            );
        }
    }

    #[test]
    fn expm1_edge_cases() {
        assert_eq!(excel_expm1(0.0).to_bits(), 0.0f64.to_bits()); // +0
        assert_eq!(excel_expm1(-0.0).to_bits(), (-0.0f64).to_bits()); // -0 preserved
        assert_eq!(excel_expm1(f64::NEG_INFINITY), -1.0);
        assert_eq!(excel_expm1(f64::INFINITY), f64::INFINITY);
        assert!(excel_expm1(f64::NAN).is_nan());
        assert_eq!(excel_expm1(1000.0), f64::INFINITY); // overflow
        assert_eq!(excel_expm1(-1000.0), -1.0); // saturates to -1
        assert_eq!(excel_expm1(-50.0), -1.0); // exp(-50) below 0.5 ULP of 1
        // tiny x: expm1(x) == x
        let tiny = f64::from_bits(0x3c80000000000000); // 0x1p-55
        assert_eq!(excel_expm1(tiny).to_bits(), tiny.to_bits());
        // expm1(1): the correctly-rounded double is 0x3ffb7e151628aed3 (mpmath).
        // Note this is NOT (E - 1.0) = 0x3ffb7e151628aed2, which is 1 ULP low —
        // exactly the cancellation expm1 exists to avoid.
        assert_eq!(excel_expm1(1.0).to_bits(), 0x3ffb7e151628aed3);
    }

    #[test]
    fn log1p_edge_cases() {
        assert_eq!(excel_log1p(0.0).to_bits(), 0.0f64.to_bits()); // +0
        assert_eq!(excel_log1p(-0.0).to_bits(), (-0.0f64).to_bits()); // -0 preserved
        assert_eq!(excel_log1p(-1.0), f64::NEG_INFINITY);
        assert!(excel_log1p(-1.5).is_nan()); // x < -1
        assert!(excel_log1p(-2.0).is_nan());
        assert_eq!(excel_log1p(f64::INFINITY), f64::INFINITY);
        assert!(excel_log1p(f64::NAN).is_nan());
        // consistency with log: log1p(x) == log(1+x) where 1+x is exact
        assert_eq!(excel_log1p(1.0).to_bits(), excel_log(2.0).to_bits());
        assert_eq!(excel_log1p(7.0).to_bits(), excel_log(8.0).to_bits());
    }

    /// W109 G4-01 trig identification pins — live Excel 16.0 build 20131.
    /// One row per identified chain plus the bit-resolution COS threshold.
    /// The 5,425-row live sweep is replayed by `verify_trig_promotion`.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn trig_matches_live_excel_pinned_witnesses() {
        use crate::ported::excel_numeric::{excel_cos, excel_sin, excel_tan, excel_x87_recip};
        let sin_rows: &[(u64, u64)] = &[
            (0x419ffffffc000000, 0xbfee977f5248babf), // SIN(134217727), old 5664-ULP row
            (0x40f86a0000000000, 0x3fa24daa9c527f7c), // SIN(100000)
            (0xc0f86a0000000000, 0xbfa24daa9c527f7c), // SIN(-100000)
        ];
        for &(xb, want) in sin_rows {
            assert_eq!(excel_sin(f64::from_bits(xb)).to_bits(), want);
        }
        let cos_rows: &[(u64, u64)] = &[
            (0x40489b7812ada40a, 0x3fdfcbaf84b75409), // COS(49.214601836), old 1-ULP row
            // G4-07 odd-quadrant tangent-square publication discriminators and
            // their immediate neighbors. The former FSIN branch missed the two
            // interior even-bit phases by one ULP.
            (0x4062a6de04ab68ff, 0xbf86a0d99f46d96c),
            (0x4062a6de04ab6900, 0xbf86a0d99f46996e),
            (0x4062a6de04ab6901, 0xbf86a0d99f46596e),
            (0x4062a6de04ab6902, 0xbf86a0d99f461970),
            (0x4062a6de04ab6903, 0xbf86a0d99f45d970),
        ];
        for &(xb, want) in cos_rows {
            assert_eq!(excel_cos(f64::from_bits(xb)).to_bits(), want);
        }
        // fFCOS small-argument shortcut: strictly below 2^-26 -> exactly 1.0;
        // 2^-26 itself takes the FCOS chain (1 - ulp).
        let t = f64::from_bits(0x3E50000000000000);
        assert_eq!(
            excel_cos(f64::from_bits(t.to_bits() - 1)).to_bits(),
            1.0f64.to_bits()
        );
        assert_eq!(
            excel_cos(-f64::from_bits(t.to_bits() - 1)).to_bits(),
            1.0f64.to_bits()
        );
        assert_eq!(excel_cos(t).to_bits(), 0x3fefffffffffffff);
        let tan_rows: &[(u64, u64)] = &[
            (0x4128574328f5c28f, 0x4023ebd3414768f1), // TAN(797601.58), old 719-ULP row
            (0x40f86a0000000000, 0xbfa250a9d5033223), // TAN(100000)
        ];
        for &(xb, want) in tan_rows {
            assert_eq!(excel_tan(f64::from_bits(xb)).to_bits(), want);
        }
        // Reciprocal family at 100000 (old 351-ULP rows): double-rounded recip
        // of the published primaries.
        let x = f64::from_bits(0x40f86a0000000000);
        assert_eq!(excel_x87_recip(excel_tan(x)).to_bits(), 0xc03bf480ac5b2f41); // COT
        assert_eq!(excel_x87_recip(excel_sin(x)).to_bits(), 0x403bf91476e180a3); // CSC
        assert_eq!(excel_x87_recip(excel_cos(x)).to_bits(), 0xbff0029eabb0db59); // SEC
    }

    #[test]
    fn edge_cases() {
        assert_eq!(excel_exp(0.0).to_bits(), 1.0f64.to_bits());
        assert_eq!(excel_exp(-0.0).to_bits(), 1.0f64.to_bits());
        assert_eq!(excel_log(1.0).to_bits(), 0.0f64.to_bits()); // +0.0, not -0.0
        assert!(excel_log(-1.0).is_nan());
        assert_eq!(excel_log(f64::INFINITY), f64::INFINITY);
        assert_eq!(excel_log(0.0), f64::NEG_INFINITY);
        assert_eq!(excel_exp(f64::NEG_INFINITY), 0.0);
        assert_eq!(excel_exp(710.0), f64::INFINITY); // overflow
        assert_eq!(excel_exp(-746.0), 0.0); // underflow to 0
        assert!(excel_exp(f64::NAN).is_nan());
    }
}


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

/// The x87 in software (dc-8dg): the extended format and the x87 operations the chains below
/// run, in portable Rust, for the targets without the hardware.
mod soft87;

use soft87::{
    CW_PC64_RN as CW, Ext80, ext_abs, ext_chs, ext_from_f64 as load, ext_l2e, ext_lg2, ext_ln2,
    ext_one, ext_pi,
};

/// The x87 operations Excel's CRT sequences run, by backend: [`Soft`], the software x87 on every
/// target, and on x86_64 [`Hard`], the instructions themselves. Each sequence below is written
/// once over this trait; the entry points run [`Native`]. Loads, `FABS`, `FCHS` and the ROM
/// constants are bit operations both backends share ([`soft87`]).
pub(crate) trait Fpu {
    fn add(a: &Ext80, b: &Ext80, cw: u16) -> Ext80;
    fn sub(a: &Ext80, b: &Ext80, cw: u16) -> Ext80;
    fn mul(a: &Ext80, b: &Ext80, cw: u16) -> Ext80;
    fn div(a: &Ext80, b: &Ext80, cw: u16) -> Ext80;
    fn sqrt(x: &Ext80, cw: u16) -> Ext80;
    fn rndint(x: &Ext80, cw: u16) -> Ext80;
    fn scale(x: &Ext80, k: &Ext80, cw: u16) -> Ext80;
    fn prem1_quo(x: &Ext80, modulus: &Ext80, cw: u16) -> (Ext80, u8);
    fn f2xm1(x: &Ext80, cw: u16) -> Ext80;
    fn fyl2x(y: &Ext80, x: &Ext80, cw: u16) -> Ext80;
    fn fpatan(y: &Ext80, x: &Ext80, cw: u16) -> Ext80;
    fn sin(x: &Ext80, cw: u16) -> Ext80;
    fn cos(x: &Ext80, cw: u16) -> Ext80;
    fn tan(x: &Ext80, cw: u16) -> Ext80;
    /// `FSTP qword`.
    fn store(x: &Ext80, cw: u16) -> f64;
}

macro_rules! fpu_backend {
    ($backend:ident, $($m:ident)::+) => {
        impl Fpu for $backend {
            fn add(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_add(a, b, cw) }
            fn sub(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_sub(a, b, cw) }
            fn mul(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_mul(a, b, cw) }
            fn div(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_div(a, b, cw) }
            fn sqrt(x: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_sqrt(x, cw) }
            fn rndint(x: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_rndint(x, cw) }
            fn scale(x: &Ext80, k: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_scale(x, k, cw) }
            fn prem1_quo(x: &Ext80, modulus: &Ext80, cw: u16) -> (Ext80, u8) {
                $($m)::+::ext_prem1_quo(x, modulus, cw)
            }
            fn f2xm1(x: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_f2xm1(x, cw) }
            fn fyl2x(y: &Ext80, x: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_fyl2x(y, x, cw) }
            fn fpatan(y: &Ext80, x: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_fpatan(y, x, cw) }
            fn sin(x: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_sin(x, cw) }
            fn cos(x: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_cos(x, cw) }
            fn tan(x: &Ext80, cw: u16) -> Ext80 { $($m)::+::ext_tan(x, cw) }
            fn store(x: &Ext80, cw: u16) -> f64 { $($m)::+::ext_to_f64(x, cw) }
        }
    };
}

/// The software x87 ([`soft87`]).
pub(crate) struct Soft;
fpu_backend!(Soft, soft87);

/// The x87 instructions ([`x87::raw`]).
#[cfg(target_arch = "x86_64")]
pub(crate) struct Hard;
#[cfg(target_arch = "x86_64")]
fpu_backend!(Hard, x87::raw);

/// The backend the entry points run: the hardware where it exists, the software x87 elsewhere,
/// and on x86_64 too under the `soft-x87` feature (to measure the wasm32 route natively, for
/// example `cargo xtask compare` over the catalogue oracle).
#[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
pub(crate) type Native = Hard;
#[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
pub(crate) type Native = Soft;

/// The x87 sign test `FTST` + `C0`: below zero (a negative zero is not).
fn is_below_zero(x: &Ext80) -> bool {
    x.0[9] & 0x80 != 0 && (x.0[..8].iter().any(|b| *b != 0) || x.0[8] != 0 || x.0[9] & 0x7f != 0)
}

// Restricted historical implementation removed (dc-96l.7).
/// `FPATAN(x, 1)`.
fn atan_with<F: Fpu>(x: f64) -> f64 {
    F::store(&F::fpatan(&load(x), &ext_one(), CW), CW)
}

/// For `|x| > 1`: `pi/2 - FPATAN(1, |x|)` in the registers, the sign of `x` after the store.
fn atan_reduced_with<F: Fpu>(x: f64) -> f64 {
    let magnitude = x.abs();
    if magnitude <= 1.0 {
        return atan_with::<F>(x);
    }
    let angle = F::fpatan(&ext_one(), &load(magnitude), CW);
    let half_pi = F::div(&ext_pi(), &load(2.0), CW);
    F::store(&F::sub(&half_pi, &angle, CW), CW).copysign(x)
}

/// One x87 operation on two binary64 operands at PC=64, then the binary64 store.
fn binary_with<F: Fpu>(op: fn(&Ext80, &Ext80, u16) -> Ext80, a: f64, b: f64) -> f64 {
    F::store(&op(&load(a), &load(b), CW), CW)
}

/// The `fFSIN` chain: `FPREM1(x, FLDPI)`, `FSIN`, the `(-1)^Q` sign.
fn sin_with<F: Fpu>(x: f64) -> f64 {
    let (r, q) = F::prem1_quo(&load(x), &ext_pi(), CW);
    let mut s = F::sin(&r, CW);
    if q & 1 == 1 {
        s = ext_chs(&s, CW);
    }
    F::store(&s, CW)
}

/// The `fFCOS` chain (see [`excel_cos`]).
fn cos_with<F: Fpu>(x: f64) -> f64 {
    let minus_one = ext_chs(&ext_one(), CW);
    let pi_half = F::scale(&ext_pi(), &minus_one, CW); // exact halving
    let xa = ext_abs(&load(x), CW);
    let (r, q) = F::prem1_quo(&xa, &pi_half, CW);
    let odd_quadrant_sine = || {
        let tangent = F::tan(&r, CW);
        let square = F::mul(&tangent, &tangent, CW);
        let denominator = F::add(&ext_one(), &square, CW);
        let ratio = F::div(&square, &denominator, CW);
        let mut sine = F::sqrt(&ratio, CW);
        if tangent.0[9] & 0x80 != 0 {
            sine = ext_chs(&sine, CW);
        }
        sine
    };
    let v = match q & 3 {
        0 => F::cos(&r, CW),
        1 => ext_chs(&odd_quadrant_sine(), CW),
        2 => ext_chs(&F::cos(&r, CW), CW),
        _ => odd_quadrant_sine(),
    };
    F::store(&v, CW)
}

/// The `fFTAN` chain: `FPREM1(x, FLDPI/2)`, `FPTAN`, and `-1/tan(r)` on odd quadrants.
fn tan_with<F: Fpu>(x: f64) -> f64 {
    let minus_one = ext_chs(&ext_one(), CW);
    let pi_half = F::scale(&ext_pi(), &minus_one, CW);
    let (r, q) = F::prem1_quo(&load(x), &pi_half, CW);
    let t = F::tan(&r, CW);
    let v = if q & 1 == 0 { t } else { ext_chs(&F::div(&ext_one(), &t, CW), CW) };
    F::store(&v, CW)
}

// Restricted historical implementation removed (dc-96l.7).
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
// Restricted historical implementation removed (dc-96l.7).
/// Shared non-normal-domain result for the `fyl2x` logarithms, so `ln` and `log10` agree with
/// the portable core on `NaN`, `+Inf`, `0`, and negative inputs. Returns `Some(result)` for
/// those; `None` means "compute the logarithm of this finite, positive `x`".
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
    let t = excel_x87_mul(exp, excel_log(base)); // RN53(RN64(exp * ln base))
    excel_exp(t)
}

// Restricted historical implementation removed (dc-96l.7).
/// Excel `SIN` — bit-exact to 64-bit Excel on `x86_64` (W109 G4-01, validated
/// 1020/1020 live rows incl. held-out, build 20131): the legacy CRT `fFSIN`
/// chain — `FPREM1(x, FLDPI)` (the 64-bit ROM π, NOT the hardware's internal
/// 66-bit reduction), `FSIN` on the residue, `(-1)^Q` parity fixup from the
/// quotient low bit. This is the source of Excel's large-argument trig
/// "error": the reduction constant is the fldpi double-rounding of π.
pub(crate) fn excel_sin(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    sin_with::<Native>(x)
}

/// Excel `COS` — bit-exact on `x86_64` (W109 G4-01/G4-07, 2561/2561 across
/// the original corpus, a focused/randomized discovery battery, and a fresh
/// oracle-blind held-out): `|x| < 2^-26` publishes exactly `1.0` (live-probed;
/// `2^-26` itself takes the chain). Otherwise Excel reduces `|x|` with
/// `FPREM1(|x|, FLDPI/2)`. Even quadrants use `FCOS`; odd quadrants reconstruct
/// the sine magnitude as `sqrt(tan(r)^2 / (1 + tan(r)^2))` in continuous x87
/// PC64/RN arithmetic, then apply the residue and quadrant signs.
pub(crate) fn excel_cos(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x.abs() < f64::from_bits(0x3E50000000000000) {
        // 2^-26
        return 1.0;
    }
    cos_with::<Native>(x)
}

/// Excel `TAN` — bit-exact on `x86_64` (W109 G4-01, 1020/1020): the `fFTAN`
/// π/2-quadrant chain — `FPREM1(x, FLDPI/2)`, `FPTAN` on the residue, and on
/// odd quadrants `-1/tan(r)` with the reciprocal taken in EXTENDED before the
/// single binary64 store.
pub(crate) fn excel_tan(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    tan_with::<Native>(x)
}

/// `RN53(RN64(a + b))` — the x87 double-rounded sum of Excel's legacy
/// x87-compiled function bodies (W109: XNPV's `1 + rate` and running-total
/// accumulation; dc-6qki: the worksheet `+` and SUM's running total). Bit-exact to 64-bit Excel
/// on `x86_64`; elsewhere the same operation in the software x87.
pub(crate) fn excel_x87_add(a: f64, b: f64) -> f64 {
    #[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
    {
        x87::add(a, b)
    }
    #[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
    {
        binary_with::<Soft>(Soft::add, a, b)
    }
}

/// `RN53(RN64(a - b))` — the x87 double-rounded difference (W109: NPER's
/// numerator). Bit-exact to 64-bit Excel on `x86_64`.
pub(crate) fn excel_x87_sub(a: f64, b: f64) -> f64 {
    #[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
    {
        x87::sub(a, b)
    }
    #[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
    {
        binary_with::<Soft>(Soft::sub, a, b)
    }
}

/// `RN53(RN64(a * b))` — the x87 double-rounded product as a `pub(crate)`
/// spill-loop helper (the same op `POWER` uses internally via [`x87::mul`]).
pub(crate) fn excel_x87_mul(a: f64, b: f64) -> f64 {
    #[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
    {
        x87::mul(a, b)
    }
    #[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
    {
        binary_with::<Soft>(Soft::mul, a, b)
    }
}

/// `RN53(RN64(a / b))` — the x87 double-rounded quotient (W109: XNPV's
/// per-term `value / pow`). Bit-exact to 64-bit Excel on `x86_64`.
pub(crate) fn excel_x87_div(a: f64, b: f64) -> f64 {
    #[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
    {
        x87::div(a, b)
    }
    #[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
    {
        binary_with::<Soft>(Soft::div, a, b)
    }
}

/// `RN53(RN64(1.0 / x))` — the x87 double-rounded reciprocal Excel uses to stage
/// `POWER(x, y)` for `y < 0` (positive power, stored to `binary64`, then one
/// reciprocal). Bit-exact to 64-bit Excel on `x86_64`.
pub(crate) fn excel_x87_recip(x: f64) -> f64 {
    #[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
    {
        x87::recip(x)
    }
    #[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
    {
        binary_with::<Soft>(Soft::div, 1.0, x)
    }
}

/// Direct public FPATAN operation retained for separately characterized callers
/// and research comparisons. W111's ATAN and revised imaginary-family callers
/// use the magnitude-reduced graph below.
pub(crate) fn excel_atan(x: f64) -> f64 {
    #[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
    {
        x87::atan(x)
    }
    #[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
    {
        atan_with::<Soft>(x)
    }
}

/// Magnitude-reduced ATAN graph characterized by W111 live observations.
pub(crate) fn excel_atan_reduced(x: f64) -> f64 {
    #[cfg(all(target_arch = "x86_64", not(feature = "soft-x87")))]
    {
        x87::atan_reduced(x)
    }
    #[cfg(not(all(target_arch = "x86_64", not(feature = "soft-x87"))))]
    {
        atan_reduced_with::<Soft>(x)
    }
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
    Native::store(&Native::sqrt(&load(x), CW), CW)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A xorshift generator for the random comparisons.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// A binary64 with a random significand and sign and a leading exponent in
        /// `lo .. lo + span`.
        fn signed(&mut self, lo: i64, span: u64) -> f64 {
            let x = self.positive(lo, span);
            if self.next() & 1 == 0 { x } else { -x }
        }
        fn positive(&mut self, lo: i64, span: u64) -> f64 {
            let e = (1023 + lo + (self.next() % span) as i64) as u64;
            f64::from_bits((self.next() & 0x000f_ffff_ffff_ffff) | (e << 52))
        }
    }

    /// dc-6qki, dc-8dg: the software x87's double-rounded operations equal the x87 hardware's,
    /// operation for operation, over random operands of mixed magnitudes, among which a few
    /// hundred per operation are the near-midpoint cases where the double rounding moves the
    /// result; and over operands at the ends of the binary64 range.
    #[cfg(target_arch = "x86_64")]
    /// Native-only: it runs the x87 hardware, which only x86_64 has.
    #[test]
    fn software_x87_arithmetic_matches_the_x87_hardware() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut moved = [0usize; 5];
        for i in 0..400_000 {
            let (a, b) = if i % 8 == 7 {
                // Products and quotients that leave the normal range, and huge operands.
                (rng.signed(-1022, 2046), rng.signed(-1022, 2046))
            } else {
                (rng.positive(-20, 40), rng.signed(-20, 40))
            };
            let cases = [
                (x87::add(a, b), binary_with::<Soft>(Soft::add, a, b), a + b),
                (x87::sub(a, b), binary_with::<Soft>(Soft::sub, a, b), a - b),
                (x87::mul(a, b), binary_with::<Soft>(Soft::mul, a, b), a * b),
                (x87::div(a, b), binary_with::<Soft>(Soft::div, a, b), a / b),
                (x87::recip(b), binary_with::<Soft>(Soft::div, 1.0, b), 1.0 / b),
            ];
            for (op, (hardware, software, single)) in cases.into_iter().enumerate() {
                assert_eq!(hardware.to_bits(), software.to_bits(), "op {op} a={a:e} b={b:e}");
                if hardware.to_bits() != single.to_bits() {
                    moved[op] += 1;
                }
            }
        }
        assert!(moved.iter().all(|&n| n > 20), "too few near-midpoint cases: {moved:?}");
    }

    /// dc-8dg: each chain written over [`Fpu`] gives, on the hardware backend, the bits of the
    /// x86_64 entry point (the fused x87 assembly) it stands beside, so the software backend runs
    /// the sequence the hardware runs.
    #[cfg(target_arch = "x86_64")]
    /// Native-only: it runs the x87 hardware, which only x86_64 has.
    #[test]
    fn chains_on_the_hardware_backend_equal_the_x87_assembly() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        for _ in 0..20_000 {
            let x = rng.signed(-30, 40);
            let p = rng.positive(-30, 60);
            let y = rng.signed(-8, 12);
            assert_eq!(exp_with::<Hard>(x, CW).to_bits(), x87::exp(x).to_bits(), "exp {x:e}");
            assert_eq!(ln_with::<Hard>(p).to_bits(), x87::ln(p).to_bits(), "ln {p:e}");
            assert_eq!(log10_with::<Hard>(p).to_bits(), x87::log10(p).to_bits(), "log10 {p:e}");
            assert_eq!(
                pow_direct_with::<Hard>(p, y).to_bits(),
                x87::pow_direct(p, y).to_bits(),
                "pow {p:e} {y:e}"
            );
            assert_eq!(atan_with::<Hard>(x).to_bits(), x87::atan(x).to_bits(), "atan {x:e}");
            assert_eq!(
                atan_reduced_with::<Hard>(x).to_bits(),
                x87::atan_reduced(x).to_bits(),
                "atan_reduced {x:e}"
            );
        }
    }

    /// dc-8dg: how often the software x87 and the hardware give different binary64 results, chain
    /// by chain, over random arguments (`SOFT87_N` per chain, 100,000 by default). Run with
    /// `cargo test -p calc-func --release --lib software_chains -- --ignored --nocapture`.
    #[cfg(target_arch = "x86_64")]
    /// Native-only: it runs the x87 hardware, which only x86_64 has.
    #[test]
    #[ignore]
    fn software_chains_against_the_hardware() {
        let n: usize = std::env::var("SOFT87_N").ok().and_then(|v| v.parse().ok()).unwrap_or(100_000);
        let run = |name: &str, seed: u64, f: &dyn Fn(&mut Rng) -> (f64, f64, String)| {
            let mut rng = Rng(seed);
            let mut differ = 0usize;
            for _ in 0..n {
                let (hardware, software, what) = f(&mut rng);
                if hardware.to_bits() != software.to_bits() && !(hardware.is_nan() && software.is_nan()) {
                    if differ < 3 {
                        eprintln!("  {name} {what}: hardware {:016x} software {:016x}", hardware.to_bits(), software.to_bits());
                    }
                    differ += 1;
                }
            }
            eprintln!("{name}: {differ} of {n} differ");
        };
        run("EXP", 1, &|r| {
            let x = r.signed(-8, 18);
            (excel_exp(x), exp_with::<Soft>(x, CW), format!("{x:e}"))
        });
        run("LN", 2, &|r| {
            let x = r.positive(-60, 120);
            (excel_log(x), ln_with::<Soft>(x), format!("{x:e}"))
        });
        run("LOG10", 3, &|r| {
            let x = r.positive(-60, 120);
            (excel_log10(x), log10_with::<Soft>(x), format!("{x:e}"))
        });
        run("pow chain", 4, &|r| {
            let (b, y) = (r.positive(-10, 20), r.signed(-6, 10));
            let soft = exp_with::<Soft>(binary_with::<Soft>(Soft::mul, y, ln_with::<Soft>(b)), CW);
            (excel_pow_chain(b, y), soft, format!("{b:e}^{y:e}"))
        });
        run("pow direct", 5, &|r| {
            let (b, y) = (r.positive(-10, 20), r.signed(-6, 10));
            (excel_pow_x87_direct(b, y), pow_direct_with::<Soft>(b, y), format!("{b:e}^{y:e}"))
        });
        run("ATAN", 6, &|r| {
            let x = r.signed(-30, 60);
            (excel_atan(x), atan_with::<Soft>(x), format!("{x:e}"))
        });
        run("ATAN reduced", 7, &|r| {
            let x = r.signed(-30, 60);
            (excel_atan_reduced(x), atan_reduced_with::<Soft>(x), format!("{x:e}"))
        });
        run("SIN", 8, &|r| {
            let x = r.signed(-20, 47);
            (excel_sin(x), sin_with::<Soft>(x), format!("{x:e}"))
        });
        run("COS", 9, &|r| {
            let x = r.signed(-20, 47);
            (excel_cos(x), cos_with::<Soft>(x), format!("{x:e}"))
        });
        run("TAN", 10, &|r| {
            let x = r.signed(-20, 47);
            (excel_tan(x), tan_with::<Soft>(x), format!("{x:e}"))
        });
        run("GEOMEAN root", 11, &|r| {
            let (p, k) = (r.positive(-40, 80), (r.next() % 30 + 2) as f64);
            (root_with::<Hard>(p, k), root_with::<Soft>(p, k), format!("{p:e} n={k}"))
        });
        run("BINOM pmf exp", 12, &|r| {
            let (lc, lf) = (r.signed(-4, 10), r.signed(-4, 10));
            (excel_binom_pmf_exp(lc, lf), binom_pmf_exp_with::<Soft>(lc, lf), format!("{lc:e} {lf:e}"))
        });
        run("EXP chop", 13, &|r| {
            let x = r.signed(-8, 18);
            (excel_exp_rz(x), exp_with::<Soft>(x, CW | 0x0C00), format!("{x:e}"))
        });
        run("SQRT staged", 14, &|r| {
            let x = r.positive(-1022, 2046);
            (excel_sqrt_staged(x), Soft::store(&Soft::sqrt(&load(x), CW), CW), format!("{x:e}"))
        });
    }

    /// The LN witnesses of `x87_excel_ground_truth.tsv` where the software x87 gives the other
    /// neighbour (dc-8dg): the hardware's `FYL2X` and the exactly rounded `FYL2X` differ by one
    /// unit of the 64-bit significand, and that unit decides the binary64 rounding. These rows
    /// were harvested as the hardest near-midpoint LN inputs; on random inputs the LN chains
    /// differ 3 times in 200,000 (`software_chains_against_the_hardware`).
    const LN_WITNESSES_THE_SOFTWARE_X87_ROUNDS_THE_OTHER_WAY: [u64; 18] = [
        0x0d45fbbe4b0e7d55, 0x0dbe4e3a44304ad6, 0x11f544676cf1bf2f, 0x1eb393c13e56f40a,
        0x23a4108745e19fab, 0x2d152de2df4ff1e8, 0x2d20badbb5556728, 0x2d3693905c941f14,
        0x2e68258fb8991289, 0x420be30a078fed72, 0x4574658f6970c8b6, 0x461d6906690fdadb,
        0x4d7d2f67e7585ef8, 0x4fb4742155e8cc52, 0x501baf60a3904e63, 0x559d2eada7837182,
        0x603a2ee36db86f49, 0x6fffb3b90594a629,
    ];

    /// dc-8dg: the software x87's EXP and LN chains against every EXP/LN witness harvested from
    /// live Excel (`x87_excel_ground_truth.tsv`, which `x87::tests` replays on the hardware), on
    /// every target. Every EXP witness matches; the LN witnesses match except the rows named in
    /// [`LN_WITNESSES_THE_SOFTWARE_X87_ROUNDS_THE_OTHER_WAY`], each one unit of the last place
    /// away, and on x86_64 each of those is shown to be a hardware `FYL2X` result that differs
    /// from the exactly rounded one.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn software_exp_and_ln_against_live_excel() {
        const GROUND_TRUTH: &str = include_str!("x87_excel_ground_truth.tsv");
        let (mut exp_rows, mut ln_rows, mut ln_differ) = (0u32, 0u32, Vec::new());
        for line in GROUND_TRUTH.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let mut it = line.split('\t');
            let func = it.next().unwrap();
            let x = f64::from_bits(u64::from_str_radix(it.next().unwrap(), 16).unwrap());
            let want = u64::from_str_radix(it.next().unwrap(), 16).unwrap();
            match func {
                "EXP" => {
                    exp_rows += 1;
                    let got = exp_with::<Soft>(x, CW).to_bits();
                    assert_eq!(got, want, "EXP x={:016x}", x.to_bits());
                }
                "LN" => {
                    ln_rows += 1;
                    let got = ln_with::<Soft>(x).to_bits();
                    if got != want {
                        assert_eq!(got.abs_diff(want), 1, "LN x={:016x}", x.to_bits());
                        ln_differ.push(x.to_bits());
                    }
                }
                _ => {}
            }
        }
        assert_eq!((exp_rows, ln_rows), (136, 113));
        assert_eq!(ln_differ, LN_WITNESSES_THE_SOFTWARE_X87_ROUNDS_THE_OTHER_WAY);
        #[cfg(target_arch = "x86_64")]
        for bits in ln_differ {
            let x = load(f64::from_bits(bits));
            assert_ne!(Hard::fyl2x(&ext_ln2(), &x, CW), Soft::fyl2x(&ext_ln2(), &x, CW), "{bits:016x}");
        }
    }

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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

    /// The trig pins on one backend; returns the rows that differ.
    fn trig_pin_mismatches<F: Fpu>() -> Vec<String> {
        let mut bad = Vec::new();
        let mut check = |name: &str, got: f64, want: u64| {
            if got.to_bits() != want {
                bad.push(format!("{name}: got {:016x} want {want:016x}", got.to_bits()));
            }
        };
        let cos = |x: f64| if x.abs() < f64::from_bits(0x3E50000000000000) { 1.0 } else { cos_with::<F>(x) };
        let sin_rows: &[(u64, u64)] = &[
            (0x419ffffffc000000, 0xbfee977f5248babf), // SIN(134217727), old 5664-ULP row
            (0x40f86a0000000000, 0x3fa24daa9c527f7c), // SIN(100000)
            (0xc0f86a0000000000, 0xbfa24daa9c527f7c), // SIN(-100000)
        ];
        for &(xb, want) in sin_rows {
            check(&format!("SIN {xb:016x}"), sin_with::<F>(f64::from_bits(xb)), want);
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
            check(&format!("COS {xb:016x}"), cos(f64::from_bits(xb)), want);
        }
        // fFCOS small-argument shortcut: strictly below 2^-26 -> exactly 1.0;
        // 2^-26 itself takes the FCOS chain (1 - ulp).
        let t = f64::from_bits(0x3E50000000000000);
        check("COS below 2^-26", cos(f64::from_bits(t.to_bits() - 1)), 1.0f64.to_bits());
        check("COS above -2^-26", cos(-f64::from_bits(t.to_bits() - 1)), 1.0f64.to_bits());
        check("COS 2^-26", cos(t), 0x3fefffffffffffff);
        let tan_rows: &[(u64, u64)] = &[
            (0x4128574328f5c28f, 0x4023ebd3414768f1), // TAN(797601.58), old 719-ULP row
            (0x40f86a0000000000, 0xbfa250a9d5033223), // TAN(100000)
        ];
        for &(xb, want) in tan_rows {
            check(&format!("TAN {xb:016x}"), tan_with::<F>(f64::from_bits(xb)), want);
        }
        // Reciprocal family at 100000 (old 351-ULP rows): double-rounded recip
        // of the published primaries.
        let x = f64::from_bits(0x40f86a0000000000);
        let recip = |v: f64| binary_with::<F>(F::div, 1.0, v);
        check("COT", recip(tan_with::<F>(x)), 0xc03bf480ac5b2f41);
        check("CSC", recip(sin_with::<F>(x)), 0x403bf91476e180a3);
        check("SEC", recip(cos(x)), 0xbff0029eabb0db59);
        bad
    }

    /// W109 G4-01 trig identification pins — live Excel 16.0 build 20131.
    /// One row per identified chain plus the bit-resolution COS threshold.
    /// The 5,425-row live sweep is replayed by `verify_trig_promotion`. dc-8dg: on the backend
    /// the entry points run and on the software x87 (on x86_64 both).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trig_matches_live_excel_pinned_witnesses() {
        let native = trig_pin_mismatches::<Native>();
        let software = trig_pin_mismatches::<Soft>();
        assert!(native.is_empty() && software.is_empty(), "native {native:?} software {software:?}");
        // The entry points add the non-finite and small-COS guards to the same chains.
        assert_eq!(excel_sin(f64::from_bits(0x40f86a0000000000)).to_bits(), 0x3fa24daa9c527f7c);
        assert_eq!(excel_cos(f64::from_bits(0x3E50000000000000)).to_bits(), 0x3fefffffffffffff);
        assert_eq!(excel_tan(f64::from_bits(0x40f86a0000000000)).to_bits(), 0xbfa250a9d5033223);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

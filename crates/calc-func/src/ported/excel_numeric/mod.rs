//! Excel-faithful elementary numeric core (W108).
//!
//! # Backend dispatch
//! The public entry points ([`excel_exp`], [`excel_log`], [`excel_log10`], the powers, the
//! trigonometric chains and the x87 double-rounded operations) are **bit-exact to 64-bit
//! Microsoft Excel** on `x86_64`, where they run x87 instructions (control word `0x133F`,
//! precision 64, as measured). The exponential and logarithm chains are in [`x87_chains`]
//! behind a documented interface (under licence review, dc-96l). Each sequence is written once
//! over the [`Fpu`] trait; on other targets
//! (wasm32) it runs on [`soft87`], the x87 in software (dc-8dg). The software x87 is exact for
//! the arithmetic, the loads and stores and the remainder; its transcendental instructions
//! (`F2XM1`, `FYL2X`, `FPATAN`, `FSIN`, `FCOS`, `FPTAN`) round the exact value once, where the
//! hardware's microcode rounds a value a few bits wider than the format, so the two can differ
//! by one unit of the 64-bit significand, and a binary64 result then differs only where that
//! unit decides the binary64 rounding. The tests measure both rates.
//!
//! # Portable core (`expm1`/`log1p`/`sqrt` substrate)
//! [`exp_portable`] / [`log_portable`] are a pure-Rust, zero-dependency port of
//! Arm optimized-routines' `exp` and `log` (Szabolcs Nagy; `math/exp.c`, `math/log.c`,
//! `math/exp_data.c`, `math/log_data.c` of https://github.com/ARM-software/optimized-routines
//! at 2ee4b0d4, MIT OR Apache-2.0 WITH LLVM-exception, used under MIT; Copyright (c) 2018-2025
//! Arm Limited; notice in `deploy/notices/ported.txt`), configured without fused multiply-add
//! and without the rounding intrinsics, `EXP_TABLE_BITS=7` / `LOG_TABLE_BITS=7`, `EXP_POLY_ORDER=5`,
//! `LOG_POLY_ORDER=6`, `LOG_POLY1_ORDER=12`. Changed from the C: written in Rust, the errno and
//! exception-flag tails dropped (they do not change the returned value). `excel_expm1` /
//! `excel_log1p` are our own, built on those same argument-reduction + polynomial kernels;
//! `excel_sqrt` is the hardware SQRTSD
//! (SSE2), IEEE-754 correctly-rounded by definition. These have no Excel
//! worksheet counterpart (they are internal composition helpers) and so are not
//! routed through the x87 backend.
//!
//! # Provenance
//! `tables.rs` (`EXP_INVLN2N`, `EXP_TAB`, `LOG_TAB`, …) holds Arm's `__exp_data` (N = 128) and
//! `__log_data` (N = 128, with `tab2`) as bit patterns; every value equals Arm's
//! `math/exp_data.c` and `math/log_data.c` at 2ee4b0d4 (checked value for value, dc-96l).
//!
//! # Determinism contract
//! * **NO `f64::mul_add` / FMA anywhere.** Every `a*b + c` is two separate IEEE
//!   ops, so the result never depends on whether the target has hardware FMA.
//!   (Rust's default codegen never contracts `a*b + c` into an FMA — that only
//!   happens under explicit `mul_add` or fast-math, neither of which we use.)
//! * No calls into the platform libm. `f64::sqrt` maps to hardware SQRTSD
//!   (SSE2), IEEE-754 correctly-rounded on every target, hence deterministic.
//! * `double_t == f64` here (matching MSVC x64 `FLT_EVAL_METHOD == 0`, the
//!   environment in which the reference was validated).
//!
//! # Accuracy
//! `<= ~0.502 ULP` vs the correctly-rounded result everywhere ("faithful"
//! rounding), i.e. correctly rounded on ~99.5% of inputs. This matches Excel's
//! own ~0.502-ULP bespoke routine on ~99% of inputs; the residual near-midpoint
//! rows (where a faithful core rounds the opposite way) are the tracked W108
//! discrepancy, not a defect.
//!
//! # External validation
//! The W108 prototype compared the core bit for bit with the reference C built without fused
//! multiply-add, and expm1/log1p with an mpmath (240-bit) correctly rounded corpus. The in-crate
//! golden-witness test below pins a handful of those correctly rounded witnesses so any
//! accidental change to the core is caught without the large corpus files.

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

/// Excel's exponential and logarithm chains on the x87, behind the interface its module docs
/// describe: written clean-room under dc-96l.3 from Intel's documented instruction set.
mod x87_chains;
pub(crate) use x87_chains::excel_exp_difference;
pub(crate) use x87_chains::{
    excel_binom_pmf_exp, excel_exp, excel_exp_rz, excel_log, excel_log10, excel_pow_x87_direct,
    excel_root_x87,
};

/// The Microsoft runtime's `pow` in portable Rust (dc-sbo).
mod crt_pow;
pub(crate) use crt_pow::crt_pow;

/// The Microsoft runtime's elementary functions in portable Rust (dc-twc).
mod crt_libm;
pub(crate) use crt_libm::{
    crt_atan2, crt_cos, crt_cosh, crt_exp, crt_hypot, crt_log, crt_log10, crt_sin, crt_sinh,
};

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

/// The SIN chain: `FPREM1(x, FLDPI)`, `FSIN`, the `(-1)^Q` sign.
fn sin_with<F: Fpu>(x: f64) -> f64 {
    let (r, q) = F::prem1_quo(&load(x), &ext_pi(), CW);
    let mut s = F::sin(&r, CW);
    if q & 1 == 1 {
        s = ext_chs(&s, CW);
    }
    F::store(&s, CW)
}

/// The COS chain (see [`excel_cos`]).
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

/// The TAN chain: `FPREM1(x, FLDPI/2)`, `FPTAN`, and `-1/tan(r)` on odd quadrants.
fn tan_with<F: Fpu>(x: f64) -> f64 {
    let minus_one = ext_chs(&ext_one(), CW);
    let pi_half = F::scale(&ext_pi(), &minus_one, CW);
    let (r, q) = F::prem1_quo(&load(x), &pi_half, CW);
    let t = F::tan(&r, CW);
    let v = if q & 1 == 0 { t } else { ext_chs(&F::div(&ext_one(), &t, CW), CW) };
    F::store(&v, CW)
}

/// Excel `GAMMALN` / `GAMMALN.PRECISE` positive-argument kernel (W109 G3-02).
/// Distinct from the shared internal `lgamma`; only the published GAMMALN
/// surfaces route here.
mod gammaln;
pub(crate) use gammaln::{gamma_excel, gammaln_excel};


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

/// Handle results that may overflow/underflow (subnormal range): Arm's `specialcase`
/// (`math/exp.c`), with the errno-only `check_oflow/uflow` tails reduced to identity (they do
/// not change the returned value).
#[inline]
fn specialcase(tmp: f64, mut sbits: u64, ki: u64) -> f64 {
    let scale: f64;
    let mut y: f64;
    if (ki & 0x80000000) == 0 {
        // k > 0, exponent of scale might have overflowed by <= 460.
        sbits = sbits.wrapping_sub(1009u64 << 52);
        scale = asdouble(sbits);
        y = f64::from_bits(0x7f00000000000000) /* 0x1p1009 */ * (scale + scale * tmp);
        return y;
    }
    // k < 0, need special care in the subnormal range.
    sbits = sbits.wrapping_add(1022u64 << 52);
    scale = asdouble(sbits);
    y = scale + scale * tmp;
    if y < 1.0 {
        // Round y to the right precision before scaling into subnormal range
        // to avoid double rounding.
        let hi: f64;
        let mut lo: f64;
        lo = scale - y + scale * tmp;
        hi = 1.0 + y;
        lo = 1.0 - hi + y + lo;
        y = (hi + lo) - 1.0;
        // Avoid -0.0 with downward rounding (WANT_ROUNDING).
        if y == 0.0 {
            y = 0.0;
        }
    }
    y = f64::from_bits(0x0010000000000000) /* 0x1p-1022 */ * y;
    y
}

/// Excel's internal expm1 (W109 identification, 17,992/18,000 on the
/// EXPON/GAMMA.DIST-a=1 windows): msvcr100 exports no C99 `expm1`, so Excel
/// builds it from its own primitives via Kahan's cancellation-free correction
/// in DOUBLE arithmetic — `u = exp(t)` (the x87 chain, RN53); `t` when
/// `u == 1`; `(u-1)*t/ln(u)` for `|t| < 1` (ln = the fyl2x chain, == CR on
/// everything probed); plain `u - 1` otherwise. The numerator is one double
/// product, then the divide (alternate orderings refuted at 82.9%/84.9%).
pub(crate) fn excel_expm1_internal(t: f64) -> f64 {
    expm1_internal_graph(t, false)
}

/// The same cancellation correction with each arithmetic assignment stored from x87 PC64.
/// EXPON.DIST's recorded rows distinguish both the numerator product and the final division
/// from binary64 arithmetic; its density also stores the rate product through x87.
pub(crate) fn excel_expm1_spilled(t: f64) -> f64 {
    expm1_internal_graph(t, true)
}

fn expm1_internal_graph(t: f64, spilled: bool) -> f64 {
    let u = excel_exp(t);
    if u == 1.0 {
        return t;
    }
    let difference = if spilled { excel_x87_sub(u, 1.0) } else { u - 1.0 };
    if t.abs() < 1.0 {
        if spilled {
            excel_x87_div(excel_x87_mul(difference, t), excel_log(u))
        } else {
            difference * t / excel_log(u)
        }
    } else {
        difference
    }
}

/// Portable faithful (`~0.502` ULP) double-precision `e^x` — Arm optimized-routines' `exp`
/// without fused multiply-add: a correctly-rounded reference in tests and the deterministic substrate the
/// financial family composes against (W108 Bead C, pending its x87 migration).
pub(crate) fn exp_portable(x: f64) -> f64 {
    let mut abstop: u32 = top12(x) & 0x7ff;

    // 0x1p-54 top12, 512.0 top12
    let t_2m54 = top12(f64::from_bits(0x3c90000000000000)); // 0x1p-54
    let t_512 = top12(512.0);
    if abstop.wrapping_sub(t_2m54) >= t_512.wrapping_sub(t_2m54) {
        if abstop.wrapping_sub(t_2m54) >= 0x80000000 {
            // Avoid spurious underflow for tiny x. 0 is common input.
            return 1.0 + x;
        }
        if abstop >= top12(1024.0) {
            if asuint64(x) == asuint64(f64::NEG_INFINITY) {
                return 0.0;
            }
            if abstop >= top12(f64::INFINITY) {
                return 1.0 + x;
            }
            if asuint64(x) >> 63 != 0 {
                return 0.0; // __math_uflow
            } else {
                return f64::INFINITY; // __math_oflow
            }
        }
        // Large x is special cased below.
        abstop = 0;
    }

    // exp(x) = 2^(k/N) * exp(r)
    let z = EXP_INVLN2N * x;
    // !TOINT_INTRINSICS path
    let kd0 = z + EXP_SHIFT;
    let ki = asuint64(kd0);
    let kd = kd0 - EXP_SHIFT;

    let r = x + kd * EXP_NEGLN2HIN + kd * EXP_NEGLN2LON;
    let idx = (2 * (ki % N_EXP)) as usize;
    let top = ki << (52 - EXP_TABLE_BITS);
    let tail = asdouble(EXP_TAB[idx]);
    let sbits = EXP_TAB[idx + 1].wrapping_add(top);
    let r2 = r * r;
    let tmp = tail + r + r2 * (c2() + r * c3()) + r2 * r2 * (c4() + r * c5());
    if abstop == 0 {
        return specialcase(tmp, sbits, ki);
    }
    let scale = asdouble(sbits);
    scale + scale * tmp
}

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

/// Excel `SIN` — bit-exact to 64-bit Excel on `x86_64` (W109 G4-01, validated
/// 1020/1020 live rows incl. held-out, build 20131): the SIN
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

/// Excel `TAN` — bit-exact on `x86_64` (W109 G4-01, 1020/1020): the TAN
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

/// `RN53(RN64(RN64(a * b) + c))`: carry the PC64 product into the sum,
/// then store once to binary64. Both x87 operations use the same backend
/// on native and portable targets, with no binary64 store between them.
pub(crate) fn excel_x87_mul_add(a: f64, b: f64, c: f64) -> f64 {
    let product = Native::mul(&load(a), &load(b), CW);
    Native::store(&Native::add(&product, &load(c), CW), CW)
}

/// Temme's `c * (w +/- coefficient * t / rta)`, retaining each PC64 result
/// until the final binary64 store. The complement starts from that stored result.
pub(crate) fn excel_x87_temme_finish(
    c: f64, w: f64, coefficient: f64, t: f64, rta: f64, subtract: bool,
) -> (f64, f64) {
    let correction = Native::div(&Native::mul(&load(coefficient), &load(t), CW), &load(rta), CW);
    let inner = if subtract {
        Native::sub(&load(w), &correction, CW)
    } else {
        Native::add(&load(w), &correction, CW)
    };
    let answer = Native::store(&Native::mul(&load(c), &inner, CW), CW);
    let half = load(0.5);
    let complement = Native::store(&Native::add(&half, &Native::sub(&half, &load(answer), CW), CW), CW);
    (answer, complement)
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

/// Portable faithful (`~0.507` ULP) double-precision `ln(x)` — Arm optimized-routines' `log`
/// without fused multiply-add: a correctly-rounded reference in tests and the deterministic substrate the
/// financial family composes against (W108 Bead C, pending its x87 migration).
pub(crate) fn log_portable(x: f64) -> f64 {
    let ix = asuint64(x);
    let top = top16(x);

    // Close-to-1.0 fast path.
    let lo_b = asuint64(1.0 - f64::from_bits(0x3fb0000000000000)); // 1.0 - 0x1p-4
    let hi_b = asuint64(1.0 + f64::from_bits(0x3fb0900000000000)); // 1.0 + 0x1.09p-4
    if ix.wrapping_sub(lo_b) < hi_b.wrapping_sub(lo_b) {
        // Fix sign of zero with downward rounding when x==1.
        if ix == asuint64(1.0) {
            return 0.0;
        }
        let r = x - 1.0;
        let r2 = r * r;
        let r3 = r * r2;
        let b = &LOG_POLY1;
        let mut y = r3
            * (b[1]
                + r * b[2]
                + r2 * b[3]
                + r3 * (b[4]
                    + r * b[5]
                    + r2 * b[6]
                    + r3 * (b[7] + r * b[8] + r2 * b[9] + r3 * b[10])));
        let w = r * f64::from_bits(0x41a0000000000000); // 0x1p27
        let rhi = r + w - w;
        let rlo = r - rhi;
        let w2 = rhi * rhi * b[0]; // b[0] == -0.5
        let hi = r + w2;
        let mut lo = r - hi + w2;
        lo += b[0] * rlo * (rhi + r);
        y += lo;
        y += hi;
        return y;
    }

    let mut ix = ix;
    if top.wrapping_sub(0x0010) >= 0x7ff0 - 0x0010 {
        // x < 0x1p-1022 or inf or nan.
        if ix.wrapping_mul(2) == 0 {
            return f64::NEG_INFINITY; // __math_divzero
        }
        if ix == asuint64(f64::INFINITY) {
            return x; // log(inf) == inf
        }
        if (top & 0x8000) != 0 || (top & 0x7ff0) == 0x7ff0 {
            return (x - x) / (x - x); // __math_invalid -> NaN
        }
        // subnormal, normalize
        ix = asuint64(x * f64::from_bits(0x4330000000000000)); // 0x1p52
        ix = ix.wrapping_sub(52u64 << 52);
    }

    let (hi, lo_rest) = log_general_hi_lo(ix);
    hi + lo_rest
}

/// Internal: the main `__log` table path, returning the result as an unrounded
/// (hi, lo_rest) pair such that `log(x) == hi + lo_rest` (before final rounding).
/// `ix` is `asuint64(x)` for a NORMAL, positive x already outside the close-to-1
/// window (subnormals must be pre-normalized by the caller as in `excel_log`).
/// Exposing the split lets `excel_log1p` inject the `1+x` compensation term
/// into `lo_rest` and round only once.
#[inline]
fn log_general_hi_lo(ix: u64) -> (f64, f64) {
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> (52 - LOG_TABLE_BITS)) % N_LOG) as usize;
    let k = (tmp as i64) >> 52; // arithmetic shift
    let iz = ix.wrapping_sub(tmp & (0xfffu64 << 52));
    let (invc, logc) = LOG_TAB[i];
    let z = asdouble(iz);

    // no-FMA path: r = (z - chi - clo) * invc
    let (chi, clo) = LOG_TAB2[i];
    let r = (z - chi - clo) * invc;
    let kd = k as f64;

    let w = kd * LOG_LN2HI + logc;
    let hi = w + r;
    let lo = w - hi + r + kd * LOG_LN2LO;

    let r2 = r * r;
    let a = &LOG_POLY;
    // lo_rest = lo + polynomial (everything except the dominant `hi` term)
    let lo_rest = lo + r2 * a[0] + r * r2 * (a[1] + r * a[2] + r2 * (a[3] + r * a[4]));
    (hi, lo_rest)
}

// ============================================================================
// expm1 — our own, built on exp_portable's argument reduction + polynomial. For non-tiny x we reproduce exp's decomposition
//   exp(x) = scale * (1 + tmp),   scale = 2^(k/N),   tmp = tail + poly(r)
// and reconstruct expm1(x) = (scale - 1) + scale*tmp with a compensated
// subtraction so the leading digits are not lost. For small |x| (k == 0) we
// evaluate the exp polynomial directly (expm1 ~ r + poly), and for tiny |x| we
// return x (expm1(x) -> x with no representable correction).
// ============================================================================

/// e^x - 1 — faithful (~0.5 ULP) deterministic double-precision expm1.
pub(crate) fn excel_expm1(x: f64) -> f64 {
    let ix = asuint64(x);
    let abstop = top12(x) & 0x7ff;

    // |x| < 0x1p-53: expm1(x) == x to nearest (including +-0 and subnormals),
    // because the next term x^2/2 is below 0.5 ULP of x.
    if abstop < top12(f64::from_bits(0x3ca0000000000000)) {
        // 0x1p-53
        return x;
    }

    // Special / out-of-range handling.
    // top12(0x1p6)=... use |x| >= 0x1.63108c75a1937p+9 ~ 708 overflow; and
    // x <= -0x1.2b708872320e2p+5 (~ -37.4) => expm1 == -1 to nearest.
    if abstop >= top12(708.0_f64) {
        // near the exp overflow / large-magnitude regime, or inf/nan
        if abstop >= 0x7ff {
            // inf or nan
            if ix == asuint64(f64::NEG_INFINITY) {
                return -1.0;
            }
            // +inf -> +inf ; nan -> nan (x propagates, +1 keeps nan)
            return x + x;
        }
        if x > 0.0 {
            // exp overflows -> expm1 overflows to +inf
            return f64::INFINITY;
        }
        // very negative: expm1(x) == -1 (exp(x) underflows below 0.5 ULP of 1)
        return -1.0;
    }
    // For x <= about -37 (still finite, abstop below 708 top12 only if |x|<512),
    // handle the "expm1 == -1" saturation explicitly: exp(x) < 2^-54 => e^x-1
    // rounds to -1.
    if x < -37.0 {
        return -1.0;
    }

    // ---- exp's argument reduction (no-FMA path, matches excel_exp) ----
    let z = EXP_INVLN2N * x;
    let kd0 = z + EXP_SHIFT;
    let ki = asuint64(kd0);
    let kd = kd0 - EXP_SHIFT;
    let r = x + kd * EXP_NEGLN2HIN + kd * EXP_NEGLN2LON;
    let idx = (2 * (ki % N_EXP)) as usize;
    let top = ki << (52 - EXP_TABLE_BITS);
    let tail = asdouble(EXP_TAB[idx]);
    let sbits = EXP_TAB[idx + 1].wrapping_add(top);
    let r2 = r * r;
    // tmp such that exp(x) = scale * (1 + tmp)
    let tmp = tail + r + r2 * (c2() + r * c3()) + r2 * r2 * (c4() + r * c5());
    let scale = asdouble(sbits);

    // expm1(x) = scale*(1+tmp) - 1 = (scale - 1) + scale*tmp
    // Compute (scale - 1) with a compensated split so we keep the low bits when
    // scale is close to 1 (small k). Use a two-sum of (scale - 1).
    let sm1 = scale - 1.0;
    // exact error of (scale - 1): scale - 1 is exact when |k| small... not
    // always. Recover the rounding remainder of scale-1 via Dekker two-sum with
    // the known operands (scale, -1.0).
    // two_sum(scale, -1.0): s = scale + (-1); bb = s - scale; err = (scale-(s-bb)) + (-1 - bb)
    let s = sm1;
    let bb = s - scale;
    let err = (scale - (s - bb)) + (-1.0 - bb);
    // result = s + (scale*tmp + err), added in increasing-magnitude order.
    let y = scale * tmp + err;
    s + y
}

// ============================================================================
// log1p — built on excel_log's poly1 double-double kernel (for small |x|) and
// the main log table path (for the rest); our own composition.
// For the rest we form 1+x as a compensated (hi, lo) pair so no precision is
// lost, take log(hi) via the main kernel, and add the exact correction lo/hi.
// ============================================================================

/// log(1 + x) — faithful (~0.5 ULP) deterministic double-precision log1p.
pub(crate) fn excel_log1p(x: f64) -> f64 {
    let ix = asuint64(x);
    let top = top16(x);

    // ---- edge cases ----
    // log1p(0) = +0 ; log1p(-0) = -0 (preserve sign of zero).
    if x == 0.0 {
        return x;
    }
    if x.is_nan() {
        return x + x;
    }
    if x == f64::INFINITY {
        return f64::INFINITY;
    }
    // x < -1 -> NaN ; x == -1 -> -inf.
    if x < -1.0 {
        return (x - x) / (x - x); // NaN
    }
    if x == -1.0 {
        return f64::NEG_INFINITY;
    }

    // ---- small |x|: direct poly1 double-double kernel with f = x ----
    // The __log close-to-1 branch computes log(1+r) with r = (input) - 1. Here
    // the input IS 1+x, so r = x exactly (no rounding), giving a cancellation-
    // free log1p for |x| in [-0x1p-4, 0x1.09p-4].
    // Bounds mirror excel_log's close-to-1 window, expressed on x directly.
    let x_lo = -f64::from_bits(0x3fb0000000000000); // -0x1p-4
    let x_hi = f64::from_bits(0x3fb0900000000000); //  0x1.09p-4
    if x >= x_lo && x <= x_hi {
        let r = x;
        let r2 = r * r;
        let r3 = r * r2;
        let b = &LOG_POLY1;
        let mut y = r3
            * (b[1]
                + r * b[2]
                + r2 * b[3]
                + r3 * (b[4]
                    + r * b[5]
                    + r2 * b[6]
                    + r3 * (b[7] + r * b[8] + r2 * b[9] + r3 * b[10])));
        let w = r * f64::from_bits(0x41a0000000000000); // 0x1p27
        let rhi = r + w - w;
        let rlo = r - rhi;
        let w2 = rhi * rhi * b[0]; // b[0] == -0.5
        let hi = r + w2;
        let mut lo = r - hi + w2;
        lo += b[0] * rlo * (rhi + r);
        y += lo;
        y += hi;
        return y;
    }

    // ---- general path: form 1+x as compensated (uhi, ulo), then
    //      log1p(x) = log(uhi) + ulo/uhi. ----
    let _ = (ix, top);
    // two_sum(1.0, x) (Knuth, no ordering assumption) so uhi + ulo == 1 + x
    // EXACTLY. ulo captures the bits of x lost when rounding 1+x.
    let uhi = 1.0 + x;
    let bv = uhi - 1.0;
    let ulo = (1.0 - (uhi - bv)) + (x - bv);

    // uhi is in (0, 2) and normal here (|x| may be up to 1e6). If uhi happens to
    // land in the close-to-1 window we still get a correct result via the table
    // kernel below (poly1 already handled small x). Get log(uhi) as (hi, lo_rest)
    // and inject the correction ulo/uhi BEFORE the single final rounding:
    //   log(1+x) = log(uhi + ulo) = log(uhi) + ulo/uhi  (d log/du = 1/u).
    let (hi, mut lo_rest) = log_general_hi_lo(asuint64(uhi));
    lo_rest += ulo / uhi;
    hi + lo_rest
}

/// `ln(1 + x)` as Excel's PMT computes it (dc-j9k.12): the atanh series
/// `ln(1 + x) = 2 atanh(s) = 2s (1 + s^2/3 + s^4/5 + ...)`, `s = x / (2 + x)` (Abramowitz and
/// Stegun 4.1.27 with `z = 1 + x`), all in binary64: `s` from a binary64 sum and quotient, the
/// tail `T = 1/3 + s^2/5 + ... + s^20/23` by Horner, and the leading `2s` added last:
/// `2s + 2s * (s^2 * T)`. Not correctly rounded: it differs from the correctly rounded log1p
/// by up to 2 ulp, and those differences are what Excel's PMT shows. Twelve terms converge for
/// `|s| <= 1/5` (`-1/3 <= x <= 1/2`), the range the caller uses it on.
pub(crate) fn excel_log1p_atanh(x: f64) -> f64 {
    let s = x / (2.0 + x);
    let s2 = s * s;
    let mut tail = 1.0 / 23.0;
    for k in (1..=10).rev() {
        tail = tail * s2 + 1.0 / f64::from(2 * k + 1);
    }
    let two_s = 2.0 * s;
    two_s + two_s * (s2 * tail)
}

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

/// Shared by this module's tests.
#[cfg(test)]
mod test_support {
    /// A xorshift generator for the random comparisons.
    pub(super) struct Rng(pub(super) u64);
    impl Rng {
        pub(super) fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// A binary64 with a random significand and sign and a leading exponent in
        /// `lo .. lo + span`.
        pub(super) fn signed(&mut self, lo: i64, span: u64) -> f64 {
            let x = self.positive(lo, span);
            if self.next() & 1 == 0 { x } else { -x }
        }
        pub(super) fn positive(&mut self, lo: i64, span: u64) -> f64 {
            let e = (1023 + lo + (self.next() % span) as i64) as u64;
            f64::from_bits((self.next() & 0x000f_ffff_ffff_ffff) | (e << 52))
        }
    }
}

/// The LN witnesses of `x87_excel_ground_truth.tsv` where the software x87 gives the other
/// neighbour (dc-8dg): the hardware's `FYL2X` and the exactly rounded `FYL2X` differ by one
/// unit of the 64-bit significand, and that unit decides the binary64 rounding. These rows
/// were harvested as the hardest near-midpoint LN inputs; on random inputs the LN chains
/// differ 3 times in 200,000.
/// The EXP witnesses where the clean-room chain ([`x87_chains`], dc-96l.3) gives the binary64
/// neighbour of Excel's result, on the hardware and the software x87 alike (an interim
/// difference the owner accepted on 2026-10-30; the remaining sequence detail is dc-j9k's).
#[cfg(test)]
const EXP_WITNESSES_THE_CLEAN_ROOM_CHAIN_ROUNDS_THE_OTHER_WAY: [u64; 4] = [
    0x3fe4a631cec5f8da, 0x3ff1f4e8c801c7ce, 0x400381fcca4a56ce, 0xbff5a2abdfff61bf,
];

#[cfg(test)]
const LN_WITNESSES_THE_SOFTWARE_X87_ROUNDS_THE_OTHER_WAY: [u64; 18] = [
    0x0d45fbbe4b0e7d55, 0x0dbe4e3a44304ad6, 0x11f544676cf1bf2f, 0x1eb393c13e56f40a,
    0x23a4108745e19fab, 0x2d152de2df4ff1e8, 0x2d20badbb5556728, 0x2d3693905c941f14,
    0x2e68258fb8991289, 0x420be30a078fed72, 0x4574658f6970c8b6, 0x461d6906690fdadb,
    0x4d7d2f67e7585ef8, 0x4fb4742155e8cc52, 0x501baf60a3904e63, 0x559d2eada7837182,
    0x603a2ee36db86f49, 0x6fffb3b90594a629,
];

#[cfg(test)]
#[path = "../../../tests/f1_support/x87_cpu.rs"]
mod x87_cpu;

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::Rng;

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

    /// dc-8dg: the ATAN chains over [`Fpu`] on the hardware backend give the bits of the x87
    /// assembly they stand beside.
    #[cfg(target_arch = "x86_64")]
    /// Native-only: it runs the x87 hardware, which only x86_64 has.
    #[test]
    fn chains_on_the_hardware_backend_equal_the_x87_assembly() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        for _ in 0..20_000 {
            let x = rng.signed(-30, 40);
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
        run("SQRT staged", 14, &|r| {
            let x = r.positive(-1022, 2046);
            (excel_sqrt_staged(x), Soft::store(&Soft::sqrt(&load(x), CW), CW), format!("{x:e}"))
        });
    }


    /// Excel's EXP and LN through the interface the rest of the crate calls
    /// ([`excel_exp`], [`excel_log`]) against every witness harvested from live Excel
    /// (`x87_excel_ground_truth.tsv`). Every EXP row matches except
    /// [`EXP_WITNESSES_THE_CLEAN_ROOM_CHAIN_ROUNDS_THE_OTHER_WAY`], each one unit of the last place
    /// away, on both backends. Hardware LN results are exact against the measured CPU vendor
    /// in `x87_cpu` for its listed rows, and against the original reading for every other row.
    /// Excel itself on Intel has not been measured for these inputs. On the software x87 the LN rows match except
    /// [`LN_WITNESSES_THE_SOFTWARE_X87_ROUNDS_THE_OTHER_WAY`], each one unit of the last place
    /// away. On x86_64, listed hardware `FYL2X` rows use the same exact vendor expectations;
    /// every other row in that list must differ from the exactly rounded software result.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn exp_and_ln_against_live_excel() {
        const GROUND_TRUTH: &str = include_str!("x87_excel_ground_truth.tsv");
        let software = cfg!(not(all(target_arch = "x86_64", not(feature = "soft-x87"))));
        let (mut exp_rows, mut ln_rows, mut ln_differ) = (0u32, 0u32, Vec::new());
        let mut exp_differ = Vec::new();
        for line in GROUND_TRUTH.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let mut it = line.split('\t');
            let func = it.next().unwrap();
            let x = f64::from_bits(u64::from_str_radix(it.next().unwrap(), 16).unwrap());
            let want = u64::from_str_radix(it.next().unwrap(), 16).unwrap();
            match func {
                "EXP" => {
                    exp_rows += 1;
                    let got = excel_exp(x).to_bits();
                    let want = if let Some(row) = x87_cpu::find("EXP", x.to_bits()) {
                        assert_eq!(want, row.for_vendor("AuthenticAMD").bits, "original Excel reading");
                        let expected = row.expected_bits();
                        assert_eq!(got, expected, "vendor-scoped EXP x={:016x}", x.to_bits());
                        expected
                    } else {
                        want
                    };
                    if got != want {
                        assert_eq!(got.abs_diff(want), 1, "EXP x={:016x}", x.to_bits());
                        exp_differ.push(x.to_bits());
                    }
                }
                "LN" => {
                    ln_rows += 1;
                    let got = excel_log(x).to_bits();
                    let want = if let Some(row) = x87_cpu::find("LN", x.to_bits()) {
                        assert_eq!(want, row.for_vendor("AuthenticAMD").bits, "original Excel reading");
                        if software {
                            assert_eq!(got, row.software_bits, "software LN x={:016x}", x.to_bits());
                            want // Preserve the software backend's own difference list below.
                        } else {
                            row.expected_bits()
                        }
                    } else {
                        want
                    };
                    if got != want {
                        assert!(software, "LN x={:016x}: got {got:016x} want {want:016x}", x.to_bits());
                        assert_eq!(got.abs_diff(want), 1, "LN x={:016x}", x.to_bits());
                        ln_differ.push(x.to_bits());
                    }
                }
                _ => {}
            }
        }
        assert_eq!((exp_rows, ln_rows), (136, 113));
        assert_eq!(exp_differ, EXP_WITNESSES_THE_CLEAN_ROOM_CHAIN_ROUNDS_THE_OTHER_WAY);
        if software {
            assert_eq!(ln_differ, LN_WITNESSES_THE_SOFTWARE_X87_ROUNDS_THE_OTHER_WAY);
        }
        #[cfg(target_arch = "x86_64")]
        for bits in LN_WITNESSES_THE_SOFTWARE_X87_ROUNDS_THE_OTHER_WAY {
            let x = load(f64::from_bits(bits));
            let hard = Hard::fyl2x(&ext_ln2(), &x, CW);
            let soft = Soft::fyl2x(&ext_ln2(), &x, CW);
            if let Some(row) = x87_cpu::find("LN", bits) {
                let expected = row.hardware_bits();
                assert_eq!(Hard::store(&hard, CW).to_bits(), expected, "hardware FYL2X {bits:016x}");
                assert_eq!(Soft::store(&soft, CW).to_bits(), row.software_bits, "software FYL2X {bits:016x}");
            } else {
                assert_ne!(hard, soft, "{bits:016x}");
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn spot_values() {
        assert_eq!(excel_exp(0.0).to_bits(), 1.0f64.to_bits());
        assert_eq!(excel_exp(1.0).to_bits(), std::f64::consts::E.to_bits());
        assert_eq!(excel_log(1.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(excel_log(std::f64::consts::E), 1.0);
        assert_eq!(excel_log10(1000.0), 3.0);
    }

    // Golden (x_bits, cr_bits) pulled from the W108-A corpus. These pin the
    // *portable* faithful core (`exp_portable`/`log_portable` and the
    // expm1/log1p/sqrt kernels) to the correctly-rounded value. Excel parity of
    // EXP/LN is covered by `exp_and_ln_against_live_excel`.
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
        // COS small-argument shortcut: strictly below 2^-26 -> exactly 1.0;
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

//! Excel's exponential, logarithm and power chains on the x87 (dc-96l.3): a clean-room
//! implementation of the interface in `C:\Work\DnaCalc\parked\licence\nt-chains-interface.md`,
//! written without reading the code it replaces.
//!
//! # Sources
//!
//! - Intel 64 and IA-32 Architectures Software Developer's Manual, vol. 1 ch. 8 (the x87 FPU, its
//!   register format, control word and rounding) and vol. 2 (the instructions `FLDL2E`, `FLDLN2`,
//!   `FLDLG2`, `FLD1`, `FMUL`, `FSUB`, `FADD`, `FDIV`, `FRNDINT`, `F2XM1`, `FSCALE`, `FYL2X`,
//!   `FST`): the textbook x87 sequences `e^x = 2^(x log2 e)`, split into an integer part (`FRNDINT`,
//!   round to nearest) and a fraction in [-1/2, 1/2] for `F2XM1`, rejoined with `FSCALE`;
//!   `ln x = ln 2 * log2 x` and `log10 x = log10 2 * log2 x` by `FYL2X`.
//! - IEEE 754-2008 for binary64 and the rounding of the final store.
//! - Excel measurements: the control word (precision 64, round to nearest), the store rounded
//!   toward zero at GAMMA.DIST's series step, and the 249 EXP and LN witnesses of
//!   `x87_excel_ground_truth.tsv`, all from the interface record; the special values as the
//!   interface states them. Scoring candidate sequences against the 136 EXP witnesses showed that
//!   a negative exponent is computed as the extended reciprocal of the positive one (132 of 136
//!   rows; the plain sequence gives 128); the four remaining rows are one unit in the last place
//!   away and listed in `mod.rs`. The LN sequence matches all 113 LN witnesses on the hardware.
//!
//! Each chain runs on [`Native`]: the x87 instructions on x86_64, the software x87 ([`soft87`]) on
//! wasm32 and under the `soft-x87` feature.
//!
//! [`soft87`]: super::soft87

use super::{CW, Ext80, Fpu, Native, ext_chs, ext_l2e, ext_lg2, ext_ln2, ext_one, load};

/// The control word with rounding toward zero (RC = 11), precision 64, exceptions masked.
const CW_RZ: u16 = CW | 0x0C00;

/// `2^t` for an extended `t >= 0`: `FRNDINT`, `F2XM1` of the fraction, `+1`, `FSCALE`.
fn pow2_positive<F: Fpu>(t: &Ext80) -> Ext80 {
    let whole = F::rndint(t, CW);
    let fraction = F::sub(t, &whole, CW);
    let m = F::add(&F::f2xm1(&fraction, CW), &ext_one(), CW);
    F::scale(&m, &whole, CW)
}

/// `2^t` for an extended `t`; a negative `t` as the extended reciprocal of `2^-t` (measured: the
/// EXP witnesses of small negative arguments are the reciprocals' roundings).
fn pow2<F: Fpu>(t: &Ext80) -> Ext80 {
    if t.0[9] & 0x80 != 0 {
        F::div(&ext_one(), &pow2_positive::<F>(&ext_chs(t, CW)), CW)
    } else {
        pow2_positive::<F>(t)
    }
}

/// `e^t` for an extended `t`: `2^(t log2 e)`.
fn exp_ext<F: Fpu>(t: &Ext80) -> Ext80 {
    pow2::<F>(&F::mul(t, &ext_l2e(), CW))
}

fn exp_with<F: Fpu>(x: f64, cw: u16) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x == f64::INFINITY {
        return x;
    }
    if x == f64::NEG_INFINITY {
        return 0.0;
    }
    F::store(&exp_ext::<F>(&load(x)), cw)
}

/// Excel's `EXP`.
pub(crate) fn excel_exp(x: f64) -> f64 {
    exp_with::<Native>(x, CW)
}

/// Excel's `EXP` with the final rounding toward zero.
pub(crate) fn excel_exp_rz(x: f64) -> f64 {
    exp_with::<Native>(x, CW_RZ)
}

/// `exp(t - x - c)` with the difference in the 64-bit registers, stored once. GAMMA.DIST's
/// prefactor: storing `t - x` puts a slow line of amplitude 5 units in the residual that Excel's
/// answers do not have.
pub(crate) fn excel_exp_difference(t: f64, x: f64, c: f64) -> f64 {
    exp_difference::<Native>(t, x, c)
}

fn exp_difference<F: Fpu>(t: f64, x: f64, c: f64) -> f64 {
    let difference = F::sub(&F::sub(&load(t), &load(x), CW), &load(c), CW);
    F::store(&exp_ext::<F>(&difference), CW)
}

/// `exp(start - terms[0] - terms[1] ...)` as Excel forms the exponential of a difference. Each
/// subtraction is stored and what the store dropped is kept: the exponential is the public EXP of
/// the last stored difference, then one stored multiply by the stored `1 + dropped` per subtraction,
/// in order.
///
/// Measured on POISSON.DIST's prefactor route: at `x = 2` and negligible `lambda` the answers sit a
/// sawtooth of one unit in the last place, with period 1/8 of the significand, away from one
/// rounding of `exp(t - c)`. That is the double rounding of `exp(hi) * (1 + lo)` with `lo = 2^-49`,
/// the part of GAMMALN(3) below the grid of `t`. At `x = 1` a run of 7,500 adjacent means then has
/// no miss.
pub(crate) fn excel_exp_compensated(start: f64, terms: &[f64]) -> f64 {
    exp_compensated::<Native>(start, terms)
}

fn exp_compensated<F: Fpu>(start: f64, terms: &[f64]) -> f64 {
    let mut stored = start;
    let mut factors = [1.0_f64; 2];
    for (term, factor) in terms.iter().zip(factors.iter_mut()) {
        let difference = F::sub(&load(stored), &load(*term), CW);
        stored = F::store(&difference, CW);
        let dropped = F::store(&F::sub(&difference, &load(stored), CW), CW);
        *factor = F::store(&F::add(&ext_one(), &load(dropped), CW), CW);
    }
    let mut value = exp_with::<F>(stored, CW);
    for factor in &factors[..terms.len()] {
        value = F::store(&F::mul(&load(value), &load(*factor), CW), CW);
    }
    value
}

/// `exp(start - terms...)` with one correction: the difference is stored after every subtraction,
/// the same subtractions run on in the registers, and the exponential is the public EXP of the last
/// stored difference times the stored `1 + (registers - stored)`. BINOM.DIST's normalizer.
pub(crate) fn excel_exp_running(start: f64, terms: &[f64]) -> f64 {
    exp_running::<Native>(start, terms)
}

fn exp_running<F: Fpu>(start: f64, terms: &[f64]) -> f64 {
    let mut stored = start;
    let mut registers = load(start);
    for term in terms {
        stored = F::store(&F::sub(&load(stored), &load(*term), CW), CW);
        registers = F::sub(&registers, &load(*term), CW);
    }
    let dropped = F::store(&F::sub(&registers, &load(stored), CW), CW);
    let factor = F::store(&F::add(&ext_one(), &load(dropped), CW), CW);
    F::store(&F::mul(&load(exp_with::<F>(stored, CW)), &load(factor), CW), CW)
}

/// `stirling_error(n)` from its asymptotic series, ten terms in the registers, stored once. Ten terms
/// because `n = 7` reads 2.2e-17 under the true value, where the tenth term leaves 2.0e-17 and the
/// ninth and eleventh are 1.0e-16 and 4.5e-18 over. From 8 on the answers in the series range hold it
/// to the true value within a few units in the last place.
pub(crate) fn excel_stirling_series(n: f64) -> f64 {
    stirling_series::<Native>(n)
}

fn stirling_series<F: Fpu>(n: f64) -> f64 {
    const TERMS: [f64; 10] = [
        1.0 / 12.0,
        -1.0 / 360.0,
        1.0 / 1260.0,
        -1.0 / 1680.0,
        1.0 / 1188.0,
        -691.0 / 360360.0,
        1.0 / 156.0,
        -3617.0 / 122400.0,
        43867.0 / 244188.0,
        -174611.0 / 125400.0,
    ];
    let t = F::div(&ext_one(), &F::mul(&load(n), &load(n), CW), CW);
    let mut w = load(TERMS[9]);
    for term in TERMS[..9].iter().rev() {
        w = F::add(&F::mul(&w, &t, CW), &load(*term), CW);
    }
    F::store(&F::div(&w, &load(n), CW), CW)
}

/// The deviance `x ln(x/np) + np - x` inside its series range. With `d = x - np` and `x + np` stored
/// and `V` their stored quotient, `S` is the stored sum of `V` and the tail `v^3/3 + v^5/5 + ...` of
/// `atanh v`, so that `ln(x/np) = 2 S`. The answer is the stored sum of the stored
/// `(np - x) + st(2 x S) - st(2 x V)`, then `d`, then the stored `d V`.
///
/// Read from POISSON.DIST on runs of adjacent means, one rounding at a time. Each stored sum leaves
/// a sawtooth in the exponent's error, and the sawtooth's frequency names the sum: the tail's
/// rounding to `x` times the grid of `V` at `x = 100`, `x + np` stored at `x = 1024`, the products at
/// `x = 501`. All 1,000 rows at `x = 100` and 1,199 of 1,200 at `x = 1024` agree.
pub(crate) fn excel_deviance_series(x: f64, np: f64) -> f64 {
    deviance_series::<Native>(x, np)
}

fn deviance_series<F: Fpu>(x: f64, np: f64) -> f64 {
    let (fx, fnp) = (load(x), load(np));
    let d = F::store(&F::sub(&fx, &fnp, CW), CW);
    let v = F::div(&load(d), &load(F::store(&F::add(&fx, &fnp, CW), CW)), CW);
    let w = F::mul(&v, &v, CW);
    let (mut tail, mut power, mut j) = (load(0.0), v, 1.0);
    loop {
        power = F::mul(&power, &w, CW);
        let next = F::add(&tail, &F::div(&power, &load(2.0 * j + 1.0), CW), CW);
        if next.0 == tail.0 {
            break;
        }
        tail = next;
        j += 1.0;
    }
    let quotient = load(F::store(&v, CW));
    let sum = load(F::store(&F::add(&quotient, &tail, CW), CW));
    let twice_x = load(2.0 * x);
    let near = F::add(
        &load(F::store(&F::sub(&fnp, &fx, CW), CW)),
        &load(F::store(&F::mul(&twice_x, &sum, CW), CW)),
        CW,
    );
    let near = F::sub(&near, &load(F::store(&F::mul(&twice_x, &quotient, CW), CW)), CW);
    let near = F::add(&load(F::store(&near, CW)), &load(d), CW);
    let product = F::store(&F::mul(&load(d), &quotient, CW), CW);
    F::store(&F::add(&near, &load(product), CW), CW)
}

/// `constant * log2 x` by `FYL2X`, with the logarithm's special values.
fn log_with<F: Fpu>(x: f64, constant: &Ext80) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return x;
    }
    if x == 1.0 {
        return 0.0;
    }
    F::store(&F::fyl2x(constant, &load(x), CW), CW)
}

/// Excel's `LN`.
pub(crate) fn excel_log(x: f64) -> f64 {
    log_with::<Native>(x, &ext_ln2())
}

/// Excel's `LOG10`.
pub(crate) fn excel_log10(x: f64) -> f64 {
    log_with::<Native>(x, &ext_lg2())
}

/// `base^exponent` with every intermediate extended: `2^(exponent log2 base)`, one store.
pub(crate) fn excel_pow_x87_direct(base: f64, exponent: f64) -> f64 {
    pow_direct::<Native>(base, exponent)
}

fn pow_direct<F: Fpu>(base: f64, exponent: f64) -> f64 {
    let t = F::fyl2x(&load(exponent), &load(base), CW);
    F::store(&pow2::<F>(&t), CW)
}

/// GEOMEAN's current root reconstruction from its caller's stored product.
pub(crate) fn excel_root_x87(product: f64, count: usize) -> f64 {
    root::<Native>(product, count)
}

fn root<F: Fpu>(product: f64, count: usize) -> f64 {
    // Original singleton controls return the stored product unchanged.
    if count == 1 {
        return product;
    }
    // R28 normalized-product controls distinguish this exact exponent step
    // independently of the private root. Adjust frexp to a significand in
    // [1, 2), then divide its integer exponent toward zero (not floor).
    // The caller supplies a positive normal stored product and count > 0.
    let exponent = ((product.to_bits() >> 52) & 0x7ff) as i64 - 1023;
    let n = count as i64;
    let scale = exponent / n;
    let reduced = libm::scalbn(product, -(scale * n) as i32);
    // Paired original LN/EXP controls isolate these assignments. Removing any
    // one publication contradicts the observed reduced roots. The reciprocal
    // is assigned before the multiply; direct division is distinguished too.
    let log = log_with::<F>(reduced, &ext_ln2());
    let reciprocal = F::store(&F::div(&ext_one(), &load(count as f64), CW), CW);
    let mean = F::store(&F::mul(&load(log), &load(reciprocal), CW), CW);
    libm::scalbn(F::store(&exp_ext::<F>(&load(mean)), CW), scale as i32)
}

#[cfg(test)]
mod tests {
    use super::super::Soft;
    use super::*;

    /// The software x87 gives the same bits as the backend the entry points run, on the values the
    /// interface names.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chains_run_on_the_software_x87() {
        assert_eq!(exp_with::<Soft>(1.0, CW), core::f64::consts::E);
        assert_eq!(exp_with::<Soft>(0.0, CW), 1.0);
        assert_eq!(log_with::<Soft>(core::f64::consts::E, &ext_ln2()), 1.0);
        assert_eq!(log_with::<Soft>(1000.0, &ext_lg2()), 3.0);
        assert_eq!(pow_direct::<Soft>(2.0, 10.0), 1024.0);
        assert_eq!(root::<Soft>(16.0, 2), 4.0);
        assert!(exp_with::<Soft>(1.5, CW_RZ) <= exp_with::<Soft>(1.5, CW));
        assert_eq!(exp_compensated::<Soft>(1.5, &[0.25]), exp_with::<Soft>(1.25, CW));
        assert_eq!(exp_running::<Soft>(1.5, &[0.25]), exp_with::<Soft>(1.25, CW));
        let exact = 4.0 * (4.0_f64 / 3.0).ln() + 3.0 - 4.0;
        assert!((deviance_series::<Soft>(4.0, 3.0) - exact).abs() < 1e-15);
    }
}

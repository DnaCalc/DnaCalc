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

/// GEOMEAN's root: the binary64 product, then `2^(log2(product) / n)` in extended precision.
pub(crate) fn excel_root_x87(values: &[f64]) -> f64 {
    root::<Native>(values)
}

fn root<F: Fpu>(values: &[f64]) -> f64 {
    let product: f64 = values.iter().product();
    let log2 = F::fyl2x(&ext_one(), &load(product), CW);
    let t = F::div(&log2, &load(values.len() as f64), CW);
    F::store(&pow2::<F>(&t), CW)
}

/// BINOM.DIST's mass exponential: `e^(lc - lf/2)` with the difference extended.
pub(crate) fn excel_binom_pmf_exp(lc: f64, lf: f64) -> f64 {
    pmf_exp::<Native>(lc, lf)
}

fn pmf_exp<F: Fpu>(lc: f64, lf: f64) -> f64 {
    let half = F::div(&load(lf), &load(2.0), CW);
    let t = F::sub(&load(lc), &half, CW);
    F::store(&exp_ext::<F>(&t), CW)
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
        assert_eq!(root::<Soft>(&[2.0, 8.0]), 4.0);
        assert!(exp_with::<Soft>(1.5, CW_RZ) <= exp_with::<Soft>(1.5, CW));
    }
}

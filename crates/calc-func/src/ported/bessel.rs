//! BESSELI, BESSELJ, BESSELK and BESSELY: a clean-room implementation (dc-96l.3), written without
//! reading the code it replaces, from published mathematics and from measurements of Excel.
//!
//! # Sources
//!
//! - Abramowitz and Stegun, *Handbook of Mathematical Functions* (1964), 9.8.1 to 9.8.8: the
//!   polynomial approximations of I0, I1, K0 and K1 used here, with their published coefficients.
//! - J0, J1, Y0 and Y1 use the existing `libm` dependency's fdlibm methods.
//!   Their agreement with Excel remains open.
//! - The three-term recurrence `C(n+1) = (2n/x) C(n) - C(n-1)` (A&S 9.1.27, 9.6.26), run forward
//!   for Y and K and for J above its order. J below its order delegates to libm::jn.
//! - I of order at least two uses the backward recurrence (DLMF 10.29.1,
//!   10.74(iv)), normalized to the same A&S I0 approximation as order zero.
//!   Its defining positive series (DLMF 10.25.2) handles a normalization ratio
//!   or I0 value outside binary64's range.
//!   No Excel parity is claimed for these generic public-method bodies.
//!
//! # What was measured in Excel (16.0, 64-bit, 2026-10-30)
//!
//! - The order is truncated; a negative order, or one above 10,000,000 after truncation, is #NUM!.
//! - Excel computes with subnormal operands and results flushed to zero (BESSELK(700,2) equals
//!   BESSELK(700,0); BESSELK(706,0) is 0); [`D`] does the same.
//! - A result that is not finite is #NUM!.
//!
//! The generic J/Y and higher-order I bodies are published methods, not an
//! identification of Excel's program. Exact disagreement counts are retained
//! in the witness and corpus reports.
//!

use crate::ported::excel_numeric::{excel_exp, excel_log};
use crate::value::WorksheetErrorCode;
use core::ops::{Add, Div, Mul, Neg, Sub};

/// A binary64 that computes as Excel does here: subnormal operands and results are zero.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
struct D(f64);

impl D {
    fn new(v: f64) -> D {
        if v.is_subnormal() { D(0.0f64.copysign(v)) } else { D(v) }
    }
    fn abs(self) -> D {
        D(self.0.abs())
    }
    fn sqrt(self) -> D {
        D::new(self.0.sqrt())
    }
}

macro_rules! flushing_op {
    ($tr:ident, $f:ident, $op:tt) => {
        impl $tr for D {
            type Output = D;
            fn $f(self, rhs: D) -> D {
                D::new(D::new(self.0).0 $op D::new(rhs.0).0)
            }
        }
    };
}
flushing_op!(Add, add, +);
flushing_op!(Sub, sub, -);
flushing_op!(Mul, mul, *);
flushing_op!(Div, div, /);

impl Neg for D {
    type Output = D;
    fn neg(self) -> D {
        D(-self.0)
    }
}

fn exp(x: D) -> D {
    D::new(excel_exp(x.0))
}
fn ln(x: D) -> D {
    D::new(excel_log(x.0))
}
/// `c[0] + y (c[1] + y (c[2] + ...))`.
fn horner(c: &[f64], y: D) -> D {
    let mut r = D(c[c.len() - 1]);
    for &k in c[..c.len() - 1].iter().rev() {
        r = D(k) + y * r;
    }
    r
}

/// The largest order (after truncation).
const MAX_ORDER: f64 = 10_000_000.0;
type R = Result<D, WorksheetErrorCode>;

// --- I0, I1, K0, K1: A&S 9.8.1 to 9.8.8 -------------------------------------------------------

const I0_SMALL: [f64; 7] = [1.0, 3.5156229, 3.0899424, 1.2067492, 0.2659732, 0.0360768, 0.0045813];
const I0_LARGE: [f64; 9] = [
    0.39894228, 0.01328592, 0.00225319, -0.00157565, 0.00916281, -0.02057706, 0.02635537, -0.01647633,
    0.00392377,
];
const I1_SMALL: [f64; 7] = [0.5, 0.87890594, 0.51498869, 0.15084934, 0.02658733, 0.00301532, 0.00032411];
const I1_LARGE: [f64; 9] = [
    0.39894228, -0.03988024, -0.00362018, 0.00163801, -0.01031555, 0.02282967, -0.02895312, 0.01787654,
    -0.00420059,
];
const K0_SMALL: [f64; 7] = [-0.57721566, 0.42278420, 0.23069756, 0.03488590, 0.00262698, 0.00010750, 0.00000740];
const K0_LARGE: [f64; 7] = [1.25331414, -0.07832358, 0.02189568, -0.01062446, 0.00587872, -0.00251540, 0.00053208];
const K1_SMALL: [f64; 7] = [1.0, 0.15443144, -0.67278579, -0.18156897, -0.01919402, -0.00110404, -0.00004686];
const K1_LARGE: [f64; 7] = [1.25331414, 0.23498619, -0.03655620, 0.01504268, -0.00780353, 0.00325614, -0.00068245];

fn i0(x: D) -> D {
    let ax = x.abs();
    if ax.0 < 3.75 {
        let t = x / D(3.75);
        horner(&I0_SMALL, t * t)
    } else {
        exp(ax) / ax.sqrt() * horner(&I0_LARGE, D(3.75) / ax)
    }
}

fn i1(x: D) -> D {
    let ax = x.abs();
    if ax.0 < 3.75 {
        let t = x / D(3.75);
        x * horner(&I1_SMALL, t * t)
    } else {
        let r = exp(ax) / ax.sqrt() * horner(&I1_LARGE, D(3.75) / ax);
        if x.0 < 0.0 { -r } else { r }
    }
}

fn k0(x: D) -> D {
    if x.0 <= 2.0 {
        let h = x / D(2.0);
        -(ln(h) * i0(x)) + horner(&K0_SMALL, x * x / D(4.0))
    } else {
        exp(-x) / x.sqrt() * horner(&K0_LARGE, D(2.0) / x)
    }
}

fn k1(x: D) -> D {
    if x.0 <= 2.0 {
        let h = x / D(2.0);
        ln(h) * i1(x) + D(1.0) / x * horner(&K1_SMALL, x * x / D(4.0))
    } else {
        exp(-x) / x.sqrt() * horner(&K1_LARGE, D(2.0) / x)
    }
}

// --- J0, J1, Y0, Y1 ---------------------------------------------------------------------------

// Public-library bodies, with no Excel-derived polynomial or phase cutoff.
fn j0(x: D) -> R { Ok(D::new(libm::j0(x.0))) }
fn j1(x: D) -> R { Ok(D::new(libm::j1(x.0))) }
fn y0(x: D) -> R { Ok(D::new(libm::y0(x.0))) }
fn y1(x: D) -> R { Ok(D::new(libm::y1(x.0))) }

// --- orders two and up --------------------------------------------------------------------------

/// The order as Excel reads it: truncated; #NUM! when negative or above [`MAX_ORDER`].
fn order(n: f64) -> Result<u32, WorksheetErrorCode> {
    if !(n >= 0.0) {
        return Err(WorksheetErrorCode::Num);
    }
    let n = n.trunc();
    if n > MAX_ORDER {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(n as u32)
}

/// The forward recurrence from orders 0 and 1 up to `n`: `next = j (2/x) c - previous` for J and
/// Y, `next = previous + j (2/x) c` for K.
fn forward(x: D, n: u32, c0: D, c1: D, plus: bool) -> D {
    let tox = D(2.0) / x;
    let (mut before, mut c) = (c0, c1);
    for j in 1..n {
        let step = D(j as f64) * tox * c;
        let next = if plus { before + step } else { step - before };
        before = c;
        c = next;
    }
    c
}

/// I_n(x) from the defining positive series (DLMF 10.25.2).
/// Sum relative to its leading term; logarithms combine the normalizer and sum
/// so an underflowed leading term does not erase a normal result.
fn i_series(x: D, n: u32) -> D {
    let half = x.abs().0 / 2.0;
    if half == 0.0 { return D(0.0); }
    let leading_log = (n as f64) * libm::log(half) - libm::lgamma(n as f64 + 1.0);
    let maximum_log = libm::log(f64::MAX);
    if leading_log > maximum_log {
        return D(f64::INFINITY);
    }
    let quarter_square = half * half;
    let (mut term, mut sum, mut log_scale) = (1.0, 1.0, 0.0);
    let mut k = 0u32;
    loop {
        k += 1;
        let ratio = quarter_square / ((k as f64) * (n as f64 + k as f64));
        // Rescale only at the binary64 representation limit, never at a bound
        // selected from Excel answers. Both terms carry the same scale.
        if ratio > 1.0 && term > f64::MAX / ratio {
            log_scale += libm::log(sum);
            term /= sum;
            sum = 1.0;
        }
        term *= ratio;
        if sum > f64::MAX - term {
            sum *= 0.5;
            term *= 0.5;
            log_scale += std::f64::consts::LN_2;
        }
        let next = sum + term;
        if !next.is_finite() { return D(next); }
        // Every term is positive: an overflowing partial result proves that
        // the complete sum overflows. Rescaling alone would keep iterating
        // long after the represented Bessel value has exceeded binary64.
        if leading_log + log_scale + libm::log(next) > maximum_log {
            return D(f64::INFINITY);
        }
        if next == sum { break; }
        sum = next;
    }
    D::new(libm::exp(leading_log + log_scale + libm::log(sum)))
}

/// Backward recurrence normalized to the existing I0 approximation. Above x,
/// the ratio recurrence has derivative bounded by 1/4. The starting depth
/// leaves at least 53 such steps before order n; it is set by binary64's
/// precision, not by an Excel answer. Power-of-two rescaling is exact whenever
/// its operands remain representable.
fn i_normalized(x: D, n: u32) -> Option<D> {
    let ax = x.abs();
    let normalizer = i0(ax);
    if !normalizer.0.is_finite() { return None; }
    let start = n + ax.0.ceil() as u32 + f64::MANTISSA_DIGITS;
    let mut before = D(0.0);
    let mut current = D(1.0);
    let mut selected = D(0.0);
    // Half the binary64 exponent range, with its exact reciprocal.
    const LIMIT: D = D(f64::from_bits(0x5ff0_0000_0000_0000));
    const SCALE: D = D(f64::from_bits(0x1ff0_0000_0000_0000));
    for k in (1..=start).rev() {
        if k == n { selected = current; }
        let following = before + (D(2.0) * D(k as f64) / ax) * current;
        if !following.0.is_finite() { return None; }
        before = current;
        current = following;
        if current.0 > LIMIT.0 {
            before = before * SCALE;
            current = current * SCALE;
            selected = selected * SCALE;
            if k < n && selected.0 == 0.0 { return None; }
        }
    }
    let ratio = selected / current;
    // I_n may be normal even when I_n/I0 underflows. The positive series
    // carries its logarithmic scale through precisely that case.
    if ratio.0 == 0.0 { return None; }
    Some(normalizer * ratio)
}

fn odd_sign(x: D, n: u32, v: D) -> D {
    if x.0 < 0.0 && n % 2 == 1 { -v } else { v }
}

fn finish(v: R) -> Result<f64, WorksheetErrorCode> {
    let v = v?.0;
    if v.is_finite() { Ok(v) } else { Err(WorksheetErrorCode::Num) }
}

/// `BESSELJ(x, n)`.
pub fn besselj_kernel(x: f64, n: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() { return Err(WorksheetErrorCode::Num); }
    let n = order(n)?;
    let x = D::new(x);
    finish(match n {
        0 => j0(x),
        1 => j1(x),
        _ if x.0 == 0.0 => Ok(D(0.0)),
        _ => {
            let ax = x.abs();
            let v = if ax.0 > n as f64 { forward(ax, n, j0(ax)?, j1(ax)?, false) } else { D::new(libm::jn(n as i32, ax.0)) };
            Ok(odd_sign(x, n, v))
        }
    })
}

/// `BESSELY(x, n)`.
pub fn bessely_kernel(x: f64, n: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() { return Err(WorksheetErrorCode::Num); }
    let n = order(n)?;
    let x = D::new(x);
    if !(x.0 > 0.0) {
        return Err(WorksheetErrorCode::Num);
    }
    finish(match n {
        0 => y0(x),
        1 => y1(x),
        _ => Ok(forward(x, n, y0(x)?, y1(x)?, false)),
    })
}

/// `BESSELI(x, n)`.
pub fn besseli_kernel(x: f64, n: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() { return Err(WorksheetErrorCode::Num); }
    let n = order(n)?;
    let x = D::new(x);
    finish(Ok(match n {
        0 => i0(x),
        1 => i1(x),
        _ if x.0 == 0.0 => D(0.0),
        _ => odd_sign(x, n, i_normalized(x, n).unwrap_or_else(|| i_series(x, n))),
    }))
}

/// `BESSELK(x, n)`.
pub fn besselk_kernel(x: f64, n: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() { return Err(WorksheetErrorCode::Num); }
    let n = order(n)?;
    let x = D::new(x);
    if !(x.0 > 0.0) {
        return Err(WorksheetErrorCode::Num);
    }
    finish(Ok(match n {
        0 => k0(x),
        1 => k1(x),
        _ => forward(x, n, k0(x), k1(x), true),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Excel 16.0 (64-bit) readings, as their bits.
    fn excel(bits: u64) -> f64 {
        f64::from_bits(bits)
    }

    // Mathematical references from the defining series at 100 and 200 decimal
    // digits (identical binary64 rounding), not Excel readings.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn besseli_series_mathematical_controls() {
        for (x, n, expected) in [
            (1.0, 2.0, 0.13574766976703828_f64),
            (20.0, 10.0, 3540200.2090195213),
            // The leading term alone underflows; the complete sum is normal.
            (709.7, 1570.0, 6.766516533174238e-301),
            // The relative sum needs representation rescaling twice.
            (7000.0, 10000.0, 9.241488714987718e284),
        ] {
            let got = i_series(D(x), n as u32).0;
            assert!((got - expected).abs() <= expected.abs() * 2e-11,
                "I_{n}({x}): {got:e}, mathematical {expected:e}");
        }
        assert_eq!(besseli_kernel(f64::NAN, 2.0), Err(WorksheetErrorCode::Num));
        // The positive partial sum already exceeds MAX at term 14. This must
        // terminate without iterating toward the enormous series mode.
        assert_eq!(besseli_kernel(1e12, 2.0), Err(WorksheetErrorCode::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn spot_values_record_exact_controls_and_known_gaps() {
        assert_eq!(besselj_kernel(1.9, 2.0).unwrap().to_bits(), 0x3fd51d80cca30f39, "recorded Excel excel(0x3fd51d81390f83eb)");
        assert_eq!(besseli_kernel(1.5, 1.0), Ok(excel(0x3fef69cfb6bc3907)));
        assert_eq!(besselk_kernel(1.5, 1.0), Ok(excel(0x3fd1c0b8c63a3a82)));
        assert_eq!(bessely_kernel(100.0, 1.0).unwrap().to_bits(), 0xbf94dc7ab72edc1f, "recorded Excel -0.020372311933786909");
        assert_eq!(bessely_kernel(0.5, 0.0).unwrap().to_bits(), 0xbfdc72feb3b7b8a2, "recorded Excel -0.44451873376270784");
        assert_eq!(besselj_kernel(0.0, 0.0).unwrap().to_bits(), 0x3ff0000000000000, "recorded Excel 1.00000000283141");
        assert_eq!(besselj_kernel(-2.5, 1.0).unwrap().to_bits(), 0xbfdfd063c84795fd, "recorded Excel -0.49709410250442176");
        assert_eq!(besseli_kernel(-2.5, 1.0), Ok(-2.5167162420253608));
        assert_eq!(besselk_kernel(700.0, 2.0), besselk_kernel(700.0, 0.0));
        assert_eq!(besselk_kernel(706.0, 0.0), Ok(0.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn domain_controls_and_known_besselj_underflow_gap() {
        use WorksheetErrorCode::Num;
        assert_eq!(besselj_kernel(1.0, -1.0), Err(Num));
        assert_eq!(besselj_kernel(1.0, -0.5), Err(Num));
        assert_eq!(besselj_kernel(5.0, 10_000_001.0), Err(Num));
        assert_eq!(besselj_kernel(5.0, 10_000_000.5), Ok(0.0));
        assert_eq!(bessely_kernel(0.0, 1.0), Err(Num));
        assert_eq!(besselk_kernel(-1.0, 1.0), Err(Num));
        assert_eq!(besselj_kernel(134217728.0, 0.0).unwrap().to_bits(), 0xbed7ff19ed9c47a0, "recorded Excel NUM; phase cutoff removed");
        assert_eq!(besselj_kernel(134217727.5, 0.0).unwrap().to_bits(), 0xbf03e23975421ed4, "recorded Excel -3.792512787637833E-05");
        assert_eq!(besseli_kernel(709.78, 0.0).unwrap().to_bits(), 0x7f8e96fc20a992f7, "recorded Excel NUM; empirical cutoff removed");
        // Excel records NUM; the public-library backward method underflows to zero.
        assert_eq!(besselj_kernel(1e-300, 2.0), Ok(0.0));
    }
}

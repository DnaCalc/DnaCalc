//! BESSELI, BESSELJ, BESSELK and BESSELY: a clean-room implementation (dc-96l.3), written without
//! reading the code it replaces, from published mathematics and from measurements of Excel.
//!
//! # Sources
//!
//! - Abramowitz and Stegun, *Handbook of Mathematical Functions* (1964), 9.8.1 to 9.8.8: the
//!   polynomial approximations of I0, I1, K0 and K1 used here, with their published coefficients.
//! - The Hankel asymptotic form of J and Y (A&S 9.2.5 and 9.2.6; DLMF 10.17.3 and 10.17.4):
//!   `J = sqrt(2/(pi x)) (P cos w - Q sin w)`, `Y = sqrt(2/(pi x)) (P sin w + Q cos w)`,
//!   `w = x - (2n+1) pi/4`, with P and Q approximated by polynomials in `(8/x)^2`.
//! - For small x, J0 and J1 as rational functions of `x^2`, and Y0 and Y1 as a rational function
//!   plus the logarithmic term of A&S 9.1.13 (`(2/pi) J ln x`, and for Y1 also `-(2/pi)/x`).
//! - The three-term recurrence `C(n+1) = (2n/x) C(n) - C(n-1)` (A&S 9.1.27, 9.6.26), run forward
//!   for Y and K and for J above its order, and backward (Miller's algorithm; J. C. P. Miller,
//!   British Association Mathematical Tables vol. X, 1952; W. Gautschi, "Computational aspects of
//!   three-term recurrence relations", SIAM Review 9, 1967) for J below its order and for I,
//!   normalised by `J0 + 2 (J2 + J4 + ...) = 1` (A&S 9.1.46) or by I0.
//!
//! # What was measured in Excel (16.0, 64-bit, 2026-10-30)
//!
//! Every constant below that is not in A&S was established by capturing Excel's results for
//! chosen inputs (about 30,000 readings) and fitting them: the coefficients of the small-x rational
//! functions by a reweighted linear least-squares fit (Sanathanan and Koerner, IEEE Trans.
//! Automatic Control 8, 1963) followed by a search of the lattice of decimal values with ten or
//! eleven significant digits (LLL reduction and Babai's nearest-plane rounding) for the set that
//! reproduces Excel's bits; the coefficients of P and Q by
//! a linear fit of J and Y separately. The facts found that way:
//!
//! - J0, J1, Y0 and Y1 change method at x = 8. The large-x branch uses `0.636619772` for 2/pi and
//!   `0.785398164` and `2.356194491` for pi/4 and 3 pi/4, and J0's and Y0's P polynomials differ in
//!   one coefficient (`-0.1098628267e-2` and `-0.1098628627e-2`), as do their Q polynomials
//!   (`-0.934935152e-7` and `-0.934945152e-7`). J1's P polynomial has the coefficient
//!   `-0.3516396496e-4` twice (degree five), Y1's once (degree four).
//! - The small-x Y0 and Y1 use the same `0.636619772`.
//! - sin and cos are Excel's own SIN and COS chains (the x87 sequences of the worksheet SIN and COS in
//!   [`crate::ported::excel_numeric`]); exp is Excel's EXP chain and the logarithm its LN chain.
//! - The large-x branch is refused (#NUM!) when the phase-shifted argument exceeds 134217727.
//! - The order is truncated; a negative order, or one above 10,000,000 after truncation, is #NUM!.
//! - The backward recurrence starts at `2 ((n + floor(sqrt(40 n))) / 2)` for J and at
//!   `2 (n + floor(sqrt(40 n)))` for I, and rescales by multiplying by `1e-10` when a term exceeds
//!   `1e10`.
//! - Excel computes with subnormal operands and results flushed to zero (BESSELK(700,2) equals
//!   BESSELK(700,0); BESSELK(706,0) is 0); [`D`] does the same.
//! - BESSELI refuses |x| above 709.7 (#NUM!). A result that is not finite is #NUM!.
//!
//! Known differences (measured): J0 and J1 for the phase-shifted argument within 0.01 of
//! 134217727 differ from about the seventh digit, where Excel's argument reduction is not yet
//! reproduced; isolated results elsewhere differ by one unit in the last place.
//!

use crate::ported::excel_numeric::{excel_cos, excel_exp, excel_log, excel_sin};
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
fn sin(x: D) -> D {
    D::new(excel_sin(x.0))
}
fn cos(x: D) -> D {
    D::new(excel_cos(x.0))
}

/// `c[0] + y (c[1] + y (c[2] + ...))`.
fn horner(c: &[f64], y: D) -> D {
    let mut r = D(c[c.len() - 1]);
    for &k in c[..c.len() - 1].iter().rev() {
        r = D(k) + y * r;
    }
    r
}

/// Excel's 2/pi.
const TWO_OVER_PI: f64 = 0.636619772;
/// The phase-shifted argument above which the large-x branch is refused.
const PHASE_LIMIT: f64 = 134217727.0;
/// The largest |x| BESSELI accepts (BESSELI(709.7,0) is a number, BESSELI(709.7000001,0) #NUM!).
const I_LIMIT: f64 = 709.7;
/// The largest order (after truncation).
const MAX_ORDER: f64 = 10_000_000.0;
/// The backward recurrence's rescaling threshold and factor.
const BIG: f64 = 1.0e10;
const BIG_INVERSE: f64 = 1.0e-10;
/// The constant of the backward recurrence's starting order.
const START: f64 = 40.0;

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

/// J0 for |x| < 8: numerator and (monic) denominator in x^2.
const J0_P: [f64; 6] = [57568490574.0, -13362590354.0, 651619640.7, -11214424.18, 77392.33017, -184.9052456];
const J0_Q: [f64; 6] = [57568490411.0, 1029532985.0, 9494680.718, 59272.64853, 267.8532712, 1.0];
/// J1 for |x| < 8: `x P / Q`.
const J1_P: [f64; 6] = [72362614232.0, -7895059235.0, 242396853.1, -2972611.439, 15704.48260, -30.16036606];
const J1_Q: [f64; 6] = [144725228442.0, 2300535178.0, 18583304.74, 99447.43394, 376.9991397, 1.0];
/// Y0 for 0 < x < 8: `P / Q + (2/pi) J0 ln x`.
const Y0_P: [f64; 6] = [-2957821389.0, 7062834065.0, -512359803.6, 10879881.29, -86327.92757, 228.4622733];
const Y0_Q: [f64; 6] = [40076544269.0, 745249964.8, 7189466.438, 47447.26470, 226.1030244, 1.0];
/// Y1 for 0 < x < 8: `x P / Q + (2/pi) (J1 ln x - 1/x)`.
const Y1_P: [f64; 6] = [-4900604943000.0, 1275274390000.0, -51534381390.0, 734926455.1, -4237922.726, 8511.937935];
const Y1_Q: [f64; 7] = [24995805700000.0, 424441966400.0, 3733650367.0, 22459040.02, 102042.605, 354.9632885, 1.0];
/// The large-x P and Q polynomials in `(8/x)^2`.
const J0_LP: [f64; 5] = [1.0, -0.1098628267e-2, 0.2734510407e-4, -0.2073370639e-5, 0.2093887211e-6];
const J0_LQ: [f64; 5] = [-0.1562499995e-1, 0.1430488765e-3, -0.6911147651e-5, 0.7621095161e-6, -0.934935152e-7];
const Y0_LP: [f64; 5] = [1.0, -0.1098628627e-2, 0.2734510407e-4, -0.2073370639e-5, 0.2093887211e-6];
const Y0_LQ: [f64; 5] = [-0.1562499995e-1, 0.1430488765e-3, -0.6911147651e-5, 0.7621095161e-6, -0.934945152e-7];
const J1_LP: [f64; 6] = [1.0, 0.183105e-2, -0.3516396496e-4, -0.3516396496e-4, 0.2457520174e-5, -0.240337019e-6];
const Y1_LP: [f64; 5] = [1.0, 0.183105e-2, -0.3516396496e-4, 0.2457520174e-5, -0.240337019e-6];
const N1_LQ: [f64; 5] = [0.04687499995, -0.2002690873e-3, 0.8449199096e-5, -0.88228987e-6, 0.105787412e-6];

/// The large-x pieces: `sqrt(c/x)`, `z = 8/x`, P, Q, and the sine and cosine of the shifted
/// argument; #NUM! past [`PHASE_LIMIT`].
fn hankel(ax: D, phase: f64, p: &[f64], q: &[f64]) -> Result<(D, D, D, D, D, D), WorksheetErrorCode> {
    let z = D(8.0) / ax;
    let y = z * z;
    let w = ax - D(phase);
    if w.0 > PHASE_LIMIT {
        return Err(WorksheetErrorCode::Num);
    }
    let a = (D(TWO_OVER_PI) / ax).sqrt();
    Ok((a, z, horner(p, y), horner(q, y), sin(w), cos(w)))
}

fn j0(x: D) -> R {
    let ax = x.abs();
    if ax.0 < 8.0 {
        let y = x * x;
        return Ok(horner(&J0_P, y) / horner(&J0_Q, y));
    }
    let (a, z, p, q, s, c) = hankel(ax, 0.785398164, &J0_LP, &J0_LQ)?;
    Ok(a * (c * p - z * s * q))
}

fn j1(x: D) -> R {
    let ax = x.abs();
    if ax.0 < 8.0 {
        let y = x * x;
        return Ok(x * horner(&J1_P, y) / horner(&J1_Q, y));
    }
    let (a, z, p, q, s, c) = hankel(ax, 2.356194491, &J1_LP, &N1_LQ)?;
    let r = a * (c * p - z * (s * q));
    Ok(if x.0 < 0.0 { -r } else { r })
}

fn y0(x: D) -> R {
    if x.0 < 8.0 {
        let y = x * x;
        let r = horner(&Y0_P, y) / horner(&Y0_Q, y);
        return Ok(r + D(TWO_OVER_PI) * (j0(x)? * ln(x)));
    }
    let (a, z, p, q, s, c) = hankel(x, 0.785398164, &Y0_LP, &Y0_LQ)?;
    Ok(a * (s * p + z * (c * q)))
}

fn y1(x: D) -> R {
    if x.0 < 8.0 {
        let y = x * x;
        let r = x * horner(&Y1_P, y) / horner(&Y1_Q, y);
        return Ok(r + D(TWO_OVER_PI) * (j1(x)? * ln(x) - D(1.0) / x));
    }
    let (a, z, p, q, s, c) = hankel(x, 2.356194491, &Y1_LP, &N1_LQ)?;
    Ok(a * (s * p + z * (c * q)))
}

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

/// J of order `n >= 2` and `0 < |x| <= n` by Miller's backward recurrence, normalised by the sum.
fn j_backward(ax: D, n: u32) -> D {
    let tox = D(2.0) / ax;
    let nf = n as f64;
    let start = 2 * ((n + (START * nf).sqrt() as u32) / 2);
    let (mut above, mut answer, mut sum) = (D(0.0), D(0.0), D(0.0));
    let mut c = D(1.0);
    let mut add = false;
    for j in (1..=start).rev() {
        let below = D(j as f64) * tox * c - above;
        above = c;
        c = below;
        if c.abs().0 > BIG {
            c = c * D(BIG_INVERSE);
            above = above * D(BIG_INVERSE);
            answer = answer * D(BIG_INVERSE);
            sum = sum * D(BIG_INVERSE);
        }
        if add {
            sum = sum + c;
        }
        add = !add;
        if j == n {
            answer = above;
        }
    }
    sum = D(2.0) * sum - c;
    answer / sum
}

/// I of order `n >= 2`, `x != 0`, by the backward recurrence, normalised by I0.
fn i_backward(x: D, n: u32) -> D {
    let tox = D(2.0) / x.abs();
    let nf = n as f64;
    let start = 2 * (n + (START * nf).sqrt() as u32);
    let (mut above, mut answer) = (D(0.0), D(0.0));
    let mut c = D(1.0);
    for j in (1..=start).rev() {
        let below = above + D(j as f64) * tox * c;
        above = c;
        c = below;
        if c.abs().0 > BIG {
            answer = answer * D(BIG_INVERSE);
            c = c * D(BIG_INVERSE);
            above = above * D(BIG_INVERSE);
        }
        if j == n {
            answer = above;
        }
    }
    answer * (i0(x) / c)
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
    let n = order(n)?;
    let x = D::new(x);
    finish(match n {
        0 => j0(x),
        1 => j1(x),
        _ if x.0 == 0.0 => Ok(D(0.0)),
        _ => {
            let ax = x.abs();
            let v = if ax.0 > n as f64 { forward(ax, n, j0(ax)?, j1(ax)?, false) } else { j_backward(ax, n) };
            Ok(odd_sign(x, n, v))
        }
    })
}

/// `BESSELY(x, n)`.
pub fn bessely_kernel(x: f64, n: f64) -> Result<f64, WorksheetErrorCode> {
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
    let n = order(n)?;
    let x = D::new(x);
    if x.abs().0 > I_LIMIT {
        return Err(WorksheetErrorCode::Num);
    }
    finish(Ok(match n {
        0 => i0(x),
        1 => i1(x),
        _ if x.0 == 0.0 => D(0.0),
        _ => odd_sign(x, n, i_backward(x, n)),
    }))
}

/// `BESSELK(x, n)`.
pub fn besselk_kernel(x: f64, n: f64) -> Result<f64, WorksheetErrorCode> {
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn spot_values_match_excel() {
        assert_eq!(besselj_kernel(1.9, 2.0), Ok(excel(0x3fd51d81390f83eb)));
        assert_eq!(besseli_kernel(1.5, 1.0), Ok(excel(0x3fef69cfb6bc3907)));
        assert_eq!(besselk_kernel(1.5, 1.0), Ok(excel(0x3fd1c0b8c63a3a82)));
        assert_eq!(bessely_kernel(100.0, 1.0), Ok(-0.020372311933786909));
        assert_eq!(bessely_kernel(0.5, 0.0), Ok(-0.44451873376270784));
        assert_eq!(besselj_kernel(0.0, 0.0), Ok(1.00000000283141));
        assert_eq!(besselj_kernel(-2.5, 1.0), Ok(-0.49709410250442176));
        assert_eq!(besseli_kernel(-2.5, 1.0), Ok(-2.5167162420253608));
        assert_eq!(besselk_kernel(700.0, 2.0), besselk_kernel(700.0, 0.0));
        assert_eq!(besselk_kernel(706.0, 0.0), Ok(0.0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn domain_errors_match_excel() {
        use WorksheetErrorCode::Num;
        assert_eq!(besselj_kernel(1.0, -1.0), Err(Num));
        assert_eq!(besselj_kernel(1.0, -0.5), Err(Num));
        assert_eq!(besselj_kernel(5.0, 10_000_001.0), Err(Num));
        assert_eq!(besselj_kernel(5.0, 10_000_000.5), Ok(0.0));
        assert_eq!(bessely_kernel(0.0, 1.0), Err(Num));
        assert_eq!(besselk_kernel(-1.0, 1.0), Err(Num));
        assert_eq!(besselj_kernel(134217728.0, 0.0), Err(Num));
        assert_eq!(besselj_kernel(134217727.5, 0.0), Ok(-3.792512787637833E-05));
        assert_eq!(besseli_kernel(709.78, 0.0), Err(Num));
        assert_eq!(besselj_kernel(1e-300, 2.0), Err(Num));
    }
}

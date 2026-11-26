//! The complex-number engineering functions' arithmetic, parsing and text rendering, ported from
//! OxFunc functions/complex_family.rs; the argument reading lives in the engineering family.

use crate::ported::excel_numeric::{
    crt_atan2, crt_cos, crt_cosh, crt_exp, crt_hypot, crt_log, crt_pow, crt_sin, crt_sinh,
    excel_x87_div, excel_x87_sub,
};
use crate::value::WorksheetErrorCode;

/// Decimal text of digits with at most one point and an optional exponent, read to its 15 leading
/// significant digits and converted to the nearest binary64; `None` outside that form or with a
/// scale outside -308..308 (OxFunc coercion.rs parse_ascii_decimal_15).
fn parse_ascii_decimal_15(text: &str) -> Option<f64> {
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(index) => {
            let mut exponent = &text[index + 1..];
            let negative = exponent.starts_with('-');
            if exponent.starts_with(['+', '-']) {
                exponent = &exponent[1..];
            }
            if exponent.is_empty() || !exponent.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let magnitude = exponent.bytes().fold(0_i64, |n, b| {
                n.saturating_mul(10).saturating_add((b - b'0') as i64)
            });
            (
                &text[..index],
                if negative { -magnitude } else { magnitude },
            )
        }
        None => (text, 0),
    };
    let mut significand = 0_u64;
    let mut significant_digits = 0_i64;
    let mut fractional_digits = 0_i64;
    let mut after_point = false;
    let mut any_digit = false;
    for byte in mantissa.bytes() {
        if byte == b'.' && !after_point {
            after_point = true;
            continue;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
        any_digit = true;
        if after_point {
            fractional_digits += 1;
        }
        if byte != b'0' || significant_digits > 0 {
            if significant_digits < 15 {
                significand = significand * 10 + (byte - b'0') as u64;
            }
            significant_digits += 1;
        }
    }
    if !any_digit {
        return None;
    }
    let position = exponent
        .saturating_sub(fractional_digits)
        .saturating_add(significant_digits);
    if !(-308..=308).contains(&position) {
        return None;
    }
    if significand == 0 {
        return Some(0.0);
    }
    let scale = exponent
        .saturating_sub(fractional_digits)
        .saturating_add((significant_digits - 15).max(0));
    format!("{significand}e{scale}").parse().ok()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ParsedComplex {
    pub(crate) re: f64,
    pub(crate) im: f64,
    pub(crate) suffix: Option<char>,
}
impl ParsedComplex {
    pub(crate) const fn new(re: f64, im: f64, suffix: Option<char>) -> Self {
        Self { re, im, suffix }
    }
    pub(crate) fn add(self, other: Self) -> Self {
        Self::new(self.re + other.re, self.im + other.im, None)
    }
    pub(crate) fn sub(self, other: Self) -> Self {
        Self::new(self.re - other.re, self.im - other.im, None)
    }
    pub(crate) fn mul(self, other: Self) -> Self {
        Self::new(
            self.re * other.re - self.im * other.im,
            self.re * other.im + self.im * other.re,
            None,
        )
    }
    pub(crate) fn div(self, other: Self) -> Result<Self, WorksheetErrorCode> {
        if other.re == 0.0 && other.im == 0.0 {
            return Err(WorksheetErrorCode::Num);
        }
        // Scale by the larger denominator component before forming the quotient. The
        // algebraically equivalent squared-denominator expression loses tiny/huge inputs
        // and has different rounding on ordinary inputs. W111's independent real and
        // complex-text captures distinguish these operation graphs, including the small
        // imaginary residual in (3e150+4e150i)/(3e150+4e150i).
        let (re, im) = if other.re.abs() >= other.im.abs() {
            let ratio = other.im / other.re;
            let denominator = other.re + other.im * ratio;
            (
                (self.re + self.im * ratio) / denominator,
                (self.im - self.re * ratio) / denominator,
            )
        } else {
            let ratio = other.re / other.im;
            let denominator = other.im + other.re * ratio;
            (
                (self.re * ratio + self.im) / denominator,
                (self.im * ratio - self.re) / denominator,
            )
        };
        Ok(Self::quotient(re, im))
    }
    /// The reciprocal functions form one reciprocal of the scaled denominator
    /// and multiply the other component by it. Generic complex division forms
    /// two quotients instead; those are observably different after rounding.
    pub(crate) fn reciprocal(self) -> Result<Self, WorksheetErrorCode> {
        if self.re == 0.0 && self.im == 0.0 {
            // ubs:ignore — exact zero denominator, not approximate comparison
            return Err(WorksheetErrorCode::Num);
        }
        let (re, im) = if self.re.abs() >= self.im.abs() {
            let ratio = self.im / self.re;
            let reciprocal = 1.0 / (self.re + self.im * ratio);
            (reciprocal, -ratio * reciprocal)
        } else {
            let ratio = self.re / self.im;
            let reciprocal = 1.0 / (self.im + self.re * ratio);
            (ratio * reciprocal, -reciprocal)
        };
        Ok(Self::quotient(re, im))
    }
    fn quotient(re: f64, im: f64) -> Self {
        // Arithmetic underflow is a zero component before text assembly. This differs
        // from formatting a nonzero normal input whose 15-digit decimal falls below MIN.
        let normal_or_zero = |x: f64| if x.abs() < f64::MIN_POSITIVE { 0.0 } else { x };
        Self::new(normal_or_zero(re), normal_or_zero(im), None)
    }
    pub(crate) fn conj(self) -> Self {
        Self::new(self.re, -self.im, None)
    }
    pub(crate) fn abs(self) -> f64 {
        crt_hypot(self.re, self.im)
    }
    pub(crate) fn argument(self) -> Result<f64, WorksheetErrorCode> {
        let re = normalize_zero(self.re);
        let im = normalize_zero(self.im);
        if re == 0.0 {
            return if im == 0.0 {
                Err(WorksheetErrorCode::Div0)
            } else {
                Ok(im.signum() * std::f64::consts::FRAC_PI_2)
            };
        }
        let ratio = im / re;
        if ratio.is_infinite() {
            return Err(WorksheetErrorCode::Div0);
        }
        let base = crate::ported::excel_numeric::excel_atan_reduced(ratio);
        let result = if re < 0.0 {
            if im < 0.0 {
                base - std::f64::consts::PI
            } else {
                base + std::f64::consts::PI
            }
        } else {
            base
        };
        Ok(if result.abs() < f64::MIN_POSITIVE {
            0.0
        } else {
            result
        })
    }
    pub(crate) fn exp(self) -> Self {
        // Excel saturates the real exponential at the largest double (IMEXP("710+i") is
        // 9.71297746009434E+307+1.51270661257499E+308i, dc-j9k.357) and flushes a subnormal
        // exp(re) to 0 before multiplying by circular factors (IMEXP("-709+1e9i") = "0", dc-shqi1).
        let flush = |x: f64| if x.abs() < f64::MIN_POSITIVE { 0.0 } else { x };
        let s = crate::ported::excel_numeric::excel_exp(self.re);
        let s = if s == f64::INFINITY { f64::MAX } else { flush(s) };
        // Past 2^27 both circular factors are the largest double, whatever the sign (dc-j9k.357 sweep).
        let (sin, cos) = if self.im.abs() >= 134_217_728.0 {
            (f64::MAX, f64::MAX)
        } else {
            (
                crate::ported::excel_numeric::excel_sin(self.im),
                crate::ported::excel_numeric::excel_cos(self.im),
            )
        };
        Self::new(flush(crate::ported::excel_numeric::excel_x87_mul(s, cos)), flush(crate::ported::excel_numeric::excel_x87_mul(s, sin)), None)
    }
    pub(crate) fn ln(self) -> Result<Self, WorksheetErrorCode> {
        // Excel (dc-j9k.175, live probes): the squares flush subnormal results; a sum that is 0 or
        // infinite is #NUM!, and so is an overflowing im/re ratio (1e-300-1.8e8i). Only when both parts are
        // nonzero: a real or pure imaginary argument takes the real route (IMLN("1e-300") = -690.78).
        let square = |x: f64| {
            let value = x * x;
            if value < f64::MIN_POSITIVE { 0.0 } else { value }
        };
        let sum = square(self.re) + square(self.im);
        if self.re != 0.0 && self.im != 0.0 && (sum == 0.0 || !sum.is_finite() || (self.im / self.re).is_infinite()) {
            return Err(WorksheetErrorCode::Num);
        }
        let mag = self.abs();
        if mag == 0.0 {
            return Err(WorksheetErrorCode::Num);
        }
        let im = crt_atan2(self.im, self.re);
        Ok(Self::new(crt_log(mag), if im.abs() < f64::MIN_POSITIVE { 0.0 } else { im }, None))
    }
    pub(crate) fn sqrt(self) -> Self {
        // IMSQRT has a distinct magnitude graph from IMABS: square each component with
        // subnormal arithmetic results flushed, add, then take two square roots. The
        // retained W111 term/sum controls also distinguish finite-term sum overflow
        // (published zero) from an overflowing individual square (published #NUM!).
        let square = |x: f64| {
            let value = x * x;
            if value < f64::MIN_POSITIVE {
                0.0
            } else {
                value
            }
        };
        let re_square = square(self.re);
        let im_square = square(self.im);
        let sum = re_square + im_square;
        if re_square.is_finite() && im_square.is_finite() && sum.is_infinite() {
            return Self::new(0.0, 0.0, None);
        }
        let radius = sum.sqrt().sqrt();
        // IMSQRT publishes angle zero for the argument primitive's zero/ratio errors;
        // IMARGUMENT exposes #DIV/0! for those same input boundaries.
        let argument = self.argument().unwrap_or(0.0);
        let angle = argument / 2.0;
        Self::new(
            radius * crate::ported::excel_numeric::excel_cos(angle),
            radius * crate::ported::excel_numeric::excel_sin(angle),
            None,
        )
    }
    pub(crate) fn sin(self) -> Self {
        let (sin, cos) = complex_circular_parts(self.re);
        let positive = crate::ported::exp_fn::exp_kernel(self.im);
        let negative = crate::ported::excel_numeric::excel_x87_recip(positive);
        Self::new(
            sin * ((positive + negative) / 2.0),
            cos * ((positive - negative) / 2.0),
            None,
        )
    }
    pub(crate) fn cos(self) -> Self {
        let (sin, cos) = complex_circular_parts(self.re);
        let positive = crate::ported::exp_fn::exp_kernel(self.im);
        let negative = crate::ported::excel_numeric::excel_x87_recip(positive);
        Self::new(
            cos * ((positive + negative) / 2.0),
            -sin * ((positive - negative) / 2.0),
            None,
        )
    }
    /// Excel 20527 takes the imaginary limit before reducing the real argument.
    /// Probes on either side of each boundary distinguish the two strict limits.
    fn tangent_imaginary_limit(self) -> Option<f64> {
        (self.im > 354.5 || self.im < -354.0).then(|| self.im.signum())
    }

    pub(crate) fn tan(self) -> Result<Self, WorksheetErrorCode> {
        if let Some(im) = self.tangent_imaginary_limit() {
            return Ok(Self::new(0.0, im, None));
        }
        crate::ported::real_policy::ExcelRealPolicy::CIRCULAR_TRIG.check_arg(self.re)?;
        if self.im == 0.0 {
            return Ok(Self::new(
                crate::ported::excel_numeric::excel_tan(self.re),
                0.0,
                None,
            ));
        }
        let (sin, cos) = complex_circular_parts(self.re);
        // Store the hyperbolic sum before the circular subtraction, as in cotangent.
        let denom = excel_x87_sub(crt_cosh(2.0 * self.im) + 1.0, 2.0 * sin * sin);
        if denom == 0.0 {
            return Err(WorksheetErrorCode::Num);
        }
        // Off-axis Excel 20527 controls retain the scalar SIN/COS reduction in
        // the double-angle numerator (eng19; jnt_parity_complex D389).
        let re = (2.0 * sin * cos) / denom;
        Ok(Self::new(
            if re.is_subnormal() { 0.0 } else { re },
            crt_sinh(2.0 * self.im) / denom,
            None,
        ))
    }
    /// The direct real-axis IMSINH/IMCOSH results have EXP's overflow domain.
    /// Their reciprocal companions retain the wider runtime computation.
    pub(crate) fn check_real_hyperbolic_domain(self) -> Result<(), WorksheetErrorCode> {
        if self.im == 0.0 && !crate::ported::exp_fn::exp_kernel(self.re.abs()).is_finite() {
            return Err(WorksheetErrorCode::Num);
        }
        Ok(())
    }
    /// Excel 20527 complex hyperbolic parts `(sinh(x), cosh(x))` (dc-ymjf.2.139):
    /// - `|x| >= 20.0`: `EXP(|x|) / 2.0` (falling back to `crt_sinh`/`crt_cosh` if `EXP(|x|)`
    ///   overflows; real-axis `IMSINH`/`IMCOSH` enforce `EXP`'s domain via
    ///   `check_real_hyperbolic_domain`).
    /// - `|x| < 20.0`: `em = EXP(-x)`, `inv = 1.0 / em`, `sinh = (inv - em) / 2.0` (or
    ///   `x + x^3/6 + x^5/120` for `|x| <= 0.01`), `cosh = (inv + em) / 2.0` (or
    ///   `1.0 + x^2/2` for `|x| <= 0.0001`).
    fn hyperbolic_parts(x: f64) -> (f64, f64) {
        if x.abs() >= 20.0 {
            let exp_abs = crate::ported::exp_fn::exp_kernel(x.abs());
            if exp_abs.is_finite() {
                let half = exp_abs / 2.0;
                return (if x < 0.0 { -half } else { half }, half);
            }
            return (crt_sinh(x), crt_cosh(x));
        }
        let em = crate::ported::exp_fn::exp_kernel(-x);
        let inv = 1.0 / em;
        let sinh = if x.abs() <= 0.01 {
            x + (x * x * x) / 6.0 + (x * x * x * x * x) / 120.0
        } else {
            (inv - em) / 2.0
        };
        let cosh = if x.abs() <= 0.0001 {
            1.0 + (x * x) / 2.0
        } else {
            (inv + em) / 2.0
        };
        (sinh, cosh)
    }
    /// Off-axis overflow-avoidance branch for `x > 709.0 || x < -708.0` (matching the
    /// `2y > 709.0 || 2y < -708.0` threshold of `IMTAN`/`IMCOT`): Excel computes
    /// `EXP(|x| + LN(cos(y)/2))` and `EXP(|x| + LN(sin(y)/2))` without `abs()`, so
    /// non-positive `cos(y)` or `sin(y)` produces `NaN` (`#NUM!`).
    fn large_off_axis_hyperbolic(self) -> Option<(f64, f64)> {
        if self.exceeds_off_axis_hyperbolic_limit() {
            let (sin_im, cos_im) = complex_circular_parts(self.im);
            let ax = self.re.abs();
            let log_pos = |t: f64| {
                if t > 0.0 {
                    crate::ported::excel_numeric::excel_log(t)
                } else {
                    f64::NAN
                }
            };
            let re_mag = crate::ported::exp_fn::exp_kernel(ax + log_pos(cos_im / 2.0));
            let im_mag = crate::ported::exp_fn::exp_kernel(ax + log_pos(sin_im / 2.0));
            return Some((re_mag, im_mag));
        }
        None
    }
    fn exceeds_off_axis_hyperbolic_limit(self) -> bool {
        self.im != 0.0 && (self.re > 709.0 || self.re < -708.0)
    }
    pub(crate) fn sinh(self) -> Self {
        if let Some((re_mag, im_mag)) = self.large_off_axis_hyperbolic() {
            return Self::new(if self.re < 0.0 { -re_mag } else { re_mag }, im_mag, None);
        }
        let (sinh_re, cosh_re) = Self::hyperbolic_parts(self.re);
        // Preserve the finite real axis when the real primitive overflows:
        // its imaginary component is zero, without forming infinity * sin(0).
        if self.im == 0.0 && self.re.is_finite() {
            return Self::new(sinh_re, 0.0, None);
        }
        let (sin_im, cos_im) = complex_circular_parts(self.im);
        Self::new(sinh_re * cos_im, cosh_re * sin_im, None)
    }
    pub(crate) fn cosh(self) -> Self {
        if let Some((re_mag, im_mag)) = self.large_off_axis_hyperbolic() {
            return Self::new(re_mag, if self.re < 0.0 { -im_mag } else { im_mag }, None);
        }
        let (sinh_re, cosh_re) = Self::hyperbolic_parts(self.re);
        if self.im == 0.0 && self.re.is_finite() {
            return Self::new(cosh_re, 0.0, None);
        }
        let (sin_im, cos_im) = complex_circular_parts(self.im);
        Self::new(cosh_re * cos_im, sinh_re * sin_im, None)
    }
    pub(crate) fn cot(self) -> Result<Self, WorksheetErrorCode> {
        if let Some(im) = self.tangent_imaginary_limit() {
            return Ok(Self::new(0.0, -im, None));
        }
        if self.im == 0.0 {
            // The real-axis cotangent uses the scalar tangent and one reciprocal.
            // At the scalar domain boundary IMCOT publishes zero, while IMTAN is #NUM!.
            if self.re.is_finite()
                && crate::ported::real_policy::ExcelRealPolicy::CIRCULAR_TRIG
                    .check_arg(self.re)
                    .is_err()
            {
                return Ok(Self::new(0.0, 0.0, None));
            }
            let tangent = crate::ported::excel_numeric::excel_tan(self.re);
            if tangent == 0.0 {
                return Err(WorksheetErrorCode::Num);
            }
            return Ok(Self::new(
                crate::ported::excel_numeric::excel_x87_recip(tangent),
                0.0,
                None,
            ));
        }
        crate::ported::real_policy::ExcelRealPolicy::CIRCULAR_TRIG.check_arg(self.re)?;
        let (sin, cos) = complex_circular_parts(self.re);
        // Excel 20527 uses separate small-argument bounds for the two
        // hyperbolic factors. Beyond them the exponential retains y's sign.
        let doubled = 2.0 * self.im;
        let square = doubled * doubled;
        let sinh_series = doubled + (doubled * square) * (1.0 / 6.0 + square / 120.0);
        let (cosh_double, sinh_double) = if doubled.abs() <= 0.0001 {
            (1.0 + square / 2.0, sinh_series)
        } else {
            let exponential = crate::ported::exp_fn::exp_kernel(doubled);
            let reciprocal = 1.0 / exponential;
            let sinh_double = if doubled.abs() <= 0.01 {
                sinh_series
            } else {
                (exponential - reciprocal) / 2.0
            };
            ((exponential + reciprocal) / 2.0, sinh_double)
        };
        // Store the sum first, then subtract the circular term in x87 precision.
        // At im = 19 the subtraction can land on a binary64 midpoint.
        let denom = excel_x87_sub(cosh_double + 1.0, 2.0 * cos * cos);
        if denom == 0.0 {
            return Err(WorksheetErrorCode::Num);
        }
        let re = excel_x87_div(2.0 * sin * cos, denom);
        Ok(Self::new(
            if re.is_subnormal() { 0.0 } else { re },
            excel_x87_div(-sinh_double, denom),
            None,
        ))
    }
    pub(crate) fn sec(self) -> Result<Self, WorksheetErrorCode> {
        self.circular_reciprocal(Self::cos)
    }
    pub(crate) fn csc(self) -> Result<Self, WorksheetErrorCode> {
        self.circular_reciprocal(Self::sin)
    }
    fn circular_reciprocal(self, denominator: fn(Self) -> Self) -> Result<Self, WorksheetErrorCode> {
        // Excel applies the imaginary limit before evaluating the real angle (dc-j9k.397).
        if self.im > 709.0 || self.im < -708.0 {
            return Ok(Self::new(0.0, 0.0, None));
        }
        denominator(self).reciprocal()
    }
    pub(crate) fn sech(self) -> Result<Self, WorksheetErrorCode> {
        if self.exceeds_off_axis_hyperbolic_limit() {
            return Ok(Self::new(0.0, 0.0, None));
        }
        self.cosh().reciprocal()
    }
    pub(crate) fn csch(self) -> Result<Self, WorksheetErrorCode> {
        if self.exceeds_off_axis_hyperbolic_limit() {
            return Ok(Self::new(0.0, 0.0, None));
        }
        let mut denominator = self.sinh();
        // Excel reverses the off-axis signs once the doubled real part leaves
        // its exponential interval (dc-j9k.398, Excel 16.0.20625).
        if self.im != 0.0 && (self.re > 354.5 || self.re < -354.0) {
            denominator.re = -denominator.re;
            denominator.im = -denominator.im;
        }
        denominator.reciprocal()
    }
    pub(crate) fn power(self, exponent: f64) -> Result<Self, WorksheetErrorCode> {
        if self.re == 0.0 && self.im == 0.0 {
            if exponent < 0.0 {
                return Err(WorksheetErrorCode::Num);
            }
            if exponent == 0.0 {
                return Ok(Self::new(1.0, 0.0, None));
            }
            return Ok(Self::new(0.0, 0.0, None));
        }
        let on_axis = self.im == 0.0 || self.re == 0.0;
        let scale = if on_axis {
            crate::ported::power_fn::power_kernel(self.abs(), exponent)
                .map_err(|_| WorksheetErrorCode::Num)?
        } else {
            crt_pow(self.abs(), exponent)
        };
        // A fully underflowed magnitude is published before validating the angle.
        if scale == 0.0 {
            return Ok(Self::new(0.0, 0.0, None));
        }
        let angle = crt_atan2(self.im, self.re) * exponent;
        // Excel 20527 rejects complex-power angles above 2^27 - 1 in magnitude.
        if angle.abs() > 134_217_727.0 {
            return Err(WorksheetErrorCode::Num);
        }
        let (sine, cosine) = if on_axis {
            // Excel uses its circular argument reduction on both axes.
            complex_circular_parts(angle)
        } else {
            (crt_sin(angle), crt_cos(angle))
        };
        Ok(Self::new(
            scale * cosine,
            scale * sine,
            None,
        ))
    }
}

pub(crate) fn normalize_zero(n: f64) -> f64 {
    if n == 0.0 { 0.0 } else { n }
}
/// Excel's complex coefficient rendering, observed on builds 20430 and 20527 through Value2 inputs:
/// 15 significant decimal digits via `ExactDecimal::fifteen_x87(n, true)` (`frexp`-based
/// `e_est = floor(k / 3.3219)`, low-nibble-first 64-bit chain, `* 10` loop, exact half-up ties,
/// and the `k = -485, -970` sliver 16-digit binary64 re-scale), fixed notation while the unsigned
/// result fits in 21 characters, otherwise scientific notation. No near-integer snapping is applied.
pub(crate) fn number_to_excel_text(n: f64) -> Result<String, WorksheetErrorCode> {
    if n == 0.0 {
        return Ok("0".to_string());
    }
    // The exact maximum finite magnitude is rejected; its immediate finite neighbors are
    // admitted. This is an observed admission boundary, not an inferred internal mechanism.
    if !n.is_finite() || n.abs() == f64::MAX {
        return Err(WorksheetErrorCode::Num);
    }

    let sign = if n < 0.0 { "-" } else { "" };
    let exact = crate::decimal::ExactDecimal::fifteen_x87(n, true)
        .expect("finite non-zero complex coefficient");
    let digits = exact.coefficient.digits();
    let scale = exact.exponent;
    let decimal = format!("{digits}e{scale}");
    let rounded: f64 = decimal
        .parse()
        .expect("finite-input decimal representation");
    // Rounding MIN_POSITIVE to 15 digits crosses below the normal range. Excel publishes
    // signed text zero for that coefficient without deleting its component from the result.
    if rounded < f64::MIN_POSITIVE {
        return Ok(format!("{sign}0"));
    }
    // Do not reject a decimal that rounds above binary64's range. For MAX-1 ULP, Excel
    // legitimately publishes 1.79769313486232E+308 even though reparsing it overflows.
    let exponent = scale + digits.len() as i32 - 1;
    let decimal_pos = exponent + 1;
    let mut out = if decimal_pos <= 0 {
        format!("0.{}{}", "0".repeat((-decimal_pos) as usize), digits)
    } else if decimal_pos as usize >= digits.len() {
        format!(
            "{}{}",
            digits,
            "0".repeat(decimal_pos as usize - digits.len())
        )
    } else {
        let split = decimal_pos as usize;
        format!("{}.{}", &digits[..split], &digits[split..])
    };

    if let Some(dot) = out.find('.') {
        while out.ends_with('0') {
            out.pop();
        }
        if out.len() == dot + 1 {
            out.pop();
        }
    }

    if out.len() > 21 {
        let mantissa = format!("{}.{}", &digits[..1], &digits[1..]);
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        out = format!("{mantissa}E{exponent:+03}");
    }
    Ok(format!("{sign}{out}"))
}
pub(crate) fn format_complex_text(
    value: ParsedComplex,
    suffix: char,
) -> Result<String, WorksheetErrorCode> {
    let re = normalize_zero(value.re);
    let im = normalize_zero(value.im);
    if im == 0.0 {
        return number_to_excel_text(re);
    }
    let coeff = if im.abs() == 1.0 {
        String::new()
    } else {
        number_to_excel_text(im.abs())?
    };
    if re == 0.0 {
        if im < 0.0 {
            Ok(format!("-{}{suffix}", coeff))
        } else {
            Ok(format!("{}{suffix}", coeff))
        }
    } else {
        let sign = if im < 0.0 { '-' } else { '+' };
        Ok(format!(
            "{}{sign}{}{suffix}",
            number_to_excel_text(re)?,
            coeff
        ))
    }
}

// The retained complex-axis controls use the scalar circular kernels' admitted
// magnitude boundary, while hyperbolic factors retain exponential cancellation.
pub(crate) fn complex_circular_parts(real: f64) -> (f64, f64) {
    if real.abs() >= 134_217_728.0 {
        (f64::NAN, f64::NAN)
    } else {
        (
            crate::ported::excel_numeric::excel_sin(real),
            crate::ported::excel_numeric::excel_cos(real),
        )
    }
}

pub(crate) fn parse_real_number(text: &str) -> Option<f64> {
    let trimmed = text.trim_matches(' ');
    if trimmed.is_empty() {
        return None;
    }
    let negative = trimmed.starts_with('-');
    let unsigned = trimmed
        .strip_prefix(['+', '-'])
        .unwrap_or(trimmed)
        .trim_matches(' ');
    let parsed = parse_ascii_decimal_15(unsigned)?;
    // Complex coefficient strings have their own grammar. Only decimal lexeme
    // conversion is shared with numeric argument coercion; decorations are not.
    // parse_ascii_decimal_15 rejects scale positions outside -308..=308 (so nonzero < 1e-309 is
    // already None -> #NUM!); subnormal values from 1e-309 up to DBL_MIN flush to 0.
    Some(if parsed < f64::MIN_POSITIVE {
        0.0
    } else if negative {
        -parsed
    } else {
        parsed
    })
}
pub(crate) fn find_real_imag_split(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut out = None;
    for i in 1..bytes.len() {
        let ch = bytes[i] as char;
        let prev = bytes[i - 1] as char;
        if (ch == '+' || ch == '-') && prev != 'e' && prev != 'E' {
            out = Some(i);
        }
    }
    out
}
pub(crate) fn parse_imag_coeff(text: &str) -> Option<f64> {
    let trimmed = text.trim_matches(' ');
    if trimmed.is_empty() || trimmed == "+" {
        Some(1.0)
    } else if trimmed == "-" {
        Some(-1.0)
    } else {
        parse_real_number(trimmed)
    }
}
pub(crate) fn parse_complex_text(raw: &str) -> Option<ParsedComplex> {
    let text = raw.trim_matches(' ');
    if text.is_empty() {
        return None;
    }
    if let Some(last) = text.chars().last() {
        if last == 'i' || last == 'j' {
            let body = &text[..text.len() - last.len_utf8()];
            if let Some(split) = find_real_imag_split(body) {
                return Some(ParsedComplex::new(
                    parse_real_number(&body[..split])?,
                    parse_imag_coeff(&body[split..])?,
                    Some(last),
                ));
            }
            return Some(ParsedComplex::new(0.0, parse_imag_coeff(body)?, Some(last)));
        }
    }
    parse_real_number(text).map(|re| ParsedComplex::new(re, 0.0, None))
}

pub(crate) fn suffix_from_operands(values: &[ParsedComplex]) -> Result<char, WorksheetErrorCode> {
    let mut suffix = None;
    for value in values {
        if let Some(ch) = value.suffix {
            match suffix {
                None => suffix = Some(ch),
                Some(current) if current == ch => {}
                Some(_) => return Err(WorksheetErrorCode::Value),
            }
        }
    }
    Ok(suffix.unwrap_or('i'))
}

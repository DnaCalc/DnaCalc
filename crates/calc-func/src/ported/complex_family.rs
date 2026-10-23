//! The complex-number engineering functions' arithmetic, parsing and text rendering, ported from
//! OxFunc functions/complex_family.rs; the argument reading lives in the engineering family.

use crate::ported::complex_decimal;
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
            (&text[..index], if negative { -magnitude } else { magnitude })
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
    re: f64,
    im: f64,
    suffix: Option<char>,
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
        // Arithmetic underflow is a zero component before text assembly. This differs
        // from formatting a nonzero normal input whose 15-digit decimal falls below MIN.
        let normal_or_zero = |x: f64| if x.abs() < f64::MIN_POSITIVE { 0.0 } else { x };
        Ok(Self::new(normal_or_zero(re), normal_or_zero(im), None))
    }
    pub(crate) fn conj(self) -> Self {
        Self::new(self.re, -self.im, None)
    }
    pub(crate) fn abs(self) -> f64 {
        self.re.hypot(self.im)
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
            if im < 0.0 { base - std::f64::consts::PI } else { base + std::f64::consts::PI }
        } else { base };
        Ok(if result.abs() < f64::MIN_POSITIVE { 0.0 } else { result })
    }
    pub(crate) fn exp(self) -> Self {
        let s = self.re.exp();
        Self::new(s * self.im.cos(), s * self.im.sin(), None)
    }
    pub(crate) fn ln(self) -> Result<Self, WorksheetErrorCode> {
        let mag = self.abs();
        if mag == 0.0 {
            return Err(WorksheetErrorCode::Num);
        }
        Ok(Self::new(mag.ln(), self.im.atan2(self.re), None))
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
    pub(crate) fn tan(self) -> Result<Self, WorksheetErrorCode> {
        let denom = (2.0 * self.re).cos() + (2.0 * self.im).cosh();
        if denom == 0.0 {
            return Err(WorksheetErrorCode::Num);
        }
        Ok(Self::new(
            (2.0 * self.re).sin() / denom,
            (2.0 * self.im).sinh() / denom,
            None,
        ))
    }
    pub(crate) fn sinh(self) -> Self {
        Self::new(
            self.re.sinh() * self.im.cos(),
            self.re.cosh() * self.im.sin(),
            None,
        )
    }
    pub(crate) fn cosh(self) -> Self {
        Self::new(
            self.re.cosh() * self.im.cos(),
            self.re.sinh() * self.im.sin(),
            None,
        )
    }
    pub(crate) fn cot(self) -> Result<Self, WorksheetErrorCode> {
        let denom = (2.0 * self.im).cosh() - (2.0 * self.re).cos();
        if denom == 0.0 {
            return Err(WorksheetErrorCode::Num);
        }
        Ok(Self::new(
            (2.0 * self.re).sin() / denom,
            -(2.0 * self.im).sinh() / denom,
            None,
        ))
    }
    pub(crate) fn sec(self) -> Result<Self, WorksheetErrorCode> {
        ParsedComplex::new(1.0, 0.0, None).div(self.cos())
    }
    pub(crate) fn csc(self) -> Result<Self, WorksheetErrorCode> {
        ParsedComplex::new(1.0, 0.0, None).div(self.sin())
    }
    pub(crate) fn sech(self) -> Result<Self, WorksheetErrorCode> {
        ParsedComplex::new(1.0, 0.0, None).div(self.cosh())
    }
    pub(crate) fn csch(self) -> Result<Self, WorksheetErrorCode> {
        ParsedComplex::new(1.0, 0.0, None).div(self.sinh())
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
        let scale = self.abs().powf(exponent);
        let angle = self.im.atan2(self.re) * exponent;
        Ok(Self::new(scale * angle.cos(), scale * angle.sin(), None))
    }
}


pub(crate) fn normalize_zero(n: f64) -> f64 {
    if n == 0.0 { 0.0 } else { n }
}
/// Excel's complex coefficient rendering, observed on build 20430 through Value2 inputs:
/// 15 significant decimal digits using the binary scaling model in `complex_decimal`, fixed
/// notation while the unsigned result fits in 21 characters, otherwise scientific notation.
/// No near-integer snapping is applied. See the retained W111 captures and failed hypotheses.
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
    let (mut significand, mut scale) = complex_decimal::decimal_pair(n);
    if significand == 1_000_000_000_000_000 {
        significand /= 10;
        scale += 1;
    }
    let decimal = format!("{significand}e{scale}");
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
    let digits = significand.to_string();
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
        (crate::ported::excel_numeric::excel_sin(real), crate::ported::excel_numeric::excel_cos(real))
    }
}

pub(crate) fn parse_real_number(text: &str) -> Option<f64> {
    let trimmed = text.trim_matches(' ');
    if trimmed.is_empty() {
        return None;
    }
    let negative = trimmed.starts_with('-');
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed).trim_matches(' ');
    let parsed = parse_ascii_decimal_15(unsigned)?;
    // Complex coefficient strings have their own grammar. Only decimal lexeme
    // conversion is shared with numeric argument coercion; decorations are not.
    Some(if parsed < f64::MIN_POSITIVE { 0.0 } else if negative { -parsed } else { parsed })
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
        return Some(ParsedComplex::new(0.0, 0.0, None));
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

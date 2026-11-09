//! The engineering family fragment ported from OxFunc (functions/ at 6fca3518): the radix
//! conversions (BIN2DEC to OCT2HEX), the bit functions, the complex-number functions (COMPLEX and
//! the IM functions), the Bessel functions, DELTA, GESTEP and CONVERT. The kernels are OxFunc's
//! ([`crate::ported`]), except the Bessel functions' ([`crate::ported::bessel`], written clean-room
//! under dc-96l.3).
//!
//! The radix functions refuse a logical argument with #VALUE! and an empty one with #N/A, as
//! OxFunc's engineering argument rule does, and check an explicit places argument before the
//! number. A complex-number argument is a number, or text in the complex form (any other text is
//! #NUM!); a logical is #VALUE! and a blank is 0.

use super::kit::{fail, given, num, out, out_text, text, text_value, values};
use crate::kernel::{Exit, Site, Step};
use crate::ported::complex_family::{
    ParsedComplex, format_complex_text, normalize_zero, parse_complex_text, suffix_from_operands,
};
use crate::ported::engineering_radix_family as radix;
use crate::ported::{self as k};
use crate::value::{CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use crate::ported::excel_numeric::crt_log;

use calc_catalogue::ported::engineering::{CODE, ENTRIES as DECLARATIONS};

/// The radix functions and BESSELJ refuse logicals with #VALUE! and omitted arguments with #N/A.
/// A blank cell remains distinct from an omitted argument.
fn radix_kind(a: &[CalcValue], index: usize) -> Step<()> {
    match a[index].core_ref() {
        CoreRef::Logical(_) => fail(WorksheetErrorCode::Value),
        CoreRef::Missing => fail(WorksheetErrorCode::NA),
        _ => Ok(()),
    }
}

/// The optional places argument: checked (kind, number and range) before the number is read.
fn places(site: &Site<'_>, a: &[CalcValue]) -> Step<Option<f64>> {
    if a.len() > 1 && !a[1].is_missing() {
        radix_kind(a, 1)?;
        let p = num(site, a, 1)?;
        radix::normalize_places(Some(p)).map_err(Exit::Error)?;
        Ok(Some(p))
    } else {
        Ok(None)
    }
}

fn from_decimal(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(f64, Option<f64>) -> Result<ExcelText, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let p = places(site, a)?;
    radix_kind(a, 0)?;
    let n = num(site, a, 0)?;
    out_text(kernel(n, p))
}

fn to_decimal(site: &Site<'_>, a: &[CalcValue], kernel: fn(&str) -> Result<f64, WorksheetErrorCode>) -> Step<CalcValue> {
    radix_kind(a, 0)?;
    let t = text(site, a, 0)?;
    out(site, kernel(&t))
}

fn between(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(&str, Option<f64>) -> Result<ExcelText, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let p = places(site, a)?;
    radix_kind(a, 0)?;
    let t = text(site, a, 0)?;
    out_text(kernel(&t, p))
}

/// A value as a complex number.
fn complex_of(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<ParsedComplex> {
    match value.core_ref() {
        CoreRef::Number(n) => Ok(ParsedComplex::new(n, 0.0, None)),
        CoreRef::Text(t) => match String::from_utf16(t.utf16_code_units()) {
            Ok(text) => parse_complex_text(&text).ok_or(Exit::Error(WorksheetErrorCode::Num)),
            Err(_) => Err(site.not_modelled(Some(index))),
        },
        CoreRef::Logical(_) => fail(WorksheetErrorCode::Value),
        CoreRef::Error(code) => fail(code),
        CoreRef::Empty | CoreRef::Missing => Ok(ParsedComplex::new(0.0, 0.0, None)),
        CoreRef::Array(_) | CoreRef::Reference(_) => Err(site.not_modelled(Some(index))),
    }
}

/// A complex-number result as its text, with the suffix the operands use.
fn complex_text(value: Result<ParsedComplex, WorksheetErrorCode>, operands: &[ParsedComplex]) -> Step<CalcValue> {
    let suffix = suffix_from_operands(operands).map_err(Exit::Error)?;
    let value = value.map_err(Exit::Error)?;
    let text = format_complex_text(value, suffix).map_err(Exit::Error)?;
    Ok(text_value(&text))
}

fn unary_complex(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(ParsedComplex) -> Result<ParsedComplex, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let z = complex_of(site, 0, &a[0])?;
    complex_text(kernel(z), &[z])
}

fn complex_number(
    site: &Site<'_>,
    a: &[CalcValue],
    kernel: fn(ParsedComplex) -> Result<f64, WorksheetErrorCode>,
) -> Step<CalcValue> {
    let z = complex_of(site, 0, &a[0])?;
    out(site, kernel(z).map(normalize_zero))
}

/// COMPLEX's real and imaginary coefficients: a number, numeric text, a blank as 0; a logical is
/// #VALUE!.
fn coefficient(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<f64> {
    match a[index].core_ref() {
        CoreRef::Logical(_) => fail(WorksheetErrorCode::Value),
        _ => num(site, a, index),
    }
}

/// Every value of IMSUM's and IMPRODUCT's arguments as a complex number.
fn complexes(site: &Site<'_>, a: &[CalcValue], sv: &mut crate::call::EvalServices<'_>) -> Step<Vec<ParsedComplex>> {
    let mut found = Vec::new();
    values(site, a, 0, a.len(), sv, &mut |_, value| {
        found.push(complex_of(site, 0, value)?);
        Ok(())
    })?;
    Ok(found)
}

fn bessel(site: &Site<'_>, a: &[CalcValue], kernel: fn(f64, f64) -> Result<f64, WorksheetErrorCode>, checked_kinds: bool) -> Step<CalcValue> {
    if checked_kinds { radix_kind(a, 0)?; }
    let x = num(site, a, 0)?;
    if checked_kinds { radix_kind(a, 1)?; }
    let n = num(site, a, 1)?;
    out(site, kernel(x, n))
}

fn bits(site: &Site<'_>, a: &[CalcValue], kernel: fn(f64, f64) -> Result<f64, WorksheetErrorCode>) -> Step<CalcValue> {
    let x = num(site, a, 0)?;
    let y = num(site, a, 1)?;
    out(site, kernel(x, y))
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 bin2dec |s, a, sv| { to_decimal(s, a, radix::bin2dec_kernel) }
    2 bin2hex |s, a, sv| { between(s, a, radix::bin2hex_kernel) }
    3 bin2oct |s, a, sv| { between(s, a, radix::bin2oct_kernel) }
    4 dec2bin |s, a, sv| { from_decimal(s, a, radix::dec2bin_kernel) }
    5 dec2hex |s, a, sv| { from_decimal(s, a, radix::dec2hex_kernel) }
    6 dec2oct |s, a, sv| { from_decimal(s, a, radix::dec2oct_kernel) }
    7 hex2bin |s, a, sv| { between(s, a, radix::hex2bin_kernel) }
    8 hex2dec |s, a, sv| { to_decimal(s, a, radix::hex2dec_kernel) }
    9 hex2oct |s, a, sv| { between(s, a, radix::hex2oct_kernel) }
    10 oct2bin |s, a, sv| { between(s, a, radix::oct2bin_kernel) }
    11 oct2dec |s, a, sv| { to_decimal(s, a, radix::oct2dec_kernel) }
    12 oct2hex |s, a, sv| { between(s, a, radix::oct2hex_kernel) }
    20 bitand |s, a, sv| { bits(s, a, k::bitand_fn::bitand_kernel) }
    21 bitor |s, a, sv| { bits(s, a, k::bitor_fn::bitor_kernel) }
    22 bitxor |s, a, sv| { bits(s, a, k::bitxor_fn::bitxor_kernel) }
    23 bitlshift |s, a, sv| {
        bits(s, a, k::bitlshift_fn::bitlshift_kernel)
    }
    24 bitrshift |s, a, sv| {
        bits(s, a, k::bitrshift_fn::bitrshift_kernel)
    }
    30 complex |s, a, sv| {
        let re = coefficient(s, a, 0)?;
        let im = coefficient(s, a, 1)?;
        let suffix = match a.get(2).map(CalcValue::core_ref) {
            None | Some(CoreRef::Missing | CoreRef::Empty) => 'i',
            Some(CoreRef::Error(code)) => return fail(code),
            Some(CoreRef::Text(_)) => match text(s, a, 2)?.trim() {
                "" | "i" => 'i',
                "j" => 'j',
                _ => return fail(WorksheetErrorCode::Value),
            },
            Some(_) => return fail(WorksheetErrorCode::Value),
        };
        let text = format_complex_text(ParsedComplex::new(re, im, None), suffix).map_err(Exit::Error)?;
        Ok(text_value(&text))
    }
    31 imabs |s, a, sv| { complex_number(s, a, |z| Ok(z.abs())) }
    32 imaginary |s, a, sv| { complex_number(s, a, |z| Ok(z.im)) }
    33 imargument |s, a, sv| { complex_number(s, a, |z| z.argument()) }
    34 imreal |s, a, sv| { complex_number(s, a, |z| Ok(z.re)) }
    35 imconjugate |s, a, sv| { unary_complex(s, a, |z| Ok(z.conj())) }
    36 imcos |s, a, sv| { unary_complex(s, a, |z| Ok(z.cos())) }
    37 imcosh |s, a, sv| { unary_complex(s, a, |z| { z.check_real_hyperbolic_domain()?; Ok(z.cosh()) }) }
    38 imcot |s, a, sv| { unary_complex(s, a, |z| z.cot()) }
    39 imcsc |s, a, sv| { unary_complex(s, a, |z| z.csc()) }
    40 imcsch |s, a, sv| { unary_complex(s, a, |z| z.csch()) }
    41 imexp |s, a, sv| { unary_complex(s, a, |z| Ok(z.exp())) }
    42 imln |s, a, sv| { unary_complex(s, a, |z| z.ln()) }
    43 imlog10 |s, a, sv| {
        unary_complex(s, a, |z| {
            let ln = z.ln()?;
            let re = if z.im == 0.0 {
                crate::ported::excel_numeric::excel_x87_mul(
                    crate::ported::excel_numeric::excel_log(z.re.abs()),
                    std::f64::consts::LOG10_E,
                )
            } else {
                ln.re / crt_log(10.0)
            };
            Ok(ParsedComplex::new(re, ln.im / crt_log(10.0), None))
        })
    }
    44 imlog2 |s, a, sv| {
        unary_complex(s, a, |z| {
            let ln = z.ln()?;
            let re = if z.im == 0.0 {
                // Fn11's exact coefficient-bin bounds uniquely determine this binary64
                // LN multiplier; independent normal and signed controls retain it.
                crate::ported::excel_numeric::excel_x87_mul(
                    crate::ported::excel_numeric::excel_log(z.re.abs()),
                    f64::from_bits(0x3ff7_1547_652b_82ff),
                )
            } else {
                ln.re / crt_log(2.0)
            };
            Ok(ParsedComplex::new(re, ln.im / crt_log(2.0), None))
        })
    }
    45 imsec |s, a, sv| { unary_complex(s, a, |z| z.sec()) }
    46 imsech |s, a, sv| { unary_complex(s, a, |z| z.sech()) }
    47 imsin |s, a, sv| { unary_complex(s, a, |z| Ok(z.sin())) }
    48 imsinh |s, a, sv| { unary_complex(s, a, |z| { z.check_real_hyperbolic_domain()?; Ok(z.sinh()) }) }
    49 imsqrt |s, a, sv| { unary_complex(s, a, |z| Ok(z.sqrt())) }
    50 imtan |s, a, sv| { unary_complex(s, a, |z| z.tan()) }
    51 imdiv |s, a, sv| {
        let x = complex_of(s, 0, &a[0])?;
        let y = complex_of(s, 1, &a[1])?;
        complex_text(x.div(y), &[x, y])
    }
    52 impower |s, a, sv| {
        let z = complex_of(s, 0, &a[0])?;
        let n = coefficient(s, a, 1)?;
        complex_text(z.power(n), &[z])
    }
    53 imsub |s, a, sv| {
        let x = complex_of(s, 0, &a[0])?;
        let y = complex_of(s, 1, &a[1])?;
        complex_text(Ok(x.sub(y)), &[x, y])
    }
    54 imsum |s, a, sv| {
        let zs = complexes(s, a, sv)?;
        let sum = zs.iter().fold(ParsedComplex::new(0.0, 0.0, None), |acc, z| acc.add(*z));
        complex_text(Ok(sum), &zs)
    }
    55 improduct |s, a, sv| {
        let zs = complexes(s, a, sv)?;
        let product = zs.iter().fold(ParsedComplex::new(1.0, 0.0, None), |acc, z| acc.mul(*z));
        complex_text(Ok(product), &zs)
    }
    60 besseli |s, a, sv| { bessel(s, a, k::bessel::besseli_kernel, false) }
    61 besselj |s, a, sv| { bessel(s, a, k::bessel::besselj_kernel, true) }
    62 besselk |s, a, sv| { bessel(s, a, k::bessel::besselk_kernel, false) }
    63 bessely |s, a, sv| { bessel(s, a, k::bessel::bessely_kernel, false) }
    70 delta |s, a, sv| {
        // Excel reads the second scalar before the first, including conversion errors.
        let y = if given(a, 1) { super::kit::reject_logical(&a[1])?; super::kit::num_with_subnormal(s, a, 1)? } else { 0.0 };
        super::kit::reject_logical(&a[0])?;
        let x = super::kit::num_with_subnormal(s, a, 0)?;
        out(s, k::delta_fn::delta_kernel(x, y))
    }
    71 gestep |s, a, sv| {
        let step = if given(a, 1) { super::kit::reject_logical(&a[1])?; super::kit::num_with_subnormal(s, a, 1)? } else { 0.0 };
        super::kit::reject_logical(&a[0])?;
        let x = super::kit::num_with_subnormal(s, a, 0)?;
        out(s, k::gestep_fn::gestep_kernel(x, step))
    }
    72 convert |s, a, sv| {
        let x = num(s, a, 0)?;
        let from = text(s, a, 1)?;
        let to = text(s, a, 2)?;
        out(s, k::misc_conversion_family::convert_kernel(x, &from, &to))
    }
}

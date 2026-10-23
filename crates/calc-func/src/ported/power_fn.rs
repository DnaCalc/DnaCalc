use crate::ported::excel_numeric::{excel_pow_positive, excel_x87_recip};
use crate::value::WorksheetErrorCode;

fn exact_integer_magnitude(power: f64) -> Option<u32> {
    if !power.is_finite() {
        return None;
    }
    let magnitude = power.abs();
    // W111 width controls distinguish the unsigned32 sentinel itself from
    // its predecessor. Out-of-range integers use the noninteger body below.
    if magnitude.fract() != 0.0 || magnitude >= u32::MAX as f64 {
        None
    } else {
        Some(magnitude as u32)
    }
}

/// Excel's real-root acceptance for a negative base with a fractional exponent.
/// Excel accepts `POWER(x<0, y)` only when `1/y`, rounded to **15 significant
/// decimal digits**, is an odd integer within signed `int32`; the result is then
/// `-POWER(|x|, y)`, evaluated at the ORIGINAL `y`. (Round-1 live-Excel finding:
/// the acceptance band around `n=101` is exactly 100x the band around `n=3` — a
/// decimal signature no binary tolerance reproduces. BUG-FUNC-042 / W108.)
fn accept_negative_base_odd_root(power: f64) -> bool {
    // 15 significant decimal digits == 14 after the point in scientific form,
    // matching Excel's `%.14e` fuzz on 1/y.
    let n: f64 = match format!("{:.14e}", 1.0 / power).parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    n.fract() == 0.0 && (-2_147_483_648.0..=2_147_483_647.0).contains(&n) && (n as i64) % 2 != 0
}

/// LSB-first (right-to-left) square-and-multiply `base^n` for `n >= 0`, with
/// staged products followed by binary64 stores (W111 numerical observations,
/// distinct from `powf`). Returns the POSITIVE power; the caller reciprocates.
fn binexp_positive(base: f64, mut n: u64) -> f64 {
    let mut acc = 1.0;
    let mut b = base;
    while n > 0 {
        if n & 1 == 1 {
            acc = crate::ported::excel_numeric::excel_x87_mul(acc, b);
        }
        n >>= 1;
        if n > 0 {
            b = crate::ported::excel_numeric::excel_x87_mul(b, b);
        }
    }
    acc
}

/// `POWER(x, y)` candidate graph (the `^` operator shares this kernel).
/// Extends the black-box BUG-FUNC-042 / W108 graph with W111 integer-power
/// observations: positive base ten has a decimal-scale branch; other integer
/// exponents use staged binary exponentiation. Fractional exponents use
/// → `exp(y·ln x)` via the x87 backend with the `y<0` reciprocal staging (positive
/// power stored to `binary64`, then one x87 double-rounded reciprocal) that
/// resolved the earlier 1-ULP residual — `exp(y·ln x)` was the right function
/// evaluated at the wrong point for `y < 0`.
pub fn power_kernel(number: f64, power: f64) -> Result<f64, WorksheetErrorCode> {
    // 1. zero exponent
    if power == 0.0 {
        return if number == 0.0 {
            Err(WorksheetErrorCode::Num) // POWER(0, 0)
        } else {
            Ok(1.0)
        };
    }
    // 2. zero base
    if number == 0.0 {
        return if power < 0.0 {
            Err(WorksheetErrorCode::Div0) // POWER(0, negative)
        } else {
            Ok(0.0)
        };
    }

    // 3. integer exponent → binary-exponentiation publication. This is the
    // declared precision quirk on `POWER_META`, also used by `^`. Consumers
    // retain their separately assessed call-site graphs. Dispatch is EXACT —
    // `POWER(2, 3+ulp)` takes the fractional
    // path. The positive power is formed by `binexp`, reciprocated for `y<0`.
    // POWER_META declares PrecisionRoundingProfile::IntegerExponentPublication.
    let integer_publication = true;
    if let Some(magnitude) = exact_integer_magnitude(power).filter(|_| integer_publication) {
        // This branch is specific to positive ten; negative ten retains the
        // ordinary repeated-product graph and differs at many large powers.
        let mut p = if number == 10.0 {
            // The observed decimal scale wraps through signed32 inside the
            // admitted unsigned32 magnitude. E.g. 2^32-2 selects scale -2.
            let scale = magnitude as i32;
            if scale > 308 {
                f64::INFINITY
            } else if scale < -308 {
                0.0
            } else {
                format!("1e{scale}").parse::<f64>().unwrap_or(f64::INFINITY)
            }
        } else {
            binexp_positive(number, u64::from(magnitude))
        };
        if power < 0.0 {
            if p.abs() < f64::MIN_POSITIVE {
                return Err(WorksheetErrorCode::Div0); // reciprocal sees tiny magnitude as zero
            }
            p = excel_x87_recip(p); // 1/inf -> 0, published below
        }
        if p.is_nan() || p.is_infinite() {
            return Err(WorksheetErrorCode::Num);
        }
        // Integer path flushes subnormal / -0 / 0 to +0 (Excel: POWER(2,-1023)=0).
        return Ok(if p.abs() < f64::MIN_POSITIVE { 0.0 } else { p });
    }

    // 4. fractional exponent.
    let mut sign = 1.0;
    let mut base = number;
    if number < 0.0 {
        if !accept_negative_base_odd_root(power) {
            return Err(WorksheetErrorCode::Num); // no real root
        }
        sign = -1.0;
        base = -number; // continue with |x|
    }

    // p = exp_x87( RN53(RN64( |y| * ln_x87(base) )) ) — the positive power.
    let p = excel_pow_positive(base, power.abs());

    if power > 0.0 {
        if p.is_infinite() {
            return Err(WorksheetErrorCode::Num); // overflow
        }
        if p == 0.0 {
            return Ok(0.0);
        }
        return Ok(sign * p); // subnormals ARE published on the y>0 path
    }

    // y < 0: reciprocal staging (the resolved 1-ULP puzzle).
    if p < f64::MIN_POSITIVE {
        return Err(WorksheetErrorCode::Div0); // subnormal/zero positive power (DAZ-style)
    }
    if p.is_infinite() {
        return Ok(0.0); // 1 / inf
    }
    let r = excel_x87_recip(p); // RN53(RN64(1/p))
    if r < f64::MIN_POSITIVE {
        return Ok(0.0); // subnormal quotient flushes to +0
    }
    if r.is_infinite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(sign * r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_kernel_matches_excel_domain_lanes() {
        assert_eq!(power_kernel(2.0, 3.0), Ok(8.0));
        assert_eq!(power_kernel(2.0, -3.0), Ok(0.125));
        assert_eq!(power_kernel(2.0, -1023.0), Ok(0.0));
        assert_eq!(power_kernel(2.0, -1022.0), Ok(f64::MIN_POSITIVE));
        assert_eq!(power_kernel(0.0, 0.0), Err(WorksheetErrorCode::Num));
        assert_eq!(power_kernel(-0.0, 0.0), Err(WorksheetErrorCode::Num));
        assert_eq!(power_kernel(0.0, -1.0), Err(WorksheetErrorCode::Div0));
        assert_eq!(power_kernel(-1.0, 0.5), Err(WorksheetErrorCode::Num));
    }

    // BUG-FUNC-027 CLASS-A4: live Excel 16.0 b20026 POWER(10,700)=#NUM!,
    // POWER(0.001,-700)=#DIV/0!, POWER(10,-700)=0.
    #[test]
    fn power_kernel_overflow_maps_to_excel_error_codes() {
        assert_eq!(power_kernel(10.0, 700.0), Err(WorksheetErrorCode::Num));
        assert_eq!(power_kernel(0.001, -700.0), Err(WorksheetErrorCode::Div0));
        assert_eq!(power_kernel(10.0, -700.0), Ok(0.0));
    }

    #[test]
    fn power_kernel_matches_excel_integer_publication_rows() {
        assert_eq!(power_kernel(1.05, 10.0), Ok(1.6288946267774416));
        assert_eq!(power_kernel(1.01, 48.0), Ok(1.6122260776824653));
        assert_eq!(
            power_kernel(1.0 + 0.08 / 12.0, 10.0),
            Ok(1.0687026403740616)
        );
    }

    #[test]
    fn power_kernel_matches_excel_negative_base_reciprocal_odd_root_rows() {
        // Reciprocal odd-integer roots of negative bases: bit-exact Excel publication
        // via -exp(power * ln(-base)). 1/3 and its 16/17-digit decimal literals
        // round to the same f64, so all three reach the exp/ln path identically.
        assert_eq!(power_kernel(-8.0, 1.0 / 3.0), Ok(-1.9999999999999998));
        assert_eq!(
            power_kernel(-8.0, 0.3333333333333333),
            Ok(-1.9999999999999998)
        );
        assert_eq!(
            power_kernel(-8.0, 0.33333333333333331),
            Ok(-1.9999999999999998)
        );
        assert_eq!(power_kernel(-27.0, 1.0 / 3.0), Ok(-2.9999999999999996));
        assert_eq!(power_kernel(-32.0, 1.0 / 5.0), Ok(-2.0));

        // Non-reciprocal-odd negative-base exponents fall through to #NUM!.
        assert_eq!(power_kernel(-8.0, 2.0 / 3.0), Err(WorksheetErrorCode::Num));
        assert_eq!(power_kernel(-8.0, 0.5), Err(WorksheetErrorCode::Num));
    }

    // W108 Phase D: Excel special-cases exponent 0.5 to the correctly-rounded
    // hardware sqrt, NOT exp(0.5·ln x). Pinned to live Excel 16.0 build 20131.
    #[test]
    fn power_kernel_exponent_half_is_sqrt() {
        // POWER(2, 0.5) = √2 exactly (0x…bcd); exp(0.5·ln2) is 1 ULP low (0x…bcc).
        assert_eq!(power_kernel(2.0, 0.5), Ok(2.0f64.sqrt()));
        assert_eq!(
            power_kernel(2.0, 0.5).unwrap().to_bits(),
            0x3ff6a09e667f3bcd
        );
        assert_eq!(power_kernel(16.0, 0.5), Ok(4.0));
        assert_eq!(power_kernel(0.25, 0.5), Ok(0.5));
        // y = -0.5: positive power is sqrt, then the x87 double-rounded reciprocal.
        assert_eq!(power_kernel(4.0, -0.5), Ok(0.5));
        // negative base with 0.5 stays #NUM! (1/0.5 = 2 is even → no real root).
        assert_eq!(power_kernel(-4.0, 0.5), Err(WorksheetErrorCode::Num));
    }

    // W108 Phase D: fractional-path witnesses pinned to live Excel (the previously
    // 1-ULP "DIVERGE" rows, now bit-exact via the y<0 reciprocal staging).
    #[test]
    fn power_kernel_fractional_pins_live_excel() {
        let rows: &[(u64, u64, u64)] = &[
            // base_bits, exp_bits, excel_result_bits
            (0x4093551c15e0b619, 0xbff2ead4957fb090, 0x3f2ceb593d898c6a), // 1237.28^-1.182
            (0x3f265c8a48107e4d, 0xc02a0aee3a890266, 0x4a1fc17fcee62254), // 1.7e-4^-13.02
            (0x404deffe2214465f, 0xc034acb0e18b10b2, 0x384eb120beb7818a), // 59.87^-20.67
            (0x40780a045b93fb26, 0xbff0a944c2b86230, 0x3f60a779471f9ac7), // 384.6^-1.041 (AGREE)
        ];
        for &(xb, yb, rb) in rows {
            assert_eq!(
                power_kernel(f64::from_bits(xb), f64::from_bits(yb)),
                Ok(f64::from_bits(rb)),
                "POWER(0x{xb:016x}, 0x{yb:016x})"
            );
        }
    }
}

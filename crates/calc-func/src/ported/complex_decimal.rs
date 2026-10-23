//! Arithmetic model for complex coefficient decimal rounding, derived from public-interface
//! Excel probes (W111, build 20430). This describes observable arithmetic, not Excel internals.
//! The powers are mathematically generated; no observed answers or fitted corrections occur.
//! This model remains partial: the independent W111 scaling discriminator has 40 unresolved
//! outcomes across ten coefficient magnitudes. See the retained complex evidence README.
use crate::value::WorksheetErrorCode;

#[path = "complex_decimal_powers.rs"]
mod powers;

/// Round a positive exact integer quotient to nearest, ties to even.
fn nearest_even(numerator: u128, denominator: u128) -> u128 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    // The denominator here is at most 2^64, so this exact comparison cannot overflow.
    quotient
        + u128::from(
            2 * remainder > denominator || (2 * remainder == denominator && quotient & 1 != 0),
        )
}

/// A decimal pair for |n| after scaling to 15 digits. For k = 14-floor(log10(|n|)),
/// the current candidate is RN64(|n| * RN64(10^k)) if k>=0, otherwise
/// RN64(|n| / RN64(10^-k)), followed by integer rounding with ties away.
/// RN64 is nearest-even rounding to 64 binary significand bits, with an unbounded exponent.
/// Multiplication by a rounded reciprocal is observably different from division.
pub(super) fn decimal_pair(n: f64) -> (u128, i32) {
    debug_assert!(n.is_finite() && n != 0.0);
    // Thirty decimal places safely identify the decade of a binary64 value, including
    // machine neighbors of powers of ten. Coefficient rounding uses integer arithmetic.
    let text = format!("{:.30e}", n.abs());
    let decimal_exponent: i32 = text
        .split_once('e')
        .expect("exponent form")
        .1
        .parse()
        .expect("exponent");
    let k = 14 - decimal_exponent;
    let bits = n.abs().to_bits();
    let encoded_exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (mantissa, exponent) = if encoded_exponent == 0 {
        (fraction as u128, -1074)
    } else {
        (
            (fraction | (1_u64 << 52)) as u128,
            encoded_exponent - 1023 - 52,
        )
    };
    let (power, power_exponent) = powers::POWERS[k.unsigned_abs() as usize];
    let (scaled, scaled_exponent) = if k >= 0 {
        let product = mantissa * power as u128;
        let shift = (128 - product.leading_zeros()).saturating_sub(64);
        (
            nearest_even(product, 1_u128 << shift),
            exponent + power_exponent as i32 + shift as i32,
        )
    } else {
        // Large values have a 53-bit significand. Relative to a normalized 64-bit power,
        // its quotient's binary exponent is -11 or -12. Either numerator fits in u128.
        let shift = if mantissa << 11 >= power as u128 {
            74
        } else {
            75
        };
        (
            nearest_even(mantissa << shift, power as u128),
            exponent - power_exponent as i32 - shift,
        )
    };
    // The scaled value is between 10^14 and 10^15 (allowing a rounding carry), hence its
    // 64-bit coefficient has a negative binary exponent. Round its exact dyadic value.
    debug_assert!((-64..0).contains(&scaled_exponent));
    let denominator_shift = -scaled_exponent as u32;
    let integer = (scaled + (1_u128 << (denominator_shift - 1))) >> denominator_shift;
    (integer, -k)
}
